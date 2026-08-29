pub mod config;
pub mod http;
pub mod speech;
pub mod supervisor;

pub use http::HttpChatterbox;

use crate::ports::speech::SpeechError;

/// Turns text into synthesized audio bytes (WAV). Internal to this adapter
/// module, not a hexagon port: Nala only ever sees `Speech`, never a raw
/// audio buffer.
pub trait SynthesizeSpeech {
    fn synthesize(&self, text: &str) -> Result<Vec<u8>, SpeechError>;
}

/// Plays back already-synthesized audio bytes. Kept separate from
/// `SynthesizeSpeech` so network failures and audio-device failures can be
/// tested and reasoned about independently.
pub trait PlayAudio {
    fn play(&self, audio: &[u8]) -> Result<(), SpeechError>;
}
