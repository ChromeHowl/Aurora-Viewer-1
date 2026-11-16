//! The statuses of the avatars around, from other Aurora viewers
//! (`aurora_net::avatar_status`), and ours: what the name plate's chip says.
//! Each entry lives for the duration its sender gave and goes when it is
//! not sent again, or when the sender clears it. Entries are kept by the
//! sender id of the effect, which any client can forge: the store holds
//! text to draw and nothing that grants anything.

use aurora_net::avatar_status::{self, AvatarStatus};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Longest and shortest time an entry lives without a refresh (s): a sender
/// asking for more only leaves a stale chip behind.
pub const MAX_TTL: f32 = 300.0;
pub const MIN_TTL: f32 = 1.0;
/// Time over which a chip fades out before its entry ends (s).
pub const FADE: f32 = 1.0;
/// Entries kept at most: the region's avatars, with room to spare.
const MAX_ENTRIES: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub status: AvatarStatus,
    pub until: Instant,
}

#[derive(Debug, Default)]
pub struct AvatarStatuses {
    entries: HashMap<Uuid, Entry>,
}

fn lifetime(seconds: f32) -> Duration {
    let s = if seconds.is_finite() { seconds } else { MIN_TTL };
    Duration::from_secs_f32(s.clamp(MIN_TTL, MAX_TTL))
}

impl AvatarStatuses {
    /// A packet from `source`: a clear removes its entry, an older packet
    /// (by its sequence number) than the one held is dropped, anything else
    /// replaces the entry for `duration` seconds.
    pub fn receive(&mut self, source: Uuid, status: AvatarStatus, duration: f32, now: Instant) {
        if let Some(held) = self.entries.get(&source)
            && held.until > now
            && !avatar_status::newer(held.status.seq, status.seq)
        {
            return;
        }
        if status.clear || status.category == avatar_status::Category::None {
            self.entries.remove(&source);
            return;
        }
        if self.entries.len() >= MAX_ENTRIES && !self.entries.contains_key(&source) {
            self.prune(now);
            if self.entries.len() >= MAX_ENTRIES {
                return;
            }
        }
        self.entries.insert(
            source,
            Entry {
                status,
                until: now + lifetime(duration),
            },
        );
    }

    /// Our own status, shown at once and without a sequence number to
    /// compare: `None` takes it away.
    pub fn set_own(&mut self, me: Uuid, status: Option<AvatarStatus>, now: Instant) {
        match status {
            Some(status) => {
                self.entries.insert(
                    me,
                    Entry {
                        status,
                        until: now + lifetime(MAX_TTL),
                    },
                );
            }
            None => {
                self.entries.remove(&me);
            }
        }
    }

    /// The status of `id` at `now`, and how visible it is: 1, down to 0 over
    /// the last `FADE` seconds. None once it is over.
    pub fn get(&self, id: &Uuid, now: Instant) -> Option<(&AvatarStatus, f32)> {
        let e = self.entries.get(id)?;
        let left = e.until.checked_duration_since(now)?.as_secs_f32();
        if left <= 0.0 {
            return None;
        }
        Some((&e.status, (left / FADE).min(1.0)))
    }

    /// Drops what is over.
    pub fn prune(&mut self, now: Instant) {
        self.entries.retain(|_, e| e.until > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_net::avatar_status::Category;

    impl AvatarStatuses {
        fn len(&self) -> usize {
            self.entries.len()
        }

        fn is_empty(&self) -> bool {
            self.entries.is_empty()
        }
    }

    fn status(seq: u8, detail: &str) -> AvatarStatus {
        AvatarStatus {
            seq,
            clear: false,
            paused: false,
            category: Category::Custom,
            program: 0,
            detail: detail.into(),
        }
    }

    #[test]
    fn a_status_lives_for_its_duration_and_fades_at_the_end() {
        let (mut s, t0, a) = (AvatarStatuses::default(), Instant::now(), Uuid::from_u128(1));
        s.receive(a, status(1, "x"), 10.0, t0);
        let (got, fade) = s.get(&a, t0 + Duration::from_secs(5)).expect("alive");
        assert_eq!((got.detail.as_str(), fade), ("x", 1.0));
        let (_, fade) = s.get(&a, t0 + Duration::from_millis(9500)).expect("fading");
        assert!(fade > 0.4 && fade < 0.6, "{fade}");
        assert!(s.get(&a, t0 + Duration::from_secs(10)).is_none());
        s.prune(t0 + Duration::from_secs(11));
        assert!(s.is_empty());
    }

    #[test]
    fn a_clear_removes_it_and_an_old_packet_is_ignored() {
        let (mut s, t0, a) = (AvatarStatuses::default(), Instant::now(), Uuid::from_u128(1));
        s.receive(a, status(5, "new"), 60.0, t0);
        s.receive(a, status(4, "old"), 60.0, t0);
        assert_eq!(s.get(&a, t0).map(|g| g.0.detail.clone()), Some("new".into()));
        s.receive(a, AvatarStatus::cleared(6), 60.0, t0);
        assert!(s.get(&a, t0).is_none());
    }

    #[test]
    fn a_sender_that_started_over_wins_once_its_entry_is_gone() {
        let (mut s, t0, a) = (AvatarStatuses::default(), Instant::now(), Uuid::from_u128(1));
        s.receive(a, status(200, "before"), 5.0, t0);
        // the sender relogged: its numbers start again, after the entry ended
        s.receive(a, status(3, "after"), 5.0, t0 + Duration::from_secs(10));
        assert_eq!(
            s.get(&a, t0 + Duration::from_secs(10)).map(|g| g.0.detail.clone()),
            Some("after".into())
        );
    }

    #[test]
    fn the_lifetime_is_bounded() {
        let (mut s, t0, a) = (AvatarStatuses::default(), Instant::now(), Uuid::from_u128(1));
        s.receive(a, status(1, "x"), f32::MAX, t0);
        assert!(s.get(&a, t0 + Duration::from_secs(301)).is_none());
        s.receive(a, status(2, "x"), f32::NAN, t0);
        assert!(s.get(&a, t0).is_some());
        assert!(s.get(&a, t0 + Duration::from_secs(2)).is_none());
    }

    #[test]
    fn the_store_is_bounded_and_our_own_is_shown_at_once() {
        let (mut s, t0) = (AvatarStatuses::default(), Instant::now());
        for i in 0..2000u128 {
            s.receive(Uuid::from_u128(i + 1), status(1, "x"), 60.0, t0);
        }
        assert_eq!(s.len(), MAX_ENTRIES);
        let me = Uuid::from_u128(99_999);
        s.set_own(me, Some(status(0, "mine")), t0);
        assert!(s.get(&me, t0).is_some());
        s.set_own(me, None, t0);
        assert!(s.get(&me, t0).is_none());
    }
}
