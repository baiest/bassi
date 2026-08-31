//! Serves audio to a phone-style client: it sends one recorded utterance
//! (WAV bytes) per turn, and gets back zero or more narration audio clips
//! followed by the final reply's audio — all WAV, all over the same
//! connection. Nala itself never sees audio; this is where STT/TTS live,
//! same boundary as the local push-to-talk flow in `main.rs`.

use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use agent_protocol::Event;
use stt::{Transcribe, Transcriber};
use tts::{SpeechError, StreamSynthesizeSpeech};
use tungstenite::{Message, WebSocket};

use crate::client::{self, NalaClient, TcpWire};
use crate::narration::TemplateNarrator;
use crate::narrator::Narrator;
use crate::wav;

/// One connection's audio transport. `recv` yields one WAV per client
/// utterance (`Ok(None)` on a clean close); `send` pushes one WAV clip to
/// play. A trait (rather than `tungstenite::WebSocket` directly) so
/// `run_audio_session` can be tested without a real socket.
pub trait AudioWire {
    fn recv(&mut self) -> std::io::Result<Option<Vec<u8>>>;
    fn send(&mut self, wav: Vec<u8>) -> std::io::Result<()>;
}

impl<S: std::io::Read + std::io::Write> AudioWire for WebSocket<S> {
    fn recv(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        loop {
            match self.read() {
                Ok(Message::Binary(bytes)) => return Ok(Some(bytes)),
                // Text/ping/pong noise: this protocol is audio-only, so
                // anything that isn't a binary WAV frame is ignored rather
                // than treated as an error.
                Ok(Message::Close(_)) => return Ok(None),
                Ok(_) => continue,
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    return Ok(None);
                }
                Err(error) => return Err(std::io::Error::other(error)),
            }
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
fn synthesize_to_wav(
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

/// Runs one connection's session loop: receive a WAV utterance, transcribe
/// it, forward the text to Nala — narrating each progress event as an audio
/// clip sent immediately, same as the local flow speaks them — then send
/// the final reply as one more audio clip. Repeats until the client
/// disconnects.
pub fn run_audio_session<T, C, N, W>(
    wire: &mut W,
    transcriber: &T,
    client: &mut NalaClient<C>,
    narrator: &mut N,
    synth: &dyn StreamSynthesizeSpeech,
) where
    T: Transcribe,
    C: client::Wire,
    N: Narrator,
    W: AudioWire,
{
    loop {
        let audio = match wire.recv() {
            Ok(Some(audio)) => audio,
            Ok(None) => break,
            Err(error) => {
                eprintln!("Warning: connection error, ending this session: {error}");
                break;
            }
        };

        let samples = match wav::decode_wav(&audio, stt::WHISPER_SAMPLE_RATE) {
            Ok(samples) => samples,
            Err(error) => {
                eprintln!("Warning: could not decode incoming audio: {error}");
                continue;
            }
        };

        let text = match transcriber.transcribe(&samples) {
            Ok(text) if !text.trim().is_empty() => text,
            Ok(_) => continue,
            Err(error) => {
                eprintln!("Warning: could not transcribe incoming audio: {error}");
                continue;
            }
        };

        let reply = client.send(text.trim(), |event: Event| {
            let Some(phrase) = narrator.narrate(&event) else {
                return;
            };
            match synthesize_to_wav(synth, &phrase) {
                Ok(clip) => {
                    if let Err(error) = wire.send(clip) {
                        eprintln!("Warning: could not send narration audio: {error}");
                    }
                }
                Err(error) => {
                    eprintln!("Warning: could not synthesize narration: {error}");
                }
            }
        });

        match reply {
            Ok(text) => match synthesize_to_wav(synth, &text) {
                Ok(clip) => {
                    if let Err(error) = wire.send(clip) {
                        eprintln!("Warning: could not send the reply audio: {error}");
                    }
                }
                Err(error) => {
                    eprintln!("Warning: could not synthesize the reply: {error}");
                }
            },
            Err(error) => {
                eprintln!("Error: {error}");
            }
        }
    }
}

fn handle_connection(stream: TcpStream, transcriber: Arc<Transcriber>, nala_addr: &str) {
    let mut wire = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(error) => {
            eprintln!("Warning: WebSocket handshake failed: {error}");
            return;
        }
    };

    let client_wire = match TcpWire::connect(nala_addr) {
        Ok(wire) => wire,
        Err(error) => {
            eprintln!("Error: could not connect to nala at {nala_addr}: {error}");
            return;
        }
    };
    let mut client = NalaClient::new(client_wire);

    let (synth, _chatterbox_supervisor) = match tts::stream_synthesizer() {
        Ok(built) => built,
        Err(error) => {
            eprintln!("Error: could not build a TTS backend: {error}");
            return;
        }
    };
    let mut narrator = TemplateNarrator::new();

    run_audio_session(
        &mut wire,
        transcriber.as_ref(),
        &mut client,
        &mut narrator,
        synth.as_ref(),
    );
}

/// Binds `addr` and serves one audio session per accepted connection on its
/// own thread, forwarding each session's turns to Nala at `nala_addr`.
/// Loads the Whisper model once and shares it (read-only, `Transcribe` only
/// needs `&self`) across every connection, since loading it is slow.
pub fn serve(addr: &str, nala_addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("Voice listening on ws://{addr}, forwarding to nala at ws://{nala_addr}");

    let transcriber = Arc::new(crate::bootstrap::build_transcriber());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let transcriber = Arc::clone(&transcriber);
                let nala_addr = nala_addr.to_string();
                thread::spawn(move || handle_connection(stream, transcriber, &nala_addr));
            }
            Err(error) => eprintln!("Warning: failed to accept a connection: {error}"),
        }
    }

    Ok(())
}
