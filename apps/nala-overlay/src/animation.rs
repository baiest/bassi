use std::time::Duration;

use device_protocol::DeviceState;

/// A radius multiplier that oscillates over time, so the overlay's circle
/// visibly pulses instead of only swapping color — a color change alone is
/// easy to miss in peripheral vision, motion is not. Idle barely breathes;
/// every other state pulses wider and faster, so an active turn reads as
/// motion at a glance. Time-driven (not frame-count-driven) so it's
/// identical regardless of the overlay's actual frame rate.
pub fn pulse_scale(state: DeviceState, elapsed: Duration) -> f32 {
    let (amplitude, cycles_per_second) = match state {
        DeviceState::Idle => (0.03, 0.5),
        DeviceState::Error => (0.10, 3.0),
        DeviceState::Listening | DeviceState::Thinking | DeviceState::Executing => (0.12, 1.5),
        DeviceState::Speaking => (0.15, 2.5),
    };

    let phase = elapsed.as_secs_f32() * cycles_per_second * std::f32::consts::TAU;
    1.0 + amplitude * phase.sin()
}

/// The ring's leading angle (radians, growing over time) and its arc length
/// (radians), or `None` when this state shows no ring at all — `Idle`
/// (nothing is happening) and `Error` (a spinning ring reads as "working",
/// not "failed"). Angle and length are picked so the ring reads as visibly
/// rotating rather than a static partial circle.
pub fn ring_sweep(state: DeviceState, elapsed: Duration) -> Option<(f32, f32)> {
    let (revolutions_per_second, arc_length) = match state {
        DeviceState::Idle | DeviceState::Error => return None,
        DeviceState::Listening => (0.6, std::f32::consts::PI * 0.9),
        DeviceState::Thinking => (0.4, std::f32::consts::PI * 0.7),
        DeviceState::Executing => (0.8, std::f32::consts::PI * 0.6),
        DeviceState::Speaking => (1.0, std::f32::consts::PI * 1.1),
    };

    let angle = elapsed.as_secs_f32() * revolutions_per_second * std::f32::consts::TAU;
    Some((angle % std::f32::consts::TAU, arc_length))
}

/// `segments + 1` points along the arc from `start` through `start + sweep`
/// radians, as `(x, y)` offsets from the center at `radius` — pure geometry
/// so `overlay.rs` only has to map these straight into `egui::Pos2`.
pub fn arc_offsets(start: f32, sweep: f32, radius: f32, segments: usize) -> Vec<(f32, f32)> {
    (0..=segments)
        .map(|i| {
            let t = start + sweep * (i as f32 / segments as f32);
            (radius * t.cos(), radius * t.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_STATES: [DeviceState; 6] = [
        DeviceState::Idle,
        DeviceState::Listening,
        DeviceState::Thinking,
        DeviceState::Executing,
        DeviceState::Speaking,
        DeviceState::Error,
    ];

    #[test]
    fn at_zero_elapsed_every_state_starts_at_the_resting_scale() {
        for state in ALL_STATES {
            assert_eq!(pulse_scale(state, Duration::ZERO), 1.0);
        }
    }

    #[test]
    fn the_scale_oscillates_above_and_below_the_resting_value_over_one_cycle() {
        // Listening completes a cycle every 1/1.5s; sample a quarter and
        // three-quarters of the way through it.
        let quarter = Duration::from_secs_f32(1.0 / 1.5 / 4.0);
        let three_quarters = Duration::from_secs_f32(1.0 / 1.5 * 3.0 / 4.0);

        assert!(pulse_scale(DeviceState::Listening, quarter) > 1.0);
        assert!(pulse_scale(DeviceState::Listening, three_quarters) < 1.0);
    }

    #[test]
    fn an_active_state_pulses_wider_than_idle() {
        let t = Duration::from_millis(250);

        let idle_swing = (pulse_scale(DeviceState::Idle, t) - 1.0).abs();
        let speaking_swing = (pulse_scale(DeviceState::Speaking, t) - 1.0).abs();

        assert!(speaking_swing >= idle_swing);
    }

    #[test]
    fn the_scale_is_deterministic_for_the_same_state_and_elapsed_time() {
        let a = pulse_scale(DeviceState::Thinking, Duration::from_millis(837));
        let b = pulse_scale(DeviceState::Thinking, Duration::from_millis(837));

        assert_eq!(a, b);
    }

    #[test]
    fn idle_and_error_show_no_ring() {
        assert_eq!(ring_sweep(DeviceState::Idle, Duration::from_secs(1)), None);
        assert_eq!(ring_sweep(DeviceState::Error, Duration::from_secs(1)), None);
    }

    #[test]
    fn every_other_state_shows_a_ring() {
        for state in [
            DeviceState::Listening,
            DeviceState::Thinking,
            DeviceState::Executing,
            DeviceState::Speaking,
        ] {
            assert!(ring_sweep(state, Duration::from_secs(1)).is_some());
        }
    }

    #[test]
    fn the_ring_rotates_as_time_advances() {
        let (angle_at_zero, _) = ring_sweep(DeviceState::Listening, Duration::ZERO).unwrap();
        let (angle_later, _) =
            ring_sweep(DeviceState::Listening, Duration::from_millis(300)).unwrap();

        assert_ne!(angle_at_zero, angle_later);
    }

    #[test]
    fn arc_offsets_returns_segments_plus_one_points_all_at_radius() {
        let points = arc_offsets(0.0, std::f32::consts::PI, 10.0, 8);

        assert_eq!(points.len(), 9);
        for (x, y) in points {
            let distance = (x * x + y * y).sqrt();
            assert!(
                (distance - 10.0).abs() < 0.001,
                "point ({x}, {y}) is not at radius 10.0"
            );
        }
    }

    #[test]
    fn arc_offsets_starts_and_ends_at_the_expected_angles() {
        let points = arc_offsets(0.0, std::f32::consts::FRAC_PI_2, 1.0, 4);

        let (start_x, start_y) = points.first().copied().unwrap();
        let (end_x, end_y) = points.last().copied().unwrap();

        assert!((start_x - 1.0).abs() < 0.001 && start_y.abs() < 0.001);
        assert!(start_x.hypot(start_y) - 1.0 < 0.001);
        assert!(end_x.abs() < 0.001 && (end_y - 1.0).abs() < 0.001);
    }
}
