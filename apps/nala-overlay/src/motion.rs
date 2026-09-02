//! Pure time-based helpers for animating the overlay: framerate-independent
//! smoothing (so the core doesn't jump between amplitude windows the way a
//! raw-value read does) and a slow idle "breathing" pulse. No egui, no
//! clock — the caller supplies `dt`/`elapsed`, which keeps this testable
//! without a window, same as `amplitude.rs` and `color.rs`.

use std::f32::consts::TAU;

/// How long, in seconds, a "breathe" cycle takes at rest.
const BREATHE_PERIOD: f32 = 3.2;

/// Exponentially smooths `current` toward `target` over `dt` seconds, with
/// `half_life` controlling how quickly it catches up (smaller = snappier).
/// Framerate-independent: applying it for `dt` gives (approximately) the
/// same result as applying it twice for `dt / 2.0`, so the animation looks
/// the same whether the overlay is running at 30 or 144 FPS.
pub fn smooth(current: f32, target: f32, dt: f32, half_life: f32) -> f32 {
    if half_life <= 0.0 {
        return target;
    }
    let decay = (-dt / half_life).exp();
    target + (current - target) * decay
}

/// A slow, normalized-to-`[0.0, 1.0]` pulse for the idle state, so the core
/// visibly "breathes" instead of sitting frozen when nothing is happening.
pub fn breathe(elapsed: f32) -> f32 {
    let phase = (elapsed / BREATHE_PERIOD) * TAU;
    (phase.sin() + 1.0) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_moves_toward_the_target() {
        let result = smooth(0.0, 1.0, 0.1, 0.2);

        assert!(result > 0.0 && result < 1.0);
    }

    #[test]
    fn smooth_with_zero_dt_does_not_change() {
        assert_eq!(smooth(0.3, 1.0, 0.0, 0.2), 0.3);
    }

    #[test]
    fn smooth_never_overshoots_the_target() {
        let result = smooth(0.0, 1.0, 100.0, 0.2);

        assert!(result <= 1.0);
        assert!((result - 1.0).abs() < 1e-3);
    }

    #[test]
    fn smooth_converges_from_above_too() {
        let result = smooth(1.0, 0.0, 100.0, 0.2);

        assert!(result >= 0.0);
        assert!(result.abs() < 1e-3);
    }

    #[test]
    fn smooth_is_framerate_independent() {
        let half_life = 0.2;
        let one_step = smooth(0.0, 1.0, 0.1, half_life);

        let mut two_steps = 0.0;
        two_steps = smooth(two_steps, 1.0, 0.05, half_life);
        two_steps = smooth(two_steps, 1.0, 0.05, half_life);

        assert!((one_step - two_steps).abs() < 1e-4);
    }

    #[test]
    fn breathe_stays_in_unit_range() {
        for i in 0..100 {
            let elapsed = i as f32 * 0.1;
            let value = breathe(elapsed);
            assert!((0.0..=1.0).contains(&value));
        }
    }

    #[test]
    fn breathe_is_periodic() {
        let a = breathe(0.5);
        let b = breathe(0.5 + BREATHE_PERIOD);

        assert!((a - b).abs() < 1e-3);
    }
}
