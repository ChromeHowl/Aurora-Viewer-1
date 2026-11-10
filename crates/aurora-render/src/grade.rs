//! Aurora's colour grade and debanding in the post pass (post.wgsl
//! `grade`, `deband`, and the white balance folded into `tonemap`). The
//! uniforms are worked out here, and the shader's maths is mirrored in the
//! tests: the shader must keep these in step.
//!
//! The white balance acts on the scene's light before the tone curve, as a
//! camera's does; lift, gamma, gain and vibrance on the display image after
//! it, where their sliders read as in a photo editor. A neutral grade and
//! debanding at 0 leave the post pass as it was, behind a uniform branch.

use glam::Vec3;

/// How far the white balance moves each channel at the end of its slider.
pub(crate) const WHITE_BALANCE_RANGE: f32 = 0.2;
/// The black level the lift reaches at the end of its slider.
pub(crate) const LIFT_RANGE: f32 = 0.25;
/// The gain's multiplier at the end of its slider is 1 plus or minus this.
pub(crate) const GAIN_RANGE: f32 = 0.5;
/// The weights of the luminance (Rec. 709), as post.wgsl's.
const LUMA: Vec3 = Vec3::new(0.2126, 0.7152, 0.0722);

/// The colour grade (Preferences › Graphics › Colour grade). Every slider
/// runs from -1 to 1, 0 changing nothing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Grade {
    /// Cooler (below 0) or warmer light.
    pub temperature: f32,
    /// Greener (below 0) or more magenta light.
    pub tint: f32,
    /// The shadows: black raised (above 0) or crushed.
    pub lift: f32,
    /// The midtones: brighter (above 0) or darker, black and white kept.
    pub gamma: f32,
    /// The highlights: white scaled up or down.
    pub gain: f32,
    /// The saturation of the dull colours, those already saturated less.
    pub vibrance: f32,
}

impl Grade {
    fn clamped(v: f32) -> f32 {
        if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 }
    }

    /// Whether the display part (lift, gamma, gain, vibrance) changes
    /// anything; the white balance costs nothing either way.
    pub fn display_neutral(&self) -> bool {
        [self.lift, self.gamma, self.gain, self.vibrance]
            .iter()
            .all(|v| Self::clamped(*v) == 0.0)
    }

    /// The gains of the white balance on the scene's linear light, the
    /// luminance of a grey kept: `PostParams::white_balance`.
    pub(crate) fn white_balance(&self) -> Vec3 {
        let t = Self::clamped(self.temperature) * WHITE_BALANCE_RANGE;
        let u = Self::clamped(self.tint) * WHITE_BALANCE_RANGE;
        let gains = Vec3::new(1.0 + t, 1.0 - u, 1.0 - t);
        gains / gains.dot(LUMA)
    }

    /// `PostParams::grade`: the lift's black level, the power of the gamma,
    /// the gain's multiplier and the vibrance.
    pub(crate) fn display(&self) -> [f32; 4] {
        [
            Self::clamped(self.lift) * LIFT_RANGE,
            // above 0 brightens the midtones: a power under 1
            (-Self::clamped(self.gamma)).exp2(),
            1.0 + Self::clamped(self.gain) * GAIN_RANGE,
            Self::clamped(self.vibrance),
        ]
    }
}

/// The debanding's strength, in steps of an 8-bit level (0 off, at most 2).
pub fn deband_strength(strength: f32) -> f32 {
    if strength.is_finite() { strength.clamp(0.0, 2.0) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHADER: &str = include_str!("shaders/post.wgsl");

    fn linear_to_srgb(c: f32) -> f32 {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }

    /// post.wgsl `grade`: on the display-encoded image, lift and gain, the
    /// gamma, then the vibrance.
    fn grade(m: Vec3, g: [f32; 4]) -> Vec3 {
        let mut c = m.clamp(Vec3::ZERO, Vec3::ONE).to_array().map(linear_to_srgb);
        for v in &mut c {
            *v = (g[2] * (*v + g[0] * (1.0 - *v))).clamp(0.0, 1.0).powf(g[1]);
        }
        let c = Vec3::from(c);
        let l = c.dot(LUMA);
        let sat = c.max_element() - c.min_element();
        let c = Vec3::splat(l).lerp(c, 1.0 + g[3] * (1.0 - sat)).clamp(Vec3::ZERO, Vec3::ONE);
        Vec3::from(c.to_array().map(srgb_to_linear))
    }

    /// post.wgsl `pcg_hash` and `dither`: a triangular noise in -1..1 from
    /// two hashes of the pixel.
    fn pcg_hash(v: u32) -> u32 {
        let x = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        let w = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277_803_737);
        (w >> 22) ^ w
    }

    fn dither(px: [u32; 2]) -> f32 {
        let a = pcg_hash(px[0].wrapping_add(pcg_hash(px[1])));
        let b = pcg_hash(a);
        let u = |h: u32| (h >> 8) as f32 / 16_777_216.0;
        u(a) + u(b) - 1.0
    }

    fn near(a: Vec3, b: Vec3, tolerance: f32) -> bool {
        (a - b).abs().max_element() <= tolerance
    }

    #[test]
    fn a_neutral_grade_changes_nothing() {
        let g = Grade::default();
        assert!(g.display_neutral());
        assert_eq!(g.white_balance(), Vec3::ONE);
        assert_eq!(g.display(), [0.0, 1.0, 1.0, 0.0]);
        for c in [Vec3::ZERO, Vec3::splat(0.18), Vec3::new(0.9, 0.4, 0.05), Vec3::ONE] {
            assert!(near(grade(c, g.display()), c, 1e-5), "{c:?}");
        }
        // odd values from a settings file are neutral
        let odd = Grade {
            lift: f32::NAN,
            gamma: f32::INFINITY,
            ..g
        };
        assert!(odd.display_neutral());
    }

    #[test]
    fn the_white_balance_keeps_a_grey_as_bright_and_turns_its_colour() {
        for (t, u) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.6, -0.4), (5.0, -5.0)] {
            let g = Grade {
                temperature: t,
                tint: u,
                ..Default::default()
            };
            let wb = g.white_balance();
            assert!((wb.dot(LUMA) - 1.0).abs() < 1e-6, "{t} {u}: {wb:?}");
            assert!(wb.min_element() > 0.0);
        }
        let warm = Grade {
            temperature: 1.0,
            ..Default::default()
        }
        .white_balance();
        assert!(warm.x > warm.z, "warmer: more red than blue");
        let magenta = Grade {
            tint: 1.0,
            ..Default::default()
        }
        .white_balance();
        assert!(magenta.y < magenta.x && magenta.y < magenta.z);
    }

    #[test]
    fn lift_gamma_and_gain_act_on_their_own_tones() {
        let at = |g: Grade, v: f32| grade(Vec3::splat(v), g.display()).x;
        let lift = Grade {
            lift: 1.0,
            ..Default::default()
        };
        assert!(at(lift, 0.0) > 0.02, "the lift raises black");
        assert!((at(lift, 1.0) - 1.0).abs() < 1e-5, "and keeps white");
        let gain = Grade {
            gain: -1.0,
            ..Default::default()
        };
        assert!(at(gain, 0.0).abs() < 1e-6, "the gain keeps black");
        assert!(at(gain, 1.0) < 0.5, "and dims white");
        let gamma = Grade {
            gamma: 1.0,
            ..Default::default()
        };
        assert!(at(gamma, 0.18) > 0.18 * 1.5, "a brighter middle grey");
        assert!(at(gamma, 0.0).abs() < 1e-6 && (at(gamma, 1.0) - 1.0).abs() < 1e-5);
        let darker = Grade {
            gamma: -1.0,
            ..Default::default()
        };
        assert!(at(darker, 0.18) < 0.18);
    }

    #[test]
    fn the_vibrance_lifts_dull_colours_more_than_bright_ones() {
        let g = Grade {
            vibrance: 1.0,
            ..Default::default()
        };
        let sat = |c: Vec3| {
            let e = c.to_array().map(linear_to_srgb);
            e.iter().copied().fold(0.0_f32, f32::max) - e.iter().copied().fold(1.0_f32, f32::min)
        };
        let dull = Vec3::new(0.25, 0.2, 0.18);
        let vivid = Vec3::new(0.8, 0.1, 0.02);
        let gain_dull = sat(grade(dull, g.display())) / sat(dull);
        let gain_vivid = sat(grade(vivid, g.display())) / sat(vivid);
        assert!(gain_dull > gain_vivid && gain_dull > 1.5, "{gain_dull} {gain_vivid}");
        // a grey stays grey
        let grey = grade(Vec3::splat(0.3), g.display());
        assert!(grey.max_element() - grey.min_element() < 1e-5);
        let less = Grade {
            vibrance: -1.0,
            ..Default::default()
        };
        assert!(sat(grade(dull, less.display())) < sat(dull));
    }

    #[test]
    fn graded_colours_stay_in_range() {
        for lift in [-1.0, 1.0] {
            for gamma in [-1.0, 1.0] {
                for gain in [-1.0, 1.0] {
                    for vibrance in [-1.0, 1.0] {
                        let g = Grade {
                            lift,
                            gamma,
                            gain,
                            vibrance,
                            ..Default::default()
                        };
                        for c in [Vec3::ZERO, Vec3::new(1.0, 0.0, 0.5), Vec3::splat(4.0), Vec3::new(0.02, 0.9, 0.3)] {
                            let out = grade(c, g.display());
                            assert!(
                                out.is_finite() && out.min_element() >= 0.0 && out.max_element() <= 1.0,
                                "{g:?} {c:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_dither_is_a_centred_triangle_of_one_level() {
        let (mut sum, mut small, n) = (0.0_f64, 0, 256 * 256);
        for y in 0..256 {
            for x in 0..256 {
                let d = dither([x, y]);
                assert!((-1.0..=1.0).contains(&d));
                sum += f64::from(d);
                if d.abs() < 0.5 {
                    small += 1;
                }
            }
        }
        assert!((sum / f64::from(n)).abs() < 0.01, "centred: {}", sum / f64::from(n));
        // a triangle holds three quarters of its weight within half of its reach
        let share = f64::from(small) / f64::from(n);
        assert!((share - 0.75).abs() < 0.02, "{share}");
        // neighbours differ: no visible pattern of one value
        assert_ne!(dither([10, 10]), dither([11, 10]));
        assert_eq!(deband_strength(f32::NAN), 0.0);
        assert_eq!(deband_strength(9.0), 2.0);
    }

    #[test]
    fn the_shader_keeps_the_mirrored_maths() {
        for line in [
            "c = clamp(post.grade.z * (c + post.grade.x * (vec3<f32>(1.0) - c)), vec3<f32>(0.0), vec3<f32>(1.0));",
            "c = pow(c, vec3<f32>(post.grade.y));",
            "c = clamp(mix(vec3<f32>(l), c, 1.0 + post.grade.w * (1.0 - sat)), vec3<f32>(0.0), vec3<f32>(1.0));",
            "let x = v * 747796405u + 2891336453u;",
            "let w = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;",
            "return (w >> 22u) ^ w;",
            "let a = pcg_hash(px.x + pcg_hash(px.y));",
            "return f32(a >> 8u) / 16777216.0 + f32(b >> 8u) / 16777216.0 - 1.0;",
            "let e = max(c * exposure() * post.white_balance.rgb, vec3<f32>(0.0));",
        ] {
            assert!(SHADER.contains(line), "post.wgsl lost: {line}");
        }
    }
}
