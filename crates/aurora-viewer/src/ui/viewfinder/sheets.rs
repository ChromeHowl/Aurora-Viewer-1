//! The viewfinder's sheets on the right, in the place of the dock's panels
//! (one at a time, `Sheet`): a custom mode's slot here; the picture style
//! (`style`), the guide's look and the lens options follow the same frame.

use super::{BankView, Out, Viewfinder, bold};
use crate::snapshot::Settings;
use crate::theme::Palette;
use crate::ui::snapshot::Action;
use crate::ui::widgets;
use egui::{Align2, Id, Order, Pos2, Rect};

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
}
