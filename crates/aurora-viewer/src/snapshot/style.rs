//! The photo's picture style and white balance: an overlay over
//! Preferences' tone curve and colour grade (`Settings::tonemapper`,
//! `render_extras::grade`), held in `PhotoLight` for the photo mode only,
//! like its exposure, and gone when the photo mode closes unless "Keep this
//! look" makes it Preferences' own. The viewfinder (its Style sheet and white
//! balance) and the photo window's "Picture style" row change it; a photo
//! profile holds it. Preferences keep showing and changing their own values.

use super::light::PhotoLight;
use crate::settings::render_extras::Grade;

/// The tone curve a photo is drawn with: the overlay's, else Preferences'.
pub fn tonemapper(light: &PhotoLight, prefs: u8) -> u8 {
    light.tonemapper.unwrap_or(prefs)
}

/// The grade a photo is drawn with: the overlay's, else Preferences'.
pub fn grade(light: &PhotoLight, prefs: &Grade) -> Grade {
    light.grade.unwrap_or(*prefs)
}

/// What the overlay starts from: Preferences' grade as it is drawn, or a
/// neutral one when it is switched off (its old values never come back
/// with a white balance).
pub fn base(prefs: &Grade) -> Grade {
    if prefs.enabled {
        *prefs
    } else {
        Grade {
            enabled: true,
            ..Grade::default()
        }
    }
}

/// The grade an edit changes: the overlay's, else the start (`base`).
pub fn edit_start(light: &PhotoLight, prefs: &Grade) -> Grade {
    light.grade.unwrap_or_else(|| base(prefs))
}

/// The overlay is set (the photo does not look as the viewer does).
pub fn is_set(light: &PhotoLight) -> bool {
    light.tonemapper.is_some() || light.grade.is_some()
}

/// "Keep this look": the overlay made Preferences' own, then cleared;
/// true when there was one.
pub fn keep(light: &mut PhotoLight, tonemapper: &mut u8, grade: &mut Grade) -> bool {
    let kept = is_set(light);
    if let Some(t) = light.tonemapper.take() {
        *tonemapper = t;
    }
    if let Some(g) = light.grade.take() {
        *grade = g;
    }
    kept
}

/// A profile's values in their ranges: a curve Aurora does not know is
/// dropped, the grade's sliders brought back between −1 and 1.
pub fn sanitize(tonemapper: Option<u8>, grade: Option<Grade>) -> (Option<u8>, Option<Grade>) {
    let t = tonemapper.filter(|t| *t <= crate::settings::TONEMAPPER_MAX);
    let clamp = |v: f32| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
    let g = grade.map(|g| Grade {
        enabled: g.enabled,
        temperature: clamp(g.temperature),
        tint: clamp(g.tint),
        lift: clamp(g.lift),
        gamma: clamp(g.gamma),
        gain: clamp(g.gain),
        vibrance: clamp(g.vibrance),
    });
    (t, g)
}

/// The white balance's grid: temperature and tint in steps of 0.1 of the
/// grade's −1…1 (`Grade::white_balance` shifts red and blue by at most
/// 20 %: no Kelvin, which would mislead), −10 to +10 steps.
pub const WB_STEP: f32 = 0.1;
pub const WB_STEPS: i32 = 10;

/// A value of the grid in steps.
pub fn steps(v: f32) -> i32 {
    if v.is_finite() {
        ((v / WB_STEP).round() as i32).clamp(-WB_STEPS, WB_STEPS)
    } else {
        0
    }
}

/// A step towards `dir` (−1 or 1), within ±10.
pub fn step(v: f32, dir: i32) -> f32 {
    (steps(v) + dir.signum()).clamp(-WB_STEPS, WB_STEPS) as f32 * WB_STEP
}

/// A camera's white balance presets. Auto is no correction: the viewer's
/// colours (Preferences' white balance, none by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wb {
    Auto,
    Daylight,
    Cloudy,
    Shade,
    Tungsten,
    Fluorescent,
}

impl Wb {
    pub const ALL: [Wb; 6] = [Wb::Auto, Wb::Daylight, Wb::Cloudy, Wb::Shade, Wb::Tungsten, Wb::Fluorescent];

    /// The temperature it pins (Auto: none), on the grid.
    pub fn temperature(self) -> Option<f32> {
        match self {
            Wb::Auto => None,
            Wb::Daylight => Some(0.0),
            Wb::Cloudy => Some(0.2),
            Wb::Shade => Some(0.4),
            Wb::Tungsten => Some(-0.6),
            Wb::Fluorescent => Some(-0.3),
        }
    }
}

/// The overlay's grade with one change; the overlay cleared when it comes
/// back to the start (`base`) with no tone curve of its own.
fn edit(light: &mut PhotoLight, prefs: &Grade, change: impl FnOnce(&mut Grade)) {
    let mut g = edit_start(light, prefs);
    change(&mut g);
    light.grade = (g != base(prefs) || light.tonemapper.is_some()).then_some(g);
}

/// The temperature set (a step of the ring, the wheel); only the white
/// balance changes, never the style's lift, gamma, gain or vibrance.
pub fn set_temperature(light: &mut PhotoLight, prefs: &Grade, t: f32) {
    let t = steps(t) as f32 * WB_STEP;
    edit(light, prefs, |g| g.temperature = t);
}

pub fn set_tint(light: &mut PhotoLight, prefs: &Grade, u: f32) {
    let u = steps(u) as f32 * WB_STEP;
    edit(light, prefs, |g| g.tint = u);
}

/// A preset chosen: Auto gives back the viewer's white balance (the whole
/// overlay goes when nothing else of it is set), the others pin their
/// temperature and no tint.
pub fn set_wb(light: &mut PhotoLight, prefs: &Grade, wb: Wb) {
    let b = base(prefs);
    let (t, u) = match wb.temperature() {
        Some(t) => (t, 0.0),
        None => (b.temperature, b.tint),
    };
    edit(light, prefs, |g| {
        g.temperature = t;
        g.tint = u;
    });
}

/// The preset the white balance is on, if any (Auto first: Daylight gives
/// the same picture when Preferences have no white balance).
pub fn wb_of(light: &PhotoLight, prefs: &Grade) -> Option<Wb> {
    let g = grade(light, prefs);
    let b = base(prefs);
    if light.grade.is_none() || (steps(g.temperature), steps(g.tint)) == (steps(b.temperature), steps(b.tint)) {
        return Some(Wb::Auto);
    }
    Wb::ALL
        .into_iter()
        .find(|w| w.temperature().is_some_and(|t| steps(t) == steps(g.temperature)) && steps(g.tint) == 0)
}

/// A picture style: a named tone curve and grade, adjusted in steps (a
/// camera's film simulations, a phone's photographic styles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Preferences' curve and grade: no style of the photo's own.
    AsViewer,
    Standard,
    Neutral,
    Soft,
    Classic,
    Natural,
    Film,
    Vivid,
    /// Off every style's values.
    Custom,
}

/// The styles that can be chosen: their curve (`Settings::tonemapper`'s
/// numbers) and lift, gamma, gain, vibrance. Standard is the default curve
/// (ACES, `Settings::default`).
pub const STYLES: [(Style, u8, [f32; 4]); 7] = [
    (Style::Standard, 1, [0.0; 4]),
    (Style::Neutral, 0, [0.0; 4]),
    (Style::Soft, 2, [0.0; 4]),
    (Style::Classic, 3, [0.0; 4]),
    (Style::Natural, 4, [0.0; 4]),
    (Style::Film, 5, [0.0; 4]),
    (Style::Vivid, 1, [0.0, 0.0, 0.25, 0.5]),
];

/// The fine tuning's grid: −4 to +4 steps of 0.25.
pub const FINE_STEP: f32 = 0.25;

fn rest(g: &Grade) -> [f32; 4] {
    [g.lift, g.gamma, g.gain, g.vibrance]
}

fn on_grid(v: f32) -> i32 {
    (v / FINE_STEP).round() as i32
}

/// A style chosen; the white balance is kept. As viewer gives back
/// Preferences' curve and the start's lift, gamma, gain and vibrance.
pub fn apply_style(light: &mut PhotoLight, prefs: &Grade, style: Style) {
    let b = base(prefs);
    let (tone, values) = match STYLES.iter().find(|(s, _, _)| *s == style) {
        Some((_, t, v)) => (Some(*t), *v),
        None if style == Style::AsViewer => (None, rest(&b)),
        None => return,
    };
    light.tonemapper = tone;
    edit(light, prefs, |g| {
        [g.lift, g.gamma, g.gain, g.vibrance] = values;
    });
}

/// The style the overlay is on (what its chips and label show).
pub fn matching(light: &PhotoLight, prefs: &Grade) -> Style {
    let g = edit_start(light, prefs);
    let same = |v: [f32; 4], w: [f32; 4]| v.iter().zip(w).all(|(a, b)| on_grid(*a) == on_grid(b) && (a - b).abs() < 1e-3);
    match light.tonemapper {
        None if same(rest(&g), rest(&base(prefs))) => Style::AsViewer,
        None => Style::Custom,
        Some(t) => STYLES
            .iter()
            .find(|(_, st, v)| *st == t && same(rest(&g), *v))
            .map_or(Style::Custom, |(s, _, _)| *s),
    }
}

/// A fine-tuning row: Shadows (lift), Midtones (gamma), Highlights (gain),
/// Colour (vibrance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fine {
    Shadows,
    Midtones,
    Highlights,
    Colour,
}

impl Fine {
    pub const ALL: [Fine; 4] = [Fine::Shadows, Fine::Midtones, Fine::Highlights, Fine::Colour];

    fn of(self, g: &mut Grade) -> &mut f32 {
        match self {
            Fine::Shadows => &mut g.lift,
            Fine::Midtones => &mut g.gamma,
            Fine::Highlights => &mut g.gain,
            Fine::Colour => &mut g.vibrance,
        }
    }

    /// Its value in steps (−4 to +4).
    pub fn steps(self, light: &PhotoLight, prefs: &Grade) -> i32 {
        let mut g = edit_start(light, prefs);
        on_grid(*self.of(&mut g))
    }
}

/// A fine-tuning row stepped (`dir` −1 or 1), within ±4 steps.
pub fn fine_step(light: &mut PhotoLight, prefs: &Grade, row: Fine, dir: i32) {
    edit(light, prefs, |g| {
        let v = row.of(g);
        *v = (on_grid(*v) + dir.signum()).clamp(-4, 4) as f32 * FINE_STEP;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs() -> Grade {
        Grade {
            enabled: true,
            temperature: 0.3,
            tint: -0.1,
            lift: 0.2,
            gamma: 0.1,
            gain: -0.2,
            vibrance: 0.4,
        }
    }

    #[test]
    fn no_overlay_draws_with_preferences_values() {
        let light = PhotoLight::default();
        assert_eq!(tonemapper(&light, 3), 3);
        assert_eq!(grade(&light, &prefs()), prefs());
        assert!(!is_set(&light));
        let light = PhotoLight {
            tonemapper: Some(5),
            grade: Some(Grade::default()),
            ..PhotoLight::default()
        };
        assert_eq!(tonemapper(&light, 3), 5);
        assert_eq!(grade(&light, &prefs()), Grade::default());
    }

    #[test]
    fn a_first_edit_from_a_grade_switched_off_starts_neutral() {
        let off = Grade { enabled: false, ..prefs() };
        let start = edit_start(&PhotoLight::default(), &off);
        assert!(start.enabled);
        assert_eq!(
            (start.temperature, start.tint, start.lift, start.gamma, start.gain, start.vibrance),
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            "the old values switched off stay off"
        );
        // switched on: from what is drawn
        assert_eq!(edit_start(&PhotoLight::default(), &prefs()), prefs());
    }

    #[test]
    fn closing_the_photo_mode_gives_back_preferences_look() {
        // (`app::snapshot::photo_light` resets the whole PhotoLight)
        let light = PhotoLight {
            tonemapper: Some(4),
            grade: Some(prefs()),
            ..PhotoLight::default()
        };
        assert!(is_set(&light));
        assert!(!is_set(&PhotoLight::default()));
    }

    #[test]
    fn keep_copies_the_overlay_and_clears_it() {
        let mut light = PhotoLight {
            tonemapper: Some(5),
            grade: Some(prefs()),
            ..PhotoLight::default()
        };
        let (mut t, mut g) = (1, Grade::default());
        assert!(keep(&mut light, &mut t, &mut g));
        assert_eq!((t, g), (5, prefs()));
        assert!(!is_set(&light));
        // nothing set: nothing changed
        assert!(!keep(&mut light, &mut t, &mut g));
        assert_eq!((t, g), (5, prefs()));
    }

    #[test]
    fn profile_values_come_back_in_range() {
        let wild = Grade {
            enabled: true,
            temperature: 9.0,
            tint: f32::NAN,
            lift: -3.0,
            ..Grade::default()
        };
        let (t, g) = sanitize(Some(200), Some(wild));
        assert_eq!(t, None);
        let g = g.expect("kept");
        assert_eq!((g.temperature, g.tint, g.lift), (1.0, 0.0, -1.0));
        assert_eq!(sanitize(Some(2), None), (Some(2), None));
    }

    #[test]
    fn the_presets_sit_on_the_grid() {
        for wb in Wb::ALL {
            if let Some(t) = wb.temperature() {
                assert!((t / WB_STEP - (t / WB_STEP).round()).abs() < 1e-4, "{wb:?}");
                assert_eq!(
                    wb_of(
                        &{
                            let mut l = PhotoLight::default();
                            set_wb(&mut l, &Grade::default(), wb);
                            l
                        },
                        &Grade::default()
                    )
                    .map(|w| w == wb || (wb == Wb::Daylight && w == Wb::Auto)),
                    Some(true)
                );
            }
        }
    }

    #[test]
    fn steps_clamp_at_ten() {
        assert_eq!(step(0.95, 1), 1.0);
        assert_eq!(step(1.0, 1), 1.0);
        assert_eq!(step(-1.0, -1), -1.0);
        assert!((step(0.2, -1) - 0.1).abs() < 1e-6);
        assert_eq!(steps(f32::NAN), 0);
        assert_eq!(steps(7.0), 10);
    }

    #[test]
    fn auto_with_a_neutral_rest_clears_the_overlay() {
        let mut light = PhotoLight::default();
        set_wb(&mut light, &Grade::default(), Wb::Shade);
        assert!(light.grade.is_some());
        set_wb(&mut light, &Grade::default(), Wb::Auto);
        assert_eq!(light.grade, None);
        // a style of its own: the overlay stays, its white balance neutral
        let mut light = PhotoLight {
            tonemapper: Some(5),
            ..PhotoLight::default()
        };
        set_wb(&mut light, &Grade::default(), Wb::Tungsten);
        set_wb(&mut light, &Grade::default(), Wb::Auto);
        let g = light.grade.expect("kept with the curve");
        assert_eq!((g.temperature, g.tint), (0.0, 0.0));
    }

    #[test]
    fn a_white_balance_edit_never_touches_the_style() {
        let mut light = PhotoLight {
            grade: Some(Grade {
                enabled: true,
                lift: 0.25,
                gamma: -0.5,
                gain: 0.75,
                vibrance: 0.5,
                ..Grade::default()
            }),
            ..PhotoLight::default()
        };
        set_temperature(&mut light, &prefs(), 0.43);
        set_tint(&mut light, &prefs(), -0.2);
        set_wb(&mut light, &prefs(), Wb::Cloudy);
        let g = light.grade.expect("set");
        assert_eq!((g.lift, g.gamma, g.gain, g.vibrance), (0.25, -0.5, 0.75, 0.5));
        assert_eq!((steps(g.temperature), steps(g.tint)), (2, 0));
    }

    #[test]
    fn each_style_round_trips_through_matching() {
        for prefs in [Grade::default(), prefs()] {
            for (style, _, _) in STYLES {
                let mut light = PhotoLight::default();
                apply_style(&mut light, &prefs, style);
                assert_eq!(matching(&light, &prefs), style, "{style:?}");
            }
            let mut light = PhotoLight::default();
            assert_eq!(matching(&light, &prefs), Style::AsViewer);
            apply_style(&mut light, &prefs, Style::Film);
            apply_style(&mut light, &prefs, Style::AsViewer);
            assert_eq!(matching(&light, &prefs), Style::AsViewer);
            assert!(!is_set(&light), "back to the viewer: no overlay");
        }
    }

    #[test]
    fn vivid_is_on_the_grid_and_any_step_is_custom() {
        let (_, _, v) = STYLES[6];
        for x in v {
            assert_eq!(x, on_grid(x) as f32 * FINE_STEP);
        }
        let mut light = PhotoLight::default();
        apply_style(&mut light, &Grade::default(), Style::Vivid);
        fine_step(&mut light, &Grade::default(), Fine::Shadows, 1);
        assert_eq!(matching(&light, &Grade::default()), Style::Custom);
        assert_eq!(Fine::Shadows.steps(&light, &Grade::default()), 1);
        for _ in 0..9 {
            fine_step(&mut light, &Grade::default(), Fine::Colour, 1);
        }
        assert_eq!(Fine::Colour.steps(&light, &Grade::default()), 4, "clamped");
    }

    #[test]
    fn standard_is_aces_the_default_curve_and_a_style_keeps_the_white_balance() {
        assert_eq!(STYLES[0].1, crate::settings::Settings::default().tonemapper);
        let mut light = PhotoLight::default();
        set_wb(&mut light, &Grade::default(), Wb::Shade);
        apply_style(&mut light, &Grade::default(), Style::Classic);
        assert_eq!(wb_of(&light, &Grade::default()), Some(Wb::Shade));
    }
}
