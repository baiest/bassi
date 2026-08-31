//! The speech engine: a `Speech` port plus every backend (Piper, Chatterbox,
//! Windows SAPI) and the audio-output adapter that plays synthesized PCM.
//! Has no dependency on `nala` — anything that wants text spoken depends on
//! this crate directly.

mod async_speech;
mod audio;
pub mod chatterbox;
mod pcm;
pub mod piper;
mod streaming_speech;
mod windows_sapi;

mod backend;
mod speech;

pub use async_speech::AsyncSpeech;
pub use audio::RodioPlayer;
pub use backend::{NullSpeech, speech_backend, stream_synthesizer};
pub use chatterbox::ChatterboxSupervisor;
pub use pcm::{PcmStream, PlayPcmStream, StreamSynthesizeSpeech};
pub use speech::{Speech, SpeechError};
pub use streaming_speech::StreamingSpeech;
pub use windows_sapi::WindowsSapiSpeech;
