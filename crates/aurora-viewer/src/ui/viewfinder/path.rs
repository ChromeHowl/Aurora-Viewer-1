//! The viewfinder's Path mode, the camera movie: the shutter places
//! waypoints on the Camera Path (`camera/path.rs`, the engine the Camera Path
//! window already runs), the dock plays, stops and records it, and a
//! timeline strip takes the place of the lens row to show and edit the
//! waypoints. This file is the mode's interface and its pure rules (the
//! timeline's time ↔ position mapping, what a drag may do, when the shutter
//! is greyed); the path itself, its saving and its playback stay in
//! `app/camera_tools.rs`, which `app/viewfinder_path.rs` reaches from here.
//! Aurora's own: the Legacy Viewer's snapshot floater has no such mode.

use super::sheets::sheet;
use super::{DOCK_SIDE, Frame, Out, Sheet, Viewfinder, bold, dock_face, icon, keep_keys, regular};
use crate::camera::path::{MAX_TIME, Transition, Waypoint};
use crate::theme::Palette;
use crate::ui::camera_path::{PathAction, transition_label};
use crate::ui::{menu, widgets};
use egui::{Align2, Color32, Id, Order, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};

/// The strip's height of track under its header (the ruler and the
/// markers), and the ruler's own.
const TRACK_H: f32 = 74.0;
const RULER_H: f32 = 18.0;
/// A marker's numbered pin.
const PIN: f32 = 20.0;
/// The strip's left edge on a wide window: clear of the camera pad.
const STRIP_LEFT: f32 = 330.0;

/// The shortest gap a drag leaves between two waypoints (seconds).
pub const MIN_GAP: f32 = 0.1;
/// The least time the ruler spans (seconds).
pub const MIN_SPAN: f32 = 30.0;

/// One saved path, as the sheet lists it.
pub struct SavedPath<'a> {
    pub name: &'a str,
    pub waypoints: usize,
    pub duration: f32,
}

/// The path in the world, put on the screen by the app: the curve in runs
/// (a run ends where the path leaves the view) and the waypoints in view.
#[derive(Default)]
pub struct WorldPath {
    pub curve: Vec<Vec<Pos2>>,
    pub markers: Vec<(usize, Pos2)>,
}

/// What the app gives the Path mode each frame: the path being edited and
/// played (`app/camera_tools.rs`) and the saved ones.
pub struct PathFrame<'a> {
    pub name: &'a str,
    pub waypoints: &'a [Waypoint],
    pub looped: bool,
    pub hold: bool,
    /// Playing, or holding the last shot.
    pub playing: bool,
    pub recording: bool,
    /// The playback's clock, seconds.
    pub elapsed: f32,
    pub selected: Option<usize>,
    /// The last thing told about the path (a refusal, a save).
    pub status: &'a str,
    pub saved: Vec<SavedPath<'a>>,
    pub world: WorldPath,
}

impl PathFrame<'_> {
    /// How long the path plays: the time of its last waypoint.
    pub fn duration(&self) -> f32 {
        self.waypoints.last().map_or(0.0, |w| w.time)
    }
}

/// A marker being dragged: which, the span the ruler keeps while it moves,
/// and how far the pointer was from the marker's centre.
#[derive(Debug, Clone, Copy)]
struct Drag {
    index: usize,
    span: f32,
    grab: f32,
}

/// The Path mode's own state in the viewfinder.
#[derive(Default)]
pub struct PathUi {
    /// The path and its waypoints are not drawn over the world.
    pub world_hidden: bool,
    drag: Option<Drag>,
    /// The name typed in the Path sheet, and the path's name it was last
    /// filled from.
    name: String,
    name_of: String,
}

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

/// How the camera travels from a waypoint to the next, drawn on the track
/// between their markers `x1` and `x2` around the line `y`: a soft arc
/// (Smooth), a straight line (Linear), an S from rest to rest (Ease). Cut
/// has no curve.
pub fn curve_points(x1: f32, x2: f32, transition: Transition, y: f32) -> Vec<Pos2> {
    const AMPLITUDE: f32 = 14.0;
    const STEPS: usize = 20;
    let w = x2 - x1;
    let (start, end) = (Pos2::new(x1, y + AMPLITUDE / 2.0), Pos2::new(x2, y - AMPLITUDE / 2.0));
    let (c1, c2) = match transition {
        Transition::Cut => return Vec::new(),
        Transition::Linear => return vec![start, end],
        Transition::Ease => (Pos2::new(x1 + w * 0.55, start.y), Pos2::new(x2 - w * 0.55, end.y)),
        Transition::Smooth => (Pos2::new(x1 + w * 0.4, y - AMPLITUDE), Pos2::new(x2 - w * 0.4, y + AMPLITUDE)),
    };
    (0..=STEPS)
        .map(|k| {
            let t = k as f32 / STEPS as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            Pos2::new(
                a * start.x + b * c1.x + c * c2.x + d * end.x,
                a * start.y + b * c1.y + c * c2.y + d * end.y,
            )
        })
        .collect()
}

/// The waypoint kept selected after waypoint `deleted` went.
pub fn selection_after_delete(selected: Option<usize>, deleted: usize) -> Option<usize> {
    match selected {
        Some(i) if i == deleted => None,
        Some(i) if i > deleted => Some(i - 1),
        other => other,
    }
}

/// The time the playhead stands at: the playback's clock while it plays,
/// else the selected waypoint's time (None: no waypoint selected).
fn head_time(path: &PathFrame) -> Option<f32> {
    if path.playing {
        Some(path.elapsed)
    } else {
        path.selected.and_then(|i| path.waypoints.get(i)).map(|w| w.time)
    }
}

/// The icon of a transition in the marker's menu.
fn transition_icon(t: Transition) -> &'static str {
    match t {
        Transition::Smooth => "wave-sine",
        Transition::Linear => "minus",
        Transition::Ease => "timeline",
        Transition::Cut => "scissors",
    }
}

/// A button of the Path group of the dock: a 44-point face like the dock's
/// others, greyed when it cannot act; true when clicked.
fn dock_tool(ui: &mut egui::Ui, p: &Palette, name: &str, tip: &str, on: bool, enabled: bool, tint: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(DOCK_SIDE), Sense::click());
    dock_face(
        ui.painter(),
        p,
        rect,
        resp.hovered() && enabled,
        on,
        if enabled { 1.0 } else { 0.5 },
    );
    let color = if enabled { tint } else { p.ink.gamma_multiply(0.45) };
    icon(ui.painter(), name, Rect::from_center_size(rect.center(), Vec2::splat(20.0)), color);
    keep_keys(&resp);
    resp.on_hover_text(tip).clicked() && enabled
}

impl Viewfinder {
    /// The dock's Path group, above the shutter: the Path sheet, the path
    /// in the world, record, play and stop. Play, stop and record live here
    /// only (no keys of their own).
    pub(super) fn path_dock(&mut self, ui: &mut egui::Ui, p: &Palette, f: &Frame, out: &mut Vec<Out>) {
        let path = &f.path;
        if dock_tool(
            ui,
            p,
            "route",
            aurora_i18n::tr!("vfp-dock-path-tip"),
            self.sheet == Some(Sheet::Path),
            true,
            p.ink,
        ) {
            self.toggle_sheet(Sheet::Path, out);
        }
        let hidden = self.path.world_hidden;
        let (name, tip) = if hidden {
            ("eye-off", aurora_i18n::tr!("vfp-dock-world-show-tip"))
        } else {
            ("eye-filled", aurora_i18n::tr!("vfp-dock-world-hide-tip"))
        };
        if dock_tool(ui, p, name, tip, !hidden, true, p.ink) {
            self.path.world_hidden = !hidden;
        }
        let tip = if path.playing {
            aurora_i18n::tr!("vfp-dock-busy-tip")
        } else if path.recording {
            aurora_i18n::tr!("vfp-dock-record-stop-tip")
        } else {
            aurora_i18n::tr!("camtools-path-record-tip")
        };
        if dock_tool(ui, p, "circle-filled", tip, path.recording, !path.playing, p.danger) {
            out.push(Out::Path(PathAction::Record(!path.recording)));
        }
        if path.playing {
            if dock_tool(
                ui,
                p,
                "player-stop-filled",
                aurora_i18n::tr!("camtools-path-stop"),
                true,
                true,
                p.ink,
            ) {
                out.push(Out::Path(PathAction::Stop));
            }
        } else {
            let has = !path.waypoints.is_empty();
            let tip = if has {
                aurora_i18n::tr!("vfp-dock-play-tip")
            } else {
                aurora_i18n::tr!("vfp-dock-play-empty-tip")
            };
            if dock_tool(ui, p, "player-play-filled", tip, false, has, p.ink) {
                out.push(Out::Path(PathAction::Play(None)));
            }
        }
    }

    /// The timeline strip, in the place of the lens row: its header (the
    /// path's name, the clock, Loop, Hold last shot) and the track with the
    /// waypoints' markers, the transitions between them and the playhead.
    pub(super) fn path_strip(&mut self, ctx: &egui::Context, p: &Palette, screen: Rect, f: &Frame, out: &mut Vec<Out>) {
        let dock_left = ctx
            .memory(|m| m.area_rect(Id::unique("vf_dock")))
            .map_or(screen.right() - 20.0 - 66.0, |r| r.left());
        let right = dock_left - 16.0;
        let left = (screen.left() + STRIP_LEFT).min(right - 360.0).max(screen.left() + 12.0);
        let width = (right - left).max(300.0);
        egui::Area::new(Id::unique("vf_path_strip"))
            .order(Order::Foreground)
            .fade_in(false)
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(Pos2::new(left, screen.bottom() - 14.0))
            .show(ctx, |ui| {
                widgets::overlay(ui, p, egui::Margin::symmetric(14, 8), |ui| {
                    ui.set_width(width - 28.0);
                    self.path_header(ctx, ui, p, f, out);
                    ui.add_space(4.0);
                    self.path_track(ctx, ui, p, f, out);
                });
            });
        if f.path.playing {
            ctx.request_repaint();
        }
    }

    fn path_header(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, p: &Palette, f: &Frame, out: &mut Vec<Out>) {
        let path = &f.path;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
            icon(ui.painter(), "movie", r, p.violet_light);
            ui.label(
                RichText::new(aurora_i18n::tr!("camtools-path-title"))
                    .font(bold(ctx, 13.0))
                    .color(p.ink),
            );
            let name = if path.name.trim().is_empty() {
                aurora_i18n::tr!("vfp-unnamed")
            } else {
                path.name
            };
            let resp = widgets::flat_button(ui, p, name).on_hover_text(aurora_i18n::tr!("vfp-name-tip"));
            if resp.clicked() {
                resp.surrender_focus();
                self.toggle_sheet(Sheet::Path, out);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let count = path.waypoints.len() as i64;
                ui.label(
                    RichText::new(aurora_i18n::tr!("vfp-count", count = count))
                        .size(11.0)
                        .color(p.muted),
                );
                let hold = crate::ui::snapshot::chip(ui, p, aurora_i18n::tr!("camtools-path-hold"), None, path.hold, true)
                    .on_hover_text(aurora_i18n::tr!("camtools-path-hold-tip"));
                if hold.clicked() {
                    hold.surrender_focus();
                    out.push(Out::Path(PathAction::Hold(!path.hold)));
                }
                let looped = crate::ui::snapshot::chip(ui, p, aurora_i18n::tr!("camtools-path-loop"), None, path.looped, true);
                if looped.clicked() {
                    looped.surrender_focus();
                    out.push(Out::Path(PathAction::Loop(!path.looped)));
                }
                let clock = format!("{} / {}", time_text(head_time(path).unwrap_or(0.0)), time_text(path.duration()));
                let color = if path.recording { p.danger } else { p.ink };
                ui.label(RichText::new(clock).monospace().size(13.0).color(color));
                if path.playing {
                    ui.label(RichText::new(aurora_i18n::tr!("vfp-state-playing")).size(11.0).color(p.success));
                } else if path.recording {
                    ui.label(RichText::new(aurora_i18n::tr!("vfp-state-recording")).size(11.0).color(p.danger));
                }
            });
        });
    }

    fn path_track(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, p: &Palette, f: &Frame, out: &mut Vec<Out>) {
        let path = &f.path;
        let (rect, bg) = ui.allocate_exact_size(Vec2::new(ui.available_width(), TRACK_H), Sense::click());
        if bg.clicked() && path.selected.is_some() {
            out.push(Out::PathSelect(None));
        }
        let inner = Rect::from_min_max(
            Pos2::new(rect.left() + PIN, rect.top()),
            Pos2::new(rect.right() - PIN, rect.bottom()),
        );
        // (a drag keeps the ruler it started on: the markers do not slide
        // under the pointer while the later ones follow)
        let span = self.path.drag.map_or_else(|| ruler_span(path.duration()), |d| d.span);
        let x_of = |t: f32| time_x(t, span, inner.left(), inner.width());
        let painter = ui.painter_at(rect);
        let radius = widgets::inner_radius(ui.visuals());
        let line_y = rect.top() + RULER_H + 32.0;

        // the ruler
        let (minor, major) = tick_steps(span);
        for k in 0..=((span / minor).round() as i32) {
            let t = k as f32 * minor;
            let big = ((t / major).round() * major - t).abs() < 1e-3;
            let x = x_of(t);
            let (top, color) = if big {
                (rect.top() + 11.0, p.muted)
            } else {
                (rect.top() + 13.0, p.muted_dim)
            };
            painter.line_segment(
                [Pos2::new(x, top), Pos2::new(x, rect.top() + RULER_H - 2.0)],
                Stroke::new(1.0, color),
            );
            if big {
                let seconds = format!("{}", t as i64);
                let label = aurora_i18n::tr!("camtools-path-clock", time = seconds.as_str());
                painter.text(Pos2::new(x, rect.top()), Align2::CENTER_TOP, label, regular(10.0), p.muted);
            }
        }
        // what lies past the last waypoint, and the line the curves follow
        let end_x = x_of(path.duration());
        if !path.waypoints.is_empty() && end_x < inner.right() {
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(end_x, rect.top() + RULER_H), Pos2::new(inner.right(), rect.bottom())),
                0.0,
                p.ink.gamma_multiply(0.04),
            );
        }
        painter.line_segment(
            [Pos2::new(inner.left(), line_y), Pos2::new(inner.right(), line_y)],
            Stroke::new(1.0, p.ink.gamma_multiply(0.12)),
        );
        // the transitions between the markers
        for pair in path.waypoints.windows(2) {
            let (x1, x2) = (x_of(pair[0].time), x_of(pair[1].time));
            match pair[0].transition {
                Transition::Cut => {
                    let mut x = x1 + 12.0;
                    while x < x2 - 12.0 {
                        painter.circle_filled(Pos2::new(x, line_y), 1.3, p.muted.gamma_multiply(0.6));
                        x += 7.0;
                    }
                    let mid = (x1 + x2) / 2.0;
                    for dx in [-3.0, 3.0] {
                        painter.line_segment(
                            [Pos2::new(mid + dx - 3.0, line_y - 6.0), Pos2::new(mid + dx + 3.0, line_y + 6.0)],
                            Stroke::new(2.0, p.danger),
                        );
                    }
                }
                t => {
                    painter.add(egui::Shape::line(
                        curve_points(x1, x2, t, line_y),
                        Stroke::new(3.0, p.violet_light.gamma_multiply(0.85)),
                    ));
                }
            }
            if x2 - x1 > 54.0 {
                painter.text(
                    Pos2::new((x1 + x2) / 2.0, line_y + 16.0),
                    Align2::CENTER_CENTER,
                    transition_label(pair[0].transition),
                    regular(9.5),
                    p.muted,
                );
            }
        }
        if path.waypoints.is_empty() {
            painter.text(
                Pos2::new(rect.center().x, line_y),
                Align2::CENTER_CENTER,
                aurora_i18n::tr!("vfp-empty"),
                regular(12.0),
                p.muted,
            );
        }
        // the markers
        let busy = path.playing;
        for (i, w) in path.waypoints.iter().enumerate() {
            let x = x_of(w.time);
            if x > rect.right() + PIN {
                continue;
            }
            let hit = Rect::from_min_max(Pos2::new(x - 12.0, rect.top() + RULER_H), Pos2::new(x + 12.0, rect.bottom()));
            let resp = ui.interact(hit, Id::unique("vf_path_marker").with(i), Sense::click_and_drag());
            let hot = resp.hovered() || resp.dragged();
            let selected = path.selected == Some(i);
            let pin = Rect::from_min_size(Pos2::new(x - PIN / 2.0, rect.top() + RULER_H), Vec2::splat(PIN));
            painter.rect_filled(pin, radius, if selected || hot { p.violet_light } else { p.violet });
            if selected {
                painter.rect_stroke(pin, radius, Stroke::new(2.0, p.teal), StrokeKind::Outside);
            }
            painter.text(
                pin.center(),
                Align2::CENTER_CENTER,
                format!("{}", i + 1),
                bold(ctx, 12.0),
                p.on_accent,
            );
            painter.line_segment(
                [Pos2::new(x, pin.bottom()), Pos2::new(x, line_y - 5.0)],
                Stroke::new(1.0, p.violet_light),
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x, line_y - 5.0),
                    Pos2::new(x + 5.0, line_y),
                    Pos2::new(x, line_y + 5.0),
                    Pos2::new(x - 5.0, line_y),
                ],
                if selected { p.teal } else { p.violet },
                Stroke::new(1.0, p.violet_pale),
            ));
            painter.text(
                Pos2::new(x, rect.bottom()),
                Align2::CENTER_BOTTOM,
                aurora_i18n::number(f64::from(w.time), 1),
                regular(10.0),
                p.muted,
            );
            // press selects; dragging retimes it (the later ones follow, as
            // in the Camera Path window), never under a playback
            if (resp.clicked() || resp.drag_started()) && !selected {
                out.push(Out::PathSelect(Some(i)));
            }
            if resp.drag_started()
                && !busy
                && let Some(at) = resp.interact_pointer_pos()
            {
                self.path.drag = Some(Drag {
                    index: i,
                    span,
                    grab: x - at.x,
                });
            }
            if resp.dragged()
                && !busy
                && let Some(drag) = self.path.drag.filter(|d| d.index == i)
                && let Some(at) = resp.interact_pointer_pos()
            {
                let wanted = x_time(at.x + drag.grab, drag.span, inner.left(), inner.width());
                let before = i.checked_sub(1).and_then(|k| path.waypoints.get(k)).map(|b| b.time);
                let time = drag_time(wanted, before, drag.span);
                if (time - w.time).abs() > 1e-4 {
                    out.push(Out::Path(PathAction::SetTime(i, time)));
                }
            }
            if resp.drag_stopped() {
                self.path.drag = None;
            }
            let resp = resp
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                .on_hover_text(aurora_i18n::tr!("vfp-marker-tip"));
            menu::context_menu(&resp, p, |ui| {
                let when = time_text(w.time);
                ui.label(
                    RichText::new(aurora_i18n::tr!("vfp-menu-title", number = (i + 1) as i64, time = when.as_str()))
                        .size(11.0)
                        .color(p.muted),
                );
                for t in Transition::ALL {
                    if menu::check(ui, p, transition_icon(t), transition_label(t), w.transition == t, !busy) {
                        out.push(Out::Path(PathAction::SetTransition(i, t)));
                    }
                }
                menu::separator(ui, p);
                if menu::item_if(ui, p, "focus-2", aurora_i18n::tr!("camtools-path-goto"), !busy && !path.recording) {
                    out.push(Out::Path(PathAction::GoTo(i)));
                }
                if menu::item_if(ui, p, "trash-filled", aurora_i18n::tr!("camtools-path-delete"), !busy) {
                    out.push(Out::Path(PathAction::Delete(i)));
                }
            });
        }
        if !ui.input(|i| i.pointer.any_down()) {
            self.path.drag = None;
        }
        // the playhead
        if let Some(t) = head_time(path) {
            let x = x_of(t.min(span));
            let color = if path.recording { p.danger } else { p.ink };
            painter.line_segment(
                [Pos2::new(x, rect.top() + RULER_H - 4.0), Pos2::new(x, rect.bottom())],
                Stroke::new(2.0, color),
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x - 5.0, rect.top() + RULER_H - 6.0),
                    Pos2::new(x + 5.0, rect.top() + RULER_H - 6.0),
                    Pos2::new(x, rect.top() + RULER_H),
                ],
                color,
                Stroke::NONE,
            ));
        }
    }

    /// The path in the world view: its curve and its waypoints' numbered
    /// pins, over the picture (the app puts them on the screen).
    pub(super) fn path_world(&self, painter: &egui::Painter, ctx: &egui::Context, p: &Palette, f: &Frame) {
        let world = &f.path.world;
        for run in world.curve.iter().filter(|r| r.len() >= 2) {
            painter.add(egui::Shape::line(run.clone(), Stroke::new(5.0, p.navy.gamma_multiply(0.5))));
            painter.add(egui::Shape::line(run.clone(), Stroke::new(2.5, p.violet_light)));
        }
        for (i, at) in &world.markers {
            let selected = f.path.selected == Some(*i);
            painter.circle_filled(*at, 10.0, if selected { p.violet_light } else { p.violet });
            painter.circle_stroke(
                *at,
                10.0,
                Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { p.teal } else { p.ink }),
            );
            painter.text(*at, Align2::CENTER_CENTER, format!("{}", i + 1), bold(ctx, 11.0), p.on_accent);
        }
    }

    /// The Path sheet: the path's name and Save, the saved paths, a new
    /// path, import and export.
    pub(super) fn path_sheet(&mut self, ctx: &egui::Context, p: &Palette, screen: Rect, f: &Frame, out: &mut Vec<Out>) {
        let path = &f.path;
        if self.path.name_of != path.name {
            self.path.name = path.name.to_owned();
            self.path.name_of = path.name.to_owned();
        }
        let name_field = &mut self.path.name;
        let open = sheet(
            ctx,
            p,
            screen,
            "vf_path_sheet",
            "route",
            aurora_i18n::tr!("camtools-path-title"),
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(name_field)
                            .hint_text(aurora_i18n::tr!("camtools-path-name"))
                            .char_limit(64)
                            .desired_width(230.0),
                    );
                    let named = !name_field.trim().is_empty() && !path.waypoints.is_empty();
                    if widgets::icon_button(ui, p, "device-floppy-filled", aurora_i18n::tr!("common-save"), named) {
                        out.push(Out::Path(PathAction::Save(name_field.trim().to_owned())));
                    }
                });
                if !path.status.is_empty() {
                    ui.label(RichText::new(path.status).size(11.0).color(p.muted));
                }
                ui.label(RichText::new(aurora_i18n::tr!("camtools-path-saved")).size(11.0).color(p.muted));
                if path.saved.is_empty() {
                    ui.label(RichText::new(aurora_i18n::tr!("vfp-sheet-none")).color(p.muted));
                }
                egui::ScrollArea::vertical()
                    .id_salt("vf_path_saved")
                    .max_height(190.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for saved in &path.saved {
                            ui.horizontal(|ui| {
                                let room = ui.available_width() - 28.0 - ui.spacing().item_spacing.x;
                                let (rect, resp) = ui.allocate_exact_size(Vec2::new(room, 36.0), Sense::click());
                                let radius = widgets::inner_radius(ui.visuals());
                                if saved.name == path.name {
                                    ui.painter().rect_filled(rect, radius, p.ink.gamma_multiply(0.14));
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(rect, radius, p.ink.gamma_multiply(0.07));
                                }
                                let clip = ui.painter_at(rect);
                                clip.text(
                                    Pos2::new(rect.left() + 8.0, rect.top() + 11.0),
                                    Align2::LEFT_CENTER,
                                    saved.name,
                                    bold(ui.ctx(), 13.0),
                                    p.ink,
                                );
                                let seconds = format!("{}", aurora_i18n::number(f64::from(saved.duration), 1));
                                let detail = aurora_i18n::tr!("vfp-sheet-detail", count = saved.waypoints as i64, time = seconds.as_str());
                                clip.text(
                                    Pos2::new(rect.left() + 8.0, rect.top() + 26.0),
                                    Align2::LEFT_CENTER,
                                    detail,
                                    regular(11.0),
                                    p.muted,
                                );
                                keep_keys(&resp);
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    out.push(Out::Path(PathAction::Load(saved.name.to_owned())));
                                }
                                if widgets::icon_button(ui, p, "trash-filled", aurora_i18n::tr!("camtools-path-delete-saved"), true) {
                                    out.push(Out::Path(PathAction::DeleteSaved(saved.name.to_owned())));
                                }
                            });
                        }
                    });
                ui.horizontal(|ui| {
                    if widgets::icon_button(ui, p, "plus", aurora_i18n::tr!("camtools-path-new"), !path.playing) {
                        out.push(Out::Path(PathAction::New));
                    }
                    if widgets::icon_button(ui, p, "file-import", aurora_i18n::tr!("camtools-path-import"), !path.playing) {
                        out.push(Out::Path(PathAction::Import));
                    }
                    let can_export = !path.waypoints.is_empty();
                    if widgets::icon_button(ui, p, "file-export", aurora_i18n::tr!("camtools-path-export"), can_export) {
                        out.push(Out::Path(PathAction::Export));
                    }
                });
            },
        );
        if !open {
            self.sheet = None;
        }
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
    fn each_transition_draws_from_one_marker_to_the_next() {
        for t in [Transition::Smooth, Transition::Ease] {
            let points = curve_points(100.0, 300.0, t, 50.0);
            assert_eq!(points.len(), 21, "{t:?}");
            assert!((points[0].x - 100.0).abs() < 1e-3 && (points[20].x - 300.0).abs() < 1e-3, "{t:?}");
            assert!(points.windows(2).all(|w| w[1].x >= w[0].x - 1e-3), "{t:?}: never goes back");
        }
        let line = curve_points(100.0, 300.0, Transition::Linear, 50.0);
        assert_eq!(line.len(), 2);
        assert!(line[1].y < line[0].y, "rises to the next waypoint");
        assert!(curve_points(100.0, 300.0, Transition::Cut, 50.0).is_empty(), "a cut has no curve");
    }

    #[test]
    fn the_selection_follows_a_deleted_waypoint() {
        assert_eq!(selection_after_delete(Some(2), 2), None);
        assert_eq!(selection_after_delete(Some(3), 1), Some(2));
        assert_eq!(selection_after_delete(Some(0), 1), Some(0));
        assert_eq!(selection_after_delete(None, 1), None);
    }
}
