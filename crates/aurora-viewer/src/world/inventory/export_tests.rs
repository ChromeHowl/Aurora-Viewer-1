//! The export rule (`actions::export_prim`, `export_asset`, `export_texture`):
//! FSExportPermsCheck::canExportNode / canExportAsset, or stricter.

use super::actions::*;
use super::*;
use aurora_prim::params::{LL_SCULPT_TYPE_MESH, SculptParams};

const ME: Uuid = Uuid::from_u128(2);
const OTHER: Uuid = Uuid::from_u128(3);
const GROUP: Uuid = Uuid::from_u128(4);
const MY_TEXTURE: Uuid = Uuid::from_u128(100);
const THEIR_TEXTURE: Uuid = Uuid::from_u128(101);
const LIBRARY_TEXTURE: Uuid = Uuid::from_u128(102);
const NO_COPY_TEXTURE: Uuid = Uuid::from_u128(103);
const MESH: Uuid = Uuid::from_u128(200);

fn folder(id: Uuid, library: bool) -> Folder {
    Folder {
        info: InvFolder { id, ..Default::default() },
        children: Vec::new(),
        items: Vec::new(),
        state: FetchState::Fetched,
        library,
    }
}

fn item(n: u128, parent: Uuid, asset: Uuid, creator: Uuid, owner: Uuid, mask: u32) -> InvItem {
    InvItem {
        id: Uuid::from_u128(n),
        parent,
        asset_type: 0,
        inv_type: 0,
        asset_id: asset,
        creator,
        owner,
        owner_mask: mask,
        base_mask: mask,
        everyone_mask: mask,
        ..Default::default()
    }
}

fn inventory() -> Inventory {
    let mut inv = Inventory {
        owner: ME,
        root: Uuid::from_u128(1),
        lib_root: Uuid::from_u128(9),
        ..Default::default()
    };
    inv.folders.insert(inv.root, folder(inv.root, false));
    inv.folders.insert(inv.lib_root, folder(inv.lib_root, true));
    let root = inv.root;
    let lib = inv.lib_root;
    inv.add_items(vec![
        item(10, root, MY_TEXTURE, ME, ME, FULL),
        item(11, root, THEIR_TEXTURE, OTHER, ME, FULL),
        // a Library item (owned by the Library's account) made by "us" still never passes
        item(12, lib, LIBRARY_TEXTURE, ME, OTHER, FULL),
        item(13, root, NO_COPY_TEXTURE, ME, ME, MODIFY | TRANSFER),
    ]);
    inv
}

fn prim(owner: Uuid, creator: Uuid, mask: u32) -> ExportPrim {
    ExportPrim {
        owner,
        group: GROUP,
        creator,
        owner_mask: mask,
        sculpt: None,
        mesh_creator: None,
    }
}

const BOTH: [ExportFormat; 2] = [ExportFormat::Backup, ExportFormat::Collada];

#[test]
fn owner_and_creator_with_full_permissions_may_export() {
    let inv = inventory();
    for format in BOTH {
        assert_eq!(export_prim(&inv, &prim(ME, ME, FULL), format), Ok(()));
    }
}

#[test]
fn an_owner_who_is_not_the_creator_may_not_export() {
    let inv = inventory();
    for format in BOTH {
        assert_eq!(export_prim(&inv, &prim(ME, OTHER, FULL), format), Err(ExportRefusal::NotCreator));
    }
}

#[test]
fn group_owned_and_other_residents_objects_are_refused() {
    let inv = inventory();
    for format in BOTH {
        // deeded to the group we created it for
        assert_eq!(export_prim(&inv, &prim(GROUP, ME, FULL), format), Err(ExportRefusal::NotOwner));
        // someone else's, made by us or not
        assert_eq!(export_prim(&inv, &prim(OTHER, ME, FULL), format), Err(ExportRefusal::NotOwner));
        assert_eq!(export_prim(&inv, &prim(OTHER, OTHER, FULL), format), Err(ExportRefusal::NotOwner));
        // properties not known yet: a nil owner is never ours here
        assert_eq!(
            export_prim(&inv, &prim(Uuid::nil(), ME, FULL), format),
            Err(ExportRefusal::NotOwner)
        );
    }
    let mut logged_out = inventory();
    logged_out.owner = Uuid::nil();
    assert_eq!(
        export_prim(&logged_out, &prim(Uuid::nil(), Uuid::nil(), FULL), ExportFormat::Backup),
        Err(ExportRefusal::NotOwner)
    );
}

#[test]
fn no_copy_no_modify_and_no_transfer_objects_are_refused_even_to_their_creator() {
    let inv = inventory();
    for mask in [MODIFY | TRANSFER, COPY | TRANSFER, COPY | MODIFY] {
        for format in BOTH {
            assert_eq!(
                export_prim(&inv, &prim(ME, ME, mask), format),
                Err(ExportRefusal::NotFullPermission),
                "mask {mask:#x}"
            );
        }
    }
}

#[test]
fn meshes_never_go_to_a_backup_and_go_to_collada_only_when_we_uploaded_them() {
    let inv = inventory();
    let mesh = |creator| ExportPrim {
        sculpt: Some(SculptParams {
            texture: MESH,
            sculpt_type: LL_SCULPT_TYPE_MESH,
        }),
        mesh_creator: creator,
        ..prim(ME, ME, FULL)
    };
    assert_eq!(
        export_prim(&inv, &mesh(Some(ME)), ExportFormat::Backup),
        Err(ExportRefusal::MeshInBackup)
    );
    assert_eq!(export_prim(&inv, &mesh(Some(ME)), ExportFormat::Collada), Ok(()));
    assert_eq!(
        export_prim(&inv, &mesh(Some(OTHER)), ExportFormat::Collada),
        Err(ExportRefusal::MeshCreator)
    );
    // header not known: refused until it is
    assert_eq!(
        export_prim(&inv, &mesh(None), ExportFormat::Collada),
        Err(ExportRefusal::MeshCreator)
    );
}

#[test]
fn sculpts_need_a_map_we_made() {
    let inv = inventory();
    let sculpt = |texture| ExportPrim {
        sculpt: Some(SculptParams { texture, sculpt_type: 1 }),
        ..prim(ME, ME, FULL)
    };
    for format in BOTH {
        assert_eq!(export_prim(&inv, &sculpt(MY_TEXTURE), format), Ok(()));
        for texture in [THEIR_TEXTURE, LIBRARY_TEXTURE, NO_COPY_TEXTURE, Uuid::from_u128(999)] {
            assert_eq!(export_prim(&inv, &sculpt(texture), format), Err(ExportRefusal::SculptCreator));
        }
    }
}

#[test]
fn assets_pass_only_from_our_own_full_permission_items() {
    let inv = inventory();
    assert!(export_asset(&inv, MY_TEXTURE));
    assert_eq!(export_asset_item(&inv, MY_TEXTURE).map(|it| it.id), Some(Uuid::from_u128(10)));
    assert!(!export_asset(&inv, THEIR_TEXTURE), "full permission, other creator");
    assert!(!export_asset(&inv, LIBRARY_TEXTURE), "Library");
    assert!(!export_asset(&inv, NO_COPY_TEXTURE), "no copy");
    assert!(!export_asset(&inv, Uuid::from_u128(999)), "not in the inventory");
    assert!(!export_asset(&inv, Uuid::nil()));
    let mut group = inventory();
    if let Some(it) = group.items.get_mut(&Uuid::from_u128(10)) {
        it.group_owned = true;
    }
    assert!(!export_asset(&group, MY_TEXTURE), "group owned");
}

#[test]
fn built_in_textures_stay_and_others_need_the_asset_rule() {
    use aurora_prim::te::{BLANK_TEXTURE, DEFAULT_TEXTURE, TRANSPARENT_TEXTURE};
    let inv = inventory();
    for texture in [BLANK_TEXTURE, DEFAULT_TEXTURE, TRANSPARENT_TEXTURE, MY_TEXTURE] {
        assert!(export_texture(&inv, texture));
    }
    for texture in [THEIR_TEXTURE, LIBRARY_TEXTURE, NO_COPY_TEXTURE] {
        assert!(!export_texture(&inv, texture));
    }
}
