//! The viewfinder's Path mode in the app (`ui/viewfinder/path.rs` is its
//! interface): the shutter places a waypoint on the Camera Path that
//! `app/camera_tools.rs` runs, the way the Camera Path window's Capture
//! does, and tells the photographer which one it is; the dock's play, stop
//! and record and the timeline's edits go to the same actions as that
//! window's; the path is put on the screen for the world view.
//!
//! `AURORA_DEMO_VF_PATH=<what>[:<what>…]`, with the viewfinder open
//! (`AURORA_DEMO_SNAPSHOT=viewfinder`): at frame 600 the Path mode is
//! chosen and a four-waypoint path in front of the camera is placed (the
//! demo's own, never saved), the second waypoint selected. `empty` leaves
//! the path empty, `sel=<n>` selects waypoint n (1 to 4), `sheet` opens the
//! Path sheet, `hidden` hides the path in the world, `rec` turns recording
//! on, `play` plays the path from frame 640. The log gives the state at
//! frame 700. Anything else (`1`) is the plain scenario.

use super::App;
use crate::camera::path::{self as engine, Path, Transition};
use crate::snapshot::viewfinder as vf;
use crate::ui::camera_path::PathAction;
use crate::ui::viewfinder::{Sheet, WorldPath, path};
use glam::Vec3;

/// The first frame of the scenario, the one the path plays from, and the
/// one the state is logged at.
const OPEN_AT: u64 = 600;
const PLAY_AT: u64 = 640;
const LOG_AT: u64 = 700;

#[derive(Debug, Default, PartialEq)]
struct Scenario {
    empty: bool,
    selected: Option<usize>,
    sheet: bool,
    hidden: bool,
    record: bool,
    play: bool,
}

fn parse(value: &str) -> Option<Scenario> {
    let mut scenario = Scenario {
        selected: Some(1),
        ..Scenario::default()
    };
    for part in value.trim().split(':') {
        match part {
            "" | "1" => {}
            "empty" => scenario.empty = true,
            "sheet" => scenario.sheet = true,
            "hidden" => scenario.hidden = true,
            "rec" => scenario.record = true,
            "play" => scenario.play = true,
            other => {
                let n = other.strip_prefix("sel=")?.parse::<usize>().ok()?;
                scenario.selected = n.checked_sub(1).filter(|i| *i < 4);
            }
        }
    }
    Some(scenario)
}

/// The demo's waypoints: (time, metres ahead, metres to the left, metres
/// up, how it goes on).
const STOPS: [(f32, f32, f32, f32, Transition); 4] = [
    (0.0, 6.0, 0.0, 0.0, Transition::Smooth),
    (7.5, 9.0, 3.0, 0.8, Transition::Ease),
    (16.0, 12.0, -2.5, 1.6, Transition::Smooth),
    (26.0, 15.0, 2.0, 0.6, Transition::Linear),
];

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

    /// One of the Path mode's actions: the Camera Path window's own, with
    /// what it tells (a refusal, a save) said over the picture too.
    pub(super) fn vf_path_action(&mut self, action: PathAction) {
        let now = self.vf_now();
        if let PathAction::Delete(i) = &action {
            let selected = path::selection_after_delete(self.camera_tools.selected(), *i);
            self.camera_tools.select(selected);
        }
        let before = self.camera_tools.status().to_owned();
        self.camera_path_action(action);
        let after = self.camera_tools.status();
        if after != before && !after.is_empty() {
            let text = after.to_owned();
            self.snapshots.vf.say(text, false, now);
        }
    }

    /// The path's curve and waypoints on the screen, for the world view
    /// (empty outside the Path mode, or when the path is hidden).
    pub(super) fn vf_path_world(&self, projector: &crate::ui::hud::Projector) -> WorldPath {
        let mut world = WorldPath::default();
        if !self.vf_path_mode() || self.snapshots.vf.path.world_hidden {
            return world;
        }
        let Some(origin) = self.world.main_origin() else {
            return world;
        };
        let path = self.camera_tools.path();
        if path.waypoints.is_empty() {
            return world;
        }
        // (positions are global metres: the main region's corner is the
        // render space's origin)
        let local = |g: glam::DVec3| Vec3::new((g.x - f64::from(origin.0)) as f32, (g.y - f64::from(origin.1)) as f32, g.z as f32);
        let duration = path.duration();
        if duration > engine::TIME_EPSILON {
            let steps = ((duration * 10.0) as usize).clamp(24, 800);
            let mut run = Vec::new();
            for k in 0..=steps {
                let t = duration * k as f32 / steps as f32;
                let at = engine::evaluate(path, t)
                    .and_then(|sample| projector.project(local(sample.global)))
                    .map(|(at, _)| at);
                match at {
                    Some(at) => run.push(at),
                    None if !run.is_empty() => world.curve.push(std::mem::take(&mut run)),
                    None => {}
                }
            }
            if !run.is_empty() {
                world.curve.push(run);
            }
        }
        for (i, waypoint) in path.waypoints.iter().enumerate() {
            if let Some((at, _)) = projector.project(local(waypoint.global())) {
                world.markers.push((i, at));
            }
        }
        world
    }

    /// The demo path: four waypoints in front of the camera.
    fn vf_demo_path(&self) -> Option<Path> {
        let origin = self.world.main_origin()?;
        let region = self.world.main()?;
        let view = self.camera.current_view();
        let base = engine::waypoint_of(&view, origin, region.handle, region.name.clone());
        let rotation = base.rotation();
        let (ahead, left) = (rotation * Vec3::X, rotation * Vec3::Y);
        let waypoints = STOPS
            .iter()
            .map(|&(time, forward, side, up, transition)| {
                let d = ahead * forward + left * side + Vec3::Z * up;
                let mut w = base.clone();
                w.pos = [base.pos[0] + d.x, base.pos[1] + d.y, base.pos[2] + d.z];
                w.time = time;
                w.transition = transition;
                w
            })
            .collect();
        Path::from_waypoints(waypoints).ok().map(|path| Path {
            name: "Demo path".to_owned(),
            ..path
        })
    }

    /// `AURORA_DEMO_VF_PATH`: see the top of the file.
    pub(super) fn viewfinder_path_demo(&mut self) {
        if !self.demo || !(OPEN_AT..=LOG_AT).contains(&self.frame_count) || !self.viewfinder_on() {
            return;
        }
        let Some(scenario) = std::env::var("AURORA_DEMO_VF_PATH").ok().as_deref().and_then(parse) else {
            return;
        };
        if self.frame_count == OPEN_AT {
            self.snapshots.ui.shot = vf::Shot::Path;
            self.snapshots.ui.depth = false;
            self.snapshots.ui.bank = None;
            if !scenario.empty {
                match self.vf_demo_path() {
                    Some(path) => self.camera_tools.set_path(path),
                    None => log::warn!("viewfinder path demo: no path could be placed"),
                }
                self.camera_tools.select(scenario.selected);
            }
            self.snapshots.vf.path.world_hidden = scenario.hidden;
            if scenario.sheet {
                let mut out = Vec::new();
                self.snapshots.vf.open_sheet(Sheet::Path, &mut out);
            }
            if scenario.record {
                self.camera_path_action(PathAction::Record(true));
            }
            log::info!("viewfinder path demo: Path mode, {scenario:?}");
        }
        if scenario.play && self.frame_count == PLAY_AT {
            self.path_play(None);
        }
        if self.frame_count == LOG_AT {
            log::info!(
                "viewfinder path demo: {} waypoints, selected {:?}, playing {}, recording {}, shutter greyed {}",
                self.camera_tools.path().waypoints.len(),
                self.camera_tools.selected(),
                self.camera_tools.playing(),
                self.camera_tools.recording(),
                !path::shutter_enabled(self.camera_tools.recording(), self.camera_tools.playing()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scenario_reads_its_parts() {
        assert_eq!(
            parse("1"),
            Some(Scenario {
                selected: Some(1),
                ..Scenario::default()
            })
        );
        let s = parse("sheet:rec:sel=3").expect("parts");
        assert!(s.sheet && s.record && !s.play);
        assert_eq!(s.selected, Some(2));
        assert_eq!(parse("sel=9").expect("kept").selected, None, "past the fourth: none");
        assert_eq!(parse("what"), None);
    }

    #[test]
    fn the_demo_stops_are_in_order_on_distinct_times() {
        assert!(STOPS.windows(2).all(|w| w[1].0 > w[0].0));
    }
}
