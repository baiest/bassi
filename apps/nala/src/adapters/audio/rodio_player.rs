use std::io::Cursor;
use std::sync::mpsc;
use std::thread;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

use crate::adapters::speech::chatterbox::PlayAudio;
use crate::ports::speech::SpeechError;

/// Plays WAV audio held entirely in memory, through the system's default
/// output device. The only adapter that touches audio hardware; kept
/// separate from TTS synthesis so a missing/broken output device never
/// looks like a Chatterbox failure and vice versa.
///
/// `cpal::Stream` (which `rodio::OutputStream` wraps) is deliberately not
/// `Send` on any platform, but `AsyncSpeech`'s worker thread requires its
/// backend to be `Send` so it can be moved into that thread. `RodioPlayer`
/// resolves this by owning a dedicated thread that opens the output device
/// and never leaves it: the struct itself holds only a channel, which is
/// `Send` regardless of what runs on the other end.
pub struct RodioPlayer {
    sender: mpsc::Sender<PlayRequest>,
}

struct PlayRequest {
    audio: Vec<u8>,
    reply: mpsc::Sender<Result<(), SpeechError>>,
}

impl RodioPlayer {
    pub fn new() -> Result<Self, SpeechError> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), SpeechError>>();
        let (sender, receiver) = mpsc::channel::<PlayRequest>();

        thread::spawn(move || {
            let (_stream, handle) = match OutputStream::try_default() {
                Ok(opened) => {
                    let _ = ready_tx.send(Ok(()));
                    opened
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(SpeechError::Playback(format!(
                        "no audio output device: {error}"
                    ))));
                    return;
                }
            };

            for request in receiver {
                let result = play_on_device(&handle, &request.audio);
                let _ = request.reply.send(result);
            }
        });

        ready_rx.recv().map_err(|_| {
            SpeechError::Playback("audio output thread failed to start".to_string())
        })??;

        Ok(Self { sender })
    }
}

impl PlayAudio for RodioPlayer {
    fn play(&self, audio: &[u8]) -> Result<(), SpeechError> {
        let (reply_tx, reply_rx) = mpsc::channel();

        self.sender
            .send(PlayRequest {
                audio: audio.to_vec(),
                reply: reply_tx,
            })
            .map_err(|_| SpeechError::Playback("audio output thread is gone".to_string()))?;

        reply_rx
            .recv()
            .map_err(|_| SpeechError::Playback("audio output thread did not respond".to_string()))?
    }
}

fn play_on_device(handle: &OutputStreamHandle, audio: &[u8]) -> Result<(), SpeechError> {
    let source = Decoder::new(Cursor::new(audio.to_vec()))
        .map_err(|error| SpeechError::Playback(format!("could not decode audio: {error}")))?;

    let sink = Sink::try_new(handle)
        .map_err(|error| SpeechError::Playback(format!("could not open playback sink: {error}")))?;

    sink.append(source);
    sink.sleep_until_end();

    Ok(())
}
