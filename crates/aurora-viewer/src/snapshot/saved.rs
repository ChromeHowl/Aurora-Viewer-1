//! Photo profiles: named sets of the photo window's own settings, saved,
//! loaded and deleted from the window, kept in settings.json beside the
//! window's settings, as the graphics profiles are
//! (`graphics_presets::saved`). A profile holds the photo's size and
//! proportions, its file format, the supersampling and Photo quality, the
//! window's lens, the photo's exposure and quick light switches, its looks,
//! the isolation chosen and the guide. Nothing the graphics profiles hold
//! (the Preferences' depth of field and exposure, the rendering levels) and
//! nothing of the camera's place (the camera presets, `camera::presets`).
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
    }
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
