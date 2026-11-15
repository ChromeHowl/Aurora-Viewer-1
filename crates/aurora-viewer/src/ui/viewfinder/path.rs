//! The viewfinder's Path mode, the camera movie: the shutter places
//! waypoints on the Camera Path (`camera/path.rs`, the engine the Camera Path
//! window already runs), the dock plays, stops and records it, and a
//! timeline strip takes the place of the lens row to show and edit the
//! waypoints. This file is the mode's interface and its pure rules (the
//! timeline's time ↔ position mapping, what a drag may do, when the shutter
//! is greyed); the path itself, its saving and its playback stay in
//! `app/camera_tools.rs`, which `app/viewfinder_path.rs` reaches from here.
//! Aurora's own: the Legacy Viewer's snapshot floater has no such mode.
// (the strip that uses the rest comes with the next commit)
#![allow(dead_code)]

use crate::camera::path::MAX_TIME;

/// The shortest gap a drag leaves between two waypoints (seconds).
pub const MIN_GAP: f32 = 0.1;
/// The least time the ruler spans (seconds).
pub const MIN_SPAN: f32 = 30.0;

/// The shutter places waypoints; it waits while the path records (the
/// recording places them itself) or plays (the waypoints do not change
/// under a playback).
pub fn shutter_enabled(recording: bool, playing: bool) -> bool {
    !recording && !playing
}

/// The time the ruler spans for a path of `duration` seconds: a little more
/// than the path, in steps of ten seconds, never less than `MIN_SPAN`.
pub fn ruler_span(duration: f32) -> f32 {
    let d = if duration.is_finite() { duration.max(0.0) } else { 0.0 };
    (((d * 1.1) / 10.0).ceil() * 10.0).clamp(MIN_SPAN, MAX_TIME)
}

/// The x of time `t` on a ruler of `span` seconds that starts at `left` and
/// is `width` wide.
pub fn time_x(t: f32, span: f32, left: f32, width: f32) -> f32 {
    left + (t / span.max(f32::EPSILON)).clamp(0.0, 1.0) * width
}

/// The time at `x`, the inverse of `time_x` (kept on the ruler).
pub fn x_time(x: f32, span: f32, left: f32, width: f32) -> f32 {
    ((x - left) / width.max(1.0)).clamp(0.0, 1.0) * span
}

/// A time to the tenth of a second, which is how the markers snap.
pub fn snap(t: f32) -> f32 {
    (t * 10.0).round() / 10.0
}

/// Where a dragged marker goes: the time `wanted` under the pointer, snapped,
/// kept after the waypoint before it (`before`: its time) and on the ruler.
/// The later waypoints follow it by as much (`Path::retime`).
pub fn drag_time(wanted: f32, before: Option<f32>, span: f32) -> f32 {
    let lo = before.map_or(0.0, |b| b + MIN_GAP);
    let hi = span.min(MAX_TIME);
    snap(wanted).max(lo).min(hi.max(lo))
}

/// The ruler's ticks: how often a small one and a labelled one are drawn
/// (seconds).
pub fn tick_steps(span: f32) -> (f32, f32) {
    if span <= 40.0 {
        (1.0, 5.0)
    } else if span <= 120.0 {
        (5.0, 15.0)
    } else if span <= 300.0 {
        (10.0, 60.0)
    } else {
        (30.0, 120.0)
    }
}

/// A time as the clock shows it: minutes, seconds and a tenth.
pub fn time_text(t: f32) -> String {
    let tenths = (if t.is_finite() { t.max(0.0) } else { 0.0 } * 10.0).round() as u32;
    format!("{:02}:{:04.1}", tenths / 600, (tenths % 600) as f32 / 10.0)
}

/// The waypoint kept selected after waypoint `deleted` went.
pub fn selection_after_delete(selected: Option<usize>, deleted: usize) -> Option<usize> {
    match selected {
        Some(i) if i == deleted => None,
        Some(i) if i > deleted => Some(i - 1),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shutter_waits_for_a_recording_or_a_playback() {
        assert!(shutter_enabled(false, false));
        assert!(!shutter_enabled(true, false));
        assert!(!shutter_enabled(false, true));
        assert!(!shutter_enabled(true, true));
    }

    #[test]
    fn the_ruler_grows_in_steps_of_ten_seconds() {
        assert_eq!(ruler_span(0.0), 30.0);
        assert_eq!(ruler_span(26.0), 30.0);
        assert_eq!(ruler_span(30.0), 40.0);
        assert_eq!(ruler_span(f32::NAN), 30.0);
        assert_eq!(ruler_span(10_000.0), MAX_TIME);
    }

    #[test]
    fn time_and_position_map_both_ways() {
        let (span, left, width) = (40.0, 100.0, 800.0);
        assert_eq!(time_x(0.0, span, left, width), 100.0);
        assert_eq!(time_x(20.0, span, left, width), 500.0);
        assert_eq!(time_x(99.0, span, left, width), 900.0, "kept on the ruler");
        assert_eq!(x_time(500.0, span, left, width), 20.0);
        assert_eq!(x_time(0.0, span, left, width), 0.0);
        assert_eq!(x_time(5000.0, span, left, width), 40.0);
        for t in [0.0, 3.5, 17.25, 40.0] {
            assert!((x_time(time_x(t, span, left, width), span, left, width) - t).abs() < 1e-3);
        }
    }

    #[test]
    fn a_dragged_marker_snaps_and_stays_after_its_neighbour() {
        assert_eq!(drag_time(7.46, Some(3.0), 30.0), 7.5);
        assert!(
            (drag_time(2.0, Some(3.0), 30.0) - 3.1).abs() < 1e-5,
            "never on or before the one before"
        );
        assert_eq!(drag_time(-4.0, None, 30.0), 0.0);
        assert_eq!(drag_time(80.0, Some(3.0), 30.0), 30.0, "kept on the ruler");
    }

    #[test]
    fn the_ruler_labels_fewer_ticks_as_it_grows() {
        assert_eq!(tick_steps(30.0), (1.0, 5.0));
        assert_eq!(tick_steps(60.0), (5.0, 15.0));
        assert_eq!(tick_steps(200.0), (10.0, 60.0));
        assert_eq!(tick_steps(600.0), (30.0, 120.0));
    }

    #[test]
    fn the_clock_reads_minutes_seconds_and_a_tenth() {
        assert_eq!(time_text(0.0), "00:00.0");
        assert_eq!(time_text(7.5), "00:07.5");
        assert_eq!(time_text(59.96), "01:00.0", "rounded before it is split");
        assert_eq!(time_text(75.2), "01:15.2");
        assert_eq!(time_text(f32::NAN), "00:00.0");
    }

    #[test]
    fn the_selection_follows_a_deleted_waypoint() {
        assert_eq!(selection_after_delete(Some(2), 2), None);
        assert_eq!(selection_after_delete(Some(3), 1), Some(2));
        assert_eq!(selection_after_delete(Some(0), 1), Some(0));
        assert_eq!(selection_after_delete(None, 1), None);
    }
}
