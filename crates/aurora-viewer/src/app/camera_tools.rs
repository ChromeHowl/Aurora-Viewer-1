//! Aurora's camera tools in the app: the Camera Lock, the view history and
//! the Camera Path (`camera/takeover.rs`, `history.rs`, `path.rs`). The lock
//! and the path show their view through the camera's one takeover hook
//! (`Camera::take_view`); this module decides when, checks what forbids it
//! (mouselook, RLVa camera restrictions, a teleport, a region not connected)
//! and runs the Camera Path window's actions. All of it is session state,
//! except the account's saved paths (`camera_paths.json`).

use super::App;
use crate::camera::history::{Debounce, History, Move};
use crate::camera::path::{self, Conditions, Frame, Invalid, Library, Path, Player, Stop};
use crate::camera::path_file::{self, ReadError};
use crate::camera::{Owner, View};
use crate::theme::Palette;
use crate::ui::camera_path::{CameraPathUi, PathAction, PathView};
use crate::world::notifications::{Data, Kind};
use uuid::Uuid;

#[derive(Default)]
pub struct CameraTools {
    history: History,
    /// The path being edited and played.
    path: Path,
    library: Library,
    /// The account the library was read for.
    account: Uuid,
    player: Option<Player>,
    recording: bool,
    record_move: Debounce,
    /// A teleport arrived since the last frame.
    teleported: bool,
    /// The last thing to tell the user in the window.
    status: String,
    ui: CameraPathUi,
}

impl CameraTools {
    /// The region positions are relative to changed (a border crossing).
    pub(super) fn shift(&mut self, delta: glam::Vec3) {
        self.history.shift(delta);
    }

    /// A teleport arrived: the kept views belong to the place left.
    pub(super) fn on_teleport(&mut self, local: bool) {
        self.teleported = true;
        if !local {
            self.history.clear();
        }
    }

    pub fn playing(&self) -> bool {
        self.player.is_some()
    }
}

fn invalid_text(error: Invalid) -> &'static str {
    match error {
        Invalid::Time => aurora_i18n::tr!("camtools-path-error-time"),
        Invalid::TimeTaken => aurora_i18n::tr!("camtools-path-error-time-taken"),
        Invalid::Full => aurora_i18n::tr!("camtools-path-error-full"),
        Invalid::Pose | Invalid::Range | Invalid::View => aurora_i18n::tr!("camtools-path-error-view"),
    }
}

impl App {
    /// The view as the history keeps it: the lens is the user's setting,
    /// not the angle a focus correction narrows it to.
    fn history_view(&self) -> View {
        View {
            fov: self.settings.camera.fov,
            ..self.camera.current_view()
        }
    }

    /// What forbids a lock or a path now, said to the user.
    fn camera_takeover_refused(&self) -> Option<&'static str> {
        if !self.in_world() {
            Some(aurora_i18n::tr!("camtools-refused-offline"))
        } else if self.world.rlva.camera_restricted() {
            Some(aurora_i18n::tr!("camtools-refused-rlva"))
        } else if self.camera.mouselook() {
            Some(aurora_i18n::tr!("camtools-refused-mouselook"))
        } else {
            None
        }
    }

    fn camera_tools_say(&mut self, text: &str) {
        self.camera_tools.status = text.to_owned();
        if !self.panels.camera_path {
            self.world
                .notifications
                .push(Kind::Info, aurora_i18n::tr!("camtools-title"), text.to_owned(), Data::None);
        }
    }

    // ------------------------------------------------------------------ lock

    /// The Camera Lock on or off: on, the view stays where it is whatever
    /// moves the camera; off, it glides back to where the camera is.
    pub(super) fn toggle_camera_lock(&mut self) {
        if self.camera.locked() {
            self.camera.release_view(true);
            log::info!("camera lock: off");
            return;
        }
        if let Some(why) = self.camera_takeover_refused() {
            self.camera_tools_say(why);
            return;
        }
        // (a path playing loses the view and stops at the next frame)
        if self.camera.take_view(Owner::Lock, self.camera.current_view()) {
            log::info!("camera lock: on at {:.2?}", self.camera.position);
        }
    }

    // --------------------------------------------------------------- history

    /// "Previous view" / "Next view".
    pub(super) fn camera_view_step(&mut self, back: bool) {
        if self.camera.view_owner().is_some() || self.camera_takeover_refused().is_some() {
            return;
        }
        let current = self.history_view();
        let tools = &mut self.camera_tools;
        let Some(view) = (if back {
            tools.history.previous(current)
        } else {
            tools.history.next()
        }) else {
            return;
        };
        if self.camera.go_to_view(view, &mut self.world.agent, &self.settings.camera) {
            // the lens with the view, unless RLVa holds it (as the lens keys)
            if self.world.rlva.camera_owner().is_none() && (self.settings.camera.fov - view.fov).abs() > 1e-4 {
                self.settings.camera.fov = view.fov;
                if !self.demo {
                    self.settings.save();
                }
            }
        }
    }

    pub(super) fn camera_history_clear(&mut self) {
        self.camera_tools.history.clear();
    }

    /// What the menus and the Controls window show: locked, a view to go
    /// back to, a view to go forward to.
    pub(super) fn camera_tools_state(&self) -> (bool, bool, bool) {
        let free = self.camera.view_owner().is_none();
        (
            self.camera.locked(),
            free && self.camera_tools.history.can_go_back(&self.history_view()),
            free && self.camera_tools.history.can_go_forward(),
        )
    }

    // ------------------------------------------------------------------ path

    /// The handle and the name of the connected region that holds the
    /// global point (`x`, `y`); the avatar's region when none does.
    fn region_at(&self, x: f64, y: f64) -> Option<(u64, String)> {
        let holds = |r: &&crate::world::Region| {
            let (ox, oy) = aurora_net::handle_to_origin(r.handle);
            let (w, h) = (f64::from(r.heightmap.size_x.max(1)), f64::from(r.heightmap.size_y.max(1)));
            x >= f64::from(ox) && x < f64::from(ox) + w && y >= f64::from(oy) && y < f64::from(oy) + h
        };
        let region = self.world.regions.values().find(holds).or_else(|| self.world.main())?;
        Some((region.handle, region.name.clone()))
    }

    fn path_regions_connected(&self) -> bool {
        self.camera_tools
            .path
            .waypoints
            .iter()
            .all(|w| self.world.regions.contains_key(&w.handle))
    }

    /// The view shown becomes a waypoint, one step after the last.
    pub(super) fn path_capture(&mut self) {
        if self.camera_tools.playing() {
            return;
        }
        let result = (|| {
            let origin = self.world.main_origin().filter(|_| self.in_world())?;
            let view = self.camera.current_view();
            let (gx, gy) = (
                f64::from(origin.0) + f64::from(view.position.x),
                f64::from(origin.1) + f64::from(view.position.y),
            );
            let (handle, name) = self.region_at(gx, gy)?;
            // RLVa @showloc: the place's name is not kept either
            let name = if self.world.rlva.has("showloc") { String::new() } else { name };
            let mut waypoint = path::waypoint_of(&view, origin, handle, name);
            waypoint.fov = waypoint.fov.clamp(*path::VIEW_RANGE.start(), *path::VIEW_RANGE.end());
            waypoint.time = self.camera_tools.path.next_time(path::DEFAULT_STEP);
            Some(self.camera_tools.path.insert(waypoint))
        })();
        match result {
            Some(Ok(index)) => {
                self.camera_tools.ui.selected = Some(index);
                let text = aurora_i18n::tr!("camtools-path-captured", count = self.camera_tools.path.waypoints.len() as i64);
                self.camera_tools.status = text;
            }
            Some(Err(error)) => self.camera_tools_say(invalid_text(error)),
            None => self.camera_tools_say(aurora_i18n::tr!("camtools-refused-offline")),
        }
    }

    /// Play from the start, or from waypoint `from`.
    pub(super) fn path_play(&mut self, from: Option<usize>) {
        let refused = if self.camera_tools.path.waypoints.is_empty() {
            Some(aurora_i18n::tr!("camtools-path-empty"))
        } else if self.camera.locked() {
            Some(aurora_i18n::tr!("camtools-refused-locked"))
        } else if self.camera.free().is_some() || self.camera.flycam.is_some() {
            Some(aurora_i18n::tr!("camtools-refused-free"))
        } else if !self.path_regions_connected() {
            Some(aurora_i18n::tr!("camtools-path-stop-region"))
        } else {
            self.camera_takeover_refused()
        };
        if let Some(why) = refused {
            self.camera_tools_say(why);
            return;
        }
        let tools = &mut self.camera_tools;
        let start = from.and_then(|i| tools.path.waypoints.get(i)).map_or(0.0, |w| w.time);
        let first = path::evaluate(&tools.path, start).zip(self.world.main_origin());
        let Some((sample, origin)) = first else {
            return;
        };
        if self.camera.take_view(Owner::Path, sample.view(origin)) {
            tools.player = Some(Player::start(start));
            tools.recording = false;
            tools.status.clear();
            // input from before the play must not stop it
            self.camera.take_touched();
            log::info!("camera path: playing {} waypoints from {start:.2} s", tools.path.waypoints.len());
        }
    }

    /// Stop: the view glides back to the camera.
    pub(super) fn path_stop(&mut self) {
        if self.camera_tools.player.take().is_some() && self.camera.view_owner() == Some(Owner::Path) {
            self.camera.release_view(true);
        }
    }

    pub(super) fn path_play_stop(&mut self) {
        if self.camera_tools.playing() {
            self.path_stop();
        } else {
            self.path_play(None);
        }
    }

    fn path_go_to(&mut self, index: usize) {
        let Some(time) = self.camera_tools.path.waypoints.get(index).map(|w| w.time) else {
            return;
        };
        if self.camera_tools.playing() || self.camera.view_owner().is_some() {
            return;
        }
        if let Some(why) = self.camera_takeover_refused() {
            self.camera_tools_say(why);
            return;
        }
        if !self.world.regions.contains_key(&self.camera_tools.path.waypoints[index].handle) {
            self.camera_tools_say(aurora_i18n::tr!("camtools-path-stop-region"));
            return;
        }
        if let Some((sample, origin)) = path::evaluate(&self.camera_tools.path, time).zip(self.world.main_origin()) {
            self.camera
                .go_to_view(sample.view(origin), &mut self.world.agent, &self.settings.camera);
        }
    }

    // ------------------------------------------------------------------ frame

    /// Once a frame, before the camera's update: the path's next view, what
    /// stops it, the recording and the view history.
    pub(super) fn camera_tools_tick(&mut self, dt: f32) {
        if self.camera_tools.account != self.world.agent_id {
            self.camera_tools.account = self.world.agent_id;
            self.camera_tools.library = if self.world.agent_id.is_nil() {
                Library::default()
            } else {
                Library::load(&crate::settings::account_dir(&self.world.agent_id).join(path::FILE))
            };
        }
        let touched = self.camera.take_touched();
        let teleported = std::mem::take(&mut self.camera_tools.teleported) || self.world.teleporting;
        if self.camera_tools.player.is_some() {
            let conditions = Conditions {
                teleported,
                regions_connected: self.world.main_origin().is_some() && self.path_regions_connected(),
                mouselook: self.camera.mouselook(),
                locked: self.camera.locked(),
                user_input: touched,
                restricted: self.world.rlva.camera_restricted(),
            };
            let owns = self.camera.view_owner() == Some(Owner::Path);
            match path::stop_reason(&conditions) {
                Some(Stop::UserInput) if owns => {
                    // the user takes the camera from where the path left it
                    self.camera.adopt_view(&mut self.world.agent, &self.settings.camera);
                    self.camera_tools.player = None;
                }
                Some(reason) => {
                    self.camera_tools.player = None;
                    if owns {
                        // (after a teleport the held view is of the place left)
                        self.camera.release_view(reason != Stop::Teleport);
                    }
                    let why = match reason {
                        Stop::Teleport => aurora_i18n::tr!("camtools-path-stop-teleport"),
                        Stop::RegionNotConnected => aurora_i18n::tr!("camtools-path-stop-region"),
                        Stop::Mouselook => aurora_i18n::tr!("camtools-refused-mouselook"),
                        Stop::CameraLock => aurora_i18n::tr!("camtools-refused-locked"),
                        Stop::Restricted => aurora_i18n::tr!("camtools-refused-rlva"),
                        Stop::UserInput => "",
                    };
                    log::info!("camera path: stopped ({reason:?})");
                    self.camera_tools.status = why.to_owned();
                }
                None if !owns => self.camera_tools.player = None,
                None => {
                    let tools = &mut self.camera_tools;
                    let frame = tools.player.as_mut().map(|player| player.advance(&tools.path, dt));
                    match (frame, self.world.main_origin()) {
                        (Some(Frame::Playing(sample) | Frame::Holding(sample)), Some(origin)) => {
                            self.camera.hold_view(Owner::Path, sample.view(origin));
                        }
                        _ => {
                            tools.player = None;
                            self.camera.release_view(true);
                        }
                    }
                }
            }
        }
        let free = self.camera.view_owner().is_none() && !self.camera.mouselook();
        // recording: a waypoint each time the user's camera move ends
        if self.camera_tools.recording {
            if !free || teleported {
                self.camera_tools.record_move.cancel();
            } else if self.camera_tools.record_move.observe(touched, dt) == Some(Move::Finished) {
                self.path_capture();
            }
        }
        if free && self.in_world() {
            let view = self.history_view();
            self.camera_tools.history.observe(touched, view, dt);
        } else {
            self.camera_tools.history.cancel_pending();
        }
    }

    // ----------------------------------------------------------------- window

    pub(super) fn camera_path_window(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.panels.camera_path {
            return;
        }
        let tools = &mut self.camera_tools;
        let view = PathView {
            path: &tools.path,
            saved: tools.library.paths.iter().map(|p| p.name.as_str()).collect(),
            playing: tools.player.is_some(),
            recording: tools.recording,
            elapsed: tools.player.map_or(0.0, |player| player.elapsed()),
            hide_regions: self.world.rlva.has("showloc"),
            keys: &self.settings.keybinds,
            status: &tools.status,
        };
        let actions = tools.ui.show(ctx, p, &mut self.panels.camera_path, &view);
        for action in actions {
            self.camera_path_action(action);
        }
    }

    fn save_path_library(&mut self) {
        if self.world.agent_id.is_nil() {
            return;
        }
        let file = crate::settings::account_dir(&self.world.agent_id).join(path::FILE);
        if let Err(error) = self.camera_tools.library.save(&file) {
            log::warn!("camera paths: {} not written ({error})", file.display());
        }
    }

    fn camera_path_action(&mut self, action: PathAction) {
        // the waypoints do not change under a playback
        let edits = matches!(
            action,
            PathAction::Delete(_)
                | PathAction::Move(..)
                | PathAction::SetTime(..)
                | PathAction::SetTransition(..)
                | PathAction::Load(_)
                | PathAction::New
                | PathAction::Import
        );
        if edits && self.camera_tools.playing() {
            return;
        }
        let tools = &mut self.camera_tools;
        match action {
            PathAction::Capture => self.path_capture(),
            PathAction::Record(on) => {
                tools.recording = on;
                tools.record_move.cancel();
                tools.status = if on {
                    aurora_i18n::tr!("camtools-path-recording").to_owned()
                } else {
                    String::new()
                };
            }
            PathAction::Delete(i) => {
                if i < tools.path.waypoints.len() {
                    tools.path.waypoints.remove(i);
                }
            }
            PathAction::Move(i, down) => {
                let other = if down { i + 1 } else { i.wrapping_sub(1) };
                tools.path.swap_shots(i, other);
            }
            PathAction::SetTime(i, time) => match tools.path.retime(i, time) {
                Ok(index) => {
                    tools.ui.selected = Some(index);
                    tools.status.clear();
                }
                Err(error) => tools.status = invalid_text(error).to_owned(),
            },
            PathAction::SetTransition(i, transition) => {
                if let Some(w) = tools.path.waypoints.get_mut(i) {
                    w.transition = transition;
                }
            }
            PathAction::GoTo(i) => self.path_go_to(i),
            PathAction::Play(from) => self.path_play(from),
            PathAction::Stop => self.path_stop(),
            PathAction::Loop(on) => tools.path.looped = on,
            PathAction::Hold(on) => tools.path.hold = on,
            PathAction::Save(name) => {
                tools.path.name = name;
                let stored = tools.library.store(tools.path.clone());
                let text = if stored {
                    aurora_i18n::tr!("camtools-path-saved-as", name = tools.path.name.as_str())
                } else {
                    aurora_i18n::tr!("camtools-path-error-full").to_owned()
                };
                tools.status = text;
                self.save_path_library();
            }
            PathAction::Load(name) => {
                if let Some(saved) = tools.library.get(&name) {
                    tools.path = saved.clone();
                    tools.ui.selected = None;
                    tools.status.clear();
                }
            }
            PathAction::DeleteSaved(name) => {
                if tools.library.remove(&name) {
                    self.save_path_library();
                }
            }
            PathAction::New => {
                tools.path = Path::default();
                tools.ui.selected = None;
                tools.status.clear();
            }
            PathAction::Import => self.path_import(),
            PathAction::Export => self.path_export(),
        }
    }

    fn path_dialog() -> rfd::FileDialog {
        rfd::FileDialog::new().add_filter(aurora_i18n::tr!("camtools-path-file-kind"), &["xml"])
    }

    /// Read a Camera Path file over the waypoints being edited: whole, or
    /// not at all.
    fn path_import(&mut self) {
        let Some(file) = Self::path_dialog().set_title(aurora_i18n::tr!("camtools-path-import")).pick_file() else {
            return;
        };
        let here = self.world.main().map(|r| (r.handle, r.name.clone()));
        let read = std::fs::metadata(&file)
            .ok()
            .filter(|m| m.len() > 0 && m.len() <= path_file::MAX_FILE_SIZE)
            .and_then(|_| std::fs::read(&file).ok())
            .ok_or(ReadError::Format)
            .and_then(|bytes| path_file::read(&bytes, here.as_ref().map(|(h, n)| (*h, n.as_str()))));
        let tools = &mut self.camera_tools;
        match read {
            Ok(read) => {
                log::info!(
                    "camera path: {} waypoints read from {} (version 1: {})",
                    read.path.waypoints.len(),
                    file.display(),
                    read.legacy
                );
                // the file holds waypoints only: the name is the file's
                let name = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                tools.path = Path {
                    name,
                    looped: tools.path.looped,
                    hold: tools.path.hold,
                    ..read.path
                };
                tools.ui.selected = None;
                let text = if read.legacy {
                    aurora_i18n::tr!("camtools-path-imported-legacy", count = tools.path.waypoints.len() as i64)
                } else {
                    aurora_i18n::tr!("camtools-path-imported", count = tools.path.waypoints.len() as i64)
                };
                tools.status = text;
            }
            Err(error) => {
                log::warn!("camera path: {} not read ({error:?})", file.display());
                tools.status = match error {
                    ReadError::Waypoint(n) => aurora_i18n::tr!("camtools-path-import-waypoint", number = n as i64),
                    ReadError::TooMany => aurora_i18n::tr!("camtools-path-error-full").to_owned(),
                    ReadError::SameTime => aurora_i18n::tr!("camtools-path-error-time-taken").to_owned(),
                    ReadError::Empty => aurora_i18n::tr!("camtools-path-empty").to_owned(),
                    ReadError::NoRegion => aurora_i18n::tr!("camtools-refused-offline").to_owned(),
                    ReadError::Format => aurora_i18n::tr!("camtools-path-import-format").to_owned(),
                };
            }
        }
    }

    fn path_export(&mut self) {
        let tools = &mut self.camera_tools;
        if tools.path.waypoints.is_empty() {
            return;
        }
        let name = if tools.path.name.trim().is_empty() {
            "camera_path".to_owned()
        } else {
            tools.path.name.trim().to_owned()
        };
        let Some(file) = Self::path_dialog()
            .set_title(aurora_i18n::tr!("camtools-path-export"))
            .set_file_name(format!("{name}.xml"))
            .save_file()
        else {
            return;
        };
        tools.status = match crate::atomic_file::write(&file, path_file::write(&tools.path).as_bytes()) {
            Ok(()) => aurora_i18n::tr!("camtools-path-exported"),
            Err(error) => {
                log::warn!("camera path: {} not written ({error})", file.display());
                aurora_i18n::tr!("camtools-path-export-failed")
            }
        }
        .to_owned();
    }
}
