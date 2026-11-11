//! "Favorite wearables": a quick-wear list of the starred clothing, body
//! parts and attachments (the toolbar's wearable_favorites, Me › Favorite
//! wearables). The Legacy Viewer keeps its own "#Wearable Favorites" folder
//! of links (FSFloaterWearableFavorites, indra/newview/
//! fsfloaterwearablefavorites.cpp, floater_fs_wearable_favorites.xml,
//! originally LGPL 2.1); Aurora lists the inventory's own favourites
//! instead (`world::inventory::actions::is_favorite`, the stars of the
//! Favorites tab), so no folder is made and nothing moves in the inventory.
//! A link lists its original once. The buttons wear, add, take off and
//! unstar, with the inventory's rules: the same greying under RLVa locks
//! (`Facts::rlva`), no Add for a body part or the physics layer, no Take
//! off for a body part, nothing worn from the marketplace listings, and
//! nothing while an outfit change is pending. A double click toggles as
//! FSFloaterWearableFavorites::onDoubleClick does.

use super::inventory::Facts;
use super::widgets::{self, ActionKind, Floater};
use crate::theme::Palette;
use crate::world::appearance::Action as Wear;
use crate::world::inventory::{Inventory, actions as rules};
use egui::{FontId, Rect, Sense, Vec2};
use uuid::Uuid;

/// The window's id (Floater.Toggle "fs_wearable_favorites").
pub const WINDOW: &str = "wearable_favorites";

const ROW_H: f32 = 24.0;
const BUTTON_H: f32 = 26.0;

/// One wearable of the list: its original item and the items that carry
/// its star (the original, or links with a star of their own).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub item: Uuid,
    pub starred: Vec<Uuid>,
    pub name: String,
    pub asset_type: i32,
    pub inv_type: i32,
    pub flags: u32,
}

/// What the window asks the app for.
#[derive(Debug, Clone)]
pub enum FavoriteAction {
    /// Wear, add or take off, through the inventory's appearance path.
    Appearance(Wear),
    /// Take the star off these items (`actions::set_favorites`, false).
    Unstar(Vec<Uuid>),
}

/// The buttons a selected wearable allows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Allowed {
    pub wear: bool,
    pub add: bool,
    pub take_off: bool,
}

/// The starred wearables of `inv`, by name: every item with a star of its
/// own, or a link with one, resolved to its original clothing (5), object
/// (6) or body part (13); nothing from the Trash or the Library.
pub fn entries(inv: &Inventory) -> Vec<Entry> {
    let mut by_item: std::collections::HashMap<Uuid, Entry> = std::collections::HashMap::new();
    for it in inv.items.values().filter(|it| it.favorite) {
        let Some(original) = rules::original(inv, it.id).and_then(|id| inv.items.get(&id)) else {
            continue;
        };
        if !matches!(original.asset_type, 5 | 6 | 13)
            || [it.id, original.id]
                .iter()
                .any(|id| rules::library(inv, *id) || rules::in_type(inv, *id, 14))
        {
            continue;
        }
        let entry = by_item.entry(original.id).or_insert_with(|| Entry {
            item: original.id,
            starred: Vec::new(),
            name: original.name.clone(),
            asset_type: original.asset_type,
            inv_type: original.inv_type,
            flags: original.flags,
        });
        entry.starred.push(it.id);
    }
    let mut out: Vec<Entry> = by_item.into_values().collect();
    for entry in &mut out {
        entry.starred.sort();
    }
    out.sort_by_cached_key(|e| (crate::natural_order::key(&e.name), e.item));
    out
}

/// The wear rules of the inventory's menus (context_catalog's "wear",
/// "add" and "detach" entries, details' `can_add`): Wear replaces what is
/// worn in the slot, Add keeps it (never a body part or the physics layer,
/// LLWearableType::getAllowMultiwear), Take off for worn clothing and
/// objects (a body part can only be replaced); each greyed where an RLVa
/// lock refuses it, and all while an outfit change is pending.
pub fn allowed(inv: &Inventory, item: Uuid, facts: &Facts) -> Allowed {
    let Some(it) = inv.items.get(&item) else {
        return Allowed::default();
    };
    if facts.appearance_busy || !matches!(it.asset_type, 5 | 6 | 13) {
        return Allowed::default();
    }
    if facts.worn.contains(&item) {
        return Allowed {
            take_off: it.asset_type != 13 && facts.rlva.can_remove(item),
            ..Allowed::default()
        };
    }
    // AlertMerchantListingCannotWear: nothing is worn from the listings
    let listed = crate::world::marketplace::root(inv).is_some_and(|root| rules::under(inv, item, root));
    if listed || rules::library(inv, item) || rules::in_type(inv, item, 14) {
        return Allowed::default();
    }
    let lock = facts.rlva.wear(inv, item, 0);
    let physics = it.asset_type == 5 && it.flags & 0xff == 15;
    Allowed {
        wear: lock.replace,
        add: lock.add && it.asset_type != 13 && !physics,
        take_off: false,
    }
}

/// FSFloaterWearableFavorites::onDoubleClick: a worn item comes off, a body
/// part replaces, clothing and objects are added; nothing a lock refuses.
pub fn double_click(inv: &Inventory, item: Uuid, facts: &Facts) -> Option<Wear> {
    let can = allowed(inv, item, facts);
    let kind = inv.items.get(&item)?.asset_type;
    if facts.worn.contains(&item) {
        return can.take_off.then_some(Wear::RemoveItems(vec![item]));
    }
    let replace = kind == 13 || !can.add;
    (if replace { can.wear } else { can.add }).then_some(Wear::WearItem { item, replace, point: 0 })
}

#[derive(Default)]
pub struct WearableFavoritesUi {
    filter: String,
    selected: Option<Uuid>,
    /// The list and the inventory generation it was made from.
    cache: Option<(u64, Vec<Entry>)>,
}

impl WearableFavoritesUi {
    fn list(&mut self, inv: &Inventory) -> &[Entry] {
        if self.cache.as_ref().is_none_or(|(generation, _)| *generation != inv.generation) {
            self.cache = Some((inv.generation, entries(inv)));
        }
        self.cache.as_ref().map_or(&[], |(_, list)| list.as_slice())
    }

    /// Draw the window while `open`.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, inv: &Inventory, facts: &Facts, open: &mut bool) -> Vec<FavoriteAction> {
        let mut actions = Vec::new();
        if !*open {
            return actions;
        }
        let filter = self.filter.trim().to_lowercase();
        let shown: Vec<Entry> = self
            .list(inv)
            .iter()
            .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter))
            .cloned()
            .collect();
        if self.selected.is_some_and(|id| !shown.iter().any(|e| e.item == id)) {
            self.selected = None;
        }
        let mut floater = Floater::new(
            WINDOW,
            aurora_i18n::tr!("toolbar-command-wearable-favorites"),
            egui::pos2(320.0, 170.0),
            Vec2::new(330.0, 390.0),
        );
        floater.min_size = Vec2::new(260.0, 220.0);
        floater.show(ctx, p, open, |ui| {
            let w = ui.available_width();
            ui.allocate_ui_with_layout(Vec2::new(w, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                widgets::clearable_search_box(ui, p, &mut self.filter, aurora_i18n::tr!("fpol-favwear-filter"), w);
            });
            ui.add_space(6.0);
            let (mut list, footer) = widgets::above_footer(ui);
            egui::ScrollArea::vertical()
                .id_salt("wearable_favorites_rows")
                .auto_shrink([false, false])
                .show(&mut list, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    if shown.is_empty() {
                        let text = if self.filter.trim().is_empty() {
                            aurora_i18n::tr!("fpol-favwear-empty")
                        } else {
                            aurora_i18n::tr!("fpol-favwear-no-match")
                        };
                        ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(p.muted)).wrap());
                    }
                    for entry in &shown {
                        self.row(ui, p, entry, inv, facts, &mut actions);
                    }
                });
            footer.show(ui, |ui| self.footer(ui, p, &shown, inv, facts, &mut actions));
        });
        actions
    }

    fn row(&mut self, ui: &mut egui::Ui, p: &Palette, entry: &Entry, inv: &Inventory, facts: &Facts, actions: &mut Vec<FavoriteAction>) {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
        let selected = self.selected == Some(entry.item);
        widgets::reveal_search_selection(ui, &resp, selected);
        if resp.clicked() || resp.secondary_clicked() {
            self.selected = Some(entry.item);
        }
        if resp.double_clicked()
            && let Some(wear) = double_click(inv, entry.item, facts)
        {
            actions.push(FavoriteAction::Appearance(wear));
        }
        if !ui.is_rect_visible(rect) {
            return;
        }
        let radius = ui.visuals().window_corner_radius;
        if selected {
            ui.painter().rect_filled(rect, radius, p.violet.gamma_multiply(0.30));
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, radius, p.raised.gamma_multiply(0.6));
        }
        let icon = super::inventory::item_icon(entry.asset_type, entry.inv_type, entry.flags);
        if let Some(t) = super::icons::global(icon) {
            ui.painter().image(
                t.id(),
                Rect::from_center_size(egui::pos2(rect.left() + 13.0, rect.center().y), Vec2::splat(16.0)),
                Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                super::inventory::item_icon_color(entry.asset_type, entry.inv_type, p),
            );
        }
        let worn = facts.worn.contains(&entry.item).then(|| {
            facts
                .worn_labels
                .get(&entry.item)
                .map(String::as_str)
                .unwrap_or(aurora_i18n::tr!("common-worn"))
        });
        let tag = worn.map_or(0.0, |label| widgets::row_tag_width(ui, label) + 8.0);
        let left = rect.left() + 28.0;
        let font = if worn.is_some() {
            FontId::new(12.5, crate::ui::fonts::semibold(ui.ctx()))
        } else {
            FontId::proportional(12.5)
        };
        let name_w = (rect.right() - left - tag - 6.0).max(0.0);
        widgets::paint_elided(ui, &entry.name, egui::pos2(left, rect.center().y), name_w, font, p.ink);
        if let Some(label) = worn {
            widgets::paint_row_tag(ui, p, label, true, rect.right() - tag + 2.0, rect.center().y);
        }
    }

    fn footer(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        shown: &[Entry],
        inv: &Inventory,
        facts: &Facts,
        actions: &mut Vec<FavoriteAction>,
    ) {
        let entry = self.selected.and_then(|id| shown.iter().find(|e| e.item == id));
        let can = entry.map_or_else(Allowed::default, |e| allowed(inv, e.item, facts));
        let size = Vec2::new(0.0, BUTTON_H);
        ui.horizontal_wrapped(|ui| {
            let Some(entry) = entry else {
                for (icon, label) in [
                    ("shirt-filled", aurora_i18n::tr!("common-wear")),
                    ("plus", aurora_i18n::tr!("common-add")),
                    ("circle-x-filled", aurora_i18n::tr!("inventory-take-off")),
                    ("star-off", aurora_i18n::tr!("fpol-favwear-unstar")),
                ] {
                    widgets::action_button_if(ui, p, icon, label, ActionKind::Flat, size, false);
                }
                return;
            };
            let item = entry.item;
            if widgets::action_button_if(
                ui,
                p,
                "shirt-filled",
                aurora_i18n::tr!("common-wear"),
                ActionKind::Flat,
                size,
                can.wear,
            )
            .on_hover_text(aurora_i18n::tr!("fpol-favwear-wear-tip"))
            .clicked()
            {
                actions.push(FavoriteAction::Appearance(Wear::WearItem {
                    item,
                    replace: true,
                    point: 0,
                }));
            }
            if widgets::action_button_if(ui, p, "plus", aurora_i18n::tr!("common-add"), ActionKind::Flat, size, can.add)
                .on_hover_text(aurora_i18n::tr!("fpol-favwear-add-tip"))
                .clicked()
            {
                actions.push(FavoriteAction::Appearance(Wear::WearItems {
                    items: vec![item],
                    replace: false,
                    point: 0,
                }));
            }
            if widgets::action_button_if(
                ui,
                p,
                "circle-x-filled",
                aurora_i18n::tr!("inventory-take-off"),
                ActionKind::Flat,
                size,
                can.take_off,
            )
            .clicked()
            {
                actions.push(FavoriteAction::Appearance(Wear::RemoveItems(vec![item])));
            }
            if widgets::action_button_if(
                ui,
                p,
                "star-off",
                aurora_i18n::tr!("fpol-favwear-unstar"),
                ActionKind::Flat,
                size,
                true,
            )
            .on_hover_text(aurora_i18n::tr!("fpol-favwear-unstar-tip"))
            .clicked()
            {
                let mut ids = entry.starred.clone();
                ids.push(item);
                actions.push(FavoriteAction::Unstar(ids));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::inventory::{FetchState, Folder};
    use aurora_net::inventory::{InvFolder, InvItem};
    use std::collections::HashSet;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    const ROOT: u128 = 1;
    const TRASH: u128 = 2;
    const CLOTHES: u128 = 3;
    const LISTINGS: u128 = 4;
    const LIB: u128 = 5;

    fn folder(inv: &mut Inventory, n: u128, parent: Uuid, kind: i32, library: bool) {
        inv.folders.insert(
            id(n),
            Folder {
                info: InvFolder {
                    id: id(n),
                    parent,
                    name: format!("folder {n}"),
                    type_default: kind,
                    ..Default::default()
                },
                items: vec![],
                children: vec![],
                state: FetchState::Fetched,
                library,
            },
        );
        if let Some(f) = inv.folders.get_mut(&parent) {
            f.children.push(id(n));
        }
    }

    fn item(inv: &mut Inventory, n: u128, parent: u128, asset_type: i32, flags: u32, name: &str, favorite: bool) {
        inv.items.insert(
            id(n),
            InvItem {
                id: id(n),
                parent: id(parent),
                name: name.into(),
                asset_type,
                flags,
                favorite,
                owner_mask: 0x7fff_ffff,
                ..Default::default()
            },
        );
        if let Some(f) = inv.folders.get_mut(&id(parent)) {
            f.items.push(id(n));
        }
    }

    fn link(inv: &mut Inventory, n: u128, parent: u128, target: u128, favorite: bool) {
        item(inv, n, parent, 24, 0, "link", favorite);
        inv.items.get_mut(&id(n)).expect("link").asset_id = id(target);
    }

    fn fixture() -> Inventory {
        let mut inv = Inventory {
            root: id(ROOT),
            lib_root: id(LIB),
            ..Default::default()
        };
        folder(&mut inv, ROOT, Uuid::nil(), 8, false);
        folder(&mut inv, TRASH, id(ROOT), 14, false);
        folder(&mut inv, CLOTHES, id(ROOT), -1, false);
        folder(&mut inv, LISTINGS, id(ROOT), 53, false);
        folder(&mut inv, LIB, Uuid::nil(), 8, true);
        item(&mut inv, 10, CLOTHES, 5, 4, "Shirt", true);
        item(&mut inv, 11, CLOTHES, 13, 0, "Shape", true);
        item(&mut inv, 12, CLOTHES, 6, 0, "Hat", false);
        link(&mut inv, 13, ROOT, 12, true); // the hat starred through a link
        link(&mut inv, 14, ROOT, 10, true); // a second star on the shirt
        item(&mut inv, 15, CLOTHES, 7, 0, "Starred notecard", true);
        item(&mut inv, 16, TRASH, 5, 4, "Trashed shirt", true);
        item(&mut inv, 17, LIB, 5, 4, "Library shirt", true);
        item(&mut inv, 18, CLOTHES, 5, 15, "Physics", true);
        item(&mut inv, 19, LISTINGS, 5, 5, "Listed pants", true);
        item(&mut inv, 20, CLOTHES, 5, 5, "Plain pants", false);
        inv.generation += 1;
        inv
    }

    #[test]
    fn the_list_holds_starred_wearables_once_by_their_originals() {
        let inv = fixture();
        let list = entries(&inv);
        let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Hat", "Listed pants", "Physics", "Shape", "Shirt"]);
        let hat = list.iter().find(|e| e.item == id(12)).expect("hat");
        assert_eq!(hat.starred, vec![id(13)], "a link's own star lists its original");
        let shirt = list.iter().find(|e| e.item == id(10)).expect("shirt");
        assert_eq!(shirt.starred, vec![id(10), id(14)], "one row, both stars kept for Remove");
    }

    #[test]
    fn the_buttons_follow_the_inventory_wear_rules() {
        let inv = fixture();
        let mut facts = Facts::default();
        let none = Allowed::default();
        assert_eq!(
            allowed(&inv, id(10), &facts),
            Allowed {
                wear: true,
                add: true,
                take_off: false
            }
        );
        assert_eq!(
            allowed(&inv, id(11), &facts),
            Allowed {
                wear: true,
                add: false,
                take_off: false
            },
            "a body part is never added"
        );
        assert!(!allowed(&inv, id(18), &facts).add, "the physics layer is never added");
        assert_eq!(allowed(&inv, id(19), &facts), none, "nothing is worn from the listings");
        assert_eq!(allowed(&inv, id(15), &facts), none, "not a wearable");
        facts.worn = HashSet::from([id(10), id(11)]);
        assert_eq!(allowed(&inv, id(10), &facts), Allowed { take_off: true, ..none });
        assert_eq!(allowed(&inv, id(11), &facts), none, "a worn body part can only be replaced");
        facts.appearance_busy = true;
        assert_eq!(allowed(&inv, id(12), &facts), none, "nothing while an outfit change runs");
    }

    /// The wear action a double click asks for, as (item, replace) or a
    /// removal (`Wear` has no equality).
    fn clicked(inv: &Inventory, item: Uuid, facts: &Facts) -> Option<(Vec<Uuid>, Option<bool>)> {
        match double_click(inv, item, facts)? {
            Wear::WearItem { item, replace, point: 0 } => Some((vec![item], Some(replace))),
            Wear::RemoveItems(items) => Some((items, None)),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_double_click_toggles_as_the_legacy_list_does() {
        let inv = fixture();
        let mut facts = Facts::default();
        assert_eq!(
            clicked(&inv, id(12), &facts),
            Some((vec![id(12)], Some(false))),
            "an object is added"
        );
        assert_eq!(
            clicked(&inv, id(11), &facts),
            Some((vec![id(11)], Some(true))),
            "a body part replaces"
        );
        assert_eq!(
            clicked(&inv, id(18), &facts),
            Some((vec![id(18)], Some(true))),
            "the physics layer replaces"
        );
        facts.worn = HashSet::from([id(12), id(11)]);
        assert_eq!(clicked(&inv, id(12), &facts), Some((vec![id(12)], None)), "a worn object comes off");
        assert_eq!(clicked(&inv, id(11), &facts), None, "a worn body part stays");
    }

    /// RLVa locks grey the buttons as in the inventory (`WearLocks`).
    #[test]
    fn rlva_locks_win_over_the_buttons() {
        use aurora_net::{ChatMessage, ChatSourceType, ChatType};
        let mut w = crate::world::World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        w.inventory = fixture();
        folder(&mut w.inventory, 6, id(ROOT), 46, false);
        link(&mut w.inventory, 30, 6, 10, false); // the shirt is worn
        w.rlva.configure(true);
        let message = ChatMessage {
            from_name: "Lock".to_owned(),
            source_id: id(900),
            owner_id: id(901),
            source_type: ChatSourceType::Object,
            chat_type: ChatType::OwnerSay,
            position: glam::Vec3::ZERO,
            message: "@remoutfit:shirt=n,addoutfit:pants=n".to_owned(),
        };
        assert!(w.rlva.receive(&message));
        let facts = Facts {
            worn: HashSet::from([id(10)]),
            rlva: w.rlva_wear_locks(),
            ..Default::default()
        };
        assert!(!allowed(&w.inventory, id(10), &facts).take_off, "@remoutfit:shirt");
        assert!(double_click(&w.inventory, id(10), &facts).is_none());
        item(&mut w.inventory, 31, CLOTHES, 5, 5, "Locked pants", true);
        let facts = Facts {
            rlva: w.rlva_wear_locks(),
            ..facts
        };
        assert_eq!(allowed(&w.inventory, id(31), &facts), Allowed::default(), "@addoutfit:pants");
        assert!(double_click(&w.inventory, id(31), &facts).is_none());
    }
}
