use std::sync::{Arc, Mutex};

use tts::{Speech, SpeechError};

#[derive(Default, Clone)]
pub struct SpySpeech {
    spoken: Arc<Mutex<Vec<String>>>,
}

impl SpySpeech {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spoken(&self) -> Vec<String> {
        self.spoken.lock().unwrap().clone()
    }
}

impl Speech for SpySpeech {
    fn say(&self, text: &str) -> Result<(), SpeechError> {
        self.spoken.lock().unwrap().push(text.to_string());
        Ok(())
    }
}
