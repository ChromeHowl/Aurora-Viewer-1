//! Which camera mode the user is in, as one name, and when to tell them it
//! changed. Aurora's own addition: the Legacy Viewer says nothing when the
//! camera changes mode (only the mouselook instructions of
//! FSShowMouselookInstructions, `LLViewerWindow::drawMouselookInstructions`).
//!
//! The camera has three modes of its own (`CameraMode`: third person,
//! scripted follow, mouselook); the rest are states layered over them (the
//! Director's track, a path, the lock, the free camera, the flycam, a seat's
//! camera, the object view). `status` folds those facts into the one that
//! is in force, in the order the camera itself obeys them (`Camera::update`:
//! the Director's track first, then the flycam, then the free camera), with
//! what a lock or a path may not be combined with refused where they start
//! (`app/camera_tools.rs`: a lock or a path is refused in mouselook, under
//! the Director and with the free camera or the flycam on).
//!
//! Pure: no window, no clock but the one it is given. The pill and the chip
//! are drawn by `ui/camera_status.rs`, the facts gathered by
//! `app/camera_status.rs`.

/// The camera mode in force, the most specific first (see `status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The Director's track holds the view (playing, scrubbed or held).
    Director,
    /// A Camera Path plays, or holds its last shot.
    PathPlaying,
    /// The Camera Path records the user's moves as waypoints.
    PathRecording,
    /// The Camera Lock holds the view.
    Locked,
    /// The 3D mouse or gamepad flies the camera.
    Flycam,
    /// The free camera (photo panel, viewfinder, Director's switch).
    Free,
    /// First person.
    Mouselook,
    /// A script moves the camera (llSetCameraParams).
    ScriptedFollow,
    /// A seat's own camera (llSetCameraEyeOffset and friends).
    Seated,
    /// The Controls window's object view: clicks act as Alt-clicks.
    ObjectView,
}

impl Status {
    /// A mode that stays until the user leaves it, or a script or a window
    /// ends it: the ones that get a small chip while they last. Mouselook
    /// has its crosshair and its instructions; the flycam its own status
    /// (`joystick_status`); a seat and the object view are not held.
    pub fn held(self) -> bool {
        matches!(
            self,
            Status::Director | Status::PathPlaying | Status::PathRecording | Status::Locked | Status::Free | Status::ScriptedFollow
        )
    }

    /// Leaving it is not worth a "normal view" pill: a seat's camera ends
    /// with the seat, the object view with a click.
    pub fn quiet_to_leave(self) -> bool {
        matches!(self, Status::Seated | Status::ObjectView)
    }
}

/// What the camera and the app know, as plain flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    pub director: bool,
    pub path_playing: bool,
    pub path_recording: bool,
    pub locked: bool,
    pub flycam: bool,
    pub free: bool,
    pub mouselook: bool,
    pub scripted_follow: bool,
    pub seated: bool,
    pub object_view: bool,
}

/// The mode in force, none in the plain third-person view. Priority:
/// Director > Path (playing, then recording) > Lock > Flycam > Free >
/// Mouselook > Scripted follow > Seat camera > Object view.
pub fn status(f: &Facts) -> Option<Status> {
    if f.director {
        Some(Status::Director)
    } else if f.path_playing {
        Some(Status::PathPlaying)
    } else if f.path_recording {
        Some(Status::PathRecording)
    } else if f.locked {
        Some(Status::Locked)
    } else if f.flycam {
        Some(Status::Flycam)
    } else if f.free {
        Some(Status::Free)
    } else if f.mouselook {
        Some(Status::Mouselook)
    } else if f.scripted_follow {
        Some(Status::ScriptedFollow)
    } else if f.seated {
        Some(Status::Seated)
    } else if f.object_view {
        Some(Status::ObjectView)
    } else {
        None
    }
}

/// How long the pill stays, seconds.
pub const PILL_SECONDS: f64 = 1.8;

/// What the pill says: a mode, or the return to the plain view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pill {
    /// `None`: back to the normal view.
    pub status: Option<Status>,
    /// The time (egui's clock, seconds) it goes at.
    pub until: f64,
}

/// Watches the status from frame to frame and decides when the pill shows:
/// when it changes, once per change, never on the first frame in the world
/// and never for the same mode twice in a row.
#[derive(Debug, Default)]
pub struct Announcer {
    /// What the last frame in the world had; `None` before the first.
    seen: Option<Option<Status>>,
    pill: Option<Pill>,
}

impl Announcer {
    /// One frame. `ready`: in the world (a login, a teleport or a logout
    /// forgets what was seen, so that the first frame after it is silent);
    /// `enabled`: Preferences' switch.
    pub fn update(&mut self, now: f64, now_status: Option<Status>, ready: bool, enabled: bool) -> Option<Pill> {
        if !ready {
            self.seen = None;
            self.pill = None;
            return None;
        }
        match self.seen {
            None => self.seen = Some(now_status),
            Some(before) if before != now_status => {
                self.seen = Some(now_status);
                self.pill = None;
                let silent = now_status.is_none() && before.is_some_and(Status::quiet_to_leave);
                if enabled && !silent {
                    self.pill = Some(Pill {
                        status: now_status,
                        until: now + PILL_SECONDS,
                    });
                }
            }
            Some(_) => {}
        }
        if !enabled {
            self.pill = None;
        }
        if self.pill.is_some_and(|p| now >= p.until) {
            self.pill = None;
        }
        self.pill
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(set: impl Fn(&mut Facts)) -> Facts {
        let mut f = Facts::default();
        set(&mut f);
        f
    }

    #[test]
    fn the_plain_view_has_no_status() {
        assert_eq!(status(&Facts::default()), None);
    }

    #[test]
    fn each_state_alone_gives_its_own_status() {
        assert_eq!(status(&only(|f| f.director = true)), Some(Status::Director));
        assert_eq!(status(&only(|f| f.path_playing = true)), Some(Status::PathPlaying));
        assert_eq!(status(&only(|f| f.path_recording = true)), Some(Status::PathRecording));
        assert_eq!(status(&only(|f| f.locked = true)), Some(Status::Locked));
        assert_eq!(status(&only(|f| f.flycam = true)), Some(Status::Flycam));
        assert_eq!(status(&only(|f| f.free = true)), Some(Status::Free));
        assert_eq!(status(&only(|f| f.mouselook = true)), Some(Status::Mouselook));
        assert_eq!(status(&only(|f| f.scripted_follow = true)), Some(Status::ScriptedFollow));
        assert_eq!(status(&only(|f| f.seated = true)), Some(Status::Seated));
        assert_eq!(status(&only(|f| f.object_view = true)), Some(Status::ObjectView));
    }

    #[test]
    fn the_priority_runs_from_the_director_down_to_the_object_view() {
        let order = [
            Status::Director,
            Status::PathPlaying,
            Status::PathRecording,
            Status::Locked,
            Status::Flycam,
            Status::Free,
            Status::Mouselook,
            Status::ScriptedFollow,
            Status::Seated,
            Status::ObjectView,
        ];
        let set = |f: &mut Facts, s: Status, on: bool| match s {
            Status::Director => f.director = on,
            Status::PathPlaying => f.path_playing = on,
            Status::PathRecording => f.path_recording = on,
            Status::Locked => f.locked = on,
            Status::Flycam => f.flycam = on,
            Status::Free => f.free = on,
            Status::Mouselook => f.mouselook = on,
            Status::ScriptedFollow => f.scripted_follow = on,
            Status::Seated => f.seated = on,
            Status::ObjectView => f.object_view = on,
        };
        // with everything on the first wins; take the winner away, the next wins
        let mut f = Facts::default();
        for s in order {
            set(&mut f, s, true);
        }
        for s in order {
            assert_eq!(status(&f), Some(s));
            set(&mut f, s, false);
        }
        assert_eq!(status(&f), None);
    }

    #[test]
    fn combinations_the_camera_refuses_still_resolve_to_one_mode() {
        // lock and free cannot both start (camera_takeover_refused, the
        // path's free check), nor a path under the lock; if one slipped
        // through, the lock holds the view
        assert_eq!(status(&only(|f| (f.locked, f.free) = (true, true))), Some(Status::Locked));
        assert_eq!(
            status(&only(|f| (f.path_playing, f.locked) = (true, true))),
            Some(Status::PathPlaying)
        );
        // the Director over everything
        let all = Facts {
            director: true,
            path_playing: true,
            locked: true,
            free: true,
            mouselook: true,
            ..Facts::default()
        };
        assert_eq!(status(&all), Some(Status::Director));
        // mouselook under a script's follow camera
        assert_eq!(
            status(&only(|f| (f.mouselook, f.scripted_follow) = (true, true))),
            Some(Status::Mouselook)
        );
    }

    #[test]
    fn only_the_modes_that_stay_get_a_chip() {
        assert!(Status::Locked.held() && Status::Free.held() && Status::PathPlaying.held());
        assert!(Status::PathRecording.held() && Status::ScriptedFollow.held() && Status::Director.held());
        assert!(!Status::Mouselook.held() && !Status::Flycam.held());
        assert!(!Status::Seated.held() && !Status::ObjectView.held());
    }

    #[test]
    fn the_first_frame_in_the_world_is_silent() {
        let mut a = Announcer::default();
        assert_eq!(a.update(10.0, Some(Status::Mouselook), true, true), None);
        assert_eq!(a.update(10.1, Some(Status::Mouselook), true, true), None);
    }

    #[test]
    fn a_change_shows_the_pill_for_its_time_and_then_stops() {
        let mut a = Announcer::default();
        a.update(0.0, None, true, true);
        let pill = a.update(1.0, Some(Status::Mouselook), true, true).expect("pill");
        assert_eq!(pill.status, Some(Status::Mouselook));
        assert!((pill.until - (1.0 + PILL_SECONDS)).abs() < 1e-9);
        assert!((1.5..=2.0).contains(&PILL_SECONDS));
        // still there just before its end, gone at it
        assert!(a.update(1.0 + PILL_SECONDS - 0.01, Some(Status::Mouselook), true, true).is_some());
        assert_eq!(a.update(1.0 + PILL_SECONDS, Some(Status::Mouselook), true, true), None);
        // and not shown again for the same mode
        assert_eq!(a.update(9.0, Some(Status::Mouselook), true, true), None);
    }

    #[test]
    fn a_new_mode_replaces_the_pill_and_leaving_says_normal_view() {
        let mut a = Announcer::default();
        a.update(0.0, None, true, true);
        a.update(1.0, Some(Status::Free), true, true);
        let pill = a.update(1.5, Some(Status::Locked), true, true).expect("pill");
        assert_eq!(pill.status, Some(Status::Locked));
        let back = a.update(2.0, None, true, true).expect("pill");
        assert_eq!(back.status, None);
        assert!((back.until - (2.0 + PILL_SECONDS)).abs() < 1e-9);
    }

    #[test]
    fn leaving_a_seat_camera_or_the_object_view_is_silent() {
        for quiet in [Status::Seated, Status::ObjectView] {
            let mut a = Announcer::default();
            a.update(0.0, Some(quiet), true, true);
            assert_eq!(a.update(1.0, None, true, true), None, "{quiet:?}");
        }
        // leaving a held mode is not
        let mut a = Announcer::default();
        a.update(0.0, Some(Status::Free), true, true);
        assert!(a.update(1.0, None, true, true).is_some());
    }

    #[test]
    fn the_switch_turns_the_pill_off_and_a_teleport_or_logout_forgets_the_mode() {
        let mut a = Announcer::default();
        a.update(0.0, None, true, false);
        assert_eq!(a.update(1.0, Some(Status::Mouselook), true, false), None);
        // switched on while in a mode: nothing until it changes
        assert_eq!(a.update(2.0, Some(Status::Mouselook), true, true), None);
        assert!(a.update(3.0, None, true, true).is_some());
        // not in the world: forgotten, the way back in is silent
        assert_eq!(a.update(4.0, Some(Status::Free), false, true), None);
        assert_eq!(a.update(5.0, Some(Status::Free), true, true), None);
    }

    #[test]
    fn turning_the_switch_off_removes_a_pill_that_shows() {
        let mut a = Announcer::default();
        a.update(0.0, None, true, true);
        assert!(a.update(1.0, Some(Status::Locked), true, true).is_some());
        assert_eq!(a.update(1.2, Some(Status::Locked), true, false), None);
    }
}
