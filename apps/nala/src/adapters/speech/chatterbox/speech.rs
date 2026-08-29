use crate::ports::speech::{Speech, SpeechError};

use super::{PlayAudio, SynthesizeSpeech};

/// Orchestrates the two halves of Chatterbox TTS: turn text into audio via
/// `SynthesizeSpeech`, then play it back via `PlayAudio`. Holds no logic of
/// its own beyond that hand-off, so both concerns stay independently
/// testable without Chatterbox or an audio device.
pub struct ChatterboxSpeech {
    synth: Box<dyn SynthesizeSpeech + Send>,
    player: Box<dyn PlayAudio + Send>,
}

impl ChatterboxSpeech {
    pub fn new(synth: Box<dyn SynthesizeSpeech + Send>, player: Box<dyn PlayAudio + Send>) -> Self {
        Self { synth, player }
    }
}

impl Speech for ChatterboxSpeech {
    fn say(&self, text: &str) -> Result<(), SpeechError> {
        let audio = self.synth.synthesize(text)?;
        self.player.play(&audio)
    }
}
