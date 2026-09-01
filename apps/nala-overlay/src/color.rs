use egui::Color32;

use crate::status::Status;

/// Maps a `Status` to the color the overlay's circle should be — pure and
/// separate from any drawing code, so it's testable without a window.
/// Colors are chosen for a distinct, at-a-glance read even in peripheral
/// vision: cool/neutral while idle, warm while acting, red for an error.
pub fn status_color(status: Status) -> Color32 {
    match status {
        Status::Idle => Color32::from_rgb(90, 90, 110),
        Status::Listening => Color32::from_rgb(64, 160, 255),
        Status::Sending => Color32::from_rgb(180, 120, 255),
        Status::Speaking => Color32::from_rgb(60, 220, 130),
        Status::Error => Color32::from_rgb(230, 60, 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Status; 5] = [
        Status::Idle,
        Status::Listening,
        Status::Sending,
        Status::Speaking,
        Status::Error,
    ];

    #[test]
    fn every_status_maps_to_a_distinct_color() {
        let colors: Vec<Color32> = ALL.iter().copied().map(status_color).collect();

        for (i, a) in colors.iter().enumerate() {
            for (j, b) in colors.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "statuses at {i} and {j} share a color");
                }
            }
        }
    }

    #[test]
    fn error_is_a_shade_of_red() {
        let color = status_color(Status::Error);

        assert!(color.r() > color.g() && color.r() > color.b());
    }

    #[test]
    fn status_color_is_deterministic() {
        assert_eq!(
            status_color(Status::Speaking),
            status_color(Status::Speaking)
        );
    }
}
