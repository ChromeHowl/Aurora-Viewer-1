//! Inventory thumbnails helper: item images set in bulk by name. Port of
//! LLFloaterInventoryThumbnailsHelper (indra/newview/
//! llfloaterinventorythumbnailshelper.cpp, originally LGPL 2.1): objects,
//! clothing and body parts are pasted from the inventory clipboard, then
//! textures; an item whose name matches a texture's gets that texture as its
//! image, through the same AIS metadata patch as the single item editor.
//!
//! Stricter than the Legacy Viewer: a texture is taken only when the item
//! image rule allows it (`thumbnail::usable`: copy and transfer, or the
//! Library), an item only when it may be changed (`rules::mutable`), and the
//! log names textures without their asset ids.

use super::*;
use crate::ui::widgets::{Floater, Modal, flat_button, primary_button_sized};
use aurora_net::inventory::operations::metadata_thumbnail;
use std::collections::BTreeMap;

/// The item kinds the helper takes from a paste (LLIsType filters of
/// onPasteItems): objects, clothing and body parts.
const ITEM_KINDS: [i32; 3] = [6, 5, 13];
/// Textures (onPasteTextures).
const TEXTURE_KINDS: [i32; 1] = [0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Confirm {
    Write,
    Clear,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    /// Name → item, the first one of a name kept (recordInventoryItemEntry).
    items: BTreeMap<String, Uuid>,
    /// Name → texture item, the first one of a name kept.
    textures: BTreeMap<String, Uuid>,
    log: Vec<String>,
    confirm: Option<Confirm>,
}

/// The items of `kinds` among `roots`: an item itself, or what a folder
/// holds at any depth, the Trash left out (collectDescendentsIf with
/// EXCLUDE_TRASH). Only loaded folders are walked: the count of those still
/// loading is returned so the log can say so.
pub(super) fn collect(inv: &Inventory, roots: &[Uuid], kinds: &[i32]) -> (Vec<Uuid>, usize) {
    let mut found = Vec::new();
    let mut loading = 0;
    let mut seen = HashSet::new();
    let mut todo: Vec<Uuid> = roots.to_vec();
    while let Some(id) = todo.pop() {
        if !seen.insert(id) || rules::in_type(inv, id, 14) {
            continue;
        }
        if let Some(folder) = inv.folders.get(&id) {
            if folder.state != FetchState::Fetched {
                loading += 1;
            }
            todo.extend(folder.children.iter().chain(&folder.items).copied());
        } else if let Some(item) = inv.items.get(&id)
            && kinds.contains(&item.asset_type)
        {
            found.push(id);
        }
    }
    found.sort_by_key(|id| inv.items.get(id).map(|it| it.created_at));
    (found, loading)
}

/// Adds `ids` under their names; a name already listed keeps its first
/// entry. Returns the names added, in order.
fn record(inv: &Inventory, ids: &[Uuid], into: &mut BTreeMap<String, Uuid>) -> Vec<String> {
    let mut added = Vec::new();
    for id in ids {
        let Some(item) = inv.items.get(id) else { continue };
        if !into.contains_key(&item.name) {
            into.insert(item.name.clone(), *id);
            added.push(item.name.clone());
        }
    }
    added
}

/// The AIS patches of "Write thumbnails" (`clear` false) or "Clear
/// thumbnails": each listed item that may be changed gets its namesake
/// texture's asset (or none). Textures the item image rule refuses, and
/// items that may not be changed, are skipped; their names are returned.
pub(super) fn plan(
    inv: &Inventory,
    items: &BTreeMap<String, Uuid>,
    textures: &BTreeMap<String, Uuid>,
    clear: bool,
) -> (Mutation, Vec<String>) {
    let mut change = Mutation::default();
    let mut skipped = Vec::new();
    for (name, item) in items {
        let asset = if clear {
            Uuid::nil()
        } else {
            let Some(texture) = textures.get(name) else { continue };
            match inv.items.get(texture).filter(|it| super::thumbnail::usable(inv, it)) {
                Some(it) => it.asset_id,
                None => {
                    skipped.push(name.clone());
                    continue;
                }
            }
        };
        let Some(current) = inv.items.get(item) else { continue };
        if current.thumbnail == asset || !rules::mutable(inv, *item) {
            if current.thumbnail != asset {
                skipped.push(name.clone());
            }
            continue;
        }
        match rules::patch(inv, *item, metadata_thumbnail(asset)) {
            Ok(update) => {
                change.operations.extend(update.operations);
                change.refresh.extend(update.refresh);
            }
            Err(_) => skipped.push(name.clone()),
        }
    }
    change.refresh.sort_unstable();
    change.refresh.dedup();
    (change, skipped)
}

/// The names of the listed items without an image (onLogMissingThumbnails).
pub(super) fn missing(inv: &Inventory, items: &BTreeMap<String, Uuid>) -> Vec<String> {
    items
        .iter()
        .filter(|(_, id)| inv.items.get(id).is_some_and(|it| it.thumbnail.is_nil()))
        .map(|(name, _)| name.clone())
        .collect()
}

impl State {
    fn paste(&mut self, inv: &Inventory, clipboard: &[Uuid], textures: bool) {
        let (kinds, into): (&[i32], _) = if textures {
            (&TEXTURE_KINDS, &mut self.textures)
        } else {
            (&ITEM_KINDS, &mut self.items)
        };
        let (ids, loading) = collect(inv, clipboard, kinds);
        self.log.push(if textures {
            aurora_i18n::tr!("l3-thumbs-log-paste-textures").to_owned()
        } else {
            aurora_i18n::tr!("l3-thumbs-log-paste-items").to_owned()
        });
        for name in record(inv, &ids, into) {
            let n = into.len().to_string();
            self.log.push(if textures {
                aurora_i18n::tr!("l3-thumbs-log-texture", n = n.as_str(), name = name.as_str())
            } else {
                aurora_i18n::tr!("l3-thumbs-log-item", n = n.as_str(), name = name.as_str())
            });
        }
        if loading > 0 {
            let n = loading.to_string();
            self.log.push(aurora_i18n::tr!("l3-thumbs-log-loading", n = n.as_str()));
        }
    }
}

pub(super) fn show(ctx: &egui::Context, p: &Palette, inv: &Inventory, st: &mut InventoryUi, actions: &mut Vec<InvAction>) {
    if !st.thumbnails_helper.open {
        return;
    }
    let busy = st.pending.is_some() || actions.iter().any(|a| matches!(a, InvAction::Edit(_)));
    let clipboard = st.clipboard.clone();
    let state = &mut st.thumbnails_helper;
    let mut open = true;
    let size = Vec2::new(560.0, 520.0);
    Floater::new(
        &format!("inventory_thumbnails_helper_{}", st.ui_id),
        aurora_i18n::tr!("l3-thumbs-title"),
        ctx.content_rect().center() - size / 2.0,
        size,
    )
    .show(ctx, p, &mut open, |ui| {
        ui.label(RichText::new(aurora_i18n::tr!("l3-thumbs-help")).size(12.0).color(p.muted));
        ui.add_space(6.0);
        let width = ui.available_width();
        egui::ScrollArea::vertical()
            .id_salt("thumbs_list")
            .max_height(220.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("thumbs_grid")
                    .num_columns(3)
                    .min_col_width(width / 3.0 - 12.0)
                    .striped(false)
                    .show(ui, |ui| {
                        for title in [
                            aurora_i18n::tr!("l3-thumbs-col-item"),
                            aurora_i18n::tr!("l3-thumbs-col-current"),
                            aurora_i18n::tr!("l3-thumbs-col-new"),
                        ] {
                            ui.label(RichText::new(title).color(p.muted));
                        }
                        ui.end_row();
                        for (name, id) in &state.items {
                            ui.label(RichText::new(name).color(p.ink));
                            let has = inv.items.get(id).is_some_and(|it| !it.thumbnail.is_nil());
                            ui.label(
                                RichText::new(if has {
                                    aurora_i18n::tr!("l3-thumbs-has-image")
                                } else {
                                    aurora_i18n::tr!("common-none")
                                })
                                .color(p.muted),
                            );
                            let new = if state.textures.contains_key(name) {
                                name.as_str()
                            } else {
                                aurora_i18n::tr!("l3-thumbs-missing")
                            };
                            ui.label(RichText::new(new).color(p.ink));
                            ui.end_row();
                        }
                    });
            });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let can_paste = !clipboard.is_empty();
            if ui
                .add_enabled_ui(can_paste, |ui| flat_button(ui, p, aurora_i18n::tr!("l3-thumbs-paste-items")))
                .inner
                .clicked()
            {
                state.paste(inv, &clipboard, false);
            }
            if ui
                .add_enabled_ui(can_paste, |ui| flat_button(ui, p, aurora_i18n::tr!("l3-thumbs-paste-textures")))
                .inner
                .clicked()
            {
                state.paste(inv, &clipboard, true);
            }
            let has_items = !state.items.is_empty();
            if ui
                .add_enabled_ui(has_items && !state.textures.is_empty() && !busy, |ui| {
                    flat_button(ui, p, aurora_i18n::tr!("l3-thumbs-write"))
                })
                .inner
                .clicked()
            {
                state.confirm = Some(Confirm::Write);
            }
            if ui
                .add_enabled_ui(has_items, |ui| flat_button(ui, p, aurora_i18n::tr!("l3-thumbs-log-missing")))
                .inner
                .clicked()
            {
                for name in missing(inv, &state.items) {
                    state.log.push(aurora_i18n::tr!("l3-thumbs-log-missing-line", name = name.as_str()));
                }
            }
            if ui
                .add_enabled_ui(has_items && !busy, |ui| flat_button(ui, p, aurora_i18n::tr!("l3-thumbs-clear")))
                .inner
                .clicked()
            {
                state.confirm = Some(Confirm::Clear);
            }
            if flat_button(ui, p, aurora_i18n::tr!("l3-thumbs-reset")).clicked() {
                state.items.clear();
                state.textures.clear();
                state.log.clear();
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("thumbs_log")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for line in &state.log {
                    ui.label(RichText::new(line).monospace().size(12.0).color(p.ink));
                }
            });
    });
    if let Some(confirm) = state.confirm {
        let mut answer = None;
        let response = Modal::new(egui::Id::unique(("inventory_thumbnails_confirm", st.ui_id))).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new(aurora_i18n::tr!("l3-thumbs-title")).strong().size(17.0));
            ui.label(match confirm {
                Confirm::Write => aurora_i18n::tr!("l3-thumbs-write-warning"),
                Confirm::Clear => aurora_i18n::tr!("l3-thumbs-clear-warning"),
            });
            ui.horizontal(|ui| {
                if flat_button(ui, p, aurora_i18n::tr!("common-cancel")).clicked() {
                    answer = Some(false);
                }
                if primary_button_sized(ui, p, aurora_i18n::tr!("common-ok"), Vec2::new(120.0, 30.0)).clicked() {
                    answer = Some(true);
                }
            });
        });
        if response.should_close() && answer.is_none() {
            answer = Some(false);
        }
        if let Some(yes) = answer {
            state.confirm = None;
            if yes && !busy {
                let (change, skipped) = plan(inv, &state.items, &state.textures, confirm == Confirm::Clear);
                for name in skipped {
                    state.log.push(aurora_i18n::tr!("l3-thumbs-log-refused", name = name.as_str()));
                }
                let n = change.operations.len().to_string();
                state.log.push(match confirm {
                    Confirm::Write => aurora_i18n::tr!("l3-thumbs-log-writing", n = n.as_str()),
                    Confirm::Clear => aurora_i18n::tr!("l3-thumbs-log-clearing", n = n.as_str()),
                });
                if !change.operations.is_empty() {
                    actions.push(InvAction::Edit(change));
                }
            }
        }
    }
    state.open = open;
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_net::inventory::InvItem;
    use aurora_net::inventory::operations::Operation;

    const AGENT: Uuid = Uuid::from_u128(2);

    fn fixture() -> (Inventory, Uuid) {
        let mut inv = Inventory::default();
        crate::world::inventory::demo::seed(&mut inv, AGENT);
        let folder = inv
            .folders
            .values()
            .find(|f| f.info.type_default == -1 && !f.library && rules::mutable(&inv, f.info.id) && !rules::in_type(&inv, f.info.id, 14))
            .map(|f| f.info.id)
            .expect("an ordinary folder");
        (inv, folder)
    }

    fn item(inv: &mut Inventory, n: u128, parent: Uuid, name: &str, asset_type: i32, owner_mask: u32) -> Uuid {
        let id = Uuid::from_u128(0x7700_0000 + n);
        inv.add_items(vec![InvItem {
            id,
            parent,
            name: name.into(),
            asset_type,
            inv_type: if asset_type == 0 { 0 } else { 6 },
            asset_id: Uuid::from_u128(0x7800_0000 + n),
            base_mask: rules::FULL | 0x80000,
            owner_mask,
            next_owner_mask: rules::FULL,
            owner: AGENT,
            creator: AGENT,
            ..Default::default()
        }]);
        id
    }

    fn written(change: &Mutation) -> Vec<(Uuid, Uuid)> {
        change
            .operations
            .iter()
            .map(|op| match op {
                Operation::Patch { id, body, .. } => {
                    let thumb = &body["thumbnail"];
                    let asset = if thumb.has("asset_id") {
                        thumb["asset_id"].to_string_value().parse().expect("uuid")
                    } else {
                        Uuid::nil()
                    };
                    (*id, asset)
                }
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    #[test]
    fn paste_keeps_kinds_first_names_and_skips_the_trash() {
        let (mut inv, folder) = fixture();
        let hat = item(&mut inv, 1, folder, "Hat", 6, rules::FULL);
        let _dupe = item(&mut inv, 2, folder, "Hat", 13, rules::FULL);
        let _note = item(&mut inv, 3, folder, "Note", 7, rules::FULL);
        let trash = rules::system(&inv, 14).expect("trash");
        let _trashed = item(&mut inv, 4, trash, "Old", 6, rules::FULL);
        let mut state = State::default();
        state.paste(&inv, &[folder, trash], false);
        assert_eq!(state.items.get("Hat"), Some(&hat));
        assert!(!state.items.contains_key("Note"));
        assert!(!state.items.contains_key("Old"));
    }

    #[test]
    fn write_matches_names_and_follows_the_image_rule() {
        let (mut inv, folder) = fixture();
        let hat = item(&mut inv, 1, folder, "Hat", 6, rules::FULL);
        let shoe = item(&mut inv, 2, folder, "Shoe", 5, rules::FULL);
        let scarf = item(&mut inv, 3, folder, "Scarf", 5, rules::FULL);
        let hat_tex = item(&mut inv, 10, folder, "Hat", 0, rules::FULL);
        // no transfer: the item image rule refuses it
        let _shoe_tex = item(&mut inv, 11, folder, "Shoe", 0, rules::COPY | rules::MODIFY);
        let mut state = State::default();
        state.paste(&inv, &[hat, shoe, scarf], false);
        state.paste(&inv, &[folder], true);
        let (change, skipped) = plan(&inv, &state.items, &state.textures, false);
        assert_eq!(written(&change), vec![(hat, inv.items[&hat_tex].asset_id)]);
        assert_eq!(skipped, vec!["Shoe".to_owned()]);
        assert_eq!(missing(&inv, &state.items).len(), 3);
    }

    #[test]
    fn clear_removes_only_existing_images_of_changeable_items() {
        let (mut inv, folder) = fixture();
        let hat = item(&mut inv, 1, folder, "Hat", 6, rules::FULL);
        let bare = item(&mut inv, 2, folder, "Bare", 6, rules::FULL);
        inv.items.get_mut(&hat).expect("hat").thumbnail = Uuid::from_u128(99);
        let mut state = State::default();
        state.paste(&inv, &[hat, bare], false);
        let (change, skipped) = plan(&inv, &state.items, &BTreeMap::new(), true);
        assert_eq!(written(&change), vec![(hat, Uuid::nil())]);
        assert!(skipped.is_empty());
        // a Library item may not be changed
        let library = inv.lib_root;
        let lib = item(&mut inv, 3, library, "Lib", 6, rules::FULL);
        inv.items.get_mut(&lib).expect("lib").thumbnail = Uuid::from_u128(98);
        let one = BTreeMap::from([("Lib".to_owned(), lib)]);
        let (change, skipped) = plan(&inv, &one, &BTreeMap::new(), true);
        assert!(change.operations.is_empty());
        assert_eq!(skipped, vec!["Lib".to_owned()]);
    }
}
