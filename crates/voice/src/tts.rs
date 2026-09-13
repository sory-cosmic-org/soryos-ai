//! Text-to-speech backends.
//!
//! Layout ready for `tts::local` (Piper/Coqui) and `tts::remote` submodules.

use assistant_core::{TextToSpeech, VoiceError};
use async_trait::async_trait;

use crate::audio::{encode_wav, AudioFormat};

/// Test double: encodes the text length as silent WAV audio so playback
/// plumbing can be verified without a real engine.
pub struct MockTts;

#[async_trait]
impl TextToSpeech for MockTts {
    fn name(&self) -> &str {
        "mock-tts"
    }

    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, VoiceError> {
        if text.trim().is_empty() {
            return Err(VoiceError::Synthesis("empty text".to_string()));
        }
        // ~20 ms of silence per character, capped at 5 s.
        let ms = ((text.chars().count() as u32) * 20).clamp(100, 5000);
        let format = AudioFormat::default();
        let chunk = crate::audio::AudioChunk::silence(ms, format);
        Ok(encode_wav(&chunk.samples, format))
    }
}

/// Silent backend: produces valid (silent) WAV for any input. Useful as a
/// "voice enabled, no engine" placeholder that never fails the pipeline.
pub struct SilentTts;

#[async_trait]
impl TextToSpeech for SilentTts {
    fn name(&self) -> &str {
        "silent-tts"
    }

    async fn synthesize(&self, _text: &str) -> Result<Vec<u8>, VoiceError> {
        let format = AudioFormat::default();
        let chunk = crate::audio::AudioChunk::silence(200, format);
        Ok(encode_wav(&chunk.samples, format))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_produces_wav() {
        let tts = MockTts;
        let wav = tts.synthesize("Bonjour").await.unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert!(tts.synthesize("  ").await.is_err());
    }
}
