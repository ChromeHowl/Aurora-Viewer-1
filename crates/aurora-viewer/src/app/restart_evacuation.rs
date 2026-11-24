//! Leaving the agent's region before it restarts and coming back after
//! (Preferences › Confirmations › Staying online): the rules of
//! `world/restart_evacuation.rs` run over the restart alert's own teleports
//! (`app/region_restart.rs`, `request_user_teleport`, so RLVa's teleport
//! restrictions apply as to any teleport of the user) and the world map's
//! named region requests (once a minute, `WorldMap::want_named_region`).

use super::App;
use crate::world::restart_evacuation::{
    Candidate, ComeBack, Destination, Evacuate, Evacuation, Origin, Reach, ReturnStep, ReturnTrip, Seen, Step, destinations,
};
use aurora_net::NetCommand;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct State {
    /// The evacuation under way.
    evacuation: Option<Evacuation>,
    /// The way back, waiting for the region.
    trip: Option<ReturnTrip>,
    /// The automatic way back, its teleport under way.
    returning: Option<ReturnTrip>,
    /// The offer to go back is shown.
    offered: bool,
    /// The automatic evacuation of this alert was started (once per alert).
    automatic_done: bool,
    /// A teleport asked by these rules, not by the user.
    own: bool,
}

impl App {
    /// The alert's "Evacuate" button shows: the setting, and nothing under way.
    pub(super) fn restart_evacuate_offered(&self) -> bool {
        self.settings.restart_evacuation.evacuate != Evacuate::Off && self.restart_evac.evacuation.is_none()
    }

    /// The backup landmarks of the preferences, as this session sees them.
    fn restart_candidates(&mut self) -> Vec<Candidate> {
        let here = self.world.main_region.map(|h| {
            let (x, y) = aurora_net::handle_to_origin(h);
            (x / 256, y / 256)
        });
        let known = self.restart_landmarks();
        let list = self.settings.restart_evacuation.landmarks.clone();
        list.iter()
            .map(|backup| {
                let found = known.iter().find(|l| l.item == backup.item);
                let reach = match found.map(|l| self.world.landmarks.peek(l.asset)) {
                    Some(crate::world::landmarks::Resolved::Ready { global, .. }) => match self.world.map.sim_at_global(global.x, global.y)
                    {
                        Some((key, _)) if Some(key) == here => Reach::Here,
                        Some((_, sim)) if sim.is_down() => Reach::Down,
                        Some(_) => Reach::Up,
                        None => Reach::Unknown,
                    },
                    _ => Reach::Unknown,
                };
                Candidate {
                    asset: found.map(|l| l.asset),
                    name: found.map_or_else(|| backup.name.clone(), |l| l.name.clone()),
                    reach,
                }
            })
            .collect()
    }

    /// Where the backup landmarks lead is asked while a restart counts, so
    /// that a landmark of a region that is down is skipped.
    fn resolve_backup_landmarks(&mut self) {
        let known = self.restart_landmarks();
        let items: Vec<Uuid> = self.settings.restart_evacuation.landmarks.iter().map(|l| l.item).collect();
        for l in known.iter().filter(|l| items.contains(&l.item)) {
            self.world.landmarks.resolve(l.asset);
        }
    }

    /// "Evacuate", or the automatic evacuation at its time.
    pub(super) fn start_restart_evacuation(&mut self) {
        let Some(a) = self.world.region_restart.alert.as_ref().filter(|a| a.live()) else {
            return;
        };
        let seconds = a.seconds().max(0) as u64;
        let Some(handle) = self.world.main_region else {
            return;
        };
        let origin = Origin {
            handle,
            region: self.world.region_name(),
            position: self.world.agent.position,
        };
        let list = destinations(&self.restart_candidates());
        log::info!("restart evacuation: {} destination(s) to try", list.len());
        self.restart_evac.trip = None;
        self.restart_evac.returning = None;
        self.restart_evac.offered = false;
        self.restart_evac.evacuation = Some(Evacuation::new(origin, Instant::now() + Duration::from_secs(seconds), list));
    }

    /// A teleport asked by these rules: the alert's own request, marked so
    /// that it is not taken for the user's.
    fn restart_evac_teleport(&mut self, command: NetCommand, dest: String) -> bool {
        self.restart_evac.own = true;
        let sent = self.request_user_teleport(command, dest, true);
        self.restart_evac.own = false;
        sent
    }

    /// `request_user_teleport`: the user goes somewhere by themselves, the
    /// evacuation and the way back are theirs to make from now on.
    pub(super) fn restart_evac_user_teleport(&mut self) {
        if self.restart_evac.own {
            return;
        }
        self.restart_evac.evacuation = None;
        if let Some(trip) = self.restart_evac.trip.as_mut() {
            trip.user_teleport();
        }
        self.restart_evac.returning = None;
    }

    /// What became of the teleport asked last.
    fn restart_evac_seen(&self, sent: bool) -> Seen {
        if !sent || (!self.world.teleporting && self.world.tp_failed) {
            Seen::Failed
        } else if self.world.teleporting {
            Seen::Pending
        } else {
            self.world.main_region.map_or(Seen::Pending, Seen::Arrived)
        }
    }

    /// Each frame in world.
    pub(super) fn update_restart_evacuation(&mut self) {
        let alert = self.world.region_restart.alert.as_ref().map(|a| (a.live(), a.seconds()));
        match alert {
            Some((live, seconds)) => {
                if live && self.settings.restart_evacuation.evacuate != Evacuate::Off {
                    self.resolve_backup_landmarks();
                }
                if !self.restart_evac.automatic_done && self.settings.restart_evacuation.due(live, seconds) {
                    self.restart_evac.automatic_done = true;
                    self.world.system_notice(
                        aurora_i18n::tr!("stay-online-evacuating"),
                        crate::world::notice_topics::Topic::Region,
                    );
                    self.start_restart_evacuation();
                }
            }
            None => self.restart_evac.automatic_done = false,
        }
        self.step_restart_evacuation();
        self.step_restart_return();
    }

    fn step_restart_evacuation(&mut self) {
        let mut sent = true;
        // a refused request (RLVa) is a failure at once: at most the whole list in a frame
        for _ in 0..=crate::world::restart_evacuation::MAX_LANDMARKS + 1 {
            let seen = self.restart_evac_seen(sent);
            let Some(evacuation) = self.restart_evac.evacuation.as_mut() else {
                return;
            };
            match evacuation.step(seen) {
                Step::Wait => return,
                Step::Try(Destination::Home) => {
                    sent = self.restart_evac_teleport(NetCommand::TeleportHome, aurora_i18n::tr!("common-home").into());
                }
                Step::Try(Destination::Landmark { asset, name }) => {
                    log::info!("restart evacuation: trying the landmark \"{name}\"");
                    sent = self.restart_evac_teleport(NetCommand::TeleportLandmark(asset), name);
                }
                Step::Done(arrival) => {
                    if let Some(e) = self.restart_evac.evacuation.take()
                        && self.settings.restart_evacuation.come_back != ComeBack::Stay
                    {
                        self.restart_evac.trip = Some(ReturnTrip::new(e.origin, e.restart_at, arrival));
                    }
                    return;
                }
                Step::Stranded => {
                    self.restart_evac.evacuation = None;
                    self.world.system_notice(
                        aurora_i18n::tr!("stay-online-evacuation-failed"),
                        crate::world::notice_topics::Topic::Warning,
                    );
                    return;
                }
            }
            if sent {
                return;
            }
        }
    }

    fn step_restart_return(&mut self) {
        // the automatic way back, its teleport over
        if let Some(mut trip) = self.restart_evac.returning.take() {
            match self.restart_evac_seen(true) {
                Seen::Pending => self.restart_evac.returning = Some(trip),
                Seen::Failed if trip.retry() => self.restart_evac.trip = Some(trip),
                Seen::Failed | Seen::Arrived(_) => {}
            }
            return;
        }
        let Some(trip) = self.restart_evac.trip.as_mut() else {
            self.restart_evac.offered = false;
            return;
        };
        if !self.world.teleporting
            && let Some(handle) = self.world.main_region
        {
            trip.region(handle);
        }
        // the region's state, as the world map hears it (asked once a minute)
        let region = trip.origin.region.clone();
        let (ox, oy) = aurora_net::handle_to_origin(trip.origin.handle);
        let key = (ox / 256, oy / 256);
        self.world.map.want_named_region(&region);
        if let Some(answered) = self.world.map.answered_at(key)
            && let Some((_, sim)) = self.world.map.sim_at(key.0, key.1)
        {
            trip.observe(answered, !sim.is_down());
        }
        match trip.step(Instant::now()) {
            ReturnStep::Wait => {}
            ReturnStep::Cancelled | ReturnStep::Expired => {
                log::info!("restart evacuation: the way back to {region} is dropped");
                self.restart_evac.trip = None;
                self.restart_evac.offered = false;
            }
            ReturnStep::Ready => match self.settings.restart_evacuation.come_back {
                ComeBack::Stay => self.restart_evac.trip = None,
                ComeBack::Offer => {
                    if !self.restart_evac.offered {
                        self.restart_evac.offered = true;
                        self.world.system_notice(
                            aurora_i18n::tr!("stay-online-return-offer", region = region.as_str()),
                            crate::world::notice_topics::Topic::Region,
                        );
                    }
                }
                ComeBack::Automatic => {
                    self.world.system_notice(
                        aurora_i18n::tr!("stay-online-returning", region = region.as_str()),
                        crate::world::notice_topics::Topic::Region,
                    );
                    if let Some(trip) = self.restart_evac.trip.take() {
                        self.restart_return(trip, true);
                    }
                }
            },
        }
    }

    /// The teleport back to the exact place left.
    fn restart_return(&mut self, mut trip: ReturnTrip, automatic: bool) {
        let command = NetCommand::TeleportTo {
            handle: trip.origin.handle,
            position: trip.origin.position,
            look_at: glam::Vec3::X,
        };
        let sent = self.restart_evac_teleport(command, trip.origin.region.clone());
        if automatic && sent {
            self.restart_evac.returning = Some(trip);
        } else if automatic && trip.retry() {
            self.restart_evac.trip = Some(trip);
        }
    }

    /// The offer to go back, under the top bar.
    pub(super) fn show_restart_return(&mut self, ctx: &egui::Context, p: &crate::theme::Palette) {
        if !self.restart_evac.offered {
            return;
        }
        let Some(region) = self.restart_evac.trip.as_ref().map(|t| t.origin.region.clone()) else {
            return;
        };
        match crate::ui::restart_evacuation::return_offer(ctx, p, &region) {
            Some(true) => {
                self.restart_evac.offered = false;
                if let Some(trip) = self.restart_evac.trip.take() {
                    self.restart_return(trip, false);
                }
            }
            Some(false) => {
                self.restart_evac.offered = false;
                self.restart_evac.trip = None;
            }
            None => {}
        }
    }
}
