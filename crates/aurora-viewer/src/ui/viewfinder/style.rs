//! The photo's white balance and picture style in the viewfinder: the
//! white balance's line in the light column and its presets over the ring
//! deck. The overlay they change is `snapshot::style`, for the photo mode
//! only.

use super::{Frame, Viewfinder, controls};
use crate::snapshot::style::{self, Wb};
use crate::theme::Palette;
use crate::ui::snapshot::SnapshotUi;
use egui::{Id, Order, Pos2, Rect};

/// A step count with its sign: "+3", "−2", "0".
pub(super) fn signed(n: i32) -> String {
    match n.signum() {
        1 => format!("+{n}"),
        -1 => format!("−{}", -n),
        _ => "0".to_owned(),
    }
}

pub(super) fn wb_label(wb: Wb) -> &'static str {
    match wb {
        Wb::Auto => aurora_i18n::tr!("pcu-wb-auto"),
        Wb::Daylight => aurora_i18n::tr!("pcu-wb-daylight"),
        Wb::Cloudy => aurora_i18n::tr!("pcu-wb-cloudy"),
        Wb::Shade => aurora_i18n::tr!("pcu-wb-shade"),
        Wb::Tungsten => aurora_i18n::tr!("pcu-wb-tungsten"),
        Wb::Fluorescent => aurora_i18n::tr!("pcu-wb-fluorescent"),
    }
}

fn wb_tip(wb: Wb) -> &'static str {
    match wb {
        Wb::Auto => aurora_i18n::tr!("pcu-wb-auto-tip"),
        Wb::Daylight => aurora_i18n::tr!("pcu-wb-daylight-tip"),
        Wb::Cloudy => aurora_i18n::tr!("pcu-wb-cloudy-tip"),
        Wb::Shade => aurora_i18n::tr!("pcu-wb-shade-tip"),
        Wb::Tungsten => aurora_i18n::tr!("pcu-wb-tungsten-tip"),
        Wb::Fluorescent => aurora_i18n::tr!("pcu-wb-fluorescent-tip"),
    }
}

/// The white balance's line: its preset, else its steps ("WB +3 · G−1";
/// green less with a positive tint).
pub(super) fn wb_text(light: &crate::snapshot::light::PhotoLight, prefs: &crate::settings::render_extras::Grade) -> String {
    if let Some(wb) = style::wb_of(light, prefs) {
        return wb_label(wb).to_owned();
    }
    let g = style::grade(light, prefs);
    let (t, u) = (style::steps(g.temperature), style::steps(g.tint));
    if u == 0 {
        format!("WB {}", signed(t))
    } else {
        format!("WB {} · G{}", signed(t), signed(-u))
    }
}

/// The rows of the preset chips (`widths`, in points) between `left` and
/// `right`: one row when it fits, else as many as needed, each ending at
/// `right` (a chip never goes left of `left`, into the camera pad). The
/// chips of each row, by index.
pub(super) fn wb_rows(widths: &[f32], left: f32, right: f32) -> Vec<std::ops::Range<usize>> {
    let room = (right - left).max(0.0);
    let mut rows = Vec::new();
    let mut start = 0;
    let mut w = 4.0;
    for (i, cw) in widths.iter().enumerate() {
        let add = cw + if i > start { 2.0 } else { 0.0 };
        if i > start && w + add > room {
            rows.push(start..i);
            start = i;
            w = 4.0 + cw;
        } else {
            w += add;
        }
    }
    if start < widths.len() {
        rows.push(start..widths.len());
    }
    rows
}

impl Viewfinder {
    /// The white balance's presets, over the ring deck's title line and
    /// ending under the ruler's right end; wrapped onto more rows (growing
    /// up) when the text is too wide for the room left of the camera pad.
    pub(super) fn wb_presets(&mut self, ctx: &egui::Context, p: &Palette, screen: Rect, panel: &mut SnapshotUi, f: &Frame) {
        // the deck: the ruler and its two buttons, centred
        let deck_right = screen.center().x + controls::DECK_WIDTH / 2.0;
        let right = deck_right - 42.0;
        let pad = ctx
            .memory(|m| m.area_rect(Id::unique("vf_pad")))
            .filter(|_| self.pad_shown)
            .map_or(screen.left() + super::GAP, |r| r.right() + 12.0);
        let now = style::wb_of(&panel.light, &f.prefs_grade);
        let font = super::bold(ctx, 12.0);
        // measured as `segmented` draws them: the font and text size chosen
        let measure = ctx.layer_painter(egui::LayerId::background());
        let widths: Vec<f32> = Wb::ALL
            .iter()
            .map(|w| measure.layout_no_wrap(wb_label(*w).to_owned(), font.clone(), p.ink).size().x + 18.0)
            .collect();
        let rows = wb_rows(&widths, pad, right);
        // the lowest row just over the deck's title (bottom − 134)
        let mut bottom = screen.bottom() - 134.0 - 4.0;
        let mut chosen = None;
        for (k, range) in rows.iter().enumerate().rev() {
            let items: Vec<(&str, bool, &str)> = Wb::ALL[range.clone()]
                .iter()
                .map(|w| (wb_label(*w), now == Some(*w), wb_tip(*w)))
                .collect();
            egui::Area::new(Id::unique(("vf_wb_presets", k)))
                .order(Order::Foreground)
                .fade_in(false)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(Pos2::new(right, bottom))
                .show(ctx, |ui| {
                    if let Some(i) = controls::segmented(ui, p, &items, None) {
                        chosen = Some(Wb::ALL[range.start + i]);
                    }
                });
            bottom -= 30.0;
        }
        self.wb_row_top = Some(bottom + 30.0 - 26.0);
        if let Some(wb) = chosen {
            style::set_wb(&mut panel.light, &f.prefs_grade, wb);
        }
    }
}

pub(super) fn style_label(st: style::Style) -> &'static str {
    use style::Style;
    match st {
        Style::AsViewer => aurora_i18n::tr!("pcu-style-viewer"),
        Style::Standard => aurora_i18n::tr!("pcu-style-standard"),
        Style::Neutral => aurora_i18n::tr!("pcu-style-neutral"),
        Style::Soft => aurora_i18n::tr!("pcu-style-soft"),
        Style::Classic => aurora_i18n::tr!("pcu-style-classic"),
        Style::Natural => aurora_i18n::tr!("pcu-style-natural"),
        Style::Film => aurora_i18n::tr!("pcu-style-film"),
        Style::Vivid => aurora_i18n::tr!("pcu-style-vivid"),
        Style::Custom => aurora_i18n::tr!("pcu-style-custom"),
    }
}

fn fine_label(f: style::Fine) -> &'static str {
    match f {
        style::Fine::Shadows => aurora_i18n::tr!("pcu-fine-shadows"),
        style::Fine::Midtones => aurora_i18n::tr!("pcu-fine-midtones"),
        style::Fine::Highlights => aurora_i18n::tr!("pcu-fine-highlights"),
        style::Fine::Colour => aurora_i18n::tr!("pcu-fine-colour"),
    }
}

/// What the photo's style rows are given: Preferences' grade, and whether
/// tone mapping applies (HDR on and a sky that is tone mapped), else why
/// the curves do nothing.
pub struct StyleContext<'a> {
    pub prefs_grade: &'a crate::settings::render_extras::Grade,
    pub tone_ok: Result<(), &'a str>,
}

/// A row of a value in steps with − and +; the step asked.
fn stepper(ui: &mut egui::Ui, p: &Palette, label: &str, value: String) -> i32 {
    let mut dir = 0;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(p.ink));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::ui::widgets::icon_button(ui, p, "plus", aurora_i18n::tr!("photo-vf-ring-more"), true) {
                dir = 1;
            }
            ui.add_sized(
                egui::Vec2::new(34.0, 18.0),
                egui::Label::new(egui::RichText::new(value).color(p.ink)),
            );
            if crate::ui::widgets::icon_button(ui, p, "minus", aurora_i18n::tr!("photo-vf-ring-less"), true) {
                dir = -1;
            }
        });
    });
    dir
}

/// The photo's picture style: its chips (the curves greyed with the reason
/// when tone mapping does nothing), the white balance's presets, and with
/// `fine` the fine-tuning rows and the tint; "Photo only" while set, "Back
/// to viewer" and "Keep this look". The viewfinder's Style sheet and the
/// photo window's row (both interface styles) draw the same rows; true
/// when "Keep this look" was clicked.
pub fn style_rows(ui: &mut egui::Ui, p: &Palette, light: &mut crate::snapshot::light::PhotoLight, cx: &StyleContext, fine: bool) -> bool {
    let prefs = cx.prefs_grade;
    let now = style::matching(light, prefs);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::splat(4.0);
        let all = std::iter::once(style::Style::AsViewer).chain(style::STYLES.iter().map(|(s, _, _)| *s));
        for st in all {
            let curve = st != style::Style::AsViewer;
            let enabled = !curve || cx.tone_ok.is_ok();
            let r = crate::ui::snapshot::chip(ui, p, style_label(st), None, now == st, enabled);
            let r = match cx.tone_ok {
                Err(why) if curve => r.on_disabled_hover_text(why),
                _ => r,
            };
            if r.clicked() {
                style::apply_style(light, prefs, st);
            }
        }
        if now == style::Style::Custom {
            crate::ui::snapshot::chip(ui, p, style_label(style::Style::Custom), None, true, false);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::splat(4.0);
        let wb = style::wb_of(light, prefs);
        for w in Wb::ALL {
            if crate::ui::snapshot::chip(ui, p, wb_label(w), None, wb == Some(w), true)
                .on_hover_text(wb_tip(w))
                .clicked()
            {
                style::set_wb(light, prefs, w);
            }
        }
    });
    if fine {
        for row in style::Fine::ALL {
            let dir = stepper(ui, p, fine_label(row), signed(row.steps(light, prefs)));
            if dir != 0 {
                style::fine_step(light, prefs, row, dir);
            }
        }
        let g = style::grade(light, prefs);
        let dir = stepper(ui, p, aurora_i18n::tr!("pcu-fine-tint"), signed(-style::steps(g.tint)));
        if dir != 0 {
            // (the row counts green: more green, less tint)
            style::set_tint(light, prefs, style::step(g.tint, -dir));
        }
    }
    let mut keep = false;
    if style::is_set(light) {
        ui.label(
            egui::RichText::new(aurora_i18n::tr!("pcu-style-photo-only"))
                .size(11.5)
                .color(p.muted),
        );
    }
    ui.horizontal(|ui| {
        let set = style::is_set(light);
        if ui
            .add_enabled_ui(set, |ui| crate::ui::widgets::flat_button(ui, p, aurora_i18n::tr!("pcu-style-back")))
            .inner
            .clicked()
        {
            light.tonemapper = None;
            light.grade = None;
        }
        keep = ui
            .add_enabled_ui(set, |ui| crate::ui::widgets::flat_button(ui, p, aurora_i18n::tr!("pcu-keep-look")))
            .inner
            .on_hover_text(aurora_i18n::tr!("pcu-keep-look-tip"))
            .clicked();
    });
    keep
}

impl Viewfinder {
    /// The Style sheet (T, the quick menu's tile, the style's line).
    pub(super) fn style_sheet(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        screen: Rect,
        panel: &mut SnapshotUi,
        f: &Frame,
        out: &mut Vec<super::Out>,
    ) {
        let mut keep = false;
        let cx = StyleContext {
            prefs_grade: &f.prefs_grade,
            tone_ok: f.tone_ok.map_or(Ok(()), Err),
        };
        let open = super::sheets::sheet(
            ctx,
            p,
            screen,
            "vf_style_sheet",
            "palette-filled",
            aurora_i18n::tr!("pcu-style-title"),
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                keep = style_rows(ui, p, &mut panel.light, &cx, true);
            },
        );
        if keep {
            out.push(super::Out::Panel(crate::ui::snapshot::Action::KeepStyle));
        }
        if !open {
            self.sheet = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The deck's right end less the ruler's button, at a screen width.
    fn right(w: f32) -> f32 {
        w / 2.0 + controls::DECK_WIDTH / 2.0 - 42.0
    }

    /// Chip widths of a row of `total` points (six chips).
    fn chips(total: f32) -> Vec<f32> {
        vec![(total - 4.0 - 10.0) / 6.0; 6]
    }

    #[test]
    fn the_preset_row_stays_clear_of_the_pad_and_the_dock() {
        // the pad ends at 300, its margin 12; the dock starts at 714 (800)
        // and 1194 (1280)
        for (w, dock) in [(800.0, 714.0), (1280.0, 1194.0)] {
            // Inter at size 1 (en 297, fr 318), OpenDyslexic or text size
            // 1.5 (fr ≈ 409, 419)
            for total in [297.0, 318.0, 409.0, 419.0] {
                let widths = chips(total);
                let rows = wb_rows(&widths, 312.0, right(w));
                for r in &rows {
                    let row: f32 = widths[r.clone()].iter().sum::<f32>() + 2.0 * (r.len() - 1) as f32 + 4.0;
                    assert!(right(w) - row >= 312.0 - 0.01, "{w} {total}: clear of the pad");
                    assert!(right(w) <= dock, "{w}: clear of the dock");
                }
                assert_eq!(rows.iter().map(|r| r.len()).sum::<usize>(), 6);
            }
        }
        // what fits stays on one row
        assert_eq!(wb_rows(&chips(318.0), 312.0, right(800.0)).len(), 1);
        assert_eq!(wb_rows(&chips(409.0), 312.0, right(800.0)).len(), 2);
    }

    #[test]
    fn the_line_says_the_preset_or_the_steps() {
        use crate::settings::render_extras::Grade;
        let mut light = crate::snapshot::light::PhotoLight::default();
        assert_eq!(wb_text(&light, &Grade::default()), aurora_i18n::tr!("pcu-wb-auto"));
        style::set_temperature(&mut light, &Grade::default(), 0.3);
        style::set_tint(&mut light, &Grade::default(), 0.1);
        assert_eq!(wb_text(&light, &Grade::default()), "WB +3 · G−1");
    }
}
