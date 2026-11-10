//! What an export of a linkset writes: each prim through the shared rule
//! (`world::inventory::actions::export_prim`, the port of
//! FSExportPermsCheck::canExportNode), each texture through
//! `actions::export_texture` (canExportAsset). A prim that fails is left out
//! and named to the user; a face texture that fails becomes the default
//! texture, as the Legacy Viewer's backup does (FSFloaterObjectExport::addPrim).

use crate::world::inventory::Inventory;
use crate::world::inventory::actions::{self, ExportFormat, ExportPrim, ExportRefusal};
use aurora_net::build::ObjectProps;
use aurora_prim::extra::ExtraParams;
use aurora_prim::params::VolumeParams;
use aurora_prim::te::{DEFAULT_TEXTURE, TextureEntry};
use glam::{Quat, Vec3};
use uuid::Uuid;

/// One prim of the linkset as the world holds it.
#[derive(Debug, Clone)]
pub struct PrimInput {
    pub id: Uuid,
    /// The root's id for a child prim, None for the root.
    pub parent: Option<Uuid>,
    /// Full ObjectProperties; None while they have not come.
    pub props: Option<ObjectProps>,
    pub mesh_creator: Option<Uuid>,
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
    pub faces: usize,
    pub te: TextureEntry,
    pub extra: ExtraParams,
}

impl PrimInput {
    fn rule_input(&self) -> ExportPrim {
        let props = self.props.as_ref();
        ExportPrim {
            owner: props.map_or(Uuid::nil(), |p| p.owner_id),
            group: props.map_or(Uuid::nil(), |p| p.group_id),
            creator: props.map_or(Uuid::nil(), |p| p.creator_id),
            owner_mask: props.map_or(0, |p| p.owner_mask),
            sculpt: self.volume.sculpt.or(self.extra.sculpt),
            mesh_creator: self.mesh_creator,
        }
    }

    pub fn name(&self) -> &str {
        self.props.as_ref().map_or("", |p| p.name.as_str())
    }
}

/// Why a prim was left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeftOut {
    Rights(ExportRefusal),
    /// Collada: a sculpt (its map is a picture this export does not read),
    /// or a mesh whose geometry is not loaded.
    Geometry,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// Root first; textures that failed the rule already replaced.
    pub prims: Vec<PrimInput>,
    pub left_out: Vec<(String, LeftOut)>,
    /// Textures that passed the rule (built-in ones excepted): their data
    /// may go into a backup.
    pub textures: Vec<Uuid>,
    /// Faces whose texture became the default texture.
    pub replaced_faces: usize,
    /// Materials, light textures and other references left out.
    pub dropped: usize,
}

impl Plan {
    /// Nothing can be written: the root itself was refused.
    pub fn refused(&self) -> Option<LeftOut> {
        self.prims.is_empty().then(|| self.left_out.first().map(|(_, why)| *why)).flatten()
    }
}

/// The plan of `format` for a linkset, root first in `prims`. When the root
/// fails the whole linkset is refused (its children would have no root).
pub fn plan(inv: &Inventory, prims: Vec<PrimInput>, format: ExportFormat, has_geometry: impl Fn(&PrimInput) -> bool) -> Plan {
    let mut out = Plan::default();
    for (i, mut prim) in prims.into_iter().enumerate() {
        let sculpted = prim.volume.sculpt.or(prim.extra.sculpt);
        let verdict = actions::export_prim(inv, &prim.rule_input(), format)
            .map_err(LeftOut::Rights)
            .and_then(|()| {
                let geometry = format == ExportFormat::Backup || (sculpted.is_none_or(|s| s.is_mesh()) && has_geometry(&prim));
                if geometry { Ok(()) } else { Err(LeftOut::Geometry) }
            });
        if let Err(why) = verdict {
            out.left_out.push((prim.name().to_owned(), why));
            if i == 0 {
                return Plan { prims: Vec::new(), ..out };
            }
            continue;
        }
        let mut te = prim.te.clone();
        for face in te.faces.iter_mut().take(prim.faces.max(1)) {
            if actions::export_texture(inv, face.texture) {
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
            if actions::export_texture(inv, light.texture) {
                note(&mut out.textures, light.texture);
            } else {
                prim.extra.light_image = None;
                out.dropped += 1;
            }
        }
        if let Some(sculpt) = sculpted.filter(|s| !s.is_mesh()) {
            note(&mut out.textures, sculpt.texture);
        }
        out.dropped += prim.extra.render_materials.len();
        prim.extra.render_materials.clear();
        out.prims.push(prim);
    }
    out
}

fn note(list: &mut Vec<Uuid>, id: Uuid) {
    use aurora_prim::te::{BLANK_TEXTURE, TRANSPARENT_TEXTURE};
    if ![DEFAULT_TEXTURE, BLANK_TEXTURE, TRANSPARENT_TEXTURE, Uuid::nil()].contains(&id) && !list.contains(&id) {
        list.push(id);
    }
}
