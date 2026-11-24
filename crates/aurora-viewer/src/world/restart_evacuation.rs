//! Leaving a region before it restarts, and coming back once it is up
//! again (Preferences › Confirmations › Staying online). Aurora's own, on
//! top of the restart alert (`region_restart.rs`): the Legacy Viewer's box
//! only offers a teleport to a landmark.
//!
//! The rules only: where to go and what to try next when a teleport fails
//! (`Evacuation`), and when the way back is offered, made or dropped
//! (`ReturnTrip`). `app/restart_evacuation.rs` runs them over the alert's
//! own teleports and the world map's region requests.

use aurora_net::RegionHandle;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// What a restart notice of the agent's region does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Evacuate {
    Off,
    /// An "Evacuate" button on the alert.
    #[default]
    Offer,
    /// The teleport by itself, `Settings::lead_seconds` before the restart.
    Automatic,
}

/// What happens once the region left is up again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ComeBack {
    Stay,
    #[default]
    Offer,
    Automatic,
}

/// A backup landmark: the inventory item (never its asset id, which the
/// settings file must not hold) and its name when it was chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackupLandmark {
    pub item: Uuid,
    pub name: String,
}

/// The longest list of backup landmarks.
pub const MAX_LANDMARKS: usize = 8;
/// The range of `Settings::lead_seconds`.
pub const LEAD_SECONDS: (u32, u32) = (10, 300);

/// Preferences › Confirmations › Staying online.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub evacuate: Evacuate,
    /// Seconds before the restart at which `Evacuate::Automatic` leaves.
    pub lead_seconds: u32,
    /// Where to go, in order; Home after them.
    pub landmarks: Vec<BackupLandmark>,
    pub come_back: ComeBack,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            evacuate: Evacuate::Offer,
            lead_seconds: 60,
            landmarks: Vec::new(),
            come_back: ComeBack::Offer,
        }
    }
}

impl Settings {
    pub fn lead(&self) -> i32 {
        self.lead_seconds.clamp(LEAD_SECONDS.0, LEAD_SECONDS.1) as i32
    }

    /// Add a landmark at the end of the list; false when it is full or
    /// holds it already.
    pub fn add(&mut self, item: Uuid, name: &str) -> bool {
        if self.landmarks.len() >= MAX_LANDMARKS || self.landmarks.iter().any(|l| l.item == item) {
            return false;
        }
        self.landmarks.push(BackupLandmark {
            item,
            name: name.to_owned(),
        });
        true
    }

    /// Whether `Evacuate::Automatic` leaves now: a live count of the
    /// agent's region with `seconds` left.
    pub fn due(&self, live: bool, seconds: i32) -> bool {
        self.evacuate == Evacuate::Automatic && live && seconds <= self.lead()
    }
}

/// What is known of where a backup landmark leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Its asset or its region is not known yet: tried, the grid answers.
    Unknown,
    Up,
    /// The world map shows its region down.
    Down,
    /// It leads into the region that restarts.
    Here,
}

/// A backup landmark as the session sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The landmark's asset; none when the item is not in this account's
    /// inventory (or lies in its trash).
    pub asset: Option<Uuid>,
    pub name: String,
    pub reach: Reach,
}

/// Where an evacuation teleports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    Landmark { asset: Uuid, name: String },
    Home,
}

/// The destinations to try, in order: the backup landmarks that can be
/// used, then Home.
pub fn destinations(candidates: &[Candidate]) -> Vec<Destination> {
    let mut out: Vec<Destination> = Vec::new();
    for c in candidates.iter().take(MAX_LANDMARKS) {
        let Some(asset) = c.asset else { continue };
        if matches!(c.reach, Reach::Down | Reach::Here) {
            continue;
        }
        let known = out
            .iter()
            .any(|d| matches!(d, Destination::Landmark { asset: a, .. } if *a == asset));
        if !known {
            out.push(Destination::Landmark {
                asset,
                name: c.name.clone(),
            });
        }
    }
    out.push(Destination::Home);
    out
}

/// The place left, to come back to.
#[derive(Debug, Clone, PartialEq)]
pub struct Origin {
    pub handle: RegionHandle,
    pub region: String,
    /// Where the agent stood, in the region's metres.
    pub position: glam::Vec3,
}

/// What became of the teleport asked last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    /// Still under way.
    Pending,
    /// Refused or failed (TeleportFailed, or a restriction of the viewer).
    Failed,
    /// Over, with the agent in this region.
    Arrived(RegionHandle),
}

/// What an evacuation does next.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Wait,
    /// Ask for this teleport.
    Try(Destination),
    /// Out of the region, in this one.
    Done(RegionHandle),
    /// Nowhere left to try.
    Stranded,
}

/// One evacuation: its destinations, tried one after the other.
#[derive(Debug, Clone)]
pub struct Evacuation {
    pub origin: Origin,
    /// When the region is expected to restart.
    pub restart_at: Instant,
    queue: VecDeque<Destination>,
    asked: bool,
}

impl Evacuation {
    pub fn new(origin: Origin, restart_at: Instant, destinations: Vec<Destination>) -> Evacuation {
        Evacuation {
            origin,
            restart_at,
            queue: destinations.into(),
            asked: false,
        }
    }

    fn next(&mut self) -> Step {
        match self.queue.pop_front() {
            Some(d) => {
                self.asked = true;
                Step::Try(d)
            }
            None => Step::Stranded,
        }
    }

    /// Each frame, with what became of the teleport asked last.
    pub fn step(&mut self, seen: Seen) -> Step {
        if !self.asked {
            return self.next();
        }
        match seen {
            Seen::Pending => Step::Wait,
            Seen::Failed => self.next(),
            // a landmark of the region itself: not out yet
            Seen::Arrived(handle) if handle == self.origin.handle => self.next(),
            Seen::Arrived(handle) => Step::Done(handle),
        }
    }
}

/// The region must have answered "up" for this long before the way back.
pub const UP_FOR: Duration = Duration::from_secs(60);
/// A region still not back after this long is no longer waited for.
pub const RETURN_EXPIRES: Duration = Duration::from_secs(60 * 60);
/// Automatic returns tried before giving up.
pub const RETURN_TRIES: u32 = 3;

/// What a waiting return asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnStep {
    Wait,
    /// The region has been up for `UP_FOR`: offer or make the teleport.
    Ready,
    /// The user went elsewhere by themselves, or back already: dropped.
    Cancelled,
    Expired,
}

/// The way back to the region left, once it has restarted.
#[derive(Debug, Clone)]
pub struct ReturnTrip {
    pub origin: Origin,
    restart_at: Instant,
    /// Where the evacuation arrived: the user is expected to wait there.
    arrival: RegionHandle,
    /// The first "up" answer of the unbroken run, and the latest answer.
    up_since: Option<Instant>,
    latest: Option<Instant>,
    cancelled: bool,
    tries: u32,
}

impl ReturnTrip {
    pub fn new(origin: Origin, restart_at: Instant, arrival: RegionHandle) -> ReturnTrip {
        ReturnTrip {
            origin,
            restart_at,
            arrival,
            up_since: None,
            latest: None,
            cancelled: false,
            tries: 0,
        }
    }

    /// The user asked for a teleport of their own.
    pub fn user_teleport(&mut self) {
        self.cancelled = true;
    }

    /// The agent's region, once no teleport is under way: anywhere but
    /// where the evacuation arrived, the user moved on (or came back).
    pub fn region(&mut self, handle: RegionHandle) {
        if handle != self.arrival {
            self.cancelled = true;
        }
    }

    /// An answer of the world map about the region, received at
    /// `answered`. Answers from before the restart tell nothing.
    pub fn observe(&mut self, answered: Instant, up: bool) {
        if answered <= self.restart_at || self.latest.is_some_and(|t| answered <= t) {
            return;
        }
        self.latest = Some(answered);
        if up {
            self.up_since.get_or_insert(answered);
        } else {
            self.up_since = None;
        }
    }

    /// Each frame. Ready needs a second "up" answer after the first (one
    /// answer alone may be a moment of the restart) and `UP_FOR` since the
    /// first.
    pub fn step(&self, now: Instant) -> ReturnStep {
        if self.cancelled {
            return ReturnStep::Cancelled;
        }
        if now.saturating_duration_since(self.restart_at) >= RETURN_EXPIRES {
            return ReturnStep::Expired;
        }
        match (self.up_since, self.latest) {
            (Some(since), Some(latest)) if latest > since && now.saturating_duration_since(since) >= UP_FOR => ReturnStep::Ready,
            _ => ReturnStep::Wait,
        }
    }

    /// The teleport back failed: the region is waited for again. False
    /// after `RETURN_TRIES`.
    pub fn retry(&mut self) -> bool {
        self.tries += 1;
        self.up_since = None;
        self.tries < RETURN_TRIES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lm(n: u128, reach: Reach) -> Candidate {
        Candidate {
            asset: Some(Uuid::from_u128(n)),
            name: format!("lm{n}"),
            reach,
        }
    }

    fn names(d: &[Destination]) -> Vec<String> {
        d.iter()
            .map(|d| match d {
                Destination::Landmark { name, .. } => name.clone(),
                Destination::Home => "home".into(),
            })
            .collect()
    }

    fn origin() -> Origin {
        Origin {
            handle: 1,
            region: "Origin".into(),
            position: glam::Vec3::new(12.5, 200.25, 31.0),
        }
    }

    #[test]
    fn the_defaults_offer_and_old_files_load() {
        let s = Settings::default();
        assert_eq!((s.evacuate, s.come_back, s.lead_seconds), (Evacuate::Offer, ComeBack::Offer, 60));
        let old: Settings = serde_json::from_str("{}").expect("empty settings");
        assert_eq!(old, s);
    }

    #[test]
    fn the_list_holds_eight_landmarks_once_each() {
        let mut s = Settings::default();
        for n in 0..MAX_LANDMARKS as u128 {
            assert!(s.add(Uuid::from_u128(n + 1), "x"));
        }
        assert!(!s.add(Uuid::from_u128(99), "one too many"));
        s.landmarks.truncate(2);
        assert!(!s.add(Uuid::from_u128(1), "again"));
    }

    #[test]
    fn automatic_leaves_at_its_lead_time_only() {
        let mut s = Settings::default();
        // "Offer" never leaves by itself
        assert!(!s.due(true, 5));
        s.evacuate = Evacuate::Automatic;
        assert!(!s.due(true, 61));
        assert!(s.due(true, 60));
        // a first notice shorter than the lead: at once
        assert!(s.due(true, 20));
        // cancelled, left, at zero
        assert!(!s.due(false, 20));
        s.lead_seconds = 0;
        assert_eq!(s.lead(), 10);
    }

    #[test]
    fn destinations_keep_the_order_skip_the_unusable_and_end_at_home() {
        let gone = Candidate {
            asset: None,
            name: "other account".into(),
            reach: Reach::Unknown,
        };
        let list = [
            lm(1, Reach::Down),
            gone,
            lm(2, Reach::Unknown),
            lm(3, Reach::Here),
            lm(4, Reach::Up),
            lm(2, Reach::Up),
        ];
        assert_eq!(names(&destinations(&list)), ["lm2", "lm4", "home"]);
        assert_eq!(names(&destinations(&[])), ["home"]);
    }

    #[test]
    fn a_failed_teleport_falls_over_to_the_next_destination_then_home() {
        let now = Instant::now();
        let d = destinations(&[lm(1, Reach::Up), lm(2, Reach::Unknown)]);
        let mut e = Evacuation::new(origin(), now, d);
        // the first is asked whatever the last teleport of the session was
        assert!(matches!(e.step(Seen::Failed), Step::Try(Destination::Landmark { ref name, .. }) if name == "lm1"));
        assert_eq!(e.step(Seen::Pending), Step::Wait);
        assert!(matches!(e.step(Seen::Failed), Step::Try(Destination::Landmark { ref name, .. }) if name == "lm2"));
        // it led into the restarting region itself
        assert_eq!(e.step(Seen::Arrived(1)), Step::Try(Destination::Home));
        assert_eq!(e.step(Seen::Pending), Step::Wait);
        assert_eq!(e.step(Seen::Arrived(7)), Step::Done(7));
    }

    #[test]
    fn an_evacuation_with_nowhere_left_is_stranded() {
        let mut e = Evacuation::new(origin(), Instant::now(), destinations(&[]));
        assert_eq!(e.step(Seen::Pending), Step::Try(Destination::Home));
        assert_eq!(e.step(Seen::Failed), Step::Stranded);
    }

    fn trip(restart: Instant) -> ReturnTrip {
        ReturnTrip::new(origin(), restart, 7)
    }

    #[test]
    fn the_return_waits_for_the_region_to_be_up_for_a_minute() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut r = trip(at(60));
        // before the restart the map still shows the region up: no meaning
        r.observe(at(30), true);
        assert_eq!(r.step(at(200)), ReturnStep::Wait);
        r.observe(at(90), false);
        r.observe(at(150), true);
        // one answer, even old enough, is not enough
        assert_eq!(r.step(at(215)), ReturnStep::Wait);
        r.observe(at(180), false);
        // down again: the minute starts over
        r.observe(at(240), true);
        r.observe(at(299), true);
        assert_eq!(r.step(at(299)), ReturnStep::Wait);
        assert_eq!(r.step(at(300)), ReturnStep::Ready);
        // the place to come back to is the exact one
        assert_eq!(r.origin.position, glam::Vec3::new(12.5, 200.25, 31.0));
    }

    #[test]
    fn an_answer_older_than_the_latest_is_ignored() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut r = trip(t0);
        r.observe(at(100), true);
        r.observe(at(100), true);
        assert_eq!(r.step(at(400)), ReturnStep::Wait);
        r.observe(at(90), false);
        r.observe(at(161), true);
        assert_eq!(r.step(at(161)), ReturnStep::Ready);
    }

    #[test]
    fn the_return_is_cancelled_when_the_user_goes_elsewhere() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let ready = |r: &mut ReturnTrip| {
            r.observe(at(10), true);
            r.observe(at(71), true);
        };
        // waiting where the evacuation arrived: kept
        let mut r = trip(t0);
        ready(&mut r);
        r.region(7);
        assert_eq!(r.step(at(80)), ReturnStep::Ready);
        // a teleport asked by the user
        let mut r = trip(t0);
        ready(&mut r);
        r.user_teleport();
        assert_eq!(r.step(at(80)), ReturnStep::Cancelled);
        // another region reached some other way (a teleport offer, a script)
        let mut r = trip(t0);
        ready(&mut r);
        r.region(9);
        assert_eq!(r.step(at(80)), ReturnStep::Cancelled);
        // back by themselves
        let mut r = trip(t0);
        r.region(1);
        assert_eq!(r.step(at(80)), ReturnStep::Cancelled);
    }

    #[test]
    fn the_return_expires_and_retries_three_times() {
        let t0 = Instant::now();
        let mut r = trip(t0);
        assert_eq!(r.step(t0 + RETURN_EXPIRES - Duration::from_secs(1)), ReturnStep::Wait);
        assert_eq!(r.step(t0 + RETURN_EXPIRES), ReturnStep::Expired);
        let at = |s: u64| t0 + Duration::from_secs(s);
        r.observe(at(10), true);
        r.observe(at(71), true);
        assert_eq!(r.step(at(71)), ReturnStep::Ready);
        // the teleport failed: a new minute of "up" answers is needed
        assert!(r.retry());
        assert_eq!(r.step(at(72)), ReturnStep::Wait);
        r.observe(at(131), true);
        r.observe(at(191), true);
        assert_eq!(r.step(at(191)), ReturnStep::Ready);
        assert!(r.retry());
        assert!(!r.retry());
    }
}
