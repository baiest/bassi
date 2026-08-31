//! One end-to-end turn over a real `TcpListener`/`tungstenite` socket, to
//! cover the wire-level `AudioWire for WebSocket<S>` impl that the
//! fake-`AudioWire` tests in `audio_server.rs` don't exercise. The Nala
//! side is still faked (a real nala process is out of scope here — that
//! round-trip is covered by `apps/nala/tests/server_smoke.rs`).

use std::collections::VecDeque;
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

use agent_protocol::{ClientMessage, ServerMessage};
use stt::{Transcribe, TranscribeError};
use tts::{PcmStream, SpeechError, StreamSynthesizeSpeech};
use tungstenite::Message;
use voice::audio_server::run_audio_session;
use voice::client::{ClientError, NalaClient, Wire as ClientWire};
use voice::narration::TemplateNarrator;
use voice::wav;

const SAMPLE_RATE: u32 = 16_000;

struct FixedTranscriber(&'static str);

impl Transcribe for FixedTranscriber {
    fn transcribe(&self, _samples: &[f32]) -> Result<String, TranscribeError> {
        Ok(self.0.to_string())
    }
}

struct FakeClientWire {
    incoming: VecDeque<ServerMessage>,
}

impl ClientWire for FakeClientWire {
    fn send(&mut self, _message: &ClientMessage) -> Result<(), ClientError> {
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<ServerMessage>, ClientError> {
        Ok(self.incoming.pop_front())
    }
}

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

#[test]
fn a_client_can_send_audio_and_receive_reply_audio_over_a_real_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr");

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept a connection");
        let mut ws = tungstenite::accept(stream).expect("complete the WS handshake");

        let client_wire = FakeClientWire {
            incoming: VecDeque::from([ServerMessage::Reply {
                text: "listo".to_string(),
            }]),
        };
        let mut client = NalaClient::new(client_wire);
        let mut narrator = TemplateNarrator::new();

        run_audio_session(
            &mut ws,
            &FixedTranscriber("hola"),
            &mut client,
            &mut narrator,
            &FakeSynth,
        );
    });

    let (mut client, _) =
        tungstenite::connect(format!("ws://{addr}")).expect("connect as a client");

    let silence = wav::encode_wav(&[0; 100], SAMPLE_RATE, 1);
    client.send(Message::Binary(silence)).unwrap();

    let reply = loop {
        match client.read().expect("read a server message") {
            Message::Binary(bytes) => break bytes,
            _ => continue,
        }
    };
    let decoded = wav::decode_wav(&reply, SAMPLE_RATE).expect("reply should be a valid WAV");
    assert_eq!(decoded.len(), "listo".len());

    client.close(None).ok();
    server.join().expect("server thread should not panic");
}
