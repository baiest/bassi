pub mod config;
pub mod http;
pub mod speech;
pub mod supervisor;

pub use http::HttpChatterbox;

// The PCM streaming seam (`PcmStream` and its two traits) lives in the
// parent `speech` module: it's shared with the Piper backend, not specific
// to Chatterbox. Re-exported here so existing call sites within this module
// (and callers that used to import them from here) keep working.
pub use super::pcm::{PcmStream, PlayPcmStream, StreamSynthesizeSpeech};
