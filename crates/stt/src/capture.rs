use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// The sample rate whisper.cpp expects. Captured audio is resampled to
/// this before being returned, so callers never have to think about the
/// input device's native rate.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no input device found")]
    NoInputDevice,
    #[error("failed to build input stream: {0}")]
    StreamBuild(String),
    #[error("failed to start recording: {0}")]
    StreamStart(String),
}

/// Audio captured from the microphone, already mono at
/// [`WHISPER_SAMPLE_RATE`] — ready to hand straight to [`crate::Transcriber`]
/// with no further resampling.
pub struct RecordedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Records from the default input device until the caller presses Enter
/// (push-to-talk). Blocking: does not return until the recording stops.
pub fn record_until_enter() -> Result<RecordedAudio, CaptureError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(CaptureError::NoInputDevice)?;

    let config = device
        .default_input_config()
        .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let buffer_writer = Arc::clone(&buffer);

    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                let mut buffer = buffer_writer.lock().unwrap();
                // Downmix to mono by averaging channels, if the device
                // captures more than one.
                for frame in data.chunks(channels) {
                    let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
                    buffer.push(mono);
                }
            },
            |err| eprintln!("Audio input error: {err}"),
            None,
        )
        .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

    stream
        .play()
        .map_err(|e| CaptureError::StreamStart(e.to_string()))?;

    println!("Grabando... Enter para terminar.");
    let mut discard = String::new();
    std::io::stdin().read_line(&mut discard).ok();

    drop(stream);

    let raw_samples = buffer.lock().unwrap().clone();
    let resampled = resample_linear(&raw_samples, sample_rate, WHISPER_SAMPLE_RATE);

    Ok(RecordedAudio {
        samples: resampled,
        sample_rate: WHISPER_SAMPLE_RATE,
    })
}

/// Records from the default input device for as long as `should_continue`
/// keeps returning `true` (polled every ~10ms) — a press-and-hold trigger
/// rather than `record_until_enter`'s Enter-key one, for a caller with its
/// own UI gesture (e.g. holding down a button). `on_amplitude` is called
/// with each newly-captured chunk's samples as they arrive, so a caller
/// can drive a live level meter while recording — same buffer the final
/// `RecordedAudio` is built from, just observed incrementally.
pub fn record_while(
    mut should_continue: impl FnMut() -> bool,
    mut on_amplitude: impl FnMut(&[f32]),
) -> Result<RecordedAudio, CaptureError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(CaptureError::NoInputDevice)?;

    let config = device
        .default_input_config()
        .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let buffer_writer = Arc::clone(&buffer);

    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                let mut buffer = buffer_writer.lock().unwrap();
                for frame in data.chunks(channels) {
                    let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
                    buffer.push(mono);
                }
            },
            |err| eprintln!("Audio input error: {err}"),
            None,
        )
        .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

    stream
        .play()
        .map_err(|e| CaptureError::StreamStart(e.to_string()))?;

    let mut last_reported_len = 0;
    while should_continue() {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let buffer = buffer.lock().unwrap();
        if buffer.len() > last_reported_len {
            on_amplitude(&buffer[last_reported_len..]);
            last_reported_len = buffer.len();
        }
    }

    drop(stream);

    let raw_samples = buffer.lock().unwrap().clone();
    let resampled = resample_linear(&raw_samples, sample_rate, WHISPER_SAMPLE_RATE);

    Ok(RecordedAudio {
        samples: resampled,
        sample_rate: WHISPER_SAMPLE_RATE,
    })
}

/// Linear resampling — not studio quality, but whisper.cpp already
/// tolerates plenty of noise/artifacts, so it's more than good enough for
/// STT and keeps this crate free of a heavier resampling dependency.
fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;

    (0..out_len)
        .map(|i| {
            let src_index = i as f64 * ratio;
            let lower = src_index.floor() as usize;
            let upper = (lower + 1).min(samples.len() - 1);
            let frac = (src_index - lower as f64) as f32;
            samples[lower] * (1.0 - frac) + samples[upper] * frac
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_linear_is_a_no_op_when_rates_match() {
        let samples = vec![0.1, 0.2, 0.3];

        let resampled = resample_linear(&samples, 16_000, 16_000);

        assert_eq!(resampled, samples);
    }

    #[test]
    fn resample_linear_handles_an_empty_input() {
        let resampled = resample_linear(&[], 44_100, WHISPER_SAMPLE_RATE);

        assert!(resampled.is_empty());
    }

    #[test]
    fn resample_linear_halves_the_length_when_halving_the_rate() {
        let samples: Vec<f32> = (0..100).map(|i| i as f32).collect();

        let resampled = resample_linear(&samples, 32_000, 16_000);

        assert_eq!(resampled.len(), 50);
    }
}
