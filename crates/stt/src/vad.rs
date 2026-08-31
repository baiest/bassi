use voice_activity_detector::VoiceActivityDetector;

use crate::WHISPER_SAMPLE_RATE;
use crate::session::CHUNK_SAMPLES;

/// Scores one chunk of audio for how likely it is to contain speech.
///
/// A trait so [`crate::Session`] can be driven by a scripted fake in
/// tests, and so an energy-based detector can be swapped in if the ONNX
/// Runtime dependency ever becomes a problem on a target platform.
pub trait VoiceDetector {
    /// Probability in `0.0..=1.0` that `chunk` contains speech. `chunk` is
    /// always [`CHUNK_SAMPLES`] long at [`WHISPER_SAMPLE_RATE`].
    fn probability(&mut self, chunk: &[f32]) -> f32;
}

#[derive(Debug, thiserror::Error)]
pub enum VadError {
    #[error("failed to build the voice activity detector: {0}")]
    Build(String),
}

/// Silero VAD v5, running under ONNX Runtime.
///
/// Cheap enough to run on every chunk forever — it is the first stage of
/// the cascade, and its only job is deciding when the more expensive
/// wake-word detector is worth waking up.
pub struct SileroVad {
    detector: VoiceActivityDetector,
}

impl SileroVad {
    pub fn new() -> Result<Self, VadError> {
        let detector = VoiceActivityDetector::builder()
            .sample_rate(WHISPER_SAMPLE_RATE as i64)
            .chunk_size(CHUNK_SAMPLES)
            .build()
            .map_err(|error| VadError::Build(error.to_string()))?;

        Ok(Self { detector })
    }
}

impl VoiceDetector for SileroVad {
    fn probability(&mut self, chunk: &[f32]) -> f32 {
        self.detector.predict(chunk.iter().copied())
    }
}
