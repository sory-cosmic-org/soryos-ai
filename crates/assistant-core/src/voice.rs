//! Voice abstractions shared by the orchestrator and the `voice` crate.

use async_trait::async_trait;
use thiserror::Error;

/// Lifecycle of a voice conversation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceState {
    #[default]
    Idle,
    Listening,
    Processing,
    Speaking,
    Interrupted,
    Error,
}

impl VoiceState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Listening => "Listening",
            Self::Processing => "Processing",
            Self::Speaking => "Speaking",
            Self::Interrupted => "Interrupted",
            Self::Error => "Error",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Idle => "🎤",
            Self::Listening => "🔴",
            Self::Processing => "⏳",
            Self::Speaking => "🔊",
            Self::Interrupted => "✋",
            Self::Error => "⚠️",
        }
    }
}

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("audio backend unavailable: {0}")]
    Unavailable(String),
    #[error("transcription failed: {0}")]
    Transcription(String),
    #[error("synthesis failed: {0}")]
    Synthesis(String),
    #[error("audio io error: {0}")]
    Io(String),
}

/// Converts speech audio (PCM bytes) into text.
#[async_trait]
pub trait SpeechToText: Send + Sync {
    fn name(&self) -> &str;
    async fn transcribe(&self, audio: &[u8]) -> Result<String, VoiceError>;
}

/// Converts text into speech audio (WAV bytes).
#[async_trait]
pub trait TextToSpeech: Send + Sync {
    fn name(&self) -> &str;
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, VoiceError>;
}
