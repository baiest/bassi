use std::sync::atomic::{AtomicBool, Ordering};

/// A fixed-capacity, overwrite-oldest ring buffer of audio samples.
///
/// Sized once at construction and never reallocated: the producer is the
/// cpal audio callback, which must not allocate. When the consumer falls
/// behind and the buffer wraps, the oldest samples are dropped and
/// [`Ring::take_overrun`] reports it, rather than growing without bound.
pub struct Ring {
    samples: Box<[f32]>,
    /// Where the next write goes.
    head: usize,
    /// How many samples are readable, capped at `samples.len()`.
    len: usize,
    overrun: AtomicBool,
}

impl Ring {
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "ring capacity must be non-zero");

        Self {
            samples: vec![0.0; capacity].into_boxed_slice(),
            head: 0,
            len: 0,
            overrun: AtomicBool::new(false),
        }
    }

    pub fn capacity(&self) -> usize {
        self.samples.len()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Appends `input`, overwriting the oldest samples if it doesn't fit.
    /// Never allocates.
    pub fn write(&mut self, input: &[f32]) {
        let capacity = self.capacity();

        // Only the last `capacity` samples can survive, so skip anything
        // that a single call would immediately overwrite itself.
        let input = if input.len() > capacity {
            self.overrun.store(true, Ordering::Relaxed);
            &input[input.len() - capacity..]
        } else {
            input
        };

        if self.len + input.len() > capacity {
            self.overrun.store(true, Ordering::Relaxed);
        }

        for &sample in input {
            self.samples[self.head] = sample;
            self.head = (self.head + 1) % capacity;
        }

        self.len = (self.len + input.len()).min(capacity);
    }

    /// Removes the oldest `out.len()` samples into `out`, returning false
    /// (and leaving `out` untouched) if that many aren't available yet.
    pub fn read(&mut self, out: &mut [f32]) -> bool {
        if out.len() > self.len {
            return false;
        }

        let oldest = (self.head + self.capacity() - self.len) % self.capacity();
        self.copy_from(oldest, out);
        self.len -= out.len();

        true
    }

    /// Copies the most recent `out.len()` samples into `out` **without
    /// consuming them**, returning false if that many aren't available.
    ///
    /// This is what primes the wake-word detector: when the VAD latches
    /// onto speech, the phrase's onset is already a few chunks in the past,
    /// and replaying it is what keeps the detector's feature extraction
    /// aligned.
    pub fn peek_last(&self, out: &mut [f32]) -> bool {
        if out.len() > self.len {
            return false;
        }

        let start = (self.head + self.capacity() - out.len()) % self.capacity();
        self.copy_from(start, out);

        true
    }

    /// Fills `out` from `start`, wrapping around the end of the backing
    /// slice. Callers are responsible for having checked availability.
    fn copy_from(&self, start: usize, out: &mut [f32]) {
        for (offset, slot) in out.iter_mut().enumerate() {
            *slot = self.samples[(start + offset) % self.capacity()];
        }
    }

    /// Drops every buffered sample. Used when re-arming the microphone
    /// after Nala speaks, so her own voice can't be read back as input.
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    /// Returns whether samples were dropped since the last call, clearing
    /// the flag. The consumer logs this; it means the reader isn't keeping
    /// up with the audio callback.
    pub fn take_overrun(&self) -> bool {
        self.overrun.swap(false, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_round_trip_in_order() {
        let mut ring = Ring::with_capacity(8);
        ring.write(&[1.0, 2.0, 3.0, 4.0]);

        let mut out = [0.0; 4];
        assert!(ring.read(&mut out));

        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
        assert!(ring.is_empty());
    }

    #[test]
    fn read_reports_false_when_not_enough_samples_are_buffered() {
        let mut ring = Ring::with_capacity(8);
        ring.write(&[1.0, 2.0]);

        let mut out = [0.0; 4];
        assert!(!ring.read(&mut out));
        assert_eq!(ring.len(), 2, "a failed read must not consume anything");
    }

    #[test]
    fn overflow_overwrites_the_oldest_and_raises_the_overrun_flag() {
        let mut ring = Ring::with_capacity(4);
        ring.write(&[1.0, 2.0, 3.0, 4.0]);
        ring.write(&[5.0, 6.0]);

        assert!(ring.take_overrun());
        assert!(
            !ring.take_overrun(),
            "reading the flag should also clear it"
        );

        let mut out = [0.0; 4];
        assert!(ring.read(&mut out));
        assert_eq!(out, [3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn a_write_larger_than_capacity_keeps_only_the_newest_samples() {
        let mut ring = Ring::with_capacity(3);
        ring.write(&[1.0, 2.0, 3.0, 4.0, 5.0]);

        assert!(ring.take_overrun());

        let mut out = [0.0; 3];
        assert!(ring.read(&mut out));
        assert_eq!(out, [3.0, 4.0, 5.0]);
    }

    #[test]
    fn peek_last_returns_the_newest_samples_without_consuming_them() {
        let mut ring = Ring::with_capacity(8);
        ring.write(&[1.0, 2.0, 3.0, 4.0, 5.0]);

        let mut out = [0.0; 3];
        assert!(ring.peek_last(&mut out));

        assert_eq!(out, [3.0, 4.0, 5.0]);
        assert_eq!(ring.len(), 5, "peeking must not consume");
    }

    #[test]
    fn peek_last_reports_false_when_asking_for_more_than_is_buffered() {
        let ring = Ring::with_capacity(8);

        let mut out = [0.0; 4];
        assert!(!ring.peek_last(&mut out));
    }

    #[test]
    fn peek_last_stays_in_order_across_a_wrap() {
        let mut ring = Ring::with_capacity(4);
        ring.write(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

        let mut out = [0.0; 3];
        assert!(ring.peek_last(&mut out));

        assert_eq!(out, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn clear_drops_everything() {
        let mut ring = Ring::with_capacity(4);
        ring.write(&[1.0, 2.0, 3.0]);

        ring.clear();

        assert!(ring.is_empty());
        let mut out = [0.0; 1];
        assert!(!ring.read(&mut out));
    }

    #[test]
    fn reads_stay_in_order_across_a_wrap() {
        let mut ring = Ring::with_capacity(4);
        ring.write(&[1.0, 2.0, 3.0]);

        let mut out = [0.0; 2];
        assert!(ring.read(&mut out));
        assert_eq!(out, [1.0, 2.0]);

        // Writing now wraps past the end of the backing slice.
        ring.write(&[4.0, 5.0]);

        let mut out = [0.0; 3];
        assert!(ring.read(&mut out));
        assert_eq!(out, [3.0, 4.0, 5.0]);
    }
}
