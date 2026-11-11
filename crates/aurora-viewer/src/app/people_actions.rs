//! What the radar's and the residents' menus do beyond the shared ones:
//! "Track" (the tracking beacon follows a resident the radar sees),
//! "Share", "Mark", and the estate's "Kick", "Teleport Home" and "Ban",
//! asked first.
//!
//! Port of FSRadar::startTracking, checkTracking and updateTracking
//! (indra/newview/fsradar.cpp), LLTracker::render3D's close-in stop
//! (lltracker.cpp), LLAvatarActions::share, estateKickMultiple,
//! callbackEstateKick, estateTeleportHomeMultiple,
//! callbackEstateTeleportHome, estateBanMultiple and callbackEstateBan
//! (llavataractions.cpp), LLNetMap::setAvatarMarkColors and
//! clearAvatarMarkColors (llnetmap.cpp), originally LGPL 2.1, with their
//! TrackAvatarNotPossible, EstateKickUser, EstateKickMultiple,
//! EstateTeleportHomeUser, EstateTeleportHomeMultiple, EstateBanUser and
//! EstateBanUserMultiple questions (notifications.xml); "Ban / Unban from
//! all my groups" asked with FSBanFromAllGroups and FSUnbanFromAllGroups
//! (LLGroupActions::confirmBanFromAllGroups), the run itself in
//! world/group_ban_all.rs.
use super::land_moderation::Ask;
use super::*;
use crate::ui::widgets::ActionKind;
use crate::world::radar::{self, TrackStep, Tracked};
use aurora_net::estate::EstateCommand;
use glam::DVec3;
use uuid::Uuid;

/// An estate action of the residents' menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EstateAction {
    Kick,
    Home,
    Ban,
}

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

    /// The names of the questions: complete, anonymous under RLVa's
    /// shownames (as `ask_moderation`).
    fn resident_name(&self, id: Uuid) -> String {
        self.world
            .social
            .avatar_names
            .complete(&id)
            .unwrap_or_else(|| self.world.social.name_of(&id))
    }

    /// "Ban from all my groups" / "Unban from all my groups": asked first,
    /// with the groups listed (confirmBanFromAllGroups).
    pub(super) fn ask_group_ban_all(&mut self, target: Uuid, ban: bool) {
        if self.world.group_ban_targets(target).is_empty() || self.world.group_ban_all.is_some() {
            return;
        }
        let name = self.resident_name(target);
        self.moderation_ask = Some(Ask::GroupBanAll { target, name, ban });
    }

    /// The question each frame; "Ban" or "Unban" starts the run.
    pub(super) fn group_ban_ask_ui(&mut self, ctx: &egui::Context, p: &crate::theme::Palette) {
        let Some(Ask::GroupBanAll { target, name, ban }) = self.moderation_ask.as_ref() else {
            return;
        };
        let (target, ban) = (*target, *ban);
        let groups = self.world.group_ban_targets(target);
        let listed = groups.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", ");
        let count = groups.len().to_string();
        let (title, question, choice) = if ban {
            (
                aurora_i18n::tr!("l2s-ban-all"),
                aurora_i18n::tr!("l2s-ban-all-question", name = name.as_str(), count = count.as_str()),
                ("ban", aurora_i18n::tr!("l2s-ban"), ActionKind::Danger),
            )
        } else {
            (
                aurora_i18n::tr!("l2s-unban-all"),
                aurora_i18n::tr!("l2s-unban-all-question", name = name.as_str(), count = count.as_str()),
                ("circle-check-filled", aurora_i18n::tr!("l2s-unban"), ActionKind::Primary),
            )
        };
        let lines = [question.as_str(), listed.as_str()];
        let Some(answer) = ui::context::choose(ctx, p, "group_ban_all_confirm", title, &lines, &[choice]) else {
            return;
        };
        let Some(Ask::GroupBanAll { name, .. }) = self.moderation_ask.take() else {
            return;
        };
        if answer.is_some() {
            log::info!(
                "group bans: {} {target} in {} group(s)",
                if ban { "ban" } else { "unban" },
                groups.len()
            );
            self.world.start_group_ban_all(target, name, ban);
        }
    }

    /// estateKickMultiple, estateTeleportHomeMultiple, estateBanMultiple:
    /// asked for the residents we may act on (canEstateKickOrTeleportHome
    /// filtering them).
    pub(super) fn ask_estate(&mut self, action: EstateAction, ids: Vec<Uuid>) {
        let targets: Vec<(Uuid, String)> = ids
            .into_iter()
            .filter(|id| self.world.estate_moderation_region(*id).is_some())
            .map(|id| (id, self.resident_name(id)))
            .collect();
        if targets.is_empty() {
            log::warn!("estate {action:?}: not allowed on these residents");
            return;
        }
        self.moderation_ask = Some(Ask::Estate { action, targets });
    }

    /// The estate question each frame, then the messages: kick and teleport
    /// home to the region each resident stands in, the ban to ours.
    pub(super) fn estate_ask_ui(&mut self, ctx: &egui::Context, p: &crate::theme::Palette) {
        let Some(Ask::Estate { action, targets }) = self.moderation_ask.as_ref() else {
            return;
        };
        let action = *action;
        let list = targets.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join("\n");
        let one = targets.first().filter(|_| targets.len() == 1).map(|(_, n)| n.clone());
        // RegionInfoAllEstatesYouOwn / RegionInfoAllEstatesYouManage
        let region = self.world.main_region.and_then(|h| self.world.regions.get(&h)?.info.clone());
        let all = region.as_ref().map(|info| {
            if info.owner == self.world.agent_id {
                aurora_i18n::tr!("b6-estate-all-own").to_owned()
            } else {
                aurora_i18n::tr!("b6-estate-all-managed", owner = self.world.social.name_of(&info.owner))
            }
        });
        let (title, question, choices): (&str, String, Vec<ui::context::Choice>) = match action {
            EstateAction::Kick => (
                aurora_i18n::tr!("b6-estate-kick"),
                match &one {
                    Some(n) => aurora_i18n::tr!("b6-estate-kick-one", name = n.as_str()),
                    None => aurora_i18n::tr!("b6-estate-kick-many").to_owned(),
                },
                vec![("door-exit", aurora_i18n::tr!("b6-estate-kick"), ActionKind::Danger)],
            ),
            EstateAction::Home => (
                aurora_i18n::tr!("b6-estate-home"),
                match &one {
                    Some(n) => aurora_i18n::tr!("b6-estate-home-one", name = n.as_str()),
                    None => aurora_i18n::tr!("b6-estate-home-many").to_owned(),
                },
                vec![("home-filled", aurora_i18n::tr!("b6-estate-home"), ActionKind::Primary)],
            ),
            EstateAction::Ban => {
                let all = all.clone().unwrap_or_default();
                let mut choices = vec![("ban", aurora_i18n::tr!("b6-estate-ban-this"), ActionKind::Danger)];
                if !all.is_empty() {
                    choices.push(("ban", aurora_i18n::tr!("b6-estate-ban-all"), ActionKind::Danger));
                }
                (
                    aurora_i18n::tr!("b6-estate-ban"),
                    match &one {
                        Some(n) => aurora_i18n::tr!("b6-estate-ban-one", name = n.as_str(), all = all.as_str()),
                        None => aurora_i18n::tr!("b6-estate-ban-many", all = all.as_str()),
                    },
                    choices,
                )
            }
        };
        let mut lines = vec![question.as_str()];
        if one.is_none() {
            lines.push(list.as_str());
        }
        let Some(choice) = ui::context::choose(ctx, p, "estate_resident_confirm", title, &lines, &choices) else {
            return;
        };
        let Some(Ask::Estate { targets, .. }) = self.moderation_ask.take() else {
            return;
        };
        let Some(choice) = choice else {
            return;
        };
        match action {
            EstateAction::Kick | EstateAction::Home => {
                for (target, _) in targets {
                    // where they stand when it is answered (getRegionFromAgentID)
                    let Some(handle) = self.world.estate_moderation_region(target) else {
                        log::warn!("estate {action:?}: {target} is no longer on an estate we manage");
                        continue;
                    };
                    let c = if action == EstateAction::Kick {
                        EstateCommand::KickResident { handle, target }
                    } else {
                        EstateCommand::TeleportResidentHome { handle, target }
                    };
                    log::info!("estate: {c:?}");
                    self.send(NetCommand::Estate(c));
                }
            }
            EstateAction::Ban => {
                let (Some(handle), Some(info)) = (self.world.main_region, region) else {
                    return;
                };
                // the owner cannot be banned
                let ids: Vec<Uuid> = targets.into_iter().map(|(id, _)| id).filter(|id| *id != info.owner).collect();
                if ids.is_empty() {
                    return;
                }
                let flags = aurora_net::estate::ban_resident_flags(choice == 1, info.owner == self.world.agent_id);
                log::info!("estate: ban {} resident(s), flags {flags}", ids.len());
                self.send(NetCommand::Estate(EstateCommand::BanResidents { handle, flags, ids }));
            }
        }
    }
}
