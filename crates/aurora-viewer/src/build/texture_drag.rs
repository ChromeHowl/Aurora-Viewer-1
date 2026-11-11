//! Dragging a face's texture with the mouse in the Select Face mode: a
//! press on the face already selected grabs its texture, moving the mouse
//! slides its offset (with Caps Lock held, its repeats), Ctrl and Shift
//! make the steps finer, and the release sends the new texture entry once
//! (Preferences › Build, off by default).
//!
//! Behaviour of LLToolFace::pickCallback, render and handleMouseUp
//! (indra/newview/lltoolface.cpp), setting FSExperimentalDragTexture,
//! originally LGPL 2.1. Only on a prim we may modify, as there
//! (permModify), and never on one RLVa keeps us from editing.
//!
//! Difference in the way only: the Legacy Viewer hides the pointer and puts
//! it back at the press point each frame, adding the steps; Aurora reads the
//! whole move since the press from the values at the press, which gives the
//! same result without moving the user's pointer.

use super::{BuildTool, Mods};
use crate::world::World;
use crate::world::objects::ObjKey;
use std::sync::Arc;

/// The texture held by the mouse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureDrag {
    pub object: ObjKey,
    pub face: u8,
    /// Pointer at the press (pixels).
    pub start: (f32, f32),
    /// Offset and repeats of the face at the press.
    pub offset: (f32, f32),
    pub scale: (f32, f32),
    /// The face changed since the press: its entry is sent at the release.
    pub moved: bool,
}

/// What the move does to the face: its new offset, or its new repeats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Change {
    Offset(f32, f32),
    Scale(f32, f32),
}

/// LLToolFace::render: the change for a move of `dx` pixels right and `dy`
/// pixels up since the press, Caps Lock (`repeats`) choosing the repeats.
/// Offset: 1/100 of a repeat per pixel (1/1000 with Ctrl, 1/10000 with
/// Shift), kept within -1..1 by whole turns. Repeats: 1/10 per pixel
/// (1/100 with Ctrl, 1/1000 with Shift), never below 0.
pub fn change(d: &TextureDrag, dx: f32, dy: f32, mods: Mods, repeats: bool) -> Change {
    let fine = |usual: f32| {
        if mods.ctrl {
            usual * 10.0
        } else if mods.shift {
            usual * 100.0
        } else {
            usual
        }
    };
    if repeats {
        let step = fine(10.0);
        Change::Scale((d.scale.0 - dx / step).max(0.0), (d.scale.1 + dy / step).max(0.0))
    } else {
        let step = fine(100.0);
        let wrap = |v: f32| if v.abs() > 1.0 { v.fract() } else { v };
        Change::Offset(wrap(d.offset.0 - dx / step * d.scale.0), wrap(d.offset.1 + dy / step * d.scale.1))
    }
}

impl BuildTool {
    /// LLToolFace::pickCallback: a press on the face selected alone just
    /// before grabs its texture, when the option is on and the prim may be
    /// modified. Called before the click changes the selection.
    pub(super) fn texture_grab(&mut self, world: &World, idx: usize, face: u8, cursor: (f32, f32), enabled: bool) {
        self.texture_drag = None;
        if !enabled || !self.can_modify(world, idx) || !world.rlva_can_edit(idx) {
            return;
        }
        let Some(o) = world.objects.get(idx) else { return };
        let alone = self.selection == [o.key] && self.faces.get(&o.key).is_some_and(|f| f == &[face]);
        let Some(tf) = o.te.as_deref().and_then(|te| te.faces.get(face as usize)) else {
            return;
        };
        if alone {
            self.texture_drag = Some(TextureDrag {
                object: o.key,
                face,
                start: cursor,
                offset: (tf.offset_s, tf.offset_t),
                scale: (tf.scale_s, tf.scale_t),
                moved: false,
            });
        }
    }

    /// The grabbed texture follows the pointer (local only until the release).
    pub(super) fn texture_drag_update(&mut self, world: &mut World, cursor: (f32, f32), mods: Mods) {
        let Some(d) = self.texture_drag.as_mut() else { return };
        let (dx, dy) = (cursor.0 - d.start.0, d.start.1 - cursor.1);
        if dx == 0.0 && dy == 0.0 && !d.moved {
            return;
        }
        let Some(idx) = world.objects.index_of(&d.object) else {
            self.texture_drag = None;
            return;
        };
        let Some(o) = world.objects.get_mut(idx) else { return };
        let Some(mut te) = o.te.as_deref().cloned() else { return };
        let Some(tf) = te.faces.get_mut(d.face as usize) else { return };
        let before = *tf;
        match change(d, dx, dy, mods, self.caps_lock_held) {
            Change::Offset(u, v) => (tf.offset_s, tf.offset_t) = (u, v),
            Change::Scale(u, v) => (tf.scale_s, tf.scale_t) = (u, v),
        }
        if *tf != before {
            o.te = Some(Arc::new(te));
            o.material_dirty = true;
            o.render.needs_records = true;
            d.moved = true;
        }
    }

    /// LLToolFace::handleMouseUp: the texture is let go and its entry sent.
    pub(super) fn texture_release(&mut self, world: &World) {
        let Some(d) = self.texture_drag.take() else { return };
        if d.moved
            && let Some(idx) = world.objects.index_of(&d.object)
        {
            self.send_te(world, idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held() -> TextureDrag {
        TextureDrag {
            object: ObjKey {
                region: Default::default(),
                local_id: 1,
            },
            face: 0,
            start: (0.0, 0.0),
            offset: (0.0, 0.0),
            scale: (2.0, 1.0),
            moved: false,
        }
    }

    const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
    };

    #[test]
    fn a_move_slides_the_offset_by_the_repeats() {
        // 10 px right, 20 px up
        assert_eq!(change(&held(), 10.0, 20.0, NONE, false), Change::Offset(-0.2, 0.2));
        let ctrl = Mods { ctrl: true, ..NONE };
        assert_eq!(change(&held(), 10.0, 0.0, ctrl, false), Change::Offset(-0.02, 0.0));
        let shift = Mods { shift: true, ..NONE };
        assert_eq!(change(&held(), 100.0, 0.0, shift, false), Change::Offset(-0.02, 0.0));
    }

    #[test]
    fn the_offset_turns_over_past_one() {
        let Change::Offset(u, v) = change(&held(), -75.0, 130.0, NONE, false) else {
            panic!("offset expected");
        };
        assert!((u - 0.5).abs() < 1e-5, "1.5 turns over to 0.5, got {u}");
        assert!((v - 0.3).abs() < 1e-5, "1.3 turns over to 0.3, got {v}");
    }

    #[test]
    fn caps_lock_changes_the_repeats_never_below_zero() {
        assert_eq!(change(&held(), 5.0, 10.0, NONE, true), Change::Scale(1.5, 2.0));
        assert_eq!(change(&held(), 500.0, -500.0, NONE, true), Change::Scale(0.0, 0.0));
        let ctrl = Mods { ctrl: true, ..NONE };
        assert_eq!(change(&held(), 25.0, 0.0, ctrl, true), Change::Scale(1.75, 1.0));
    }
}
