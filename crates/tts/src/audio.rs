use std::sync::mpsc;
use std::thread;

use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamHandle, Sink};

use crate::pcm::{PcmStream, PlayPcmStream};
use crate::speech::SpeechError;

/// Plays streamed PCM audio through the system's default output device. The
/// only adapter that touches audio hardware; kept separate from TTS
/// synthesis so a missing/broken output device never looks like a
/// Chatterbox failure and vice versa.
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
    stream: PcmStream,
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
                let result = play_stream_on_device(&handle, request.stream);
                let _ = request.reply.send(result);
            }
        });

        ready_rx.recv().map_err(|_| {
            SpeechError::Playback("audio output thread failed to start".to_string())
        })??;

        Ok(Self { sender })
    }
}

impl PlayPcmStream for RodioPlayer {
    fn play_stream(&self, stream: PcmStream) -> Result<(), SpeechError> {
        let (reply_tx, reply_rx) = mpsc::channel();

        self.sender
            .send(PlayRequest {
                stream,
                reply: reply_tx,
            })
            .map_err(|_| SpeechError::Playback("audio output thread is gone".to_string()))?;

        reply_rx
            .recv()
            .map_err(|_| SpeechError::Playback("audio output thread did not respond".to_string()))?
    }
}

/// Feeds chunks into a single `Sink` as they arrive, so playback starts on
/// the first chunk and continues gaplessly as later ones are appended -
/// rodio queues appended sources back-to-back and only falls silent if the
/// sink actually runs dry. Blocks (on this dedicated output thread, not the
/// caller) until every chunk has both arrived and finished playing.
fn play_stream_on_device(
    handle: &OutputStreamHandle,
    stream: PcmStream,
) -> Result<(), SpeechError> {
    let sink = Sink::try_new(handle)
        .map_err(|error| SpeechError::Playback(format!("could not open playback sink: {error}")))?;

    for chunk in stream.chunks {
        let samples = chunk?;
        sink.append(SamplesBuffer::new(
            stream.channels,
            stream.sample_rate,
            samples,
        ));
    }

    sink.sleep_until_end();

    Ok(())
}
