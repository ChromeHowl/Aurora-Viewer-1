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
}
