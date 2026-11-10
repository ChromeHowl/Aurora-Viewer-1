//! The pose stand (`ui/pose_stand.rs`, `ik_edit::poser::stand`): while its
//! window is open, our avatar holds the chosen pose through the poser's
//! local layer (`Scene::poser`), seen by this viewer only.
//!
//! The Legacy Viewer (FSFloaterPoseStand::onOpen / onClose,
//! indra/newview/fsfloaterposestand.cpp, originally LGPL 2.1) pauses the AO,
//! stops our animations and plays the pose on the simulator, and may sit us
//! down to lock the avatar in place (FSPoseStandLock). Being local, this one
//! touches none of that: the pose replaces what is drawn, nothing is sent,
//! and closing the window gives the animations back at once.
//!
//! The poser and IK Edit come first: while their window is open, or while
//! the poser holds our avatar, the stand waits.
use super::App;
use std::sync::Arc;

/// Whether the stand shows its pose on our avatar this frame.
pub fn holds(open: bool, editor_open: bool, posed_by_poser: bool) -> bool {
    open && !editor_open && !posed_by_poser
}

impl App {
    pub(super) fn pose_stand_window(&mut self, ctx: &egui::Context, p: &crate::theme::Palette) {
        if !self.panels.pose_stand {
            return;
        }
        let held = self.pose_stand_held;
        let mut pose = self.settings.avatar_motion.pose_stand;
        if self.pose_stand_ui.show(ctx, p, &mut pose, held, &mut self.panels.pose_stand) {
            self.settings.avatar_motion.pose_stand = pose;
            self.settings.save();
        }
    }

    /// After the poser's layers: add ours for our avatar. The shaped
    /// skeleton is captured once when the stand starts holding, and again
    /// when our avatar or its rig changed.
    pub(super) fn apply_pose_stand(&mut self) {
        let me = self.world.agent_id;
        let hold = holds(self.panels.pose_stand, self.ik_editor.open, self.scene.poser.contains_key(&me));
        self.pose_stand_held = hold;
        if !hold {
            self.pose_stand_capture = None;
            return;
        }
        let rig = &self.world.avatar_lib.rig;
        let stale = self
            .pose_stand_capture
            .as_ref()
            .is_none_or(|(id, pose)| *id != me || !pose.valid(rig));
        if stale {
            self.pose_stand_capture = crate::ik_edit::Frame::new(&self.world)
                .and_then(|frame| crate::ik_edit::poser::SubjectPose::capture(rig, &frame))
                .map(|pose| (me, pose));
        }
        if let Some((_, captured)) = &self.pose_stand_capture {
            let pose = self.settings.avatar_motion.pose_stand.pose(rig, captured);
            if pose.valid(rig) {
                self.scene.poser.insert(me, Arc::new(pose));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::holds;

    #[test]
    fn the_stand_holds_only_when_nothing_else_poses_us() {
        assert!(holds(true, false, false));
        assert!(!holds(false, false, false));
        assert!(!holds(true, true, false), "the poser's or IK Edit's window is open");
        assert!(!holds(true, false, true), "the poser holds our avatar");
    }
}
