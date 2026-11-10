//! The tone curves of the post pass (post.wgsl `neutral`, `aces`,
//! `reinhard`, `hable`, `agx`), mirrored here to test them: the shader must
//! keep these in step.

use glam::{Mat3, Vec3};

const SHADER: &str = include_str!("shaders/post.wgsl");

const CURVE_INPUT_MAX: f32 = 65504.0;
const REINHARD_WHITE: f32 = 4.0;
const HABLE_WHITE: f32 = 11.2;
const AGX_MIN_EV: f32 = -12.47393;
const AGX_MAX_EV: f32 = 4.026069;

fn neutral(color_in: Vec3) -> Vec3 {
    let start = 0.8 - 0.04;
    let desat = 0.15;
    let x = color_in.min_element();
    let offset = if x < 0.08 { x - 6.25 * x * x } else { 0.04 };
    let mut color = color_in - Vec3::splat(offset);
    let peak = color.max_element();
    if peak < start {
        return color;
    }
    let d = 1.0 - start;
    let new_peak = 1.0 - d * d / (peak + d - start);
    color *= new_peak / peak;
    let g = 1.0 - 1.0 / (desat * (peak - new_peak) + 1.0);
    color.lerp(Vec3::splat(new_peak), g)
}

fn aces(x: Vec3) -> Vec3 {
    let input = Mat3::from_cols(
        Vec3::new(0.59719, 0.07600, 0.02840),
        Vec3::new(0.35458, 0.90834, 0.13383),
        Vec3::new(0.04823, 0.01566, 0.83777),
    );
    let output = Mat3::from_cols(
        Vec3::new(1.60475, -0.10208, -0.00327),
        Vec3::new(-0.53108, 1.10813, -0.07276),
        Vec3::new(-0.07367, -0.00605, 1.07602),
    );
    let c = input * x;
    let a = c * (c + 0.024_578_6) - 0.000_090_537;
    let b = c * (0.983_729 * c + 0.432_951) + 0.238_081;
    (output * (a / b)).clamp(Vec3::ZERO, Vec3::ONE)
}

fn reinhard(color_in: Vec3) -> Vec3 {
    let c = color_in.min(Vec3::splat(CURVE_INPUT_MAX));
    let l = c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let scale = (1.0 + l / (REINHARD_WHITE * REINHARD_WHITE)) / (1.0 + l);
    (c * scale).clamp(Vec3::ZERO, Vec3::ONE)
}

fn hable_curve(x: Vec3) -> Vec3 {
    let num = x * (0.15 * x + 0.10 * 0.50) + 0.20 * 0.02;
    let den = x * (0.15 * x + 0.50) + 0.20 * 0.30;
    num / den - 0.02 / 0.30
}

fn hable(color_in: Vec3) -> Vec3 {
    let c = color_in.min(Vec3::splat(CURVE_INPUT_MAX));
    let white = hable_curve(Vec3::splat(HABLE_WHITE));
    (hable_curve(2.0 * c) / white).clamp(Vec3::ZERO, Vec3::ONE)
}

fn agx_contrast(x: Vec3) -> Vec3 {
    let x2 = x * x;
    let x4 = x2 * x2;
    15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232
}

fn agx(color_in: Vec3) -> Vec3 {
    let inset = Mat3::from_cols(
        Vec3::new(0.842_479_06, 0.042_328_24, 0.042_375_655),
        Vec3::new(0.078_433_6, 0.878_468_63, 0.078_433_6),
        Vec3::new(0.079_223_745, 0.079_166_13, 0.879_142_94),
    );
    let outset = Mat3::from_cols(
        Vec3::new(1.196_879, -0.052_896_85, -0.052_971_635),
        Vec3::new(-0.098_020_88, 1.151_903_2, -0.098_043_45),
        Vec3::new(-0.099_029_74, -0.098_961_18, 1.151_073_7),
    );
    let v = (inset * color_in).max(Vec3::splat(1.0e-10));
    let ev = Vec3::new(v.x.log2(), v.y.log2(), v.z.log2()).clamp(Vec3::splat(AGX_MIN_EV), Vec3::splat(AGX_MAX_EV));
    let x = (ev - AGX_MIN_EV) / (AGX_MAX_EV - AGX_MIN_EV);
    (outset * agx_contrast(x)).clamp(Vec3::ZERO, Vec3::ONE).powf(2.2)
}

type Curve = fn(Vec3) -> Vec3;

const CURVES: [(&str, Curve); 5] = [
    ("neutral", neutral),
    ("aces", aces),
    ("reinhard", reinhard),
    ("hable", hable),
    ("agx", agx),
];

fn near(a: Vec3, b: [f32; 3], tolerance: f32) -> bool {
    (a - Vec3::from(b)).abs().max_element() <= tolerance
}

/// What the post pass does with a curve (post.wgsl `tonemap`): the mix with
/// the exposed linear color, then the clamp.
fn mixed(curve: Curve, e: Vec3, mix: f32) -> Vec3 {
    e.lerp(curve(e), mix).clamp(Vec3::ZERO, Vec3::ONE)
}

#[test]
fn black_stays_black() {
    for (name, curve) in CURVES {
        let out = curve(Vec3::ZERO);
        assert!(near(out, [0.0; 3], 1e-6), "{name}: {out:?}");
    }
}

#[test]
fn a_brighter_grey_is_never_darker() {
    for (name, curve) in CURVES {
        // AgX's published fit of the sigmoid dips by about a millionth
        // over its last stop
        let slack = if name == "agx" { 1e-5 } else { 1e-6 };
        let mut last = 0.0_f32;
        // 0 to 64 by steps growing with the value, then far past white
        let ramp = (0..4000).map(|i| (i as f32 / 4000.0).powi(3) * 64.0);
        for g in ramp.chain([100.0, 1.0e3, 6.0e4, 1.0e6]) {
            let out = curve(Vec3::splat(g)).clamp(Vec3::ZERO, Vec3::ONE);
            assert!(out.x >= last - slack, "{name}: {g} gives {} after {last}", out.x);
            last = last.max(out.x);
        }
        assert!(last > 0.95, "{name}: a very bright grey reaches white, got {last}");
    }
}

/// An 18 % grey stays a middle grey: between 0.10 and 0.22 in linear light
/// (about 89 to 129 of 255 once encoded).
#[test]
fn middle_grey_stays_a_middle_grey() {
    for (name, curve) in CURVES {
        let out = curve(Vec3::splat(0.18));
        assert!((0.10..=0.22).contains(&out.x), "{name}: {out:?}");
        // a grey stays neutral (AgX's matrices leave a 0.1 % tint)
        assert!((out.max_element() - out.min_element()).abs() < 2e-3, "{name}: {out:?}");
    }
}

#[test]
fn large_and_odd_colors_give_finite_colors_in_range() {
    let colors = [
        Vec3::splat(1.0e4),
        Vec3::splat(CURVE_INPUT_MAX),
        Vec3::new(6.0e4, 0.0, 0.0),
        Vec3::new(0.0, 1.0e-12, 5.0e4),
        Vec3::new(1.0e-20, 3.0, 0.0),
    ];
    // past the half floats of the scene image too, for the three curves
    // that cut their input
    let beyond = [Vec3::splat(1.0e20), Vec3::new(3.0e38, 0.0, 1.0), Vec3::splat(f32::MAX)];
    for (name, curve) in CURVES {
        let own = !matches!(name, "neutral" | "aces");
        for c in colors.iter().chain(beyond.iter().filter(|_| own)) {
            for mix in [0.0, 0.7, 1.0] {
                let out = mixed(curve, *c, mix);
                assert!(out.is_finite(), "{name}: {c:?} at mix {mix} gives {out:?}");
                assert!(out.min_element() >= 0.0 && out.max_element() <= 1.0, "{name}: {c:?} gives {out:?}");
            }
        }
    }
}

/// RenderTonemapMix acts on every curve: 0 is the exposed color clamped, 1
/// the curve alone, and between the two it moves from one to the other.
#[test]
fn the_mix_goes_from_the_linear_color_to_the_curve() {
    let e = Vec3::new(0.9, 0.4, 0.1);
    for (name, curve) in CURVES {
        assert!(near(mixed(curve, e, 0.0), e.to_array(), 1e-6), "{name}");
        assert!(near(mixed(curve, e, 1.0), curve(e).to_array(), 1e-6), "{name}");
        let half = mixed(curve, e, 0.5);
        assert!(near(half, ((e + curve(e)) * 0.5).to_array(), 1e-6), "{name}");
    }
}

/// The Legacy Viewer's two curves give what they gave before the others
/// came (values worked out apart from this file, in double precision).
#[test]
fn neutral_and_aces_are_unchanged() {
    let cases: [([f32; 3], [f32; 3], [f32; 3]); 4] = [
        ([0.18, 0.18, 0.18], [0.14, 0.14, 0.14], [0.105_591_2, 0.105_591_2, 0.105_590_2]),
        (
            [1.0, 0.5, 0.25],
            [0.869_090_9, 0.422_528_9, 0.199_247_9],
            [0.634_990_4, 0.384_600_1, 0.203_160_4],
        ),
        (
            [4.0, 2.0, 0.1],
            [0.983_255_8, 0.639_951_4, 0.313_812_2],
            [0.950_184_7, 0.811_815_9, 0.313_324_5],
        ),
        (
            [0.02, 0.05, 0.9],
            [0.007_558_6, 0.035_974_9, 0.841_103_4],
            [0.0, 0.015_419_6, 0.564_200_3],
        ),
    ];
    for (input, n, a) in cases {
        let c = Vec3::from(input);
        assert!(near(neutral(c), n, 2e-5), "neutral {input:?}: {:?}", neutral(c));
        assert!(near(aces(c), a, 2e-5), "aces {input:?}: {:?}", aces(c));
    }
    // and their shader text is the one mirrored above
    for line in [
        "let offset = select(0.04, x - 6.25 * x * x, x < 0.08);",
        "let new_peak = 1.0 - d * d / (peak + d - start);",
        "let g = 1.0 - 1.0 / (desat * (peak - new_peak) + 1.0);",
        "vec3<f32>(0.59719, 0.07600, 0.02840),",
        "vec3<f32>(-0.07367, -0.00605, 1.07602),",
        "let a = c * (c + 0.0245786) - 0.000090537;",
        "let b = c * (0.983729 * c + 0.4329510) + 0.238081;",
        "return clamp(ACES_OUTPUT * (a / b), vec3<f32>(0.0), vec3<f32>(1.0));",
    ] {
        assert!(SHADER.contains(line), "post.wgsl lost: {line}");
    }
}

/// Known points of the three curves (double precision, apart from this
/// file), and their constants as the shader writes them.
#[test]
fn reinhard_hable_and_agx_match_their_references() {
    let c = Vec3::new(1.0, 0.5, 0.25);
    assert!(near(reinhard(c), [0.652_77, 0.326_39, 0.163_19], 2e-5), "{:?}", reinhard(c));
    assert!(near(hable(c), [0.492_92, 0.304_30, 0.171_97], 2e-5), "{:?}", hable(c));
    assert!(near(agx(c), [0.610_11, 0.431_15, 0.291_68], 1e-4), "{:?}", agx(c));
    for (curve, grey, white) in [
        (reinhard as Curve, 0.154_258, 0.531_25),
        (hable, 0.128_338, 0.492_919),
        (agx, 0.214_467, 0.589_977),
    ] {
        assert!((curve(Vec3::splat(0.18)).x - grey).abs() < 1e-4);
        assert!((curve(Vec3::splat(1.0)).x - white).abs() < 1e-4);
    }
    // Reinhard's white point is white, Uncharted 2's linear white too
    assert!((reinhard(Vec3::splat(REINHARD_WHITE)).x - 1.0).abs() < 1e-6);
    assert!((hable(Vec3::splat(HABLE_WHITE / 2.0)).x - 1.0).abs() < 1e-5);
    for line in [
        "const CURVE_INPUT_MAX: f32 = 65504.0;",
        "const REINHARD_WHITE: f32 = 4.0;",
        "const HABLE_WHITE: f32 = 11.2;",
        "const AGX_MIN_EV: f32 = -12.47393;",
        "const AGX_MAX_EV: f32 = 4.026069;",
        "vec3<f32>(0.842479062253094, 0.0423282422610123, 0.0423756549057051),",
        "vec3<f32>(-0.0990297440797205, -0.0989611768448433, 1.15107367264116),",
        "return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;",
    ] {
        assert!(SHADER.contains(line), "post.wgsl lost: {line}");
    }
}

/// The numbers the post pass tells the curves apart with: the Legacy
/// Viewer's two and "none" keep theirs, the others come after.
#[test]
fn the_shader_ids_of_the_first_curves_are_kept() {
    use crate::types::{TONEMAPPER_NONE, Tonemapper};
    let ids = [
        Tonemapper::KhronosNeutral,
        Tonemapper::Aces,
        Tonemapper::Reinhard,
        Tonemapper::Uncharted2,
        Tonemapper::Agx,
    ]
    .map(Tonemapper::shader_id);
    assert_eq!(ids, [0.0, 1.0, 3.0, 4.0, 5.0]);
    assert_eq!(TONEMAPPER_NONE, 2.0);
}
