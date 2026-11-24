//! What a linkset backup (`.oxp`) writes: each prim through the creator
//! rule (`rights::backup_prim`, the port of FSExportPermsCheck::canExportNode
//! for a backup), each texture through `rights::backup_asset_allowed`
//! (canExportAsset). A prim that fails is left out; a face texture that
//! fails becomes the default texture, as the Legacy Viewer's backup does
//! (FSFloaterObjectExport::addPrim); a refused root takes its whole linkset
//! with it (its children would have no root).

use super::rights::{self, Context, PrimFacts, Shape};
use crate::world::World;
use crate::world::objects::ObjKey;
use aurora_net::build::ObjectProps;
use aurora_prim::extra::ExtraParams;
use aurora_prim::params::VolumeParams;
use aurora_prim::particles::PartSysData;
use aurora_prim::te::{BLANK_TEXTURE, DEFAULT_TEXTURE, TRANSPARENT_TEXTURE, TextureEntry};
use glam::{Quat, Vec3};
use std::collections::HashMap;
use uuid::Uuid;

/// One prim of a linkset as the world holds it.
#[derive(Debug, Clone)]
pub struct PrimInput {
    pub id: Uuid,
    /// The root's id for a child prim, None for the root.
    pub parent: Option<Uuid>,
    /// Full ObjectProperties; None while they have not come.
    pub props: Option<ObjectProps>,
    pub material: u8,
    pub click_action: u8,
    pub flags: u32,
    /// Attachment point of a worn root (0 in the world).
    pub attachment_point: u8,
    /// Relative to the parent for a child, as the simulator sends them.
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub volume: VolumeParams,
    /// How many faces' texture entries are written.
    pub faces: usize,
    pub te: TextureEntry,
    pub extra: ExtraParams,
    pub particles: Option<PartSysData>,
}

impl PrimInput {
    fn root(&self) -> Uuid {
        self.parent.unwrap_or(self.id)
    }

    pub fn name(&self) -> &str {
        self.props.as_ref().map_or("", |p| p.name.as_str())
    }

    fn sculpt(&self) -> Option<aurora_prim::params::SculptParams> {
        self.volume.sculpt.or(self.extra.sculpt)
    }

    /// What the creator rule reads. None while the properties have not come.
    fn facts(&self, comments: &HashMap<Uuid, Option<Uuid>>) -> Option<PrimFacts> {
        let props = self.props.as_ref()?;
        let shape = match self.sculpt() {
            Some(s) if s.is_mesh() => Shape::Mesh { creator: None },
            Some(s) => Shape::Sculpt {
                map: s.texture,
                uploader: comments.get(&s.texture).copied().flatten(),
            },
            None => Shape::Prim,
        };
        Some(PrimFacts {
            creator: props.creator_id,
            owner: props.owner_id,
            owner_mask: props.owner_mask,
            everyone_mask: props.everyone_mask,
            group_owned: !props.owner_id.is_nil() && props.owner_id == props.group_id,
            scale: self.scale,
            shape,
        })
    }
}

/// Every prim of the linksets the selection `keys` touches: a backup holds
/// whole linksets (a child alone has no root to hang on), the first prim of
/// each being its root.
pub fn linksets_of(world: &World, keys: &[ObjKey]) -> Vec<ObjKey> {
    let mut roots: Vec<usize> = Vec::new();
    for key in keys {
        if let Some(root) = root_index(world, *key)
            && !roots.contains(&root)
        {
            roots.push(root);
        }
    }
    roots
        .into_iter()
        .flat_map(|root| crate::build::family(world, root))
        .filter_map(|i| world.objects.get(i))
        .filter(|o| !o.is_avatar())
        .map(|o| o.key)
        .collect()
}

/// The prims of the objects `keys` as the world holds them, with the
/// properties `props` gives. Root first within a linkset is the plan's
/// business.
pub fn inputs(world: &World, keys: &[ObjKey], props: &dyn Fn(Uuid) -> Option<ObjectProps>) -> Vec<PrimInput> {
    keys.iter()
        .filter_map(|k| world.objects.index_of(k).and_then(|i| world.objects.get(i)))
        .filter(|o| !o.is_avatar() && o.pcode == aurora_prim::params::LL_PCODE_VOLUME && !o.is_tree())
        .map(|o| {
            let root = root_index(world, o.key).and_then(|i| world.objects.get(i)).map(|r| r.full_id);
            let sculpt = o.volume.sculpt.or(o.extra.sculpt);
            let faces = match sculpt {
                Some(_) => 1,
                None => aurora_prim::volume::num_faces(&o.volume).max(1),
            };
            PrimInput {
                id: o.full_id,
                parent: root.filter(|r| *r != o.full_id),
                props: props(o.full_id).filter(|p| p.full),
                material: o.prim_material,
                click_action: o.click_action,
                flags: o.update_flags,
                attachment_point: if root.is_none_or(|r| r == o.full_id) { o.attachment_point() } else { 0 },
                position: o.position,
                rotation: o.rotation,
                scale: o.scale,
                volume: o.volume,
                faces,
                te: o.te.as_deref().cloned().unwrap_or_default(),
                extra: o.extra.clone(),
                particles: o.particles.as_deref().and_then(PartSysData::parse),
            }
        })
        .collect()
}

/// The linkset root above `key`: the parents up to the one that sits on no
/// other prim (an attachment's root is the prim on the avatar).
fn root_index(world: &World, key: ObjKey) -> Option<usize> {
    let mut i = world.objects.index_of(&key)?;
    for _ in 0..256 {
        match world.objects.get(i).and_then(|o| world.objects.parent_of(o)) {
            Some(parent) if world.objects.get(parent).is_some_and(|o| !o.is_avatar()) => i = parent,
            _ => break,
        }
    }
    Some(i)
}

/// What the file holds.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// One list per linkset, root first; textures that failed the rule
    /// already replaced.
    pub linksets: Vec<Vec<PrimInput>>,
    /// Prims left out: refused by the rule, or without their properties.
    pub left_out: usize,
    /// Textures, sculpt maps, particle and light images that passed the rule
    /// (built-in ones excepted): their data goes into the file.
    pub textures: Vec<Uuid>,
    /// Faces whose texture became the default texture.
    pub replaced_faces: usize,
    /// Materials and other references left out.
    pub dropped: usize,
}

impl Plan {
    pub fn prims(&self) -> usize {
        self.linksets.iter().map(Vec::len).sum()
    }
}

/// Every texture, sculpt map and particle or light image the prims name:
/// the assets whose items the rule reads.
pub fn assets(prims: &[PrimInput]) -> std::collections::HashSet<Uuid> {
    let mut out = std::collections::HashSet::new();
    for p in prims {
        out.extend(p.te.faces.iter().take(p.faces.max(1)).map(|f| f.texture));
        out.extend(p.sculpt().map(|s| s.texture));
        out.extend(p.extra.light_image.map(|l| l.texture));
        out.extend(p.particles.map(|ps| ps.part_image_id));
    }
    out
}

/// A built-in texture's id may always be written (FSCommon::isDefaultTexture).
fn built_in(id: Uuid) -> bool {
    [BLANK_TEXTURE, DEFAULT_TEXTURE, TRANSPARENT_TEXTURE].contains(&id)
}

fn note(list: &mut Vec<Uuid>, id: Uuid) {
    if !built_in(id) && !id.is_nil() && !list.contains(&id) {
        list.push(id);
    }
}

/// The plan for `prims`, the linksets in the order their first prim comes.
/// `comments` holds the uploaders read from the textures' J2C comments.
pub fn plan(cx: &Context, prims: Vec<PrimInput>, comments: &HashMap<Uuid, Option<Uuid>>) -> Plan {
    let mut out = Plan::default();
    let mut order: Vec<Uuid> = Vec::new();
    let mut sets: HashMap<Uuid, Vec<PrimInput>> = HashMap::new();
    for prim in prims {
        let root = prim.root();
        if !sets.contains_key(&root) {
            order.push(root);
        }
        sets.entry(root).or_default().push(prim);
    }
    let texture_ok = |id: Uuid| built_in(id) || rights::backup_asset_allowed(cx, id, comments.get(&id).copied().flatten());
    for root in order {
        let set = sets.remove(&root).unwrap_or_default();
        let allowed = |p: &PrimInput| p.facts(comments).is_some_and(|f| rights::backup_prim(cx, &f));
        let Some(root_prim) = set.iter().find(|p| p.id == root).filter(|p| allowed(*p)) else {
            out.left_out += set.len();
            continue;
        };
        let mut kept = vec![root_prim.clone()];
        for p in set.iter().filter(|p| p.id != root) {
            if allowed(p) {
                kept.push(p.clone());
            } else {
                out.left_out += 1;
            }
        }
        for prim in &mut kept {
            apply_textures(prim, &texture_ok, &mut out);
        }
        out.linksets.push(kept);
    }
    out
}

fn apply_textures(prim: &mut PrimInput, texture_ok: &dyn Fn(Uuid) -> bool, out: &mut Plan) {
    let sculpt = prim.sculpt().filter(|s| !s.is_mesh());
    let mut te = prim.te.clone();
    for face in te.faces.iter_mut().take(prim.faces.max(1)) {
        if texture_ok(face.texture) {
            note(&mut out.textures, face.texture);
        } else {
            face.texture = DEFAULT_TEXTURE;
            out.replaced_faces += 1;
        }
        // legacy materials and their maps are not followed: left out
        if !face.material_id.is_nil() {
            face.material_id = Uuid::nil();
            out.dropped += 1;
        }
    }
    prim.te = te;
    if let Some(light) = prim.extra.light_image {
        if texture_ok(light.texture) {
            note(&mut out.textures, light.texture);
        } else {
            prim.extra.light_image = None;
            out.dropped += 1;
        }
    }
    // the rule held the prim to its sculpt map already
    if let Some(sculpt) = sculpt {
        note(&mut out.textures, sculpt.texture);
    }
    // FSFloaterObjectExport::addPrim: a particle texture that fails the
    // rule is written as the null id
    if let Some(particles) = prim.particles.as_mut() {
        if texture_ok(particles.part_image_id) {
            note(&mut out.textures, particles.part_image_id);
        } else if !particles.part_image_id.is_nil() {
            particles.part_image_id = Uuid::nil();
            out.dropped += 1;
        }
    }
    out.dropped += prim.extra.render_materials.len();
    prim.extra.render_materials.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::oxp;
    use crate::world::inventory::Inventory;
    use crate::world::inventory::actions::{COPY, ExportGrid, FULL, MODIFY};
    use aurora_net::inventory::InvItem;
    use aurora_prim::params::{LL_SCULPT_TYPE_MESH, SculptParams};
    use aurora_prim::particles::{LL_PART_EMISSIVE_MASK, LL_PART_SRC_PATTERN_EXPLODE};
    use aurora_llsd::Llsd;
    use std::collections::HashSet;

    const ME: Uuid = Uuid::from_u128(2);
    const OTHER: Uuid = Uuid::from_u128(3);
    const MY_TEXTURE: Uuid = Uuid::from_u128(100);
    const THEIR_TEXTURE: Uuid = Uuid::from_u128(101);
    const MESH: Uuid = Uuid::from_u128(200);
    const ROOT: Uuid = Uuid::from_u128(500);

    fn inventory() -> Inventory {
        let mut inv = Inventory {
            owner: ME,
            ..Default::default()
        };
        for (n, asset, creator, owner) in [(10u128, MY_TEXTURE, ME, ME), (11, THEIR_TEXTURE, OTHER, ME)] {
            let id = Uuid::from_u128(n);
            inv.items.insert(
                id,
                InvItem {
                    id,
                    name: format!("texture {n}"),
                    desc: format!("about {n}"),
                    asset_id: asset,
                    creator,
                    owner,
                    owner_mask: FULL,
                    ..Default::default()
                },
            );
        }
        inv
    }

    fn plan_of(inv: &Inventory, prims: Vec<PrimInput>, comments: &HashMap<Uuid, Option<Uuid>>) -> Plan {
        let assets: HashSet<Uuid> = inv.items.values().map(|i| i.asset_id).collect();
        // whatever the grid the session is on, a backup follows the creator rule
        let cx = Context::new(ME, ExportGrid::OpenSim { export_supported: true }, 64.0, inv, &assets);
        plan(&cx, prims, comments)
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
            particles: None,
        }
    }

    fn sculpted(n: u128, map: Uuid) -> PrimInput {
        let mut p = prim(n, ME, FULL, &[]);
        p.volume.sculpt = Some(SculptParams {
            texture: map,
            sculpt_type: 1,
        });
        p
    }

    fn meta() -> oxp::Meta {
        oxp::Meta {
            client: "test".into(),
            date: "2026-11-10".into(),
            author: "me".into(),
            grid: "grid".into(),
        }
    }

    #[test]
    fn a_mixed_linkset_leaves_out_others_parts_and_replaces_their_textures() {
        let inv = inventory();
        let mut mesh = prim(504, ME, FULL, &[]);
        mesh.volume.sculpt = Some(SculptParams {
            texture: MESH,
            sculpt_type: LL_SCULPT_TYPE_MESH,
        });
        let prims = vec![
            prim(500, ME, FULL, &[MY_TEXTURE, THEIR_TEXTURE]),
            prim(501, OTHER, FULL, &[MY_TEXTURE]),
            prim(502, ME, COPY | MODIFY, &[]),
            prim(503, ME, FULL, &[THEIR_TEXTURE]),
            mesh,
        ];
        let plan = plan_of(&inv, prims, &HashMap::new());
        let ids: Vec<Uuid> = plan.linksets[0].iter().map(|p| p.id).collect();
        assert_eq!(ids, [Uuid::from_u128(500), Uuid::from_u128(503)]);
        assert_eq!(plan.left_out, 3, "someone else's prim, no transfer, a mesh");
        assert_eq!(plan.textures, [MY_TEXTURE]);
        // face 1 of the root and face 0 of 503 carried someone else's texture
        assert_eq!(plan.replaced_faces, 2);
        assert_eq!(plan.linksets[0][0].te.faces[1].texture, DEFAULT_TEXTURE);
        assert_eq!(plan.linksets[0][1].te.faces[0].texture, DEFAULT_TEXTURE);
        assert!(plan.linksets[0].iter().flat_map(|p| &p.te.faces).all(|f| f.texture != THEIR_TEXTURE));
    }

    #[test]
    fn a_refused_root_refuses_the_whole_linkset() {
        let inv = inventory();
        let plan = plan_of(
            &inv,
            vec![prim(500, OTHER, FULL, &[]), prim(501, ME, FULL, &[])],
            &HashMap::new(),
        );
        assert!(plan.linksets.is_empty());
        assert_eq!(plan.left_out, 2);
        // properties that never came count as not ours
        let mut unknown = prim(500, ME, FULL, &[]);
        unknown.props = None;
        assert!(plan_of(&inv, vec![unknown], &HashMap::new()).linksets.is_empty());
        // a child whose root is not among the prims has no root to hang on
        assert!(plan_of(&inv, vec![prim(501, ME, FULL, &[])], &HashMap::new()).linksets.is_empty());
    }

    #[test]
    fn a_group_owned_prim_is_never_saved() {
        let inv = inventory();
        let mut p = prim(500, ME, FULL, &[]);
        if let Some(props) = p.props.as_mut() {
            props.group_id = ME;
        }
        assert!(plan_of(&inv, vec![p], &HashMap::new()).linksets.is_empty());
    }

    #[test]
    fn a_sculpt_needs_a_map_the_agent_made() {
        let inv = inventory();
        let prims = vec![prim(500, ME, FULL, &[]), sculpted(501, MY_TEXTURE), sculpted(502, THEIR_TEXTURE)];
        let plan = plan_of(&inv, prims.clone(), &HashMap::new());
        let ids: Vec<Uuid> = plan.linksets[0].iter().map(|p| p.id).collect();
        assert_eq!(ids, [500, 501].map(Uuid::from_u128));
        assert_eq!(plan.textures, [MY_TEXTURE], "its map goes with it");
        // the J2C comment names us: a map with no item of ours is fine
        let unknown = Uuid::from_u128(300);
        let with_comment = HashMap::from([(unknown, Some(ME))]);
        let plan = plan_of(&inv, vec![prim(500, ME, FULL, &[]), sculpted(501, unknown)], &with_comment);
        assert_eq!(plan.prims(), 2);
        // a comment naming someone else does not
        let theirs = HashMap::from([(unknown, Some(OTHER))]);
        assert_eq!(plan_of(&inv, vec![prim(500, ME, FULL, &[]), sculpted(501, unknown)], &theirs).prims(), 1);
    }

    #[test]
    fn two_linksets_are_kept_apart() {
        let inv = inventory();
        let other_root = Uuid::from_u128(600);
        let mut b = prim(601, ME, FULL, &[]);
        b.parent = Some(other_root);
        let mut root_b = prim(600, ME, FULL, &[]);
        root_b.parent = None;
        let plan = plan_of(&inv, vec![prim(500, ME, FULL, &[]), b, root_b, prim(501, ME, FULL, &[])], &HashMap::new());
        assert_eq!(plan.linksets.len(), 2);
        assert_eq!(plan.linksets[0].iter().map(|p| p.id).collect::<Vec<_>>(), [500, 501].map(Uuid::from_u128));
        assert_eq!(plan.linksets[1].iter().map(|p| p.id).collect::<Vec<_>>(), [600, 601].map(Uuid::from_u128));
    }

    #[test]
    fn the_backup_holds_only_the_allowed_assets_and_reads_back() {
        let inv = inventory();
        let plan = plan_of(
            &inv,
            vec![prim(500, ME, FULL, &[MY_TEXTURE, THEIR_TEXTURE]), prim(501, ME, COPY | MODIFY, &[])],
            &HashMap::new(),
        );
        assert_eq!(plan.prims(), 1, "no transfer: left out");
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
        let bytes = oxp::encode(&oxp::manifest(&plan, &assets, &meta())).expect("encode");
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
        assert!(!format!("{back:?}").contains(&THEIR_TEXTURE.to_string()));
        assert!(oxp::decode(b"not a backup").is_err());
    }

    #[test]
    fn particles_and_sculpts_keep_only_allowed_textures_in_the_backup() {
        use aurora_prim::particles::PartSysData;
        let inv = inventory();
        let mut mine = prim(500, ME, FULL, &[]);
        let mut ps = PartSysData {
            pattern: LL_PART_SRC_PATTERN_EXPLODE,
            part_image_id: MY_TEXTURE,
            burst_part_count: 7,
            ..Default::default()
        };
        ps.part.flags = LL_PART_EMISSIVE_MASK;
        mine.particles = Some(ps);
        let mut child = prim(501, ME, FULL, &[]);
        child.particles = Some(PartSysData {
            part_image_id: THEIR_TEXTURE,
            ..Default::default()
        });
        let plan = plan_of(&inv, vec![mine, child], &HashMap::new());
        assert_eq!(plan.textures, [MY_TEXTURE]);
        assert_eq!(plan.linksets[0][1].particles.map(|p| p.part_image_id), Some(Uuid::nil()));
        let manifest = oxp::manifest(&plan, &[], &meta());
        let root = &manifest["prim"][ROOT.to_string().as_str()]["particle"];
        assert_eq!(root["PartImageID"].to_string_value(), MY_TEXTURE.to_string());
        assert_eq!(root["LL_PART_EMISSIVE_MASK"].as_i32(), 1);
        assert_eq!(root["LL_PART_SRC_PATTERN_EXPLODE"].as_i32(), 1);
        assert_eq!(root["LL_PART_SRC_PATTERN_DROP"].as_i32(), 0);
        assert_eq!(root["BurstPartCount"].as_i32(), 7);
        let child = &manifest["prim"][Uuid::from_u128(501).to_string().as_str()]["particle"];
        assert_eq!(child["PartImageID"].to_string_value(), Uuid::nil().to_string());
        // the sculpt's map id is written with the prim, since the rule let it pass
        let manifest = oxp::manifest(&plan_of(&inv, vec![prim(500, ME, FULL, &[]), sculpted(501, MY_TEXTURE)], &HashMap::new()), &[], &meta());
        assert_eq!(
            manifest["prim"][Uuid::from_u128(501).to_string().as_str()]["sculpt"]["texture"].as_uuid(),
            MY_TEXTURE
        );
    }
}
