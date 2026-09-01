use device_protocol::DeviceState;
use egui::Color32;

/// Maps a `DeviceState` to the color the overlay's circle should be —
/// pure and separate from any drawing code, so it's testable without a
/// window. Colors are chosen for a distinct, at-a-glance read even in
/// peripheral vision: cool/neutral while idle, warm while acting, red for
/// an error.
pub fn state_color(state: DeviceState) -> Color32 {
    match state {
        DeviceState::Idle => Color32::from_rgb(90, 90, 110),
        DeviceState::Listening => Color32::from_rgb(64, 160, 255),
        DeviceState::Thinking => Color32::from_rgb(180, 120, 255),
        DeviceState::Executing => Color32::from_rgb(255, 170, 40),
        DeviceState::Speaking => Color32::from_rgb(60, 220, 130),
        DeviceState::Error => Color32::from_rgb(230, 60, 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_maps_to_a_distinct_color() {
        let states = [
            DeviceState::Idle,
            DeviceState::Listening,
            DeviceState::Thinking,
            DeviceState::Executing,
            DeviceState::Speaking,
            DeviceState::Error,
        ];

        let colors: Vec<Color32> = states.iter().copied().map(state_color).collect();

        for (i, a) in colors.iter().enumerate() {
            for (j, b) in colors.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "states at {i} and {j} share a color");
                }
            }
        }
    }

    #[test]
    fn error_is_a_shade_of_red() {
        let color = state_color(DeviceState::Error);

        assert!(color.r() > color.g() && color.r() > color.b());
    }

    #[test]
    fn state_color_is_deterministic() {
        assert_eq!(
            state_color(DeviceState::Executing),
            state_color(DeviceState::Executing)
        );
    }
}
