//! Backend errors (transport-level; the core [`assistant_core::VoiceError`]
//! is what the orchestrator sees).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VoiceBackendError {
    #[error("backend unavailable: {0}")]
    Unavailable(String),
    #[error("io error: {0}")]
    Io(String),
}
