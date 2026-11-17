//! Gaming mode: a toolbar button with which the viewer steps back for a game
//! or another heavy program and stays logged in. The world stops drawing at
//! once (the pause of a window behind on a busy card, busy_pause.rs, without
//! waiting for slow frames: the user said why), the loop runs at
//! `VISIBLE_FPS` while the window can be seen and `HIDDEN_FPS` once it is
//! minimised or covered, and the in-world media stop drawing (Hidden
//! priority, media/update.rs). The network, the chat, the sounds and the
//! notifications go on. Back in front, it ends by itself
//! (`gaming_mode_leave_on_focus`), or with the button again.
//!
//! Memory goes back too, so that the game gets it: the textures' video
//! memory as from a minimised window (`Textures::set_stepped_back`, within
//! a second; the 3D targets already shrink with the pause), and
//! `RELEASE_AFTER` into it the decoded sounds nothing plays, the pages the
//! allocator holds free and the process's working set (`release_memory`).
//! Everything comes back from the disk cache once it ends, the textures
//! sharpening over a few seconds. Not in the Legacy Viewer.

use super::*;

/// Frames per second while the window is in sight: the interface over the
/// blurred world still answers a click, slowly.
const VISIBLE_FPS: f32 = 5.0;
/// Frames per second minimised or covered: enough to read the network's
/// news and raise the notifications, nothing is seen.
const HIDDEN_FPS: f32 = 2.0;
/// Time into gaming mode before its memory goes back: the textures have
/// started to give theirs, and a press of the button by mistake, undone at
/// once, costs nothing.
const RELEASE_AFTER: Duration = Duration::from_secs(3);

#[derive(Debug, Default)]
pub(super) struct GamingMode {
    pub(super) on: bool,
    /// AURORA_DEMO_GAMING_MODE: the scenario turned it on once.
    demo_done: bool,
    /// When its memory goes back (`RELEASE_AFTER` after it starts), until
    /// it has.
    release_at: Option<Instant>,
}

/// The frame limiter's cap (0 = none) under gaming mode: `hidden` the
/// window is minimised or covered.
pub(super) fn cap(cap: f32, on: bool, hidden: bool) -> f32 {
    if !on {
        return cap;
    }
    let low = if hidden { HIDDEN_FPS } else { VISIBLE_FPS };
    if cap >= 1.0 { cap.min(low) } else { low }
}

/// Whether the memory goes back now, due at `at`.
fn due(at: Option<Instant>, now: Instant) -> bool {
    at.is_some_and(|at| now >= at)
}

/// The process's pages leave its working set (Windows `EmptyWorkingSet`):
/// the system can give them to the game, and those the viewer touches
/// again come back as it reads them. The memory stays committed. Nothing
/// elsewhere.
fn trim_working_set() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet;
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        // SAFETY: the pseudo-handle of the current process is always
        // valid and holds the rights the call needs; it changes no memory
        // of the process, only where its pages are.
        if unsafe { K32EmptyWorkingSet(GetCurrentProcess()) } == 0 {
            log::warn!(
                "gaming mode: the working set could not be trimmed: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}

/// `AURORA_DEMO_GAMING_MODE=<frame>`: gaming mode turns on at that frame,
/// as from its button, to see the pause, its notice and its title.
fn demo_from() -> Option<u64> {
    static FROM: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *FROM.get_or_init(|| std::env::var("AURORA_DEMO_GAMING_MODE").ok()?.trim().parse().ok())
}

impl App {
    /// The toolbar's Gaming mode button.
    pub(super) fn toggle_gaming_mode(&mut self) {
        self.set_gaming_mode(!self.gaming_mode.on);
    }

    fn set_gaming_mode(&mut self, on: bool) {
        if self.gaming_mode.on == on {
            return;
        }
        self.gaming_mode.on = on;
        self.gaming_mode.release_at = on.then(|| Instant::now() + RELEASE_AFTER);
        log::info!("gaming mode {}", if on { "on" } else { "off" });
    }

    /// The memory the viewer can do without while gaming mode lasts goes
    /// back to the system, once (see the module).
    fn release_memory(&mut self) {
        let before = crate::sys_probe::process_memory();
        self.scene.sounds.release_idle();
        crate::allocator::collect();
        trim_working_set();
        let after = crate::sys_probe::process_memory();
        if let (Some(b), Some(a)) = (before, after) {
            log::info!(
                "gaming mode: memory given back, working set {} -> {} MB, private {} -> {} MB",
                b.working_set >> 20,
                a.working_set >> 20,
                b.private >> 20,
                a.private >> 20
            );
        }
    }

    /// The window has the focus again.
    pub(super) fn gaming_mode_focused(&mut self) {
        if self.settings.gaming_mode_leave_on_focus {
            self.set_gaming_mode(false);
        }
    }

    /// The 3D drawing pauses for gaming mode; a photo being taken is
    /// drawn all the same.
    pub(super) fn gaming_mode_pauses(&self) -> bool {
        self.gaming_mode.on && !self.snapshots.drawing()
    }

    /// Every frame: the memory going back once it is due, then the demo
    /// scenario.
    pub(super) fn gaming_mode_frame(&mut self) {
        if due(self.gaming_mode.release_at, Instant::now()) && self.gaming_mode_pauses() {
            self.gaming_mode.release_at = None;
            self.release_memory();
        }
        if self.demo && !self.gaming_mode.demo_done && demo_from().is_some_and(|from| self.frame_count >= from) {
            self.gaming_mode.demo_done = true;
            self.set_gaming_mode(true);
        }
    }

    /// The notice over the paused world (busy_pause.rs draws it).
    pub(super) fn gaming_mode_notice(&self) -> &'static str {
        if self.settings.gaming_mode_leave_on_focus {
            aurora_i18n::tr!("gaming-mode-notice")
        } else {
            aurora_i18n::tr!("gaming-mode-notice-button")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_leaves_the_cap_as_it_was() {
        assert_eq!(cap(0.0, false, true), 0.0);
        assert_eq!(cap(15.0, false, false), 15.0);
    }

    #[test]
    fn on_crawls_and_slower_out_of_sight() {
        assert_eq!(cap(0.0, true, false), VISIBLE_FPS, "no cap at all becomes the visible one");
        assert_eq!(cap(120.0, true, false), VISIBLE_FPS);
        assert_eq!(cap(120.0, true, true), HIDDEN_FPS);
        assert_eq!(cap(1.0, true, false), 1.0, "a lower cap of the user stays");
    }
}
