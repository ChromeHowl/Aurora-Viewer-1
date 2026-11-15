//! The viewfinder's Path mode in the app (`ui/viewfinder/path.rs` is its
//! interface): the shutter places a waypoint on the Camera Path that
//! `app/camera_tools.rs` runs, the way the Camera Path window's Capture
//! does, and tells the photographer which one it is.

use super::App;
use crate::snapshot::viewfinder as vf;
use crate::ui::viewfinder::path;

impl App {
    /// The shot mode is the camera movie.
    pub(super) fn vf_path_mode(&self) -> bool {
        let ui = &self.snapshots.ui;
        vf::shown(ui.shot, ui.depth, ui.bank) == vf::ShotChoice::Path
    }

    /// The shutter in the Path mode: the camera's view becomes the next
    /// waypoint, unless the path records or plays.
    pub(super) fn vf_waypoint(&mut self) {
        let now = self.vf_now();
        self.snapshots.vf.hinted = true;
        if !path::shutter_enabled(self.camera_tools.recording(), self.camera_tools.playing()) {
            self.snapshots.vf.say(aurora_i18n::tr!("vfp-shutter-busy-tip"), true, now);
            return;
        }
        self.path_capture();
        let text = self.camera_tools.status().to_owned();
        if !text.is_empty() {
            self.snapshots.vf.say(text, false, now);
        }
        log::info!("viewfinder: waypoint placed");
    }
}
