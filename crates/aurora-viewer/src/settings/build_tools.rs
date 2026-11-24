//! Build tools options of the Legacy Viewer that Aurora lacked: the unlink
//! confirmation (Preferences › Build). Held in `BuildSettings::tools`; every
//! default is the Legacy Viewer's (settings.xml names in comments).

use serde::{Deserialize, Serialize};

/// MinObjectsForUnlinkConfirm's upper bound (panel_preferences_firestorm.xml).
pub const UNLINK_MIN_MAX: u32 = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BuildTools {
    /// FSUnlinkConfirmEnabled (on): ask before unlinking.
    pub unlink_confirm: bool,
    /// MinObjectsForUnlinkConfirm (0): from this many prims selected (0 to 256).
    pub unlink_confirm_min: u32,
}

impl Default for BuildTools {
    fn default() -> Self {
        BuildTools {
            unlink_confirm: true,
            unlink_confirm_min: 0,
        }
    }
}

impl BuildTools {
    /// LLSelectMgr::unlinkObjects: confirm when on, at least the minimum is
    /// selected, and some linkset would come apart (more prims than roots).
    pub fn unlink_needs_confirm(&self, prims: usize, roots: usize) -> bool {
        self.unlink_confirm && prims >= self.unlink_confirm_min as usize && prims > roots
    }

    pub fn sanitize(&mut self) {
        self.unlink_confirm_min = self.unlink_confirm_min.min(UNLINK_MIN_MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_legacy_viewers_and_old_files_load() {
        let t: BuildTools = serde_json::from_str("{}").unwrap();
        assert_eq!(t, BuildTools::default());
        assert!(t.unlink_confirm);
        assert_eq!(t.unlink_confirm_min, 0);
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
    fn the_minimum_stays_in_range() {
        let mut t = BuildTools {
            unlink_confirm_min: 9999,
            ..Default::default()
        };
        t.sanitize();
        assert_eq!(t.unlink_confirm_min, UNLINK_MIN_MAX);
    }
}
