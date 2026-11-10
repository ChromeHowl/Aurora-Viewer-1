//! Poser › Animations: inventory animations played on the posed subject,
//! with play, pause, stop and a position slider, locally like the rest of
//! the poser (nothing is started on the simulator).
//!
//! The animations are the IK editor's stack (`ik_edit::animations`, the same
//! inventory picker, drop target, order and priorities); here their frames
//! follow one playback clock at the 30 Hz authoring grid, each clip looping
//! on its own length, and a clip without length (a static pose) holds its
//! single frame.
//!
//! The animation drives the joints it keys and leaves the others as posed.
//! With "Manual edits override animation" on, a joint edited in the poser
//! keeps the edit and the animation moves the rest; off, the animation wins
//! on its joints. "Apply" bakes the frame shown into the pose, with an undo
//! step, as loading an animation file does (`Editor::tick_pose_storage`).
use super::*;
use crate::scene::ik::animations::{FPS, Sample, last_frame};
use std::time::Instant;

/// Where the playback stands.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) enum Transport {
    /// No animation on the pose.
    #[default]
    Stopped,
    /// Playing since `since`, from `from` seconds.
    Playing { since: Instant, from: f32 },
    /// Held at this position (seconds).
    Paused(f32),
}

impl Transport {
    /// The position (seconds) shown, None when stopped.
    pub fn position(self, now: Instant) -> Option<f32> {
        match self {
            Transport::Stopped => None,
            Transport::Playing { since, from } => Some(from + now.saturating_duration_since(since).as_secs_f32()),
            Transport::Paused(at) => Some(at),
        }
    }
    pub fn play(self, now: Instant) -> Self {
        match self {
            Transport::Playing { .. } => self,
            other => Transport::Playing {
                since: now,
                from: other.position(now).unwrap_or(0.0),
            },
        }
    }
    pub fn pause(self, now: Instant) -> Self {
        Transport::Paused(self.position(now).unwrap_or(0.0))
    }
    /// The slider moved: the same state at another position.
    pub fn seek(self, at: f32, now: Instant) -> Self {
        let at = if at.is_finite() { at.max(0.0) } else { 0.0 };
        match self {
            Transport::Playing { .. } => Transport::Playing { since: now, from: at },
            _ => Transport::Paused(at),
        }
    }
}

/// The frame of a clip whose last frame is `last` at `t` seconds, looping.
pub(super) fn frame_at(t: f32, last: u32) -> u32 {
    if !t.is_finite() || t <= 0.0 || last == 0 {
        return 0;
    }
    ((t * FPS) as u64 % (u64::from(last) + 1)) as u32
}

/// The longest clip (seconds), for the position slider.
pub(super) fn length(samples: &[Sample]) -> f32 {
    samples
        .iter()
        .map(|s| last_frame(s.animation.anim.duration) as f32 / FPS)
        .fold(0.0, f32::max)
}

/// `pose` with the animated joints of `samples` (their frames already set),
/// the edited joints kept when `override_edits` (`edited(bone)`).
pub(super) fn compose(
    rig: &Rig,
    pose: &SubjectPose,
    base: &SubjectPose,
    samples: &[Sample],
    override_edits: bool,
    edited: impl Fn(usize) -> bool,
) -> SubjectPose {
    let mut local = ik::LocalPose {
        rotations: pose.rotations.clone(),
        positions: pose.positions.clone(),
    };
    ik::animations::apply(rig, &mut local, Vec3::ZERO, samples);
    let mut out = pose.clone();
    for bone in 0..rig.len().min(out.rotations.len()) {
        if override_edits && edited(bone) {
            continue;
        }
        if out.rotations[bone].dot(local.rotations[bone]).abs() < 0.9999999 {
            // a joint the animation drives is the animation's, not the capture's
            out.origin[bone] = rig.local_rot[bone];
            out.rotations[bone] = local.rotations[bone];
            out.locks.remove(&bone);
        }
        out.positions[bone] = local.positions[bone];
    }
    out.keep_offsets(rig, base);
    out
}

impl Editor {
    /// The stack's samples at the playback position, None when stopped or
    /// nothing is loaded.
    fn playback_samples(&self, now: Instant) -> Option<Vec<Sample>> {
        let at = self.poser.playback.position(now)?;
        let mut samples = self.animations.samples();
        if samples.is_empty() {
            return None;
        }
        for sample in &mut samples {
            sample.frame = frame_at(at, last_frame(sample.animation.anim.duration));
        }
        Some(samples)
    }

    /// The subject's pose with the playing animation, for `pose_layers`.
    pub(super) fn played_pose(&self, rig: &Rig, subject: Uuid, now: Instant) -> Option<Arc<SubjectPose>> {
        if subject != self.poser.subject {
            return None;
        }
        let samples = self.playback_samples(now)?;
        let session = self.poser.sessions.get(&subject)?;
        let pose = compose(rig, &session.pose, &session.base, &samples, self.poser.override_edits, |b| {
            session.edited(b)
        });
        pose.valid(rig).then(|| Arc::new(pose))
    }

    /// "Apply": the frame shown becomes the pose, one undo step.
    fn bake_playback(&mut self, rig: &Rig) {
        let Some(samples) = self.playback_samples(Instant::now()) else {
            return;
        };
        let override_edits = self.poser.override_edits;
        let Some(session) = self.poser.sessions.get_mut(&self.poser.subject) else {
            return;
        };
        let edited: Vec<bool> = (0..rig.len()).map(|b| session.edited(b)).collect();
        let pose = compose(rig, &session.pose, &session.base, &samples, override_edits, |b| {
            edited.get(b).copied().unwrap_or(false)
        });
        if pose.valid(rig) {
            session.checkpoint();
            session.pose = Arc::new(pose);
            self.poser.playback = Transport::Stopped;
            self.refresh = true;
        }
    }

    /// The Animations panel, over the joint controls.
    pub(super) fn playback_ui(&mut self, ui: &mut egui::Ui, p: &Palette, world: &World) {
        let rig = &world.avatar_lib.rig;
        let now = Instant::now();
        ui.heading(aurora_i18n::tr!("b4-poser-animations"));
        let samples = self.animations.samples();
        let length = length(&samples);
        let playing = matches!(self.poser.playback, Transport::Playing { .. });
        ui.horizontal(|ui| {
            let ready = !samples.is_empty();
            let (icon, tip) = if playing {
                ("player-pause-filled", aurora_i18n::tr!("b4-poser-pause"))
            } else {
                ("player-play-filled", aurora_i18n::tr!("b4-poser-play"))
            };
            if widgets::icon_button(ui, p, icon, tip, ready) {
                self.poser.playback = if playing {
                    self.poser.playback.pause(now)
                } else {
                    self.poser.playback.play(now)
                };
            }
            let stopped = self.poser.playback == Transport::Stopped;
            if widgets::icon_button(ui, p, "player-stop-filled", aurora_i18n::tr!("b4-poser-stop"), !stopped) {
                self.poser.playback = Transport::Stopped;
            }
            if ui
                .add_enabled(ready && !stopped, egui::Button::new(aurora_i18n::tr!("b4-poser-apply")))
                .on_hover_text(aurora_i18n::tr!("b4-poser-apply-tip"))
                .clicked()
            {
                self.bake_playback(rig);
            }
        });
        let mut at = self.poser.playback.position(now).unwrap_or(0.0);
        let shown = if length > 0.0 { at % (length + 1.0 / FPS) } else { 0.0 };
        let mut slider = shown;
        let changed = ui
            .add_enabled(
                length > 0.0,
                egui::Slider::new(&mut slider, 0.0..=length.max(0.0)).suffix(" s").max_decimals(2),
            )
            .changed();
        if changed {
            at = slider;
            self.poser.playback = self.poser.playback.seek(at, now);
        }
        widgets::switch_labeled(ui, p, &mut self.poser.override_edits, aurora_i18n::tr!("b4-poser-override"))
            .on_hover_text(aurora_i18n::tr!("b4-poser-override-tip"));
        ui.add_space(6.0);
        self.animations.show_rows(ui, p, &world.inventory);
        if playing {
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::avatar::AvatarLibrary;
    use aurora_assets::{Animation, JointMotion, anim::RotKey};
    use std::time::Duration;

    #[test]
    fn the_transport_plays_pauses_seeks_and_stops() {
        let t0 = Instant::now();
        let at = |s: f32| t0 + Duration::from_secs_f32(s);
        let stopped = Transport::default();
        assert_eq!(stopped.position(t0), None);
        let playing = stopped.play(t0);
        assert!((playing.position(at(1.5)).expect("playing") - 1.5).abs() < 1e-3);
        let paused = playing.pause(at(2.0));
        assert_eq!(paused, Transport::Paused(2.0));
        assert_eq!(paused.position(at(9.0)), Some(2.0));
        let resumed = paused.play(at(5.0));
        assert!((resumed.position(at(6.0)).expect("resumed") - 3.0).abs() < 1e-3);
        assert_eq!(paused.seek(0.5, at(9.0)), Transport::Paused(0.5));
        assert!((resumed.seek(4.0, at(6.0)).position(at(7.0)).expect("seek") - 5.0).abs() < 1e-3);
        assert_eq!(stopped.seek(f32::NAN, t0), Transport::Paused(0.0));
    }

    #[test]
    fn clips_loop_on_their_own_length_and_static_poses_hold() {
        assert_eq!(frame_at(0.0, 30), 0);
        assert_eq!(frame_at(0.5, 30), 15);
        // frame 30 is the last; one frame later the loop starts again
        assert_eq!(frame_at(31.0 / 30.0 + 0.001, 30), 0);
        assert_eq!(frame_at(5.0, 0), 0);
        assert_eq!(frame_at(f32::NAN, 30), 0);
        assert_eq!(frame_at(-1.0, 30), 0);
    }

    fn head_clip(rig: &Rig, angle: f32) -> Sample {
        let key = |time: f32| RotKey {
            time,
            rotation: Quat::from_rotation_z(angle),
        };
        let animation = Animation {
            base_priority: 4,
            duration: 1.0,
            emote_name: String::new(),
            loop_in: 0.0,
            loop_out: 1.0,
            looping: true,
            ends_after: None,
            ease_in: 0.0,
            ease_out: 0.0,
            hand_pose: 0,
            joints: vec![JointMotion {
                joint_name: "mHead".into(),
                priority: -1,
                rot_keys: vec![key(0.0), key(1.0)],
                pos_keys: Vec::new(),
            }],
        };
        Sample {
            animation: Arc::new(crate::scene::anim::BoundAnim::bind(animation, rig)),
            frame: 0,
        }
    }

    #[test]
    fn the_animation_drives_its_joints_and_edits_can_override_it() {
        let lib = AvatarLibrary::load();
        let rig = &lib.rig;
        let base = SubjectPose {
            rotations: rig.local_rot.clone(),
            positions: rig.local_pos.clone(),
            scales: rig.scale.clone(),
            locks: Default::default(),
            origin: rig.local_rot.clone(),
        };
        let (head, neck) = (rig.names["mHead"], rig.names["mNeck"]);
        let mut pose = base.clone();
        pose.rotations[head] = Quat::from_rotation_y(0.4);
        pose.rotations[neck] = Quat::from_rotation_x(0.2);
        let edited = |b: usize| b == head || b == neck;
        let samples = [head_clip(rig, 0.7)];
        // the animation wins on the head; the neck it does not key stays posed
        let played = compose(rig, &pose, &base, &samples, false, edited);
        assert!(played.rotations[head].angle_between(Quat::from_rotation_z(0.7)) < 1e-3);
        assert_eq!(played.rotations[neck], pose.rotations[neck]);
        // manual edits override: the edited head keeps its edit
        let kept = compose(rig, &pose, &base, &samples, true, edited);
        assert_eq!(kept.rotations[head], pose.rotations[head]);
        // an unedited head follows the animation either way
        let free = compose(rig, &base, &base, &samples, true, |_| false);
        assert!(free.rotations[head].angle_between(Quat::from_rotation_z(0.7)) < 1e-3);
        assert!((length(&samples) - 1.0).abs() < 1e-3);
        assert!(played.valid(rig));
    }
}
