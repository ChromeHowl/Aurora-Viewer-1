//! Me › Avatar health's character tests, our own avatar only:
//!
//! - "Appearance to XML" (Advanced.AppearanceToXML, LLVOAvatar::
//!   dumpArchetypeXML in indra/newview/llvoavatar.cpp): the Appearance
//!   window's own export (`world::appearance_xml`), from this menu too.
//! - "Reset default male / female avatar (Character Test)" (handle_test_male
//!   / handle_test_female in indra/newview/llviewermenu.cpp,
//!   LLAppearanceMgr::wearOutfitByName in llappearancemgr.cpp): the folder
//!   "Male Shape & Outfit" or "Female Shape & Outfit" is worn in place of
//!   the outfit, the user's own copy first, else the Library's, copied into
//!   the inventory first. Refused while RLVa holds a wear lock, as there.
//!
//! Difference: the Library's folder is worn item by item through the
//! Library wear (`wear_selection`, LLAppearanceMgr::wearItemsOnAvatar),
//! which replaces what its items replace and keeps the other attachments,
//! where the Legacy Viewer replaces the whole outfit with the copied folder.
//!
//! All originally LGPL 2.1.
use super::App;
use crate::world::appearance::{Action, WearMode};
use uuid::Uuid;

/// The folder handle_test_male / handle_test_female wear.
pub fn outfit_name(male: bool) -> &'static str {
    if male { "Male Shape & Outfit" } else { "Female Shape & Outfit" }
}

/// A folder of that name: `(id, in the Library, in the trash)`.
pub type Candidate = (Uuid, bool, bool);

/// wearOutfitByName: the user's own folder (out of the trash) first, else
/// the Library's; the lowest id when there are several, so that the choice
/// does not depend on the inventory's order. `(folder, from the Library)`.
pub fn pick_outfit(candidates: &[Candidate]) -> Option<(Uuid, bool)> {
    let first = |library: bool| {
        candidates
            .iter()
            .filter(|(_, lib, trash)| *lib == library && !trash)
            .map(|(id, _, _)| *id)
            .min()
    };
    first(false).map(|id| (id, false)).or_else(|| first(true).map(|id| (id, true)))
}

impl App {
    /// Me › Avatar health › Appearance to XML: the Appearance window's
    /// "Export my appearance XML…" (`world::appearance_xml`), reached from
    /// where the Legacy Viewer's Advanced › Character menu has it.
    pub(super) fn dump_appearance_xml(&mut self) {
        if !crate::world::appearance_xml::ready(&self.world) {
            self.world.system_message(aurora_i18n::tr!("b4-dump-not-ready"));
            return;
        }
        self.appearance_ui.json.export_appearance(&self.egui_ctx, &self.world);
    }

    pub(super) fn reset_default_avatar(&mut self, male: bool) {
        if self.world.rlva.inventory_locked() {
            self.world.system_message(aurora_i18n::tr!("b4-default-rlva"));
            return;
        }
        let name = outfit_name(male);
        let inv = &self.world.inventory;
        let candidates: Vec<Candidate> = inv
            .folders
            .iter()
            .filter(|(_, f)| f.info.name == name)
            .map(|(id, f)| (*id, f.library, crate::world::inventory::actions::in_type(inv, *id, 14)))
            .collect();
        match pick_outfit(&candidates) {
            Some((folder, false)) => {
                log::info!("character test: wearing {name} ({folder})");
                self.apply_appearance_action(Action::WearContents {
                    folder,
                    mode: WearMode::ReplaceOutfit,
                });
            }
            Some((folder, true)) => {
                let items = library_items(inv, folder);
                if items.is_empty() {
                    // its contents are fetched when the Library folder opens
                    self.world.system_message(aurora_i18n::tr!("b4-default-loading", folder = name));
                    return;
                }
                log::info!("character test: wearing a copy of the Library's {name} ({} items)", items.len());
                self.wear_selection(items, true, 0);
            }
            None => self.world.system_message(aurora_i18n::tr!("b4-default-missing", folder = name)),
        }
    }
}

/// The wearable items under a Library folder (body parts, clothing and
/// objects), as collectDescendents finds them.
fn library_items(inv: &crate::world::inventory::Inventory, folder: Uuid) -> Vec<Uuid> {
    let mut out = Vec::new();
    let mut stack = vec![folder];
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(f) = inv.folders.get(&id) else { continue };
        stack.extend(f.children.iter().copied());
        let mut items: Vec<Uuid> = f
            .items
            .iter()
            .copied()
            .filter(|i| inv.items.get(i).is_some_and(|it| matches!(it.asset_type, 5 | 6 | 13)))
            .collect();
        items.sort_unstable();
        out.extend(items);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_users_own_outfit_comes_before_the_librarys_and_the_trash_is_skipped() {
        let id = Uuid::from_u128;
        assert_eq!(outfit_name(true), "Male Shape & Outfit");
        assert_eq!(outfit_name(false), "Female Shape & Outfit");
        assert_eq!(pick_outfit(&[]), None);
        assert_eq!(pick_outfit(&[(id(5), true, false), (id(9), false, false)]), Some((id(9), false)));
        assert_eq!(pick_outfit(&[(id(5), true, false), (id(9), false, true)]), Some((id(5), true)));
        assert_eq!(pick_outfit(&[(id(9), false, false), (id(3), false, false)]), Some((id(3), false)));
        assert_eq!(pick_outfit(&[(id(9), false, true)]), None);
    }
}
