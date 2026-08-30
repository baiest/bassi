//! Speech-to-text: microphone capture and transcription. No dependency on
//! `nala` — any consumer of this crate can use it standalone.

mod capture;
mod resample;
mod ring;
mod session;
mod transcribe;

pub use capture::{CaptureError, RecordedAudio, WHISPER_SAMPLE_RATE, record_until_enter};
pub use resample::Resampler;
pub use ring::Ring;
pub use session::{
    Action, CHUNK_MS, CHUNK_SAMPLES, ListenMode, Session, SessionConfig, SpeechGate,
};
pub use transcribe::{TranscribeError, Transcriber};
