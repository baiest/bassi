use std::io::Read;
use std::sync::mpsc;

use crate::ports::speech::SpeechError;

/// Number of bytes to read per `read` call from a streaming TTS backend's
/// output. Small enough to keep playback latency low (the player can start
/// on the first full sample as soon as it arrives), large enough to avoid
/// excessive syscall overhead.
const READ_CHUNK_BYTES: usize = 8192;

/// A streamed PCM audio result: format info known up front (parsed from a
/// WAV header, or read from a voice's config file, before any sample data
/// arrives) plus a channel of sample chunks that keeps producing as the
/// backend keeps generating. Letting the receiver start consuming before
/// the channel is exhausted is the whole point — it's what lets playback
/// start before synthesis finishes. Shared by every streaming TTS backend
/// (Chatterbox over HTTP, Piper over a child process's stdout) so
/// `RodioPlayer` only ever needs to know this one shape.
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

/// Turns text into a streamed PCM result. Internal to the adapters layer,
/// not a hexagon port: Nala only ever sees `Speech`, never a raw audio
/// stream. Implementations should return as soon as the audio format is
/// known, so the caller can start playing chunks while synthesis is still
/// producing more.
pub trait StreamSynthesizeSpeech {
    fn synthesize_stream(&self, text: &str) -> Result<PcmStream, SpeechError>;
}

/// Plays back a `PcmStream` to completion. Kept separate from
/// `StreamSynthesizeSpeech` so backend failures (network, a missing
/// executable) and audio-device failures can be tested and reasoned about
/// independently.
pub trait PlayPcmStream {
    fn play_stream(&self, stream: PcmStream) -> Result<(), SpeechError>;
}

/// Converts a little-endian byte buffer into `i16` samples. `leftover`
/// carries a dangling odd byte from one call to the next, since chunk
/// boundaries from a stream never line up with 2-byte sample boundaries.
pub(crate) fn bytes_to_samples(bytes: &[u8], leftover: &mut Option<u8>) -> Vec<i16> {
    let mut samples = Vec::with_capacity(bytes.len() / 2 + 1);
    let mut iter = bytes.iter().copied();

    let mut pending = leftover.take();
    loop {
        let low = match pending.take() {
            Some(byte) => byte,
            None => match iter.next() {
                Some(byte) => byte,
                None => break,
            },
        };
        match iter.next() {
            Some(high) => samples.push(i16::from_le_bytes([low, high])),
            None => {
                *leftover = Some(low);
                break;
            }
        }
    }

    samples
}

/// Reads raw 16-bit PCM from `reader` until EOF, sending each non-empty
/// chunk of decoded samples into `sender`. Used by every streaming
/// backend's background thread once its own header/format parsing is done
/// and only sample bytes remain. A read error or an empty stream (no
/// samples ever produced) is sent as a terminal `Err`, described by
/// `context` (e.g. "Chatterbox" or "Piper") for a clearer message.
pub(crate) fn stream_pcm_from<R: Read>(
    mut reader: R,
    sender: &mpsc::Sender<Result<Vec<i16>, SpeechError>>,
    context: &str,
) {
    let mut buffer = [0u8; READ_CHUNK_BYTES];
    let mut leftover = None;
    let mut sent_any_samples = false;

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let samples = bytes_to_samples(&buffer[..n], &mut leftover);
                if !samples.is_empty() {
                    sent_any_samples = true;
                    if sender.send(Ok(samples)).is_err() {
                        // Receiver gone (playback stopped listening) - no
                        // point reading more.
                        return;
                    }
                }
            }
            Err(error) => {
                let _ = sender.send(Err(SpeechError::Synthesis(format!(
                    "{context} stream read failed: {error}"
                ))));
                return;
            }
        }
    }

    if !sent_any_samples {
        let _ = sender.send(Err(SpeechError::Synthesis(format!(
            "{context} returned an empty audio stream"
        ))));
    }
}
