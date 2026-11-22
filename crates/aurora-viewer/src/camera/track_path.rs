//! The Director's track and the Camera Path, one into the other (Aurora's
//! own). Both are keyframes of a camera view on a clock, so a track the
//! Director films can be sent to the Camera Path (to export it as a path
//! file, or to play it from the viewfinder's Path mode), and a Camera Path
//! (captured, recorded, saved by name or imported from a file) can be
//! opened in the Director to be filmed. Pure: the app (`app/director.rs`)
//! supplies the main region's corner and where each point lies.
//!
//! What does not cross: a path's "Cut" and "Ease" transitions have no
//! counterpart in the track (a path with any transition that is not Smooth
//! becomes a straight-line track; an Ease waypoint becomes a keyframe the
//! camera rests on), and the track's lock, its hand-held shake and its lens
//! stay with the Director.

use super::path::{self, Path, Transition, Waypoint};
use super::track::{self, KeyFile, Track, TrackFile};
use crate::director::{Aim, Joint, Target};

/// Why a track or a path could not be turned into the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bridge {
    /// Nothing to turn.
    Empty,
    /// More keyframes than a track holds.
    TooMany,
    /// A view that is no pose, or a position no region holds.
    Invalid,
}

/// The track of `path`, for a main region whose corner is at `origin`.
pub fn track_from_path(path: &Path, origin: (u32, u32)) -> Result<Track, Bridge> {
    if path.waypoints.is_empty() {
        return Err(Bridge::Empty);
    }
    if path.waypoints.len() > track::MAX_KEYS {
        return Err(Bridge::TooMany);
    }
    let last = path.waypoints.len() - 1;
    // a path that is smooth between two waypoints only is a smooth track:
    // one straight, eased or cut segment makes the whole track straight
    let linear = path.waypoints[..last].iter().any(|w| w.transition != Transition::Smooth);
    let keys = path
        .waypoints
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let global = w.global();
            let before = i.checked_sub(1).map(|b| path.waypoints[b].transition);
            KeyFile {
                time: w.time,
                position: [global.x, global.y, global.z],
                rotation: w.rotation().to_array(),
                fov: w.fov,
                rest: w.transition == Transition::Ease || before == Some(Transition::Ease),
                locked: false,
                aim: None,
                ease: Default::default(),
                dolly_zoom: false,
                follow: None,
                extra: Default::default(),
            }
        })
        .collect();
    let file = TrackFile {
        version: track::FILE_VERSION,
        linear,
        ease_ends: true,
        keys,
    };
    // no keyframe of a path is locked, so no target is read from `older`
    let older = Aim {
        target: Target::Own,
        joint: Joint::Neck,
    };
    Track::from_file(&file, origin, older).ok_or(Bridge::Invalid)
}

/// The path of `track`, named `name`, for a main region whose corner is at
/// `origin`. `region_at` gives the handle and the name of the region that
/// holds a global point (x, y), `None` when none does.
pub fn path_from_track(
    track: &Track,
    origin: (u32, u32),
    name: &str,
    region_at: impl Fn(f64, f64) -> Option<(u64, String)>,
) -> Result<Path, Bridge> {
    if track.is_empty() {
        return Err(Bridge::Empty);
    }
    let transition = match track.curve {
        track::Curve::Smooth => Transition::Smooth,
        track::Curve::Linear => Transition::Linear,
    };
    let mut waypoints = Vec::with_capacity(track.len());
    for key in track.keys() {
        let [gx, gy, gz] = super::presets::to_global(origin, key.pose.position);
        let (handle, region) = region_at(gx, gy).ok_or(Bridge::Invalid)?;
        let (rx, ry) = aurora_net::handle_to_origin(handle);
        waypoints.push(Waypoint {
            time: key.time,
            region,
            handle,
            pos: [(gx - f64::from(rx)) as f32, (gy - f64::from(ry)) as f32, gz as f32],
            rot: key.pose.rotation.normalize().to_array(),
            fov: key.pose.fov.clamp(*path::VIEW_RANGE.start(), *path::VIEW_RANGE.end()),
            transition: if key.rest { Transition::Ease } else { transition },
        });
    }
    let mut path = Path::from_waypoints(waypoints).map_err(|_| Bridge::Invalid)?;
    path.name = name.to_owned();
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    const ORIGIN: (u32, u32) = (256_000, 256_000);

    fn waypoint(time: f32, x: f32, transition: Transition) -> Waypoint {
        Waypoint {
            time,
            region: "A".into(),
            handle: aurora_net::origin_to_handle(ORIGIN.0, ORIGIN.1),
            pos: [x, 100.0, 30.0],
            transition,
            ..Waypoint::default()
        }
    }

    fn here(_: f64, _: f64) -> Option<(u64, String)> {
        Some((aurora_net::origin_to_handle(ORIGIN.0, ORIGIN.1), "A".into()))
    }

    #[test]
    fn a_path_becomes_a_track_with_its_times_and_places() {
        let path = Path::from_waypoints(vec![
            waypoint(0.0, 10.0, Transition::Smooth),
            waypoint(4.0, 50.0, Transition::Smooth),
            waypoint(9.0, 90.0, Transition::Smooth),
        ])
        .expect("path");
        let track = track_from_path(&path, ORIGIN).expect("track");
        assert_eq!(track.len(), 3);
        assert_eq!(track.curve, track::Curve::Smooth);
        assert!((track.keys()[1].time - 4.0).abs() < 1e-4);
        assert!((track.keys()[2].pose.position - Vec3::new(90.0, 100.0, 30.0)).length() < 1e-3);
    }

    #[test]
    fn a_straight_or_eased_segment_makes_the_track_straight() {
        let path = Path::from_waypoints(vec![
            waypoint(0.0, 10.0, Transition::Ease),
            waypoint(4.0, 50.0, Transition::Smooth),
            waypoint(9.0, 90.0, Transition::Smooth),
        ])
        .expect("path");
        let track = track_from_path(&path, ORIGIN).expect("track");
        assert_eq!(track.curve, track::Curve::Linear);
        assert!(track.keys()[0].rest && track.keys()[1].rest && !track.keys()[2].rest);
    }

    #[test]
    fn an_empty_path_and_a_path_too_long_for_a_track_are_refused() {
        assert_eq!(track_from_path(&Path::default(), ORIGIN), Err(Bridge::Empty));
        let many: Vec<Waypoint> = (0..=track::MAX_KEYS)
            .map(|i| waypoint(i as f32, 10.0, Transition::Smooth))
            .collect();
        let path = Path::from_waypoints(many).expect("path");
        assert_eq!(track_from_path(&path, ORIGIN), Err(Bridge::TooMany));
    }

    #[test]
    fn a_track_makes_the_path_it_came_from() {
        let path = Path::from_waypoints(vec![
            waypoint(0.0, 10.0, Transition::Smooth),
            waypoint(4.0, 50.0, Transition::Smooth),
            waypoint(9.0, 90.0, Transition::Smooth),
        ])
        .expect("path");
        let track = track_from_path(&path, ORIGIN).expect("track");
        let back = path_from_track(&track, ORIGIN, "again", here).expect("path");
        assert_eq!(back.name, "again");
        assert_eq!(back.waypoints.len(), 3);
        for (a, b) in path.waypoints.iter().zip(&back.waypoints) {
            assert!((a.time - b.time).abs() < 1e-3);
            assert!(a.pos.iter().zip(&b.pos).all(|(x, y)| (x - y).abs() < 1e-2));
            assert_eq!(b.transition, Transition::Smooth);
        }
    }

    #[test]
    fn a_track_outside_every_region_is_refused() {
        let path = Path::from_waypoints(vec![waypoint(0.0, 10.0, Transition::Smooth)]).expect("path");
        let track = track_from_path(&path, ORIGIN).expect("track");
        assert_eq!(path_from_track(&track, ORIGIN, "x", |_, _| None), Err(Bridge::Invalid));
        assert_eq!(path_from_track(&Track::default(), ORIGIN, "x", here), Err(Bridge::Empty));
    }
}
