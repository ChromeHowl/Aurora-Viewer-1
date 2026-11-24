//! What the evacuation of a restarting region shows besides the alert's
//! "Evacuate" button (`ui/region_restart.rs`): the offer to go back once the
//! region is up again (`world/restart_evacuation.rs` for the rule).

use super::widgets;
use crate::theme::Palette;
use egui::{Align2, Id, RichText, Vec2};

/// The card under the top bar: "<region> is back online", Return and Stay.
/// Some(true) to go back, Some(false) to stay.
pub fn return_offer(ctx: &egui::Context, p: &Palette, region: &str) -> Option<bool> {
    let mut answer = None;
    egui::Area::new(Id::unique("restart_return_offer"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 96.0))
        .show(ctx, |ui| {
            widgets::overlay(ui, p, egui::Margin::symmetric(14, 10), |ui| {
                ui.set_max_width(360.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    widgets::glyph(ui, "refresh", p.violet, 15.0);
                    ui.label(
                        RichText::new(aurora_i18n::tr!("stay-online-return-offer", region = region))
                            .size(12.5)
                            .color(p.ink),
                    );
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if widgets::action_button(
                        ui,
                        p,
                        "arrow-back-up",
                        aurora_i18n::tr!("stay-online-return-go"),
                        widgets::ActionKind::Primary,
                        Vec2::new(0.0, 27.0),
                    )
                    .clicked()
                    {
                        answer = Some(true);
                    }
                    if widgets::action_button(
                        ui,
                        p,
                        "",
                        aurora_i18n::tr!("stay-online-return-stay"),
                        widgets::ActionKind::Flat,
                        Vec2::new(0.0, 27.0),
                    )
                    .clicked()
                    {
                        answer = Some(false);
                    }
                });
            });
        });
    answer
}
