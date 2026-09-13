//! SoryOS AI Assistant — native COSMIC desktop interface (libcosmic).
//!
//! Same assistant stack as the terminal REPL (`assistant-core`, providers,
//! tools, security, storage, voice), presented as a graphical window with
//! sidebar, streaming chat, tool confirmations and provider settings.

mod ctx;
mod envfile;
mod gui;
mod sory_theme;

use std::path::PathBuf;
use std::sync::Arc;

use ai_providers::{provider_from_env, ProviderKind, ProviderRegistry};
use assistant_core::{AppConfig, Assistant};
use cosmic::app::Settings;
use cosmic::iced::Size;
use soryos_security::SecurityPolicy;
use soryos_storage::{Database, SettingsStore, SqliteConversationStore, SqliteMemoryStore};
use tracing::{info, warn};

use crate::ctx::GuiCtx;
use crate::gui::{Flags, ProviderLite, SoryApp};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    load_dotenv();
    let config = load_config();

    // Tokio runtime for `spawn_blocking` storage and background turns.
    // (The COSMIC executor also runs Tokio tasks once the app starts.)
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let ctx = runtime.block_on(build_ctx(config))?;

    let settings = Settings::default().size(Size::new(1200.0, 800.0));
    cosmic::app::run::<SoryApp>(
        settings,
        Flags {
            provider_states: provider_lites(&ctx),
            ctx,
        },
    )?;
    Ok(())
}

async fn build_ctx(config: AppConfig) -> anyhow::Result<Arc<GuiCtx>> {
    let db_path = data_dir().join("soryos.db");
    let db = Database::open(&db_path)?;
    info!(path = %db_path.display(), "database opened");
    let conv_store = Arc::new(SqliteConversationStore::new(db.clone()));
    let memory_store = Arc::new(SqliteMemoryStore::new(db.clone()));
    let settings = SettingsStore::new(db.clone());

    let mut registry = ProviderRegistry::new();
    for kind in ProviderKind::all() {
        registry.register(provider_from_env(*kind));
    }
    let provider = pick_provider(&registry, &config, &settings).await;
    info!(provider = provider.name(), "active provider");

    let policy = Arc::new(SecurityPolicy {
        shell_requires_confirmation: config.security.shell_requires_confirmation,
        filesystem_write_requires_confirmation: config
            .security
            .filesystem_write_requires_confirmation,
        filesystem_sandbox: config.security.filesystem_sandbox.clone(),
        block_destructive_shell: true,
    });

    let mut assistant = Assistant::new(provider.clone(), policy, config.assistant.clone());
    for tool in soryos_tools::default_tools() {
        assistant.register_tool(tool);
    }
    assistant.set_memory(memory_store);
    assistant.set_store(conv_store.clone());

    // Restore persisted preferences.
    if let Ok(Some(model)) = settings.get("model").await {
        if !model.is_empty() {
            assistant.set_generation_params(
                model,
                assistant.config().temperature,
                assistant.config().max_tokens,
            );
        }
    }

    Ok(Arc::new(GuiCtx {
        assistant: std::sync::Mutex::new(assistant),
        queue: Default::default(),
        confirm: Default::default(),
        turn: Default::default(),
        store: conv_store,
        settings,
        tts: Arc::new(soryos_voice::MockTts),
    }))
}

fn provider_lites(ctx: &GuiCtx) -> Vec<ProviderLite> {
    let active = ctx.assistant.lock().unwrap().provider_name();
    ai_providers::config::all_provider_states()
        .into_iter()
        .map(|s| ProviderLite {
            active: s.name == active,
            name: s.name,
            status: s.status.label().to_string(),
        })
        .collect()
}

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
