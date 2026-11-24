//! Which prims and textures of a selection may be written to a Collada or
//! OBJ file. Port of FSExportPermsCheck::canExportNode
//! (indra/newview/fsexportperms.cpp, Cinder Biscuits, LGPL 2.1) and of the
//! texture check of DAESaver::updateTextureInfo
//! (indra/newview/daeexport.cpp, Latif Khalifa and Cinder Biscuits, LGPL
//! 2.1). An item's own rule is the shared
//! `world::inventory::actions::asset_export_allowed`.
//!
//! - Second Life: a prim we own and created; a megaprim (larger than the
//!   region allows) also when Zwagoth Klaar, who made the megaprims, created
//!   it. A mesh then also needs us as its uploader (the mesh header's
//!   creator), a sculpted prim a sculpt map we uploaded (the J2C comment's
//!   "a=") or an item of ours that lets it be exported.
//! - OpenSim with "ExportSupported": a prim we created, or whose Export bit
//!   we hold (LLPermissions::allowOpenSimExportBy); a sculpt map needs an
//!   item that lets it be exported, a mesh nothing more.
//! - OpenSim without: as Second Life, without the megaprim case; a sculpt
//!   map then needs an item we created.
//! - A texture is written only with an inventory item that lets it be
//!   exported, or (Second Life) our own upload of which we hold an item:
//!   the file takes that item's name. The blank texture is never written.

use crate::world::inventory::Inventory;
use crate::world::inventory::actions::{ExportGrid, FULL, asset_export_allowed};
use aurora_net::inventory::InvItem;
use glam::Vec3;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// The resident who made the megaprims, whose prims larger than a region
/// allows may be exported by anyone (canExportNode's "Megaprim check").
pub const MEGAPRIM_CREATOR: Uuid = Uuid::from_u128(0x7ffd02d0_12f4_48b4_9640_695708fd4ae4);

/// What the rule reads of a prim (LLSelectNode's mPermissions and the
/// object itself).
#[derive(Debug, Clone, PartialEq)]
pub struct PrimFacts {
    pub creator: Uuid,
    pub owner: Uuid,
    pub owner_mask: u32,
    pub everyone_mask: u32,
    pub group_owned: bool,
    pub scale: Vec3,
    pub shape: Shape,
}

/// The prim's geometry, for the second half of the rule.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Prim,
    /// `uploader`: the sculpt map's J2C comment, once its bytes are read.
    Sculpt {
        map: Uuid,
        uploader: Option<Uuid>,
    },
    /// `creator`: the mesh header's, once the header is read
    /// (LLMeshRepository::getCreatorFromHeader gives null before).
    Mesh {
        creator: Option<Uuid>,
    },
}

/// The facts of the session the rule needs, with the inventory's items of
/// the assets in question gathered in one pass (LLAssetIDMatches over the
/// whole inventory, Trash and Library included): the window reads the rule
/// several times a second, and an inventory holds tens of thousands of
/// items.
#[derive(Clone)]
pub struct Context<'a> {
    pub agent: Uuid,
    pub grid: ExportGrid,
    /// LLWorld::getRegionMaxPrimScale.
    pub max_prim_scale: f32,
    /// By asset, by name then id so that the file name taken from the first
    /// one does not change between runs.
    by_asset: HashMap<Uuid, Vec<&'a InvItem>>,
}

impl<'a> Context<'a> {
    /// The context for the textures and sculpt maps `assets`.
    pub fn new(agent: Uuid, grid: ExportGrid, max_prim_scale: f32, inventory: &'a Inventory, assets: &HashSet<Uuid>) -> Self {
        let mut by_asset: HashMap<Uuid, Vec<&InvItem>> = HashMap::new();
        if !assets.is_empty() {
            for item in inventory.items.values().filter(|i| assets.contains(&i.asset_id)) {
                by_asset.entry(item.asset_id).or_default().push(item);
            }
        }
        for items in by_asset.values_mut() {
            items.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        }
        Self {
            agent,
            grid,
            max_prim_scale,
            by_asset,
        }
    }

    fn items_of(&self, asset: Uuid) -> &[&'a InvItem] {
        self.by_asset.get(&asset).map_or(&[], Vec::as_slice)
    }

    fn some_item_allows(&self, asset: Uuid) -> bool {
        !asset.is_nil() && self.items_of(asset).iter().any(|i| asset_export_allowed(i, self.agent, self.grid))
    }
}

/// Whether the prim may be exported (canExportNode with `dae`: a mesh may
/// go to a Collada file, never to a backup).
pub fn prim(cx: &Context, p: &PrimFacts) -> bool {
    let agent = cx.agent;
    if agent.is_nil() {
        return false;
    }
    let ours = p.owner == agent && !p.group_owned;
    let object = match cx.grid {
        ExportGrid::SecondLife => {
            (ours && p.creator == agent)
                || (p.scale.max_element() > cx.max_prim_scale && (p.creator == MEGAPRIM_CREATOR || p.creator == agent))
        }
        // allowOpenSimExportBy: allowOperationBy(PERM_EXPORT, agent, no group)
        ExportGrid::OpenSim { export_supported: true } => {
            let mask = if ours { p.owner_mask } else { p.everyone_mask };
            p.creator == agent || mask & aurora_net::build::perm::EXPORT != 0
        }
        ExportGrid::OpenSim { export_supported: false } => ours && p.creator == agent,
    };
    if !object {
        return false;
    }
    match (&p.shape, cx.grid) {
        (Shape::Prim, _) => true,
        (Shape::Mesh { creator }, ExportGrid::SecondLife) => *creator == Some(agent),
        (Shape::Mesh { .. }, ExportGrid::OpenSim { .. }) => true,
        (Shape::Sculpt { map, uploader }, grid) => {
            (grid == ExportGrid::SecondLife && *uploader == Some(agent)) || cx.some_item_allows(*map)
        }
    }
}

/// Whether the prim may go into a linkset backup (`.oxp`), for its creator
/// only: FSExportPermsCheck::canExportNode with `dae` false
/// (indra/newview/fsexportperms.cpp, Cinder Biscuits, LGPL 2.1), applied on
/// every grid as Second Life applies it. The prim must be owned by the agent
/// (never by a group) and created by the agent, who must also hold copy,
/// modify and transfer on it. A mesh is never written to a backup, and a
/// sculpt needs a map the agent uploaded or holds an item of the agent's
/// creation for. Stricter than the Legacy Viewer: the owner's full
/// permissions are required, and neither the megaprim case nor OpenSim's
/// Export bit is used.
pub fn backup_prim(cx: &Context, p: &PrimFacts) -> bool {
    let agent = cx.agent;
    if agent.is_nil() || p.owner != agent || p.group_owned || p.creator != agent {
        return false;
    }
    if p.owner_mask & FULL != FULL {
        return false;
    }
    match &p.shape {
        Shape::Prim => true,
        Shape::Mesh { .. } => false,
        Shape::Sculpt { map, uploader } => backup_asset_allowed(cx, *map, *uploader),
    }
}

/// Whether an asset (a texture, a sculpt map) may keep its id in a backup:
/// FSExportPermsCheck::canExportAsset for Second Life, on every grid. The
/// agent made it (the J2C comment's uploader) or holds an item of the
/// agent's own creation that carries it.
pub fn backup_asset_allowed(cx: &Context, id: Uuid, uploader: Option<Uuid>) -> bool {
    !id.is_nil() && !cx.agent.is_nil() && (uploader == Some(cx.agent) || backup_item(cx, id).is_some())
}

/// The inventory item that lets a backup carry the asset `id`: the first
/// one the agent created (by name then id), whose name and description go
/// into the file next to the texture's data.
pub fn backup_item<'a>(cx: &Context<'a>, id: Uuid) -> Option<&'a InvItem> {
    cx.items_of(id)
        .iter()
        .copied()
        .find(|i| asset_export_allowed(i, cx.agent, ExportGrid::SecondLife))
}

/// The name a texture is written under, when it may be: the first item
/// holding it, its name made safe for a file (getScrubbedFileName, spaces
/// to underscores). `uploader` is the texture's J2C comment, when read.
pub fn texture(cx: &Context, id: Uuid, uploader: Option<Uuid>) -> Option<String> {
    if id.is_nil() || id == aurora_prim::te::BLANK_TEXTURE || cx.agent.is_nil() {
        return None;
    }
    let items = cx.items_of(id);
    let first = items.first()?;
    let comment = cx.grid == ExportGrid::SecondLife && uploader == Some(cx.agent);
    let allowed = comment || items.iter().any(|i| asset_export_allowed(i, cx.agent, cx.grid));
    allowed.then(|| file_stem(&first.name))
}

/// LLDir::getScrubbedFileName, then spaces to underscores: a name every
/// file system takes. An empty result becomes "texture".
pub fn file_stem(name: &str) -> String {
    const ILLEGAL: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
    let stem: String = name
        .trim()
        .chars()
        .map(|c| if ILLEGAL.contains(&c) || c.is_control() { '_' } else { c })
        .map(|c| if c == ' ' { '_' } else { c })
        .collect();
    let stem = stem.trim_matches('.').to_owned();
    if stem.is_empty() { "texture".into() } else { stem }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::inventory::actions::{COPY, EXPORT, FULL, MODIFY, TRANSFER};

    const ME: Uuid = Uuid::from_u128(0xA1);
    const OTHER: Uuid = Uuid::from_u128(0xB2);
    const LIBRARY: Uuid = Uuid::from_u128(0xC3);
    const MAP: Uuid = Uuid::from_u128(0x5C);
    const SL: ExportGrid = ExportGrid::SecondLife;
    const OS_EXPORT: ExportGrid = ExportGrid::OpenSim { export_supported: true };
    const OS_DENIED: ExportGrid = ExportGrid::OpenSim { export_supported: false };

    fn facts(creator: Uuid, owner: Uuid, shape: Shape) -> PrimFacts {
        PrimFacts {
            creator,
            owner,
            owner_mask: FULL,
            everyone_mask: 0,
            group_owned: false,
            scale: Vec3::ONE,
            shape,
        }
    }

    fn cx(grid: ExportGrid, inventory: &Inventory) -> Context<'_> {
        let assets = inventory.items.values().map(|i| i.asset_id).collect();
        Context::new(ME, grid, 64.0, inventory, &assets)
    }

    fn inventory(items: Vec<InvItem>) -> Inventory {
        let mut inv = Inventory {
            owner: ME,
            ..Default::default()
        };
        for item in items {
            inv.items.insert(item.id, item);
        }
        inv
    }

    fn item(
        n: u128,
        name: &str,
        asset: Uuid,
        owner: Uuid,
        creator: Uuid,
        owner_mask: u32,
        everyone_mask: u32,
        group_owned: bool,
    ) -> InvItem {
        InvItem {
            id: Uuid::from_u128(n),
            name: name.into(),
            asset_id: asset,
            owner,
            creator,
            owner_mask,
            everyone_mask,
            group_owned,
            ..Default::default()
        }
    }

    #[test]
    fn second_life_exports_only_the_prims_we_own_and_created() {
        let inv = inventory(vec![]);
        let c = cx(SL, &inv);
        assert!(prim(&c, &facts(ME, ME, Shape::Prim)), "our own work");
        assert!(!prim(&c, &facts(OTHER, ME, Shape::Prim)), "bought, even full permissions");
        assert!(!prim(&c, &facts(ME, OTHER, Shape::Prim)), "made by us, someone else's now");
        let mut group = facts(ME, ME, Shape::Prim);
        group.group_owned = true;
        assert!(!prim(&c, &group), "deeded to a group");
        for mask in [COPY | TRANSFER, MODIFY | TRANSFER, COPY | MODIFY] {
            let mut p = facts(ME, ME, Shape::Prim);
            p.owner_mask = mask;
            assert!(prim(&c, &p), "our own work, whatever its masks ({mask:x})");
        }
        let nobody = Context {
            agent: Uuid::nil(),
            ..c.clone()
        };
        assert!(!prim(&nobody, &facts(Uuid::nil(), Uuid::nil(), Shape::Prim)), "before the login");
    }

    #[test]
    fn a_megaprim_exports_when_its_maker_or_we_created_it() {
        let inv = inventory(vec![]);
        let c = cx(SL, &inv);
        let mega = |creator, owner| PrimFacts {
            scale: Vec3::new(256.0, 0.5, 0.5),
            ..facts(creator, owner, Shape::Prim)
        };
        assert!(prim(&c, &mega(MEGAPRIM_CREATOR, OTHER)), "the megaprims' maker");
        assert!(prim(&c, &mega(ME, OTHER)), "ours");
        assert!(!prim(&c, &mega(OTHER, ME)), "another creator");
        let at_limit = PrimFacts {
            scale: Vec3::splat(64.0),
            ..facts(MEGAPRIM_CREATOR, ME, Shape::Prim)
        };
        assert!(!prim(&c, &at_limit), "a prim the region allows is no megaprim");
        let os = cx(OS_DENIED, &inv);
        assert!(!prim(&os, &mega(MEGAPRIM_CREATOR, OTHER)), "Second Life only");
    }

    #[test]
    fn a_mesh_on_second_life_needs_us_as_its_uploader() {
        let inv = inventory(vec![]);
        let c = cx(SL, &inv);
        assert!(prim(&c, &facts(ME, ME, Shape::Mesh { creator: Some(ME) })));
        assert!(
            !prim(&c, &facts(ME, ME, Shape::Mesh { creator: Some(OTHER) })),
            "someone else's mesh on our prim"
        );
        assert!(!prim(&c, &facts(ME, ME, Shape::Mesh { creator: None })), "header not read yet");
        assert!(
            prim(&cx(OS_EXPORT, &inv), &facts(ME, ME, Shape::Mesh { creator: None })),
            "OpenSim: no mesh check"
        );
    }

    #[test]
    fn a_sculpted_prim_needs_its_map_uploaded_by_us_or_an_item_that_allows_it() {
        let sculpt = |uploader| facts(ME, ME, Shape::Sculpt { map: MAP, uploader });
        let empty = inventory(vec![]);
        assert!(prim(&cx(SL, &empty), &sculpt(Some(ME))), "the map's comment names us");
        assert!(!prim(&cx(SL, &empty), &sculpt(Some(OTHER))));
        assert!(!prim(&cx(SL, &empty), &sculpt(None)), "no comment, no item");
        assert!(!prim(&cx(OS_DENIED, &empty), &sculpt(Some(ME))), "OpenSim reads no comment");

        // (what, item holding the map, Second Life, OpenSim without export, with export)
        let cases = [
            (
                "its own upload, full permissions",
                item(1, "a", MAP, ME, ME, FULL | EXPORT, 0, false),
                true,
                true,
                true,
            ),
            (
                "its own upload, no copy",
                item(1, "a", MAP, ME, ME, MODIFY | TRANSFER, 0, false),
                true,
                true,
                false,
            ),
            (
                "its own upload, no transfer",
                item(1, "a", MAP, ME, ME, COPY | MODIFY, 0, false),
                true,
                true,
                false,
            ),
            (
                "its own upload, no modify",
                item(1, "a", MAP, ME, ME, COPY | TRANSFER, 0, false),
                true,
                true,
                false,
            ),
            (
                "bought, full permissions",
                item(1, "a", MAP, ME, OTHER, FULL, 0, false),
                false,
                false,
                false,
            ),
            (
                "bought, Export allowed",
                item(1, "a", MAP, ME, OTHER, FULL | EXPORT, 0, false),
                false,
                false,
                true,
            ),
            (
                "group owned",
                item(1, "a", MAP, ME, OTHER, FULL | EXPORT, 0, true),
                false,
                false,
                false,
            ),
            (
                "someone else's",
                item(1, "a", MAP, OTHER, OTHER, FULL | EXPORT, 0, false),
                false,
                false,
                false,
            ),
            (
                "a Library item",
                item(1, "a", MAP, LIBRARY, LIBRARY, FULL | EXPORT, COPY, false),
                false,
                false,
                false,
            ),
        ];
        for (what, it, sl, denied, allowed) in cases {
            let inv = inventory(vec![it]);
            assert_eq!(prim(&cx(SL, &inv), &sculpt(None)), sl, "Second Life: {what}");
            assert_eq!(prim(&cx(OS_DENIED, &inv), &sculpt(None)), denied, "OpenSim without export: {what}");
            assert_eq!(prim(&cx(OS_EXPORT, &inv), &sculpt(None)), allowed, "OpenSim with export: {what}");
        }
    }

    #[test]
    fn opensim_with_export_reads_the_creator_or_the_export_bit_we_hold() {
        let inv = inventory(vec![]);
        let c = cx(OS_EXPORT, &inv);
        assert!(prim(&c, &facts(ME, OTHER, Shape::Prim)), "we created it, someone else owns it");
        let mut bought = facts(OTHER, ME, Shape::Prim);
        assert!(!prim(&c, &bought), "bought without Export");
        bought.owner_mask = FULL | EXPORT;
        assert!(prim(&c, &bought), "bought with Export");
        let mut theirs = facts(OTHER, OTHER, Shape::Prim);
        theirs.owner_mask = FULL | EXPORT;
        assert!(!prim(&c, &theirs), "their owner's Export is not ours");
        theirs.everyone_mask = EXPORT;
        assert!(prim(&c, &theirs), "Export given to everyone");
        let mut group = facts(OTHER, ME, Shape::Prim);
        group.group_owned = true;
        group.owner_mask = FULL | EXPORT;
        assert!(!prim(&c, &group), "deeded: the group's Export, not ours");
        assert!(!prim(&cx(OS_DENIED, &inv), &bought), "the region does not honour Export");
        assert!(prim(&cx(OS_DENIED, &inv), &facts(ME, ME, Shape::Prim)), "our own work");
    }

    #[test]
    fn a_texture_takes_the_name_of_an_item_that_allows_it() {
        let tex = Uuid::from_u128(0x7E);
        // (what, item, Second Life, OpenSim without export, with export)
        let cases = [
            (
                "its own upload, full permissions",
                item(1, "My wood", tex, ME, ME, FULL | EXPORT, 0, false),
                true,
                true,
                true,
            ),
            (
                "its own upload, no copy",
                item(1, "My wood", tex, ME, ME, MODIFY | TRANSFER, 0, false),
                true,
                true,
                false,
            ),
            (
                "its own upload, no transfer",
                item(1, "My wood", tex, ME, ME, COPY | MODIFY, 0, false),
                true,
                true,
                false,
            ),
            (
                "its own upload, no modify",
                item(1, "My wood", tex, ME, ME, COPY | TRANSFER, 0, false),
                true,
                true,
                false,
            ),
            (
                "bought, full permissions",
                item(1, "My wood", tex, ME, OTHER, FULL, 0, false),
                false,
                false,
                false,
            ),
            (
                "bought, Export allowed",
                item(1, "My wood", tex, ME, OTHER, FULL | EXPORT, 0, false),
                false,
                false,
                true,
            ),
            (
                "group owned",
                item(1, "My wood", tex, ME, OTHER, FULL | EXPORT, 0, true),
                false,
                false,
                false,
            ),
            (
                "someone else's",
                item(1, "My wood", tex, OTHER, OTHER, FULL | EXPORT, 0, false),
                false,
                false,
                false,
            ),
            (
                "a Library item",
                item(1, "My wood", tex, LIBRARY, LIBRARY, FULL | EXPORT, COPY, false),
                false,
                false,
                false,
            ),
        ];
        for (what, it, sl, denied, allowed) in cases {
            let inv = inventory(vec![it]);
            let name = |grid| texture(&cx(grid, &inv), tex, None);
            assert_eq!(name(SL).is_some(), sl, "Second Life: {what}");
            assert_eq!(name(OS_DENIED).is_some(), denied, "OpenSim without export: {what}");
            assert_eq!(name(OS_EXPORT).is_some(), allowed, "OpenSim with export: {what}");
            if sl {
                assert_eq!(name(SL).as_deref(), Some("My_wood"));
            }
        }
    }

    #[test]
    fn our_upload_comment_needs_an_item_for_the_file_name() {
        let tex = Uuid::from_u128(0x7E);
        let bought = inventory(vec![item(1, "Wood", tex, ME, OTHER, FULL, 0, false)]);
        assert_eq!(
            texture(&cx(SL, &bought), tex, Some(ME)).as_deref(),
            Some("Wood"),
            "comment names us"
        );
        assert_eq!(texture(&cx(OS_DENIED, &bought), tex, Some(ME)), None, "OpenSim reads no comment");
        let empty = inventory(vec![]);
        assert_eq!(
            texture(&cx(SL, &empty), tex, Some(ME)),
            None,
            "no item: no name, as the Legacy Viewer"
        );
        let blank = inventory(vec![item(1, "Blank", aurora_prim::te::BLANK_TEXTURE, ME, ME, FULL, 0, false)]);
        assert_eq!(
            texture(&cx(SL, &blank), aurora_prim::te::BLANK_TEXTURE, Some(ME)),
            None,
            "never the blank texture"
        );
    }

    #[test]
    fn the_first_item_by_name_gives_the_file_name() {
        let tex = Uuid::from_u128(0x7E);
        let inv = inventory(vec![
            item(2, "zebra", tex, ME, OTHER, FULL, 0, false),
            item(1, "apple", tex, ME, ME, FULL, 0, false),
        ]);
        assert_eq!(texture(&cx(SL, &inv), tex, None).as_deref(), Some("apple"));
    }

    #[test]
    fn a_backup_is_for_the_creator_on_every_grid() {
        let inv = inventory(vec![]);
        let backup = |grid, p: PrimFacts| backup_prim(&cx(grid, &inv), &p);
        for grid in [SL, OS_EXPORT, OS_DENIED] {
            assert!(backup(grid, facts(ME, ME, Shape::Prim)), "ours and made by us");
            assert!(!backup(grid, facts(OTHER, ME, Shape::Prim)), "bought");
            assert!(!backup(grid, facts(ME, OTHER, Shape::Prim)), "made by us, owned by someone else");
            let mut group = facts(ME, ME, Shape::Prim);
            group.group_owned = true;
            assert!(!backup(grid, group), "deeded to a group");
            let mut no_transfer = facts(ME, ME, Shape::Prim);
            no_transfer.owner_mask = FULL & !TRANSFER;
            assert!(!backup(grid, no_transfer), "no transfer");
            let mut no_copy = facts(ME, ME, Shape::Prim);
            no_copy.owner_mask = FULL & !COPY;
            assert!(!backup(grid, no_copy), "no copy");
            let mut no_modify = facts(ME, ME, Shape::Prim);
            no_modify.owner_mask = FULL & !MODIFY;
            assert!(!backup(grid, no_modify), "no modify");
            // neither the Export bit nor a megaprim opens a backup
            let mut export_bit = facts(OTHER, OTHER, Shape::Prim);
            export_bit.owner_mask = FULL | EXPORT;
            export_bit.everyone_mask = EXPORT;
            assert!(!backup(grid, export_bit));
            let mut mega = facts(MEGAPRIM_CREATOR, OTHER, Shape::Prim);
            mega.scale = Vec3::splat(1000.0);
            assert!(!backup(grid, mega));
            assert!(
                !backup(grid, facts(ME, ME, Shape::Mesh { creator: Some(ME) })),
                "a mesh is never in a backup"
            );
        }
        let nobody = Context::new(Uuid::nil(), SL, 64.0, &inv, &HashSet::new());
        assert!(!backup_prim(&nobody, &facts(Uuid::nil(), Uuid::nil(), Shape::Prim)));
    }

    #[test]
    fn a_backup_carries_a_sculpt_map_only_when_the_creator_made_it() {
        let inv = inventory(vec![
            item(1, "mine", MAP, ME, ME, FULL, 0, false),
            item(2, "bought", Uuid::from_u128(0x5D), ME, OTHER, FULL | EXPORT, 0, false),
            item(3, "library", Uuid::from_u128(0x5E), LIBRARY, LIBRARY, FULL | EXPORT, COPY, false),
        ]);
        for grid in [SL, OS_EXPORT, OS_DENIED] {
            let cx = cx(grid, &inv);
            let sculpt = |map, uploader| facts(ME, ME, Shape::Sculpt { map, uploader });
            assert!(backup_prim(&cx, &sculpt(MAP, None)));
            assert!(
                !backup_prim(&cx, &sculpt(Uuid::from_u128(0x5D), None)),
                "an item made by someone else"
            );
            assert!(!backup_prim(&cx, &sculpt(Uuid::from_u128(0x5E), None)), "a Library item");
            assert!(backup_prim(&cx, &sculpt(Uuid::from_u128(0x77), Some(ME))), "its comment names us");
            assert!(!backup_prim(&cx, &sculpt(Uuid::from_u128(0x77), Some(OTHER))));
            assert_eq!(backup_item(&cx, MAP).map(|i| i.name.as_str()), Some("mine"));
            assert!(!backup_asset_allowed(&cx, Uuid::nil(), Some(ME)));
        }
    }

    #[test]
    fn file_stems_are_safe_on_every_system() {
        assert_eq!(file_stem("Old wood: dark/light"), "Old_wood__dark_light");
        assert_eq!(file_stem("  ..  "), "texture");
        assert_eq!(file_stem("a\u{7}b"), "a_b");
        assert_eq!(file_stem("..hidden."), "hidden");
    }
}
