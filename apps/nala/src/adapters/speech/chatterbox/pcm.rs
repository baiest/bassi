use std::sync::mpsc;

use crate::ports::speech::SpeechError;

/// A streamed PCM audio result: format info known up front (parsed from the
/// WAV header before any sample data arrives) plus a channel of sample
/// chunks that keeps producing as the backend keeps generating. Letting the
/// receiver start consuming before the channel is exhausted is the whole
/// point — it's what lets playback start before synthesis finishes.
pub struct PcmStream {
    pub sample_rate: u32,
    pub channels: u16,
    /// Chunks of interleaved 16-bit samples, in generation order. The
    /// stream ends when the sender side is dropped (`recv` returns `Err`)
    /// or a chunk carries a terminal `Err`.
    pub chunks: mpsc::Receiver<Result<Vec<i16>, SpeechError>>,
}

impl std::fmt::Debug for PcmStream {
    /// `mpsc::Receiver` has no `Debug` impl, so this only shows the format
    /// fields — good enough for test assertions like `unwrap_err()`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PcmStream")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}
