#[derive(Debug, Clone, thiserror::Error)]
pub enum SpeechError {
    #[error("TTS backend failed: {0}")]
    Backend(String),
    /// The backend is not reachable at all: connection refused, DNS
    /// failure, timeout, or a 5xx response. Distinct from `Synthesis`
    /// because callers may want to fall back to another backend on this
    /// one but not on a request-shaped error.
    #[error("TTS backend unavailable: {0}")]
    Unavailable(String),
    /// The backend was reachable but refused or botched this specific
    /// request (4xx, empty body, unknown voice).
    #[error("TTS synthesis failed: {0}")]
    Synthesis(String),
    /// Audio was produced but could not be played (corrupt data, no
    /// output device).
    #[error("TTS playback failed: {0}")]
    Playback(String),
    /// The backend's own configuration is invalid (missing reference
    /// file, bad URL, unparseable parameter). Meant to be caught at
    /// startup, not in the middle of a turn.
    #[error("TTS configuration invalid: {0}")]
    Configuration(String),
}

pub trait Speech {
    fn say(&self, text: &str) -> Result<(), SpeechError>;
}
