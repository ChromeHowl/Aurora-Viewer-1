//! Camera Paths (Aurora's own): a list of waypoints the camera travels
//! through, each at its time on the path's clock. The Legacy Viewer has no
//! such feature; the data and the motion follow the Camera Path file other
//! viewers exchange (`path_file.rs`), so a path plays the same there.
//!
//! A waypoint keeps its region (name and grid handle) and its position in
//! that region; played, positions are taken in global metres (the region's
//! corner plus the position), so a path runs across connected regions and
//! means the same wherever the avatar stands. This module holds the pure
//! parts: the data and its rules (times ascending, one waypoint per time),
//! the motion of each transition, the player's state machine (loop, hold
//! on the last shot), the reasons a playback stops, and the per-account
//! library (`camera_paths.json`, next to `tracked_regions.json`). The app's
//! side (capturing, driving `Camera`) is `app/camera_tools.rs`.

use super::takeover::View;
use super::view_reference_up;
use glam::{DVec3, Mat3, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::path::Path as FilePath;

pub const FILE: &str = "camera_paths.json";
/// Version of `camera_paths.json`.
pub const FILE_VERSION: u32 = 1;
/// Seconds between a waypoint and the next one captured.
pub const DEFAULT_STEP: f32 = 3.0;
/// The latest a waypoint may be on the path's clock, seconds.
pub const MAX_TIME: f32 = 600.0;
pub const MAX_WAYPOINTS: usize = 6000;
pub const MAX_PATHS: usize = 256;
/// Two waypoints closer than this in time are the same instant.
pub const TIME_EPSILON: f32 = 0.0001;
/// The lens a waypoint may hold, radians.
pub const VIEW_RANGE: std::ops::RangeInclusive<f32> = 0.08..=3.06;
/// A position in a region, generous for large regions, metres.
const MAX_LOCAL: f32 = 100_000.0;
/// Consecutive waypoints this close hold the position exactly, metres.
const HOLD_DISTANCE: f64 = 0.001;
/// How far ahead a played view looks: the point the camera orbits when the
/// user takes it back. A waypoint keeps a direction, not a focus point.
const FOCUS_DISTANCE: f32 = 5.0;

/// How the camera travels from a waypoint to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Transition {
    /// The default: a curve through the waypoints around (a Catmull-Rom
    /// spline on the path's clock), leaving the first waypoint and reaching
    /// the last gently.
    #[default]
    Smooth,
    /// A straight line at an even pace.
    Linear,
    /// A straight line from rest to rest.
    Ease,
    /// This shot is held until the next waypoint's time, then the view
    /// jumps there.
    Cut,
}

impl Transition {
    pub const ALL: [Transition; 4] = [Transition::Smooth, Transition::Linear, Transition::Ease, Transition::Cut];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Waypoint {
    /// Seconds from the start of the path.
    pub time: f32,
    /// The region's name when captured (shown; the handle is what counts).
    pub region: String,
    /// The region's grid handle: its south-west corner in global metres.
    pub handle: u64,
    /// Position in the region, metres.
    pub pos: [f32; 3],
    /// The view's rotation (x, y, z, w): +X forward, +Y left, +Z up.
    pub rot: [f32; 4],
    /// Vertical field of view, radians.
    pub fov: f32,
    /// How the camera goes from this waypoint to the next.
    pub transition: Transition,
}

impl Default for Waypoint {
    fn default() -> Self {
        Waypoint {
            time: 0.0,
            region: String::new(),
            handle: 0,
            pos: [128.0, 128.0, 30.0],
            rot: [0.0, 0.0, 0.0, 1.0],
            fov: super::DEFAULT_FOV,
            transition: Transition::Smooth,
        }
    }
}

/// Why a waypoint is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// Not between 0 and `MAX_TIME`.
    Time,
    /// A position or rotation that is not a pose.
    Pose,
    /// A position outside any region, or a region outside the grid.
    Range,
    /// A lens outside `VIEW_RANGE`.
    View,
    /// Another waypoint already sits at that time.
    TimeTaken,
    /// `MAX_WAYPOINTS` reached.
    Full,
}

impl Waypoint {
    /// Position in global metres.
    pub fn global(&self) -> DVec3 {
        let (x, y) = aurora_net::handle_to_origin(self.handle);
        DVec3::new(
            f64::from(x) + f64::from(self.pos[0]),
            f64::from(y) + f64::from(self.pos[1]),
            f64::from(self.pos[2]),
        )
    }

    pub fn rotation(&self) -> Quat {
        let q = Quat::from_array(self.rot);
        if q.is_finite() && (q.length_squared() - 1.0).abs() <= 1e-5 {
            // (kept as it is, so that reading a file back changes nothing)
            q
        } else if q.is_finite() && q.length_squared() > 1e-6 {
            q.normalize()
        } else {
            Quat::IDENTITY
        }
    }

    /// The checks every waypoint passes before it enters a path, whether
    /// captured, typed or read from a file.
    pub fn validate(&self) -> Result<(), Invalid> {
        if !self.time.is_finite() || !(0.0..=MAX_TIME).contains(&self.time) {
            return Err(Invalid::Time);
        }
        let norm: f32 = self.rot.iter().map(|v| v * v).sum();
        if !self.pos.iter().all(|v| v.is_finite()) || !norm.is_finite() || (norm - 1.0).abs() > 0.01 {
            return Err(Invalid::Pose);
        }
        if self.pos.iter().any(|v| v.abs() > MAX_LOCAL) {
            return Err(Invalid::Range);
        }
        if !self.fov.is_finite() || !VIEW_RANGE.contains(&self.fov) {
            return Err(Invalid::View);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Path {
    pub name: String,
    /// In the order of their times, no two at the same time.
    pub waypoints: Vec<Waypoint>,
    /// At the end, from the start again.
    pub looped: bool,
    /// "Hold last shot": at the end the camera stays on the last waypoint
    /// until the user moves it or stops.
    pub hold: bool,
}

impl Default for Path {
    fn default() -> Self {
        Path {
            name: String::new(),
            waypoints: Vec::new(),
            looped: false,
            hold: true,
        }
    }
}

impl Path {
    /// A path from waypoints in any order: sorted by time, each one checked,
    /// two at the same time refused. Nothing is kept of a list with a bad
    /// waypoint.
    pub fn from_waypoints(mut waypoints: Vec<Waypoint>) -> Result<Path, Invalid> {
        if waypoints.len() > MAX_WAYPOINTS {
            return Err(Invalid::Full);
        }
        for w in &mut waypoints {
            w.validate()?;
            w.rot = w.rotation().to_array();
        }
        waypoints.sort_by(|a, b| a.time.total_cmp(&b.time));
        if waypoints.windows(2).any(|w| w[1].time - w[0].time <= TIME_EPSILON) {
            return Err(Invalid::TimeTaken);
        }
        Ok(Path {
            waypoints,
            ..Path::default()
        })
    }

    /// The time of the last waypoint: how long the path plays.
    pub fn duration(&self) -> f32 {
        self.waypoints.last().map_or(0.0, |w| w.time)
    }

    /// The time the next captured waypoint takes: `step` after the last.
    pub fn next_time(&self, step: f32) -> f32 {
        self.waypoints.last().map_or(0.0, |w| w.time + step)
    }

    /// Put `waypoint` at its time; one already there is replaced and keeps
    /// its transition when the new one has none of its own. Returns its
    /// index.
    pub fn insert(&mut self, mut waypoint: Waypoint) -> Result<usize, Invalid> {
        waypoint.validate()?;
        waypoint.rot = waypoint.rotation().to_array();
        let at = self.waypoints.partition_point(|w| w.time < waypoint.time - TIME_EPSILON);
        let full = self.waypoints.len() >= MAX_WAYPOINTS;
        match self.waypoints.get_mut(at) {
            Some(old) if (old.time - waypoint.time).abs() <= TIME_EPSILON => {
                if waypoint.transition == Transition::Smooth {
                    waypoint.transition = old.transition;
                }
                *old = waypoint;
            }
            _ if full => return Err(Invalid::Full),
            _ => self.waypoints.insert(at, waypoint),
        }
        Ok(at)
    }

    /// Waypoint `index` moves to `time`, the later ones by as much (their
    /// spacing stays). Returns where it is afterwards; nothing changes when
    /// a time would leave the range or meet another.
    pub fn retime(&mut self, index: usize, time: f32) -> Result<usize, Invalid> {
        let Some(old) = self.waypoints.get(index).map(|w| w.time) else {
            return Err(Invalid::Time);
        };
        let delta = time - old;
        let mut updated = self.waypoints.clone();
        for w in &mut updated[index..] {
            w.time += delta;
            if !w.time.is_finite() || !(0.0..=MAX_TIME).contains(&w.time) {
                return Err(Invalid::Time);
            }
        }
        // (moving earlier can pass the waypoints before it)
        let mut order: Vec<usize> = (0..updated.len()).collect();
        order.sort_by(|a, b| updated[*a].time.total_cmp(&updated[*b].time));
        let sorted: Vec<Waypoint> = order.iter().map(|i| updated[*i].clone()).collect();
        if sorted.windows(2).any(|w| w[1].time - w[0].time <= TIME_EPSILON) {
            return Err(Invalid::TimeTaken);
        }
        self.waypoints = sorted;
        Ok(order.iter().position(|i| *i == index).unwrap_or(index))
    }

    /// Reorder: the shots of waypoints `a` and `b` trade places, the times
    /// and the transitions of the two slots stay.
    pub fn swap_shots(&mut self, a: usize, b: usize) {
        if a == b || a >= self.waypoints.len() || b >= self.waypoints.len() {
            return;
        }
        let (ta, tb) = (self.waypoints[a].time, self.waypoints[b].time);
        let (ka, kb) = (self.waypoints[a].transition, self.waypoints[b].transition);
        self.waypoints.swap(a, b);
        (self.waypoints[a].time, self.waypoints[b].time) = (ta, tb);
        (self.waypoints[a].transition, self.waypoints[b].transition) = (ka, kb);
    }
}

// ------------------------------------------------------------ view ↔ waypoint

/// The rotation of a view: +X along it, +Z up, the roll included.
pub fn rotation_of(view: &View) -> Quat {
    let at = view.forward();
    let up = Quat::from_axis_angle(at, view.roll) * view_reference_up(at);
    let left = up.cross(at).normalize_or(Vec3::Y);
    let up = at.cross(left).normalize_or(Vec3::Z);
    Quat::from_mat3(&Mat3::from_cols(at, left, up)).normalize()
}

/// The roll of a rotation around its forward axis, as `Camera::roll`.
pub fn roll_of(rotation: Quat) -> f32 {
    let at = rotation * Vec3::X;
    let flat = |v: Vec3| (v - at * v.dot(at)).normalize_or_zero();
    let (reference, up) = (flat(view_reference_up(at)), flat(rotation * Vec3::Z));
    reference.cross(up).dot(at).atan2(reference.dot(up))
}

/// One instant of a path, in global metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub global: DVec3,
    pub rotation: Quat,
    pub fov: f32,
}

impl Sample {
    /// The sample in render space, the main region's corner at `origin`.
    pub fn view(&self, origin: (u32, u32)) -> View {
        let position = Vec3::new(
            (self.global.x - f64::from(origin.0)) as f32,
            (self.global.y - f64::from(origin.1)) as f32,
            self.global.z as f32,
        );
        View {
            position,
            target: position + self.rotation * Vec3::X * FOCUS_DISTANCE,
            fov: self.fov,
            roll: roll_of(self.rotation),
        }
    }
}

/// `view` (render space, the main region's corner at `origin`) as a
/// waypoint of the region `handle` named `region`.
pub fn waypoint_of(view: &View, origin: (u32, u32), handle: u64, region: String) -> Waypoint {
    let (rx, ry) = aurora_net::handle_to_origin(handle);
    let gx = f64::from(origin.0) + f64::from(view.position.x);
    let gy = f64::from(origin.1) + f64::from(view.position.y);
    Waypoint {
        region,
        handle,
        pos: [(gx - f64::from(rx)) as f32, (gy - f64::from(ry)) as f32, view.position.z],
        rot: rotation_of(view).to_array(),
        fov: view.fov,
        ..Waypoint::default()
    }
}

// --------------------------------------------------------------------- motion

/// The view at `time` on the path's clock: the first waypoint before its
/// time, the last one after its own. None: an empty path.
///
/// Smooth is a Hermite curve on the clock. The speed at a waypoint is the
/// difference of its neighbours over their time span; zero at the path's
/// two ends and next to an Ease segment (met at rest); one-sided next to a
/// Cut (nothing bends toward the other shot) and next to a Linear segment
/// (its speed, bounded so the curve cannot overshoot its own chord); zero
/// next to a position held between two waypoints. The rotation and the lens
/// follow the same eased progress.
pub fn evaluate(path: &Path, time: f32) -> Option<Sample> {
    let points = &path.waypoints;
    let at = |w: &Waypoint| Sample {
        global: w.global(),
        rotation: w.rotation(),
        fov: w.fov,
    };
    let (first, last) = (points.first()?, points.last()?);
    if points.len() == 1 || !time.is_finite() || time <= first.time {
        return Some(at(first));
    }
    if time >= last.time {
        return Some(at(last));
    }
    let i1 = points.partition_point(|w| w.time <= time).clamp(1, points.len() - 1);
    let i0 = i1 - 1;
    let (w0, w1) = (&points[i0], &points[i1]);
    let (p0, p1) = (w0.global(), w1.global());
    let h = f64::from(w1.time - w0.time);
    let s = if h > f64::from(TIME_EPSILON) {
        (f64::from(time - w0.time) / h).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let held = |i: usize| points[i].global().distance(points[i + 1].global()) <= HOLD_DISTANCE;
    let mix = |position: DVec3, progress: f64| Sample {
        global: if held(i0) { p0 } else { position },
        rotation: w0.rotation().slerp(w1.rotation(), progress as f32).normalize(),
        fov: w0.fov + (w1.fov - w0.fov) * progress as f32,
    };
    let (s2, s3) = (s * s, s * s * s);
    // Hermite basis: the two ends, then their speeds
    let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
    match w0.transition {
        Transition::Cut => return Some(at(w0)),
        Transition::Linear => return Some(mix(p0.lerp(p1, s), s)),
        Transition::Ease => return Some(mix(p0 * h00 + p1 * h01, h01)),
        Transition::Smooth => {}
    }
    // the speed through waypoint `i`, and the rate of the progress there
    let tangent = |i: usize| -> (DVec3, f64) {
        if i == 0 || i + 1 >= points.len() {
            return (DVec3::ZERO, 0.0);
        }
        use Transition::{Cut, Ease, Linear, Smooth};
        let (before, after) = (points[i - 1].transition, points[i].transition);
        let (from, to) = match (before, after) {
            (Ease, _) | (_, Ease) | (Cut, Cut) => return (DVec3::ZERO, 0.0),
            (Cut, _) | (Smooth, Linear) => (i, i + 1),
            (_, Cut) | (Linear, Smooth) => (i - 1, i),
            _ => (i - 1, i + 1),
        };
        let span = f64::from(points[to].time - points[from].time);
        if span <= f64::from(TIME_EPSILON) {
            return (DVec3::ZERO, 0.0);
        }
        let mut rate = (to - from) as f64 / span;
        let mut velocity = if held(i - 1) || held(i) {
            DVec3::ZERO
        } else {
            (points[to].global() - points[from].global()) / span
        };
        // a Linear neighbour's speed over this segment's own time: bounded
        // to three chords, the progress to three segments
        let smooth = match (before, after) {
            (Linear, Smooth) => Some(i),
            (Smooth, Linear) => Some(i - 1),
            _ => None,
        };
        if let Some(start) = smooth {
            let own = f64::from(points[start + 1].time - points[start].time);
            let chord = points[start].global().distance(points[start + 1].global());
            let reach = velocity.length() * own;
            if reach > 3.0 * chord && reach > 0.0 {
                velocity *= 3.0 * chord / reach;
            }
            if rate * own > 3.0 {
                rate = 3.0 / own;
            }
        }
        (velocity, rate)
    };
    let ((v0, r0), (v1, r1)) = (tangent(i0), tangent(i1));
    let position = p0 * h00 + v0 * (h10 * h) + p1 * h01 + v1 * (h11 * h);
    let progress = (h10 * h * r0 + h01 + h11 * h * r1).clamp(0.0, 1.0);
    Some(mix(position, progress))
}

// --------------------------------------------------------------------- player

/// What a frame of playback shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Frame {
    Playing(Sample),
    /// The last shot, held until the user moves the camera or stops.
    Holding(Sample),
    /// The path ended (or lost its waypoints): the camera goes back.
    Finished,
}

/// A path being played: where its clock is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Player {
    elapsed: f32,
    holding: bool,
}

impl Player {
    /// Play from `time` on the path's clock (0: the start; a waypoint's
    /// time: from that waypoint).
    pub fn start(time: f32) -> Player {
        Player {
            elapsed: if time.is_finite() { time.max(0.0) } else { 0.0 },
            holding: false,
        }
    }

    #[cfg(test)]
    pub fn holding(&self) -> bool {
        self.holding
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// `dt` seconds later. At the end a looped path starts again (a jump
    /// back to its first waypoint); otherwise the last shot is held, or the
    /// playback finishes.
    pub fn advance(&mut self, path: &Path, dt: f32) -> Frame {
        let duration = path.duration();
        if !self.holding {
            self.elapsed += if dt.is_finite() { dt.max(0.0) } else { 0.0 };
            if self.elapsed >= duration {
                if path.looped && duration > TIME_EPSILON {
                    self.elapsed %= duration;
                } else {
                    self.elapsed = duration;
                    if !path.hold {
                        return Frame::Finished;
                    }
                    self.holding = true;
                }
            }
        }
        match (evaluate(path, self.elapsed), self.holding) {
            (Some(sample), true) => Frame::Holding(sample),
            (Some(sample), false) => Frame::Playing(sample),
            (None, _) => Frame::Finished,
        }
    }
}

// ------------------------------------------------------------ stop conditions

/// What can end a playback, as things are this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Conditions {
    /// A teleport started or arrived (not a border crossing).
    pub teleported: bool,
    /// Every waypoint's region is connected.
    pub regions_connected: bool,
    pub mouselook: bool,
    /// The Camera Lock is on.
    pub locked: bool,
    /// The user moved the camera.
    pub user_input: bool,
    /// An RLVa camera restriction is active.
    pub restricted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Teleport,
    RegionNotConnected,
    Mouselook,
    CameraLock,
    Restricted,
    /// The user's own camera input: the camera goes on from where it is.
    UserInput,
}

/// Why the playback stops now, if it does. A reason that makes the played
/// positions meaningless comes before one that only interrupts.
pub fn stop_reason(c: &Conditions) -> Option<Stop> {
    if c.teleported {
        Some(Stop::Teleport)
    } else if c.restricted {
        Some(Stop::Restricted)
    } else if c.mouselook {
        Some(Stop::Mouselook)
    } else if c.locked {
        Some(Stop::CameraLock)
    } else if !c.regions_connected {
        Some(Stop::RegionNotConnected)
    } else if c.user_input {
        Some(Stop::UserInput)
    } else {
        None
    }
}

// -------------------------------------------------------------------- library

/// The account's named paths (`camera_paths.json` in its folder).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    version: u32,
    pub paths: Vec<Path>,
}

impl Library {
    /// The saved paths; none when there is no file yet or it cannot be read.
    pub fn load(path: &FilePath) -> Library {
        let Ok(bytes) = std::fs::read(path) else {
            return Library::default();
        };
        match Library::parse(&bytes) {
            Ok(library) => library,
            Err(error) => {
                log::warn!("camera paths: {} unreadable ({error})", path.display());
                Library::default()
            }
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Library, serde_json::Error> {
        let mut library: Library = serde_json::from_slice(bytes)?;
        // each path through the same checks as one built in the window
        library.paths = std::mem::take(&mut library.paths)
            .into_iter()
            .filter(|p| !p.name.trim().is_empty())
            .filter_map(|p| match Path::from_waypoints(p.waypoints) {
                Ok(checked) => Some(Path {
                    name: p.name,
                    looped: p.looped,
                    hold: p.hold,
                    ..checked
                }),
                Err(error) => {
                    log::warn!("camera paths: \"{}\" dropped ({error:?})", p.name);
                    None
                }
            })
            .take(MAX_PATHS)
            .collect();
        Ok(library)
    }

    pub fn save(&self, path: &FilePath) -> std::io::Result<()> {
        let out = Library {
            version: FILE_VERSION,
            paths: self.paths.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&out).map_err(std::io::Error::other)?;
        crate::atomic_file::write(path, &bytes)
    }

    /// Keep `path` under its name: a path of that name is replaced where it
    /// is, a new one goes last. False: no name, or the library is full.
    pub fn store(&mut self, path: Path) -> bool {
        if path.name.trim().is_empty() {
            return false;
        }
        match self.paths.iter().position(|p| p.name == path.name) {
            Some(i) => self.paths[i] = path,
            None if self.paths.len() < MAX_PATHS => self.paths.push(path),
            None => return false,
        }
        true
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.paths.len();
        self.paths.retain(|p| p.name != name);
        self.paths.len() != before
    }

    pub fn get(&self, name: &str) -> Option<&Path> {
        self.paths.iter().find(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERE: u64 = (256_000u64 << 32) | 256_000;

    fn wp(x: f32, time: f32, transition: Transition) -> Waypoint {
        Waypoint {
            handle: HERE,
            pos: [x, 100.0, 30.0],
            time,
            transition,
            ..Waypoint::default()
        }
    }

    fn path(points: Vec<Waypoint>) -> Path {
        Path {
            name: "test".into(),
            ..Path::from_waypoints(points).expect("valid waypoints")
        }
    }

    fn x(s: Sample) -> f32 {
        (s.global.x - 256_000.0) as f32
    }

    fn at(p: &Path, time: f32) -> f32 {
        x(evaluate(p, time).expect("not empty"))
    }

    fn speed(p: &Path, time: f32) -> f32 {
        (at(p, time + 0.005) - at(p, time - 0.005)) / 0.01
    }

    #[test]
    fn linear_goes_at_an_even_pace_and_the_ends_hold() {
        let p = path(vec![wp(0.0, 1.0, Transition::Linear), wp(10.0, 3.0, Transition::Linear)]);
        for (t, want) in [(0.0, 0.0), (1.0, 0.0), (1.5, 2.5), (2.0, 5.0), (3.0, 10.0), (99.0, 10.0)] {
            assert!((at(&p, t) - want).abs() < 1e-3, "{t}");
        }
        assert!(evaluate(&Path::default(), 0.0).is_none());
    }

    #[test]
    fn ease_goes_from_rest_to_rest() {
        let p = path(vec![wp(0.0, 0.0, Transition::Ease), wp(10.0, 2.0, Transition::Linear)]);
        let quarter = at(&p, 0.5);
        assert!(quarter < 2.5, "slow start: {quarter}");
        assert!((at(&p, 1.0) - 5.0).abs() < 1e-3);
        assert!((at(&p, 1.5) - (10.0 - quarter)).abs() < 1e-3);
        assert!(speed(&p, 0.01).abs() < 0.5 && speed(&p, 1.99).abs() < 0.5);
    }

    #[test]
    fn cut_holds_its_shot_then_jumps_at_the_next_time() {
        let p = path(vec![wp(0.0, 0.0, Transition::Cut), wp(10.0, 2.0, Transition::Linear)]);
        assert_eq!(at(&p, 0.0), 0.0);
        assert_eq!(at(&p, 1.99), 0.0);
        assert_eq!(at(&p, 2.0), 10.0);
    }

    #[test]
    fn smooth_passes_through_every_waypoint_at_a_continuous_speed() {
        let mut points = vec![
            wp(0.0, 0.0, Transition::Smooth),
            wp(10.0, 2.0, Transition::Smooth),
            wp(10.0, 4.0, Transition::Smooth),
            wp(0.0, 6.0, Transition::Smooth),
        ];
        points[2].pos[1] = 110.0;
        points[3].pos[1] = 110.0;
        let p = path(points);
        for w in &p.waypoints {
            let s = evaluate(&p, w.time).expect("on the path");
            assert!(s.global.distance(w.global()) < 1e-6);
        }
        // the middle segment bulges outward: a curve, not the straight line
        assert!(at(&p, 3.0) > 10.2, "{}", at(&p, 3.0));
        // the speed through an interior waypoint: its neighbours' difference
        // over their time span, the same on both sides
        let (before, after) = (speed(&p, 1.99), speed(&p, 2.01));
        assert!((before - after).abs() < 0.3, "{before} {after}");
        assert!((before - 2.5).abs() < 0.3, "{before}");
        // rest at both ends of the path
        assert!(speed(&p, 0.01).abs() < 0.3 && speed(&p, 5.99).abs() < 0.3);
    }

    #[test]
    fn smooth_adapts_to_the_segments_next_to_it() {
        use Transition::{Cut, Ease, Linear, Smooth};
        let line = |kinds: [Transition; 3]| {
            path(vec![
                wp(0.0, 0.0, kinds[0]),
                wp(10.0, 2.0, kinds[1]),
                wp(20.0, 4.0, kinds[2]),
                wp(30.0, 6.0, Smooth),
            ])
        };
        // after a Linear segment: its speed (5 m/s), no jolt
        assert!((speed(&line([Linear, Smooth, Smooth]), 2.01) - 5.0).abs() < 0.3);
        // before a Linear segment too
        assert!((speed(&line([Smooth, Linear, Smooth]), 1.99) - 5.0).abs() < 0.3);
        // next to an Ease segment: at rest
        assert!(speed(&line([Ease, Smooth, Smooth]), 2.01).abs() < 0.3);
        assert!(speed(&line([Smooth, Ease, Smooth]), 1.99).abs() < 0.3);
        // after a Cut: one-sided, toward the next waypoint only
        assert!((speed(&line([Cut, Smooth, Smooth]), 2.01) - 5.0).abs() < 0.3);
        // a Linear neighbour a hundred times faster cannot throw the curve
        // past three chords
        let fast = path(vec![wp(0.0, 0.0, Linear), wp(1000.0, 1.0, Smooth), wp(1001.0, 11.0, Smooth)]);
        let furthest = (0..100).map(|n| at(&fast, 1.0 + n as f32 * 0.1)).fold(0.0f32, f32::max);
        assert!(furthest < 1003.1, "{furthest}");
    }

    #[test]
    fn a_held_position_stays_exactly_still_while_the_view_turns() {
        let mut turn = wp(10.0, 2.0, Transition::Smooth);
        turn.rot = Quat::from_rotation_z(1.0).to_array();
        let p = path(vec![
            wp(0.0, 0.0, Transition::Smooth),
            wp(10.0, 1.0, Transition::Smooth),
            turn,
            wp(20.0, 3.0, Transition::Smooth),
        ]);
        for n in 0..=10 {
            let s = evaluate(&p, 1.0 + n as f32 * 0.1).expect("on the path");
            assert_eq!(x(s), 10.0);
        }
        let mid = evaluate(&p, 1.5).expect("on the path");
        assert!((mid.rotation * Vec3::X).y > 0.2, "turning");
        // and the camera arrives at the hold slowing to rest (5 m/s without it)
        assert!(speed(&p, 0.99).abs() < 1.0);
    }

    #[test]
    fn rotation_and_lens_follow_the_transition() {
        let mut b = wp(10.0, 2.0, Transition::Linear);
        b.rot = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
        b.fov = 0.5;
        let mut a = wp(0.0, 0.0, Transition::Linear);
        a.fov = 1.0;
        let p = path(vec![a, b]);
        let mid = evaluate(&p, 1.0).expect("mid");
        let forward = mid.rotation * Vec3::X;
        assert!((forward.x - forward.y).abs() < 1e-4, "{forward:?}");
        assert!((mid.fov - 0.75).abs() < 1e-5);
    }

    #[test]
    fn positions_are_global_across_regions() {
        let mut b = wp(10.0, 2.0, Transition::Linear);
        b.handle = (256_256u64 << 32) | 256_000; // the region to the east
        let p = path(vec![wp(250.0, 0.0, Transition::Linear), b]);
        // 250 m in the first region to 10 m in the next: 16 m, not -240
        assert!((at(&p, 1.0) - 258.0).abs() < 1e-3);
        // seen from the eastern region, the same place
        let s = evaluate(&p, 1.0).expect("mid");
        assert!((s.view((256_256, 256_000)).position.x - 2.0).abs() < 1e-3);
    }

    #[test]
    fn a_view_survives_the_round_trip_through_a_waypoint() {
        for (target, roll) in [
            (Vec3::new(5.0, 3.0, -2.0), 0.0),
            (Vec3::new(-4.0, 1.0, 6.0), 0.4),
            (Vec3::new(0.0, 0.0, 5.0), -0.3), // straight up
        ] {
            let position = Vec3::new(40.0, 50.0, 60.0);
            let view = View {
                position,
                target: position + target,
                fov: 0.9,
                roll,
            };
            let w = waypoint_of(&view, (256_000, 256_000), HERE, "Here".into());
            assert_eq!(w.pos, [40.0, 50.0, 60.0]);
            let p = path(vec![w]);
            let back = evaluate(&p, 0.0).expect("one").view((256_000, 256_000));
            assert!(back.position.distance(view.position) < 1e-4);
            assert!(back.forward().dot(view.forward()) > 0.99999);
            assert!((back.roll - roll).abs() < 1e-4, "{} {roll}", back.roll);
            assert!((back.fov - 0.9).abs() < 1e-6);
        }
    }

    fn play(p: &Path, from: f32, seconds: f32) -> (Player, Frame) {
        let mut player = Player::start(from);
        let mut frame = player.advance(p, 0.0);
        for _ in 0..(seconds * 10.0).round() as usize {
            frame = player.advance(p, 0.1);
        }
        (player, frame)
    }

    fn three() -> Path {
        path(vec![
            wp(0.0, 0.0, Transition::Linear),
            wp(10.0, 1.0, Transition::Linear),
            wp(20.0, 2.0, Transition::Linear),
        ])
    }

    #[test]
    fn the_player_holds_the_last_shot_by_default() {
        let p = three();
        assert!(p.hold);
        let (player, frame) = play(&p, 0.0, 0.5);
        assert!(!player.holding());
        assert!(matches!(frame, Frame::Playing(s) if (x(s) - 5.0).abs() < 0.2));
        let (player, frame) = play(&p, 0.0, 5.0);
        assert!(player.holding());
        assert!(matches!(frame, Frame::Holding(s) if x(s) == 20.0));
        // and stays there
        assert!(matches!(play(&p, 0.0, 60.0).1, Frame::Holding(_)));
    }

    #[test]
    fn without_hold_the_player_finishes() {
        let mut p = three();
        p.hold = false;
        assert_eq!(play(&p, 0.0, 3.0).1, Frame::Finished);
        // from the last waypoint there is nothing to play
        assert_eq!(play(&p, 2.0, 0.0).1, Frame::Finished);
        p.hold = true;
        assert!(matches!(play(&p, 2.0, 0.0).1, Frame::Holding(s) if x(s) == 20.0));
    }

    #[test]
    fn a_loop_starts_again_from_the_first_waypoint_and_goes_on() {
        let mut p = three();
        p.looped = true;
        // 2 s of path, 0.5 s into the second lap
        let (player, frame) = play(&p, 0.0, 2.5);
        assert!((player.elapsed() - 0.5).abs() < 0.01);
        assert!(matches!(frame, Frame::Playing(s) if (x(s) - 5.0).abs() < 0.2), "{frame:?}");
        // laps later it is still playing, never holding
        let (player, frame) = play(&p, 0.0, 41.3);
        assert!(!player.holding() && matches!(frame, Frame::Playing(_)));
    }

    #[test]
    fn play_from_a_selected_waypoint_starts_on_it() {
        let p = three();
        let (_, frame) = play(&p, p.waypoints[1].time, 0.0);
        assert!(matches!(frame, Frame::Playing(s) if x(s) == 10.0));
    }

    #[test]
    fn edits_while_playing_and_broken_times_never_hang_or_panic() {
        let mut p = three();
        p.looped = true;
        let mut player = Player::start(0.0);
        assert!(matches!(player.advance(&p, 0.1), Frame::Playing(_)));
        // the waypoints are deleted under the player
        p.waypoints.clear();
        assert_eq!(player.advance(&p, 0.1), Frame::Finished);
        // a broken frame time moves nothing
        let p = three();
        let mut player = Player::start(f32::NAN);
        assert!(matches!(player.advance(&p, f32::NAN), Frame::Playing(s) if x(s) == 0.0));
        // a single waypoint is a shot to hold
        let one = path(vec![wp(3.0, 0.0, Transition::Smooth)]);
        assert!(matches!(play(&one, 0.0, 0.0).1, Frame::Holding(_)));
    }

    #[test]
    fn waypoints_stay_sorted_and_one_per_time() {
        let mut p = Path::default();
        assert_eq!(p.next_time(DEFAULT_STEP), 0.0);
        assert_eq!(p.insert(wp(0.0, 0.0, Transition::Smooth)), Ok(0));
        assert_eq!(p.insert(wp(20.0, 6.0, Transition::Cut)), Ok(1));
        assert_eq!(p.insert(wp(10.0, 3.0, Transition::Smooth)), Ok(1));
        assert_eq!(p.next_time(DEFAULT_STEP), 9.0);
        // captured again at a time: replaced, its transition kept
        assert_eq!(p.insert(wp(25.0, 6.0, Transition::Smooth)), Ok(2));
        assert_eq!(p.waypoints.len(), 3);
        assert_eq!((p.waypoints[2].pos[0], p.waypoints[2].transition), (25.0, Transition::Cut));
        // refused, nothing changed
        assert_eq!(p.insert(wp(0.0, -1.0, Transition::Smooth)), Err(Invalid::Time));
        assert_eq!(p.insert(wp(0.0, MAX_TIME + 1.0, Transition::Smooth)), Err(Invalid::Time));
        assert_eq!(p.insert(wp(f32::NAN, 1.0, Transition::Smooth)), Err(Invalid::Pose));
        assert_eq!(p.insert(wp(1e6, 1.0, Transition::Smooth)), Err(Invalid::Range));
        let mut wide = wp(0.0, 1.0, Transition::Smooth);
        wide.fov = 3.1;
        assert_eq!(p.insert(wide), Err(Invalid::View));
        let mut skewed = wp(0.0, 1.0, Transition::Smooth);
        skewed.rot = [0.0, 0.0, 0.0, 2.0];
        assert_eq!(p.insert(skewed), Err(Invalid::Pose));
        assert_eq!(p.waypoints.len(), 3);
        // a list with a bad waypoint or two at one time gives no path
        assert_eq!(
            Path::from_waypoints(vec![wp(0.0, 1.0, Transition::Smooth), wp(5.0, 1.0, Transition::Smooth)]),
            Err(Invalid::TimeTaken)
        );
        let sorted = Path::from_waypoints(vec![wp(5.0, 2.0, Transition::Smooth), wp(0.0, 1.0, Transition::Smooth)]);
        assert_eq!(sorted.expect("sorted").waypoints[0].pos[0], 0.0);
    }

    #[test]
    fn a_new_time_shifts_the_later_waypoints_by_as_much() {
        let mut p = path(vec![
            wp(0.0, 0.0, Transition::Smooth),
            wp(10.0, 2.0, Transition::Smooth),
            wp(20.0, 4.0, Transition::Smooth),
            wp(30.0, 6.0, Transition::Smooth),
        ]);
        assert_eq!(p.retime(1, 3.0), Ok(1));
        let times: Vec<f32> = p.waypoints.iter().map(|w| w.time).collect();
        assert_eq!(times, [0.0, 3.0, 5.0, 7.0]);
        // onto another waypoint's time, or out of range: nothing changes
        assert_eq!(p.retime(1, 0.0), Err(Invalid::TimeTaken));
        assert_eq!(p.retime(1, MAX_TIME), Err(Invalid::Time));
        assert_eq!(p.retime(9, 1.0), Err(Invalid::Time));
        let after: Vec<f32> = p.waypoints.iter().map(|w| w.time).collect();
        assert_eq!(after, times);
        // reordering trades the shots, the clock stays
        p.swap_shots(0, 1);
        assert_eq!((p.waypoints[0].pos[0], p.waypoints[0].time), (10.0, 0.0));
        assert_eq!((p.waypoints[1].pos[0], p.waypoints[1].time), (0.0, 3.0));
    }

    #[test]
    fn stop_reasons_in_order() {
        let fine = Conditions {
            regions_connected: true,
            ..Conditions::default()
        };
        assert_eq!(stop_reason(&fine), None);
        for (c, want) in [
            (Conditions { teleported: true, ..fine }, Stop::Teleport),
            (Conditions { restricted: true, ..fine }, Stop::Restricted),
            (Conditions { mouselook: true, ..fine }, Stop::Mouselook),
            (Conditions { locked: true, ..fine }, Stop::CameraLock),
            (
                Conditions {
                    regions_connected: false,
                    ..fine
                },
                Stop::RegionNotConnected,
            ),
            (Conditions { user_input: true, ..fine }, Stop::UserInput),
        ] {
            assert_eq!(stop_reason(&c), Some(want));
        }
        // a teleport wins over the input that may come with it
        let both = Conditions {
            teleported: true,
            user_input: true,
            ..fine
        };
        assert_eq!(stop_reason(&both), Some(Stop::Teleport));
        // RLVa wins over the lock
        let both = Conditions {
            restricted: true,
            locked: true,
            ..fine
        };
        assert_eq!(stop_reason(&both), Some(Stop::Restricted));
    }

    #[test]
    fn the_library_stores_by_name_and_reads_old_or_damaged_files() {
        let mut lib = Library::default();
        let mut tour = path(vec![wp(0.0, 0.0, Transition::Linear), wp(10.0, 1.0, Transition::Cut)]);
        tour.name = "Tour".into();
        assert!(lib.store(tour.clone()));
        tour.looped = true;
        assert!(lib.store(tour.clone()), "replaced");
        assert_eq!(lib.paths.len(), 1);
        assert!(lib.get("Tour").is_some_and(|p| p.looped));
        assert!(!lib.store(Path::default()), "no name");
        let bytes = serde_json::to_vec(&lib).expect("json");
        assert_eq!(Library::parse(&bytes).expect("round trip").paths, lib.paths);
        // fields missing: defaults (hold on); a path with a waypoint that
        // cannot be used or without a name: dropped, the others kept
        let old = br#"{"paths":[
            {"name":"Old","waypoints":[{"pos":[1,2,3]},{"pos":[4,5,6],"time":2}]},
            {"name":"Bad","waypoints":[{"pos":[1,2,3],"fov":99}]},
            {"name":" "}]}"#;
        let mut lib = Library::parse(old).expect("tolerant");
        assert_eq!(lib.paths.len(), 1);
        let p = lib.get("Old").expect("kept");
        assert!(p.hold && !p.looped);
        assert_eq!(p.waypoints.len(), 2);
        assert_eq!(p.waypoints[1].time, 2.0);
        assert!(lib.remove("Old") && !lib.remove("Old"));
    }
}
