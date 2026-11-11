use super::plan::{LeftOut, PrimInput, plan};
use super::*;
use crate::world::inventory::actions::{COPY, FULL, MODIFY};
use crate::world::inventory::{FetchState, Folder, Inventory};
use aurora_llsd::Llsd;
use aurora_net::build::ObjectProps;
use aurora_net::inventory::{InvFolder, InvItem};
use aurora_prim::params::{LL_SCULPT_TYPE_MESH, SculptParams};
use aurora_prim::te::{DEFAULT_TEXTURE, TextureEntry};

const ME: Uuid = Uuid::from_u128(2);
const OTHER: Uuid = Uuid::from_u128(3);
const MY_TEXTURE: Uuid = Uuid::from_u128(100);
const THEIR_TEXTURE: Uuid = Uuid::from_u128(101);
const MESH: Uuid = Uuid::from_u128(200);
const ROOT: Uuid = Uuid::from_u128(500);

fn inventory() -> Inventory {
    let mut inv = Inventory {
        owner: ME,
        root: Uuid::from_u128(1),
        ..Default::default()
    };
    inv.folders.insert(
        inv.root,
        Folder {
            info: InvFolder {
                id: inv.root,
                ..Default::default()
            },
            children: Vec::new(),
            items: Vec::new(),
            state: FetchState::Fetched,
            library: false,
        },
    );
    let item = |n: u128, asset: Uuid, creator: Uuid| InvItem {
        id: Uuid::from_u128(n),
        parent: Uuid::from_u128(1),
        name: format!("texture {n}"),
        asset_id: asset,
        creator,
        owner: ME,
        owner_mask: FULL,
        base_mask: FULL,
        ..Default::default()
    };
    inv.add_items(vec![item(10, MY_TEXTURE, ME), item(11, THEIR_TEXTURE, OTHER)]);
    inv
}

fn prim(n: u128, creator: Uuid, mask: u32, textures: &[Uuid]) -> PrimInput {
    let mut te = TextureEntry::default();
    for (face, id) in te.faces.iter_mut().zip(textures) {
        face.texture = *id;
    }
    let id = Uuid::from_u128(n);
    PrimInput {
        id,
        parent: (id != ROOT).then_some(ROOT),
        props: Some(ObjectProps {
            object_id: id,
            full: true,
            creator_id: creator,
            owner_id: ME,
            owner_mask: mask,
            name: format!("prim {n}"),
            ..Default::default()
        }),
        mesh_creator: None,
        material: 3,
        click_action: 0,
        flags: 0,
        attachment_point: 0,
        position: Vec3::new(n as f32 - 500.0, 0.0, 0.0),
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
        volume: Default::default(),
        faces: 6,
        te,
        extra: Default::default(),
    }
}

fn mesh(n: u128, creator: Option<Uuid>) -> PrimInput {
    let mut p = prim(n, ME, FULL, &[]);
    p.volume.sculpt = Some(SculptParams {
        texture: MESH,
        sculpt_type: LL_SCULPT_TYPE_MESH,
    });
    p.mesh_creator = creator;
    p
}

#[test]
fn a_mixed_linkset_leaves_out_others_parts_and_replaces_their_textures() {
    let inv = inventory();
    let prims = vec![
        prim(500, ME, FULL, &[MY_TEXTURE, THEIR_TEXTURE]),
        prim(501, OTHER, FULL, &[MY_TEXTURE]),
        prim(502, ME, COPY | MODIFY, &[]),
        prim(503, ME, FULL, &[THEIR_TEXTURE]),
        mesh(504, Some(OTHER)),
        mesh(505, Some(ME)),
    ];
    let backup = plan(&inv, prims.clone(), ExportFormat::Backup, |_| true);
    let ids: Vec<Uuid> = backup.prims.iter().map(|p| p.id).collect();
    assert_eq!(ids, [Uuid::from_u128(500), Uuid::from_u128(503)]);
    let why: Vec<LeftOut> = backup.left_out.iter().map(|(_, why)| *why).collect();
    assert_eq!(
        why,
        [
            LeftOut::Rights(ExportRefusal::NotCreator),
            LeftOut::Rights(ExportRefusal::NotFullPermission),
            LeftOut::Rights(ExportRefusal::MeshInBackup),
            LeftOut::Rights(ExportRefusal::MeshInBackup),
        ]
    );
    assert_eq!(backup.left_out[0].0, "prim 501");
    assert_eq!(backup.textures, [MY_TEXTURE]);
    // face 1 of the root and face 0 of 503 carried someone else's texture
    assert_eq!(backup.replaced_faces, 2);
    assert_eq!(backup.prims[0].te.faces[1].texture, DEFAULT_TEXTURE);
    assert_eq!(backup.prims[1].te.faces[0].texture, DEFAULT_TEXTURE);
    assert!(backup.prims.iter().flat_map(|p| &p.te.faces).all(|f| f.texture != THEIR_TEXTURE));

    let collada = plan(&inv, prims, ExportFormat::Collada, |_| true);
    let ids: Vec<Uuid> = collada.prims.iter().map(|p| p.id).collect();
    assert_eq!(ids, [500, 503, 505].map(Uuid::from_u128));
    assert!(
        collada
            .left_out
            .contains(&("prim 504".into(), LeftOut::Rights(ExportRefusal::MeshCreator)))
    );
}

#[test]
fn a_refused_root_refuses_the_whole_linkset() {
    let inv = inventory();
    let result = plan(
        &inv,
        vec![prim(500, OTHER, FULL, &[]), prim(501, ME, FULL, &[])],
        ExportFormat::Backup,
        |_| true,
    );
    assert!(result.prims.is_empty());
    assert_eq!(result.refused(), Some(LeftOut::Rights(ExportRefusal::NotCreator)));
    // properties that never came count as not ours
    let mut unknown = prim(500, ME, FULL, &[]);
    unknown.props = None;
    let result = plan(&inv, vec![unknown], ExportFormat::Collada, |_| true);
    assert_eq!(result.refused(), Some(LeftOut::Rights(ExportRefusal::NotOwner)));
}

#[test]
fn collada_leaves_out_sculpts_and_meshes_without_geometry() {
    let inv = inventory();
    let mut sculpt = prim(501, ME, FULL, &[]);
    sculpt.volume.sculpt = Some(SculptParams {
        texture: MY_TEXTURE,
        sculpt_type: 1,
    });
    let prims = vec![prim(500, ME, FULL, &[]), sculpt, mesh(502, Some(ME))];
    let result = plan(&inv, prims.clone(), ExportFormat::Collada, |p| p.id != Uuid::from_u128(502));
    assert_eq!(result.prims.len(), 1);
    assert_eq!(result.left_out.len(), 2);
    assert!(result.left_out.iter().all(|(_, why)| *why == LeftOut::Geometry));
    // a backup keeps our own sculpt and its map
    let backup = plan(&inv, prims, ExportFormat::Backup, |_| false);
    assert_eq!(backup.prims.len(), 2);
    assert_eq!(backup.textures, [MY_TEXTURE]);
}

#[test]
fn the_backup_holds_only_the_allowed_assets_and_reads_back() {
    let inv = inventory();
    let plan = plan(
        &inv,
        vec![prim(500, ME, FULL, &[MY_TEXTURE, THEIR_TEXTURE]), prim(501, ME, NO_TRANSFER, &[])],
        ExportFormat::Backup,
        |_| true,
    );
    let assets = vec![
        oxp::Asset {
            id: MY_TEXTURE,
            name: "mine".into(),
            description: String::new(),
            j2c: vec![1, 2, 3],
        },
        // offered by mistake: never written
        oxp::Asset {
            id: THEIR_TEXTURE,
            name: "theirs".into(),
            description: String::new(),
            j2c: vec![4, 5, 6],
        },
    ];
    let meta = oxp::Meta {
        client: "test".into(),
        date: "2026-11-10".into(),
        author: "me".into(),
        grid: "grid".into(),
    };
    let bytes = oxp::encode(&oxp::manifest(&plan, &assets, &meta)).expect("encode");
    let back = oxp::decode(&bytes).expect("decode");
    assert_eq!(back["format_version"].as_i32(), oxp::FORMAT_VERSION);
    let asset_ids: Vec<&String> = back["asset"].as_map().expect("assets").keys().collect();
    assert_eq!(asset_ids, [&MY_TEXTURE.to_string()]);
    assert_eq!(back["asset"][MY_TEXTURE.to_string().as_str()]["data"].as_binary(), &[1, 2, 3]);
    assert_eq!(back["linkset"].as_array().len(), 1);
    assert_eq!(back["linkset"][0].as_array(), &[Llsd::Uuid(ROOT)]);
    let root = &back["prim"][ROOT.to_string().as_str()];
    assert_eq!(root["texture"].as_array().len(), 6);
    assert_eq!(root["texture"][1]["imageid"].as_uuid(), DEFAULT_TEXTURE);
    assert_eq!(root["owner_mask"].as_binary(), &FULL.to_be_bytes());
    assert_eq!(root["volume"]["path"]["revolutions"].as_f32(), 1.0);
    // no id of someone else's texture anywhere in the file
    let text = format!("{back:?}");
    assert!(!text.contains(&THEIR_TEXTURE.to_string()));
    assert!(oxp::decode(b"not a backup").is_err());
}

const NO_TRANSFER: u32 = COPY | MODIFY;

#[test]
fn collada_places_children_from_the_root_and_writes_no_texture() {
    let inv = inventory();
    let mut root = prim(500, ME, FULL, &[MY_TEXTURE]);
    root.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    let mut child = prim(501, ME, FULL, &[]);
    child.position = Vec3::new(1.0, 0.0, 0.0);
    child.scale = Vec3::splat(2.0);
    let plan = plan(&inv, vec![root, child], ExportFormat::Collada, |_| true);
    let t = transforms(&plan.prims);
    assert!(t[0].w_axis.truncate().length() < 1e-6, "the root sits at the origin");
    assert!((t[1].transform_point3(Vec3::ZERO) - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5);
    assert!((t[1].transform_vector3(Vec3::X).length() - 2.0).abs() < 1e-5);
    let nodes: Vec<dae::Node> = plan
        .prims
        .iter()
        .zip(t)
        .map(|(p, transform)| dae::Node {
            name: format!("{} <&>", p.name()),
            transform,
            faces: faces(&Shape::Prim(p.volume), &[[1.0, 0.5, 0.25, 1.0]]),
        })
        .collect();
    let doc = dae::write(&nodes, "me", "test");
    assert!(doc.starts_with("<?xml"));
    assert!(doc.contains("<instance_geometry url=\"#geometry-1-0\">"));
    assert!(doc.contains("prim 500 &lt;&amp;&gt;"));
    assert!(!doc.contains(&MY_TEXTURE.to_string()), "Collada carries no asset id");
    assert_eq!(doc.matches("<node ").count(), 2);
}

#[test]
fn file_names_are_scrubbed() {
    assert_eq!(file_name("My: chair/1?", ExportFormat::Backup), "My_ chair_1_.oxp");
    assert_eq!(file_name("  ", ExportFormat::Collada), "object.dae");
}
