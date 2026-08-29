pub mod config;
pub mod http;
pub mod pcm;
pub mod speech;
pub mod supervisor;

pub use http::HttpChatterbox;
pub use pcm::PcmStream;

use crate::ports::speech::SpeechError;

/// Turns text into a streamed PCM result. Internal to this adapter module,
/// not a hexagon port: Nala only ever sees `Speech`, never a raw audio
/// stream. Returns as soon as the response header is parsed, so the caller
/// can start playing chunks while synthesis is still producing more.
pub trait StreamSynthesizeSpeech {
    fn synthesize_stream(&self, text: &str) -> Result<PcmStream, SpeechError>;
}

/// Plays back a `PcmStream` to completion. Kept separate from
/// `StreamSynthesizeSpeech` so network failures and audio-device failures
/// can be tested and reasoned about independently.
pub trait PlayPcmStream {
    fn play_stream(&self, stream: PcmStream) -> Result<(), SpeechError>;
}
