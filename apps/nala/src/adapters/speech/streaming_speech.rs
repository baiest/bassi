use crate::ports::speech::{Speech, SpeechError};

use super::pcm::{PlayPcmStream, StreamSynthesizeSpeech};

/// Orchestrates the two halves of a streaming TTS backend: turn text into a
/// streamed PCM result via `StreamSynthesizeSpeech`, then play it back via
/// `PlayPcmStream` as chunks arrive. Holds no logic of its own beyond that
/// hand-off, so both concerns stay independently testable without a real
/// backend or an audio device. Shared by every streaming backend
/// (Chatterbox, Piper) rather than living inside either adapter.
pub struct StreamingSpeech {
    synth: Box<dyn StreamSynthesizeSpeech + Send>,
    player: Box<dyn PlayPcmStream + Send>,
}

impl StreamingSpeech {
    pub fn new(
        synth: Box<dyn StreamSynthesizeSpeech + Send>,
        player: Box<dyn PlayPcmStream + Send>,
    ) -> Self {
        Self { synth, player }
    }
}

impl Speech for StreamingSpeech {
    fn say(&self, text: &str) -> Result<(), SpeechError> {
        let stream = self.synth.synthesize_stream(text)?;
        self.player.play_stream(stream)
    }
}
