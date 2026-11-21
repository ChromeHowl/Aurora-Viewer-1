//! Quick light for photos (Firestorm's Phototools, floater_phototools.xml:
//! Quick Environments, the sky, water and day cycle lists of its Env tab,
//! Pause Clouds, Enable Attached Lights, Exposure, and the sun of "Personal
//! lighting"): a time of day, a sky, a water, a day cycle, the sun's
//! azimuth and elevation, an exposure, the clouds held still and the lights
//! worn by avatars (face lights) off, the softness and the darkness of the
//! sun's shadows, for the photo only, while the photo
//! window is open. Firestorm changes the environment, the cloud scroll and
//! RenderAttachedLights until they are changed back; here nothing is saved
//! and closing the window gives back the user's own (`app::snapshot`; the
//! environment is a layer of its own, `world::eep_photo`), unless "Keep
//! this light" makes the photo's environment theirs.

use crate::world::eep_photo::PhotoChoice;

/// The photo window's light, all at the user's own by default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhotoLight {
    /// The photo's time of day (1 sunrise, 2 noon, 3 sunset, 4 midnight),
    /// sky, water, day cycle and sun over the environment shown.
    pub env: PhotoChoice,
    /// The exposure for the photo; None: Preferences' one.
    pub exposure: Option<f32>,
    /// The softness of the sun's shadows for the photo (soft shadows on);
    /// None: Preferences' one.
    pub shadow_softness: Option<f32>,
    /// How dark the sun's shadows are for the photo; None: Preferences' one.
    pub shadow_intensity: Option<f32>,
    /// The clouds stop moving (LLEnvironment::pauseCloudScroll). A switch
    /// of the session, shared with Phototools and kept when the window
    /// closes (`closed`).
    pub clouds_paused: bool,
    /// The lights worn by avatars light the scene
    /// (LLPipeline::sRenderAttachedLights).
    pub attached_lights: bool,
    /// The photo's tone curve (`Settings::tonemapper`'s numbers); None:
    /// Preferences' one (`snapshot::style`).
    pub tonemapper: Option<u8>,
    /// The photo's colour grade, white balance included; None: Preferences'
    /// one.
    pub grade: Option<crate::settings::render_extras::Grade>,
}

impl Default for PhotoLight {
    fn default() -> Self {
        Self {
            env: PhotoChoice::default(),
            exposure: None,
            shadow_softness: None,
            shadow_intensity: None,
            clouds_paused: false,
            attached_lights: true,
            tonemapper: None,
            grade: None,
        }
    }
}

impl PhotoLight {
    /// The light while the photo window is closed: everything back to the
    /// user's own, but the paused clouds, which are a switch of the session
    /// (never saved) that Phototools shares with the photo window, as the
    /// Legacy Viewer's pauseCloudScroll lasts the session.
    pub fn closed(self) -> Self {
        Self {
            clouds_paused: self.clouds_paused,
            ..Self::default()
        }
    }
}

/// The range of the exposure (`Settings::exposure`).
pub const EXPOSURE: std::ops::RangeInclusive<f32> = 0.25..=4.0;
/// The ranges of the shadow softness and intensity (`Settings`).
pub const SHADOW_SOFTNESS: std::ops::RangeInclusive<f32> = 0.0..=crate::settings::SHADOW_SOFTNESS_MAX;
pub const SHADOW_INTENSITY: std::ops::RangeInclusive<f32> = 0.0..=1.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_the_photo_window_keeps_only_the_paused_clouds() {
        let light = PhotoLight {
            exposure: Some(2.0),
            clouds_paused: true,
            attached_lights: false,
            ..Default::default()
        };
        let closed = light.closed();
        assert!(closed.clouds_paused, "a session switch, shared with Phototools");
        assert_eq!(
            closed,
            PhotoLight {
                clouds_paused: true,
                ..Default::default()
            }
        );
        assert!(!PhotoLight::default().closed().clouds_paused);
    }
}
