//! The viewfinder's Camera extras and Photo-only quality sheets (on the
//! right, like the guide's and the lens options').
//!
//! Camera extras: the render extras of Preferences › Graphics a photographer
//! reaches for, as sliders with camera names: the scene's lights and their
//! strength (lamp power), the sun's shadow detail, the ground textures'
//! size and the debanding. They are Preferences' own global settings,
//! shown here too (`ExtraEdit`, applied by `app::viewfinder`); the heavy ones
//! (mirrors, probes, shadow quality) stay in Preferences, and the colour
//! grade is the Style sheet's (photo only).
//!
//! Photo-only quality: how much of the "Photo quality" boost a photo takes
//! (`snapshot::tune`: anti-aliasing, shadows, draw distance) and the
//! window's render scale (supersampling). All of it only applies while the
//! photo window is open and the user's own values come back with it.
//! Aurora's own; the Legacy Viewer has neither.

use super::sheets::{mark_row, row_slot, sheet};
use super::{Frame, Out, Viewfinder};
use crate::settings::render_extras::{LIGHT_STRENGTH_RANGE, RenderExtras, SHADOW_SCALE_RANGE, TERRAIN_SCALE_RANGE};
use crate::snapshot::Settings;
use crate::snapshot::quality::Supersampling;
use crate::snapshot::tune::{Antialiasing, Reach, Shadows};
use crate::theme::Palette;
use crate::ui::widgets;
use egui::{Align2, FontId, Rect, Sense, Vec2};
use std::ops::RangeInclusive;

/// The debanding's range (8-bit levels).
pub const DEBAND_RANGE: RangeInclusive<f32> = 0.0..=2.0;

/// One arrow-key step of each slider.
const LAMP_STEP: f32 = 0.25;
const SHADOW_STEP: f32 = 0.25;
const GROUND_STEP: f32 = 1.0;
const DEBAND_STEP: f32 = 0.25;

/// The width the sliders' labels take.
const LABEL_WIDTH: f32 = 112.0;

/// The rows of the Camera extras sheet, as the arrow keys go through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtraRow {
    Lights,
    Lamp,
    Shadow,
    Ground,
    Deband,
}

pub const EXTRA_ROWS: [ExtraRow; 5] = [
    ExtraRow::Lights,
    ExtraRow::Lamp,
    ExtraRow::Shadow,
    ExtraRow::Ground,
    ExtraRow::Deband,
];

/// Preferences' render extras as the sheet shows them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtrasView {
    pub scene_lights: bool,
    pub light_strength: f32,
    pub shadow_scale: f32,
    pub terrain_scale: f32,
    pub deband: f32,
}

impl ExtrasView {
    pub fn of(e: &RenderExtras) -> ExtrasView {
        ExtrasView {
            scene_lights: e.scene_lights,
            light_strength: e.light_strength,
            shadow_scale: e.shadow_resolution_scale,
            terrain_scale: e.terrain_scale,
            deband: e.deband,
        }
    }
}

/// What the Camera extras sheet asks of the app (Preferences' global
/// settings).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtraEdit {
    Lights(bool),
    Lamp(f32),
    Shadow(f32),
    Ground(f32),
    Deband(f32),
}

impl ExtraEdit {
    /// Write the edit in Preferences' settings, within their ranges (a
    /// value that is not a number is left out).
    pub fn apply(self, e: &mut RenderExtras) {
        let within = |v: f32, r: RangeInclusive<f32>| v.is_finite().then(|| v.clamp(*r.start(), *r.end()));
        match self {
            ExtraEdit::Lights(on) => e.scene_lights = on,
            ExtraEdit::Lamp(v) => e.light_strength = within(v, LIGHT_STRENGTH_RANGE).unwrap_or(e.light_strength),
            ExtraEdit::Shadow(v) => e.shadow_resolution_scale = within(v, SHADOW_SCALE_RANGE).unwrap_or(e.shadow_resolution_scale),
            ExtraEdit::Ground(v) => e.terrain_scale = within(v, TERRAIN_SCALE_RANGE).unwrap_or(e.terrain_scale),
            ExtraEdit::Deband(v) => e.deband = within(v, DEBAND_RANGE).unwrap_or(e.deband),
        }
    }
}

/// `v` moved `turn` steps on the grid of `step`, within `range`.
pub fn nudge(v: f32, turn: i32, step: f32, range: RangeInclusive<f32>) -> f32 {
    (((v / step).round() + turn as f32) * step).clamp(*range.start(), *range.end())
}

/// The edit an arrow key makes on `row` (a turn of the switch toggles it).
pub fn key_edit(row: ExtraRow, turn: i32, e: &ExtrasView) -> ExtraEdit {
    match row {
        ExtraRow::Lights => ExtraEdit::Lights(!e.scene_lights),
        ExtraRow::Lamp => ExtraEdit::Lamp(nudge(e.light_strength, turn, LAMP_STEP, LIGHT_STRENGTH_RANGE)),
        ExtraRow::Shadow => ExtraEdit::Shadow(nudge(e.shadow_scale, turn, SHADOW_STEP, SHADOW_SCALE_RANGE)),
        ExtraRow::Ground => ExtraEdit::Ground(nudge(e.terrain_scale, turn, GROUND_STEP, TERRAIN_SCALE_RANGE)),
        ExtraRow::Deband => ExtraEdit::Deband(nudge(e.deband, turn, DEBAND_STEP, DEBAND_RANGE)),
    }
}

/// The choice `turn` places after `now` in `all`, round (a value not in the
/// list: the first).
pub fn cycle<T: Copy + PartialEq>(all: &[T], now: T, turn: i32) -> T {
    let at = all.iter().position(|c| *c == now).map_or(0, |i| i as i32 + turn);
    all[at.rem_euclid(all.len() as i32) as usize]
}

/// A slider with its label, on one line. `Some(commit)` when the value
/// changed or was just let go: `commit` is true once it is final (a click,
/// the end of a drag, the end of an edit), so that the settings are written
/// to disk once, not at every frame of a drag.
#[allow(clippy::too_many_arguments)]
fn slider_row(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    v: &mut f32,
    range: RangeInclusive<f32>,
    step: f64,
    suffix: &str,
    lit: bool,
) -> Option<bool> {
    let mark = row_slot(ui);
    let line = ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(LABEL_WIDTH, 20.0), Sense::hover());
        ui.painter()
            .text(rect.left_center(), Align2::LEFT_CENTER, label, FontId::proportional(12.0), p.ink);
        ui.spacing_mut().slider_width = 104.0;
        ui.add(widgets::restricted_number(
            egui::Slider::new(v, range)
                .step_by(step)
                .max_decimals(2)
                .suffix(suffix)
                .custom_parser(aurora_i18n::parse_number)
                .clamping(egui::SliderClamping::Edits)
                .trailing_fill(true),
            widgets::TextPolicy::Decimal,
        ))
    });
    mark_row(ui, p, mark, line.response.rect, lit);
    let r = line.inner;
    let done = r.drag_stopped() || r.lost_focus();
    (r.changed() || done).then_some(done || (r.changed() && !r.dragged() && !r.has_focus()))
}

/// A label and the choices under it, side by side: the one made (an index
/// of `names`) is lit. Some(i) when another is clicked.
fn choice_row(ui: &mut egui::Ui, p: &Palette, label: &str, names: &[String], now: usize, lit: bool) -> Option<usize> {
    let mark = row_slot(ui);
    let top = ui.cursor().top();
    ui.label(egui::RichText::new(label).color(p.ink));
    let mut picked = None;
    let line = ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, name) in names.iter().enumerate() {
            if crate::ui::snapshot::chip(ui, p, name, None, i == now, true).clicked() {
                picked = Some(i);
            }
        }
    });
    mark_row(ui, p, mark, line.response.rect.with_min_y(top), lit);
    picked
}

/// The sheet's rows of the photo-only choices, in the order of the arrows.
const QUALITY_ROWS: usize = 4;

fn own() -> String {
    aurora_i18n::tr!("vfx-own").to_owned()
}

/// What each anti-aliasing, shadow, reach and render scale choice is called.
fn aa_names() -> Vec<String> {
    vec![
        aurora_i18n::tr!("vfx-aa-max").to_owned(),
        aurora_i18n::tr!("vfx-aa-4").to_owned(),
        aurora_i18n::tr!("vfx-aa-2").to_owned(),
        own(),
    ]
}

fn shadow_names() -> Vec<String> {
    vec![
        aurora_i18n::tr!("vfx-shadows-max").to_owned(),
        aurora_i18n::tr!("vfx-shadows-high").to_owned(),
        own(),
    ]
}

fn reach_names() -> Vec<String> {
    vec![
        aurora_i18n::tr!("vfx-reach-far").to_owned(),
        aurora_i18n::tr!("vfx-reach-mid").to_owned(),
        own(),
    ]
}

fn scale_names() -> Vec<String> {
    Supersampling::ALL
        .into_iter()
        .map(|c| match c {
            Supersampling::Off => aurora_i18n::tr!("p16-supersampling-off").to_owned(),
            c => aurora_i18n::tr!("p16-supersampling-samples", count = c.samples()).to_owned(),
        })
        .collect()
}

fn index_of<T: PartialEq>(all: &[T], now: T) -> usize {
    all.iter().position(|c| *c == now).unwrap_or(0)
}

impl Viewfinder {
    /// The render extras as sliders: scene lights, lamp power, shadow
    /// detail, ground texture size, debanding.
    pub(super) fn extras_sheet(&mut self, ctx: &egui::Context, p: &Palette, screen: Rect, f: &Frame, out: &mut Vec<Out>) {
        let e = f.extras;
        let (row, turn) = self.sheet_nav(EXTRA_ROWS.len());
        let row = EXTRA_ROWS[row.min(EXTRA_ROWS.len() - 1)];
        if turn != 0 {
            out.push(Out::Extras(key_edit(row, turn, &e), true));
        }
        let open = sheet(
            ctx,
            p,
            screen,
            "vf_extras_sheet",
            "adjustments-horizontal-filled",
            aurora_i18n::tr!("vfx-extras-title"),
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let mut on = e.scene_lights;
                let mark = row_slot(ui);
                let r = widgets::switch_labeled(ui, p, &mut on, aurora_i18n::tr!("vfx-lights"));
                if r.changed() {
                    out.push(Out::Extras(ExtraEdit::Lights(on), true));
                }
                mark_row(ui, p, mark, r.rect, row == ExtraRow::Lights);
                let mut v = e.light_strength;
                if let Some(commit) = slider_row(
                    ui,
                    p,
                    aurora_i18n::tr!("vfx-lamp"),
                    &mut v,
                    LIGHT_STRENGTH_RANGE,
                    0.05,
                    "",
                    row == ExtraRow::Lamp,
                ) {
                    out.push(Out::Extras(ExtraEdit::Lamp(v), commit));
                }
                let mut v = e.shadow_scale;
                if let Some(commit) = slider_row(
                    ui,
                    p,
                    aurora_i18n::tr!("vfx-shadow"),
                    &mut v,
                    SHADOW_SCALE_RANGE,
                    0.1,
                    "×",
                    row == ExtraRow::Shadow,
                ) {
                    out.push(Out::Extras(ExtraEdit::Shadow(v), commit));
                }
                let mut v = e.terrain_scale;
                if let Some(commit) = slider_row(
                    ui,
                    p,
                    aurora_i18n::tr!("vfx-ground"),
                    &mut v,
                    TERRAIN_SCALE_RANGE,
                    1.0,
                    " m",
                    row == ExtraRow::Ground,
                ) {
                    out.push(Out::Extras(ExtraEdit::Ground(v), commit));
                }
                let mut v = e.deband;
                if let Some(commit) = slider_row(
                    ui,
                    p,
                    aurora_i18n::tr!("vfx-deband"),
                    &mut v,
                    DEBAND_RANGE,
                    0.05,
                    "",
                    row == ExtraRow::Deband,
                ) {
                    out.push(Out::Extras(ExtraEdit::Deband(v), commit));
                }
                ui.label(egui::RichText::new(aurora_i18n::tr!("pcu-lens-global")).size(11.0).color(p.muted));
                ui.label(egui::RichText::new(aurora_i18n::tr!("vfx-heavy")).size(11.0).color(p.muted));
            },
        );
        if !open {
            self.sheet = None;
        }
    }

    /// The photo-only choices: anti-aliasing, shadows and draw distance of
    /// the boost, and the render scale.
    pub(super) fn quality_sheet(&mut self, ctx: &egui::Context, p: &Palette, screen: Rect, s: &mut Settings) {
        let (row, turn) = self.sheet_nav(QUALITY_ROWS);
        if turn != 0 {
            match row {
                0 => s.quality_tune.antialiasing = cycle(&Antialiasing::ALL, s.quality_tune.antialiasing, turn),
                1 => s.quality_tune.shadows = cycle(&Shadows::ALL, s.quality_tune.shadows, turn),
                2 => s.quality_tune.reach = cycle(&Reach::ALL, s.quality_tune.reach, turn),
                _ => s.supersampling = cycle(&Supersampling::ALL, s.supersampling, turn),
            }
        }
        let boost_on = s.quality;
        let open = sheet(
            ctx,
            p,
            screen,
            "vf_quality_sheet",
            "sparkles-filled",
            aurora_i18n::tr!("vfx-quality-title"),
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(
                    egui::RichText::new(if boost_on {
                        aurora_i18n::tr!("vfx-quality-on")
                    } else {
                        aurora_i18n::tr!("vfx-quality-off")
                    })
                    .size(11.0)
                    .color(if boost_on { p.muted } else { p.amber }),
                );
                let t = &mut s.quality_tune;
                if let Some(i) = choice_row(
                    ui,
                    p,
                    aurora_i18n::tr!("vfx-aa"),
                    &aa_names(),
                    index_of(&Antialiasing::ALL, t.antialiasing),
                    row == 0,
                ) {
                    t.antialiasing = Antialiasing::ALL[i];
                }
                if let Some(i) = choice_row(
                    ui,
                    p,
                    aurora_i18n::tr!("vfx-shadows"),
                    &shadow_names(),
                    index_of(&Shadows::ALL, t.shadows),
                    row == 1,
                ) {
                    t.shadows = Shadows::ALL[i];
                }
                if let Some(i) = choice_row(
                    ui,
                    p,
                    aurora_i18n::tr!("vfx-reach"),
                    &reach_names(),
                    index_of(&Reach::ALL, t.reach),
                    row == 2,
                ) {
                    t.reach = Reach::ALL[i];
                }
                if let Some(i) = choice_row(
                    ui,
                    p,
                    aurora_i18n::tr!("vfx-scale"),
                    &scale_names(),
                    index_of(&Supersampling::ALL, s.supersampling),
                    row == 3,
                ) {
                    s.supersampling = Supersampling::ALL[i];
                }
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

    fn view() -> ExtrasView {
        ExtrasView::of(&RenderExtras::default())
    }

    #[test]
    fn the_view_reads_preferences() {
        let v = view();
        assert!(v.scene_lights);
        assert_eq!((v.light_strength, v.shadow_scale, v.terrain_scale, v.deband), (1.0, 1.0, 12.0, 0.0));
    }

    #[test]
    fn nudging_steps_on_the_grid_and_stops_at_the_ends() {
        assert_eq!(nudge(1.0, 1, 0.25, LIGHT_STRENGTH_RANGE), 1.25);
        assert_eq!(nudge(1.1, 1, 0.25, LIGHT_STRENGTH_RANGE), 1.25, "a value off the grid lands on it");
        assert_eq!(nudge(0.0, -1, 0.25, LIGHT_STRENGTH_RANGE), 0.0);
        assert_eq!(nudge(4.0, 1, 0.25, LIGHT_STRENGTH_RANGE), 4.0);
        assert_eq!(nudge(12.0, -1, 1.0, TERRAIN_SCALE_RANGE), 11.0);
        assert_eq!(nudge(1.0, -1, 1.0, TERRAIN_SCALE_RANGE), 1.0);
    }

    #[test]
    fn a_key_turn_makes_the_rows_edit() {
        let v = view();
        assert_eq!(key_edit(ExtraRow::Lights, 1, &v), ExtraEdit::Lights(false));
        assert_eq!(key_edit(ExtraRow::Lamp, 1, &v), ExtraEdit::Lamp(1.25));
        assert_eq!(key_edit(ExtraRow::Shadow, -1, &v), ExtraEdit::Shadow(1.0), "1x is the smallest");
        assert_eq!(key_edit(ExtraRow::Ground, -1, &v), ExtraEdit::Ground(11.0));
        assert_eq!(key_edit(ExtraRow::Deband, 1, &v), ExtraEdit::Deband(0.25));
    }

    #[test]
    fn an_edit_lands_in_preferences_within_their_ranges() {
        let mut e = RenderExtras::default();
        ExtraEdit::Lamp(9.0).apply(&mut e);
        assert_eq!(e.light_strength, *LIGHT_STRENGTH_RANGE.end());
        ExtraEdit::Shadow(0.1).apply(&mut e);
        assert_eq!(e.shadow_resolution_scale, *SHADOW_SCALE_RANGE.start());
        ExtraEdit::Ground(f32::NAN).apply(&mut e);
        assert_eq!(e.terrain_scale, 12.0, "not a number: left as it was");
        ExtraEdit::Deband(1.5).apply(&mut e);
        ExtraEdit::Lights(false).apply(&mut e);
        assert_eq!((e.deband, e.scene_lights), (1.5, false));
        // the other settings stay
        assert_eq!(e.mirror_update_rate, RenderExtras::default().mirror_update_rate);
    }

    #[test]
    fn choices_cycle_both_ways_and_a_stranger_starts_over() {
        assert_eq!(cycle(&Antialiasing::ALL, Antialiasing::Max, 1), Antialiasing::X4);
        assert_eq!(cycle(&Antialiasing::ALL, Antialiasing::Max, -1), Antialiasing::Own);
        assert_eq!(cycle(&Reach::ALL, Reach::Own, 1), Reach::Far);
        assert_eq!(cycle(&[1, 2, 3], 9, 1), 1, "not in the list: the first");
    }

    #[test]
    fn every_choice_has_a_name() {
        assert_eq!(aa_names().len(), Antialiasing::ALL.len());
        assert_eq!(shadow_names().len(), Shadows::ALL.len());
        assert_eq!(reach_names().len(), Reach::ALL.len());
        assert_eq!(scale_names().len(), Supersampling::ALL.len());
    }
}
