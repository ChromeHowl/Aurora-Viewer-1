//! The agent paused while the viewer is held up by a long task (a large
//! photo): AgentPause before it, AgentResume after it, each with the next
//! serial number and sent reliably to every region of the session
//! (send_agent_pause / send_agent_resume, indra/newview/llworld.cpp,
//! originally LGPL 2.1; the Legacy Viewer also sends them around modal
//! dialogs and while its window is minimised).
//!
//! The paused time does not count against the session's own circuit
//! timeout (`SIM_TIMEOUT`): the silence of a region is measured without the
//! paused intervals that fall inside it. Real loss still ends the session:
//! the credit of one pause is capped at [`MAX_CREDIT`], and once the agent
//! resumes the region has the rest of the usual window to be heard from.
//! In Aurora the circuits are kept by the network runtime's own threads
//! (pings answered, acks sent) whatever the main thread does, so this only
//! covers a region that goes quiet towards a paused agent.

use std::time::{Duration, Instant};

/// The most one pause takes off a region's silence: past it, a pause left
/// on (a task that never ends) no longer hides a dead circuit.
pub const MAX_CREDIT: Duration = Duration::from_secs(600);
/// Ended pauses remembered, the oldest dropped first.
const KEPT: usize = 8;

/// The pause state of a session and the paused intervals still relevant to
/// a region's silence.
#[derive(Debug, Default)]
pub struct AgentPause {
    /// SerialNum of the last AgentPause or AgentResume (gAgentPauseSerialNum:
    /// one counter for both, the region ignores numbers that do not grow).
    serial: u32,
    /// Paused since then.
    since: Option<Instant>,
    /// Pauses over: their start and end.
    ended: Vec<(Instant, Instant)>,
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

    /// The agent resumes at `now`: the serial number of the AgentResume to
    /// send, None when it was not paused.
    pub fn resume(&mut self, now: Instant) -> Option<u32> {
        let since = self.since.take()?;
        if self.ended.len() >= KEPT {
            self.ended.remove(0);
        }
        self.ended.push((since, now.max(since)));
        self.serial = self.serial.wrapping_add(1);
        Some(self.serial)
    }

    /// How long a region last heard at `last_recv` has been silent at `now`,
    /// the paused time within that silence left out (each pause counted
    /// [`MAX_CREDIT`] at most).
    pub fn silence(&self, last_recv: Instant, now: Instant) -> Duration {
        let raw = now.saturating_duration_since(last_recv);
        let current = self.since.map(|since| (since, now));
        let paused: Duration = self
            .ended
            .iter()
            .copied()
            .chain(current)
            .map(|(start, end)| {
                let end = end.min(start + MAX_CREDIT).min(now);
                let start = start.max(last_recv);
                end.saturating_duration_since(start)
            })
            .sum();
        raw.saturating_sub(paused)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn pause_and_resume_take_one_growing_serial() {
        let t = Instant::now();
        let mut p = AgentPause::default();
        assert_eq!(p.resume(t), None, "never paused: no AgentResume");
        assert_eq!(p.pause(t), Some(1));
        assert_eq!(p.pause(t + S(1)), None, "paused once only");
        assert!(p.since.is_some());
        assert_eq!(p.resume(t + S(2)), Some(2));
        assert_eq!(p.pause(t + S(3)), Some(3));
        assert_eq!(p.resume(t + S(4)), Some(4));
        assert!(p.since.is_none());
    }

    #[test]
    fn the_paused_time_does_not_count_as_silence() {
        let t = Instant::now();
        let mut p = AgentPause::default();
        // heard at t, paused from 10 s for 141 s (the frozen photo)
        p.pause(t + S(10));
        assert_eq!(p.silence(t, t + S(151)), S(10), "only the 10 s before the pause");
        p.resume(t + S(151));
        assert_eq!(p.silence(t, t + S(151)), S(10));
        // after it the region has the rest of the usual window
        assert_eq!(p.silence(t, t + S(200)), S(59));
        assert_eq!(p.silence(t, t + S(241)), S(100));
    }

    #[test]
    fn traffic_heard_during_the_pause_counts_from_then() {
        let t = Instant::now();
        let mut p = AgentPause::default();
        p.pause(t);
        p.resume(t + S(60));
        // last packet 30 s into the pause: 30 s of pause left in the silence
        assert_eq!(p.silence(t + S(30), t + S(60)), Duration::ZERO);
        assert_eq!(p.silence(t + S(30), t + S(90)), S(30));
        // a packet after the pause: nothing to take off
        assert_eq!(p.silence(t + S(70), t + S(90)), S(20));
    }

    #[test]
    fn a_pause_left_on_stops_hiding_a_dead_circuit() {
        let t = Instant::now();
        let mut p = AgentPause::default();
        p.pause(t);
        let later = t + MAX_CREDIT + S(100);
        assert_eq!(p.silence(t, later), S(100), "the credit is capped");
    }

    #[test]
    fn without_a_pause_the_silence_is_the_raw_one() {
        let t = Instant::now();
        let p = AgentPause::default();
        assert_eq!(p.silence(t, t + S(42)), S(42));
        // a clock read before the last packet never underflows
        assert_eq!(p.silence(t + S(5), t), Duration::ZERO);
    }

    #[test]
    fn only_the_last_pauses_are_kept() {
        let t = Instant::now();
        let mut p = AgentPause::default();
        for i in 0..20 {
            p.pause(t + S(i * 10));
            p.resume(t + S(i * 10 + 5));
        }
        assert_eq!(p.ended.len(), KEPT);
        // the kept ones still count: heard at 150 s, the pauses at 150, 160,
        // 170, 180 and 190 s take 25 s off the 50 s up to 200 s
        assert_eq!(p.silence(t + S(150), t + S(200)), S(25));
    }
}
