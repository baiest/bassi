pub mod config;
pub mod http;
pub mod supervisor;

pub use http::HttpChatterbox;

// The PCM streaming seam (`PcmStream`, its two traits, and the
// `StreamingSpeech` hand-off) lives in the parent `speech` module: it's
// shared with the Piper backend, not specific to Chatterbox. Re-exported
// here so callers that used to import them from here keep working.
pub use super::pcm::{PcmStream, PlayPcmStream, StreamSynthesizeSpeech};
pub use super::streaming_speech::StreamingSpeech;
