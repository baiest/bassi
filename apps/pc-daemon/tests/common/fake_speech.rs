use std::sync::Mutex;

use tts::{Speech, SpeechError};

/// Records every `say` call instead of speaking anything, so a test can
/// assert what the daemon tried to say without a real audio device.
#[derive(Default)]
pub struct FakeSpeech {
    pub said: Mutex<Vec<String>>,
}

impl FakeSpeech {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Speech for FakeSpeech {
    fn say(&self, text: &str) -> Result<(), SpeechError> {
        self.said.lock().unwrap().push(text.to_string());
        Ok(())
    }
}
