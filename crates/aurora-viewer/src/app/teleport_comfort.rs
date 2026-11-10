//! Teleport options (settings/teleport.rs, Preferences › Mouse and keyboard
//! › Teleports and map): the world map closed or kept open when a teleport
//! starts, the destination's beacon cleared and the "back" SLURL told once
//! a teleport to another region is under way, muted sounds brought back
//! on arrival, and the teleport and login screens left out.
//!
//! Behaviour of the Legacy Viewer: LLAgent::teleportCore
//! (FSDoNotHideMapOnTeleport), process_teleport_finish
//! (FSDisableBeaconAfterTeleport), LLAgent::setTeleportState
//! (TELEPORT_MOVING saves the "back" SLURL), process_agent_movement_complete
//! (FSShowBackSLURL, FSAutoUnmuteSounds, FSAutoUnmuteAmbient), update_tp_display
//! (FSDisableTeleportScreens) and LLStartUp (FSDisableLoginScreens), in
//! indra/newview/llagent.cpp, llviewermessage.cpp, llviewerdisplay.cpp and
//! llstartup.cpp, originally LGPL 2.1.
//!
//! Differences: without its screen a teleport shows the world and its
//! notices in the nearby chat, where the Legacy Viewer still draws a small
//! progress bar; under RLVa @showloc the "back" SLURL is not told at all
//! (the Legacy Viewer prints it).

use super::*;

/// `AudioSettings::muted` of the ambient sounds and of the sound effects
/// (`settings::audio_channels`).
const AMBIENT: usize = 2;
const SOUNDS: usize = 3;

/// A teleport to another region under way, from its TeleportFinish to its
/// arrival (the Legacy Viewer's TELEPORT_MOVING).
#[derive(Debug, Default)]
pub(super) struct Leaving {
    /// The place left (LLAgent::mTeleportSourceSLURL), none when hidden by
    /// RLVa or not asked for.
    slurl: Option<String>,
}

/// FSAutoUnmuteSounds / FSAutoUnmuteAmbient: the muted channels asked come
/// back; true when one did.
fn unmute_after_teleport(muted: &mut [bool; 7], sounds: bool, ambient: bool) -> bool {
    let mut changed = false;
    for (on, channel) in [(sounds, SOUNDS), (ambient, AMBIENT)] {
        if on && muted[channel] {
            muted[channel] = false;
            changed = true;
        }
    }
    changed
}

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

    /// TeleportFinish: the agent leaves for another region. The place left
    /// is kept for the arrival's "back" SLURL, and the destination's beacon
    /// goes when asked.
    pub(super) fn teleport_comfort_leaving(&mut self) {
        let t = self.settings.teleport;
        let slurl = (t.back_slurl && !self.world.rlva.has("showloc"))
            .then(|| self.world.region_name())
            .filter(|region| !region.is_empty())
            .map(|region| crate::slurl::make(&region, self.world.agent.position));
        self.tp_leaving = Some(Leaving { slurl });
        if t.clear_beacon && self.world.map.clear_track_after_teleport() {
            log::info!("teleport: the destination's beacon is cleared (option)");
        }
    }

    /// AgentMovementComplete: the arrival of a teleport to another region
    /// tells the place left and brings muted sounds back, as asked.
    pub(super) fn teleport_comfort_arrived(&mut self) {
        let Some(leaving) = self.tp_leaving.take() else {
            return;
        };
        // a teleport that failed after its TeleportFinish: this arrival is
        // a crossing
        if self.world.arrived_by_crossing {
            return;
        }
        if let Some(slurl) = leaving.slurl
            && self.settings.teleport.back_slurl
            && !self.world.rlva.has("showloc")
        {
            self.world.system_notice(
                aurora_i18n::tr!("tpmap-back-slurl", slurl = slurl),
                crate::world::notice_topics::Topic::Teleport,
            );
        }
        let t = self.settings.teleport;
        if unmute_after_teleport(&mut self.settings.audio.muted, t.unmute_sounds, t.unmute_ambient) {
            log::info!("teleport: muted sounds are back (option)");
            self.settings.save();
        }
    }

    /// A teleport that did not happen, or the session's end: nothing is
    /// told at the next arrival.
    pub(super) fn teleport_comfort_reset(&mut self) {
        self.tp_leaving = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::worldmap::{Track, WorldMap};

    #[test]
    fn only_the_channels_asked_come_back() {
        let mut muted = [false, false, true, true, true, false, false];
        assert!(unmute_after_teleport(&mut muted, true, false));
        assert_eq!(muted, [false, false, true, false, true, false, false]);
        assert!(unmute_after_teleport(&mut muted, true, true));
        assert_eq!(muted, [false, false, false, false, true, false, false], "the music stays muted");
        assert!(!unmute_after_teleport(&mut muted, true, true), "nothing left to bring back");
        let mut muted = [true; 7];
        assert!(!unmute_after_teleport(&mut muted, false, false));
        assert_eq!(muted, [true; 7]);
    }

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
