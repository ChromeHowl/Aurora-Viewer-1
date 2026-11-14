//! The Spline interpolation of a track: a uniform Catmull-Rom curve through
//! its keys, taking the neighbouring keys into account. A native animation
//! only holds keys, which every viewer plays linearly (rotations along the
//! shortest arc), so `Document::build` writes the curve as a key on every
//! frame of the export, exactly as Smooth and Custom curve are written; the
//! result plays the same everywhere. Step and Linear are the existing
//! `Interpolation::Stepped` and `Linear` of `document.rs`.
use glam::{Quat, Vec3, Vec4};

/// The uniform Catmull-Rom curve between `p1` and `p2`, `u` in 0..=1.
fn catmull_rom(p0: Vec4, p1: Vec4, p2: Vec4, p3: Vec4, u: f32) -> Vec4 {
    let u2 = u * u;
    let u3 = u2 * u;
    0.5 * ((2.0 * p1) + (p2 - p0) * u + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * u3)
}

/// `[before, a, b, after]` on the Catmull-Rom curve between `a` and `b`.
pub(super) fn spline_position([p0, p1, p2, p3]: [Vec3; 4], u: f32) -> Vec3 {
    let v = catmull_rom(p0.extend(0.0), p1.extend(0.0), p2.extend(0.0), p3.extend(0.0), u);
    Vec3::new(v.x, v.y, v.z)
}

/// The same for rotations: the quaternions are brought to the hemisphere of
/// `a` (and `after` to that of `b`) so that the curve takes the short way,
/// and the result is normalised.
pub(super) fn spline_rotation([q0, q1, q2, q3]: [Quat; 4], u: f32) -> Quat {
    let same_side = |q: Quat, reference: Quat| if reference.dot(q) < 0.0 { -q } else { q };
    let (q0, q2) = (same_side(q0, q1), same_side(q2, q1));
    let q3 = same_side(q3, q2);
    let v = catmull_rom(Vec4::from(q0), Vec4::from(q1), Vec4::from(q2), Vec4::from(q3), u);
    let q = Quat::from_vec4(v);
    if q.is_finite() && q.length_squared() > 1e-12 {
        q.normalize()
    } else {
        q1.slerp(q2, u)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_pass_through_the_two_keys_and_bend_over_the_middle_one() {
        let (a, b, c) = (Vec3::ZERO, Vec3::X, Vec3::ZERO);
        // from the first key to the second, with the third after it
        assert_eq!(spline_position([a, a, b, c], 0.0), a);
        assert!((spline_position([a, a, b, c], 1.0) - b).length() < 1e-5);
        // the curve overshoots the straight line towards the key that follows
        assert!(spline_position([a, a, b, c], 0.9).x > 0.9);
    }

    #[test]
    fn rotations_take_the_short_way_and_stay_unit() {
        let a = Quat::from_rotation_z(0.2);
        let b = -Quat::from_rotation_z(0.6);
        for step in 0..=10 {
            let q = spline_rotation([a, a, b, b], step as f32 / 10.0);
            assert!((q.length() - 1.0).abs() < 1e-4);
            // between 0.2 and 0.6 around Z, never the long way round
            let angle = q.angle_between(Quat::IDENTITY);
            assert!(angle < 0.61 && angle > 0.19, "{angle}");
        }
    }
}
