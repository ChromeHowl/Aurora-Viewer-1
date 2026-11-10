//! The Camera Path file viewers exchange: LLSD XML, pretty printed. All of
//! the format is in this module.
//!
//! A map with `version` (integer 2) and `waypoints`, an array of maps:
//! `time` (seconds from the start of the path), `region` (its name),
//! `region_origin` (the region's south-west corner in global metres, x and
//! y), `position` (in the region), `rotation` (quaternion x, y, z, w: +X
//! forward, +Y left, +Z up), `view` (vertical field of view, radians) and,
//! only when it is not the default, `transition` ("linear", "ease", "cut":
//! how the camera goes from this waypoint to the next). Keys the reader
//! does not know are ignored; an unknown transition is the default.
//!
//! Version 1 files have an `origin` per waypoint instead of the region and
//! the position: a position in the frame of the region the avatar is in
//! when the file is read. They are read as waypoints of that region.
//!
//! A file is taken whole or not at all: one bad waypoint refuses it.

use super::path::{Invalid, MAX_WAYPOINTS, Path, Transition, Waypoint};
use aurora_llsd::Llsd;

pub const VERSION: i32 = 2;
const VERSION_LEGACY: i32 = 1;
/// A file larger than this is not a camera path.
pub const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024;
/// The grid's global coordinates fit 32 bits.
const MAX_GLOBAL: f64 = 4_294_967_296.0;

fn transition_name(t: Transition) -> Option<&'static str> {
    match t {
        Transition::Smooth => None,
        Transition::Linear => Some("linear"),
        Transition::Ease => Some("ease"),
        Transition::Cut => Some("cut"),
    }
}

fn transition_named(name: &str) -> Transition {
    match name {
        "linear" => Transition::Linear,
        "ease" => Transition::Ease,
        "cut" => Transition::Cut,
        _ => Transition::Smooth,
    }
}

fn reals<T: Copy + Into<f64>>(values: &[T]) -> Llsd {
    Llsd::Array(values.iter().map(|v| Llsd::Real((*v).into())).collect())
}

pub fn write(path: &Path) -> String {
    let mut waypoints = Vec::with_capacity(path.waypoints.len());
    for w in &path.waypoints {
        let (x, y) = aurora_net::handle_to_origin(w.handle);
        let mut entry = Llsd::new_map();
        entry.insert("time", w.time);
        entry.insert("region", w.region.as_str());
        entry.insert("region_origin", reals(&[x, y]));
        entry.insert("position", reals(&w.pos));
        entry.insert("rotation", reals(&w.rot));
        entry.insert("view", w.fov);
        if let Some(name) = transition_name(w.transition) {
            entry.insert("transition", name);
        }
        waypoints.push(entry);
    }
    let mut data = Llsd::new_map();
    data.insert("version", VERSION);
    data.insert("waypoints", waypoints);
    aurora_llsd::to_pretty_xml_string(&data)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    /// The waypoints, sorted and checked; no name, loop or hold: the file
    /// has none.
    pub path: Path,
    /// A version 1 file: its positions were taken in the avatar's region.
    pub legacy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// Not LLSD XML, not a camera path, or a version this viewer does not
    /// know.
    Format,
    /// More than `MAX_WAYPOINTS`.
    TooMany,
    /// A waypoint lacks a value or holds one out of range (its place in the
    /// file, from 1).
    Waypoint(usize),
    /// Two waypoints at the same time.
    SameTime,
    Empty,
    /// A version 1 file needs the avatar's region, and there is none.
    NoRegion,
}

fn number(v: &Llsd) -> Option<f64> {
    match v {
        Llsd::Real(r) => Some(*r),
        Llsd::Integer(i) => Some(f64::from(*i)),
        _ => None,
    }
}

fn vector<const N: usize>(v: &Llsd) -> Option<[f64; N]> {
    let items = v.as_array();
    if !v.is_array() || items.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (slot, item) in out.iter_mut().zip(items) {
        *slot = number(item)?;
    }
    Some(out)
}

fn waypoint(entry: &Llsd, legacy_region: Option<(u64, &str)>) -> Option<Waypoint> {
    if !entry.is_map() {
        return None;
    }
    let time = number(entry.get("time"))? as f32;
    let fov = number(entry.get("view"))? as f32;
    let rot: [f64; 4] = vector(entry.get("rotation"))?;
    let (handle, region, pos) = match legacy_region {
        Some((handle, name)) => (handle, name.to_owned(), vector::<3>(entry.get("origin"))?),
        None => {
            let Llsd::String(region) = entry.get("region") else {
                return None;
            };
            let origin: [f64; 2] = vector(entry.get("region_origin"))?;
            if origin.iter().any(|v| !v.is_finite() || *v < 0.0 || *v >= MAX_GLOBAL) {
                return None;
            }
            let handle = aurora_net::origin_to_handle(origin[0].round() as u32, origin[1].round() as u32);
            (handle, region.clone(), vector::<3>(entry.get("position"))?)
        }
    };
    let transition = match entry.get("transition") {
        Llsd::String(name) => transition_named(name),
        _ => Transition::Smooth,
    };
    Some(Waypoint {
        time,
        region,
        handle,
        pos: pos.map(|v| v as f32),
        rot: rot.map(|v| v as f32),
        fov,
        transition,
    })
}

/// Read a file's bytes. `here`: the handle and the name of the region the
/// avatar is in, for a version 1 file.
pub fn read(bytes: &[u8], here: Option<(u64, &str)>) -> Result<Read, ReadError> {
    let data = aurora_llsd::from_xml(bytes).map_err(|_| ReadError::Format)?;
    let version = match data.get("version") {
        Llsd::Integer(v) if data.is_map() => *v,
        _ => 0,
    };
    if (version != VERSION && version != VERSION_LEGACY) || !data.get("waypoints").is_array() {
        return Err(ReadError::Format);
    }
    let entries = data.get("waypoints").as_array();
    if entries.len() > MAX_WAYPOINTS {
        return Err(ReadError::TooMany);
    }
    let legacy = version == VERSION_LEGACY;
    let legacy_region = match (legacy, here) {
        (true, None) => return Err(ReadError::NoRegion),
        (true, here) => here,
        (false, _) => None,
    };
    let mut waypoints = Vec::with_capacity(entries.len());
    for (n, entry) in entries.iter().enumerate() {
        let w = waypoint(entry, legacy_region).ok_or(ReadError::Waypoint(n + 1))?;
        w.validate().map_err(|_| ReadError::Waypoint(n + 1))?;
        waypoints.push(w);
    }
    if waypoints.is_empty() {
        return Err(ReadError::Empty);
    }
    let path = Path::from_waypoints(waypoints).map_err(|e| match e {
        Invalid::TimeTaken => ReadError::SameTime,
        Invalid::Full => ReadError::TooMany,
        _ => ReadError::Format,
    })?;
    Ok(Read { path, legacy })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERE: u64 = (255_232u64 << 32) | 256_512;

    fn tour() -> Path {
        let waypoints = [Transition::Smooth, Transition::Linear, Transition::Ease, Transition::Cut]
            .into_iter()
            .enumerate()
            .map(|(n, transition)| Waypoint {
                time: 1.25 * n as f32,
                region: "Ahern & <Co>".into(),
                handle: HERE,
                pos: [128.0 + 10.0 * n as f32, 64.5, 30.25],
                rot: glam::Quat::from_rotation_z(0.3 * n as f32).to_array(),
                fov: 0.5 + 0.1 * n as f32,
                transition,
            })
            .collect();
        Path::from_waypoints(waypoints).expect("valid")
    }

    #[test]
    fn a_path_survives_the_round_trip_exactly() {
        let path = tour();
        let xml = write(&path);
        let back = read(xml.as_bytes(), None).expect("our own file");
        assert!(!back.legacy);
        assert_eq!(back.path.waypoints, path.waypoints);
    }

    #[test]
    fn the_file_has_the_exchange_formats_keys() {
        let xml = write(&tour());
        let data = aurora_llsd::from_xml(xml.as_bytes()).expect("llsd");
        assert_eq!(data.get("version"), &Llsd::Integer(2));
        let first = data.get("waypoints").at(0);
        let keys: Vec<&str> = first.as_map().expect("a map").keys().map(String::as_str).collect();
        // a default transition writes no key
        assert_eq!(keys, ["position", "region", "region_origin", "rotation", "time", "view"]);
        assert_eq!(first.get("region_origin"), &reals(&[255_232u32, 256_512]));
        assert_eq!(first.get("position").as_array().len(), 3);
        assert_eq!(first.get("rotation").as_array().len(), 4);
        assert_eq!(data.get("waypoints").at(1).get("transition").as_str(), "linear");
        assert_eq!(data.get("waypoints").at(3).get("transition").as_str(), "cut");
    }

    fn file(version: &str, waypoints: &str) -> String {
        format!("<llsd><map><key>version</key>{version}<key>waypoints</key><array>{waypoints}</array></map></llsd>")
    }

    const GOOD: &str = "<map><key>time</key><real>0</real><key>region</key><string>Here</string>\
        <key>region_origin</key><array><real>255232</real><real>256512</real></array>\
        <key>position</key><array><real>1</real><real>2</real><real>3</real></array>\
        <key>rotation</key><array><real>0</real><real>0</real><real>0</real><real>1</real></array>\
        <key>view</key><real>1</real><key>extra</key><string>ignored</string>\
        <key>transition</key><string>wobble</string></map>";

    #[test]
    fn unknown_keys_and_transitions_are_tolerated() {
        let read = read(file("<integer>2</integer>", GOOD).as_bytes(), None).expect("tolerant");
        let [w] = &read.path.waypoints[..] else {
            panic!("one waypoint");
        };
        assert_eq!((w.handle, w.region.as_str(), w.pos), (HERE, "Here", [1.0, 2.0, 3.0]));
        assert_eq!(w.transition, Transition::Smooth);
    }

    #[test]
    fn a_bad_waypoint_refuses_the_whole_file() {
        let v2 = "<integer>2</integer>";
        for (broken, why) in [
            (GOOD.replace("<key>view</key><real>1</real>", ""), "no view"),
            (
                GOOD.replace("<key>view</key><real>1</real>", "<key>view</key><real>9</real>"),
                "view out of range",
            ),
            (
                GOOD.replace("<real>1</real><real>2</real><real>3</real>", "<real>1</real><real>2</real>"),
                "short position",
            ),
            (GOOD.replace("<string>Here</string>", "<integer>4</integer>"), "region not a string"),
            (GOOD.replace("<real>255232</real>", "<real>-256</real>"), "origin off the grid"),
            (
                GOOD.replace("<key>time</key><real>0</real>", "<key>time</key><real>-1</real>"),
                "negative time",
            ),
            (
                GOOD.replace(
                    "<real>0</real><real>0</real><real>0</real><real>1</real>",
                    "<real>0</real><real>0</real><real>0</real><real>3</real>",
                ),
                "rotation not unit",
            ),
        ] {
            let two = format!("{GOOD}{broken}");
            assert_eq!(read(file(v2, &two).as_bytes(), None), Err(ReadError::Waypoint(2)), "{why}");
        }
        // two at the same time
        let two = format!("{GOOD}{GOOD}");
        assert_eq!(read(file(v2, &two).as_bytes(), None), Err(ReadError::SameTime));
    }

    #[test]
    fn other_versions_and_non_paths_are_refused() {
        assert_eq!(read(file("<integer>3</integer>", GOOD).as_bytes(), None), Err(ReadError::Format));
        assert_eq!(read(file("<string>2</string>", GOOD).as_bytes(), None), Err(ReadError::Format));
        assert_eq!(read(file("<integer>2</integer>", "").as_bytes(), None), Err(ReadError::Empty));
        assert_eq!(read(b"not xml at all", None), Err(ReadError::Format));
        assert_eq!(read(b"<llsd><array /></llsd>", None), Err(ReadError::Format));
        assert_eq!(
            read(b"<llsd><map><key>version</key><integer>2</integer></map></llsd>", None),
            Err(ReadError::Format)
        );
    }

    #[test]
    fn version_one_positions_are_taken_in_the_avatars_region() {
        let old = "<map><key>time</key><real>2</real>\
            <key>origin</key><array><real>10</real><real>20</real><real>30</real></array>\
            <key>rotation</key><array><real>0</real><real>0</real><real>0</real><real>1</real></array>\
            <key>view</key><real>1</real></map>";
        let xml = file("<integer>1</integer>", old);
        assert_eq!(read(xml.as_bytes(), None), Err(ReadError::NoRegion));
        let read = read(xml.as_bytes(), Some((HERE, "Ahern"))).expect("legacy");
        assert!(read.legacy);
        let w = &read.path.waypoints[0];
        assert_eq!(
            (w.handle, w.region.as_str(), w.pos, w.time),
            (HERE, "Ahern", [10.0, 20.0, 30.0], 2.0)
        );
        // a version 2 waypoint in a version 1 file has no `origin`
        assert_eq!(
            super::read(file("<integer>1</integer>", GOOD).as_bytes(), Some((HERE, "Ahern"))),
            Err(ReadError::Waypoint(1))
        );
    }
}
