#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    #[error("TTS backend failed: {0}")]
    Backend(String),
}

pub trait Speech {
    fn say(&self, text: &str) -> Result<(), SpeechError>;
}
