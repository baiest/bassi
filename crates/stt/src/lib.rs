//! Speech-to-text: microphone capture and transcription. No dependency on
//! `nala` — any consumer of this crate can use it standalone.

mod capture;
mod listener;
mod resample;
mod ring;
mod session;
mod stream;
mod transcribe;
mod vad;
mod wake;

pub use capture::{CaptureError, RecordedAudio, WHISPER_SAMPLE_RATE, record_until_enter};
pub use listener::{ListenError, Listener, ListenerConfig};
pub use resample::Resampler;
pub use ring::Ring;
pub use session::{
    Action, CHUNK_MS, CHUNK_SAMPLES, ListenMode, Session, SessionConfig, SpeechGate,
};
pub use stream::{AudioSource, MicStream};
pub use transcribe::{Transcribe, TranscribeError, Transcriber};
pub use vad::{SileroVad, VadError, VoiceDetector};
pub use wake::{WAKE_PHRASES, WakeDetector, WhisperWake, strip_wake_prefix};
