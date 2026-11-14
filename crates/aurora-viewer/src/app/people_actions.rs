//! What the radar's and the residents' menus do beyond the shared ones:
//! "Track" (the tracking beacon follows a resident the radar sees),
//! "Share" and "Mark" (the estate's "Kick", "Teleport Home" and "Ban" are in
//! app/land_moderation.rs; "Ban from all my groups" is in
//! ui/chat/moderation.rs and app/group_chat_moderation.rs).
//!
//! Port of FSRadar::startTracking, checkTracking and updateTracking
//! (indra/newview/fsradar.cpp), LLTracker::render3D's close-in stop
//! (lltracker.cpp), LLAvatarActions::share (llavataractions.cpp),
//! LLNetMap::setAvatarMarkColors and clearAvatarMarkColors (llnetmap.cpp),
//! originally LGPL 2.1, with their TrackAvatarNotPossible question
//! (notifications.xml).
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
                    aurora_i18n::tr!("b6-track"),
                    aurora_i18n::tr!("b6-track-not-possible"),
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
        let seen = if self.world.rlva.has("shownearby") {
            None
        } else {
            self.radar_global(tracked.id)
        };
        match radar::track_step(&tracked, seen, me, self.settings.people.track_clear_close_in) {
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

    /// "Share" (LLAvatarActions::share): the inventory and the conversation
    /// with them, where items are dropped to give them, with its hint.
    pub(super) fn share_with(&mut self, id: Uuid) {
        if !self.world.rlva.allows("share", &id.to_string()) {
            self.world
                .system_notice(aurora_i18n::tr!("rlva-blocked"), crate::world::notice_topics::Topic::Warning);
            return;
        }
        // FIRE-8804: @showinv keeps the inventory closed
        if self.world.rlva.has("showinv") {
            return;
        }
        ui::widgets::open_floater(&self.egui_ctx, "inventory", &mut self.panels.inventory);
        self.open_conversation_to_type(id);
        if self.world.social.has_session(&id) {
            self.world
                .conversation_notice(id, aurora_i18n::tr!("b6-share-alert"), crate::world::notice_topics::Topic::Items);
        }
    }
}
