//! Plays received clips one at a time, in order, and drives a shared
//! amplitude value in lockstep so the overlay pulses to what's actually
//! playing. Owns a real audio output device — not unit-tested, same as
//! `adapters/process/windows.rs` elsewhere in this workspace; the pure
//! decode/amplitude steps it calls into are covered in `clip.rs` and
//! `amplitude.rs`.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rodio::{OutputStream, Sink};

use crate::amplitude::amplitude_windows;
use crate::clip::decode_clip;
use crate::status::Status;

/// How many samples each amplitude reading covers — smaller means a more
/// responsive pulse, larger means fewer wake-ups. ~50ms at typical TTS
/// sample rates (16-24 kHz) is smooth without being chatty.
const WINDOW_SAMPLES: usize = 1024;

/// Spawns the playback worker and returns a handle to enqueue clips onto
/// it. `amplitude` is updated continuously while a clip plays (0.0 the
/// rest of the time); `status` is set to `Speaking` for the duration of
/// each clip and back to `Idle` once it's done — the overlay's UI thread
/// just reads both each frame.
pub fn spawn(amplitude: Arc<Mutex<f32>>, status: Arc<Mutex<Status>>) -> ClipPlayer {
    let (sender, receiver) = mpsc::channel::<Vec<u8>>();

    thread::spawn(move || {
        let Ok((_stream, handle)) = OutputStream::try_default() else {
            eprintln!("Warning: no audio output device — clips will be silently dropped.");
            return;
        };

        for clip in receiver {
            *status.lock().unwrap() = Status::Speaking;
            play_one(&handle, &clip, &amplitude);
            *status.lock().unwrap() = Status::Idle;
        }
    });

    ClipPlayer { sender }
}

fn play_one(handle: &rodio::OutputStreamHandle, clip: &[u8], amplitude: &Arc<Mutex<f32>>) {
    let decoded = match decode_clip(clip) {
        Ok(decoded) => decoded,
        Err(error) => {
            eprintln!("Warning: could not decode a clip, skipping it: {error}");
            return;
        }
    };

    let Ok(sink) = Sink::try_new(handle) else {
        eprintln!("Warning: could not open a playback sink, skipping a clip.");
        return;
    };
    let Ok(source) = rodio::Decoder::new(std::io::Cursor::new(clip.to_vec())) else {
        return;
    };
    sink.append(source);

    let windows = amplitude_windows(&decoded.samples, WINDOW_SAMPLES * decoded.channels as usize);
    let window_duration =
        Duration::from_secs_f64(WINDOW_SAMPLES as f64 / decoded.sample_rate.max(1) as f64);
    for level in windows {
        *amplitude.lock().unwrap() = level;
        thread::sleep(window_duration);
    }

    // Catches any remainder past the last full window (rounding, or the
    // sink taking slightly longer than the precomputed curve estimated).
    sink.sleep_until_end();
    *amplitude.lock().unwrap() = 0.0;
}

pub struct ClipPlayer {
    sender: mpsc::Sender<Vec<u8>>,
}

impl ClipPlayer {
    /// Queues `clip` for playback — returns immediately, the worker thread
    /// plays clips strictly in the order they were enqueued.
    pub fn enqueue(&self, clip: Vec<u8>) {
        // The worker thread is gone only if the process is shutting down;
        // nothing useful to do with that here.
        let _ = self.sender.send(clip);
    }
}
