//! Teleport, world map and logout options, from the Legacy Viewer's
//! settings.xml: FSTeleportHistoryShowPosition, FSTeleportHistoryShowDate,
//! FSAdvancedWorldmapRegionInfo, WorldmapFilterDuplicateLandmarks,
//! FSDoNotHideMapOnTeleport, FSDisableBeaconAfterTeleport,
//! FSDisableTeleportScreens, FSDisableLoginScreens and
//! FSDisableLogoutScreens. The history's time zone (FSTPHistoryTZ), the
//! "back" SLURL (FSShowBackSLURL), the unmuting after a teleport
//! (FSAutoUnmute*) and the teleport Cancel (FSAlwaysShowTPCancel) are
//! settings of their own in `settings.rs` and `AudioSettings`.
//!
//! Same defaults as the Legacy Viewer, except one: the world map stays open
//! during a teleport, as Aurora's map always did.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Teleport {
    /// The position in the region of each place of the teleport history's
    /// Legacy tabs (FSTeleportHistoryShowPosition).
    pub history_position: bool,
    /// The date and time of the arrival of each place of the history's
    /// Legacy tabs (FSTeleportHistoryShowDate).
    pub history_date: bool,
    /// The people and the rating under each region's name on the world map
    /// (FSAdvancedWorldmapRegionInfo).
    pub map_region_info: bool,
    /// A landmark listed once on the world map when several items lead to
    /// the same place (WorldmapFilterDuplicateLandmarks).
    pub map_unique_landmarks: bool,
    /// The world map stays open when a teleport starts
    /// (FSDoNotHideMapOnTeleport). On by default, as the map behaved in
    /// Aurora before the option (the Legacy Viewer's default is off).
    pub keep_map_open: bool,
    /// The destination's beacon goes away after a teleport to another
    /// region (FSDisableBeaconAfterTeleport).
    pub clear_beacon: bool,
    /// No teleport screen: the world stays in view, with a small progress
    /// bar (FSDisableTeleportScreens).
    pub no_teleport_screen: bool,
    /// The login's loading screen ends at the arrival, the world loading in
    /// view (FSDisableLoginScreens).
    pub no_login_screen: bool,
    /// Logging out keeps the world in view with a small progress bar instead
    /// of the logout screen (FSDisableLogoutScreens). The screen is shown by
    /// default.
    pub no_logout_screen: bool,
}

impl Default for Teleport {
    fn default() -> Self {
        Teleport {
            history_position: false,
            history_date: false,
            map_region_info: true,
            map_unique_landmarks: false,
            keep_map_open: true,
            clear_beacon: false,
            no_teleport_screen: false,
            no_login_screen: false,
            no_logout_screen: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings file without the block, or with part of it, gets the
    /// defaults for the rest: the map kept open, the logout screen shown.
    #[test]
    fn old_files_keep_the_defaults() {
        let t: Teleport = serde_json::from_str("{}").expect("empty block");
        assert_eq!(t, Teleport::default());
        assert!(t.map_region_info && t.keep_map_open && !t.no_logout_screen);
        let t: Teleport = serde_json::from_str(r#"{"history_date":true}"#).expect("partial block");
        assert!(t.history_date && !t.history_position && t.keep_map_open);
    }

    /// A choice the user saved stays as saved.
    #[test]
    fn saved_choices_win_over_the_defaults() {
        let t: Teleport = serde_json::from_str(r#"{"keep_map_open":false,"no_logout_screen":true}"#).expect("saved choices");
        assert!(!t.keep_map_open && t.no_logout_screen);
    }
}
