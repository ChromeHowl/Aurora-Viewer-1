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
