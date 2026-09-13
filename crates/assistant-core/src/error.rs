//! Core error type tying together all fallible operations.

use thiserror::Error;

/// Single error enum for everything orchestrated by [`crate::Assistant`].
#[derive(Debug, Error)]
pub enum AssistantError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("security: request denied: {0}")]
    Denied(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("voice error: {0}")]
    Voice(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("operation cancelled")]
    Cancelled,
}

impl From<crate::provider::ProviderError> for AssistantError {
    fn from(e: crate::provider::ProviderError) -> Self {
        Self::Provider(e.to_string())
    }
}

impl From<crate::tool::ToolError> for AssistantError {
    fn from(e: crate::tool::ToolError) -> Self {
        Self::Tool(e.to_string())
    }
}

impl From<crate::voice::VoiceError> for AssistantError {
    fn from(e: crate::voice::VoiceError) -> Self {
        Self::Voice(e.to_string())
    }
}
