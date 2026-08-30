/// Linear resampler that keeps its position across calls.
///
/// The free `resample_linear` in `capture.rs` restarts at source index 0
/// every time, which is fine for a whole recording but wrong for a live
/// stream: applied per audio callback it would drop or duplicate a
/// fraction of a sample at every buffer boundary, injecting a
/// discontinuity ~100 times a second. Whisper might tolerate that; the VAD
/// and wake-word feature extractors would not.
///
/// Carrying the fractional read position and the last sample of the
/// previous block across calls is what makes block-wise resampling
/// equivalent to resampling the whole signal at once.
pub struct Resampler {
    /// Input samples consumed per output sample.
    ratio: f64,
    /// Fractional read position within the current input block.
    position: f64,
    /// Final sample of the previous block, so interpolation can span the
    /// boundary instead of restarting.
    previous: Option<f32>,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        assert!(
            from_rate > 0 && to_rate > 0,
            "sample rates must be non-zero"
        );

        Self {
            ratio: from_rate as f64 / to_rate as f64,
            position: 0.0,
            previous: None,
        }
    }

    /// Whether input and output rates match, in which case `push` is a
    /// straight copy.
    pub fn is_identity(&self) -> bool {
        self.ratio == 1.0
    }

    /// Resamples `input`, appending to `out`.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if input.is_empty() {
            return;
        }

        if self.is_identity() {
            out.extend_from_slice(input);
            return;
        }

        // Index -1 refers to the previous block's last sample, so the very
        // first output of this block can interpolate across the boundary.
        let previous = self.previous;
        let sample_at = |index: isize| -> f32 {
            if index < 0 {
                previous.unwrap_or(input[0])
            } else {
                input[index as usize]
            }
        };

        let last = input.len() as isize - 1;

        // `position` is relative to the start of this block, and may be
        // negative when the last output landed between blocks.
        let mut position = self.position;
        // Stop as soon as interpolating would need the sample *after* this
        // block. Clamping to the block's final sample instead would be the
        // very boundary discontinuity this type exists to avoid; that
        // output is emitted once the next block supplies its right-hand
        // neighbour.
        while (position.floor() as isize) < last {
            let lower = position.floor();
            let fraction = (position - lower) as f32;
            let lower = lower as isize;

            let a = sample_at(lower);
            let b = sample_at(lower + 1);
            out.push(a * (1.0 - fraction) + b * fraction);

            position += self.ratio;
        }

        // Carry the leftover past the end of this block into the next one,
        // where this block's final sample becomes index -1.
        self.position = position - input.len() as f64;
        self.previous = Some(input[input.len() - 1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resamples `input` in `block` sized pieces, the way the audio
    /// callback would deliver it.
    fn push_in_blocks(resampler: &mut Resampler, input: &[f32], block: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for chunk in input.chunks(block) {
            resampler.push(chunk, &mut out);
        }
        out
    }

    #[test]
    fn matching_rates_pass_samples_through_unchanged() {
        let mut resampler = Resampler::new(16_000, 16_000);
        let input: Vec<f32> = (0..10).map(|i| i as f32).collect();

        let mut out = Vec::new();
        resampler.push(&input, &mut out);

        assert_eq!(out, input);
    }

    #[test]
    fn halving_the_rate_halves_the_sample_count() {
        let mut resampler = Resampler::new(32_000, 16_000);
        let input: Vec<f32> = (0..100).map(|i| i as f32).collect();

        let mut out = Vec::new();
        resampler.push(&input, &mut out);

        assert_eq!(out.len(), 50);
    }

    #[test]
    fn block_wise_resampling_matches_whole_buffer_resampling() {
        let input: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin()).collect();

        let mut whole = Resampler::new(44_100, 16_000);
        let mut expected = Vec::new();
        whole.push(&input, &mut expected);

        let mut blocked = Resampler::new(44_100, 16_000);
        let actual = push_in_blocks(&mut blocked, &input, 128);

        assert!(
            actual.len().abs_diff(expected.len()) <= 1,
            "expected {} samples, got {}",
            expected.len(),
            actual.len()
        );

        // Interpolation across a block boundary uses the real previous
        // sample, so values match to within floating-point noise.
        let compared = actual.len().min(expected.len());
        for i in 0..compared {
            assert!(
                (actual[i] - expected[i]).abs() < 1e-5,
                "sample {i} diverged: {} vs {}",
                actual[i],
                expected[i]
            );
        }
    }

    #[test]
    fn a_constant_signal_stays_constant_across_block_boundaries() {
        // The clearest test for boundary clicks: any discontinuity in the
        // carried position or previous sample shows up as a value that
        // isn't the constant.
        let input = vec![0.5_f32; 1000];

        let mut resampler = Resampler::new(44_100, 16_000);
        let out = push_in_blocks(&mut resampler, &input, 64);

        assert!(!out.is_empty());
        for (i, sample) in out.iter().enumerate() {
            assert!(
                (sample - 0.5).abs() < 1e-6,
                "sample {i} was {sample}, expected a constant 0.5"
            );
        }
    }

    #[test]
    fn an_empty_block_produces_no_output_and_keeps_state() {
        let mut resampler = Resampler::new(32_000, 16_000);

        let mut out = Vec::new();
        resampler.push(&[], &mut out);
        assert!(out.is_empty());

        resampler.push(&[1.0, 2.0, 3.0, 4.0], &mut out);
        assert_eq!(out.len(), 2);
    }
}
