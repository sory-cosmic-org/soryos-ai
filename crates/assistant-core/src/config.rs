//! Global configuration: file (`soryos.toml`) + environment overrides.
//!
//! Secrets are **never** read from the config file — only from environment
//! variables or the OS keyring. The file only holds non-secret preferences.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantConfig {
    #[serde(default = "default_provider")]
    pub default_provider: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: usize,
}

fn default_provider() -> String {
    "mock".to_string()
}
fn default_temperature() -> f32 {
    0.7
}
fn default_max_tokens() -> u32 {
    1024
}
fn default_max_history() -> usize {
    40
}
fn default_max_tool_iterations() -> usize {
    5
}
fn default_system_prompt() -> String {
    "Tu es SoryOS AI, l'assistant personnel natif de SoryOS. \
     Tu réponds de façon claire, concise et utile. \
     Tu peux utiliser les outils mis à ta disposition quand cela aide l'utilisateur."
        .to_string()
}

impl Default for AssistantConfig {
    fn default() -> Self {
        Self {
            default_provider: default_provider(),
            default_model: String::new(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            system_prompt: default_system_prompt(),
            max_history: default_max_history(),
            max_tool_iterations: default_max_tool_iterations(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_stt")]
    pub stt_provider: String,
    #[serde(default = "default_tts")]
    pub tts_provider: String,
}

fn default_true() -> bool {
    true
}
fn default_stt() -> String {
    "mock".to_string()
}
fn default_tts() -> String {
    "mock".to_string()
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            stt_provider: default_stt(),
            tts_provider: default_tts(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub shell_requires_confirmation: bool,
    #[serde(default = "default_true")]
    pub filesystem_write_requires_confirmation: bool,
    #[serde(default)]
    pub filesystem_sandbox: String,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            shell_requires_confirmation: true,
            filesystem_write_requires_confirmation: true,
            filesystem_sandbox: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub assistant: AssistantConfig,
    #[serde(default)]
    pub voice: VoiceConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

impl AppConfig {
    /// Load `soryos.toml` when present, otherwise defaults. Environment
    /// variables (`SORYOS_PROVIDER`, `SORYOS_MODEL`, ...) always win.
    pub fn load(path: Option<&Path>) -> Self {
        let mut cfg = Self::default();
        if let Some(path) = path {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(parsed) = toml::from_str::<AppConfig>(&text) {
                    cfg = parsed;
                }
            }
        }
        if let Ok(v) = std::env::var("SORYOS_PROVIDER") {
            if !v.trim().is_empty() {
                cfg.assistant.default_provider = v;
            }
        }
        if let Ok(v) = std::env::var("SORYOS_MODEL") {
            if !v.trim().is_empty() {
                cfg.assistant.default_model = v;
            }
        }
        cfg
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_select_mock_provider() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.assistant.default_provider, "mock");
    }

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = AppConfig::load(Some(Path::new("/nonexistent/soryos.toml")));
        assert_eq!(cfg.assistant.default_provider, "mock");
    }
}
