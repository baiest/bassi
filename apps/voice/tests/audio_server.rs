use std::collections::VecDeque;
use std::sync::mpsc;

use agent_protocol::{ClientMessage, Event, LlmCallId, ServerMessage, TaskId};
use stt::{Transcribe, TranscribeError};
use tts::{PcmStream, SpeechError, StreamSynthesizeSpeech};
use voice::audio_server::{AudioWire, run_audio_session};
use voice::client::{ClientError, NalaClient, Wire as ClientWire};
use voice::narrator::Narrator;
use voice::wav;

const SAMPLE_RATE: u32 = 16_000;

/// An in-memory `AudioWire`: `recv` pops scripted incoming WAVs, `send`
/// records every WAV clip it was asked to send, in order.
struct FakeAudioWire {
    incoming: VecDeque<Vec<u8>>,
    sent: Vec<Vec<u8>>,
}

impl FakeAudioWire {
    fn new(incoming: Vec<Vec<u8>>) -> Self {
        Self {
            incoming: incoming.into(),
            sent: Vec::new(),
        }
    }
}

impl AudioWire for FakeAudioWire {
    fn recv(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        Ok(self.incoming.pop_front())
    }

    fn send(&mut self, wav: Vec<u8>) -> std::io::Result<()> {
        self.sent.push(wav);
        Ok(())
    }
}

/// An in-memory `client::Wire`: same shape as the one in `tests/client.rs`,
/// scripted with the server messages a turn should receive.
struct FakeClientWire {
    incoming: VecDeque<ServerMessage>,
}

impl FakeClientWire {
    fn new(incoming: Vec<ServerMessage>) -> Self {
        Self {
            incoming: incoming.into(),
        }
    }
}

impl ClientWire for FakeClientWire {
    fn send(&mut self, _message: &ClientMessage) -> Result<(), ClientError> {
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<ServerMessage>, ClientError> {
        Ok(self.incoming.pop_front())
    }
}

/// Always transcribes to the same fixed text, regardless of the samples —
/// good enough since this suite is about the audio-session wiring, not
/// Whisper itself (covered separately in `crates/stt`).
struct FixedTranscriber(&'static str);

impl Transcribe for FixedTranscriber {
    fn transcribe(&self, _samples: &[f32]) -> Result<String, TranscribeError> {
        Ok(self.0.to_string())
    }
}

/// A narrator whose answer is scripted per call — same pattern as
/// `speaking_sink.rs`'s `ScriptedNarrator`.
struct ScriptedNarrator {
    answers: VecDeque<Option<&'static str>>,
}

impl ScriptedNarrator {
    fn new(answers: Vec<Option<&'static str>>) -> Self {
        Self {
            answers: answers.into(),
        }
    }
}

impl Narrator for ScriptedNarrator {
    fn narrate(&mut self, _event: &Event) -> Option<String> {
        self.answers.pop_front().flatten().map(str::to_string)
    }
}

/// Synthesizes `text` into one sample per character, so a test can assert
/// on the clip's length without depending on a real TTS backend.
struct FakeSynth;

impl StreamSynthesizeSpeech for FakeSynth {
    fn synthesize_stream(&self, text: &str) -> Result<PcmStream, SpeechError> {
        let (tx, rx) = mpsc::channel();
        let samples: Vec<i16> = (0..text.len() as i16).collect();
        tx.send(Ok(samples)).unwrap();
        Ok(PcmStream {
            sample_rate: SAMPLE_RATE,
            channels: 1,
            chunks: rx,
        })
    }
}

fn silence_wav() -> Vec<u8> {
    wav::encode_wav(&[0; 100], SAMPLE_RATE, 1)
}

fn decode_clip_len(clip: &[u8]) -> usize {
    wav::decode_wav(clip, SAMPLE_RATE).unwrap().len()
}

fn task_id() -> TaskId {
    TaskId::new()
}

#[test]
fn transcribes_incoming_audio_and_sends_the_reply_as_audio() {
    let mut audio_wire = FakeAudioWire::new(vec![silence_wav()]);
    let client_wire = FakeClientWire::new(vec![ServerMessage::Reply {
        text: "listo".to_string(),
    }]);
    let mut client = NalaClient::new(client_wire);
    let mut narrator = ScriptedNarrator::new(vec![]);
    let synth = FakeSynth;

    run_audio_session(
        &mut audio_wire,
        &FixedTranscriber("hola"),
        &mut client,
        &mut narrator,
        &synth,
    );

    assert_eq!(audio_wire.sent.len(), 1);
    assert_eq!(decode_clip_len(&audio_wire.sent[0]), "listo".len());
}

#[test]
fn narration_audio_is_sent_before_the_reply_audio() {
    let mut audio_wire = FakeAudioWire::new(vec![silence_wav()]);
    let task_id = task_id();
    let client_wire = FakeClientWire::new(vec![
        ServerMessage::Event(Event::RequestStarted {
            task_id: task_id.clone(),
        }),
        ServerMessage::Event(Event::LlmStarted {
            llm_call_id: LlmCallId::new(&task_id, 1),
            task_id,
            call_index: 1,
            images: 0,
        }),
        ServerMessage::Reply {
            text: "listo".to_string(),
        },
    ]);
    let mut client = NalaClient::new(client_wire);
    let mut narrator = ScriptedNarrator::new(vec![Some("un momento"), None]);
    let synth = FakeSynth;

    run_audio_session(
        &mut audio_wire,
        &FixedTranscriber("hola"),
        &mut client,
        &mut narrator,
        &synth,
    );

    assert_eq!(audio_wire.sent.len(), 2);
    assert_eq!(decode_clip_len(&audio_wire.sent[0]), "un momento".len());
    assert_eq!(decode_clip_len(&audio_wire.sent[1]), "listo".len());
}

#[test]
fn invalid_incoming_audio_is_skipped_without_ending_the_session() {
    let mut audio_wire = FakeAudioWire::new(vec![b"not a wav".to_vec(), silence_wav()]);
    let client_wire = FakeClientWire::new(vec![ServerMessage::Reply {
        text: "listo".to_string(),
    }]);
    let mut client = NalaClient::new(client_wire);
    let mut narrator = ScriptedNarrator::new(vec![]);
    let synth = FakeSynth;

    run_audio_session(
        &mut audio_wire,
        &FixedTranscriber("hola"),
        &mut client,
        &mut narrator,
        &synth,
    );

    assert_eq!(audio_wire.sent.len(), 1);
    assert_eq!(decode_clip_len(&audio_wire.sent[0]), "listo".len());
}

#[test]
fn an_empty_transcription_is_skipped_without_calling_nala() {
    let mut audio_wire = FakeAudioWire::new(vec![silence_wav()]);
    // No server messages scripted: if `client.send` were called, the
    // session would error out trying to read a reply that never comes.
    let client_wire = FakeClientWire::new(vec![]);
    let mut client = NalaClient::new(client_wire);
    let mut narrator = ScriptedNarrator::new(vec![]);
    let synth = FakeSynth;

    run_audio_session(
        &mut audio_wire,
        &FixedTranscriber("   "),
        &mut client,
        &mut narrator,
        &synth,
    );

    assert!(audio_wire.sent.is_empty());
}

#[test]
fn the_session_loop_ends_when_the_client_disconnects() {
    let mut audio_wire = FakeAudioWire::new(vec![]);
    let client_wire = FakeClientWire::new(vec![]);
    let mut client = NalaClient::new(client_wire);
    let mut narrator = ScriptedNarrator::new(vec![]);
    let synth = FakeSynth;

    run_audio_session(
        &mut audio_wire,
        &FixedTranscriber("hola"),
        &mut client,
        &mut narrator,
        &synth,
    );

    assert!(audio_wire.sent.is_empty());
}
