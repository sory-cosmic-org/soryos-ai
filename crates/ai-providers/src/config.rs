//! Provider construction from environment variables.

use std::sync::Arc;

use assistant_core::{AiProvider, MockProvider};

use crate::{GeminiProvider, LocalProvider, MistralProvider, OpenRouterProvider};

/// Supported provider kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    OpenRouter,
    Mistral,
    Gemini,
    Local,
    Mock,
}

impl ProviderKind {
    pub fn all() -> &'static [ProviderKind] {
        &[
            Self::OpenRouter,
            Self::Mistral,
            Self::Gemini,
            Self::Local,
            Self::Mock,
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::OpenRouter => "openrouter",
            Self::Mistral => "mistral",
            Self::Gemini => "gemini",
            Self::Local => "local",
            Self::Mock => "mock",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "openrouter" => Some(Self::OpenRouter),
            "mistral" => Some(Self::Mistral),
            "gemini" | "google" => Some(Self::Gemini),
            "local" | "ollama" => Some(Self::Local),
            "mock" | "demo" => Some(Self::Mock),
            _ => None,
        }
    }
}

/// Connectivity of one provider, safe to display (no secrets).
#[derive(Debug, Clone)]
pub struct ProviderState {
    pub name: String,
    pub status: assistant_core::provider::ProviderStatus,
    pub models: Vec<String>,
}

/// Build a provider from its environment variables. Never panics on missing
/// keys: unconfigured providers report `NotConfigured` and fail at request
/// time with a clear error.
pub fn provider_from_env(kind: ProviderKind) -> Arc<dyn AiProvider> {
    match kind {
        ProviderKind::OpenRouter => Arc::new(OpenRouterProvider::from_env()),
        ProviderKind::Mistral => Arc::new(MistralProvider::from_env()),
        ProviderKind::Gemini => Arc::new(GeminiProvider::from_env()),
        ProviderKind::Local => Arc::new(LocalProvider::from_env()),
        ProviderKind::Mock => Arc::new(MockProvider::new()),
    }
}

/// Build every known provider and report their state.
pub fn all_provider_states() -> Vec<ProviderState> {
    ProviderKind::all()
        .iter()
        .map(|k| {
            let p = provider_from_env(*k);
            ProviderState {
                name: p.name().to_string(),
                status: p.status(),
                models: p.models(),
            }
        })
        .collect()
}

/// Pick the provider named by `SORYOS_PROVIDER`, falling back to the first
/// configured real provider, then to a *reachable* local server, then to
/// the mock (the app always starts).
pub fn default_provider_from_env() -> Arc<dyn AiProvider> {
    if let Ok(name) = std::env::var("SORYOS_PROVIDER") {
        if let Some(kind) = ProviderKind::parse(&name) {
            let p = provider_from_env(kind);
            if p.status() != assistant_core::provider::ProviderStatus::NotConfigured {
                return p;
            }
        }
    }
    for kind in [
        ProviderKind::OpenRouter,
        ProviderKind::Mistral,
        ProviderKind::Gemini,
    ] {
        let p = provider_from_env(kind);
        if p.status() == assistant_core::provider::ProviderStatus::Ready {
            return p;
        }
    }
    let local = provider_from_env(ProviderKind::Local);
    if local_reachable() {
        return local;
    }
    provider_from_env(ProviderKind::Mock)
}

/// True when the configured local server accepts TCP connections.
/// `LocalProvider` always reports Ready (no key needed), so without this
/// probe the app would select a dead local server over the mock.
pub fn local_reachable() -> bool {
    use std::net::ToSocketAddrs;
    use std::time::Duration;

    let base = std::env::var("LOCAL_AI_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
    let authority = base
        .split("://")
        .nth(1)
        .unwrap_or(&base)
        .split('/')
        .next()
        .unwrap_or_default();
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().unwrap_or(80)),
        None => (authority, 80),
    };
    if host.is_empty() {
        return false;
    }
    format!("{host}:{port}")
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .and_then(|addr| {
            std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(400)).ok()
        })
        .is_some()
}
