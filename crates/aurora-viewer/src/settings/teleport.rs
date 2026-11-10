//! Settings of the teleport history's columns, the world map's extras and
//! the teleport options (Preferences › Mouse and keyboard › Teleports and
//! map, the history's ⚙ menu), from the Legacy Viewer's settings.xml:
//! FSTeleportHistoryShowPosition, FSTeleportHistoryShowDate, FSTPHistoryTZ,
//! FSAdvancedWorldmapRegionInfo, WorldmapFilterDuplicateLandmarks,
//! FSDoNotHideMapOnTeleport, FSDisableBeaconAfterTeleport, FSShowBackSLURL,
//! FSDisableTeleportScreens, FSDisableLoginScreens, FSAutoUnmuteSounds and
//! FSAutoUnmuteAmbient; same defaults.

use serde::{Deserialize, Serialize};

/// The time of the teleport history: its day sections and its dates
/// (FSTPHistoryTZ "utc", "slt", "local").
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum HistoryZone {
    Utc,
    /// Second Life time, the Legacy Viewer's default.
    #[default]
    Slt,
    /// The computer's time zone.
    Local,
}

impl HistoryZone {
    pub const ALL: [HistoryZone; 3] = [HistoryZone::Utc, HistoryZone::Slt, HistoryZone::Local];

    pub fn label(self) -> &'static str {
        match self {
            HistoryZone::Utc => aurora_i18n::tr!("tpmap-zone-utc"),
            HistoryZone::Slt => aurora_i18n::tr!("tpmap-zone-slt"),
            HistoryZone::Local => aurora_i18n::tr!("tpmap-zone-local"),
        }
    }
}

/// What the teleport history shows of each place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryView {
    /// The position in the region (FSTeleportHistoryShowPosition).
    pub position: bool,
    /// The date and time of the arrival (FSTeleportHistoryShowDate).
    pub date: bool,
    pub zone: HistoryZone,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Teleport {
    pub history: HistoryView,
    /// The people and the rating under each region's name on the world map
    /// (FSAdvancedWorldmapRegionInfo).
    pub map_region_info: bool,
    /// A landmark listed once on the world map when several items lead to
    /// the same place (WorldmapFilterDuplicateLandmarks).
    pub map_unique_landmarks: bool,
    /// The world map stays open when a teleport starts
    /// (FSDoNotHideMapOnTeleport).
    pub keep_map_open: bool,
    /// The destination's beacon goes away after a teleport to another
    /// region (FSDisableBeaconAfterTeleport).
    pub clear_beacon: bool,
    /// "Teleport completed from" and the SLURL of the place left, in nearby
    /// chat (FSShowBackSLURL).
    pub back_slurl: bool,
    /// No teleport screen: the world stays in view (FSDisableTeleportScreens).
    pub no_teleport_screen: bool,
    /// The login's loading screen ends at the arrival, the world loading in
    /// view (FSDisableLoginScreens).
    pub no_login_screen: bool,
    /// Muted sound effects come back after a teleport (FSAutoUnmuteSounds).
    pub unmute_sounds: bool,
    /// Muted ambient sounds come back after a teleport (FSAutoUnmuteAmbient).
    pub unmute_ambient: bool,
}

impl Default for Teleport {
    fn default() -> Self {
        Teleport {
            history: HistoryView::default(),
            map_region_info: true,
            map_unique_landmarks: false,
            keep_map_open: false,
            clear_beacon: false,
            back_slurl: true,
            no_teleport_screen: false,
            no_login_screen: false,
            unmute_sounds: false,
            unmute_ambient: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings file without the block, or with part of it, keeps the
    /// Legacy Viewer's defaults for the rest.
    #[test]
    fn old_files_keep_the_defaults() {
        let t: Teleport = serde_json::from_str("{}").expect("empty block");
        assert_eq!(t, Teleport::default());
        assert!(t.map_region_info && t.back_slurl && !t.keep_map_open);
        assert_eq!(t.history.zone, HistoryZone::Slt);
        let t: Teleport = serde_json::from_str(r#"{"history":{"date":true}}"#).expect("partial block");
        assert!(t.history.date && !t.history.position);
        assert_eq!(t.history.zone, HistoryZone::Slt);
    }
}
