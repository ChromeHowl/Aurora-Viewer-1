//! The agent paused while the viewer is held up by a long task (a large
//! photo): AgentPause before it, AgentResume after it, each with the next
//! serial number and sent reliably to every region of the session
//! (send_agent_pause / send_agent_resume, indra/newview/llworld.cpp,
//! originally LGPL 2.1; the Legacy Viewer also sends them around modal
//! dialogs and while its window is minimised).
//!
//! Unlike the Legacy Viewer, whose message loop stops with its main loop,
//! Aurora keeps its circuits on the network runtime's own threads (pings
//! sent and answered, acks flushed by `Session::tick`) whatever the main
//! thread does. A region silent for the usual circuit timeout during a
//! pause is therefore a real loss, and nothing is taken off its silence.

use std::time::Instant;

/// The pause state of a session.
#[derive(Debug, Default)]
pub struct AgentPause {
    /// SerialNum of the last AgentPause or AgentResume (gAgentPauseSerialNum:
    /// one counter for both, the region ignores numbers that do not grow).
    serial: u32,
    /// Paused since then.
    since: Option<Instant>,
}

impl AgentPause {
    /// The agent pauses at `now`: the serial number of the AgentPause to
    /// send, None when it is already paused.
    pub fn pause(&mut self, now: Instant) -> Option<u32> {
        if self.since.is_some() {
            return None;
        }
        self.since = Some(now);
        self.serial = self.serial.wrapping_add(1);
        Some(self.serial)
    }

    /// The agent resumes: the serial number of the AgentResume to send,
    /// None when it was not paused.
    pub fn resume(&mut self) -> Option<u32> {
        self.since.take()?;
        self.serial = self.serial.wrapping_add(1);
        Some(self.serial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_and_resume_take_one_growing_serial() {
        let t = Instant::now();
        let mut p = AgentPause::default();
        assert_eq!(p.resume(), None, "never paused: no AgentResume");
        assert_eq!(p.pause(t), Some(1));
        assert_eq!(p.pause(t), None, "paused once only");
        assert!(p.since.is_some());
        assert_eq!(p.resume(), Some(2));
        assert_eq!(p.resume(), None, "resumed once only");
        assert_eq!(p.pause(t), Some(3));
        assert_eq!(p.resume(), Some(4));
        assert!(p.since.is_none());
    }
}
