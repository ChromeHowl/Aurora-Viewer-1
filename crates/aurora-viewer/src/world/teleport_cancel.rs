//! Cancelling a teleport under way. Port of LLAgent::teleportCancel
//! (indra/newview/llagent.cpp) and of the Cancel button's visibility in
//! process_teleport_start / process_teleport_progress
//! (indra/newview/llviewermessage.cpp, originally LGPL 2.1): the button shows
//! once the simulator has started the teleport, while its TeleportFlags do
//! not carry TELEPORT_FLAGS_DISABLE_CANCEL (or always, with
//! FSAlwaysShowTPCancel: the simulator ignores a cancel it does not allow),
//! never while RLVa holds the teleport (@tpto, an auto-accepted @accepttp
//! lure: RlvHandler::getCanCancelTp), and no more once TeleportFinish came
//! ("Teleport is finished; it can't be cancelled now").

use aurora_net::NetCommand;

/// TELEPORT_FLAGS_DISABLE_CANCEL (indra/llinventory/llteleportflags.h).
pub const DISABLE_CANCEL: u32 = 1 << 11;

/// Whether the Cancel button shows: `flags` are the last TeleportStart or
/// TeleportProgress flags of the teleport under way (None: none yet, or it
/// can no longer be cancelled).
pub fn cancel_shown(flags: Option<u32>, always: bool, rlva_locked: bool) -> bool {
    match flags {
        Some(flags) => !rlva_locked && (always || flags & DISABLE_CANCEL == 0),
        None => false,
    }
}

impl super::World {
    /// TeleportStart / TeleportProgress: their flags decide the Cancel
    /// button while the teleport is under way.
    pub(super) fn teleport_flags(&mut self, flags: u32) {
        if self.teleporting {
            self.tp_cancel_flags = Some(flags);
        }
    }

    /// TeleportFinish, TeleportLocal, TeleportFailed: no cancel any more,
    /// and RLVa's hold ends (RlvHandler::onTeleportFinished / onTeleportFailed).
    pub(super) fn teleport_cancel_over(&mut self) {
        self.tp_cancel_flags = None;
        self.tp_cancel_locked = false;
    }

    /// Whether the teleport under way shows its Cancel button.
    pub fn teleport_cancel_shown(&self, always: bool) -> bool {
        self.teleporting && cancel_shown(self.tp_cancel_flags, always, self.tp_cancel_locked)
    }

    /// The Cancel button (LLProgressView::onCancelButtonClicked →
    /// LLAgent::teleportCancel): the teleport is abandoned at once, the
    /// teleport screen goes, and TeleportCancel goes to the simulator.
    pub fn cancel_teleport(&mut self, always: bool) -> Option<NetCommand> {
        if !self.teleport_cancel_shown(always) {
            return None;
        }
        self.teleport_cancel_over();
        self.cancel_friend_join_flight();
        self.cancel_friend_join();
        self.tp_history.failed();
        self.teleporting = false;
        self.tp_cancelled = true;
        self.tp_failed = true;
        self.arrival_wait_since = None;
        self.system_notice(aurora_i18n::tr!("l3-tp-cancelled"), super::notice_topics::Topic::Teleport);
        Some(NetCommand::TeleportCancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_follows_the_flags_the_option_and_rlva() {
        assert!(!cancel_shown(None, true, false), "nothing started yet");
        assert!(cancel_shown(Some(0), false, false));
        assert!(!cancel_shown(Some(DISABLE_CANCEL), false, false));
        assert!(cancel_shown(Some(DISABLE_CANCEL | 4), true, false), "always available");
        assert!(!cancel_shown(Some(0), true, true), "RLVa wins over the option");
    }
}
