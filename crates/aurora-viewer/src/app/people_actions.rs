//! What the radar's menus do beyond the shared ones: "Track" (the tracking
//! beacon follows a resident the radar sees).
//!
//! Port of FSRadar::startTracking, checkTracking and updateTracking
//! (indra/newview/fsradar.cpp), LLTracker::render3D's close-in stop
//! (lltracker.cpp), originally LGPL 2.1, with their TrackAvatarNotPossible
//! question (notifications.xml).
use super::*;
use crate::world::radar::{self, TrackStep, Tracked};
use glam::DVec3;
use uuid::Uuid;

impl App {
    /// Where the radar sees a resident now, in global meters.
    fn radar_global(&self, id: Uuid) -> Option<DVec3> {
        let e = self.radar.get(&id)?;
        let (ox, oy) = self.world.main_origin()?;
        Some(DVec3::new(
            f64::from(ox) + f64::from(e.position.x),
            f64::from(oy) + f64::from(e.position.y),
            f64::from(e.position.z),
        ))
    }

    /// LLTracker::trackLocation with LOCATION_AVATAR: the map's track on
    /// the resident, under their name.
    fn place_radar_track(&mut self, id: Uuid, at: DVec3) {
        self.world.map.track_location(at.x, at.y, at.z as f32, false);
        let name = self.world.social.name_of(&id);
        if let Some(t) = self.world.map.track.as_mut() {
            t.label = name;
        }
    }

    /// "Track" (FSRadar::startTracking): only a resident of the radar.
    pub(super) fn track_avatar(&mut self, id: Uuid) {
        match self.radar_global(id) {
            Some(at) => {
                self.radar.tracked = Some(Tracked { id, at });
                self.place_radar_track(id, at);
            }
            None => {
                self.world.notifications.push(
                    crate::world::notifications::Kind::Info,
                    aurora_i18n::tr!("radar-track"),
                    aurora_i18n::tr!("radar-track-not-possible"),
                    crate::world::notifications::Data::None,
                );
            }
        }
    }

    /// After each radar sweep (FSRadar::checkTracking): the beacon follows
    /// the resident, and goes away when they leave the radar or we reach
    /// them. Another track, or the beacon dismissed, ends the following.
    pub(super) fn update_radar_track(&mut self) {
        let Some(tracked) = self.radar.tracked else {
            return;
        };
        let ours = self.world.map.avatar.is_none()
            && self
                .world
                .map
                .track
                .as_ref()
                .is_some_and(|t| t.x == tracked.at.x && t.y == tracked.at.y && t.z == tracked.at.z as f32);
        if !ours {
            self.radar.tracked = None;
            return;
        }
        let Some((ox, oy)) = self.world.main_origin() else {
            return;
        };
        let p = self.world.agent.position;
        let me = DVec3::new(f64::from(ox) + f64::from(p.x), f64::from(oy) + f64::from(p.y), f64::from(p.z));
        // @shownearby empties the radar: the beacon must not reveal them
        let seen = if self.world.rlva.has("shownearby") {
            None
        } else {
            self.radar_global(tracked.id)
        };
        match radar::track_step(&tracked, seen, me, self.settings.radar.track_clear_close_in) {
            TrackStep::Keep => {}
            TrackStep::Move(at) => {
                self.radar.tracked = Some(Tracked { at, ..tracked });
                self.place_radar_track(tracked.id, at);
            }
            TrackStep::Stop => {
                self.radar.tracked = None;
                self.world.map.clear_track();
            }
        }
    }
}
