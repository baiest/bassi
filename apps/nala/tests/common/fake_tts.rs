use std::sync::Mutex;

use nala::adapters::speech::chatterbox::{PlayAudio, SynthesizeSpeech};
use nala::ports::speech::SpeechError;

/// A `SynthesizeSpeech` fake that either returns canned bytes or a
/// configured failure, and records the text it was asked to synthesize.
pub struct FakeSynth {
    result: Result<Vec<u8>, SpeechError>,
    received: Mutex<Vec<String>>,
}

impl FakeSynth {
    pub fn returning(bytes: Vec<u8>) -> Self {
        Self {
            result: Ok(bytes),
            received: Mutex::new(Vec::new()),
        }
    }

    pub fn failing(error: SpeechError) -> Self {
        Self {
            result: Err(error),
            received: Mutex::new(Vec::new()),
        }
    }

    pub fn received(&self) -> Vec<String> {
        self.received.lock().unwrap().clone()
    }
}

impl SynthesizeSpeech for FakeSynth {
    fn synthesize(&self, text: &str) -> Result<Vec<u8>, SpeechError> {
        self.received.lock().unwrap().push(text.to_string());
        self.result.clone()
    }
}

/// A `PlayAudio` spy that records the bytes it was asked to play, or fails
/// if configured to.
pub struct SpyPlayer {
    should_fail: bool,
    played: Mutex<Vec<Vec<u8>>>,
}

impl SpyPlayer {
    pub fn new() -> Self {
        Self {
            should_fail: false,
            played: Mutex::new(Vec::new()),
        }
    }

    pub fn failing() -> Self {
        Self {
            should_fail: true,
            played: Mutex::new(Vec::new()),
        }
    }

    pub fn played(&self) -> Vec<Vec<u8>> {
        self.played.lock().unwrap().clone()
    }
}

impl PlayAudio for SpyPlayer {
    fn play(&self, audio: &[u8]) -> Result<(), SpeechError> {
        self.played.lock().unwrap().push(audio.to_vec());
        if self.should_fail {
            return Err(SpeechError::Playback("simulated playback failure".into()));
        }
        Ok(())
    }
}
