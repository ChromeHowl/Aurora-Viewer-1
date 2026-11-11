//! World › Shadow Realm…: the residents of the account's Shadow Realm
//! (`world/shadow_realm.rs`), with the switch, the distance within which
//! they show (in red), and Add / Remove. Nothing here talks to the server.

use super::avatar_picker::AvatarPicker;
use super::menu;
use super::widgets::{self, ActionKind, Floater};
use crate::settings::shadow_realm::{MAX_DISTANCE, MIN_DISTANCE, ShadowRealm as Settings};
use crate::theme::Palette;
use crate::world::World;
use egui::{RichText, Sense, Vec2};
use std::collections::HashSet;
use uuid::Uuid;

/// The window's id.
pub const WINDOW: &str = "shadow_realm";
const ROW_H: f32 = 26.0;

/// One resident of the list, as the window shows them.
pub struct Row {
    pub id: Uuid,
    /// Their name, or RLVa's stand-in while it hides names.
    pub name: String,
    pub can_name: bool,
    /// In range now (their name is red elsewhere).
    pub shown: bool,
}

/// What the window asks the app for.
#[derive(Debug, PartialEq)]
pub enum Action {
    Add(Vec<Uuid>),
    Remove(Vec<Uuid>),
    Profile(Uuid),
}

#[derive(Default)]
pub struct ShadowRealmUi {
    filter: String,
    selected: HashSet<Uuid>,
    picker: AvatarPicker,
}

/// The rows the filter keeps, by name (natural order).
fn filtered<'a>(rows: &'a [Row], filter: &str) -> Vec<&'a Row> {
    let filter = filter.trim().to_lowercase();
    let mut shown: Vec<&Row> = rows
        .iter()
        .filter(|r| filter.is_empty() || r.name.to_lowercase().contains(&filter) || (r.can_name && r.id.to_string().contains(&filter)))
        .collect();
    shown.sort_by(|a, b| crate::natural_order::cmp(&a.name, &b.name).then(a.id.cmp(&b.id)));
    shown
}

impl ShadowRealmUi {
    /// Draw the window while `open`; `settings` changes in place (true when
    /// it changed: the app saves it).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        rows: &[Row],
        settings: &mut Settings,
        world: &mut World,
        open: &mut bool,
    ) -> (Option<Action>, bool) {
        if !*open {
            self.picker.open = false;
            return (None, false);
        }
        let mut action = None;
        let mut changed = false;
        let red = p.danger;
        let mut floater = Floater::new(
            WINDOW,
            aurora_i18n::tr!("sr-title"),
            egui::pos2(300.0, 160.0),
            Vec2::new(460.0, 420.0),
        );
        floater.min_size = Vec2::new(380.0, 280.0);
        floater.show(ctx, p, open, |ui| {
            ui.horizontal(|ui| {
                changed |= widgets::switch_labeled(ui, p, &mut settings.enabled, aurora_i18n::tr!("sr-enabled"))
                    .on_hover_text(aurora_i18n::tr!("sr-enabled-tip"))
                    .changed();
                ui.add_space(12.0);
                ui.label(RichText::new(aurora_i18n::tr!("sr-distance")).color(p.muted));
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut settings.distance_m)
                            .range(MIN_DISTANCE..=MAX_DISTANCE)
                            .speed(0.25)
                            .max_decimals(1)
                            .suffix(aurora_i18n::tr!("sr-metres-suffix")),
                    )
                    .on_hover_text(aurora_i18n::tr!("sr-distance-tip"))
                    .changed();
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let add_label = aurora_i18n::tr!("common-add");
                let add_width = widgets::action_button_fit(ui, "plus", add_label);
                let width = (ui.available_width() - add_width - ui.spacing().item_spacing.x).max(80.0);
                widgets::clearable_search_box(ui, p, &mut self.filter, aurora_i18n::tr!("sr-filter"), width);
                // RLVa's @shownames: no picker naming residents
                let can_pick = !world.rlva.has("shownames");
                if widgets::action_button_if(ui, p, "plus", add_label, ActionKind::Flat, Vec2::new(0.0, 26.0), can_pick).clicked() {
                    self.picker.open("shadow_realm_picker", true);
                }
            });
            ui.add_space(6.0);
            let shown = filtered(rows, &self.filter);
            let order: Vec<Uuid> = shown.iter().map(|r| r.id).collect();
            self.selected.retain(|id| order.contains(id));
            let footer = 32.0 + ui.spacing().item_spacing.y;
            let height = (ui.available_height() - footer).max(0.0);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_height(height);
                    egui::ScrollArea::vertical().id_salt(WINDOW).auto_shrink([false, false]).show_rows(
                        ui,
                        ROW_H,
                        shown.len(),
                        |ui, range| {
                            if shown.is_empty() {
                                ui.label(RichText::new(aurora_i18n::tr!("sr-empty")).color(p.muted));
                            }
                            for row in &shown[range] {
                                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
                                let selected = self.selected.contains(&row.id);
                                if selected || resp.hovered() {
                                    let color = if selected {
                                        p.violet.gamma_multiply(0.30)
                                    } else {
                                        p.raised.gamma_multiply(0.6)
                                    };
                                    ui.painter().rect_filled(rect, ui.visuals().window_corner_radius, color);
                                }
                                let split = rect.left() + rect.width() * 0.62;
                                // red in range, as everywhere, unless RLVa hides the name
                                let name_color = if row.shown && row.can_name && settings.enabled {
                                    red
                                } else {
                                    p.ink
                                };
                                widgets::paint_elided(
                                    ui,
                                    &row.name,
                                    egui::pos2(rect.left() + 6.0, rect.center().y),
                                    split - rect.left() - 12.0,
                                    egui::FontId::proportional(12.5),
                                    name_color,
                                );
                                let state = match (settings.enabled, row.shown) {
                                    (false, _) => aurora_i18n::tr!("sr-state-off"),
                                    (true, true) => aurora_i18n::tr!("sr-state-shown"),
                                    (true, false) => aurora_i18n::tr!("sr-state-hidden"),
                                };
                                widgets::paint_elided(
                                    ui,
                                    state,
                                    egui::pos2(split + 6.0, rect.center().y),
                                    rect.right() - split - 12.0,
                                    egui::FontId::proportional(12.0),
                                    p.muted,
                                );
                                if row.can_name {
                                    resp.clone().on_hover_text(format!("{}\n{}", row.name, row.id));
                                }
                                if resp.clicked() {
                                    let command = ui.input(|i| i.modifiers.command);
                                    if !command {
                                        self.selected.clear();
                                    }
                                    if !self.selected.remove(&row.id) {
                                        self.selected.insert(row.id);
                                    }
                                }
                                if resp.secondary_clicked() && !selected {
                                    self.selected.clear();
                                    self.selected.insert(row.id);
                                }
                                menu::context_menu(&resp, p, |ui| {
                                    if menu::item(ui, p, "trash-filled", aurora_i18n::tr!("sr-remove")) {
                                        action = Some(Action::Remove(
                                            order.iter().copied().filter(|id| self.selected.contains(id)).collect(),
                                        ));
                                        ui.close();
                                    }
                                    if row.can_name && menu::item(ui, p, "user-filled", aurora_i18n::tr!("common-profile")) {
                                        action = Some(Action::Profile(row.id));
                                        ui.close();
                                    }
                                });
                            }
                        },
                    );
                },
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let remove = widgets::action_button_if(
                    ui,
                    p,
                    "trash-filled",
                    aurora_i18n::tr!("sr-remove"),
                    ActionKind::Flat,
                    Vec2::new(0.0, 26.0),
                    !self.selected.is_empty(),
                );
                if remove.clicked() {
                    action = Some(Action::Remove(
                        order.iter().copied().filter(|id| self.selected.contains(id)).collect(),
                    ));
                }
            });
        });
        if !*open || world.rlva.has("shownames") {
            self.picker.open = false;
        }
        if let Some(ids) = self.picker.show(ctx, p, world) {
            action = Some(Action::Add(ids));
        }
        settings.sanitize();
        (action, changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(n: u128, name: &str, can_name: bool) -> Row {
        Row {
            id: Uuid::from_u128(n),
            name: name.to_owned(),
            can_name,
            shown: false,
        }
    }

    #[test]
    fn the_filter_finds_names_and_only_the_ids_that_may_be_named() {
        let rows = [
            row(1, "Zed Resident", true),
            row(2, "anna Example", true),
            row(3, "Someone 3", false),
        ];
        let names = |filter: &str| filtered(&rows, filter).iter().map(|r| r.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(""), vec!["anna Example", "Someone 3", "Zed Resident"]);
        assert_eq!(names("ZED"), vec!["Zed Resident"]);
        let one = Uuid::from_u128(1).to_string();
        assert_eq!(names(&one), vec!["Zed Resident"]);
        let three = Uuid::from_u128(3).to_string();
        assert!(names(&three).is_empty(), "an id RLVa hides is not searchable");
    }
}
