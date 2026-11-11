//! The shot mode, as on a phone camera: shown on the shutter's disc, chosen
//! in a list left of the dock (a right-click on the shutter, the quick
//! menu's tile, the P key). The logic is `snapshot::viewfinder` (`Shot`,
//! `available`, `choose`); the app applies the choice (`Out::Shot`).

use super::{Frame, Out, Sheet, Viewfinder, bold, icon, regular};
use crate::snapshot::viewfinder::{self as vf, Refused, ShotChoice};
use crate::theme::Palette;
use crate::ui::snapshot::SnapshotUi;
use crate::ui::widgets;
use egui::{Align2, Id, Order, Pos2, Rect, Sense, Vec2};

/// Between the list and the dock's left edge.
const LIST_GAP: f32 = 12.0;
/// The list's width with its margins.
pub(super) const LIST_WIDTH: f32 = 240.0;

/// Where the mode list goes: its right edge `LIST_GAP` left of the dock's
/// real left edge (the 66-point shutter widens the dock's column),
/// vertically centred on the shutter, kept under the top rows' band
/// (top + 96) and above the ring deck and its preset row (bottom − 170);
/// the top wins when the screen is too short.
pub(super) fn list_rect(screen: Rect, dock_left: f32, shutter_y: f32, size: Vec2) -> Rect {
    let right = dock_left - LIST_GAP;
    let (lo, hi) = (screen.top() + 96.0, screen.bottom() - 170.0);
    let top = (shutter_y - size.y / 2.0).min(hi - size.y).max(lo);
    Rect::from_min_size(Pos2::new(right - size.x, top), size)
}

/// The glyph a mode puts on the shutter's disc (Photo: none).
pub(super) enum Glyph {
    Icon(&'static str),
    Text(String),
}

pub(super) fn glyph(choice: ShotChoice) -> Option<Glyph> {
    match choice {
        ShotChoice::Photo => None,
        ShotChoice::FullSet => Some(Glyph::Icon("box-multiple-filled")),
        ShotChoice::Pano => Some(Glyph::Icon("panorama-horizontal-filled")),
        ShotChoice::Depth => Some(Glyph::Icon("layers-difference")),
        ShotChoice::Bank(i) => Some(Glyph::Text(format!("C{}", i + 1))),
    }
}

/// A mode's name and its second line.
pub(super) fn label(choice: ShotChoice) -> (String, Option<&'static str>) {
    match choice {
        ShotChoice::Photo => (aurora_i18n::tr!("pcu-shot-photo").to_owned(), None),
        ShotChoice::FullSet => (
            aurora_i18n::tr!("pcu-shot-fullset").to_owned(),
            Some(aurora_i18n::tr!("pcu-shot-fullset-caption")),
        ),
        ShotChoice::Pano => ("360°".to_owned(), None),
        ShotChoice::Depth => (aurora_i18n::tr!("pcu-shot-depth").to_owned(), None),
        ShotChoice::Bank(i) => (format!("C{}", i + 1), None),
    }
}

/// Why an entry is greyed, as said.
pub fn refused_text(why: Refused) -> &'static str {
    match why {
        Refused::CameraLimited => aurora_i18n::tr!("b1-pano-rlva"),
        Refused::VisionLimited => aurora_i18n::tr!("pcu-depth-rlva"),
        Refused::NoProfile => aurora_i18n::tr!("pcu-bank-none"),
    }
}

impl Viewfinder {
    /// The list of the shot modes, left of the shutter.
    pub(super) fn mode_list(&mut self, ctx: &egui::Context, p: &Palette, screen: Rect, panel: &SnapshotUi, f: &Frame, out: &mut Vec<Out>) {
        let id = Id::unique("vf_modes");
        let dock_left = ctx
            .memory(|m| m.area_rect(Id::unique("vf_dock")))
            .map_or(screen.right() - 20.0 - 66.0, |r| r.left());
        let shutter_y = self.shutter_at.map_or(screen.center().y, |r| r.center().y);
        let size = ctx.memory(|m| m.area_rect(id)).map_or(Vec2::splat(LIST_WIDTH), |r| r.size());
        let rect = list_rect(screen, dock_left, shutter_y, size);
        let now = vf::shown(panel.shot, panel.depth, panel.bank);
        let list = vf::available(f.pano_ok, f.depth_ok, std::array::from_fn(|i| f.banks[i].exists));
        egui::Area::new(id)
            .order(Order::Foreground)
            .fade_in(false)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                widgets::overlay(ui, p, egui::Margin::same(12), |ui| {
                    ui.set_width(LIST_WIDTH - 24.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("pcu-shot-title"))
                            .font(bold(ctx, 13.0))
                            .color(p.ink),
                    );
                    for (i, (choice, why)) in list.into_iter().enumerate() {
                        let (name, caption) = label(choice);
                        let name = match (choice, why) {
                            (ShotChoice::Bank(b), _) => match &f.banks[usize::from(b)].name {
                                Some(n) if f.banks[usize::from(b)].exists => format!("{name} · {n}"),
                                Some(_) => format!("{name} —"),
                                None => format!("{name} · {}", aurora_i18n::tr!("pcu-bank-empty")),
                            },
                            _ => name,
                        };
                        let differs = matches!(choice, ShotChoice::Bank(b) if f.banks[usize::from(b)].differs && now == choice);
                        let resp = mode_row(
                            ui,
                            p,
                            choice,
                            &name,
                            caption,
                            now == choice,
                            self.mode_cursor == i,
                            why.is_none(),
                            differs,
                        );
                        let resp = match why {
                            Some(w) => resp.on_hover_text(refused_text(w)),
                            None => resp,
                        };
                        if resp.clicked() {
                            resp.surrender_focus();
                            self.mode_cursor = i;
                            match (choice, why) {
                                (_, None) => {
                                    self.sheet = None;
                                    out.push(Out::Shot(choice));
                                }
                                // an empty or lost slot: its sheet, to fill it
                                (ShotChoice::Bank(b), Some(_)) => self.open_sheet(Sheet::Bank(b), out),
                                _ => {}
                            }
                        }
                        if let ShotChoice::Bank(b) = choice
                            && resp.secondary_clicked()
                        {
                            self.open_sheet(Sheet::Bank(b), out);
                        }
                    }
                });
            });
    }
}

/// A row of the mode list: its glyph, its name (a second line under it),
/// the mode shown marked, the row the keys are on lit.
#[allow(clippy::too_many_arguments)]
fn mode_row(
    ui: &mut egui::Ui,
    p: &Palette,
    choice: ShotChoice,
    name: &str,
    caption: Option<&str>,
    on: bool,
    cursor: bool,
    enabled: bool,
    differs: bool,
) -> egui::Response {
    let h = if caption.is_some() { 38.0 } else { 28.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
    let radius = widgets::inner_radius(ui.visuals());
    if on {
        ui.painter().rect_filled(rect, radius, p.ink.gamma_multiply(0.16));
    } else if resp.hovered() || cursor {
        ui.painter().rect_filled(rect, radius, p.ink.gamma_multiply(0.07));
    }
    if cursor {
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0, p.teal.gamma_multiply(0.7)),
            egui::StrokeKind::Inside,
        );
    }
    let color = if !enabled {
        p.muted_dim
    } else if on {
        p.ink
    } else {
        p.muted
    };
    let g = Rect::from_center_size(Pos2::new(rect.left() + 16.0, rect.top() + 14.0), Vec2::splat(18.0));
    match glyph(choice) {
        Some(Glyph::Icon(name)) => icon(ui.painter(), name, g, color),
        Some(Glyph::Text(t)) => {
            ui.painter().text(g.center(), Align2::CENTER_CENTER, t, bold(ui.ctx(), 11.0), color);
        }
        None => icon(ui.painter(), "camera-filled", g, color),
    }
    let text = ui.painter().text(
        Pos2::new(rect.left() + 34.0, rect.top() + 14.0),
        Align2::LEFT_CENTER,
        name,
        bold(ui.ctx(), 13.0),
        color,
    );
    if let Some(c) = caption {
        ui.painter().text(
            Pos2::new(rect.left() + 34.0, rect.top() + 29.0),
            Align2::LEFT_CENTER,
            c,
            regular(11.0),
            p.muted,
        );
    }
    if differs {
        ui.painter()
            .circle_filled(Pos2::new(text.right() + 8.0, text.center().y), 3.5, p.teal);
    }
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dock's left edge as drawn: 20 points from the right, as wide as
    /// the shutter.
    fn dock_left(screen: Rect) -> f32 {
        screen.right() - 20.0 - 66.0
    }

    #[test]
    fn the_list_stays_left_of_the_dock_and_between_the_bars() {
        for (w, h, right) in [(800.0, 560.0, 702.0), (1280.0, 720.0, 1182.0)] {
            let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h));
            for shutter_y in [0.0, h / 2.0, h] {
                let r = list_rect(screen, dock_left(screen), shutter_y, Vec2::new(LIST_WIDTH, 240.0));
                assert_eq!(r.right(), right, "{w}: ends left of the shutter");
                assert!(r.top() >= screen.top() + 96.0, "{w}: under the top rows");
                assert!(r.bottom() <= screen.bottom() - 170.0, "{w}: over the ring deck");
                // clear of the camera pad (it ends at 300)
                assert!(r.left() > 300.0);
            }
        }
    }

    #[test]
    fn every_mode_but_photo_marks_the_shutter() {
        assert!(glyph(ShotChoice::Photo).is_none());
        for c in [ShotChoice::FullSet, ShotChoice::Pano, ShotChoice::Depth, ShotChoice::Bank(2)] {
            assert!(glyph(c).is_some(), "{c:?}");
        }
        assert!(matches!(glyph(ShotChoice::Bank(1)), Some(Glyph::Text(t)) if t == "C2"));
    }
}
