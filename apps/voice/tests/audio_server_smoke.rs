//! End-to-end turns over a real `TcpListener`/`tungstenite` socket, against
//! a fake `nala --serve` that speaks the real JSON protocol (a real nala
//! process is out of scope here — that round-trip is covered by
//! `apps/nala/tests/server_smoke.rs`). Exercises the wire-level
//! `AudioWire for WebSocket<S>` impl and `VoiceSession` together, since a
//! turn now runs on its own thread and only unit tests with a scripted wire
//! (see `audio_server.rs`) can isolate the outbox synchronously.

use std::net::TcpListener;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use agent_protocol::{ClientMessage, Event, LlmCallId, ServerMessage, TaskId};
use stt::{Transcribe, TranscribeError};
use tts::{PcmStream, SpeechError, StreamSynthesizeSpeech};
use tungstenite::Message;
use voice::audio_server::run_audio_session;
use voice::narrator::Narrator;
use voice::session::VoiceSession;
use voice::wav;

const SAMPLE_RATE: u32 = 16_000;

struct FixedTranscriber(&'static str);

impl Transcribe for FixedTranscriber {
    fn transcribe(&self, _samples: &[f32]) -> Result<String, TranscribeError> {
        Ok(self.0.to_string())
    }
}

/// A narrator whose answer is scripted per call — same pattern as
/// `speaking_sink.rs`'s `ScriptedNarrator`.
struct ScriptedNarrator {
    answers: std::collections::VecDeque<Option<&'static str>>,
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

/// Starts a fake `nala --serve` that accepts one connection and replies to
/// each `Input` with the scripted `ServerMessage`s in order, resetting to
/// the next batch after each `Reply`/`Error`.
fn spawn_fake_nala(turns: Vec<Vec<ServerMessage>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr").to_string();

    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept a connection from voice");
        let mut ws = tungstenite::accept(stream).expect("complete the WS handshake");

        // Real Nala sends this once, right after connecting, before ever
        // reading a ClientMessage — VoiceSession's reconnect path expects
        // it first.
        let greeting = serde_json::to_string(&ServerMessage::Event(Event::Greeting {
            text: "hola".to_string(),
        }))
        .unwrap();
        ws.send(Message::Text(greeting)).expect("send the greeting");

        let mut turns = turns.into_iter();

        loop {
            match ws.read() {
                Ok(Message::Text(text)) => {
                    let _: ClientMessage =
                        serde_json::from_str(&text).expect("valid ClientMessage JSON");
                    let Some(messages) = turns.next() else {
                        break;
                    };
                    for message in messages {
                        let json = serde_json::to_string(&message).unwrap();
                        ws.send(Message::Text(json)).expect("send a server message");
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => continue,
            }
        }
    });

    addr
}

/// Sends `utterance` and reads back exactly `expected_clips` binary frames,
/// waiting up to a few seconds for the async turn to produce them.
fn send_and_collect(
    client: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    utterance: Vec<u8>,
    expected_clips: usize,
) -> Vec<Vec<u8>> {
    client.send(Message::Binary(utterance)).unwrap();

    let mut clips = Vec::new();
    while clips.len() < expected_clips {
        match client.read().expect("read a server message") {
            Message::Binary(bytes) => clips.push(bytes),
            _ => continue,
        }
    }
    clips
}

fn task_id() -> TaskId {
    TaskId::new()
}

fn connect_client(
    addr: std::net::SocketAddr,
) -> tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>> {
    let (client, _) = tungstenite::connect(format!("ws://{addr}")).expect("connect as a client");
    client
}

#[test]
fn transcribes_incoming_audio_and_sends_the_reply_as_audio() {
    let nala_addr = spawn_fake_nala(vec![vec![ServerMessage::Reply {
        text: "listo".to_string(),
    }]]);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let session = Arc::new(VoiceSession::new(
        nala_addr,
        Box::new(ScriptedNarrator::new(vec![])),
        Box::new(FakeSynth),
    ));

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        // Matches `handle_connection`'s production setup: without this, a
        // blocking read here would never return control to check the
        // outbox while waiting for the phone's next frame.
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        run_audio_session(&mut ws, &FixedTranscriber("hola"), &session);
    });

    let mut client = connect_client(addr);
    // No greeting here: that's sent directly by `handle_connection` (see
    // `apps/voice/tests/audio_server_greeting_smoke.rs`), which these
    // tests bypass by calling `run_audio_session` directly.
    let clips = send_and_collect(&mut client, silence_wav(), 1);
    assert_eq!(decode_clip_len(&clips[0]), "listo".len());

    client.close(None).ok();
    server.join().unwrap();
}

#[test]
fn narration_audio_is_sent_before_the_reply_audio() {
    let task = task_id();
    let nala_addr = spawn_fake_nala(vec![vec![
        ServerMessage::Event(Event::RequestStarted {
            task_id: task.clone(),
        }),
        ServerMessage::Event(Event::LlmStarted {
            llm_call_id: LlmCallId::new(&task, 1),
            task_id: task,
            call_index: 1,
            images: 0,
        }),
        ServerMessage::Reply {
            text: "listo".to_string(),
        },
    ]]);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let session = Arc::new(VoiceSession::new(
        nala_addr,
        Box::new(ScriptedNarrator::new(vec![Some("un momento"), None])),
        Box::new(FakeSynth),
    ));

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        // Matches `handle_connection`'s production setup: without this, a
        // blocking read here would never return control to check the
        // outbox while waiting for the phone's next frame.
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        run_audio_session(&mut ws, &FixedTranscriber("hola"), &session);
    });

    let mut client = connect_client(addr);
    let clips = send_and_collect(&mut client, silence_wav(), 2);
    assert_eq!(decode_clip_len(&clips[0]), "un momento".len());
    assert_eq!(decode_clip_len(&clips[1]), "listo".len());

    client.close(None).ok();
    server.join().unwrap();
}

#[test]
fn invalid_incoming_audio_is_skipped_without_ending_the_session() {
    let nala_addr = spawn_fake_nala(vec![vec![ServerMessage::Reply {
        text: "listo".to_string(),
    }]]);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let session = Arc::new(VoiceSession::new(
        nala_addr,
        Box::new(ScriptedNarrator::new(vec![])),
        Box::new(FakeSynth),
    ));

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        // Matches `handle_connection`'s production setup: without this, a
        // blocking read here would never return control to check the
        // outbox while waiting for the phone's next frame.
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let mut ws = tungstenite::accept(stream).unwrap();
        run_audio_session(&mut ws, &FixedTranscriber("hola"), &session);
    });

    let mut client = connect_client(addr);
    client.send(Message::Binary(b"not a wav".to_vec())).unwrap();
    // Give the (silently skipped) bad frame a moment before the real one.
    thread::sleep(Duration::from_millis(50));
    let clips = send_and_collect(&mut client, silence_wav(), 1);
    assert_eq!(decode_clip_len(&clips[0]), "listo".len());

    client.close(None).ok();
    server.join().unwrap();
}
