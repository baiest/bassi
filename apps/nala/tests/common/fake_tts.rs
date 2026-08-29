use std::sync::Mutex;
use std::sync::mpsc;

use nala::adapters::speech::chatterbox::{PcmStream, PlayPcmStream, StreamSynthesizeSpeech};
use nala::ports::speech::SpeechError;

/// A `StreamSynthesizeSpeech` fake that either returns a `PcmStream` made
/// of canned chunks or a configured up-front failure, and records the text
/// it was asked to synthesize.
pub struct FakeSynth {
    chunks: Result<Vec<Vec<i16>>, SpeechError>,
    sample_rate: u32,
    channels: u16,
    received: Mutex<Vec<String>>,
}

impl FakeSynth {
    /// Succeeds with a single chunk containing `samples`, at 24 kHz mono
    /// (Chatterbox's typical output format - exact values don't matter to
    /// callers that only assert on the samples themselves).
    pub fn returning(samples: Vec<i16>) -> Self {
        Self {
            chunks: Ok(vec![samples]),
            sample_rate: 24_000,
            channels: 1,
            received: Mutex::new(Vec::new()),
        }
    }

    pub fn failing(error: SpeechError) -> Self {
        Self {
            chunks: Err(error),
            sample_rate: 24_000,
            channels: 1,
            received: Mutex::new(Vec::new()),
        }
    }

    pub fn received(&self) -> Vec<String> {
        self.received.lock().unwrap().clone()
    }
}

impl StreamSynthesizeSpeech for FakeSynth {
    fn synthesize_stream(&self, text: &str) -> Result<PcmStream, SpeechError> {
        self.received.lock().unwrap().push(text.to_string());

        let chunks = self.chunks.clone()?;
        let (sender, receiver) = mpsc::channel();
        for chunk in chunks {
            let _ = sender.send(Ok(chunk));
        }

        Ok(PcmStream {
            sample_rate: self.sample_rate,
            channels: self.channels,
            chunks: receiver,
        })
    }
}

/// A `PlayPcmStream` spy that drains the stream, recording every sample it
/// was asked to play (flattened across chunks), or fails if configured to.
pub struct SpyPlayer {
    should_fail: bool,
    played: Mutex<Vec<i16>>,
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

    pub fn played(&self) -> Vec<i16> {
        self.played.lock().unwrap().clone()
    }
}

impl PlayPcmStream for SpyPlayer {
    fn play_stream(&self, stream: PcmStream) -> Result<(), SpeechError> {
        for chunk in stream.chunks {
            let samples = chunk?;
            self.played.lock().unwrap().extend(samples);
        }

        if self.should_fail {
            return Err(SpeechError::Playback("simulated playback failure".into()));
        }
        Ok(())
    }
}
