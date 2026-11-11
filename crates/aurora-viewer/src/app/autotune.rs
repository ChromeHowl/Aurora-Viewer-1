//! The auto-tune to a target frame rate (settings/render_photo.rs): each
//! frame in the world, the avatars over the render time limit become
//! impostors, and the tuner lowers or raises the limit and the draw
//! distance (LLPerfStats::StatsRecorder::updateAvatarParams,
//! LLVOAvatar::updateTooSlow, llperfstats.cpp and llvoavatar.cpp,
//! originally LGPL 2.1).
use super::*;
use crate::settings::render_photo::{TuneAction, TuneInput, Tuner, too_slow};
use std::collections::HashMap;

/// The auto-tune's state of the session.
#[derive(Default)]
pub(super) struct AutoTuneState {
    tuner: Tuner,
    /// Each avatar's estimated render time (µs) and whether it is always
    /// drawn in full (Always render, or a friend with the friends rule),
    /// refreshed once a second.
    avatars: HashMap<usize, (f32, bool)>,
    measured: Option<Instant>,
    was_on: bool,
    /// Turned off by reaching its target: the limit stays.
    kept: bool,
}

impl App {
    pub(super) fn autotune_frame(&mut self) {
        let cfg = self.settings.render_photo.autotune;
        let state = &mut self.autotune;
        if !cfg.enabled {
            // turned off by the user: the limit goes with it (the target
            // reached keeps it, as the Legacy Viewer does)
            if state.was_on && !state.kept {
                state.tuner.max_art_us = 0.0;
            }
            state.was_on = false;
            if state.tuner.max_art_us <= 0.0 {
                self.scene.too_slow.clear();
                return;
            }
        } else {
            state.was_on = true;
            state.kept = false;
        }
        let now = Instant::now();
        if state.measured.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(1)) {
            state.measured = Some(now);
            let gpu_ms = self.last_render.gpu_ms.unwrap_or(self.perf.frames.avg);
            let frame_triangles = self.last_render.triangles;
            let friends_rule = self.settings.complexity_mode >= 1;
            let friends: std::collections::HashSet<uuid::Uuid> = self.world.social.shown_friends().map(|f| f.id).collect();
            state.avatars = self
                .scene
                .avatar_triangles()
                .into_iter()
                .filter_map(|(idx, tris)| {
                    let o = self.world.objects.get(idx)?;
                    if o.full_id == self.world.agent_id {
                        return None;
                    }
                    let always = self.settings.render_exception(&o.full_id) == 2 || (friends_rule && friends.contains(&o.full_id));
                    Some((idx, (crate::scene::render_time::estimate_us(gpu_ms, frame_triangles, tris), always)))
                })
                .collect();
            // AutoTuneImpostorByDistEnabled: the fully drawn avatars are
            // those within the distance (RenderAvatarMaxNonImpostors)
            if cfg.enabled && cfg.impostor_by_distance {
                let reach = cfg.impostor_distance.min(self.settings.draw_distance);
                let eye = self.camera.position;
                let near = self
                    .world
                    .objects
                    .avatars()
                    .filter(|(_, o)| o.full_id != self.world.agent_id)
                    .filter(|(idx, _)| {
                        crate::scene::Scene::object_transform_unshifted(&self.world, *idx, now)
                            .is_some_and(|(p, _, _)| p.distance(eye) <= reach)
                    })
                    .count() as u32;
                // (with ours; past the slider, no limit)
                self.settings.max_avatars = (near + 1).clamp(1, 100);
            }
        }
        let limit = state.tuner.max_art_us;
        let excluded = |idx: &usize| self.settings.render_photo.impostor_exclude_animesh && self.scene.animesh_wearers.contains(idx);
        self.scene.too_slow = state
            .avatars
            .iter()
            .filter(|(idx, (us, always))| too_slow(limit, *us, false, *always, excluded(idx)))
            .map(|(idx, _)| *idx)
            .collect();
        if !cfg.enabled {
            return;
        }
        let input = TuneInput {
            frame_ms: self.perf.frames.avg,
            focused: self.focused,
            avatars_us: state.avatars.values().map(|(us, _)| us).sum(),
            max_avatar_us: state.avatars.values().map(|(us, _)| *us).fold(0.0, f32::max),
            tuned_avatars: self.scene.too_slow.len(),
            draw_distance: self.settings.draw_distance,
        };
        match state.tuner.step(&cfg, &input) {
            None => {}
            Some(TuneAction::DrawDistance(dd)) => {
                log::info!("auto-tune: draw distance {dd} m");
                self.settings.draw_distance = dd;
            }
            Some(TuneAction::AvatarLimit(us)) => log::debug!("auto-tune: avatar render time limit {us:.0} µs"),
            Some(TuneAction::Done) => {
                log::info!("auto-tune: target frame rate reached, tuning off");
                state.kept = true;
                self.settings.render_photo.autotune.enabled = false;
            }
        }
    }
}
