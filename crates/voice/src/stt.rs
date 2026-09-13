//! Speech-to-text backends.
//!
//! Layout ready for `stt::local` (Whisper) and `stt::remote` (cloud API)
//! submodules — start here when wiring real transcription.

use assistant_core::{SpeechToText, VoiceError};
use async_trait::async_trait;

/// Deterministic test double: returns a canned sentence (or echoes the
/// audio byte length) so the voice pipeline is exercisable headlessly.
pub struct MockStt {
    pub transcript: String,
}

impl MockStt {
    pub fn new(transcript: impl Into<String>) -> Self {
        Self {
            transcript: transcript.into(),
        }
    }
}

impl Default for MockStt {
    fn default() -> Self {
        Self::new("Bonjour, ceci est une transcription simulée.")
    }
}

#[async_trait]
impl SpeechToText for MockStt {
    fn name(&self) -> &str {
        "mock-stt"
    }

    async fn transcribe(&self, audio: &[u8]) -> Result<String, VoiceError> {
        if audio.is_empty() {
            return Err(VoiceError::Transcription("empty audio buffer".to_string()));
        }
        Ok(self.transcript.clone())
    }
}

/// Backend used when no microphone/engine is available: always reports
/// [`VoiceError::Unavailable`] with an actionable message.
pub struct NoAudioStt;

#[async_trait]
impl SpeechToText for NoAudioStt {
    fn name(&self) -> &str {
        "unavailable-stt"
    }

    async fn transcribe(&self, _audio: &[u8]) -> Result<String, VoiceError> {
        Err(VoiceError::Unavailable(
            "no speech-to-text backend configured (see voice.stt_provider)".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_transcribes() {
        let stt = MockStt::new("hello");
        assert_eq!(stt.transcribe(&[1, 2, 3]).await.unwrap(), "hello");
        assert!(stt.transcribe(&[]).await.is_err());
    }
}
