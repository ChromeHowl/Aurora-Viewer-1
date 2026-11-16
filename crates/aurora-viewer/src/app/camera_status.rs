//! The camera mode indicator in the app: gathers the facts
//! (`camera/mode_status.rs` resolves them), shows the pill when the mode
//! changes and the chip for a mode that stays (`ui/camera_status.rs`), notes
//! the mode in the Controls window's header, and leaves a mode when its
//! chip is clicked. Nothing here changes what the camera does, and RLVa and
//! scripts are only read.

use super::App;
use crate::camera::mode_status::{self, Announcer, Facts, Status};
use crate::camera::{CameraMode, Owner};
use crate::keybinds::Action;
use crate::theme::Palette;
use crate::ui::camera_path::PathAction;
use crate::ui::camera_status::{self as text, Badge, Keys};
use crate::ui::controls::ControlAction;

#[derive(Default)]
pub struct State {
    announcer: Announcer,
    /// The pill the viewfinder already said (its `until`), not to say twice.
    said_until: f64,
}

impl App {
    fn camera_facts(&self) -> Facts {
        Facts {
            director: self.camera.track_active(),
            path_playing: self.camera.view_owner() == Some(Owner::Path),
            path_recording: self.camera_tools.recording(),
            locked: self.camera.locked(),
            flycam: self.camera.flycam.is_some(),
            free: self.camera.free().is_some(),
            mouselook: self.camera.mouselook(),
            scripted_follow: self.camera.mode == CameraMode::Follow,
            seated: self.camera.sit_camera_active(),
            object_view: self.object_view,
        }
    }

    /// The camera mode in force; none in VR, whose views have their own
    /// rules (the headset's, `vr_mode`).
    pub(super) fn camera_status_now(&self) -> Option<Status> {
        if self.vr_mode.is_some() {
            return None;
        }
        mode_status::status(&self.camera_facts())
    }

    fn camera_keys(&self) -> Keys {
        let label = |action: Action| self.settings.keybinds.get(action)[0].as_ref().map(|b| b.label());
        Keys {
            reset: label(Action::ResetCamera).unwrap_or_else(|| aurora_i18n::tr!("keybinds-esc").to_string()),
            lock: label(Action::CameraLock),
            path: label(Action::PathPlayStop),
        }
    }

    /// Once a frame, before the Controls window (whose header it notes).
    pub(super) fn camera_status_ui(&mut self, ctx: &egui::Context, p: &Palette) {
        self.camera_mode_demo();
        let status = self.camera_status_now();
        let keys = self.camera_keys();
        let ready = self.in_world() && self.vr_mode.is_none();
        let now = ctx.input(|i| i.time);
        let pill = self
            .camera_status
            .announcer
            .update(now, status, ready, self.settings.camera.show_mode_changes);
        self.controls_ui.mode = status.filter(|_| ready).map(|s| {
            let name = text::name(s);
            (name.to_owned(), text::line(name, text::how_to_leave(s, &keys).as_deref()))
        });
        if !ready {
            return;
        }
        if self.viewfinder_on() {
            // the viewfinder is the indicator; its own line says the change
            if let Some(pill) = pill
                && pill.until != self.camera_status.said_until
            {
                self.camera_status.said_until = pill.until;
                let line = text::pill_text(pill.status, &keys);
                let now = self.vf_now();
                self.snapshots.vf.say(line, false, now);
            }
            return;
        }
        if self.interface.clears_view() {
            return;
        }
        let room = crate::ui::layout::work_area(ctx);
        if let Some(pill) = pill {
            ctx.request_repaint();
            let line = text::pill_text(pill.status, &keys);
            let badge = Badge {
                id: "camera-mode-pill",
                icon: text::icon(pill.status),
                text: &line,
                clickable: false,
                tip: "",
            };
            text::show(ctx, p, room, &badge);
        } else if self.settings.camera.show_mode_changes
            && let Some(s) = status.filter(|s| s.held())
        {
            let line = text::line(text::name(s), text::how_to_leave(s, &keys).as_deref());
            let leaves = text::click_leaves(s);
            let tip = if leaves {
                aurora_i18n::tr!("camera-mode-click-to-leave", mode = line.clone())
            } else {
                line.clone()
            };
            let badge = Badge {
                id: "camera-mode-chip",
                icon: text::icon(Some(s)),
                text: &line,
                clickable: leaves,
                tip: &tip,
            };
            if text::show(ctx, p, room, &badge) {
                self.leave_camera_mode(s);
            }
        }
    }

    /// What a click on a chip does: the action the mode's own button does.
    fn leave_camera_mode(&mut self, status: Status) {
        match status {
            Status::Locked => self.toggle_camera_lock(),
            Status::PathPlaying => {
                if self.camera_tools.playing() {
                    self.path_stop();
                } else {
                    // the last shot, held
                    self.camera.release_view(true);
                }
            }
            Status::PathRecording => self.camera_path_action(PathAction::Record(false)),
            Status::Free => self.control_action(ControlAction::FreeExit),
            _ => {}
        }
    }

    /// AURORA_DEMO_CAMERA_MODE=cycle (demo mode): mouselook in and out, the
    /// free camera, the lock, then a path played and stopped, one step every
    /// 80 frames from 620 on, for a capture of each pill and chip.
    fn camera_mode_demo(&mut self) {
        static CYCLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !self.demo || !*CYCLE.get_or_init(|| std::env::var("AURORA_DEMO_CAMERA_MODE").is_ok_and(|v| v == "cycle")) {
            return;
        }
        match self.frame_count {
            620 | 700 => self.toggle_mouselook(),
            780 => {
                self.director.free = true;
                self.photo_free_camera();
            }
            860 => {
                self.director.free = false;
                self.photo_free_camera();
            }
            880 | 960 => self.toggle_camera_lock(),
            1000 => self.path_capture(),
            1040 => self.path_play(None),
            1160 => self.path_stop(),
            _ => {}
        }
    }
}
