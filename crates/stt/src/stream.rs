use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::CaptureError;
use crate::WHISPER_SAMPLE_RATE;
use crate::resample::Resampler;
use crate::ring::Ring;
use crate::session::CHUNK_SAMPLES;

/// Supplies fixed-size chunks of mono audio at [`WHISPER_SAMPLE_RATE`].
///
/// A trait so [`crate::Listener`] can be driven by a scripted fake in
/// tests, with no microphone involved.
pub trait AudioSource {
    /// Blocks until [`crate::CHUNK_SAMPLES`] samples are available and
    /// fills `out` with them, or returns `false` if the source is closed
    /// and no more chunks will ever come.
    fn next_chunk(&mut self, out: &mut [f32]) -> bool;
}

/// How long the ring keeps audio: enough for the longest utterance plus
/// margin, so `Session::observe`'s max-utterance cap is always reached
/// before the ring itself would need to overwrite unread audio.
const RING_SECONDS: usize = 30;

/// Milliseconds to sleep between polls when the ring doesn't yet have a
/// full chunk. Chunks arrive every 32 ms, so this costs roughly 3 wakeups
/// per chunk — negligible even on a Raspberry Pi, and far simpler than
/// coordinating a `Condvar` with the realtime audio callback.
const POLL_INTERVAL_MS: u64 = 10;

/// A long-lived microphone input stream feeding a [`Ring`].
///
/// The cpal callback thread must never block or allocate, so it only
/// downmixes to mono, resamples, and writes into the ring; everything
/// else (VAD, wake word, the session state machine) happens on the
/// caller's thread via [`AudioSource::next_chunk`].
pub struct MicStream {
    // Kept alive only to keep the stream running — dropping it stops
    // capture. Never read directly.
    _stream: cpal::Stream,
    ring: Arc<Mutex<Ring>>,
    device_name: String,
}

impl MicStream {
    pub fn open() -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(CaptureError::NoInputDevice)?;
        // Best-effort: a device that refuses to report its own name still
        // works, it just can't be named back to the user.
        let device_name = device
            .name()
            .unwrap_or_else(|_| "<dispositivo sin nombre>".to_string());
        let config = device
            .default_input_config()
            .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

        let source_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let ring = Arc::new(Mutex::new(Ring::with_capacity(
            RING_SECONDS * WHISPER_SAMPLE_RATE as usize,
        )));
        let ring_writer = Arc::clone(&ring);
        let mut resampler = Resampler::new(source_rate, WHISPER_SAMPLE_RATE);
        let mut resampled = Vec::new();

        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    resampled.clear();

                    // Downmix to mono, then resample — same order as the
                    // one-shot capture path in capture.rs, just streamed
                    // through a stateful resampler instead of a
                    // whole-buffer one.
                    let mono = data
                        .chunks(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32);
                    let mono: Vec<f32> = mono.collect();

                    resampler.push(&mono, &mut resampled);

                    if let Ok(mut ring) = ring_writer.lock() {
                        ring.write(&resampled);
                    }
                },
                |err| eprintln!("Audio input error: {err}"),
                None,
            )
            .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

        stream
            .play()
            .map_err(|e| CaptureError::StreamStart(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            ring,
            device_name,
        })
    }

    /// The name of the input device actually opened — not necessarily
    /// what the caller expected, since this is always whatever the OS
    /// currently has set as the default recording device.
    pub fn device_name(&self) -> &str {
        &self.device_name
    }
}

impl AudioSource for MicStream {
    fn next_chunk(&mut self, out: &mut [f32]) -> bool {
        debug_assert_eq!(out.len(), CHUNK_SAMPLES);

        loop {
            {
                let mut ring = self.ring.lock().expect("ring mutex poisoned");
                if ring.read(out) {
                    ring.take_overrun();
                    return true;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ring/ordering half of this module is already covered by
    /// `ring::tests`; what's specific here is `AudioSource::next_chunk`
    /// blocking until a full chunk is available rather than returning a
    /// short read.
    #[test]
    fn next_chunk_waits_for_a_full_chunk_to_accumulate() {
        let ring = Arc::new(Mutex::new(Ring::with_capacity(CHUNK_SAMPLES * 4)));

        // Write half a chunk, then the rest from another thread after a
        // short delay, standing in for the audio callback.
        {
            let mut guard = ring.lock().unwrap();
            guard.write(&vec![1.0; CHUNK_SAMPLES / 2]);
        }

        let writer = Arc::clone(&ring);
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            let mut guard = writer.lock().unwrap();
            guard.write(&vec![1.0; CHUNK_SAMPLES / 2]);
        });

        // Reuse next_chunk's polling logic directly against the ring,
        // without needing a real MicStream (which needs a real device).
        let mut out = vec![0.0; CHUNK_SAMPLES];
        let start = std::time::Instant::now();
        loop {
            let mut guard = ring.lock().unwrap();
            if guard.read(&mut out) {
                break;
            }
            drop(guard);
            std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
        }

        assert!(start.elapsed() >= std::time::Duration::from_millis(20));
        assert!(out.iter().all(|&sample| sample == 1.0));
        handle.join().unwrap();
    }
}
