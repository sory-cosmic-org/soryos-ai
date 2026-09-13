//! SoryOS AI Assistant — desktop entry point.
//!
//! Wiring: providers -> assistant-core -> tools/security/storage, driven by
//! the framework-agnostic `soryos-ui` state machine and presented here as an
//! interactive terminal chat. A COSMIC/libcosmic shell can replace this REPL
//! later without touching any other crate.

mod repl;

use std::path::PathBuf;
use std::sync::Arc;

use ai_providers::{provider_from_env, ProviderKind, ProviderRegistry};
use assistant_core::{AppConfig, Assistant, AssistantConfig};
use soryos_security::{SecurityPolicy, TerminalConfirmer};
use soryos_storage::{Database, SettingsStore, SqliteConversationStore, SqliteMemoryStore};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    load_dotenv();

    let config = load_config();
    info!(provider = %config.assistant.default_provider, "configuration loaded");

    // ---- Storage ----
    let db_path = data_dir().join("soryos.db");
    let db = Database::open(&db_path)?;
    info!(path = %db_path.display(), "database opened");
    let conv_store = Arc::new(SqliteConversationStore::new(db.clone()));
    let memory_store = Arc::new(SqliteMemoryStore::new(db.clone()));
    let settings = SettingsStore::new(db.clone());

    // ---- Providers ----
    let mut registry = ProviderRegistry::new();
    for kind in ProviderKind::all() {
        registry.register(provider_from_env(*kind));
    }
    let provider = pick_provider(&registry, &config, &settings).await;
    info!(provider = provider.name(), "active provider");

    // ---- Security ----
    let policy = Arc::new(SecurityPolicy {
        shell_requires_confirmation: config.security.shell_requires_confirmation,
        filesystem_write_requires_confirmation: config
            .security
            .filesystem_write_requires_confirmation,
        filesystem_sandbox: config.security.filesystem_sandbox.clone(),
        block_destructive_shell: true,
    });

    // ---- Assistant ----
    let mut assistant = Assistant::new(provider, policy, config.assistant.clone());
    for tool in soryos_tools::default_tools() {
        assistant.register_tool(tool);
    }
    assistant.set_memory(memory_store);
    assistant.set_store(conv_store.clone());

    // ---- Voice backends (isolated, mock by default) ----
    let stt: Arc<dyn assistant_core::SpeechToText> = Arc::new(soryos_voice::MockStt::default());
    let tts: Arc<dyn assistant_core::TextToSpeech> = Arc::new(soryos_voice::MockTts);
    info!(stt = stt.name(), tts = tts.name(), "voice backends");

    // ---- Run the chat loop ----
    let confirmer = TerminalConfirmer;
    repl::run(
        assistant, registry, conv_store, settings, stt, tts, confirmer,
    )
    .await
}

/// Select the active provider: explicit config/env choice, else first ready
/// backend, else the offline mock (the app always starts).
async fn pick_provider(
    registry: &ProviderRegistry,
    config: &AppConfig,
    settings: &SettingsStore,
) -> Arc<dyn assistant_core::AiProvider> {
    let wanted = settings
        .get("provider")
        .await
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| config.assistant.default_provider.clone());

    if let Some(kind) = ProviderKind::parse(&wanted) {
        if let Some(p) = registry.get(kind.name()) {
            if p.status() == assistant_core::provider::ProviderStatus::Ready {
                return p;
            }
            warn!(
                provider = kind.name(),
                "requested provider is not configured"
            );
        }
    }
    let env_default = ai_providers::config::default_provider_from_env();
    if env_default.name() != "mock" {
        return env_default;
    }
    registry
        .get("mock")
        .expect("mock provider is always registered")
}

/// Config file lookup: `./soryos.toml`, then roast; env always wins.
fn load_config() -> AppConfig {
    let candidates = [PathBuf::from("soryos.toml"), data_dir().join("soryos.toml")];
    for path in &candidates {
        if path.exists() {
            info!(path = %path.display(), "loading config file");
            return AppConfig::load(Some(path));
        }
    }
    AppConfig::load(None)
}

fn data_dir() -> PathBuf {
    std::env::var("SORYOS_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data"))
}

/// Minimal `.env` loader (no extra dependency): `KEY=VALUE` lines.
fn load_dotenv() {
    let Ok(text) = std::fs::read_to_string(".env") else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let (k, v) = (k.trim(), v.trim().trim_matches('"').trim_matches('\''));
            if !k.is_empty() && std::env::var(k).is_err() {
                std::env::set_var(k, v);
            }
        }
    }
}

/// Re-exported for tests.
pub fn _assistant_config_for_tests() -> AssistantConfig {
    AssistantConfig::default()
}
