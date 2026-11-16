//! Sharing our status with the other Aurora viewers nearby (`status_share`):
//! once a frame, what the settings and the machine say becomes our own chip
//! at once, and a ViewerEffect when `status_share::sender` says it is time.
//! With the master switch off nothing is read from the machine and nothing
//! is sent, apart from the clears of a status that was.

use super::App;
use crate::status_share::collect::Collector;
use crate::status_share::sender::{self, Sender};
use crate::status_share::{self, Inputs, Payload};
use aurora_net::NetCommand;
use std::time::Instant;

#[derive(Default)]
pub(super) struct Sharing {
    /// The reading thread: only while the master switch is on.
    collector: Option<Collector>,
    sender: Sender,
    /// What our own plate shows.
    shown: Option<Payload>,
    /// One effect id for all our packets, so a receiver updates one object.
    effect: Option<uuid::Uuid>,
}

impl App {
    pub(super) fn tick_status_sharing(&mut self) {
        let now = Instant::now();
        let me = self.world.agent_id;
        if self.demo
            && let Ok(mode) = std::env::var("AURORA_DEMO_STATUS")
        {
            crate::demo::status::tick(&mut self.world, self.frame_count, &mode);
        }
        if self.frame_count.is_multiple_of(120) {
            self.world.avatar_status.prune(now);
        }
        let cfg = &self.settings.status_share;
        let sharing = &mut self.status_sharing;
        // nothing is read until the master switch is on
        let (music, programs) = if cfg.share {
            let collector = sharing.collector.get_or_insert_with(Collector::start);
            collector.want(cfg.music, cfg.programs);
            collector.read()
        } else {
            sharing.collector = None;
            (None, Vec::new())
        };
        let inputs = Inputs {
            music,
            programs,
            busy: matches!(crate::ui::presence::of_avatar(&self.world, &me), crate::ui::presence::Status::Busy),
            invisible: self.world.status.invisible.on,
        };
        let want = status_share::desired(cfg, &inputs);
        // ours is shown at once, before and whether or not a packet goes out
        if want != sharing.shown {
            let own = want.as_ref().map(|p| status_share::packet(p, 0));
            self.world.avatar_status.set_own(me, own, now);
            sharing.shown.clone_from(&want);
        }
        if me.is_nil() {
            return;
        }
        let near = self.world.coarse.values().map(Vec::len).sum::<usize>();
        let time = self.egui_ctx.input(|i| i.time);
        if let Some(status) = sharing.sender.poll(time, want.as_ref(), near) {
            let effect = *sharing.effect.get_or_insert_with(uuid::Uuid::new_v4);
            self.send(NetCommand::AvatarStatus {
                effect,
                status,
                duration: sender::TTL,
            });
        }
    }
}
