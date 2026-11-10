//! The Camera Path window (World menu, Aurora's own): the waypoints of the
//! path being edited, its playback, the account's saved paths and the file
//! exchange. What its buttons do is `app/camera_tools.rs`; the path itself
//! `camera/path.rs`. Closing the window stops nothing.

use super::widgets::{self, ActionKind, Floater};
use crate::camera::path::{MAX_TIME, Path, Transition};
use crate::keybinds::{Action, KeyBindings};
use crate::theme::Palette;
use egui::{Pos2, Sense, Vec2};

pub const WINDOW: &str = "camera_path";
const ROW_H: f32 = 26.0;
const BUTTON_H: f32 = 26.0;

#[derive(Debug, Clone, PartialEq)]
pub enum PathAction {
    Capture,
    Record(bool),
    Delete(usize),
    /// The shot of a waypoint trades places with the one before (false) or
    /// after (true).
    Move(usize, bool),
    SetTime(usize, f32),
    SetTransition(usize, Transition),
    GoTo(usize),
    /// From the start, or from a waypoint.
    Play(Option<usize>),
    Stop,
    Loop(bool),
    Hold(bool),
    Save(String),
    Load(String),
    DeleteSaved(String),
    New,
    Import,
    Export,
}

/// What the window shows.
pub struct PathView<'a> {
    pub path: &'a Path,
    pub saved: Vec<&'a str>,
    /// Playing, or holding the last shot.
    pub playing: bool,
    pub recording: bool,
    /// The playback's clock, seconds.
    pub elapsed: f32,
    /// RLVa @showloc: the regions' names are not shown.
    pub hide_regions: bool,
    pub keys: &'a KeyBindings,
    /// The last thing to tell the user (why a play was refused…).
    pub status: &'a str,
}

#[derive(Default)]
pub struct CameraPathUi {
    pub selected: Option<usize>,
    name: String,
    /// The path's name when the field was last filled from it.
    name_of: String,
}

pub fn transition_label(t: Transition) -> &'static str {
    match t {
        Transition::Smooth => aurora_i18n::tr!("camtools-path-smooth"),
        Transition::Linear => aurora_i18n::tr!("camtools-path-linear"),
        Transition::Ease => aurora_i18n::tr!("camtools-path-ease"),
        Transition::Cut => aurora_i18n::tr!("camtools-path-cut"),
    }
}

fn key_of(keys: &KeyBindings, action: Action) -> Option<crate::keybinds::Binding> {
    keys.get(action).into_iter().flatten().next()
}

impl CameraPathUi {
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, open: &mut bool, view: &PathView) -> Vec<PathAction> {
        let mut out = Vec::new();
        if !*open {
            return out;
        }
        if self.name_of != view.path.name {
            self.name = view.path.name.clone();
            self.name_of = view.path.name.clone();
        }
        if self.selected.is_some_and(|i| i >= view.path.waypoints.len()) {
            self.selected = None;
        }
        let screen = ctx.content_rect();
        let pos = Pos2::new((screen.width() - 560.0).max(0.0) * 0.5, 120.0);
        let mut floater = Floater::new(WINDOW, aurora_i18n::tr!("camtools-path-title"), pos, Vec2::new(560.0, 440.0));
        floater.min_size = Vec2::new(460.0, 300.0);
        floater.show(ctx, p, open, |ui| {
            self.library_row(ui, p, view, &mut out);
            self.play_row(ui, p, view, &mut out);
            ui.horizontal_wrapped(|ui| {
                let (mut looped, mut hold) = (view.path.looped, view.path.hold);
                if widgets::checkbox(ui, p, &mut looped, aurora_i18n::tr!("camtools-path-loop")).changed() {
                    out.push(PathAction::Loop(looped));
                }
                if widgets::checkbox(ui, p, &mut hold, aurora_i18n::tr!("camtools-path-hold"))
                    .on_hover_text(aurora_i18n::tr!("camtools-path-hold-tip"))
                    .changed()
                {
                    out.push(PathAction::Hold(hold));
                }
                if !view.status.is_empty() {
                    ui.label(egui::RichText::new(view.status).color(p.muted).size(12.0));
                }
            });
            widgets::divider(ui, p);
            self.rows(ui, p, view, &mut out);
        });
        out
    }

    /// The path's name, Save, the saved paths, New, and the files.
    fn library_row(&mut self, ui: &mut egui::Ui, p: &Palette, view: &PathView, out: &mut Vec<PathAction>) {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.name)
                    .hint_text(aurora_i18n::tr!("camtools-path-name"))
                    .desired_width(150.0),
            );
            let named = !self.name.trim().is_empty() && !view.path.waypoints.is_empty();
            let size = Vec2::new(0.0, BUTTON_H);
            let save = aurora_i18n::tr!("common-save");
            if sized(ui, p, "device-floppy-filled", save, ActionKind::Flat, size, named).clicked() {
                out.push(PathAction::Save(self.name.trim().to_owned()));
            }
            let saved = sized(
                ui,
                p,
                "bookmark-filled",
                aurora_i18n::tr!("camtools-path-saved"),
                ActionKind::Flat,
                size,
                !view.saved.is_empty(),
            );
            egui::Popup::menu(&saved).show(|ui| {
                ui.set_min_width(200.0);
                for name in &view.saved {
                    ui.horizontal(|ui| {
                        let room = ui.available_width() - 20.0 - ui.spacing().item_spacing.x;
                        let used = ui.scope(|ui| {
                            ui.set_max_width(room);
                            super::menu::item(ui, p, "route", name)
                        });
                        if used.inner {
                            out.push(PathAction::Load((*name).to_owned()));
                        }
                        if widgets::icon_button(ui, p, "trash-filled", aurora_i18n::tr!("camtools-path-delete-saved"), true) {
                            out.push(PathAction::DeleteSaved((*name).to_owned()));
                        }
                    });
                }
            });
            if widgets::icon_button(ui, p, "plus", aurora_i18n::tr!("camtools-path-new"), true) {
                out.push(PathAction::New);
            }
            if widgets::icon_button(ui, p, "file-import", aurora_i18n::tr!("camtools-path-import"), !view.playing) {
                out.push(PathAction::Import);
            }
            let can_export = !view.path.waypoints.is_empty();
            if widgets::icon_button(ui, p, "file-export", aurora_i18n::tr!("camtools-path-export"), can_export) {
                out.push(PathAction::Export);
            }
        });
    }

    fn play_row(&mut self, ui: &mut egui::Ui, p: &Palette, view: &PathView, out: &mut Vec<PathAction>) {
        ui.horizontal_wrapped(|ui| {
            let size = Vec2::new(0.0, BUTTON_H);
            let has = !view.path.waypoints.is_empty();
            let play_key = key_of(view.keys, Action::PathPlayStop);
            if view.playing {
                let stop = aurora_i18n::tr!("camtools-path-stop");
                if keyed(
                    ui,
                    p,
                    "player-stop-filled",
                    stop,
                    ActionKind::Primary,
                    size,
                    true,
                    play_key.as_ref(),
                )
                .clicked()
                {
                    out.push(PathAction::Stop);
                }
                let clock = format!(
                    "{} / {}",
                    aurora_i18n::number(f64::from(view.elapsed), 1),
                    aurora_i18n::number(f64::from(view.path.duration()), 1)
                );
                ui.label(egui::RichText::new(aurora_i18n::tr!("camtools-path-clock", time = clock.as_str())).color(p.muted));
            } else {
                let play = aurora_i18n::tr!("camtools-path-play");
                if keyed(ui, p, "player-play-filled", play, ActionKind::Primary, size, has, play_key.as_ref()).clicked() {
                    out.push(PathAction::Play(None));
                }
                let from = aurora_i18n::tr!("camtools-path-play-selected");
                if sized(ui, p, "player-play-filled", from, ActionKind::Flat, size, self.selected.is_some()).clicked() {
                    out.push(PathAction::Play(self.selected));
                }
            }
            let capture_key = key_of(view.keys, Action::PathCapture);
            let capture = aurora_i18n::tr!("camtools-path-capture");
            if keyed(
                ui,
                p,
                "camera-filled",
                capture,
                ActionKind::Flat,
                size,
                !view.playing,
                capture_key.as_ref(),
            )
            .on_hover_text(aurora_i18n::tr!("camtools-path-capture-tip"))
            .clicked()
            {
                out.push(PathAction::Capture);
            }
            let kind = if view.recording { ActionKind::Danger } else { ActionKind::Flat };
            let record = aurora_i18n::tr!("camtools-path-record");
            if sized(ui, p, "circle-filled", record, kind, size, !view.playing)
                .on_hover_text(aurora_i18n::tr!("camtools-path-record-tip"))
                .clicked()
            {
                out.push(PathAction::Record(!view.recording));
            }
        });
    }

    fn rows(&mut self, ui: &mut egui::Ui, p: &Palette, view: &PathView, out: &mut Vec<PathAction>) {
        if view.path.waypoints.is_empty() {
            ui.label(egui::RichText::new(aurora_i18n::tr!("camtools-path-empty")).color(p.muted));
            return;
        }
        let count = view.path.waypoints.len();
        egui::ScrollArea::vertical()
            .id_salt("camera_path_rows")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, w) in view.path.waypoints.iter().enumerate() {
                    let selected = self.selected == Some(i);
                    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
                    if selected || response.hovered() {
                        let fill = if selected { p.violet.gamma_multiply(0.28) } else { p.raised };
                        ui.painter().rect_filled(rect, ui.visuals().widgets.inactive.corner_radius, fill);
                    }
                    if response.clicked() {
                        self.selected = Some(i);
                    }
                    if response.double_clicked() {
                        out.push(PathAction::GoTo(i));
                    }
                    let mut row = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect.shrink2(Vec2::new(4.0, 0.0)))
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    let ui = &mut row;
                    ui.add_enabled_ui(!view.playing, |ui| {
                        ui.label(egui::RichText::new(format!("{}", i + 1)).color(p.muted).monospace());
                        let mut time = w.time;
                        let field = egui::DragValue::new(&mut time)
                            .range(0.0..=MAX_TIME)
                            .speed(0.05)
                            .fixed_decimals(2)
                            .suffix(" s");
                        if ui
                            .add_sized([72.0, 20.0], field)
                            .on_hover_text(aurora_i18n::tr!("camtools-path-time-tip"))
                            .changed()
                        {
                            out.push(PathAction::SetTime(i, time));
                        }
                        // the last waypoint leads nowhere, unless nothing: its
                        // transition is kept for when another follows
                        egui::ComboBox::from_id_salt(("camera_path_transition", i))
                            .width(96.0)
                            .selected_text(transition_label(w.transition))
                            .show_ui(ui, |ui| {
                                for t in Transition::ALL {
                                    if ui.selectable_label(w.transition == t, transition_label(t)).clicked() {
                                        out.push(PathAction::SetTransition(i, t));
                                    }
                                }
                            })
                            .response
                            .on_hover_text(aurora_i18n::tr!("camtools-path-transition-tip"));
                        if widgets::icon_button(ui, p, "focus-2", aurora_i18n::tr!("camtools-path-goto"), true) {
                            self.selected = Some(i);
                            out.push(PathAction::GoTo(i));
                        }
                        if widgets::icon_button(ui, p, "arrow-up", aurora_i18n::tr!("camtools-path-up"), i > 0) {
                            self.selected = Some(i - 1);
                            out.push(PathAction::Move(i, false));
                        }
                        if widgets::icon_button(ui, p, "arrow-down", aurora_i18n::tr!("camtools-path-down"), i + 1 < count) {
                            self.selected = Some(i + 1);
                            out.push(PathAction::Move(i, true));
                        }
                        if widgets::icon_button(ui, p, "trash-filled", aurora_i18n::tr!("camtools-path-delete"), true) {
                            out.push(PathAction::Delete(i));
                        }
                        let place = if view.hide_regions || w.region.is_empty() {
                            format!("{:.0}, {:.0}, {:.0}", w.pos[0], w.pos[1], w.pos[2])
                        } else {
                            format!("{} ({:.0}, {:.0}, {:.0})", w.region, w.pos[0], w.pos[1], w.pos[2])
                        };
                        let detail = format!("{place} · {:.0}°", w.fov.to_degrees());
                        ui.add(egui::Label::new(egui::RichText::new(detail).color(p.ink).size(12.0)).truncate());
                    });
                }
            });
    }
}

fn sized(ui: &mut egui::Ui, p: &Palette, icon: &str, label: &str, kind: ActionKind, size: Vec2, enabled: bool) -> egui::Response {
    keyed(ui, p, icon, label, kind, size, enabled, None)
}

/// An action button as wide as its content.
#[allow(clippy::too_many_arguments)]
fn keyed(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: &str,
    label: &str,
    kind: ActionKind,
    size: Vec2,
    enabled: bool,
    key: Option<&crate::keybinds::Binding>,
) -> egui::Response {
    let width = widgets::action_button_keyed_content(ui, icon, label, key) + 24.0;
    widgets::action_button_keyed(ui, p, icon, label, kind, Vec2::new(width, size.y), enabled, key)
}
