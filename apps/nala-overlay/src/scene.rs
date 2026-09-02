//! Pure 3D geometry for the overlay's "Jarvis" core: a point cloud spread
//! over a sphere plus a few tilted orbital rings, rotated and projected to
//! 2D with simple perspective. No egui types here — just numbers — so the
//! projection math is testable without a window, the same way `color.rs`
//! keeps its palette pure and separate from drawing.

use std::f32::consts::PI;

/// How many points make up the sphere's point cloud.
pub const SPHERE_POINTS: usize = 90;

/// How many points make up each orbital ring.
pub const RING_POINTS: usize = 48;

/// Tilt (radians, rotation around X) of each orbital ring, so they read as
/// distinct rings rather than one flat circle.
pub const RING_TILTS: [f32; 2] = [0.35, -0.55];

/// Distance from the camera to the projection plane, in units of the
/// scene's radius (which is always 1.0 before projection). Must stay
/// greater than 1.0 so no point (at most 1.0 away from the origin) can ever
/// reach the camera and divide by zero.
pub const PERSPECTIVE: f32 = 2.6;

/// A point in the scene's local 3D space, before rotation/projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[cfg(test)]
    fn norm(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

/// A `Point3` after rotation and perspective projection, ready to paint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projected {
    /// Position in the projection plane, in units of the caller's radius.
    pub pos: (f32, f32),
    /// Original (rotated) depth — more positive is closer to the camera.
    /// Used only for depth sorting; painting reads `scale` instead.
    pub depth: f32,
    /// Perspective scale factor: > 1.0 for points closer than the origin,
    /// < 1.0 for points farther away. Drives both point radius and alpha.
    pub scale: f32,
}

/// Spreads `count` points evenly over the unit sphere using the Fibonacci
/// (golden angle) lattice — deterministic and free of clustering at the
/// poles, unlike a naive latitude/longitude grid. `count == 0` returns an
/// empty vector rather than panicking.
pub fn sphere_points(count: usize) -> Vec<Point3> {
    if count == 0 {
        return Vec::new();
    }

    let golden_angle = PI * (3.0 - 5.0_f32.sqrt());
    (0..count)
        .map(|i| {
            let i = i as f32;
            let n = count as f32;
            // y sweeps linearly from +1 to -1 across all points.
            let y = 1.0 - (i / (n - 1.0).max(1.0)) * 2.0;
            let radius_at_y = (1.0 - y * y).max(0.0).sqrt();
            let theta = golden_angle * i;
            Point3::new(theta.cos() * radius_at_y, y, theta.sin() * radius_at_y)
        })
        .collect()
}

/// Builds a unit circle in the XZ plane, then tilts it by `tilt` radians
/// around the X axis — one orbital ring. `count == 0` returns an empty
/// vector rather than panicking.
pub fn ring_points(count: usize, tilt: f32) -> Vec<Point3> {
    if count == 0 {
        return Vec::new();
    }

    (0..count)
        .map(|i| {
            let angle = 2.0 * PI * (i as f32) / (count as f32);
            let (x, z) = (angle.cos(), angle.sin());
            // Rotate (x, 0, z) around X by `tilt`.
            let y = -z * tilt.sin();
            let z = z * tilt.cos();
            Point3::new(x, y, z)
        })
        .collect()
}

/// Rotates `p` around the Y axis by `yaw`, then around the X axis by
/// `pitch`. Order matters for how the scene reads but not for the
/// invariants tested below (rotation preserves distance from the origin).
pub fn rotate(p: Point3, yaw: f32, pitch: f32) -> Point3 {
    // Yaw: rotate around Y.
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let x1 = p.x * cos_yaw + p.z * sin_yaw;
    let z1 = -p.x * sin_yaw + p.z * cos_yaw;
    let y1 = p.y;

    // Pitch: rotate around X.
    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    let y2 = y1 * cos_pitch - z1 * sin_pitch;
    let z2 = y1 * sin_pitch + z1 * cos_pitch;

    Point3::new(x1, y2, z2)
}

/// Projects `p` (assumed already rotated) onto a 2D plane with simple
/// perspective, scaling the result by `radius` (the on-screen radius that
/// corresponds to the scene's unit sphere). `perspective` must be > 1.0 —
/// see [`PERSPECTIVE`]'s doc comment for why that keeps this division safe.
pub fn project(p: Point3, radius: f32, perspective: f32) -> Projected {
    let k = perspective / (perspective - p.z);
    Projected {
        pos: (p.x * k * radius, p.y * k * radius),
        depth: p.z,
        scale: k,
    }
}

/// Sorts projected points back-to-front (ascending depth) so painting them
/// in order gives a correct painter's-algorithm overlap — points farther
/// from the camera get drawn first and are covered by nearer ones.
pub fn depth_sorted(mut points: Vec<Projected>) -> Vec<Projected> {
    points.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-4;

    #[test]
    fn sphere_points_returns_the_requested_count() {
        assert_eq!(sphere_points(90).len(), 90);
    }

    #[test]
    fn sphere_points_of_zero_is_empty_not_a_panic() {
        assert!(sphere_points(0).is_empty());
    }

    #[test]
    fn every_sphere_point_lies_on_the_unit_sphere() {
        for p in sphere_points(50) {
            assert!((p.norm() - 1.0).abs() < 1e-3, "point off the sphere: {p:?}");
        }
    }

    #[test]
    fn ring_points_of_zero_is_empty_not_a_panic() {
        assert!(ring_points(0, 0.3).is_empty());
    }

    #[test]
    fn every_ring_point_lies_on_the_unit_circle() {
        for p in ring_points(32, 0.4) {
            assert!(
                (p.norm() - 1.0).abs() < EPSILON,
                "point off the ring: {p:?}"
            );
        }
    }

    #[test]
    fn an_untilted_ring_stays_in_the_xz_plane() {
        for p in ring_points(16, 0.0) {
            assert!(p.y.abs() < EPSILON);
        }
    }

    #[test]
    fn rotating_by_zero_is_identity() {
        let p = Point3::new(0.3, 0.5, 0.8);
        let rotated = rotate(p, 0.0, 0.0);

        assert!((rotated.x - p.x).abs() < EPSILON);
        assert!((rotated.y - p.y).abs() < EPSILON);
        assert!((rotated.z - p.z).abs() < EPSILON);
    }

    #[test]
    fn rotation_preserves_distance_from_the_origin() {
        let p = Point3::new(0.2, -0.6, 0.7);
        let rotated = rotate(p, 1.234, -0.876);

        assert!((rotated.norm() - p.norm()).abs() < EPSILON);
    }

    #[test]
    fn a_full_turn_returns_to_the_start() {
        let p = Point3::new(0.4, 0.1, 0.9);
        let rotated = rotate(p, 2.0 * PI, 2.0 * PI);

        assert!((rotated.x - p.x).abs() < 1e-3);
        assert!((rotated.y - p.y).abs() < 1e-3);
        assert!((rotated.z - p.z).abs() < 1e-3);
    }

    #[test]
    fn a_closer_point_projects_with_a_bigger_scale() {
        let near = project(Point3::new(0.0, 0.0, 0.5), 100.0, PERSPECTIVE);
        let far = project(Point3::new(0.0, 0.0, -0.5), 100.0, PERSPECTIVE);

        assert!(near.scale > far.scale);
    }

    #[test]
    fn a_point_on_the_axis_projects_to_the_center() {
        let projected = project(Point3::new(0.0, 0.0, 0.3), 100.0, PERSPECTIVE);

        assert!(projected.pos.0.abs() < EPSILON);
        assert!(projected.pos.1.abs() < EPSILON);
    }

    #[test]
    fn projection_never_divides_by_zero_for_any_point_on_the_unit_sphere() {
        for p in sphere_points(200) {
            let projected = project(p, 100.0, PERSPECTIVE);
            assert!(projected.scale.is_finite());
        }
    }

    #[test]
    fn depth_sorted_orders_ascending_by_depth() {
        let points = vec![
            Projected {
                pos: (0.0, 0.0),
                depth: 0.5,
                scale: 1.0,
            },
            Projected {
                pos: (0.0, 0.0),
                depth: -0.5,
                scale: 1.0,
            },
            Projected {
                pos: (0.0, 0.0),
                depth: 0.0,
                scale: 1.0,
            },
        ];

        let sorted = depth_sorted(points);

        assert_eq!(sorted[0].depth, -0.5);
        assert_eq!(sorted[1].depth, 0.0);
        assert_eq!(sorted[2].depth, 0.5);
    }

    #[test]
    fn depth_sorted_is_stable_for_ties() {
        let points = vec![
            Projected {
                pos: (1.0, 0.0),
                depth: 0.0,
                scale: 1.0,
            },
            Projected {
                pos: (2.0, 0.0),
                depth: 0.0,
                scale: 1.0,
            },
        ];

        let sorted = depth_sorted(points);

        assert_eq!(sorted[0].pos, (1.0, 0.0));
        assert_eq!(sorted[1].pos, (2.0, 0.0));
    }
}
