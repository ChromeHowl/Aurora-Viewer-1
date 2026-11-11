//! The viewfinder's sheets on the right, in the place of the dock's panels
//! (one at a time, `Sheet`): a custom mode's slot here; the picture style
//! (`style`), the guide's look and the lens options follow the same frame.

use super::{BankView, Frame, Out, Viewfinder, bold};
use crate::snapshot::Guide;
use crate::snapshot::Settings;
use crate::snapshot::extras::Corner;
use crate::theme::Palette;
use crate::ui::snapshot::Action;
use crate::ui::widgets;
use egui::{Align2, Id, Order, Pos2, Rect};

/// The guide's widths offered (points): Fine (the default), Medium, Bold.
pub const GUIDE_WIDTHS: [f32; 3] = [1.0, 2.0, 4.0];
/// Debanding offered (8-bit levels): Off, Low, High.
pub const DEBANDS: [f32; 3] = [0.0, 0.5, 1.0];
/// The bokeh's colour fringe offered: Off, Low, High.
pub const FRINGES: [f32; 3] = [0.0, 0.25, 0.5];

/// Which of `offered` a value is, if any (another one reads "Custom").
pub fn preset_of(v: f32, offered: &[f32]) -> Option<usize> {
    offered.iter().position(|o| (o - v).abs() < 1e-3)
}

/// The next of `offered` after `v` (a value of no preset: the first).
pub fn next_preset(v: f32, offered: &[f32]) -> f32 {
    preset_of(v, offered).map_or(offered[0], |i| offered[(i + 1) % offered.len()])
}

/// What the lens options ask of the app (Preferences' global settings).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LensEdit {
    FrontBlur(bool),
    Fringe(f32),
}

/// What a custom mode's sheet asks of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum BankEdit {
    /// The photo's settings now saved as the slot's profile (a new one,
    /// named after the slot, when none is pinned).
    SaveHere,
    /// A profile pinned to the slot.
    Pin(String),
    /// The pinned profile renamed.
    Rename(String),
    Unpin,
}

/// A sheet's frame: the drawer's place and width, its title with an icon
/// and a close button; false once closed.
pub(super) fn sheet(
    ctx: &egui::Context,
    p: &Palette,
    screen: Rect,
    id: &str,
    icon: &str,
    title: &str,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut open = true;
    egui::Area::new(Id::unique(id))
        .order(Order::Foreground)
        .fade_in(false)
        .pivot(Align2::RIGHT_TOP)
        .fixed_pos(Pos2::new(screen.right() - 84.0, screen.top() + 128.0))
        .show(ctx, |ui| {
            widgets::overlay(ui, p, egui::Margin::same(12), |ui| {
                ui.set_width(316.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
                    super::icon(ui.painter(), icon, r, p.ink);
                    ui.label(egui::RichText::new(title).font(bold(ctx, 14.0)).color(p.ink));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::icon_button(ui, p, "x", aurora_i18n::tr!("common-close"), true) {
                            open = false;
                        }
                    });
                });
                ui.add_space(4.0);
                body(ui);
            });
        });
    open
}

impl Viewfinder {
    /// A custom mode's slot: save the photo's settings there, pin another
    /// profile, rename it, reset a part, unpin.
    pub(super) fn bank_sheet(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        screen: Rect,
        slot: u8,
        s: &Settings,
        bank: &BankView,
        out: &mut Vec<Out>,
    ) {
        let title = format!("C{}", slot + 1);
        let mut edits = Vec::new();
        let name_field = &mut self.bank_name;
        let open = sheet(ctx, p, screen, "vf_bank_sheet", "bookmarks", &title, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            let now = match (&bank.name, bank.exists) {
                (Some(n), true) => n.clone(),
                (Some(n), false) => format!("{n} — {}", aurora_i18n::tr!("pcu-bank-deleted")),
                (None, _) => aurora_i18n::tr!("pcu-bank-empty").to_owned(),
            };
            ui.label(egui::RichText::new(now).color(if bank.exists { p.ink } else { p.muted }));
            if widgets::flat_button(ui, p, aurora_i18n::tr!("pcu-bank-save-here")).clicked() {
                edits.push(BankEdit::SaveHere);
            }
            ui.label(egui::RichText::new(aurora_i18n::tr!("pcu-bank-choose")).size(12.0).color(p.muted));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::splat(4.0);
                for name in s.profiles.keys() {
                    let on = bank.name.as_deref() == Some(name.as_str());
                    if crate::ui::snapshot::chip(ui, p, name, None, on, true).clicked() {
                        edits.push(BankEdit::Pin(name.clone()));
                    }
                }
                if s.profiles.is_empty() {
                    ui.label(egui::RichText::new(aurora_i18n::tr!("p16-profiles-empty")).color(p.muted));
                }
            });
            if bank.exists {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(name_field)
                            .hint_text(aurora_i18n::tr!("pcu-bank-new-name"))
                            .char_limit(64)
                            .desired_width(180.0),
                    );
                    let valid = crate::snapshot::saved::valid_name(name_field) && !s.profiles.contains_key(name_field.trim());
                    if widgets::icon_button(ui, p, "pencil", aurora_i18n::tr!("pcu-bank-rename"), valid) {
                        edits.push(BankEdit::Rename(name_field.trim().to_owned()));
                        name_field.clear();
                    }
                });
            }
            ui.horizontal(|ui| {
                widgets::icon_menu(ui, p, "restore", aurora_i18n::tr!("pcu-bank-reset-part"), |ui| {
                    for part in crate::snapshot::saved::Part::ALL {
                        if crate::ui::menu::item(ui, p, "arrow-back-up", crate::ui::snapshot::reset_label(part)) {
                            out.push(Out::Panel(Action::ProfileReset(Some(part))));
                        }
                    }
                });
                if bank.name.is_some() && widgets::flat_button(ui, p, aurora_i18n::tr!("pcu-bank-unpin")).clicked() {
                    edits.push(BankEdit::Unpin);
                }
            });
        });
        out.extend(edits.into_iter().map(|e| Out::Bank(slot, e)));
        if !open {
            self.sheet = None;
        }
    }

    /// The guide's look: which guide, the spiral's corner, its colour (or
    /// the theme's) and the width of its lines.
    pub(super) fn guide_sheet(&mut self, ctx: &egui::Context, p: &Palette, screen: Rect, s: &mut Settings) {
        let open = sheet(
            ctx,
            p,
            screen,
            "vf_guide_sheet",
            "grid-3x3",
            aurora_i18n::tr!("b1-guide-section"),
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::splat(4.0);
                    for g in std::iter::once(Guide::None).chain(Guide::ALL) {
                        let (icon, tip) = crate::ui::snapshot::guide_face(g);
                        if widgets::icon_toggle(ui, p, icon, tip, s.guide == g) {
                            s.guide = g;
                        }
                    }
                });
                if s.guide == Guide::Spiral {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (c, arrow) in Corner::ALL.into_iter().zip(["↖", "↗", "↙", "↘"]) {
                            if crate::ui::snapshot::chip(ui, p, arrow, None, s.extras.guide_corner == c, true).clicked() {
                                s.extras.guide_corner = c;
                            }
                        }
                    });
                }
                let e = &mut s.extras;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if crate::ui::snapshot::chip(ui, p, aurora_i18n::tr!("pcu-guide-theme"), None, e.guide_color.is_none(), true).clicked()
                    {
                        e.guide_color = None;
                    }
                    // (the Legacy Viewer's yellow at half its opacity, then the user's)
                    let mut c = e.guide_color.unwrap_or([255, 255, 0, 128]);
                    let before = c;
                    widgets::color_button(ui, p, "vf_guide_color", &mut c, true);
                    if c != before {
                        e.guide_color = Some(c);
                    }
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let now = preset_of(e.guide_width, &GUIDE_WIDTHS);
                    let names = [
                        aurora_i18n::tr!("pcu-width-fine"),
                        aurora_i18n::tr!("pcu-width-medium"),
                        aurora_i18n::tr!("pcu-width-bold"),
                    ];
                    for (i, (w, name)) in GUIDE_WIDTHS.into_iter().zip(names).enumerate() {
                        if crate::ui::snapshot::chip(ui, p, name, None, now == Some(i), true).clicked() {
                            e.guide_width = w;
                        }
                    }
                    if now.is_none() {
                        crate::ui::snapshot::chip(ui, p, aurora_i18n::tr!("pcu-custom"), None, true, false);
                    }
                });
            },
        );
        if !open {
            self.sheet = None;
        }
    }

    /// The lens options of the aperture ring: Preferences' front blur and
    /// bokeh fringe (global settings, shown here too).
    pub(super) fn lens_sheet(&mut self, ctx: &egui::Context, p: &Palette, screen: Rect, f: &Frame, out: &mut Vec<Out>) {
        let (front, fringe) = f.lens_extras;
        let open = sheet(
            ctx,
            p,
            screen,
            "vf_lens_sheet",
            "aperture",
            aurora_i18n::tr!("pcu-lens-title"),
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let mut on = front;
                if widgets::switch_labeled(ui, p, &mut on, aurora_i18n::tr!("pcu-lens-front")).changed() {
                    out.push(Out::Lens(LensEdit::FrontBlur(on)));
                }
                ui.label(egui::RichText::new(aurora_i18n::tr!("pcu-lens-fringe")).color(p.ink));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let now = preset_of(fringe, &FRINGES);
                    let names = [
                        aurora_i18n::tr!("photo-vf-off"),
                        aurora_i18n::tr!("pcu-low"),
                        aurora_i18n::tr!("pcu-high"),
                    ];
                    for (i, (v, name)) in FRINGES.into_iter().zip(names).enumerate() {
                        if crate::ui::snapshot::chip(ui, p, name, None, now == Some(i), true).clicked() {
                            out.push(Out::Lens(LensEdit::Fringe(v)));
                        }
                    }
                    if now.is_none() {
                        crate::ui::snapshot::chip(ui, p, aurora_i18n::tr!("pcu-custom"), None, true, false);
                    }
                });
                ui.label(egui::RichText::new(aurora_i18n::tr!("pcu-lens-global")).size(11.0).color(p.muted));
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
    fn values_off_the_offered_ones_read_custom() {
        assert_eq!(preset_of(1.0, &GUIDE_WIDTHS), Some(0), "Fine is the default width");
        assert_eq!(
            preset_of(crate::snapshot::extras::Extras::default().guide_width, &GUIDE_WIDTHS),
            Some(0)
        );
        assert_eq!(preset_of(3.0, &GUIDE_WIDTHS), None);
        assert_eq!(preset_of(0.5, &DEBANDS), Some(1));
        assert_eq!(preset_of(1.7, &DEBANDS), None);
        assert_eq!(preset_of(0.25, &FRINGES), Some(1));
        assert_eq!(preset_of(0.3, &FRINGES), None);
        assert_eq!(next_preset(1.0, &DEBANDS), 0.0, "round");
        assert_eq!(next_preset(1.7, &DEBANDS), 0.0, "custom: from the first");
        assert_eq!(next_preset(0.0, &DEBANDS), 0.5);
    }
}
