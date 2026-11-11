//! What a new prim and a new script start with beyond their look: the
//! default texture of new prims, the inventory item put inside each new
//! prim, and the script a "New Script" button makes (Preferences › Build).
//! Held in `BuildSettings::new_objects`; every default is the Legacy
//! Viewer's (settings.xml / settings_per_account.xml names in comments).
//!
//! Behaviour of FSCommon::applyDefaultBuildPreferences (indra/newview/fscommon.cpp),
//! LLPanelContents::onClickNewScript (llpanelcontents.cpp) and
//! create_new_item's "lsl" branch (llviewerinventory.cpp), with the drop
//! targets of FSPanelPrefs (fspanelprefs.cpp), originally LGPL 2.1.
//!
//! Stricter than the Legacy Viewer, on purpose (AGENTS.md "Creators'
//! permissions"): the default texture is an asset id kept in the settings
//! file, so only a texture whose id may be shown is taken
//! (`actions::asset_uuid_shown`: full permissions or a built-in texture),
//! and checked again when a prim is made; a script template must be one we
//! may copy, so the button never moves a no-copy script out of the
//! inventory. Only item ids (never asset ids) are kept for the item and the
//! template.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// AT_TEXTURE, AT_LSL_TEXT, AT_CALLINGCARD, AT_LINK, AT_LINK_FOLDER
/// (llassettype.h).
const AT_TEXTURE: i32 = 0;
const AT_LSL_TEXT: i32 = 10;
const AT_CALLINGCARD: i32 = 2;
const AT_LINK: i32 = 24;
const AT_LINK_FOLDER: i32 = 25;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NewObjects {
    /// FSDefaultObjectTexture: the texture of every face of a new prim.
    /// None keeps the region's own default (the Legacy default,
    /// DEFAULT_OBJECT_TEXTURE, is that same texture).
    pub texture: Option<Uuid>,
    /// The items of each account (settings_per_account.xml).
    pub accounts: HashMap<Uuid, AccountItems>,
}

/// One account's FSBuildPrefs_* inventory choices.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AccountItems {
    /// FSBuildPrefs_EmbedItem (off).
    pub embed: bool,
    /// FSBuildPrefs_Item: the inventory item put into new prims.
    pub embed_item: Option<Uuid>,
    /// FSBuildPrefs_UseCustomScript (off).
    pub custom_script: bool,
    /// FSBuildPrefs_CustomScriptItem: the script "New Script" copies.
    pub script_item: Option<Uuid>,
}

impl NewObjects {
    pub fn account(&self, agent: Uuid) -> AccountItems {
        self.accounts.get(&agent).copied().unwrap_or_default()
    }

    /// The account's entry; an entry back to the defaults is dropped, so
    /// the file keeps only accounts that chose something.
    pub fn set_account(&mut self, agent: Uuid, items: AccountItems) {
        if items == AccountItems::default() {
            self.accounts.remove(&agent);
        } else {
            self.accounts.insert(agent, items);
        }
    }

    /// The item to put into each new prim, when the option is on.
    pub fn embed_item(&self, agent: Uuid) -> Option<Uuid> {
        let a = self.account(agent);
        a.embed.then_some(a.embed_item).flatten()
    }

    /// The script "New Script" copies, when the option is on.
    pub fn script_template(&self, agent: Uuid) -> Option<Uuid> {
        let a = self.account(agent);
        a.custom_script.then_some(a.script_item).flatten()
    }
}

/// FSEmbeddedItemDropTarget: any item an object may hold (calling cards and
/// links are never given to an object, `contents_drop::accepts`).
pub fn embeddable(item: &aurora_net::inventory::InvItem) -> bool {
    !matches!(item.asset_type, AT_CALLINGCARD | AT_LINK | AT_LINK_FOLDER)
}

/// A script template: a script whose holder may copy it.
pub fn script_template_allowed(item: &aurora_net::inventory::InvItem, agent: Uuid) -> bool {
    use crate::world::inventory::actions;
    item.asset_type == AT_LSL_TEXT && actions::agent_mask(item, agent) & actions::COPY != 0
}

/// The default texture: a texture whose asset id may be kept and shown.
pub fn texture_allowed(inv: &crate::world::inventory::Inventory, item: &aurora_net::inventory::InvItem) -> bool {
    item.asset_type == AT_TEXTURE
        && !item.asset_id.is_nil()
        && crate::world::inventory::actions::held(inv, item) & crate::world::inventory::actions::FULL
            == crate::world::inventory::actions::FULL
}

/// How Preferences names a choice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Label {
    #[default]
    NotSet,
    /// Chosen, but not in the inventory (EmbeddedItemNotAvailable).
    Missing,
    Name(String),
}

/// The single inventory entry dragged over Preferences, and the choices it
/// may become (FSEmbeddedItemDropTarget, the texture control's drop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Carried {
    pub item: Uuid,
    pub asset: Uuid,
    pub texture: bool,
    pub embed: bool,
    pub script: bool,
}

/// What Preferences › Build shows of the account's choices, read from the
/// inventory by the app (never the asset ids themselves).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    pub agent: Uuid,
    pub texture: Label,
    pub embed: Label,
    pub script: Label,
    pub carried: Option<Carried>,
}

fn item_label(inv: &crate::world::inventory::Inventory, id: Option<Uuid>) -> Label {
    match id {
        None => Label::NotSet,
        Some(id) => inv.items.get(&id).map_or(Label::Missing, |it| Label::Name(it.name.clone())),
    }
}

/// The view of `n` for `agent`, `carried` the inventory ids being dragged.
pub fn view(n: &NewObjects, inv: &crate::world::inventory::Inventory, agent: Uuid, carried: Option<&[Uuid]>) -> View {
    let a = n.account(agent);
    let texture = match n.texture {
        None => Label::NotSet,
        Some(t) => inv
            .items
            .values()
            .filter(|it| it.asset_id == t && texture_allowed(inv, it))
            .map(|it| it.name.clone())
            .min()
            .map_or(Label::Missing, Label::Name),
    };
    let carried = match carried {
        Some([id]) => {
            let id = crate::world::inventory::actions::original(inv, *id).unwrap_or(*id);
            inv.items.get(&id).map(|it| Carried {
                item: it.id,
                asset: it.asset_id,
                texture: texture_allowed(inv, it),
                embed: embeddable(it),
                script: script_template_allowed(it, agent),
            })
        }
        _ => None,
    };
    View {
        agent,
        texture,
        embed: item_label(inv, a.embed_item),
        script: item_label(inv, a.script_item),
        carried,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::inventory::actions::{COPY, FULL, MODIFY, TRANSFER};
    use aurora_net::inventory::InvItem;

    const AGENT: Uuid = Uuid::from_u128(1);
    const OTHER: Uuid = Uuid::from_u128(2);

    fn item(asset_type: i32, owner: Uuid, owner_mask: u32, everyone_mask: u32) -> InvItem {
        InvItem {
            id: Uuid::from_u128(100),
            asset_id: Uuid::from_u128(200),
            asset_type,
            owner,
            owner_mask,
            everyone_mask,
            ..Default::default()
        }
    }

    #[test]
    fn defaults_are_the_legacy_viewers_and_old_files_load() {
        let n: NewObjects = serde_json::from_str("{}").unwrap();
        assert_eq!(n, NewObjects::default());
        assert_eq!(n.texture, None);
        assert_eq!(n.embed_item(AGENT), None);
        assert_eq!(n.script_template(AGENT), None);
        let a: AccountItems = serde_json::from_str(r#"{"embed":true}"#).unwrap();
        assert!(a.embed && a.embed_item.is_none() && !a.custom_script);
    }

    #[test]
    fn items_are_kept_per_account_and_only_used_when_on() {
        let mut n = NewObjects::default();
        let chosen = Uuid::from_u128(9);
        n.set_account(
            AGENT,
            AccountItems {
                embed_item: Some(chosen),
                script_item: Some(chosen),
                ..Default::default()
            },
        );
        assert_eq!(n.embed_item(AGENT), None, "chosen but off");
        assert_eq!(n.script_template(AGENT), None);
        let mut a = n.account(AGENT);
        a.embed = true;
        a.custom_script = true;
        n.set_account(AGENT, a);
        assert_eq!(n.embed_item(AGENT), Some(chosen));
        assert_eq!(n.script_template(AGENT), Some(chosen));
        assert_eq!(n.embed_item(OTHER), None, "another account has its own");
        n.set_account(AGENT, AccountItems::default());
        assert!(n.accounts.is_empty());
    }

    #[test]
    fn the_view_names_choices_and_what_a_drag_may_become() {
        let mut inv = crate::world::inventory::Inventory::default();
        inv.owner = AGENT;
        let mut tex = item(AT_TEXTURE, AGENT, FULL, 0);
        tex.name = "Wood".into();
        let mut script = item(AT_LSL_TEXT, AGENT, FULL, 0);
        script.id = Uuid::from_u128(101);
        script.asset_id = Uuid::from_u128(201);
        script.name = "Template".into();
        inv.add_items(vec![tex.clone(), script.clone()]);
        let mut n = NewObjects {
            texture: Some(tex.asset_id),
            ..Default::default()
        };
        n.set_account(
            AGENT,
            AccountItems {
                embed_item: Some(Uuid::from_u128(999)),
                script_item: Some(script.id),
                ..Default::default()
            },
        );
        let v = view(&n, &inv, AGENT, Some(&[script.id]));
        assert_eq!(v.texture, Label::Name("Wood".into()));
        assert_eq!(v.embed, Label::Missing);
        assert_eq!(v.script, Label::Name("Template".into()));
        let c = v.carried.unwrap();
        assert!(c.script && c.embed && !c.texture);
        let v = view(&n, &inv, AGENT, Some(&[tex.id, script.id]));
        assert_eq!(v.carried, None, "one item at a time");
        n.texture = Some(Uuid::from_u128(555));
        assert_eq!(view(&n, &inv, AGENT, None).texture, Label::Missing);
    }

    #[test]
    fn calling_cards_and_links_are_never_embedded() {
        assert!(embeddable(&item(AT_TEXTURE, AGENT, FULL, 0)));
        assert!(embeddable(&item(AT_LSL_TEXT, AGENT, 0, 0)));
        for t in [AT_CALLINGCARD, AT_LINK, AT_LINK_FOLDER] {
            assert!(!embeddable(&item(t, AGENT, FULL, 0)));
        }
    }

    /// The rights cases of AGENTS.md: the owner's, a group-owned, another's,
    /// a Library item, no copy, no transfer, no modify, full permissions.
    #[test]
    fn script_templates_need_copy() {
        let ok = |i: &InvItem| script_template_allowed(i, AGENT);
        assert!(ok(&item(AT_LSL_TEXT, AGENT, FULL, 0)), "full permissions");
        assert!(ok(&item(AT_LSL_TEXT, AGENT, COPY, 0)), "owner's, copy only");
        let mut group = item(AT_LSL_TEXT, OTHER, FULL, 0);
        group.group_owned = true;
        assert!(!ok(&group), "group-owned: the everyone mask");
        assert!(!ok(&item(AT_LSL_TEXT, OTHER, FULL, 0)), "someone else's");
        assert!(ok(&item(AT_LSL_TEXT, OTHER, 0, COPY)), "a Library item copyable by everyone");
        assert!(!ok(&item(AT_LSL_TEXT, AGENT, MODIFY | TRANSFER, 0)), "no copy");
        assert!(ok(&item(AT_LSL_TEXT, AGENT, COPY | MODIFY, 0)), "no transfer");
        assert!(ok(&item(AT_LSL_TEXT, AGENT, COPY | TRANSFER, 0)), "no modify");
        assert!(!ok(&item(AT_TEXTURE, AGENT, FULL, 0)), "not a script");
    }

    #[test]
    fn the_default_texture_needs_full_permissions() {
        let mut inv = crate::world::inventory::Inventory::default();
        inv.owner = AGENT;
        let ok = |i: &InvItem| texture_allowed(&inv, i);
        assert!(ok(&item(AT_TEXTURE, AGENT, FULL, 0)), "full permissions");
        let mut group = item(AT_TEXTURE, OTHER, FULL, 0);
        group.group_owned = true;
        assert!(!ok(&group), "group-owned");
        assert!(!ok(&item(AT_TEXTURE, OTHER, FULL, 0)), "someone else's");
        assert!(ok(&item(AT_TEXTURE, OTHER, 0, FULL)), "a full-permission Library texture");
        assert!(!ok(&item(AT_TEXTURE, OTHER, 0, COPY)), "a copy-only Library texture");
        assert!(!ok(&item(AT_TEXTURE, AGENT, MODIFY | TRANSFER, 0)), "no copy");
        assert!(!ok(&item(AT_TEXTURE, AGENT, COPY | MODIFY, 0)), "no transfer");
        assert!(!ok(&item(AT_TEXTURE, AGENT, COPY | TRANSFER, 0)), "no modify");
        let mut no_asset = item(AT_TEXTURE, AGENT, FULL, 0);
        no_asset.asset_id = Uuid::nil();
        assert!(!ok(&no_asset));
        assert!(!ok(&item(AT_LSL_TEXT, AGENT, FULL, 0)), "not a texture");
    }
}
