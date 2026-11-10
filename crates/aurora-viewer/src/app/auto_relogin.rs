//! Automatic reconnection after a lost link or a region restart
//! (Preferences › Confirmations › Staying online, off by default): the rules
//! of `auto_relogin.rs` run over the login screen's own path. The viewer
//! presses "Reconnect" for the user, nothing more: the same `start_login`,
//! the same saved login, the same waits for a previous session
//! (`app/relogin.rs`), the same place (`ui::link_health::reconnect_place`).

use super::{App, Screen};
use crate::auto_relogin::{self as rules, Cause, End, LoopGuard, Run, Step};
use crate::ui::auto_relogin as card;
use aurora_net::login::StartLocation;
use std::time::Instant;

#[derive(Default)]
pub(super) struct AutoRelogin {
    /// The attempts that follow the last lost session.
    run: Option<Run>,
    guard: LoopGuard,
    /// The user asked to log out of the running session.
    user_logout: bool,
}

impl App {
    /// The user asked to log out (the top bar, the exit prompt, the
    /// cancelled arrival): whatever ends the session now is their own doing.
    pub(super) fn note_user_logout(&mut self) {
        self.auto_relogin.user_logout = true;
        self.auto_relogin.run = None;
    }

    /// Second Life, or any grid when the user allowed it.
    fn auto_relogin_on_this_grid(&self) -> bool {
        let s = &self.settings.auto_relogin;
        s.enabled && (s.other_grids || !matches!(self.settings.grid, crate::settings::GridChoice::Custom))
    }

    /// `on_disconnected`, before the login screen comes back: whether the
    /// viewer will log in again by itself, and why. A run already under way
    /// (its session did not last) goes on when the end allows it.
    pub(super) fn auto_relogin_cause(&mut self, lost: bool, restart: bool) -> Option<Cause> {
        let end = End::Disconnected {
            lost,
            restart,
            user_logout: self.auto_relogin.user_logout,
        };
        let verdict = rules::classify(end);
        let cause = match verdict {
            Ok(cause) if self.auto_relogin_on_this_grid() => cause,
            Ok(_) => {
                self.auto_relogin.run = None;
                return None;
            }
            Err(never) => {
                if self.settings.auto_relogin.enabled {
                    log::info!("automatic reconnection: not for this end of session ({never:?})");
                }
                self.auto_relogin.run = None;
                return None;
            }
        };
        if self.auto_relogin.guard.tripped(Instant::now()) {
            log::warn!(
                "automatic reconnection: stopped, {} automatic logins in {} minutes",
                rules::LOOP_LOGINS,
                rules::LOOP_WINDOW.as_secs() / 60
            );
            self.auto_relogin.run = None;
            return None;
        }
        Some(cause)
    }

    /// `on_disconnected`, once the login screen is back: the count starts
    /// (or goes on, when the session of the last attempt did not last).
    pub(super) fn auto_relogin_arm(&mut self, cause: Cause, place: Option<StartLocation>) {
        let now = Instant::now();
        let give_up = self.settings.auto_relogin.give_up();
        if let Some(run) = self.auto_relogin.run.as_mut() {
            if run.place.is_none() {
                run.place = place;
            }
            if !run.failed(now, give_up) {
                self.auto_relogin_gave_up();
            }
        } else {
            log::info!("automatic reconnection: armed ({cause:?})");
            self.auto_relogin.run = Some(Run::new(now, self.settings.login_key(), place, cause));
        }
        // "Reconnect" by hand goes to the same place
        self.auto_relogin_keep_place();
        self.login_form.reconnect = true;
    }

    /// The loop guard stopped a reconnection that the end of the session
    /// allowed: said on the login screen.
    pub(super) fn auto_relogin_loop_message(&self, lost: bool, restart: bool) -> Option<String> {
        let allowed = rules::classify(End::Disconnected {
            lost,
            restart,
            user_logout: self.auto_relogin.user_logout,
        })
        .is_ok();
        (allowed && self.auto_relogin_on_this_grid() && self.auto_relogin.run.is_none()).then(|| {
            aurora_i18n::tr!(
                "stay-online-relogin-loop",
                count = rules::LOOP_LOGINS as i64,
                minutes = (rules::LOOP_WINDOW.as_secs() / 60) as i64
            )
        })
    }

    /// The place of the run, for the next `start_login` (which takes it).
    fn auto_relogin_keep_place(&mut self) {
        if let Some(run) = &self.auto_relogin.run
            && let Some(place) = &run.place
        {
            self.reconnect_place = Some((run.key.0.clone(), run.key.1.clone(), place.clone()));
        }
    }

    fn auto_relogin_gave_up(&mut self) {
        let minutes = i64::from(self.settings.auto_relogin.give_up().as_secs() as u32 / 60);
        log::info!("automatic reconnection: given up after {minutes} minutes");
        self.auto_relogin_keep_place();
        self.auto_relogin.run = None;
        self.login_form.error = Some(aurora_i18n::tr!("stay-online-relogin-gave-up", minutes = minutes));
        self.login_form.reconnect = true;
    }

    /// LoggedIn: an attempt succeeded (counted by the loop guard), or the
    /// user logged in by hand.
    pub(super) fn auto_relogin_logged_in(&mut self) {
        self.auto_relogin.user_logout = false;
        let now = Instant::now();
        match self.auto_relogin.run.as_mut() {
            Some(run) if run.connecting() => {
                run.logged_in(now);
                self.auto_relogin.guard.record(now);
                log::info!("automatic reconnection: logged in");
            }
            _ => self.auto_relogin.run = None,
        }
    }

    /// LoginFailed, once the login screen shows it: a login server that did
    /// not answer is tried again, a refusal ends the attempts.
    pub(super) fn auto_relogin_login_failed(&mut self, end: End) {
        let Some(run) = self.auto_relogin.run.as_mut().filter(|run| run.connecting()) else {
            return;
        };
        match rules::classify(end) {
            Ok(_) => {
                let give_up = self.settings.auto_relogin.give_up();
                if run.failed(Instant::now(), give_up) {
                    self.login_form.reconnect = true;
                } else {
                    self.auto_relogin_gave_up();
                }
            }
            Err(never) => {
                log::info!("automatic reconnection: stopped by the login server ({never:?})");
                self.auto_relogin.run = None;
            }
        }
    }

    /// While in world: a session opened by an attempt that lasted ends the
    /// run; the next lost link starts a new one.
    pub(super) fn auto_relogin_in_world(&mut self) {
        if self.auto_relogin.run.as_ref().is_some_and(|run| run.stable(Instant::now())) {
            self.auto_relogin.run = None;
        }
    }

    /// Each frame of the login screen, after the saved login was read: the
    /// count, its card, and the login when it is over.
    pub(super) fn poll_auto_relogin(&mut self, ctx: &egui::Context, p: &crate::theme::Palette) {
        if !matches!(self.screen, Screen::Login) {
            return;
        }
        let off = !self.auto_relogin_on_this_grid();
        let Some(run) = self.auto_relogin.run.as_mut() else {
            return;
        };
        let now = Instant::now();
        if run.online() || (run.connecting() && !self.login_form.busy) {
            // the arrival was cancelled, or the wait for a previous
            // session was (Escape): the user took over
            self.auto_relogin.run = None;
            return;
        }
        if !run.waiting() {
            return;
        }
        // only for the account that was logged in, with its saved login
        let same_account = self.settings.login_key() == run.key;
        let read = self.login_form.stored_for.as_ref() == Some(&run.key);
        let saved = self.login_form.stored.is_some();
        if off || !same_account || (read && !saved) || self.login_form.block_reason.is_some() {
            log::info!("automatic reconnection: dropped (setting off, another account, no saved login or logins blocked)");
            self.auto_relogin.run = None;
            return;
        }
        if !read || self.login_form.busy || self.login_form.editing || self.login_form.adding {
            // the saved login is not read yet, or the form is in use
            return;
        }
        match run.poll(now, self.settings.auto_relogin.give_up()) {
            Step::Idle => {}
            Step::GiveUp => self.auto_relogin_gave_up(),
            Step::Wait { left, fraction } => {
                let region = match &run.place {
                    Some(StartLocation::Region { name, .. }) => Some(name.as_str()),
                    _ => None,
                };
                let view = card::View {
                    cause: run.cause,
                    region,
                    seconds: card::seconds_left(left),
                    fraction,
                    attempt: run.attempt(),
                };
                match card::show(ctx, p, &view) {
                    Some(card::Action::TryNow) => run.try_now(now),
                    Some(card::Action::Cancel) => {
                        log::info!("automatic reconnection: cancelled");
                        self.auto_relogin_keep_place();
                        self.auto_relogin.run = None;
                    }
                    Some(card::Action::LoginScreen) => {
                        log::info!("automatic reconnection: cancelled, back to the usual login screen");
                        self.auto_relogin.run = None;
                        self.reconnect_place = None;
                        self.login_form.reconnect = false;
                        self.login_form.error = None;
                    }
                    None => {}
                }
            }
            Step::Attempt => {
                if self.auto_relogin.guard.tripped(now) {
                    self.auto_relogin.run = None;
                    self.login_form.error = Some(aurora_i18n::tr!(
                        "stay-online-relogin-loop",
                        count = rules::LOOP_LOGINS as i64,
                        minutes = (rules::LOOP_WINDOW.as_secs() / 60) as i64
                    ));
                    return;
                }
                log::info!("automatic reconnection: attempt {}", run.attempt() - 1);
                self.auto_relogin_keep_place();
                // what the login button does (`credentials_form::content`)
                self.login_form.error = None;
                self.login_form.tos_message = None;
                self.login_form.critical_message = None;
                self.login_form.show_mfa = false;
                self.start_login();
                if !self.login_form.busy {
                    // not sent (the offline demo, a start place that no
                    // longer resolves): counted as an attempt without answer
                    let give_up = self.settings.auto_relogin.give_up();
                    let retry = self.auto_relogin.run.as_mut().map(|run| run.failed(Instant::now(), give_up));
                    match retry {
                        Some(true) => self.login_form.reconnect = true,
                        Some(false) => self.auto_relogin_gave_up(),
                        None => {}
                    }
                }
            }
        }
    }
}
