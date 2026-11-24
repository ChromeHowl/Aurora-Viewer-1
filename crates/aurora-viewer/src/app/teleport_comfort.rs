//! Teleport options (settings/teleport.rs, Preferences › Mouse and keyboard):
//! the world map closed or kept open when a teleport starts, the
//! destination's beacon cleared once a teleport to another region is under
//! way, and the login's loading screen left out.
//!
//! Behaviour of the Legacy Viewer: LLAgent::teleportCore
//! (FSDoNotHideMapOnTeleport), process_teleport_finish
//! (FSDisableBeaconAfterTeleport: LLTracker::stopTracking,
//! LLWorldMap::cancelTracking) and LLStartUp (FSDisableLoginScreens), in
//! indra/newview/llagent.cpp, llviewermessage.cpp and llstartup.cpp,
//! originally LGPL 2.1.
//!
//! The "back" SLURL, the unmuting after a teleport and the teleport Cancel
//! are settings of their own (`show_back_slurl`, `AudioSettings`,
//! `always_show_tp_cancel`).

use super::*;

/// The login's loading screen gives way to the world: once arrived, after
/// the terrain or a few seconds, or at the arrival itself without the
/// screen (FSDisableLoginScreens).
pub(super) fn loading_screen_ends(skip: bool, arrived: bool, shown: Duration, terrain: f32, since_arrival: f32) -> bool {
    arrived && (skip || (shown > Duration::from_millis(1500) && (terrain > 0.97 || since_arrival > 6.0)))
}

impl App {
    /// LLAgent::teleportCore: the world map closes unless asked to stay
    /// (FSDoNotHideMapOnTeleport).
    pub(super) fn teleport_comfort_begin(&mut self) {
        if !self.settings.teleport.keep_map_open {
            self.panels.world_map = false;
        }
    }

    /// TeleportFinish, the agent leaves for another region: the
    /// destination's beacon goes when asked.
    pub(super) fn teleport_comfort_leaving(&mut self) {
        if self.settings.teleport.clear_beacon && self.world.map.clear_track_after_teleport() {
            log::info!("teleport: the destination's beacon is cleared (option)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::worldmap::{Track, WorldMap};

    #[test]
    fn the_login_screen_ends_at_the_arrival_without_it() {
        let early = Duration::from_millis(200);
        let late = Duration::from_secs(3);
        assert!(!loading_screen_ends(false, false, late, 1.0, 10.0), "not arrived");
        assert!(
            !loading_screen_ends(true, false, late, 1.0, 10.0),
            "not arrived, even without the screen"
        );
        assert!(!loading_screen_ends(false, true, early, 1.0, 10.0));
        assert!(!loading_screen_ends(false, true, late, 0.5, 1.0), "the terrain is still loading");
        assert!(loading_screen_ends(false, true, late, 0.98, 0.0));
        assert!(loading_screen_ends(false, true, late, 0.5, 6.5));
        assert!(loading_screen_ends(true, true, early, 0.0, 0.0));
    }

    /// FSDisableBeaconAfterTeleport: the tracked spot goes; nothing tracked,
    /// nothing to clear.
    #[test]
    fn the_beacon_is_cleared_after_a_teleport() {
        let mut map = WorldMap::default();
        assert!(!map.clear_track_after_teleport());
        map.set_track(Track {
            x: 256_128.0,
            y: 256_128.0,
            z: 25.0,
            label: "Plaza".into(),
            tooltip: String::new(),
            pending: false,
            teleport: false,
            invalid: false,
        });
        assert!(map.clear_track_after_teleport());
        assert!(map.track.is_none());
    }
}
