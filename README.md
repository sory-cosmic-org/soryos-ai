# SoryOS AI Assistant

Assistant IA desktop natif de SoryOS, développé en **Rust** : dialogue texte, streaming,
outils système sécurisés, mémoire locale, multi-providers (cloud + local) et voix —
le tout derrière des abstractions indépendantes de tout fournisseur.

> État : fondation fonctionnelle. Lancement sans clé API via `MockProvider` (mode démo),
> providers cloud réels branchés dès que les clés sont configurées.

## Sommaire

- [Installation](#installation)
- [Lancement](#lancement)
- [Architecture](#architecture)
- [Configuration](#configuration)
- [Providers IA](#providers-ia)
- [Ajouter un provider](#ajouter-un-provider)
- [Voix](#voix)
- [Tools](#tools)
- [Sécurité](#sécurité)
- [Stockage](#stockage)
- [Tests](#tests)
- [Roadmap](#roadmap)

## Installation

Prérequis : Rust stable ≥ 1.80, `cc`, SQLite (compilé en statique via `bundled`, aucun
paquet système requis).

```bash
git clone <repo> soryos-ai-assistant
cd soryos-ai-assistant
cargo build -p soryos-desktop
```

Environnement réseau contraint : si `cargo fetch` échoue avec des erreurs de
lenteur, pré-téléchargez les crates avec `curl` (tolérant aux démarrages lents)
puis compilez hors-ligne (voir `cargo check --offline --workspace`).

Note build GUI : `libcosmic` est une dépendance git (pas sur crates.io) et la
compilation Wayland/X11 exige les headers `xkbcommon`. Sans droits root :

```bash
apt-get download libxkbcommon-dev libxkbcommon-x11-dev
for f in libxkbcommon*.deb; do dpkg-deb -x "$f" sysroot/; done
# + liens .so et .pc : voir `.cargo/config.toml` (PKG_CONFIG_PATH local)
```

Astuce de test : `SORYOS_GUI_SMOKE=1` fait envoyer « Bonjour » automatiquement
au démarrage de la GUI (auto-test du pipeline complet).

```bash
# 1. Résoudre les dépendances (index léger)
cargo generate-lockfile
# 2. Télécharger chaque .crate manquant (parallèle + reprises)
CACHE=~/.cargo/registry/cache/index.crates.io-*/
python3 -c "
import re
lock = open('Cargo.lock').read()
pkgs = re.findall(r'\[\[package\]\]\nname = \"([^\"]+)\"\nversion = \"([^\"]+)\"(?:\nsource = \"([^\"]*)\")?', lock)
for n,v,s in pkgs:
    if s and 'crates.io' in s:
        print(f'https://static.crates.io/crates/{n}/{v}/download')
" | sort -u > /tmp/urls.txt
# adapter : télécharger vers $CACHE/<nom>-<version>.crate
# 3. Compiler sans réseau
cargo check --offline --workspace
```

## Lancement

Interface graphique native (libcosmic, COSMIC/SoryOS) :

```bash
cargo run -p soryos-desktop-gui
```

Elle s'ouvre sur le bureau : sidebar (＋ nouvelle conversation, recherche,
historique groupé par date avec suppression, providers, réglages),
chat avec streaming, confirmations d'outils intégrées, bouton voix (TTS).
Également disponible dans le menu des applications (« SoryOS AI Assistant »,
via `soryos-ai-assistant.desktop`).

Sidebar providers : cliquer un provider l'active et déplie son panneau —
champ clé API masqué (coller + Enregistrer, stocké dans `.env`, jamais logué),
modèle, URL locale, bouton « Voir les modèles » (liste live depuis l'API :
Mistral/OpenRouter `/v1/models`, Gemini, Ollama), « Recharger les clés (.env) »
pour prendre en compte une clé collée à la main sans redémarrer.

Version terminal (même moteur) :

```bash
cargo run -p soryos-desktop
```

Sans aucune clé API, l'assistant démarre en **mode démo** (`MockProvider`) : la
conversation, les outils lecture seule et la persistance SQLite fonctionnent
immédiatement. Commandes du REPL : `/help`, `/new`, `/list`, `/open <n>`,
`/providers`, `/provider <nom>`, `/model <nom>`, `/temp <0-2>`, `/tools`,
`/voice [texte]`, `/remember <texte>`, `/settings`, `/show`, `/quit`.

## Architecture

Workspace Cargo : 7 crates + 1 application.

```text
apps/desktop-gui   → fenêtre libcosmic (sidebar, chat streaming, confirmations, voix)
apps/desktop      → REPL terminal, câblage, sélection provider, confirmation
crates/soryos-ui  → state machine UI agnostique (Événements → Effets),
                    prête pour libcosmic/COSMIC sans toucher au core
crates/assistant-core → Assistant, ChatRequest/Response, AiProvider,
                    Tool/ToolGate/Confirmer, Memory/Conversation stores (traits),
                    config, MockProvider
crates/ai-providers   → OpenRouter, Mistral, Gemini, Local (Ollama/llama.cpp/vLLM),
                    ProviderRegistry, sélection par variables d'environnement
crates/voice          → SpeechToText/TextToSpeech, audio PCM/WAV, backends mock
crates/soryos-tools   → registry + read_file/write_file/list_dir/run_shell/system_info
crates/soryos-security→ SecurityPolicy (ToolGate), permissions, TerminalConfirmer
crates/soryos-storage → SQLite (conversations, memories, settings)
```

Règle de dépendances respectée : `assistant-core` ne connaît **aucun**
fournisseur, aucune base, aucun toolkit. Tout le reste dépend de lui.

Flux d'un tour :

```text
Utilisateur → Assistant → ChatContext → AI Provider → Tool calls éventuels
→ Security Policy → Permission Check → Confirmation utilisateur si requise
→ Tool → Résultat → AI Provider → Réponse (streaming) → UI + persistance
```

## Configuration

`AppConfig` = fichier `soryos.toml` (voir `soryos.toml.example`) + surcharges
d'environnement (`SORYOS_PROVIDER`, `SORYOS_MODEL`). Les **secrets ne sont
jamais** dans le fichier : uniquement en variables d'environnement
(voir `.env.example` ; un mini-chargeur `.env` est intégré au binaire).

| Variable             | Rôle                              |
| -------------------- | --------------------------------- |
| `OPENROUTER_API_KEY` | Provider OpenRouter               |
| `MISTRAL_API_KEY`    | Provider Mistral                  |
| `GEMINI_API_KEY`     | Provider Google Gemini            |
| `LOCAL_AI_BASE_URL`  | Serveur local (défaut Ollama)     |
| `LOCAL_AI_MODEL`     | Modèle local                      |
| `SORYOS_PROVIDER`    | Provider par défaut (ou `mock`)   |
| `SORYOS_MODEL`       | Modèle par défaut                 |
| `SORYOS_DATA_DIR`    | Dossier données (défaut `./data`) |

## Providers IA

Tous implémentent `AiProvider::complete` + `AiProvider::stream` :

- **OpenRouter** — `POST {base}/chat/completions`, SSE, headers passerelle
  (`HTTP-Referer`, `X-Title`), timeout 60 s.
- **Mistral** — même client OpenAI-compatible sur `api.mistral.ai/v1`.
- **Gemini** — `generateContent` + `streamGenerateContent` (function calling
  mappé vers les tool calls du core).
- **Local** — base URL configurable, compatible Ollama / llama.cpp / vLLM.
- **Mock** — sans réseau, réponses configurables, simule un tool call
  (`system_info`) pour tester la boucle d'outils.

`ProviderRegistry` + `provider_from_env` : le reste de l'app demande
« provider X / modèle Y » sans connaître le HTTP interne. Statuts affichés
(`Connected` / `Not configured` / `Error`) sans jamais exposer les clés.

## Ajouter un provider

1. Créer `crates/ai-providers/src/monprovider.rs` avec une struct implémentant
   `AiProvider` (réutiliser `OpenAiCompatProvider` si l'API est compatible OpenAI).
2. L'exporter dans `lib.rs` et l'ajouter à `ProviderKind` + `provider_from_env`.
3. Documenter les variables d'environnement dans `.env.example` et ici.

## Voix

Traits `SpeechToText` / `TextToSpeech` dans le core, backends dans `voice`
(`MockStt`, `MockTts`, `SilentTts`, `NoAudioStt`) + utilitaires PCM/WAV
(`AudioChunk`, `rms_level`, `encode_wav`). États du pipeline :
`Idle → Listening → Processing → Speaking`, avec `Interrupted` / `Error`.
Le REPL expose `/voice` (simulation STT+TTS) ; brancher Whisper/Piper/remote
= implémenter les deux traits, sans changer le core.

## Tools

`ToolRegistry` + `Tool` (`name`, `description`, `parameters_schema`, `execute`) :
`read_file` (256 Kio max), `write_file`, `list_dir` (500 entrées max),
`run_shell` (timeout 30 s, sortie bornée), `system_info` (lecture seule).
Nouveau tool = implémenter `Tool` + `registry.register(...)`.

## Sécurité

Couche **critique et non contournable** : chaque appel modèle passe par
`SecurityPolicy` (`ToolGate`) avant exécution.

| Action               | Niveau par défaut        |
| -------------------- | ------------------------ |
| Lecture fichier / listage / system_info | `ReadOnly` → autorisé |
| Écriture fichier     | `RequiresConfirmation`   |
| Suppression fichier  | `RequiresConfirmation`   |
| Shell normal         | `RequiresConfirmation`   |
| Shell destructif (`rm -rf /`, `mkfs`, `dd`, `shutdown`, `curl…|sh`, …) | `Restricted` → **bloqué** |
| Hors sandbox (si configurée) | `Denied`            |

Confirmation via `TerminalConfirmer` (`o/N`) ; tests via `AllowAll`/`DenyAll`.
`tracing` ne loggue jamais de clés (statuts seuls).

## Stockage

SQLite local (`data/soryos.db`) : tables `conversations`, `memories`,
`settings`. Accès via `spawn_blocking` (jamais de blocage du runtime async).
Les clés API ne sont **jamais** persistées (env uniquement).

## Tests

```bash
cargo test --workspace          # unitaires + intégration (mock, sans clé API)
cargo fmt --check
cargo clippy --workspace -- -D warnings
```

Le test d'intégration `apps/desktop/tests/demo_flow.rs` couvre :
bonjour → réponse, boucle d'outil sécurisée, outil refusé par la policy.

## Roadmap

- [ ] UI COSMIC/libcosmic branchée sur la state machine `soryos-ui`
- [ ] Vrai STT/TTS (Whisper local, Piper, backends remote) + wake word + VAD
- [ ] Mémoire sémantique (embeddings locaux)
- [ ] Vision / compréhension d'écran, web search, calendrier, plugins
- [ ] Bouton stop d'annulation temps réel du streaming
