//! Protected folders manager: the folders locked from the inventory menu,
//! listed with a filter, unlocked from here. Port of
//! FSFloaterProtectedFolders (indra/newview/fsfloaterprotectedfolders.cpp,
//! originally LGPL 2.1): a folder no longer in the inventory shows as
//! unknown; a double click shows the folder in the inventory, unless RLVa's
//! @showinv forbids it.

use super::*;
use crate::ui::widgets::{Floater, flat_button, search_field};

#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    filter: String,
    selected: HashSet<Uuid>,
}

/// The rows: each protected folder with its name (None: not in the
/// inventory), sorted by name, the unknown ones last, kept when the
/// filter's text is in the name (case ignored, as LLScrollListCtrl's filter).
pub(super) fn rows(inv: &Inventory, protected: &HashSet<Uuid>, filter: &str) -> Vec<(Uuid, Option<String>)> {
    let needle = filter.trim().to_lowercase();
    let unknown = aurora_i18n::tr!("l3-protected-unknown").to_lowercase();
    let mut rows: Vec<(Uuid, Option<String>)> = protected
        .iter()
        .map(|id| (*id, inv.folders.contains_key(id).then(|| rules::name(inv, *id))))
        .filter(|(_, name)| needle.is_empty() || name.as_deref().map_or(unknown.as_str(), |n| n).to_lowercase().contains(&needle))
        .collect();
    rows.sort_by(|a, b| match (&a.1, &b.1) {
        (Some(x), Some(y)) => super::view::picker_name_cmp(x, y).then(a.0.cmp(&b.0)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.0.cmp(&b.0),
    });
    rows
}

/// "Remove" (handleRemove): the selected folders leave the protected list.
pub(super) fn remove(protected: &mut HashSet<Uuid>, selected: &HashSet<Uuid>) -> bool {
    let before = protected.len();
    protected.retain(|id| !selected.contains(id));
    protected.len() != before
}

pub(super) fn show(
    ctx: &egui::Context,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    prefs: &mut InventoryPreferences,
    show_inv: bool,
) {
    if !st.protected_folders.open {
        return;
    }
    let mut open = true;
    let mut reveal = None;
    let state = &mut st.protected_folders;
    let size = Vec2::new(320.0, 360.0);
    Floater::new(
        &format!("inventory_protected_folders_{}", st.ui_id),
        aurora_i18n::tr!("l3-protected-title"),
        ctx.content_rect().center() - size / 2.0,
        size,
    )
    .show(ctx, p, &mut open, |ui| {
        ui.horizontal(|ui| {
            let width = ui.available_width() - 90.0;
            search_field(ui, &mut state.filter, aurora_i18n::tr!("l3-protected-filter"), width.max(80.0));
            if ui
                .add_enabled_ui(!state.selected.is_empty(), |ui| {
                    flat_button(ui, p, aurora_i18n::tr!("l3-protected-remove"))
                })
                .inner
                .on_hover_text(aurora_i18n::tr!("l3-protected-remove-tip"))
                .clicked()
            {
                remove(&mut prefs.protected, &state.selected);
                state.selected.clear();
            }
        });
        ui.add_space(6.0);
        let rows = rows(inv, &prefs.protected, &state.filter);
        state.selected.retain(|id| prefs.protected.contains(id));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if rows.is_empty() {
                ui.label(RichText::new(aurora_i18n::tr!("l3-protected-empty")).color(p.muted));
            }
            for (id, name) in rows {
                let text = match &name {
                    Some(name) => RichText::new(name).color(p.ink),
                    None => RichText::new(aurora_i18n::tr!("l3-protected-unknown")).italics().color(p.muted),
                };
                let selected = state.selected.contains(&id);
                let response = ui.selectable_label(selected, text);
                if response.clicked() {
                    if ui.input(|i| i.modifiers.command || i.modifiers.shift) {
                        if !state.selected.remove(&id) {
                            state.selected.insert(id);
                        }
                    } else {
                        state.selected = HashSet::from([id]);
                    }
                }
                if response.double_clicked() && name.is_some() && show_inv {
                    reveal = Some(id);
                }
            }
        });
    });
    st.protected_folders.open = open;
    if let Some(id) = reveal {
        st.show_original(inv, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_sort_filter_and_mark_unknown_folders() {
        let mut inv = Inventory::default();
        crate::world::inventory::demo::seed(&mut inv, Uuid::from_u128(2));
        let mut folders: Vec<Uuid> = inv
            .folders
            .values()
            .filter(|f| f.info.type_default == -1 && !f.library)
            .map(|f| f.info.id)
            .collect();
        folders.sort();
        let (a, b) = (folders[0], folders[1]);
        let gone = Uuid::from_u128(0xdead);
        let protected = HashSet::from([a, b, gone]);
        let all = rows(&inv, &protected, "");
        assert_eq!(all.len(), 3);
        assert_eq!(all.last(), Some(&(gone, None)));
        let name = rules::name(&inv, a);
        let found = rows(&inv, &protected, &name.to_uppercase());
        assert!(found.iter().any(|(id, _)| *id == a));
        assert!(!found.iter().any(|(id, _)| *id == gone));
        let mut protected = protected;
        assert!(remove(&mut protected, &HashSet::from([a, gone])));
        assert_eq!(protected, HashSet::from([b]));
        assert!(!remove(&mut protected, &HashSet::from([a])));
    }
}
