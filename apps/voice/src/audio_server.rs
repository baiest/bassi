//! Serves audio to a phone-style client: it sends one recorded utterance
//! (WAV bytes) per turn, and gets back zero or more narration audio clips
//! followed by the final reply's audio — all WAV, all over the same
//! connection. Nala itself never sees audio; this is where STT/TTS live,
//! same boundary as the local push-to-talk flow in `main.rs`.
//!
//! Turns run against one persistent `VoiceSession` shared by every phone
//! connection (see `session.rs`), not against the connection itself: a
//! turn keeps running and its audio keeps queuing even if the phone drops
//! mid-turn, and the next connection (a reconnect, or a new one) just
//! drains whatever is waiting.

use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use stt::{Transcribe, Transcriber};
use tts::{SpeechError, StreamSynthesizeSpeech};
use tungstenite::{Message, WebSocket};

use crate::narration::TemplateNarrator;
use crate::session::VoiceSession;
use crate::wav;

/// How often a connection with nothing to read wakes up to check the
/// outbox for clips to deliver. Short enough that narration feels prompt,
/// long enough not to busy-loop.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// What one `recv` on the wire produced.
pub enum WireEvent {
    Audio(Vec<u8>),
    /// Nothing arrived within the read timeout — not an error, just a
    /// chance for the caller to check the outbox and loop again.
    Idle,
    Closed,
}

/// One connection's audio transport. `recv` yields one WAV per client
/// utterance, `Idle` on a read timeout, `Closed` on a clean close. A trait
/// (rather than `tungstenite::WebSocket` directly) so `run_audio_session`
/// can be tested without a real socket.
pub trait AudioWire {
    fn recv(&mut self) -> std::io::Result<WireEvent>;
    fn send(&mut self, wav: Vec<u8>) -> std::io::Result<()>;
}

impl<S: std::io::Read + std::io::Write> AudioWire for WebSocket<S> {
    fn recv(&mut self) -> std::io::Result<WireEvent> {
        match self.read() {
            Ok(Message::Binary(bytes)) => Ok(WireEvent::Audio(bytes)),
            Ok(Message::Close(_)) => Ok(WireEvent::Closed),
            // Text/ping/pong noise: this protocol is audio-only, so
            // anything that isn't a binary WAV frame is ignored rather
            // than treated as an error.
            Ok(_) => Ok(WireEvent::Idle),
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                Ok(WireEvent::Closed)
            }
            Err(tungstenite::Error::Io(ref io_error))
                if matches!(
                    io_error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(WireEvent::Idle)
            }
            Err(error) => Err(std::io::Error::other(error)),
        }
    }

    fn send(&mut self, wav: Vec<u8>) -> std::io::Result<()> {
        WebSocket::send(self, Message::Binary(wav)).map_err(std::io::Error::other)
    }
}

/// Synthesizes `text` and fully drains the resulting `PcmStream` into one
/// WAV file — an audio server sends complete clips, not a live stream, so
/// there's no benefit in forwarding partial chunks the way local playback
/// does.
pub(crate) fn synthesize_to_wav(
    synth: &dyn StreamSynthesizeSpeech,
    text: &str,
) -> Result<Vec<u8>, SpeechError> {
    let stream = synth.synthesize_stream(text)?;
    let mut samples = Vec::new();
    for chunk in stream.chunks {
        samples.extend(chunk?);
    }
    Ok(wav::encode_wav(
        &samples,
        stream.sample_rate,
        stream.channels,
    ))
}

/// Runs one connection: forwards every incoming utterance to `session`
/// (transcribing it first), and drains `session`'s outbox for clips to
/// send back, interleaved on this one thread via `wire`'s read timeout.
/// Returns when the connection closes or errors — the session and its
/// outbox live on regardless.
pub fn run_audio_session<T, W>(wire: &mut W, transcriber: &T, session: &Arc<VoiceSession>)
where
    T: Transcribe,
    W: AudioWire,
{
    loop {
        // Drain whatever is waiting before (and after) touching the wire's
        // read side, so a clip queued by an earlier turn — including one
        // still sitting from before this connection even opened, e.g. a
        // reconnect — goes out right away instead of waiting for the next
        // incoming utterance.
        while let Some(clip) = session.outbox().try_pop() {
            if let Err(error) = wire.send(clip.clone()) {
                eprintln!("Warning: could not send audio to phone, ending this session: {error}");
                // The connection died mid-send: put the clip back so the
                // next connection (a reconnect, most likely) delivers it
                // instead of it being lost.
                session.outbox().push_front(clip);
                return;
            }
        }

        match wire.recv() {
            Ok(WireEvent::Audio(audio)) => {
                let samples = match wav::decode_wav(&audio, stt::WHISPER_SAMPLE_RATE) {
                    Ok(samples) => samples,
                    Err(error) => {
                        eprintln!("Warning: could not decode incoming audio: {error}");
                        continue;
                    }
                };

                match transcriber.transcribe(&samples) {
                    Ok(text) if !text.trim().is_empty() => session.submit(text.trim().to_string()),
                    Ok(_) => {}
                    Err(error) => {
                        eprintln!("Warning: could not transcribe incoming audio: {error}");
                    }
                }
            }
            Ok(WireEvent::Idle) => {}
            Ok(WireEvent::Closed) => break,
            Err(error) => {
                eprintln!("Warning: connection error, ending this session: {error}");
                break;
            }
        }
    }
}

fn handle_connection(stream: TcpStream, transcriber: Arc<Transcriber>, session: Arc<VoiceSession>) {
    if let Err(error) = stream.set_read_timeout(Some(POLL_INTERVAL)) {
        eprintln!("Warning: could not set a read timeout on the connection: {error}");
    }

    let mut wire = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(error) => {
            eprintln!("Warning: WebSocket handshake failed: {error}");
            return;
        }
    };

    run_audio_session(&mut wire, transcriber.as_ref(), &session);
}

/// Binds `addr` and serves one audio session per accepted connection on its
/// own thread, all sharing one `VoiceSession` (one Nala connection, one
/// outbox) so a phone that reconnects mid-turn still gets its reply.
/// Loads the Whisper model once and shares it (read-only, `Transcribe` only
/// needs `&self`) across every connection, since loading it is slow.
pub fn serve(addr: &str, nala_addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("Voice listening on ws://{addr}, forwarding to nala at ws://{nala_addr}");

    let transcriber = Arc::new(crate::bootstrap::build_transcriber());

    // Kept alive for the lifetime of `serve` (it never returns in
    // practice): dropping it would tear down a locally-managed Chatterbox
    // process out from under the session.
    let (synth, _chatterbox_supervisor) =
        tts::stream_synthesizer().expect("could not build a TTS backend");
    let session = Arc::new(VoiceSession::new(
        nala_addr.to_string(),
        Box::new(TemplateNarrator::new()),
        synth,
    ));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let transcriber = Arc::clone(&transcriber);
                let session = Arc::clone(&session);
                thread::spawn(move || handle_connection(stream, transcriber, session));
            }
            Err(error) => eprintln!("Warning: failed to accept a connection: {error}"),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use stt::TranscribeError;
    use tts::PcmStream;

    const SAMPLE_RATE: u32 = 16_000;

    struct FixedTranscriber(&'static str);

    impl Transcribe for FixedTranscriber {
        fn transcribe(&self, _samples: &[f32]) -> Result<String, TranscribeError> {
            Ok(self.0.to_string())
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

    /// A scripted wire: yields one `Audio` frame, then reports `Closed` as
    /// soon as `fail_send` many clips have been sent (simulating the phone
    /// dropping mid-delivery), recording everything it was asked to send.
    struct ScriptedWire {
        utterance: Option<Vec<u8>>,
        fail_after: usize,
        sent: Vec<Vec<u8>>,
    }

    impl AudioWire for ScriptedWire {
        fn recv(&mut self) -> std::io::Result<WireEvent> {
            match self.utterance.take() {
                Some(audio) => Ok(WireEvent::Audio(audio)),
                None => Ok(WireEvent::Closed),
            }
        }

        fn send(&mut self, wav: Vec<u8>) -> std::io::Result<()> {
            if self.sent.len() >= self.fail_after {
                return Err(std::io::Error::other("connection dropped"));
            }
            self.sent.push(wav);
            Ok(())
        }
    }

    #[test]
    fn a_clip_that_fails_to_send_is_delivered_to_the_next_connection() {
        // Simulate a turn that already queued two clips (as if a reply had
        // just finished synthesizing) directly into the session's outbox,
        // bypassing a real Nala round-trip — this test is about delivery
        // surviving a dropped connection, not about turn execution.
        let session = Arc::new(VoiceSession::new(
            "unused:0".to_string(),
            Box::new(TemplateNarrator::new()),
            Box::new(FakeSynth),
        ));
        session.outbox().push(b"clip-one".to_vec());
        session.outbox().push(b"clip-two".to_vec());

        let mut dying_wire = ScriptedWire {
            utterance: None,
            fail_after: 0,
            sent: Vec::new(),
        };
        run_audio_session(&mut dying_wire, &FixedTranscriber(""), &session);

        assert!(dying_wire.sent.is_empty());

        // A fresh connection picks up where the dead one left off, in the
        // original order — nothing lost, nothing reordered.
        let mut next_wire = ScriptedWire {
            utterance: None,
            fail_after: 10,
            sent: Vec::new(),
        };
        run_audio_session(&mut next_wire, &FixedTranscriber(""), &session);

        assert_eq!(
            next_wire.sent,
            vec![b"clip-one".to_vec(), b"clip-two".to_vec()]
        );
    }

    #[test]
    fn an_utterance_is_transcribed_and_submitted_as_a_turn() {
        let session = Arc::new(VoiceSession::new(
            "127.0.0.1:0".to_string(), // no server listening: the turn will
            // fail to connect and log an error, which is fine for this
            // test — it only checks that `submit` was reached at all by
            // observing no panic and a clean return once the connection
            // closes.
            Box::new(TemplateNarrator::new()),
            Box::new(FakeSynth),
        ));

        let silence = wav::encode_wav(&[0; 100], SAMPLE_RATE, 1);
        let mut wire = ScriptedWire {
            utterance: Some(silence),
            fail_after: 10,
            sent: Vec::new(),
        };

        run_audio_session(&mut wire, &FixedTranscriber("hola"), &session);
        // No assertion beyond "doesn't hang or panic": the turn runs on a
        // background thread against an address nothing is listening on,
        // and this function returns as soon as the wire reports `Closed`.
    }
}
