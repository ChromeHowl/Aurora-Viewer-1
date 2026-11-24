//! Small preferences of the Legacy Viewer that Aurora had no setting for:
//! the interface's tooltip delay, the size of the HUDs, scripts opening the
//! map and the simulator version in the top bar. Each field names its Legacy
//! setting (indra/newview/app_settings/settings.xml) and keeps its default,
//! unless the comment says why not.

use serde::{Deserialize, Serialize};

/// ToolTipDelay: seconds before a tooltip of the interface shows.
pub const TOOLTIP_DELAY: f32 = 0.7;
/// HUDScaleFactor's range in Preferences (panel_preferences_UI.xml).
pub const HUD_SCALE: std::ops::RangeInclusive<f32> = 1.0..=2.0;

/// The preferences (see the module).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LegacyPrefs {
    /// ToolTipDelay (0.7 s): how long the pointer rests on a control before
    /// its tooltip shows.
    pub tooltip_delay: f32,
    /// HUDScaleFactor (1, 1 to 2): the size of every HUD on the screen
    /// (LLAgentCamera::getAgentHUDTargetZoom).
    pub hud_scale: f32,
    /// ScriptsCanShowUI (on): llMapDestination opens the world map.
    pub scripts_can_show_ui: bool,
    /// FSStatusbarShowSimulatorVersion (off): the simulator's version after
    /// the region's name in the top bar.
    pub sim_version_in_bar: bool,
}

impl Default for LegacyPrefs {
    fn default() -> Self {
        LegacyPrefs {
            tooltip_delay: TOOLTIP_DELAY,
            hud_scale: 1.0,
            scripts_can_show_ui: true,
            sim_version_in_bar: false,
        }
    }
}

fn finite_or(value: f32, default: f32) -> f32 {
    if value.is_finite() { value } else { default }
}

impl LegacyPrefs {
    /// Bounds a file's values.
    pub fn sanitize(&mut self) {
        self.tooltip_delay = finite_or(self.tooltip_delay, TOOLTIP_DELAY).clamp(0.0, 5.0);
        self.hud_scale = finite_or(self.hud_scale, 1.0).clamp(*HUD_SCALE.start(), *HUD_SCALE.end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_legacy_viewers() {
        let p = LegacyPrefs::default();
        assert_eq!(p.tooltip_delay, 0.7);
        assert_eq!(p.hud_scale, 1.0);
        assert!(!p.sim_version_in_bar);
        assert!(p.scripts_can_show_ui);
    }

    #[test]
    fn an_old_file_loads_with_the_defaults() {
        let p: LegacyPrefs = serde_json::from_str(r#"{"hud_scale": 1.5}"#).expect("partial file");
        assert_eq!(p.hud_scale, 1.5);
        assert_eq!(p.tooltip_delay, TOOLTIP_DELAY);
    }

    #[test]
    fn sanitize_bounds_every_value() {
        let mut p = LegacyPrefs {
            tooltip_delay: f32::NAN,
            hud_scale: 9.0,
            ..LegacyPrefs::default()
        };
        p.sanitize();
        assert_eq!(p.tooltip_delay, TOOLTIP_DELAY);
        assert_eq!(p.hud_scale, 2.0);
    }
}
