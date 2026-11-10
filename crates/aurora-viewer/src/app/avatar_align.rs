//! Avatar alignment and Face nearest avatar (`ui/avatar_align.rs`): the
//! agent's frame turned to a bearing, as FSAvatarAlignBase::applyRotation
//! (indra/newview/fsfloateravataralign.cpp, originally LGPL 2.1) resets the
//! agent's axes, sends them, and snaps the drawn body so that it does not
//! play its turn. The rotation goes out with the next AgentUpdate, like
//! RLVa's @setrot (world/rlva/policy.rs).
use super::App;
use crate::ui::avatar_align::{self, Action};

impl App {
    /// Our compass bearing, None before our avatar is in the world.
    fn own_bearing(&self) -> Option<f32> {
        let me = self.world.agent_id;
        self.world.objects.index_of_uuid(&me)?;
        Some(avatar_align::bearing(self.world.agent.yaw))
    }

    fn align_blocked(&self, action: Action) -> Option<avatar_align::Blocked> {
        avatar_align::blocked(action, self.world.agent.is_sitting(), self.world.rlva.has("shownearby"))
    }

    pub(super) fn avatar_align_window(&mut self, ctx: &egui::Context, p: &crate::theme::Palette) {
        if !self.panels.avatar_align {
            return;
        }
        let facing = self.own_bearing();
        let blocked = self.align_blocked(Action::Bearing(0.0));
        if let Some(action) = self.avatar_align_ui.show(ctx, p, facing, blocked, &mut self.panels.avatar_align) {
            self.avatar_align(action);
        }
    }

    /// Run one alignment: refused seated, Face nearest refused under
    /// @shownearby, with a line in the nearby chat saying why.
    pub(super) fn avatar_align(&mut self, action: Action) {
        if let Some(blocked) = self.align_blocked(action) {
            self.world.system_message(blocked.message());
            return;
        }
        let Some(current) = self.own_bearing() else { return };
        let bearing = match action {
            Action::Bearing(b) => b,
            Action::Rotate(delta) => avatar_align::rotated(current, delta),
            Action::NearestCardinal => avatar_align::nearest_cardinal(current),
            Action::FaceNearest => {
                let me = self.world.agent.position;
                let others = self
                    .world
                    .objects
                    .avatars()
                    .map(|(_, o)| o.full_id)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .filter_map(|id| Some((id, crate::autopilot::avatar_position(&self.world, id)?)));
                let target = avatar_align::nearest(me, others).and_then(|(_, at)| avatar_align::bearing_to(me, at));
                match target {
                    Some(b) => b,
                    None => {
                        self.world.system_message(aurora_i18n::tr!("b4-align-nobody"));
                        return;
                    }
                }
            }
        };
        let yaw = avatar_align::yaw(bearing);
        self.world.agent.yaw = yaw;
        // snapAvatarBody: the drawn body faces there at once
        let me = self.world.agent_id;
        if let Some(body) = self.world.bodies.get_mut(&me) {
            body.rotation = glam::Quat::from_rotation_z(yaw);
        }
        log::info!("avatar alignment: {action:?}, facing {bearing:.0}°");
    }
}
