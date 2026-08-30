use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Debug, thiserror::Error)]
pub enum TranscribeError {
    #[error("failed to load whisper model at '{0}': {1}")]
    ModelLoad(String, String),
    #[error("transcription failed: {0}")]
    Transcription(String),
}

/// Turns audio samples into text.
///
/// A trait so anything built on top of it — the wake detector, the
/// listener — can be tested with a scripted fake instead of a real,
/// multi-hundred-megabyte model.
pub trait Transcribe {
    /// `samples` must be mono at [`crate::WHISPER_SAMPLE_RATE`].
    fn transcribe(&self, samples: &[f32]) -> Result<String, TranscribeError>;
}

/// Wraps a loaded whisper.cpp model. Loading is slow (reads the whole
/// model file), so build one `Transcriber` once and reuse it across turns
/// rather than loading per call.
pub struct Transcriber {
    context: WhisperContext,
}

impl Transcriber {
    pub fn load(model_path: &str) -> Result<Self, TranscribeError> {
        let context =
            WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
                .map_err(|e| TranscribeError::ModelLoad(model_path.to_string(), e.to_string()))?;

        Ok(Self { context })
    }

    /// `samples` must be mono at [`crate::WHISPER_SAMPLE_RATE`] — exactly
    /// what [`crate::record_until_enter`] returns.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String, TranscribeError> {
        let mut state = self
            .context
            .create_state()
            .map_err(|e| TranscribeError::Transcription(e.to_string()))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("es"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);

        state
            .full(params, samples)
            .map_err(|e| TranscribeError::Transcription(e.to_string()))?;

        let num_segments = state
            .full_n_segments()
            .map_err(|e| TranscribeError::Transcription(e.to_string()))?;

        let mut text = String::new();
        for i in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(i) {
                text.push_str(&segment);
            }
        }

        Ok(text.trim().to_string())
    }
}

impl Transcribe for Transcriber {
    fn transcribe(&self, samples: &[f32]) -> Result<String, TranscribeError> {
        Transcriber::transcribe(self, samples)
    }
}
