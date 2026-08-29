use crate::ports::speech::{Speech, SpeechError};

use super::{PlayPcmStream, StreamSynthesizeSpeech};

/// Orchestrates the two halves of Chatterbox TTS: turn text into a streamed
/// PCM result via `StreamSynthesizeSpeech`, then play it back via
/// `PlayPcmStream` as chunks arrive. Holds no logic of its own beyond that
/// hand-off, so both concerns stay independently testable without
/// Chatterbox or an audio device.
pub struct ChatterboxSpeech {
    synth: Box<dyn StreamSynthesizeSpeech + Send>,
    player: Box<dyn PlayPcmStream + Send>,
}

impl ChatterboxSpeech {
    pub fn new(
        synth: Box<dyn StreamSynthesizeSpeech + Send>,
        player: Box<dyn PlayPcmStream + Send>,
    ) -> Self {
        Self { synth, player }
    }
}

impl Speech for ChatterboxSpeech {
    fn say(&self, text: &str) -> Result<(), SpeechError> {
        let stream = self.synth.synthesize_stream(text)?;
        self.player.play_stream(stream)
    }
}
