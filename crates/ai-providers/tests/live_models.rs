//! Live provider checks — ignored by default (need real keys + network).
//! Run with: `cargo test -p ai-providers --test live_models -- --ignored`

use ai_providers::{provider_from_env, MistralProvider, ProviderKind};
use assistant_core::AiProvider;

fn env_key(name: &str) -> Option<String> {
    // Also look into the workspace `.env` file for developer convenience.
    if let Ok(value) = std::env::var(name) {
        if !value.trim().is_empty() {
            return Some(value);
        }
    }
    let text = std::fs::read_to_string(".env").ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == name && !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

#[tokio::test]
#[ignore]
async fn mistral_lists_live_models() {
    let Some(key) = env_key("MISTRAL_API_KEY") else {
        eprintln!("MISTRAL_API_KEY missing, skipping");
        return;
    };
    let provider = MistralProvider::new(key, String::new());
    let models = provider.fetch_models().await.expect("models list");
    assert!(!models.is_empty());
    assert!(
        models.iter().any(|m| m.contains("mistral-small")),
        "unexpected list: {models:?}"
    );
}

#[tokio::test]
#[ignore]
async fn openrouter_free_router_comes_first() {
    use ai_providers::OpenRouterProvider;

    // Public endpoint: no key required.
    let provider = OpenRouterProvider::new(String::new(), String::new());
    let models = provider.fetch_models().await.expect("free models list");
    assert_eq!(
        models.first().map(String::as_str),
        Some("openrouter/free"),
        "free router must lead, got {models:?}"
    );
    assert!(
        models.len() > 1,
        "expected several free models, got {models:?}"
    );
}

#[tokio::test]
#[ignore]
async fn all_providers_report_status_without_panic() {
    for kind in ProviderKind::all() {
        let p = provider_from_env(*kind);
        let _ = p.status();
        let _ = p.models();
    }
}
