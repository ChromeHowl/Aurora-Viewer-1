//! Automatic reconnection after a lost session (Preferences › Confirmations
//! › Staying online, off unless the user turns it on): which ends of a
//! session may be followed by a login made by the viewer itself, when each
//! attempt is made, and when it stops for good. Aurora's own: the Legacy
//! Viewer goes back to its login screen and waits for the user.
//!
//! The rules only: `app/auto_relogin.rs` runs them over the normal login
//! path (`App::start_login`, the saved login of the login screen), and
//! `ui/auto_relogin.rs` draws the countdown.

use aurora_net::login::StartLocation;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Preferences › Confirmations › Staying online.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Log in again by itself after a lost link or a region restart.
    pub enabled: bool,
    /// Also on grids other than Second Life.
    pub other_grids: bool,
    /// Minutes after which the attempts stop.
    pub give_up_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: false,
            other_grids: false,
            give_up_minutes: 30,
        }
    }
}

impl Settings {
    pub fn give_up(&self) -> Duration {
        Duration::from_secs(u64::from(self.give_up_minutes.clamp(GIVE_UP_MINUTES.0, GIVE_UP_MINUTES.1)) * 60)
    }
}

/// The range of `Settings::give_up_minutes`.
pub const GIVE_UP_MINUTES: (u32, u32) = (5, 240);

/// How a session, or an attempt to open one, ended: exactly what the
/// network layer tells apart (`NetEvent::Disconnected`, `LoggedOut`,
/// `LoginFailed`), and what the viewer knows besides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// `NetEvent::Disconnected`. `lost`: the link went silent (dead
    /// circuit, lost event queue, DisableSimulator, a login region that
    /// never answered) rather than a KickUser of the grid. `restart`: the
    /// region's restart count was in its last seconds
    /// (`App::restart_disconnect`). `user_logout`: the user had asked to log
    /// out, the session ended before the grid's reply.
    Disconnected { lost: bool, restart: bool, user_logout: bool },
    /// `NetEvent::LoggedOut`: the LogoutReply to the user's own logout.
    LoggedOut,
    /// `NetEvent::LoginFailed`, with its flags.
    LoginFailed {
        mfa_required: bool,
        tos_required: bool,
        critical_required: bool,
        bad_credentials: bool,
        previous_session: bool,
        network: bool,
    },
}

/// Why the viewer may log in again by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// The link with the region was lost.
    LinkLost,
    /// The region restarted under the agent.
    RegionRestart,
    /// The login server could not be reached.
    NoAnswer,
}

/// Why it never does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Never {
    /// The user logged out.
    UserLogout,
    /// A KickUser of the grid: another login of the account, an
    /// administrator, a ban, inactivity. The grid gives them as free text
    /// only, so none of them is told from the others and none is retried.
    Kicked,
    /// Wrong name or password (reason "key").
    BadCredentials,
    /// The Terms of Service are to be accepted (reason "tos").
    Terms,
    /// A message of the grid is to be read (reason "critical").
    CriticalMessage,
    /// A second factor code is asked (reason "mfa_challenge").
    SecondFactor,
    /// The grid still holds a session of the account after the login's own
    /// waits (reason "presence", `app/relogin.rs`).
    PreviousSession,
    /// Any other refusal of the login server (a suspended account, a closed
    /// grid, a required update): it answered, and said no.
    Refused,
}

/// The one place that decides whether an end may be followed by an
/// automatic login.
pub fn classify(end: End) -> Result<Cause, Never> {
    match end {
        End::LoggedOut => Err(Never::UserLogout),
        End::Disconnected { user_logout: true, .. } => Err(Never::UserLogout),
        // the restart's own kick, or its silence
        End::Disconnected { restart: true, .. } => Ok(Cause::RegionRestart),
        End::Disconnected { lost: true, .. } => Ok(Cause::LinkLost),
        End::Disconnected { lost: false, .. } => Err(Never::Kicked),
        // the refusals first: a flag of the server wins over anything else
        End::LoginFailed { bad_credentials: true, .. } => Err(Never::BadCredentials),
        End::LoginFailed { tos_required: true, .. } => Err(Never::Terms),
        End::LoginFailed {
            critical_required: true, ..
        } => Err(Never::CriticalMessage),
        End::LoginFailed { mfa_required: true, .. } => Err(Never::SecondFactor),
        End::LoginFailed {
            previous_session: true, ..
        } => Err(Never::PreviousSession),
        End::LoginFailed { network: true, .. } => Ok(Cause::NoAnswer),
        End::LoginFailed { .. } => Err(Never::Refused),
    }
}

/// The wait before each attempt: 30 s, then 60, 120, and 300 from then on.
pub const BACKOFF: [u64; 4] = [30, 60, 120, 300];

/// The wait before attempt number `attempt` (0: the first).
pub fn delay(attempt: u32) -> Duration {
    let i = (attempt as usize).min(BACKOFF.len() - 1);
    Duration::from_secs(BACKOFF[i])
}

/// A session opened by an attempt counts as kept once it lasted this long;
/// lost sooner, the attempts go on where they were.
pub const STABLE_AFTER: Duration = Duration::from_secs(60);

/// More automatic logins than this within `LOOP_WINDOW` stop the automation
/// (two viewers that log each other out, a region that drops every arrival).
pub const LOOP_LOGINS: usize = 5;
pub const LOOP_WINDOW: Duration = Duration::from_secs(15 * 60);

/// The automatic logins that succeeded lately.
#[derive(Debug, Default)]
pub struct LoopGuard {
    logins: VecDeque<Instant>,
}

impl LoopGuard {
    fn trim(&mut self, now: Instant) {
        while self.logins.front().is_some_and(|t| now.duration_since(*t) >= LOOP_WINDOW) {
            self.logins.pop_front();
        }
    }

    /// An automatic login succeeded.
    pub fn record(&mut self, now: Instant) {
        self.trim(now);
        self.logins.push_back(now);
    }

    /// One more automatic login would be one too many.
    pub fn tripped(&mut self, now: Instant) -> bool {
        self.trim(now);
        self.logins.len() >= LOOP_LOGINS
    }
}

/// Where a run of attempts stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Counting down to the next attempt.
    Waiting { since: Instant, until: Instant },
    /// A login is under way.
    Connecting,
    /// Logged in since then, not yet for `STABLE_AFTER`.
    Online { since: Instant },
}

/// What a waiting run asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Nothing to do (a login under way, a session).
    Idle,
    /// Still counting: the time left, and how much of this wait is left (0..1).
    Wait { left: Duration, fraction: f32 },
    /// Log in now.
    Attempt,
    /// The give-up time is over.
    GiveUp,
}

/// The attempts that follow one lost session.
#[derive(Debug, Clone)]
pub struct Run {
    /// The account that was logged in: (login URI, lowercase user name).
    pub key: (String, String),
    /// Where it was (`ui::link_health::reconnect_place`).
    pub place: Option<StartLocation>,
    pub cause: Cause,
    started: Instant,
    /// Attempts made so far.
    attempts: u32,
    phase: Phase,
}

impl Run {
    pub fn new(now: Instant, key: (String, String), place: Option<StartLocation>, cause: Cause) -> Run {
        Run {
            key,
            place,
            cause,
            started: now,
            attempts: 0,
            phase: Phase::Waiting {
                since: now,
                until: now + delay(0),
            },
        }
    }

    /// The number of the attempt counted down to, from 1.
    pub fn attempt(&self) -> u32 {
        self.attempts + 1
    }

    pub fn waiting(&self) -> bool {
        matches!(self.phase, Phase::Waiting { .. })
    }

    pub fn connecting(&self) -> bool {
        self.phase == Phase::Connecting
    }

    /// Each frame of the login screen. `Attempt` is told once: the run is
    /// connecting from then on.
    pub fn poll(&mut self, now: Instant, give_up: Duration) -> Step {
        let Phase::Waiting { since, until } = self.phase else {
            return Step::Idle;
        };
        if now.duration_since(self.started) >= give_up {
            return Step::GiveUp;
        }
        if now >= until {
            self.attempts += 1;
            self.phase = Phase::Connecting;
            return Step::Attempt;
        }
        let whole = until.duration_since(since).as_secs_f32().max(1.0);
        let left = until.duration_since(now);
        Step::Wait {
            left,
            fraction: (left.as_secs_f32() / whole).clamp(0.0, 1.0),
        }
    }

    /// "Try now": the count ends at once.
    pub fn try_now(&mut self, now: Instant) {
        if let Phase::Waiting { since, .. } = self.phase {
            self.phase = Phase::Waiting { since, until: now };
        }
    }

    /// The attempt failed, or its session was lost before it was stable:
    /// the next wait starts, longer. False when the give-up time is over.
    pub fn failed(&mut self, now: Instant, give_up: Duration) -> bool {
        if now.duration_since(self.started) >= give_up {
            return false;
        }
        self.phase = Phase::Waiting {
            since: now,
            until: now + delay(self.attempts),
        };
        true
    }

    /// The attempt logged in.
    pub fn logged_in(&mut self, now: Instant) {
        self.phase = Phase::Online { since: now };
    }

    /// The session of the last attempt lasted: the run is over.
    pub fn stable(&self, now: Instant) -> bool {
        matches!(self.phase, Phase::Online { since } if now.duration_since(since) >= STABLE_AFTER)
    }

    /// Logged in by an attempt (stable or not yet).
    pub fn online(&self) -> bool {
        matches!(self.phase, Phase::Online { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disconnected(lost: bool, restart: bool, user_logout: bool) -> End {
        End::Disconnected {
            lost,
            restart,
            user_logout,
        }
    }

    fn failed() -> [bool; 6] {
        [false; 6]
    }

    fn login_failed(f: [bool; 6]) -> End {
        End::LoginFailed {
            mfa_required: f[0],
            tos_required: f[1],
            critical_required: f[2],
            bad_credentials: f[3],
            previous_session: f[4],
            network: f[5],
        }
    }

    #[test]
    fn a_lost_link_and_a_region_restart_are_followed_by_a_login() {
        // the dead circuit, the lost event queue, DisableSimulator
        assert_eq!(classify(disconnected(true, false, false)), Ok(Cause::LinkLost));
        // the restart ends the session by a kick or by silence
        assert_eq!(classify(disconnected(false, true, false)), Ok(Cause::RegionRestart));
        assert_eq!(classify(disconnected(true, true, false)), Ok(Cause::RegionRestart));
    }

    #[test]
    fn a_kick_is_never_followed_by_a_login() {
        // KickUser outside a restart: a second login of the account, an
        // administrator, a ban, inactivity
        assert_eq!(classify(disconnected(false, false, false)), Err(Never::Kicked));
    }

    #[test]
    fn the_users_own_logout_is_never_followed_by_a_login() {
        assert_eq!(classify(End::LoggedOut), Err(Never::UserLogout));
        // the link dropped while the logout was on its way, restart or not
        assert_eq!(classify(disconnected(true, false, true)), Err(Never::UserLogout));
        assert_eq!(classify(disconnected(true, true, true)), Err(Never::UserLogout));
        assert_eq!(classify(disconnected(false, false, true)), Err(Never::UserLogout));
    }

    #[test]
    fn a_refused_login_stops_and_a_silent_server_is_tried_again() {
        let with = |i: usize| {
            let mut f = failed();
            f[i] = true;
            login_failed(f)
        };
        assert_eq!(classify(with(0)), Err(Never::SecondFactor));
        assert_eq!(classify(with(1)), Err(Never::Terms));
        assert_eq!(classify(with(2)), Err(Never::CriticalMessage));
        assert_eq!(classify(with(3)), Err(Never::BadCredentials));
        assert_eq!(classify(with(4)), Err(Never::PreviousSession));
        assert_eq!(classify(with(5)), Ok(Cause::NoAnswer));
        // answered, with a reason the viewer does not know: a suspension,
        // a closed grid
        assert_eq!(classify(login_failed(failed())), Err(Never::Refused));
        // a flag of the server wins over the network one
        let mut both = failed();
        both[3] = true;
        both[5] = true;
        assert_eq!(classify(login_failed(both)), Err(Never::BadCredentials));
    }

    #[test]
    fn the_waits_grow_to_five_minutes() {
        let secs: Vec<u64> = (0..7).map(|a| delay(a).as_secs()).collect();
        assert_eq!(secs, [30, 60, 120, 300, 300, 300, 300]);
    }

    fn run(now: Instant) -> Run {
        Run::new(now, ("grid".into(), "user".into()), None, Cause::LinkLost)
    }

    #[test]
    fn a_run_counts_down_tries_and_backs_off() {
        let t0 = Instant::now();
        let give_up = Duration::from_secs(30 * 60);
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut r = run(t0);
        assert_eq!(r.attempt(), 1);
        let Step::Wait { left, fraction } = r.poll(at(10), give_up) else {
            panic!("waiting");
        };
        assert_eq!(left.as_secs(), 20);
        assert!((fraction - 2.0 / 3.0).abs() < 1e-3);
        assert_eq!(r.poll(at(30), give_up), Step::Attempt);
        assert!(r.connecting());
        // told once
        assert_eq!(r.poll(at(31), give_up), Step::Idle);
        // the region is still down: 60 s, then 120, then 300
        assert!(r.failed(at(40), give_up));
        assert_eq!(r.attempt(), 2);
        assert!(matches!(r.poll(at(99), give_up), Step::Wait { left, .. } if left.as_secs() == 1));
        assert_eq!(r.poll(at(100), give_up), Step::Attempt);
        assert!(r.failed(at(100), give_up));
        assert!(matches!(r.poll(at(100), give_up), Step::Wait { left, .. } if left.as_secs() == 120));
        assert_eq!(r.poll(at(220), give_up), Step::Attempt);
        assert!(r.failed(at(220), give_up));
        assert!(matches!(r.poll(at(220), give_up), Step::Wait { left, .. } if left.as_secs() == 300));
    }

    #[test]
    fn try_now_ends_the_count() {
        let t0 = Instant::now();
        let give_up = Duration::from_secs(1800);
        let mut r = run(t0);
        let now = t0 + Duration::from_secs(3);
        r.try_now(now);
        assert_eq!(r.poll(now, give_up), Step::Attempt);
        // nothing to hurry while a login runs
        r.try_now(now);
        assert!(r.connecting());
    }

    #[test]
    fn a_run_gives_up_after_its_time() {
        let t0 = Instant::now();
        let give_up = Duration::from_secs(5 * 60);
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut r = run(t0);
        assert_eq!(r.poll(at(30), give_up), Step::Attempt);
        assert!(r.failed(at(31), give_up));
        assert_eq!(r.poll(at(91), give_up), Step::Attempt);
        assert!(r.failed(at(92), give_up));
        assert_eq!(r.poll(at(212), give_up), Step::Attempt);
        // the next wait would end after the five minutes: given up while waiting
        assert!(r.failed(at(213), give_up));
        assert!(matches!(r.poll(at(299), give_up), Step::Wait { .. }));
        assert_eq!(r.poll(at(300), give_up), Step::GiveUp);
        // an attempt that fails after the time does not start another wait
        let mut late = run(t0);
        assert_eq!(late.poll(at(30), give_up), Step::Attempt);
        assert!(!late.failed(at(301), give_up));
    }

    #[test]
    fn a_session_lost_at_once_continues_the_run_and_a_kept_one_ends_it() {
        let t0 = Instant::now();
        let give_up = Duration::from_secs(1800);
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut r = run(t0);
        assert_eq!(r.poll(at(30), give_up), Step::Attempt);
        r.logged_in(at(35));
        assert!(r.online() && !r.stable(at(60)));
        // the login region never answered: the second wait, not the first again
        assert!(r.failed(at(50), give_up));
        assert!(matches!(r.poll(at(50), give_up), Step::Wait { left, .. } if left.as_secs() == 60));
        assert_eq!(r.poll(at(110), give_up), Step::Attempt);
        r.logged_in(at(115));
        assert!(r.stable(at(115 + STABLE_AFTER.as_secs())));
    }

    #[test]
    fn the_loop_guard_allows_five_logins_in_fifteen_minutes() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut g = LoopGuard::default();
        for i in 0..4 {
            g.record(at(i * 100));
            assert!(!g.tripped(at(i * 100)));
        }
        g.record(at(400));
        // five so far: a sixth would be more than five
        assert!(g.tripped(at(401)));
        assert!(g.tripped(at(899)));
        // the first one is fifteen minutes old: room for one more
        assert!(!g.tripped(at(900)));
        g.record(at(900));
        assert!(g.tripped(at(901)));
        // an hour later the guard has forgotten
        assert!(!g.tripped(at(4600)));
    }

    #[test]
    fn the_settings_are_off_by_default_and_old_files_load() {
        let s = Settings::default();
        assert!(!s.enabled && !s.other_grids);
        assert_eq!(s.give_up(), Duration::from_secs(30 * 60));
        let old: Settings = serde_json::from_str("{}").expect("empty settings");
        assert_eq!(old, s);
        let wild: Settings = serde_json::from_str(r#"{"give_up_minutes":0}"#).expect("settings");
        assert_eq!(wild.give_up(), Duration::from_secs(5 * 60));
    }
}
