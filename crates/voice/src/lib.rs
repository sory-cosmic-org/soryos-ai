//! Voice I/O for SoryOS AI Assistant: audio helpers, STT/TTS backends.
//!
//! ```text
//! Microphone -> Audio capture -> STT -> Assistant Core -> AI Provider
//!   -> TTS -> Speakers
//! ```
//!
//! Real backends (local Whisper, Piper, remote APIs) plug in behind the
//! [`assistant_core::SpeechToText`] / [`assistant_core::TextToSpeech`]
//! traits. This crate ships isolated, testable backends ([`MockStt`],
//! [`MockTts`], [`SilentTts`]) so the full voice pipeline runs today.

pub mod audio;
pub mod error;
pub mod stt;
pub mod tts;

pub use audio::{rms_level, AudioChunk, AudioFormat};
pub use error::VoiceBackendError;
pub use stt::{MockStt, NoAudioStt};
pub use tts::{MockTts, SilentTts};
