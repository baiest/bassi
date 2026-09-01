//! Turns raw PCM16 samples into a loudness curve the overlay can pulse to.
//! Pure and platform-independent — no audio device, no window — so both
//! live mic capture and clip playback can share it.

/// Below this dBFS, treated as silence (amplitude 0.0) — matches
/// `apps/android`'s `Recorder.amplitudeFromRms` so a clip sounds/looks the
/// same whether it's played on the phone or here.
const SILENCE_FLOOR_DB: f32 = -50.0;

/// Root-mean-square loudness of `samples`, normalized against full-scale
/// `i16` and mapped through a dBFS curve into `[0.0, 1.0]` — linear PCM
/// amplitude reads as mostly-loud to the ear/eye, so the dB mapping is
/// what makes quieter sounds still visibly move the circle. Empty input is
/// silence (`0.0`), not a division-by-zero panic.
pub fn amplitude_from_samples(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_squares: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_squares / samples.len() as f64).sqrt();

    if rms <= 0.0 {
        return 0.0;
    }

    let db = 20.0 * (rms / i16::MAX as f64).log10();
    let normalized = (db as f32 - SILENCE_FLOOR_DB) / -SILENCE_FLOOR_DB;
    normalized.clamp(0.0, 1.0)
}

/// Splits `samples` into fixed-size windows (the last one possibly
/// shorter) and computes [`amplitude_from_samples`] for each — the
/// loudness-over-time curve a playback/recording loop walks in real time
/// to drive the overlay.
pub fn amplitude_windows(samples: &[i16], window_len: usize) -> Vec<f32> {
    if window_len == 0 {
        return Vec::new();
    }
    samples
        .chunks(window_len)
        .map(amplitude_from_samples)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_zero_amplitude() {
        assert_eq!(amplitude_from_samples(&[0; 100]), 0.0);
    }

    #[test]
    fn empty_input_is_zero_not_a_panic() {
        assert_eq!(amplitude_from_samples(&[]), 0.0);
    }

    #[test]
    fn full_scale_is_at_or_near_full_amplitude() {
        let samples = vec![i16::MAX; 100];

        assert!(amplitude_from_samples(&samples) > 0.95);
    }

    #[test]
    fn a_quieter_signal_has_lower_amplitude_than_a_louder_one() {
        let quiet = vec![1000i16; 100];
        let loud = vec![20000i16; 100];

        assert!(amplitude_from_samples(&quiet) < amplitude_from_samples(&loud));
    }

    #[test]
    fn amplitude_is_never_negative_or_above_one() {
        for value in [0i16, 1, 100, 1000, i16::MAX, i16::MIN] {
            let amplitude = amplitude_from_samples(&[value; 10]);
            assert!((0.0..=1.0).contains(&amplitude));
        }
    }

    #[test]
    fn amplitude_windows_splits_into_the_expected_number_of_chunks() {
        let samples = vec![1000i16; 250];

        let windows = amplitude_windows(&samples, 100);

        assert_eq!(windows.len(), 3); // 100, 100, 50
    }

    #[test]
    fn amplitude_windows_with_zero_window_len_returns_nothing() {
        assert!(amplitude_windows(&[1, 2, 3], 0).is_empty());
    }

    #[test]
    fn amplitude_windows_of_empty_samples_is_empty() {
        assert!(amplitude_windows(&[], 100).is_empty());
    }
}
