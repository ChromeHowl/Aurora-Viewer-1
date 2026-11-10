//! People › Hidden, beside Blocked: the friends hidden in this viewer
//! (`world/hidden_friends.rs`) by name, each with "Un-hide" on the row
//! under the pointer and in its menu. Nothing here talks to the server: a
//! hidden friend is still a friend, only shown as a stranger everywhere.
//! Both looks share the list (`entries`); the Legacy look draws it as its
//! simple table (`legacy_tab`).

use super::rows::{self, RICH_GAP, RICH_H};
use super::{Ctx, PeopleAction, PeopleUi};
use crate::theme::Palette;
use crate::ui::context::{self, CtxAction};
use crate::ui::{menu, widgets};
use crate::world::World;
use egui::{FontId, Pos2, Rect, RichText, Sense, Vec2};
use uuid::Uuid;

/// The hidden friends whose name matches `filter` (trimmed, lowercase), by
/// name.
pub(super) fn entries(world: &World, filter: &str) -> Vec<(Uuid, String)> {
    let mut out: Vec<(Uuid, String)> = world
        .social
        .hidden_friends()
        .map(|f| (f.id, world.social.name_of(&f.id)))
        .filter(|(id, name)| super::matches_name(world, id, name, filter))
        .collect();
    out.sort_by_cached_key(|(_, name)| crate::natural_order::key(name));
    out
}

/// How many friends are hidden (the tab's count).
pub(super) fn count(world: &World) -> usize {
    world.social.hidden_friends().count()
}

fn unhide(ui: &egui::Ui, id: Uuid) {
    context::request(ui.ctx(), CtxAction::UnhideFriend(id));
}

/// "Show them greyed in Friends" (FDShowHiddenFriends, for this session),
/// and its tip.
fn show_switch(ui: &mut egui::Ui, p: &Palette, show: &mut bool) {
    widgets::switch_labeled(ui, p, show, aurora_i18n::tr!("b6-hidden-show")).on_hover_text(aurora_i18n::tr!("b6-hidden-show-tip"));
}

/// The row's menu, the same in both looks.
fn row_menu(resp: &egui::Response, p: &Palette, id: Uuid, actions: &mut Vec<PeopleAction>) {
    menu::context_menu(resp, p, |ui| {
        if menu::item(ui, p, "user-check", aurora_i18n::tr!("hidefriend-unhide")) {
            unhide(ui, id);
        }
        menu::separator(ui, p);
        if menu::item(ui, p, "user-filled", aurora_i18n::tr!("common-profile")) {
            actions.push(PeopleAction::Profile(id));
        }
    });
}

pub(super) fn tab(ui: &mut egui::Ui, c: &mut Ctx, st: &mut PeopleUi) {
    let p = c.p;
    let world = c.world;
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.label(RichText::new(aurora_i18n::tr!("hidefriend-list-hint")).size(11.5).color(p.muted));
    ui.add_space(4.0);
    show_switch(ui, p, &mut st.show_hidden);
    ui.add_space(4.0);
    let line = Vec2::new(ui.available_width(), rows::TOOLBAR_H);
    ui.allocate_ui_with_layout(line, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let w = ui.available_width();
        widgets::search_box(ui, p, &mut st.filter, aurora_i18n::tr!("people-filter-short"), w);
    });
    ui.add_space(7.0);
    let list = entries(world, &st.filter.trim().to_lowercase());
    egui::ScrollArea::vertical()
        .id_salt("people-hidden-rows")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = RICH_GAP;
            if list.is_empty() {
                rows::empty(ui, p, aurora_i18n::tr!("hidefriend-list-empty"));
            }
            for (id, name) in &list {
                row(ui, c, st, *id, name);
            }
        });
}

/// The width of "Un-hide" on a row.
fn unhide_width(ui: &egui::Ui) -> f32 {
    ui.painter()
        .layout_no_wrap(
            aurora_i18n::tr!("hidefriend-unhide-short").to_owned(),
            FontId::new(12.5, crate::ui::fonts::semibold(ui.ctx())),
            egui::Color32::PLACEHOLDER,
        )
        .size()
        .x
        + 20.0
}

fn row(ui: &mut egui::Ui, c: &mut Ctx, st: &PeopleUi, id: Uuid, name: &str) {
    let p = c.p;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), RICH_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let hot = ui.rect_contains_pointer(rect) || st.demo_hover == Some(id);
    rows::face(ui, p, rect, false, hot);
    let disc = Rect::from_center_size(Pos2::new(rect.left() + 8.0 + 15.0, rect.center().y), Vec2::splat(30.0));
    widgets::avatar_disc(ui, p, disc, None, name, false);
    let left = rows::names_left(rect);
    let widths: Vec<f32> = if hot { vec![unhide_width(ui)] } else { Vec::new() };
    let end = rows::end(rect, left, &widths, 6.0, true);
    if hot && let Some(cell) = end.items.first() {
        let r = Rect::from_center_size(cell.center(), Vec2::new(cell.width(), 26.0));
        // `place`, not `put`: the row is already laid out (as in Blocked)
        let button = ui.place(
            r,
            egui::Button::new(RichText::new(aurora_i18n::tr!("hidefriend-unhide-short")).size(12.5)),
        );
        if button.clicked() {
            unhide(ui, id);
        }
    }
    rows::names(
        ui,
        p,
        rect,
        left,
        end.names_right,
        name,
        p.ink,
        aurora_i18n::tr!("hidefriend-row-sub"),
        None,
        None,
    );
    row_menu(&resp, p, id, c.actions);
}

/// The Legacy look's table: the names and an "Un-hide" button each.
pub(super) fn legacy_tab(ui: &mut egui::Ui, p: &Palette, world: &World, filter: &str, show: &mut bool, actions: &mut Vec<PeopleAction>) {
    ui.label(RichText::new(aurora_i18n::tr!("hidefriend-list-hint")).size(11.5).color(p.muted));
    ui.add_space(4.0);
    show_switch(ui, p, show);
    ui.add_space(4.0);
    let list = entries(world, filter);
    let id = ui.scope_id().with("hidden_friends_list");
    let w = widgets::header_width(ui, id);
    let title = aurora_i18n::tr!("ui-people-name", value0 = format!("{}", count(world)));
    widgets::header_row(ui, p, &[(&title, w - 110.0), ("", 110.0)]);
    let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if list.is_empty() {
            ui.label(RichText::new(aurora_i18n::tr!("hidefriend-list-empty")).size(12.0).color(p.muted));
        }
        for (friend, name) in &list {
            ui.horizontal(|ui| {
                ui.set_min_height(20.0);
                let name_w = (ui.available_width() - 120.0).max(60.0);
                let r = ui
                    .allocate_ui_with_layout(Vec2::new(name_w, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(name).size(13.0).color(p.ink))
                                .truncate()
                                .sense(Sense::click()),
                        )
                    })
                    .inner;
                row_menu(&r, p, *friend, actions);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::flat_button(ui, p, aurora_i18n::tr!("hidefriend-unhide-short")).clicked() {
                        unhide(ui, *friend);
                    }
                });
            });
        }
    });
    widgets::remember_scroll_bar(ui, id, &out);
}

/// The hidden friends under the friends, greyed, with their own header
/// (People › Hidden's switch, Aurora's look). Still strangers: their rows
/// are this tab's, with "Un-hide" and the profile only.
pub(super) fn greyed_rows(ui: &mut egui::Ui, c: &mut Ctx, st: &PeopleUi, filter: &str) {
    let list = entries(c.world, filter);
    if list.is_empty() {
        return;
    }
    super::nearby::section_header(ui, c.p, &aurora_i18n::tr!("b6-hidden-section", count = list.len() as i64));
    let opacity = ui.opacity();
    ui.set_opacity(opacity * GREYED);
    for (id, name) in &list {
        row(ui, c, st, *id, name);
    }
    ui.set_opacity(opacity);
}

/// The same in the Legacy look's friends list.
pub(super) fn legacy_greyed(ui: &mut egui::Ui, p: &Palette, world: &World, filter: &str, actions: &mut Vec<PeopleAction>) {
    for (friend, name) in entries(world, filter) {
        ui.horizontal(|ui| {
            ui.set_min_height(22.0);
            let r = ui.add(
                egui::Label::new(RichText::new(&name).size(13.0).color(p.muted.gamma_multiply(GREYED)))
                    .truncate()
                    .sense(Sense::click()),
            );
            row_menu(&r, p, friend, actions);
            r.on_hover_text(aurora_i18n::tr!("b6-hidden-status"));
        });
    }
}

/// How much a hidden friend's row is faded in the friends list.
const GREYED: f32 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::social::Friend;

    fn world_with(friends: &[(u128, &str)]) -> World {
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        for (n, name) in friends {
            let id = Uuid::from_u128(*n);
            world.social.friends.push(Friend {
                id,
                online: false,
                rights_given: 0,
                rights_has: 0,
            });
            world.social.names.insert(id, (*name).to_owned());
        }
        world
    }

    #[test]
    fn the_list_names_the_hidden_friends_only_and_un_hiding_empties_it() {
        let mut world = world_with(&[(1, "Zoe Young"), (2, "Adam Old"), (3, "Mia Shown")]);
        assert!(entries(&world, "").is_empty());
        assert!(world.hide_friend(Uuid::from_u128(1)));
        assert!(world.hide_friend(Uuid::from_u128(2)));
        let names: Vec<String> = entries(&world, "").into_iter().map(|(_, n)| n).collect();
        assert_eq!(names, ["Adam Old", "Zoe Young"], "by name, without the friend still shown");
        assert_eq!(count(&world), 2);
        let found: Vec<Uuid> = entries(&world, "zoe").into_iter().map(|(id, _)| id).collect();
        assert_eq!(found, [Uuid::from_u128(1)]);
        assert!(world.unhide_friend(Uuid::from_u128(1)));
        assert!(world.unhide_friend(Uuid::from_u128(2)));
        assert!(entries(&world, "").is_empty());
        assert_eq!(world.social.shown_friends().count(), 3);
    }
}
