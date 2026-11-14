//! Advanced › Set window size › Custom size…: a small window with a width
//! and a height in pixels and an Apply button, for a size the list of
//! `bars::WINDOW_SIZES` does not have (Legacy Viewer: the custom size of
//! LLFloaterWindowSize, indra/newview/llfloaterwindowsize.cpp). The values are
//! parsed and clamped by `app/window_size.rs`.
use super::widgets::{self, Floater};
use crate::theme::Palette;

/// The smallest inside size a window is given, the same floor as
/// `AURORA_WINDOW_SIZE`.
pub(crate) const MIN_SIZE: (u32, u32) = (800, 560);
/// The largest inside size when the monitor is unknown (8K).
pub(crate) const MAX_SIZE: (u32, u32) = (7680, 4320);

/// The size kept within `MIN_SIZE` and the monitor's size (`MAX_SIZE` when
/// the monitor is unknown; never below the minimum).
pub(crate) fn clamp_size(width: u32, height: u32, monitor: Option<(u32, u32)>) -> (u32, u32) {
    let (max_w, max_h) = limits(monitor).1;
    (width.clamp(MIN_SIZE.0, max_w), height.clamp(MIN_SIZE.1, max_h))
}

/// The (minimum, maximum) size the custom window accepts.
pub(crate) fn limits(monitor: Option<(u32, u32)>) -> ((u32, u32), (u32, u32)) {
    let (mw, mh) = monitor.filter(|&(w, h)| w > 0 && h > 0).unwrap_or(MAX_SIZE);
    (MIN_SIZE, (mw.max(MIN_SIZE.0), mh.max(MIN_SIZE.1)))
}

/// The two fields of the custom size window: whole pixels, spaces around
/// them allowed. `None` when either is empty, negative, fractional or not
/// a number.
pub(crate) fn parse_size(width: &str, height: &str) -> Option<(u32, u32)> {
    Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
}

#[derive(Default)]
pub struct WindowSizeUi {
    pub open: bool,
    width: String,
    height: String,
    /// The last Apply had a field that is not a whole number.
    invalid: bool,
}

impl WindowSizeUi {
    /// Opens the window with the current inside size in both fields.
    pub fn open_with(&mut self, current: Option<(u32, u32)>) {
        let (w, h) = current.unwrap_or((1280, 720));
        self.width = w.to_string();
        self.height = h.to_string();
        self.invalid = false;
        self.open = true;
    }

    /// The size to apply once Apply (or Enter) was pressed. `monitor` is the
    /// monitor's size in pixels when known.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, monitor: Option<(u32, u32)>) -> Option<(u32, u32)> {
        if !self.open {
            return None;
        }
        let (min, max) = limits(monitor);
        let mut open = self.open;
        let mut apply = false;
        Floater::new(
            "window_size",
            aurora_i18n::tr!("window-size-title"),
            egui::pos2(460.0, 180.0),
            egui::vec2(330.0, 190.0),
        )
        .show(ctx, p, &mut open, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            egui::Grid::new("window_size_fields").num_columns(2).show(ui, |ui| {
                for (label, text) in [
                    (aurora_i18n::tr!("window-size-width"), &mut self.width),
                    (aurora_i18n::tr!("window-size-height"), &mut self.height),
                ] {
                    ui.label(label);
                    let response = ui.add(egui::TextEdit::singleline(text).desired_width(90.0).char_limit(5));
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        apply = true;
                    }
                    ui.end_row();
                }
            });
            ui.label(aurora_i18n::tr!(
                "window-size-range",
                min_w = min.0.to_string(),
                min_h = min.1.to_string(),
                max_w = max.0.to_string(),
                max_h = max.1.to_string()
            ));
            if self.invalid {
                ui.label(egui::RichText::new(aurora_i18n::tr!("window-size-invalid")).color(p.violet_pale));
            }
            ui.add_space(6.0);
            if widgets::primary_button_sized(ui, p, aurora_i18n::tr!("window-size-apply"), egui::vec2(0.0, 28.0)).clicked() {
                apply = true;
            }
        });
        self.open = open;
        if !apply {
            return None;
        }
        match parse_size(&self.width, &self.height) {
            Some((w, h)) => {
                self.invalid = false;
                let (w, h) = clamp_size(w, h, monitor);
                // show what was kept
                self.width = w.to_string();
                self.height = h.to_string();
                Some((w, h))
            }
            None => {
                self.invalid = true;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_parse_whole_pixels_only() {
        assert_eq!(parse_size("1280", "720"), Some((1280, 720)));
        assert_eq!(parse_size(" 1280 ", "\t720\n"), Some((1280, 720)));
        assert_eq!(parse_size("", "720"), None);
        assert_eq!(parse_size("1280", ""), None);
        assert_eq!(parse_size("-1280", "720"), None);
        assert_eq!(parse_size("12.5", "720"), None);
        assert_eq!(parse_size("wide", "720"), None);
        assert_eq!(parse_size("99999999999", "720"), None, "beyond u32");
    }

    #[test]
    fn the_size_stays_between_the_minimum_and_the_monitor() {
        let monitor = Some((1920, 1080));
        assert_eq!(clamp_size(1280, 720, monitor), (1280, 720));
        assert_eq!(clamp_size(1, 1, monitor), MIN_SIZE);
        assert_eq!(clamp_size(0, 0, None), MIN_SIZE);
        assert_eq!(clamp_size(3840, 2160, monitor), (1920, 1080));
        assert_eq!(clamp_size(3840, 600, monitor), (1920, 600), "each side on its own");
        assert_eq!(clamp_size(100_000, 100_000, None), MAX_SIZE);
        assert_eq!(clamp_size(3840, 2160, None), (3840, 2160));
    }

    #[test]
    fn a_tiny_or_unknown_monitor_never_pushes_the_maximum_below_the_minimum() {
        assert_eq!(limits(Some((640, 480))), (MIN_SIZE, MIN_SIZE));
        assert_eq!(limits(Some((0, 0))), (MIN_SIZE, MAX_SIZE));
        assert_eq!(limits(None), (MIN_SIZE, MAX_SIZE));
        assert_eq!(clamp_size(1000, 700, Some((640, 480))), MIN_SIZE);
    }
}
