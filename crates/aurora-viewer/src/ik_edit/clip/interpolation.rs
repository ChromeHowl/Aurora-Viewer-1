//! How the timeline goes from one key to the next. A native animation only
//! holds keys, which every viewer plays linearly (rotations along the
//! shortest arc); Step and Spline are written into the file as extra keys
//! on the 30 Hz authoring grid, so that the result plays the same
//! everywhere: Step holds each key until one frame before the next, Spline
//! adds a key on every frame between two keys, on a Catmull-Rom curve
//! through their neighbours.
use aurora_assets::anim::{PosKey, RotKey};
use glam::{Quat, Vec3, Vec4};
use serde::{Deserialize, Serialize};

use super::document::FPS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Interpolation {
    Step,
    #[default]
    Linear,
    Spline,
}

impl Interpolation {
    pub const ALL: [Interpolation; 3] = [Interpolation::Step, Interpolation::Linear, Interpolation::Spline];

    pub fn label(self) -> &'static str {
        match self {
            Interpolation::Step => aurora_i18n::tr!("b4-clip-step"),
            Interpolation::Linear => aurora_i18n::tr!("b4-clip-linear"),
            Interpolation::Spline => aurora_i18n::tr!("b4-clip-spline"),
        }
    }

    pub fn rotations(self, keys: Vec<RotKey>) -> Vec<RotKey> {
        let values: Vec<Vec4> = aligned(&keys).into_iter().map(Vec4::from).collect();
        let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
        self.expand(&times, &values)
            .into_iter()
            .map(|(time, v)| RotKey {
                time,
                rotation: Quat::from_vec4(v).normalize(),
            })
            .collect()
    }

    pub fn positions(self, keys: Vec<PosKey>) -> Vec<PosKey> {
        let values: Vec<Vec4> = keys.iter().map(|k| k.position.extend(0.0)).collect();
        let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
        self.expand(&times, &values)
            .into_iter()
            .map(|(time, v)| PosKey {
                time,
                position: Vec3::new(v.x, v.y, v.z),
            })
            .collect()
    }

    /// The keys with what this interpolation adds between them.
    fn expand(self, times: &[f32], values: &[Vec4]) -> Vec<(f32, Vec4)> {
        let step = 1.0 / FPS;
        let mut out = Vec::with_capacity(values.len());
        for i in 0..values.len() {
            out.push((times[i], values[i]));
            let Some(&next) = times.get(i + 1) else { break };
            let gap = next - times[i];
            match self {
                Interpolation::Linear => {}
                Interpolation::Step => {
                    if gap > step * 1.5 {
                        out.push((next - step, values[i]));
                    }
                }
                Interpolation::Spline => {
                    let p0 = values[i.saturating_sub(1)];
                    let (p1, p2) = (values[i], values[i + 1]);
                    let p3 = values[(i + 2).min(values.len() - 1)];
                    let frames = (gap / step).round() as usize;
                    for f in 1..frames {
                        let u = f as f32 / frames as f32;
                        out.push((times[i] + f as f32 * step, catmull_rom(p0, p1, p2, p3, u)));
                    }
                }
            }
        }
        out
    }
}

/// Quaternions on the same hemisphere as the one before, so that the curve
/// takes the short way.
fn aligned(keys: &[RotKey]) -> Vec<Quat> {
    let mut out: Vec<Quat> = Vec::with_capacity(keys.len());
    for key in keys {
        let q = match out.last() {
            Some(previous) if previous.dot(key.rotation) < 0.0 => -key.rotation,
            _ => key.rotation,
        };
        out.push(q);
    }
    out
}

/// The uniform Catmull-Rom curve between `p1` and `p2`.
fn catmull_rom(p0: Vec4, p1: Vec4, p2: Vec4, p3: Vec4, u: f32) -> Vec4 {
    let u2 = u * u;
    let u3 = u2 * u;
    0.5 * ((2.0 * p1) + (p2 - p0) * u + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * u3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(time: f32, x: f32) -> PosKey {
        PosKey {
            time,
            position: Vec3::new(x, 0.0, 0.0),
        }
    }

    #[test]
    fn linear_keeps_the_keys_and_step_holds_until_the_next() {
        let keys = vec![pos(0.0, 0.0), pos(1.0, 1.0), pos(1.0 + 1.0 / FPS, 2.0)];
        assert_eq!(Interpolation::Linear.positions(keys.clone()), keys);
        let stepped = Interpolation::Step.positions(keys.clone());
        // a held key one frame before the second, none between frames next to each other
        assert_eq!(stepped.len(), 4);
        assert!((stepped[1].time - (1.0 - 1.0 / FPS)).abs() < 1e-5);
        assert_eq!(stepped[1].position, keys[0].position);
        assert_eq!(stepped[2], keys[1]);
        assert_eq!(stepped[3], keys[2]);
    }

    #[test]
    fn spline_adds_one_key_a_frame_through_the_keys() {
        let keys = vec![pos(0.0, 0.0), pos(1.0, 1.0), pos(2.0, 0.0)];
        let curve = Interpolation::Spline.positions(keys.clone());
        assert_eq!(curve.len(), 61);
        assert!(curve.windows(2).all(|w| w[1].time > w[0].time));
        // through every key, and smooth over the middle one (above the line)
        for key in &keys {
            assert!(curve.iter().any(|k| (k.time - key.time).abs() < 1e-5 && k.position == key.position));
        }
        let near = curve.iter().find(|k| (k.time - (1.0 - 1.0 / FPS)).abs() < 1e-4).expect("frame 29");
        assert!(near.position.x > 1.0 - 1.0 / FPS, "{}", near.position.x);
        // a single key stays alone
        assert_eq!(Interpolation::Spline.positions(vec![pos(0.0, 3.0)]).len(), 1);
        assert!(Interpolation::Spline.positions(Vec::new()).is_empty());
    }

    #[test]
    fn rotations_take_the_short_way_and_stay_unit() {
        let a = Quat::from_rotation_z(0.2);
        let b = -Quat::from_rotation_z(0.6);
        let keys = vec![RotKey { time: 0.0, rotation: a }, RotKey { time: 0.5, rotation: b }];
        for kind in Interpolation::ALL {
            let out = kind.rotations(keys.clone());
            assert!(out.iter().all(|k| (k.rotation.length() - 1.0).abs() < 1e-4), "{kind:?}");
            for k in &out {
                // between 0.2 and 0.6 around Z, never the long way round
                let angle = k.rotation.angle_between(Quat::IDENTITY);
                assert!(angle < 0.61 && angle > 0.19, "{kind:?} {angle}");
            }
        }
        let saved = serde_json::to_string(&Interpolation::Spline).expect("save");
        assert_eq!(serde_json::from_str::<Interpolation>(&saved).expect("load"), Interpolation::Spline);
    }
}
