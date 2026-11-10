//! Build tools options of the Legacy Viewer that Aurora lacked: the unlink
//! confirmation, the pivot point of the edit handles, the light radius and
//! the level of detail of the selection, and the selection filters
//! (Preferences › Build, the Build menu). Held in `BuildSettings::tools`;
//! every default is the Legacy Viewer's (settings.xml names in comments).

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// The pivot spinners' bounds (panel_preferences_firestorm.xml): a
/// percentage of the selection's size, or metres.
pub const PIVOT_RANGE: std::ops::RangeInclusive<f32> = -256.0..=256.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BuildTools {
    /// FSUnlinkConfirmEnabled: ask before unlinking.
    pub unlink_confirm: bool,
    /// MinObjectsForUnlinkConfirm: from this many prims selected (0 to 256).
    pub unlink_confirm_min: u32,
    /// FSBuildPrefs_PivotIsPercent: the pivot below is a percentage of the
    /// size (50 = the centre), else metres from the centre.
    pub pivot_percent: bool,
    /// FSBuildPrefs_PivotX / Y / Z.
    pub pivot: [f32; 3],
    /// RenderLightRadius: the range sphere of the selected lights.
    pub light_radius: bool,
    /// FSSelectCopyableOnly: rectangle selection takes copyable objects only.
    pub select_copyable: bool,
    /// FSSelectLockedOnly: rectangle selection takes locked objects only.
    pub select_locked: bool,
    /// FSSelectIncludeGroupOwned: rectangle selection takes group-owned
    /// objects too (on); off, it leaves them out.
    pub select_group_owned: bool,
}

impl Default for BuildTools {
    fn default() -> Self {
        BuildTools {
            unlink_confirm: true,
            unlink_confirm_min: 0,
            pivot_percent: true,
            pivot: [50.0; 3],
            light_radius: false,
            select_copyable: false,
            select_locked: false,
            select_group_owned: true,
        }
    }
}

impl BuildTools {
    /// LLSelectMgr::unlinkObjects: confirm when on, at least the minimum is
    /// selected, and some linkset would come apart (more prims than roots).
    pub fn unlink_needs_confirm(&self, prims: usize, roots: usize) -> bool {
        self.unlink_confirm && prims >= self.unlink_confirm_min as usize && prims > roots
    }

    /// LLManip::getPivotPoint's offset: from the centre of what the pivot
    /// stands on (`size` its full size, `rotation` its frame), a
    /// percentage of the size (50 % the centre) or metres along its axes.
    pub fn pivot_offset(&self, size: Vec3, rotation: Quat) -> Vec3 {
        let pivot = Vec3::from_array(self.pivot);
        let local = if self.pivot_percent {
            -size * 0.5 + size * (pivot * 0.01)
        } else {
            pivot
        };
        rotation * local
    }

    pub fn sanitize(&mut self) {
        for v in &mut self.pivot {
            *v = if v.is_finite() {
                v.clamp(*PIVOT_RANGE.start(), *PIVOT_RANGE.end())
            } else {
                50.0
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_legacy_viewers_and_old_files_load() {
        let t: BuildTools = serde_json::from_str("{}").unwrap();
        assert_eq!(t, BuildTools::default());
        assert!(t.unlink_confirm && t.pivot_percent && !t.light_radius);
        assert!(!t.select_copyable && !t.select_locked && t.select_group_owned);
        assert_eq!(t.pivot, [50.0; 3]);
    }

    #[test]
    fn unlink_asks_only_when_a_linkset_comes_apart() {
        let mut t = BuildTools::default();
        assert!(t.unlink_needs_confirm(3, 1));
        assert!(!t.unlink_needs_confirm(2, 2), "only roots: nothing comes apart");
        t.unlink_confirm_min = 5;
        assert!(!t.unlink_needs_confirm(4, 1));
        assert!(t.unlink_needs_confirm(5, 1));
        t.unlink_confirm = false;
        assert!(!t.unlink_needs_confirm(10, 1));
    }

    #[test]
    fn pivot_offset_in_percent_and_metres() {
        let mut t = BuildTools::default();
        let size = Vec3::new(2.0, 4.0, 6.0);
        assert_eq!(t.pivot_offset(size, Quat::IDENTITY), Vec3::ZERO, "50 % is the centre");
        t.pivot = [100.0, 0.0, 50.0];
        assert_eq!(t.pivot_offset(size, Quat::IDENTITY), Vec3::new(1.0, -2.0, 0.0));
        let turned = t.pivot_offset(size, Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
        assert!(turned.abs_diff_eq(Vec3::new(2.0, 1.0, 0.0), 1e-5));
        t.pivot_percent = false;
        t.pivot = [0.5, 0.0, -1.0];
        assert_eq!(t.pivot_offset(size, Quat::IDENTITY), Vec3::new(0.5, 0.0, -1.0));
        t.pivot = [f32::NAN, 1e9, 0.0];
        t.sanitize();
        assert_eq!(t.pivot, [50.0, 256.0, 0.0]);
    }
}
