//! The look and flags a prim made with the Create tool takes when it first
//! arrives: colour, transparency, glow, full bright, shininess, physical,
//! temporary and phantom (Preferences › Build › New objects).
//!
//! Port of FSCommon::applyDefaultBuildPreferences (indra/newview/fscommon.cpp)
//! and its call in LLViewerObjectList::processObjectUpdate
//! (llviewerobjectlist.cpp), with the count kept by LLToolPlacer::addObject
//! and cleared by LLToolDragAndDrop::dropObject, originally LGPL 2.1: one
//! ObjectImage with every face set, then one ObjectFlagUpdate, for each
//! object the Create tool asked for and that arrives created selected, ours
//! with full permissions.
//!
//! The default texture (FSDefaultObjectTexture) goes on every face when one
//! is chosen, and the item chosen for new prims (FSBuildPrefs_EmbedItem /
//! FSBuildPrefs_Item) goes into the prim as a drop on its Contents tab
//! would put it (`contents_drop::plan`): scripts running, a no-copy item
//! leaving the inventory (settings/new_objects.rs).
//!
//! Deliberate difference: the Legacy Viewer writes a fresh texture entry on
//! every face; Aurora keeps each face's mapping as the region made them and
//! sets only the values above. A new prim's faces are the defaults already.
//! The default texture is used only while its asset id may be shown
//! (`actions::asset_uuid_shown`), stricter than the Legacy Viewer. The next
//! owner permissions sent there are not ported yet.

use super::{BuildSettings, BuildTool, FLAGS_CREATE_SELECTED};
use crate::ui::context::flags::{OBJECT_COPY, OBJECT_MODIFY, OBJECT_TRANSFER, OBJECT_YOU_OWNER};
use crate::world::World;
use aurora_net::build::BuildCmd;
use aurora_prim::te::TextureFace;
use std::sync::Arc;

/// TEM_FULLBRIGHT_MASK and the shininess bits of a face's bump / shiny /
/// fullbright byte (lltextureentry.h).
const TEM_FULLBRIGHT: u8 = 0x20;
const TEM_SHINY_SHIFT: u8 = 6;
const TEM_SHINY_MASK: u8 = 0xc0;

/// The rights applyDefaultBuildPreferences asks of the object
/// (own_full_perm: permYouOwner, permModify, permTransfer, permCopy).
const FULL_RIGHTS: u32 = OBJECT_YOU_OWNER | OBJECT_MODIFY | OBJECT_COPY | OBJECT_TRANSFER;

/// A face with the new objects' look, and `texture` when one is chosen.
pub fn styled_face(face: &TextureFace, s: &BuildSettings, texture: Option<uuid::Uuid>) -> TextureFace {
    let [r, g, b, _] = s.new_prim_color;
    let channel = |v: u8| v as f32 / 255.0;
    let mut out = *face;
    if let Some(texture) = texture {
        out.texture = texture;
    }
    // (the colour's own alpha is not used: setAlpha follows setColor)
    out.color = [channel(r), channel(g), channel(b), (100.0 - s.new_prim_transparency) / 100.0];
    out.glow = s.new_prim_glow;
    let shiny = s.new_prim_shiny.min(3) << TEM_SHINY_SHIFT;
    let fullbright = if s.new_prim_fullbright { TEM_FULLBRIGHT } else { 0 };
    out.bump_shiny_fullbright = (face.bump_shiny_fullbright & !(TEM_SHINY_MASK | TEM_FULLBRIGHT)) | shiny | fullbright;
    out
}

impl BuildTool {
    /// The Create tool asked the region for one more prim
    /// (LLToolPlacer::addObject, `sObjectAddMsg++`).
    pub(super) fn expect_new_defaults(&mut self) {
        self.new_defaults_pending = self.new_defaults_pending.saturating_add(1);
    }

    /// An object rezzed from inventory: nothing the Create tool asked for is
    /// still to come (LLToolDragAndDrop::dropObject, "prevent default build
    /// parms from being applied due to lost packet").
    pub fn forget_new_defaults(&mut self) {
        self.new_defaults_pending = 0;
    }

    /// Gives a newly arrived object `idx` the new objects' look and flags,
    /// when the Create tool asked for one and the object is ours with full
    /// permissions.
    pub(super) fn apply_new_defaults(&mut self, world: &mut World, idx: usize, s: &BuildSettings) {
        if self.new_defaults_pending == 0 {
            return;
        }
        let Some(o) = world.objects.get_mut(idx) else { return };
        if o.update_flags & FLAGS_CREATE_SELECTED == 0 || o.update_flags & FULL_RIGHTS != FULL_RIGHTS {
            return;
        }
        self.new_defaults_pending -= 1;
        let texture = s
            .new_objects
            .texture
            .filter(|t| crate::world::inventory::actions::asset_uuid_shown(&world.inventory, *t));
        let Some(o) = world.objects.get_mut(idx) else { return };
        let (handle, local_id) = (o.key.region, o.key.local_id);
        if let Some(te) = o.te.as_deref() {
            let mut styled = te.clone();
            for face in styled.faces.iter_mut() {
                *face = styled_face(face, s, texture);
            }
            o.te = Some(Arc::new(styled));
            o.material_dirty = true;
            o.render.needs_records = true;
            self.send_te(world, idx);
        }
        self.send(BuildCmd::SetFlags {
            handle,
            local_id,
            physics: s.new_prim_physical,
            temporary: s.new_prim_temporary,
            phantom: s.new_prim_phantom,
        });
        self.embed_new_item(world, idx, s);
    }

    /// FSBuildPrefs_EmbedItem: the chosen item goes into the new prim, as a
    /// drop on its Contents tab (LLToolDragAndDrop::dropInventory /
    /// dropScript from applyDefaultBuildPreferences). An item no longer in
    /// the inventory, worn, or refused by the rules is left out.
    fn embed_new_item(&mut self, world: &mut World, idx: usize, s: &BuildSettings) {
        let Some(id) = s.new_objects.embed_item(world.agent_id) else {
            return;
        };
        let id = crate::world::inventory::actions::original(&world.inventory, id).unwrap_or(id);
        match super::contents_drop::plan(world, &[id], idx, false, super::contents_drop::Target::Panel) {
            Ok(drops) => self.drop_inventory(world, idx, drops),
            Err(reason) => log::info!("new prim: the item to embed is left out ({reason})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_face_takes_the_look_and_keeps_the_rest() {
        let s = BuildSettings {
            new_prim_color: [255, 0, 51, 7],
            new_prim_transparency: 25.0,
            new_prim_glow: 0.4,
            new_prim_fullbright: true,
            new_prim_shiny: 2,
            ..BuildSettings::default()
        };
        let face = TextureFace {
            scale_s: 2.0,
            // a bump of 5, no shine, no full bright
            bump_shiny_fullbright: 5,
            ..TextureFace::default()
        };
        let out = styled_face(&face, &s, None);
        assert_eq!(out.color, [1.0, 0.0, 0.2, 0.75]);
        assert_eq!(out.glow, 0.4);
        assert_eq!(out.bump_shiny_fullbright, 5 | TEM_FULLBRIGHT | (2 << TEM_SHINY_SHIFT));
        assert_eq!(out.scale_s, 2.0);
        assert_eq!(out.texture, face.texture);
    }

    #[test]
    fn a_chosen_texture_goes_on_the_face() {
        let chosen = uuid::Uuid::from_u128(7);
        let face = TextureFace {
            scale_s: 3.0,
            ..TextureFace::default()
        };
        let out = styled_face(&face, &BuildSettings::default(), Some(chosen));
        assert_eq!(out.texture, chosen);
        assert_eq!(out.scale_s, 3.0, "the mapping stays");
    }

    #[test]
    fn the_defaults_leave_a_new_prim_as_the_region_made_it() {
        let s = BuildSettings::default();
        let face = TextureFace {
            bump_shiny_fullbright: TEM_FULLBRIGHT | (3 << TEM_SHINY_SHIFT),
            ..TextureFace::default()
        };
        let out = styled_face(&face, &s, None);
        assert_eq!(out.color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(out.glow, 0.0);
        assert_eq!(out.bump_shiny_fullbright, 0);
    }
}
