//! Interactive terminal chat driving the `soryos-ui` state machine.

use std::sync::Arc;

use ai_providers::ProviderRegistry;
use assistant_core::{Assistant, AssistantEvent, Conversation, ConversationStore};
use soryos_storage::{SettingsStore, SqliteConversationStore};
use soryos_ui::{chat::render_snapshot, sidebar::summarize, App, Effect, UiEvent};
use tokio::io::{AsyncBufReadExt, BufReader};

pub async fn run(
    mut assistant: Assistant,
    registry: ProviderRegistry,
    conv_store: Arc<SqliteConversationStore>,
    settings: SettingsStore,
    stt: Arc<dyn assistant_core::SpeechToText>,
    tts: Arc<dyn assistant_core::TextToSpeech>,
    confirmer: impl assistant_core::Confirmer + 'static,
) -> anyhow::Result<()> {
    let confirmer = Arc::new(confirmer);
    let mut app = App::new();
    refresh_sidebar(&mut app, &conv_store).await;

    // Restore generation params from settings when present.
    if let Ok(Some(model)) = settings.get("model").await {
        assistant.set_generation_params(
            model,
            assistant.config().temperature,
            assistant.config().max_tokens,
        );
    }

    let mut current = Conversation::new("Nouvelle conversation");
    let mut created = false;

    print_banner(&assistant, &registry);
    println!("Tapez /help pour la liste des commandes.\n");

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    loop {
        print!("› ");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        let Some(line) = lines.next_line().await? else {
            break;
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('/') {
            match handle_command(
                &line,
                &mut app,
                &mut assistant,
                &registry,
                &conv_store,
                &settings,
                &mut current,
                &mut created,
                &stt,
                &tts,
            )
            .await?
            {
                CommandOutcome::Quit => break,
                CommandOutcome::Continue => continue,
                CommandOutcome::NotACommand => {}
            }
        }

        // ---- Normal message: push through the UI state machine ----
        app.update(UiEvent::InputChanged(line.clone()));
        match app.update(UiEvent::SendMessage) {
            Effect::SendToAssistant(text) => {
                if !created {
                    if conv_store.create(current.clone()).await.is_ok() {
                        created = true;
                    }
                    refresh_sidebar(&mut app, &conv_store).await;
                }
                run_turn(&mut app, &assistant, &mut current, &text, &confirmer).await;
                refresh_sidebar(&mut app, &conv_store).await;
            }
            _ => println!("(génération déjà en cours… tapez /new pour recommencer)"),
        }
    }
    println!("\n👋 À bientôt sur SoryOS !");
    Ok(())
}

async fn run_turn(
    app: &mut App,
    assistant: &Assistant,
    conversation: &mut Conversation,
    text: &str,
    confirmer: &Arc<impl assistant_core::Confirmer + 'static>,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AssistantEvent>(64);
    let mut working = conversation.clone();

    // Event pump and responder run concurrently in this task via `join!`,
    // so streaming chunks print as they arrive and the UI state mirrors them.
    let pump = async {
        while let Some(ev) = rx.recv().await {
            match ev {
                AssistantEvent::TextChunk(d) => {
                    print!("{d}");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                    app.update(UiEvent::StreamChunk(d));
                }
                AssistantEvent::ToolRequested { name, .. } => {
                    println!("\n⚙️  Outil demandé : {name}");
                    app.update(UiEvent::ToolCallStarted { name });
                }
                AssistantEvent::ConfirmationRequired { tool, reason } => {
                    println!("🔐 Confirmation requise pour « {tool} » : {reason}");
                }
                AssistantEvent::ConfirmationDecided { tool, approved } => {
                    println!(
                        "{}",
                        if approved {
                            format!("✅ « {tool} » approuvé.")
                        } else {
                            format!("⛔ « {tool} » refusé.")
                        }
                    );
                }
                AssistantEvent::ToolCompleted { name, is_error } => {
                    println!(
                        "{}",
                        if is_error {
                            format!("⚠️  Outil « {name} » terminé avec erreur.")
                        } else {
                            format!("✅ Outil « {name} » terminé.")
                        }
                    );
                }
                AssistantEvent::Done(_) => break,
            }
        }
    };

    let confirmer = confirmer.clone();
    let responder = assistant.respond(&mut working, text, &*confirmer, &tx);
    let (_, result) = tokio::join!(pump, responder);
    drop(tx);

    match result {
        Ok(response) => {
            *conversation = working;
            println!();
            app.update(UiEvent::StreamDone {
                content: response.content.clone(),
            });
        }
        Err(e) => {
            println!("\n⚠️  Erreur : {e}");
            app.update(UiEvent::StreamError(e.to_string()));
        }
    }
}

enum CommandOutcome {
    Quit,
    Continue,
    NotACommand,
}

#[allow(clippy::too_many_arguments)]
async fn handle_command(
    line: &str,
    app: &mut App,
    assistant: &mut Assistant,
    registry: &ProviderRegistry,
    conv_store: &Arc<SqliteConversationStore>,
    settings: &SettingsStore,
    current: &mut Conversation,
    created: &mut bool,
    stt: &Arc<dyn assistant_core::SpeechToText>,
    tts: &Arc<dyn assistant_core::TextToSpeech>,
) -> anyhow::Result<CommandOutcome> {
    let mut parts = line.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next().unwrap_or("").trim();
    match cmd {
        "/quit" | "/q" | "/exit" => Ok(CommandOutcome::Quit),
        "/help" | "/h" => {
            print_help();
            Ok(CommandOutcome::Continue)
        }
        "/new" => {
            *current = Conversation::new("Nouvelle conversation");
            *created = false;
            app.update(UiEvent::NewConversation);
            println!("✨ Nouvelle conversation.");
            Ok(CommandOutcome::Continue)
        }
        "/list" => {
            let list = conv_store.list(20).await.unwrap_or_default();
            if list.is_empty() {
                println!("(aucune conversation sauvegardée)");
            }
            for (i, c) in list.iter().enumerate() {
                println!(
                    "  {i}. {} — {}",
                    c.title,
                    c.updated_at.format("%d/%m %H:%M")
                );
            }
            Ok(CommandOutcome::Continue)
        }
        "/open" => {
            let idx: usize = arg.parse().unwrap_or(usize::MAX);
            let list = conv_store.list(20).await.unwrap_or_default();
            if let Some(conv) = list.get(idx) {
                *current = conv_store.get(conv.id).await?;
                *created = true;
                app.update(UiEvent::ConversationOpened {
                    id: current.id,
                    messages: current.messages.clone(),
                });
                println!("{}", render_snapshot(app.state()));
            } else {
                println!("Usage : /open <numéro de /list>");
            }
            Ok(CommandOutcome::Continue)
        }
        "/providers" => {
            for name in registry.names() {
                let p = registry.get(&name).unwrap();
                let mark = if p.name() == assistant.provider_name() {
                    "◉"
                } else {
                    "○"
                };
                println!(
                    "  {mark} {name:12} [{}] models: {}",
                    p.status().label(),
                    p.models().join(", ")
                );
            }
            Ok(CommandOutcome::Continue)
        }
        "/provider" => {
            if arg.is_empty() {
                println!("Provider actuel : {}", assistant.provider_name());
                println!("Usage : /provider <openrouter|mistral|gemini|local|mock>");
            } else if let Some(p) = registry.get(arg) {
                if p.status() == assistant_core::provider::ProviderStatus::NotConfigured {
                    println!("⚠️  « {arg} » n'est pas configuré (clé API manquante).");
                } else {
                    assistant.set_provider(p);
                    settings.set("provider", arg).await.ok();
                    println!("✅ Provider actif : {arg}");
                }
            } else {
                println!("Provider inconnu : {arg}");
            }
            Ok(CommandOutcome::Continue)
        }
        "/model" => {
            if arg.is_empty() {
                println!("Modèle actuel : {}", assistant.config().default_model);
            } else {
                let cfg = assistant.config().clone();
                assistant.set_generation_params(arg.to_string(), cfg.temperature, cfg.max_tokens);
                settings.set("model", arg).await.ok();
                println!("✅ Modèle : {arg}");
            }
            Ok(CommandOutcome::Continue)
        }
        "/temp" => {
            if let Ok(t) = arg.parse::<f32>() {
                let cfg = assistant.config().clone();
                assistant.set_generation_params(
                    cfg.default_model,
                    t.clamp(0.0, 2.0),
                    cfg.max_tokens,
                );
                println!("✅ Température : {t}");
            } else {
                println!("Température actuelle : {}", assistant.config().temperature);
            }
            Ok(CommandOutcome::Continue)
        }
        "/tools" => {
            println!("Outils disponibles :");
            for t in soryos_tools::default_tools() {
                println!("  • {} — {}", t.name(), t.description());
            }
            Ok(CommandOutcome::Continue)
        }
        "/voice" => {
            // Simulated voice turn: the typed text stands in for STT output.
            let transcript = if arg.is_empty() {
                print!("🎤 Dictez (tapez une phrase) : ");
                std::io::Write::flush(&mut std::io::stdout()).ok();
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf).ok();
                buf.trim().to_string()
            } else {
                arg.to_string()
            };
            if transcript.is_empty() {
                println!("(rien à transcrire)");
                return Ok(CommandOutcome::Continue);
            }
            app.update(UiEvent::VoiceStateChanged(
                assistant_core::VoiceState::Processing,
            ));
            let recognized = stt
                .transcribe(transcript.as_bytes())
                .await
                .unwrap_or(transcript);
            println!("🔴→⏳ Transcription : « {recognized} »");
            app.update(UiEvent::InputChanged(recognized.clone()));
            if let Effect::SendToAssistant(text) = app.update(UiEvent::SendMessage) {
                // Reuse the normal pipeline via a scratch Confirmer below.
                let _ = text;
                println!("(réponse vocale : utilisez la phrase comme message normal)");
                app.update(UiEvent::StreamError(
                    "mode voix simulé : renvoyez la phrase sans /voice".to_string(),
                ));
            }
            let wav = tts.synthesize(&recognized).await.unwrap_or_default();
            println!("🔊 Synthèse : {} octets audio (WAV).", wav.len());
            app.update(UiEvent::VoiceStateChanged(assistant_core::VoiceState::Idle));
            Ok(CommandOutcome::Continue)
        }
        "/remember" => {
            if arg.is_empty() {
                println!("Usage : /remember <chose à mémoriser>");
            } else {
                println!("🧠 Mémorisé : {arg} (stockage mémoire persistant : bientôt branché sur cette commande)");
            }
            Ok(CommandOutcome::Continue)
        }
        "/settings" => {
            println!("Paramètres :");
            println!("  provider    : {}", assistant.provider_name());
            println!("  model       : {}", assistant.config().default_model);
            println!("  temperature : {}", assistant.config().temperature);
            println!("  max_tokens  : {}", assistant.config().max_tokens);
            Ok(CommandOutcome::Continue)
        }
        "/show" => {
            println!("{}", render_snapshot(app.state()));
            Ok(CommandOutcome::Continue)
        }
        _ => Ok(CommandOutcome::NotACommand),
    }
}

async fn refresh_sidebar(app: &mut App, conv_store: &Arc<SqliteConversationStore>) {
    let list = conv_store.list(50).await.unwrap_or_default();
    app.update(UiEvent::ConversationsLoaded(summarize(&list)));
}

fn print_banner(assistant: &Assistant, registry: &ProviderRegistry) {
    println!("╭─────────────────────────────────────────────╮");
    println!("│  SoryOS AI Assistant — mode terminal        │");
    println!("│  Provider : {:<29} │", assistant.provider_name());
    let others: Vec<String> = registry
        .names()
        .into_iter()
        .filter(|n| n != &assistant.provider_name())
        .collect();
    println!("│  Autres : {:<31} │", others.join(", "));
    println!("╰─────────────────────────────────────────────╯");
}

fn print_help() {
    println!("Commandes :");
    println!("  /new              nouvelle conversation");
    println!("  /list             lister les conversations");
    println!("  /open <n>         ouvrir une conversation");
    println!("  /providers        état des providers IA");
    println!("  /provider <nom>   changer de provider");
    println!("  /model <nom>      changer de modèle");
    println!("  /temp <0-2>       régler la température");
    println!("  /tools            lister les outils");
    println!("  /voice [texte]    simuler un tour vocal (STT+TTS)");
    println!("  /remember <texte> mémoriser une préférence");
    println!("  /settings         afficher les paramètres");
    println!("  /show             réafficher la conversation");
    println!("  /quit             quitter");
}
