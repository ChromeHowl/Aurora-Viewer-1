//! What the interface may still offer on an object: the rule the cursor,
//! the hover tips and the one-click actions ask before showing anything
//! (the send boundary of `policy.rs` refuses the command anyway).

use super::super::World;
use glam::Vec3;
use uuid::Uuid;

impl World {
    /// LLViewerWindow::cursorIntersect (indra/newview/llviewerwindow.cpp):
    /// under @interact every pick is dropped, a HUD's as much as the
    /// world's. Only our own avatar stays under the pie tool, and the
    /// camera and drag-and-drop tools keep their picks (`rlva_can_rez`).
    /// A touch that needs no pick is another rule: `rlva_can_touch_at`
    /// (RlvActions::canTouch, the Touch entry of a worn object).
    pub fn rlva_drops_picks(&self) -> bool {
        self.rlva.enabled() && self.rlva.has("interact")
    }

    /// Whether a click or the pointer may act on object `index`: the pick
    /// first (`rlva_drops_picks`; RlvActions::canInteract exempts the HUDs
    /// from @interact, but it is only asked about a pick that was kept),
    /// then RlvActions::canInteract (indra/newview/rlvactions.cpp):
    /// @fartouch leaves the HUDs alone and limits every other object to its
    /// distance, our own worn objects included (canTouch exempts them, this
    /// rule does not). `position` is the point under the cursor in the
    /// viewer's region-relative world, like `rlva_can_touch_at`; without one
    /// the object's own position is measured.
    pub fn rlva_can_interact_at(&self, index: usize, position: Option<Vec3>) -> bool {
        let r = &self.rlva;
        if !r.enabled() {
            return true;
        }
        if self.rlva_drops_picks() {
            return false;
        }
        if self.hud_root(index).is_some() {
            return true;
        }
        let max = r.distance_limit("fartouch", 1.5, Uuid::nil());
        // an object whose place is unknown is out of reach, never in reach
        !max.is_finite()
            || position
                .filter(|p| p.is_finite())
                .or_else(|| crate::scene::Scene::object_transform_unshifted(self, index, std::time::Instant::now()).map(|(p, _, _)| p))
                .is_some_and(|p| p.distance(self.agent.position) <= max)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Command;
    use super::*;

    const A: Uuid = Uuid::from_u128(1);
    const PRIM: Uuid = Uuid::from_u128(201);
    const HUD: Uuid = Uuid::from_u128(202);

    fn world() -> (World, usize, usize) {
        let mut w = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        for event in crate::demo::events() {
            w.apply(event);
        }
        let region = w.main_region.expect("demo region");
        let own = w.objects.index_of_uuid(&w.agent_id).expect("own avatar");
        let own_local = w.objects.get(own).expect("own avatar").key.local_id;
        // a prim of the world, and one worn on a HUD point (35: Center)
        for (id, local, parent, point) in [(PRIM, 32001, 0, 0u8), (HUD, 32002, own_local, 35)] {
            let mut o = crate::demo::outfit_attachment(&aurora_net::AttachRequest {
                item_id: id,
                owner_id: w.agent_id,
                point,
                add: true,
                flags: 0,
                group_mask: 0,
                everyone_mask: 0,
                next_owner_mask: 0,
                name: String::new(),
                desc: String::new(),
            });
            o.local_id = local;
            o.parent_id = parent;
            if parent == 0 {
                o.state = 0;
                o.name_values.clear();
            }
            w.objects.upsert(region, o);
        }
        let prim = w.objects.index_of_uuid(&PRIM).expect("prim");
        let hud = w.objects.index_of_uuid(&HUD).expect("hud");
        w.objects.get_mut(prim).expect("prim").position = Vec3::new(120.0, 120.0, 30.0);
        w.agent.position = Vec3::new(120.0, 123.0, 30.0);
        w.rlva.configure(true);
        (w, prim, hud)
    }

    fn restrict(w: &mut World, text: &str) {
        w.rlva.process(A, Command::parse(text).expect("command"));
    }

    #[test]
    fn rlva_interact_drops_every_pick_a_hud_included() {
        let (mut w, prim, hud) = world();
        assert!(w.hud_root(hud).is_some(), "the fixture's second prim is worn on a HUD point");
        assert!(w.rlva_can_interact_at(prim, None) && w.rlva_can_interact_at(hud, None));
        restrict(&mut w, "interact=n");
        assert!(!w.rlva_can_interact_at(prim, None));
        assert!(
            !w.rlva_can_interact_at(prim, Some(w.agent.position)),
            "a near point changes nothing"
        );
        // LLViewerWindow::cursorIntersect drops a HUD's pick like any other
        assert!(w.rlva_drops_picks());
        assert!(!w.rlva_can_interact_at(hud, None), "a HUD answers no click either");
        // its touch from the worn object's Touch entry needs no pick
        // (handle_attachment_touch asks RlvActions::canTouch alone)
        let o = w.objects.get(hud).expect("hud");
        assert!(w.rlva_can_touch(o.key.region, o.key.local_id));
        restrict(&mut w, "interact=y");
        assert!(!w.rlva_drops_picks());
        assert!(w.rlva_can_interact_at(prim, None) && w.rlva_can_interact_at(hud, None));
        // the preference off: no rule at all
        restrict(&mut w, "interact=n");
        w.rlva.configure(false);
        assert!(!w.rlva_drops_picks());
        assert!(w.rlva_can_interact_at(prim, None) && w.rlva_can_interact_at(hud, None));
    }

    #[test]
    fn rlva_fartouch_measures_the_point_under_the_cursor() {
        let (mut w, prim, hud) = world();
        // the prim's origin is 3 m away
        restrict(&mut w, "fartouch:2=n");
        assert!(!w.rlva_can_interact_at(prim, None));
        // a face of it reaching within the limit is in reach
        assert!(w.rlva_can_interact_at(prim, Some(w.agent.position + Vec3::X * 1.9)));
        assert!(!w.rlva_can_interact_at(prim, Some(w.agent.position + Vec3::X * 2.1)));
        // a point that is not a number falls back on the object's place
        assert!(!w.rlva_can_interact_at(prim, Some(Vec3::NAN)));
        assert!(w.rlva_can_interact_at(hud, Some(Vec3::splat(4000.0))), "a HUD has no distance");
        restrict(&mut w, "fartouch:2=y");
        // the bare restriction is 1.5 m
        restrict(&mut w, "fartouch=n");
        assert!(w.rlva_can_interact_at(prim, Some(w.agent.position + Vec3::X * 1.4)));
        assert!(!w.rlva_can_interact_at(prim, Some(w.agent.position + Vec3::X * 1.6)));
        restrict(&mut w, "fartouch=y");
        assert!(w.rlva_can_interact_at(prim, None));
    }
}
