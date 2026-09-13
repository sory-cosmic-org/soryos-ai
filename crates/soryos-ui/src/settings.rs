//! Settings view helpers: provider rows and parameter summaries.

use crate::state::{ProviderRow, SettingsView};

/// Build provider rows from live states (status strings only, no secrets).
pub fn provider_rows(states: &[ai_providers_state::ProviderState]) -> Vec<ProviderRow> {
    states
        .iter()
        .map(|s| ProviderRow {
            name: s.name.clone(),
            status: s.status.label().to_string(),
            models: s.models.clone(),
        })
        .collect()
}

/// Minimal state struct so this crate does not depend on `ai-providers`.
pub mod ai_providers_state {
    #[derive(Debug, Clone)]
    pub struct ProviderState {
        pub name: String,
        pub status: assistant_core::provider::ProviderStatus,
        pub models: Vec<String>,
    }
}

/// One-line summary of generation parameters.
pub fn params_summary(view: &SettingsView) -> String {
    format!(
        "provider={} model={} temperature={:.2} max_tokens={}",
        view.selected_provider, view.model, view.temperature, view.max_tokens
    )
}
