//! The Legacy Viewer's linkset backup (`.oxp`, format version 2): one LLSD
//! map, binary and zlib-compressed (zip_llsd), as
//! FSFloaterObjectExport::exportSelection / addPrim / saveFormattedImage
//! write it: `linkset` (the ids of each linkset, root first), `prim` (each
//! prim's shape, faces, extra parameters and properties) and `asset` (the
//! JPEG 2000 data of the textures the export rule allows). Contents,
//! particles and materials are not written.

use super::plan::{Plan, PrimInput};
use aurora_llsd::{Llsd, Map, llsd_map};
use aurora_prim::te::TextureFace;
use std::io::{Read, Write};
use uuid::Uuid;

/// OXP_FORMAT_VERSION (fsexportperms.h).
pub const FORMAT_VERSION: i32 = 2;
/// Largest file read back (a backup holds JPEG 2000 textures).
const MAX_FILE: u64 = 512 << 20;

/// A texture whose data goes into the file, with the name and description
/// of the inventory item that let it pass (canExportAsset).
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub j2c: Vec<u8>,
}

/// The header fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub client: String,
    /// YYYY-MM-DD (UTC).
    pub date: String,
    pub author: String,
    pub grid: String,
}

/// ll_sd_from_U32: four bytes in network order.
fn u32_sd(v: u32) -> Llsd {
    Llsd::Binary(v.to_be_bytes().to_vec())
}

/// ll_sd_from_U64: eight bytes in network order.
fn u64_sd(v: u64) -> Llsd {
    Llsd::Binary(v.to_be_bytes().to_vec())
}

fn reals(v: &[f32]) -> Llsd {
    Llsd::Array(v.iter().map(|x| Llsd::Real(f64::from(*x))).collect())
}

/// LLTextureEntry::asLLSD.
fn face(f: &TextureFace) -> Llsd {
    llsd_map! {
        "imageid" => f.texture,
        "colors" => reals(&f.color),
        "scales" => f.scale_s,
        "scalet" => f.scale_t,
        "offsets" => f.offset_s,
        "offsett" => f.offset_t,
        "imagerot" => f.rotation,
        // getBumpShiny: everything but the fullbright bit
        "bump" => i32::from(f.bump_shiny_fullbright & !0x20),
        "fullbright" => i32::from(f.fullbright()),
        "media_flags" => i32::from(f.media_flags),
        "glow" => f.glow,
    }
}

/// LLSaleInfo::asLLSD.
fn sale(kind: u8, price: i32) -> Llsd {
    let kind = match kind {
        1 => "orig",
        2 => "copy",
        3 => "cntn",
        _ => "not",
    };
    llsd_map! { "sale_type" => kind, "sale_price" => price }
}

fn prim(p: &PrimInput) -> Llsd {
    let mut m = Map::new();
    let mut set = |k: &str, v: Llsd| {
        m.insert(k.to_owned(), v);
    };
    match p.parent {
        Some(parent) => set("parent", parent.into()),
        None if p.attachment_point != 0 => set("attachment_point", i32::from(p.attachment_point).into()),
        None => {}
    }
    set("position", reals(&p.position.to_array()));
    set("scale", reals(&p.scale.to_array()));
    set("rotation", reals(&p.rotation.to_array()));
    set("flags", u32_sd(p.flags));
    let path = &p.volume.path;
    let profile = &p.volume.profile;
    set(
        "volume",
        llsd_map! {
            "path" => llsd_map! {
                "curve" => i32::from(path.curve_type),
                "begin" => path.begin,
                "end" => path.end,
                "scale_x" => path.scale[0],
                "scale_y" => path.scale[1],
                "shear_x" => path.shear[0],
                "shear_y" => path.shear[1],
                "twist" => path.twist_end,
                "twist_begin" => path.twist_begin,
                "radius_offset" => path.radius_offset,
                "taper_x" => path.taper[0],
                "taper_y" => path.taper[1],
                "revolutions" => path.revolutions,
                "skew" => path.skew,
            },
            "profile" => llsd_map! {
                "curve" => i32::from(profile.curve_type),
                "begin" => profile.begin,
                "end" => profile.end,
                "hollow" => profile.hollow,
            },
        },
    );
    set("material", i32::from(p.material).into());
    if p.click_action != 0 {
        set("clickaction", i32::from(p.click_action).into());
    }
    if let Some(s) = p.volume.sculpt.or(p.extra.sculpt) {
        set("sculpt", llsd_map! { "texture" => s.texture, "type" => i32::from(s.sculpt_type) });
    }
    if let Some(f) = p
        .extra
        .flexible
        .filter(|_| p.volume.path.curve_type == aurora_prim::params::LL_PCODE_PATH_FLEXIBLE)
    {
        set(
            "flexible",
            llsd_map! {
                "air_friction" => f.air_friction,
                "gravity" => f.gravity,
                "simulate_lod" => i32::from(f.softness),
                "tension" => f.tension,
                "user_force" => reals(&f.user_force.to_array()),
                "wind_sensitivity" => f.wind_sensitivity,
            },
        );
    }
    if let Some(l) = p.extra.light {
        set(
            "light",
            llsd_map! {
                "color" => reals(&l.color),
                "radius" => l.radius,
                "falloff" => l.falloff,
                "cutoff" => l.cutoff,
            },
        );
    }
    if let Some(l) = p.extra.light_image {
        set(
            "light_texture",
            llsd_map! { "texture" => l.texture, "params" => reals(&l.params.to_array()) },
        );
    }
    set("texture", Llsd::Array(p.te.faces.iter().take(p.faces.max(1)).map(face).collect()));
    if let Some(props) = &p.props {
        set("name", props.name.clone().into());
        set("description", props.description.clone().into());
        set("creation_date", u64_sd(props.creation_date));
        for (key, id) in [
            ("creator_id", props.creator_id),
            ("owner_id", props.owner_id),
            ("group_id", props.group_id),
            ("last_owner_id", props.last_owner_id),
        ] {
            if !id.is_nil() {
                set(key, id.into());
            }
        }
        set("base_mask", u32_sd(props.base_mask));
        set("owner_mask", u32_sd(props.owner_mask));
        set("group_mask", u32_sd(props.group_mask));
        set("everyone_mask", u32_sd(props.everyone_mask));
        set("next_owner_mask", u32_sd(props.next_owner_mask));
        set("sale_info", sale(props.sale_type, props.sale_price));
        set("touch_name", props.touch_name.clone().into());
        set("sit_name", props.sit_name.clone().into());
    }
    Llsd::Map(m)
}

/// The manifest of one linkset. Only the assets of `plan.textures` are
/// written: anything else in `assets` is ignored.
pub fn manifest(plan: &Plan, assets: &[Asset], meta: &Meta) -> Llsd {
    let mut prims = Map::new();
    for p in &plan.prims {
        prims.insert(p.id.to_string(), prim(p));
    }
    let mut asset_map = Map::new();
    for a in assets.iter().filter(|a| plan.textures.contains(&a.id) && !a.j2c.is_empty()) {
        asset_map.insert(
            a.id.to_string(),
            llsd_map! {
                "name" => a.name.clone(),
                "description" => a.description.clone(),
                "type" => "texture",
                "data" => a.j2c.clone(),
            },
        );
    }
    let linkset = Llsd::Array(plan.prims.iter().map(|p| Llsd::Uuid(p.id)).collect());
    llsd_map! {
        "format_version" => FORMAT_VERSION,
        "client" => meta.client.clone(),
        "creation_date" => meta.date.clone(),
        "author" => meta.author.clone(),
        "grid" => meta.grid.clone(),
        "linkset" => Llsd::Array(vec![linkset]),
        "prim" => Llsd::Map(prims),
        "asset" => Llsd::Map(asset_map),
    }
}

/// zip_llsd: binary LLSD, zlib-compressed.
pub fn encode(manifest: &Llsd) -> std::io::Result<Vec<u8>> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&aurora_llsd::to_binary(manifest))?;
    z.finish()
}

/// unzip_llsd, for reading a backup back (bounded).
pub fn decode(bytes: &[u8]) -> Result<Llsd, String> {
    let mut raw = Vec::new();
    flate2::read::ZlibDecoder::new(bytes)
        .take(MAX_FILE)
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    aurora_llsd::from_binary_local(&raw).map(|(v, _)| v).map_err(|e| e.to_string())
}
