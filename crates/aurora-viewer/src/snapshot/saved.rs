//! Photo profiles: named sets of the photo window's own settings, saved,
//! loaded and deleted from the window, kept in settings.json beside the
//! window's settings, as the graphics profiles are
//! (`graphics_presets::saved`). A profile holds the photo's size and
//! proportions, its file format, the supersampling and Photo quality, the
//! window's lens, the photo's exposure and quick light switches, its looks,
//! the isolation chosen and the guide, and the photo's own tone curve and
//! colour grade (its picture style and white balance, `style`). Nothing of
//! the graphics profiles' own (the Preferences' depth of field, exposure,
//! tone curve and grade, the rendering levels) and nothing of the camera's
//! place (the camera presets, `camera::presets`).
//!
//! A profile is external data: each field is read on its own, one that
//! does not parse (a later version's value) left at its default, and the
//! values brought back into their ranges when it is applied.

use super::isolate::{self, Isolation};
use super::light::{EXPOSURE, PhotoLight};
use super::quality::Supersampling;
use super::{Aspect, FileFormat, Guide, Resolution, Settings, looks};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

macro_rules! profile_fields {
    ($($field:ident: $ty:ty),+ $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct Profile {
            $(pub $field: $ty,)+
        }

        impl Profile {
            /// A profile read field by field: what does not parse keeps
            /// its default.
            fn from_value(v: &serde_json::Value) -> Profile {
                let mut p = Profile::default();
                if let Some(map) = v.as_object() {
                    $(
                        if let Some(Ok(x)) = map.get(stringify!($field)).map(<$ty>::deserialize) {
                            p.$field = x;
                        }
                    )+
                }
                p
            }
        }
    };
}

profile_fields! {
    aspect: Aspect,
    resolution: Resolution,
    custom: [u32; 2],
    keep_ratio: bool,
    format: FileFormat,
    jpeg_quality: u8,
    supersampling: Supersampling,
    quality: bool,
    dof: crate::dof::PhotoDof,
    exposure: Option<f32>,
    clouds_paused: bool,
    attached_lights: bool,
    looks: Vec<looks::Layer>,
    look_settings: BTreeMap<String, Vec<f32>>,
    isolation: isolate::Mode,
    transparent: bool,
    guide: Guide,
    tonemapper: Option<u8>,
    grade: Option<crate::settings::render_extras::Grade>,
}

impl Default for Profile {
    fn default() -> Self {
        Profile::capture(&Settings::default(), &PhotoLight::default(), &Isolation::default())
    }
}

impl Profile {
    /// The window's settings now.
    pub fn capture(s: &Settings, light: &PhotoLight, iso: &Isolation) -> Self {
        let looks = crate::camera::presets::looks_of(&s.looks, &s.look_settings);
        Profile {
            aspect: s.aspect,
            resolution: s.resolution,
            custom: s.custom,
            keep_ratio: s.keep_ratio,
            format: s.format,
            jpeg_quality: s.jpeg_quality,
            supersampling: s.supersampling,
            quality: s.quality,
            dof: s.dof.clone(),
            exposure: light.exposure,
            clouds_paused: light.clouds_paused,
            attached_lights: light.attached_lights,
            looks: looks.layers,
            look_settings: looks.settings,
            isolation: iso.mode,
            transparent: iso.transparent,
            guide: s.guide,
            tonemapper: light.tonemapper,
            grade: light.grade,
        }
    }

    /// The profile's settings made the window's, in their ranges. Photo
    /// quality is only switched on once its warning was accepted; the
    /// settings of looks the profile does not use are kept.
    pub fn apply(&self, s: &mut Settings, light: &mut PhotoLight, iso: &mut Isolation) {
        s.aspect = self.aspect;
        s.resolution = self.resolution;
        s.custom = self.custom.map(super::clamp_side);
        s.keep_ratio = self.keep_ratio;
        s.format = self.format;
        s.jpeg_quality = self.jpeg_quality.min(100);
        s.supersampling = self.supersampling;
        s.quality = self.quality && s.quality_warned;
        s.dof = self.dof.clone();
        s.dof.sanitize();
        light.exposure = self
            .exposure
            .filter(|e| e.is_finite())
            .map(|e| e.clamp(*EXPOSURE.start(), *EXPOSURE.end()));
        light.clouds_paused = self.clouds_paused;
        light.attached_lights = self.attached_lights;
        s.looks = self.looks.clone();
        for (id, values) in &self.look_settings {
            s.look_settings.insert(id.clone(), values.clone());
        }
        iso.set_mode(self.isolation);
        iso.set_transparent(self.transparent);
        s.guide = self.guide;
        (light.tonemapper, light.grade) = super::style::sanitize(self.tonemapper, self.grade);
    }
}

/// A part of the window's settings a profile holds, reset on its own (the
/// Legacy Viewer's photo tools reset a tab: here a part of the window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// Proportions and size.
    Format,
    /// File format and JPEG quality.
    File,
    /// Photo quality and supersampling.
    Quality,
    /// The window's depth of field.
    Lens,
    /// Exposure and the quick light switches.
    Light,
    /// The looks and their settings.
    Looks,
    /// What the photo shows.
    Isolation,
    /// The composition guide.
    Guide,
    /// The photo's picture style and white balance.
    Style,
}

impl Part {
    pub const ALL: [Part; 9] = [
        Part::Format,
        Part::File,
        Part::Quality,
        Part::Lens,
        Part::Light,
        Part::Looks,
        Part::Isolation,
        Part::Guide,
        Part::Style,
    ];
}

/// "Reset all" (None) or "Reset tab" (Some): the part's settings, or all
/// those a profile holds, back to their defaults, the others as they are.
/// The looks' settings are forgotten with them.
pub fn reset(part: Option<Part>, s: &mut Settings, light: &mut PhotoLight, iso: &mut Isolation) {
    let d = Profile::default();
    let mut p = Profile::capture(s, light, iso);
    let all = part.is_none();
    let is = |x: Part| all || part == Some(x);
    if is(Part::Format) {
        (p.aspect, p.resolution, p.custom, p.keep_ratio) = (d.aspect, d.resolution, d.custom, d.keep_ratio);
    }
    if is(Part::File) {
        (p.format, p.jpeg_quality) = (d.format, d.jpeg_quality);
    }
    if is(Part::Quality) {
        (p.supersampling, p.quality) = (d.supersampling, d.quality);
    }
    if is(Part::Lens) {
        p.dof = d.dof.clone();
    }
    if is(Part::Light) {
        (p.exposure, p.clouds_paused, p.attached_lights) = (d.exposure, d.clouds_paused, d.attached_lights);
    }
    if is(Part::Looks) {
        p.looks = d.looks.clone();
        p.look_settings = d.look_settings.clone();
        s.look_settings.clear();
    }
    if is(Part::Isolation) {
        (p.isolation, p.transparent) = (d.isolation, d.transparent);
    }
    if is(Part::Guide) {
        p.guide = d.guide;
    }
    if is(Part::Style) {
        (p.tonemapper, p.grade) = (d.tonemapper, d.grade);
    }
    p.apply(s, light, iso);
}

/// Names are labels, never file paths: one line, at most 64 characters.
pub fn valid_name(name: &str) -> bool {
    crate::graphics_presets::saved::valid_name(name)
}

/// The profiles of settings.json, each read on its own (`Profile`): a
/// library that is not a map, or a name that is not valid, is left out
/// rather than the whole settings file refused.
pub fn lenient<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Profile>, D::Error> {
    let raw = serde_json::Value::deserialize(d)?;
    Ok(raw
        .as_object()
        .map(|map| {
            map.iter()
                .filter(|(name, _)| valid_name(name))
                .map(|(name, v)| (name.trim().to_owned(), Profile::from_value(v)))
                .collect()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_round_trips_through_the_settings() {
        let mut s = Settings {
            aspect: Aspect::R4x5,
            resolution: Resolution::Custom,
            custom: [12_000, 15_000],
            format: FileFormat::Jpeg,
            jpeg_quality: 92,
            supersampling: Supersampling::X9,
            quality: true,
            quality_warned: true,
            guide: Guide::Center,
            ..Settings::default()
        };
        s.dof.enabled = true;
        let light = PhotoLight {
            exposure: Some(1.5),
            clouds_paused: true,
            attached_lights: false,
            ..PhotoLight::default()
        };
        let mut iso = Isolation::default();
        iso.set_mode(isolate::Mode::Background);
        iso.set_transparent(false);
        let profile = Profile::capture(&s, &light, &iso);
        s.profiles.insert("Portrait 16K".into(), profile.clone());

        let json = serde_json::to_string(&s).expect("serialize");
        let mut loaded: Settings = serde_json::from_str(&json).expect("load");
        assert_eq!(loaded.profiles["Portrait 16K"], profile);

        // applied over other settings: the window as it was saved
        let mut other_light = PhotoLight::default();
        let mut other_iso = Isolation::default();
        let saved = loaded.profiles["Portrait 16K"].clone();
        loaded.aspect = Aspect::Window;
        loaded.supersampling = Supersampling::Off;
        loaded.folder = Some("keep/me".into());
        saved.apply(&mut loaded, &mut other_light, &mut other_iso);
        assert_eq!(Profile::capture(&loaded, &other_light, &other_iso), profile);
        assert_eq!(loaded.folder, Some("keep/me".into()), "what a profile does not hold is kept");
        // Photo quality never switched on before its warning was accepted
        loaded.quality_warned = false;
        loaded.quality = false;
        saved.apply(&mut loaded, &mut other_light, &mut other_iso);
        assert!(!loaded.quality);

        // nothing of the graphics profiles
        let value = serde_json::to_value(&profile).expect("value");
        for unrelated in ["draw_distance", "shadows", "fov", "folder", "base_name", "camera"] {
            assert!(value.get(unrelated).is_none(), "{unrelated}");
        }
    }

    #[test]
    fn reset_all_or_one_part_brings_back_the_defaults() {
        let mut s = Settings {
            aspect: Aspect::R1x1,
            format: FileFormat::Bmp,
            supersampling: Supersampling::X4,
            guide: Guide::Center,
            quality_warned: true,
            quality: true,
            folder: Some("keep/me".into()),
            ..Settings::default()
        };
        s.looks = vec![looks::Layer::new("Sepia")];
        s.look_settings.insert("aurora:haze".into(), vec![0.5]);
        let mut light = PhotoLight {
            exposure: Some(2.0),
            ..PhotoLight::default()
        };
        let mut iso = Isolation::default();
        iso.set_mode(isolate::Mode::Own);

        reset(Some(Part::File), &mut s, &mut light, &mut iso);
        assert_eq!(s.format, FileFormat::Png);
        assert_eq!(
            (s.aspect, s.guide, s.supersampling),
            (Aspect::R1x1, Guide::Center, Supersampling::X4),
            "the rest kept"
        );
        assert_eq!(light.exposure, Some(2.0));

        reset(Some(Part::Looks), &mut s, &mut light, &mut iso);
        assert!(s.looks.is_empty() && s.look_settings.is_empty());
        assert_eq!(iso.mode, isolate::Mode::Own);

        reset(None, &mut s, &mut light, &mut iso);
        let defaults = Profile::default();
        assert_eq!(Profile::capture(&s, &light, &iso), defaults);
        assert_eq!(s.folder, Some("keep/me".into()), "what a profile does not hold is kept");
        for part in Part::ALL {
            let (mut s2, mut l2, mut i2) = (Settings::default(), PhotoLight::default(), Isolation::default());
            reset(Some(part), &mut s2, &mut l2, &mut i2);
            assert_eq!(
                Profile::capture(&s2, &l2, &i2),
                defaults,
                "{part:?} of the defaults changes nothing"
            );
        }
    }

    #[test]
    fn the_photo_style_is_held_reset_and_old_profiles_load_without_it() {
        use crate::settings::render_extras::Grade;
        let style = Grade {
            enabled: true,
            temperature: 0.4,
            gain: 0.25,
            ..Grade::default()
        };
        let s = Settings::default();
        let mut light = PhotoLight {
            tonemapper: Some(5),
            grade: Some(style),
            ..PhotoLight::default()
        };
        let mut iso = Isolation::default();
        let profile = Profile::capture(&s, &light, &iso);
        assert_eq!((profile.tonemapper, profile.grade), (Some(5), Some(style)));
        // applied over a photo without a style: the style comes back
        let (mut l2, mut s2) = (PhotoLight::default(), Settings::default());
        profile.apply(&mut s2, &mut l2, &mut iso);
        assert_eq!((l2.tonemapper, l2.grade), (Some(5), Some(style)));
        // a profile saved before the style: none (Preferences' look)
        let old: Settings = serde_json::from_str(r#"{"profiles": {"Old": {"guide": "Center"}}}"#).expect("load");
        assert_eq!((old.profiles["Old"].tonemapper, old.profiles["Old"].grade), (None, None));
        // its part alone, or everything, back to Preferences' look
        let mut s = Settings {
            guide: Guide::Center,
            ..Settings::default()
        };
        reset(Some(Part::Style), &mut s, &mut light, &mut iso);
        assert_eq!((light.tonemapper, light.grade), (None, None));
        assert_eq!(s.guide, Guide::Center, "the rest kept");
        light.tonemapper = Some(2);
        reset(None, &mut s, &mut light, &mut iso);
        assert_eq!(light.tonemapper, None);
    }

    #[test]
    fn profiles_load_tolerantly() {
        let json = r#"{
            "quality": true,
            "profiles": {
                "Good": { "supersampling": "X16", "custom": [99999, 5], "exposure": 40.0, "jpeg_quality": 150 },
                "Future": { "resolution": "K32", "format": "Avif", "guide": "Center", "extra": 1 },
                "Broken": 7,
                "": { "guide": "Center" }
            }
        }"#;
        let mut s: Settings = serde_json::from_str(json).expect("settings still load");
        assert!(s.quality);
        assert_eq!(
            s.profiles.keys().map(String::as_str).collect::<Vec<_>>(),
            ["Broken", "Future", "Good"]
        );
        let future = &s.profiles["Future"];
        assert_eq!(
            (future.resolution, future.format, future.guide),
            (Resolution::Uhd, FileFormat::Png, Guide::Center)
        );
        assert_eq!(s.profiles["Broken"], Profile::default());
        // applied: the values in their ranges
        let good = s.profiles["Good"].clone();
        let (mut light, mut iso) = (PhotoLight::default(), Isolation::default());
        good.apply(&mut s, &mut light, &mut iso);
        assert_eq!(s.supersampling, Supersampling::X16);
        assert_eq!(s.custom, [super::super::MAX_SIDE, super::super::MIN_SIDE]);
        assert_eq!(s.jpeg_quality, 100);
        assert_eq!(light.exposure, Some(*EXPOSURE.end()));
        // a library that is not a map: none, the settings kept
        let s: Settings = serde_json::from_str(r#"{"profiles": [1, 2], "jpeg_quality": 60}"#).expect("load");
        assert!(s.profiles.is_empty());
        assert_eq!(s.jpeg_quality, 60);
    }
}
