//! When a status packet goes out: a pure state machine the app polls every
//! frame with what it wants to share. Effects are unreliable and the region
//! relays them only at the moment they are sent, so a status is sent on
//! change, again on a slow heartbeat, and again for a resident who has just
//! come near; a status that stops is cleared twice.
//!
//! Limits: at most one packet every `MIN_GAP` seconds; a change waits
//! `CHANGE_GAP` seconds after the last packet (a track change counts as one
//! change); a heartbeat every `HEARTBEAT`; an arrival re-sends at most every
//! `ARRIVAL_GAP`.

use super::Payload;
use aurora_net::avatar_status::AvatarStatus;

/// Seconds a status lasts without a refresh: the effect's duration.
pub const TTL: f32 = 180.0;
/// Fewest seconds between two packets.
pub const MIN_GAP: f64 = 2.0;
/// Fewest seconds between a packet and the next one for a changed status.
pub const CHANGE_GAP: f64 = 5.0;
/// Seconds between two refreshes of a status that does not change.
pub const HEARTBEAT: f64 = 60.0;
/// Fewest seconds between two packets sent for arrivals.
pub const ARRIVAL_GAP: f64 = 10.0;
/// How many times a clear is sent: one may be lost.
const CLEARS: u8 = 2;

#[derive(Debug, Default)]
pub struct Sender {
    seq: u8,
    /// What the others hold of us: the last payload sent.
    last: Option<Payload>,
    sent_at: Option<f64>,
    clears_left: u8,
    /// The avatars near at the last poll, to see who arrived.
    near: Option<usize>,
}

impl Sender {
    /// The packet to send now, if any. `want` is what we share (None: nothing),
    /// `near` the number of other avatars in view, `now` the time in seconds.
    pub fn poll(&mut self, now: f64, want: Option<&Payload>, near: usize) -> Option<AvatarStatus> {
        let arrived = self.near.is_some_and(|before| near > before);
        self.near = Some(near);
        let since = self.sent_at.map(|t| now - t);
        if since.is_some_and(|s| s < MIN_GAP) {
            return None;
        }
        let Some(want) = want else {
            // a status that was sent is cleared; the clear is sent again
            if self.last.take().is_some() {
                self.clears_left = CLEARS;
            }
            if self.clears_left == 0 {
                return None;
            }
            self.clears_left -= 1;
            return Some(self.stamp(now, AvatarStatus::cleared(0)));
        };
        self.clears_left = 0;
        let changed = self.last.as_ref() != Some(want);
        let due = if changed {
            since.is_none_or(|s| s >= CHANGE_GAP)
        } else {
            since.is_none_or(|s| s >= HEARTBEAT) || (arrived && since.is_none_or(|s| s >= ARRIVAL_GAP))
        };
        if !due {
            return None;
        }
        self.last = Some(want.clone());
        Some(self.stamp(now, super::packet(want, 0)))
    }

    fn stamp(&mut self, now: f64, mut status: AvatarStatus) -> AvatarStatus {
        self.seq = self.seq.wrapping_add(1);
        self.sent_at = Some(now);
        status.seq = self.seq;
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_net::avatar_status::Category;

    fn payload(text: &str) -> Payload {
        Payload {
            category: Category::Custom,
            program: 0,
            detail: text.into(),
        }
    }

    #[test]
    fn a_new_status_goes_out_at_once_and_is_refreshed_on_the_heartbeat() {
        let mut s = Sender::default();
        let p = payload("a");
        let first = s.poll(100.0, Some(&p), 0).expect("sent");
        assert!(!first.clear && first.detail == "a" && first.seq == 1);
        for t in [101.0, 130.0, 159.9] {
            assert!(s.poll(t, Some(&p), 0).is_none(), "{t}");
        }
        assert_eq!(s.poll(160.0, Some(&p), 0).map(|x| x.seq), Some(2));
    }

    #[test]
    fn a_change_waits_for_the_change_gap() {
        let mut s = Sender::default();
        s.poll(100.0, Some(&payload("a")), 0).expect("sent");
        assert!(s.poll(103.0, Some(&payload("b")), 0).is_none());
        assert_eq!(s.poll(105.0, Some(&payload("b")), 0).map(|x| x.detail), Some("b".into()));
        // a flip back and forth sends once per gap, not once per flip
        assert!(s.poll(106.0, Some(&payload("a")), 0).is_none());
    }

    #[test]
    fn a_stopped_status_is_cleared_twice_and_then_left_alone() {
        let mut s = Sender::default();
        s.poll(100.0, Some(&payload("a")), 0).expect("sent");
        assert!(s.poll(101.0, None, 0).is_none(), "not within the minimum gap");
        let c1 = s.poll(102.0, None, 0).expect("clear");
        assert!(c1.clear && c1.seq == 2);
        assert!(s.poll(103.0, None, 0).is_none());
        let c2 = s.poll(104.0, None, 0).expect("second clear");
        assert!(c2.clear && c2.seq == 3);
        assert!(s.poll(110.0, None, 0).is_none());
        assert!(s.poll(500.0, None, 0).is_none());
    }

    #[test]
    fn nothing_is_ever_sent_when_nothing_was_shared() {
        let mut s = Sender::default();
        for t in 0..100 {
            assert!(s.poll(f64::from(t), None, t as usize).is_none());
        }
    }

    #[test]
    fn a_new_status_cancels_a_pending_clear() {
        let mut s = Sender::default();
        s.poll(100.0, Some(&payload("a")), 0).expect("sent");
        s.poll(102.0, None, 0).expect("clear");
        assert!(s.poll(104.0, Some(&payload("a")), 0).is_none(), "a change waits its gap too");
        let again = s.poll(107.0, Some(&payload("a")), 0).expect("sent again");
        assert!(!again.clear);
        assert!(s.poll(110.0, Some(&payload("a")), 0).is_none());
    }

    #[test]
    fn an_arrival_sends_again_at_most_every_ten_seconds() {
        let mut s = Sender::default();
        let p = payload("a");
        s.poll(100.0, Some(&p), 3).expect("sent");
        assert!(s.poll(105.0, Some(&p), 4).is_none(), "too soon after the last packet");
        // the arrival was seen at 105; the one at 111 counts on its own
        assert!(s.poll(111.0, Some(&p), 5).is_some());
        assert!(s.poll(113.0, Some(&p), 6).is_none());
        assert!(s.poll(122.0, Some(&p), 7).is_some());
        // a departure re-sends nothing
        assert!(s.poll(140.0, Some(&p), 2).is_none());
    }

    #[test]
    fn sequence_numbers_wrap() {
        let mut s = Sender::default();
        s.seq = 255;
        assert_eq!(s.poll(0.0, Some(&payload("a")), 0).map(|x| x.seq), Some(0));
    }
}
