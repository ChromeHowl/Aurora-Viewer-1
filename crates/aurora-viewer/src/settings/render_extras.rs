//! Rendering extras of Preferences › Graphics: No Post-processing while
//! building (RenderDisablePostProcessing, session only), mirror
//! resolution and update rate (RenderHeroProbeResolution,
//! RenderHeroProbeUpdateRate), shadow resolution scale
//! (RenderShadowResolutionScale), terrain texture scale
//! (RenderTerrainScale), the water a region's environment hides
//! (FSAllowEEPWaterDerender), and Aurora's own colour
//! grade, debanding, scene lights, light strength and depth of field
//! extras. Defaults leave the picture as it was.

use serde::{Deserialize, Serialize};

/// RenderTerrainScale's default and bounds (settings.xml, the slider of
/// panel_preferences_graphics1.xml): metres per repeat of the terrain's
/// detail textures.
pub const TERRAIN_SCALE: f32 = 12.0;
pub const TERRAIN_SCALE_RANGE: std::ops::RangeInclusive<f32> = 1.0..=24.0;
/// RenderShadowResolutionScale's slider in the Legacy Viewer's preferences.
pub const SHADOW_SCALE_RANGE: std::ops::RangeInclusive<f32> = 1.0..=4.0;
/// The choices of RenderHeroProbeResolution (0: from the mirror quality).
pub const MIRROR_RESOLUTIONS: [u32; 5] = [0, 256, 512, 1024, 2048];
/// The global light strength's bounds.
pub const LIGHT_STRENGTH_RANGE: std::ops::RangeInclusive<f32> = 0.0..=4.0;
/// Bokeh dispersion's bounds.
pub const DISPERSION_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;

/// Leaves the water out of a frame's lists, with its reflection, its
/// shadows and its events, when the environment hides it
/// (`eep_hides_water`). The Advanced › Rendering types switches are
/// `scene::render_types`.
pub fn strip_water(lists: &mut aurora_render::DrawLists) {
    lists.water.clear();
    lists.water_exclusion.clear();
    lists.water_events.clear();
    lists.water_contacts.clear();
}

/// Aurora's colour grade (Preferences › Graphics › Colour grade); every
/// slider from -1 to 1, 0 changing nothing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub enabled: bool,
    pub temperature: f32,
    pub tint: f32,
    pub lift: f32,
    pub gamma: f32,
    pub gain: f32,
    pub vibrance: f32,
}

impl Default for Grade {
    fn default() -> Self {
        Grade {
            enabled: false,
            temperature: 0.0,
            tint: 0.0,
            lift: 0.0,
            gamma: 0.0,
            gain: 0.0,
            vibrance: 0.0,
        }
    }
}

impl Grade {
    /// What the post pass gets: neutral while off.
    pub fn render(&self) -> aurora_render::grade::Grade {
        if !self.enabled {
            return aurora_render::grade::Grade::default();
        }
        aurora_render::grade::Grade {
            temperature: self.temperature,
            tint: self.tint,
            lift: self.lift,
            gamma: self.gamma,
            gain: self.gain,
            vibrance: self.vibrance,
        }
    }
}

/// The settings of this module (`Settings::render_extras`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderExtras {
    /// RenderHeroProbeResolution: the mirror image's longer side in pixels,
    /// 0 to keep the mirror quality's share of the screen (the default:
    /// the picture as before).
    pub mirror_resolution: u32,
    /// RenderHeroProbeUpdateRate: a mirror drawn every this many frames
    /// (1, every frame, by default: the Legacy Viewer's is 2).
    pub mirror_update_rate: u32,
    /// RenderShadowResolutionScale: the sun's shadow maps this much larger
    /// than the shadow quality's (1 to 4).
    pub shadow_resolution_scale: f32,
    /// RenderTerrainScale: metres per repeat of the classic terrain's
    /// detail textures (12; 1 to 24). Applied at once, where the Legacy
    /// Viewer asks for a restart.
    pub terrain_scale: f32,
    /// FSAllowEEPWaterDerender (off): an environment whose water normal map
    /// is the transparent texture hides the water.
    pub eep_water_derender: bool,
    /// Lights of the scene, those no avatar wears (on).
    pub scene_lights: bool,
    /// Every local light's intensity times this (1; 0 to 4).
    pub light_strength: f32,
    pub grade: Grade,
    /// Debanding of the image, in 8-bit levels (0 off; up to 2).
    pub deband: f32,
    /// Depth of field: what is in front of the focus is blurred too (on).
    pub dof_front_blur: bool,
    /// Depth of field: the bokeh's red and blue spread apart (0 none).
    pub dof_dispersion: f32,
    /// Off: no exposure nor tone curve while the build tools are open
    /// (RenderDisablePostProcessing, for those making materials). Session
    /// only: every session starts with it on.
    #[serde(skip)]
    pub post_processing: bool,
}

impl Default for RenderExtras {
    fn default() -> Self {
        RenderExtras {
            mirror_resolution: 0,
            mirror_update_rate: 1,
            shadow_resolution_scale: 1.0,
            terrain_scale: TERRAIN_SCALE,
            eep_water_derender: false,
            scene_lights: true,
            light_strength: 1.0,
            grade: Grade::default(),
            deband: 0.0,
            dof_front_blur: true,
            dof_dispersion: 0.0,
            post_processing: true,
        }
    }
}

fn finite_in(v: f32, range: std::ops::RangeInclusive<f32>, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(*range.start(), *range.end())
    } else {
        default
    }
}

impl RenderExtras {
    /// The bounds of a settings file.
    pub fn sanitize(&mut self) {
        if !MIRROR_RESOLUTIONS.contains(&self.mirror_resolution) {
            // the nearest choice above, the largest past it
            self.mirror_resolution = MIRROR_RESOLUTIONS
                .iter()
                .copied()
                .find(|r| *r >= self.mirror_resolution)
                .unwrap_or(2048);
        }
        self.mirror_update_rate = self.mirror_update_rate.clamp(1, 6);
        self.shadow_resolution_scale = finite_in(self.shadow_resolution_scale, SHADOW_SCALE_RANGE, 1.0);
        self.terrain_scale = finite_in(self.terrain_scale, TERRAIN_SCALE_RANGE, TERRAIN_SCALE);
        self.light_strength = finite_in(self.light_strength, LIGHT_STRENGTH_RANGE, 1.0);
        for v in [
            &mut self.grade.temperature,
            &mut self.grade.tint,
            &mut self.grade.lift,
            &mut self.grade.gamma,
            &mut self.grade.gain,
            &mut self.grade.vibrance,
        ] {
            *v = finite_in(*v, -1.0..=1.0, 0.0);
        }
        self.deband = finite_in(self.deband, 0.0..=2.0, 0.0);
        self.dof_dispersion = finite_in(self.dof_dispersion, DISPERSION_RANGE, 0.0);
    }

    /// The sun's shadow map resolution for the shadow quality's
    /// `resolution`, scaled (the renderer bounds it by the GPU's largest
    /// texture).
    pub fn shadow_resolution(&self, resolution: u32) -> u32 {
        if resolution == 0 {
            return 0;
        }
        let scale = finite_in(self.shadow_resolution_scale, SHADOW_SCALE_RANGE, 1.0);
        ((resolution as f32 * scale).round() as u32).min(16_384)
    }
}

/// LLViewerFetchedTexture's IMG_TRANSPARENT (UIImgTransparentUUID): the
/// water normal map by which an environment asks for no water.
pub const IMG_TRANSPARENT: uuid::Uuid = uuid::uuid!("8dcd4a48-2d37-4909-9f78-f7a9eb4ef903");

/// Whether the water is hidden by the environment
/// (LLEnvironment::updateEnvironment with FSAllowEEPWaterDerender): only
/// while the option is on and the water's normal map is IMG_TRANSPARENT.
pub fn eep_hides_water(allowed: bool, water_normal_map: uuid::Uuid) -> bool {
    allowed && water_normal_map == IMG_TRANSPARENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_picture_and_old_files_load() {
        let d: RenderExtras = serde_json::from_str("{}").expect("an empty object");
        assert_eq!(d, RenderExtras::default());
        assert_eq!((d.mirror_resolution, d.mirror_update_rate), (0, 1));
        assert_eq!(d.shadow_resolution(2048), 2048);
        assert_eq!(d.terrain_scale, 12.0);
        assert!(d.scene_lights && d.dof_front_blur && !d.eep_water_derender);
        assert_eq!((d.light_strength, d.deband, d.dof_dispersion), (1.0, 0.0, 0.0));
        assert_eq!(d.grade.render(), aurora_render::grade::Grade::default());
        assert!(d.post_processing);
        // the session's choice is never written
        let mut s = d.clone();
        s.post_processing = false;
        let text = serde_json::to_string(&s).expect("serialised");
        assert!(!text.contains("post_processing"));
    }

    #[test]
    fn a_settings_file_is_bounded() {
        let mut s = RenderExtras {
            mirror_resolution: 700,
            mirror_update_rate: 0,
            shadow_resolution_scale: f32::NAN,
            terrain_scale: 100.0,
            light_strength: -2.0,
            deband: 9.0,
            dof_dispersion: f32::INFINITY,
            ..Default::default()
        };
        s.grade.gain = 3.0;
        s.grade.lift = f32::NAN;
        s.sanitize();
        assert_eq!((s.mirror_resolution, s.mirror_update_rate), (1024, 1));
        assert_eq!(s.shadow_resolution_scale, 1.0);
        assert_eq!((s.terrain_scale, s.light_strength, s.deband), (24.0, 0.0, 2.0));
        assert_eq!(s.dof_dispersion, 0.0);
        assert_eq!((s.grade.gain, s.grade.lift), (1.0, 0.0));
        s.mirror_resolution = 9000;
        s.mirror_update_rate = 40;
        s.sanitize();
        assert_eq!((s.mirror_resolution, s.mirror_update_rate), (2048, 6));
    }

    #[test]
    fn the_shadow_scale_multiplies_the_quality_and_keeps_shadows_off() {
        let s = RenderExtras {
            shadow_resolution_scale: 1.5,
            ..Default::default()
        };
        assert_eq!(s.shadow_resolution(2048), 3072);
        assert_eq!(s.shadow_resolution(0), 0, "no shadows stay none");
        let big = RenderExtras {
            shadow_resolution_scale: 4.0,
            ..Default::default()
        };
        assert_eq!(big.shadow_resolution(4096), 16_384);
    }

    #[test]
    fn a_water_the_environment_hides_leaves_the_lists() {
        let cmd = aurora_render::DrawCmd {
            index_count: 3,
            first_index: 0,
            base_vertex: 0,
            record: 1,
            bounds: [0.0; 4],
        };
        let mut l = aurora_render::DrawLists::default();
        l.terrain.push(cmd);
        l.water.push(cmd);
        l.opaque.push(cmd);
        strip_water(&mut l);
        assert!(l.water.is_empty());
        assert_eq!((l.terrain.len(), l.opaque.len()), (1, 1));
    }

    #[test]
    fn only_the_transparent_normal_map_hides_the_water_and_only_when_allowed() {
        assert!(eep_hides_water(true, IMG_TRANSPARENT));
        assert!(!eep_hides_water(false, IMG_TRANSPARENT));
        assert!(!eep_hides_water(true, uuid::Uuid::nil()));
        assert!(!eep_hides_water(true, uuid::Uuid::from_u128(7)));
    }

    #[test]
    fn the_grade_is_neutral_while_off() {
        let mut g = Grade {
            temperature: 0.5,
            vibrance: 0.3,
            ..Default::default()
        };
        assert_eq!(g.render(), aurora_render::grade::Grade::default());
        g.enabled = true;
        assert_eq!(g.render().temperature, 0.5);
        assert_eq!(g.render().vibrance, 0.3);
    }
}
