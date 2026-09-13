//! Audio primitives: formats, chunks and level metering.

use serde::{Deserialize, Serialize};

/// PCM capture format used across STT/TTS boundaries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
        }
    }
}

/// A slice of mono 16-bit PCM samples with its format.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<i16>,
    pub format: AudioFormat,
}

impl AudioChunk {
    pub fn silence(duration_ms: u32, format: AudioFormat) -> Self {
        let n = (format.sample_rate * duration_ms / 1000) as usize;
        Self {
            samples: vec![0; n],
            format,
        }
    }

    pub fn duration_ms(&self) -> u32 {
        if self.format.sample_rate == 0 {
            return 0;
        }
        (self.samples.len() as u32 * 1000) / self.format.sample_rate
    }

    pub fn level(&self) -> f32 {
        rms_level(&self.samples)
    }
}

/// Root-mean-square level in 0.0..=1.0, for VU meters and VAD thresholds.
pub fn rms_level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|s| (*s as f64).powi(2)).sum();
    ((sum / samples.len() as f64).sqrt() / i16::MAX as f64) as f32
}

/// Encode mono 16-bit PCM as a minimal WAV file (for playback / debugging).
pub fn encode_wav(samples: &[i16], format: AudioFormat) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&format.channels.to_le_bytes());
    out.extend_from_slice(&format.sample_rate.to_le_bytes());
    let byte_rate = format.sample_rate * format.channels as u32 * 2;
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&(format.channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_has_zero_level() {
        let c = AudioChunk::silence(100, AudioFormat::default());
        assert_eq!(c.level(), 0.0);
        assert_eq!(c.duration_ms(), 100);
    }

    #[test]
    fn wav_header_is_valid() {
        let wav = encode_wav(&[0, 1000, -1000], AudioFormat::default());
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + 6);
    }
}
