//! Native rotation keys and pelvis translations. Capture uses LL's rigid
//! parent rotation rule, never inherited matrix scale. Export trims/rebases
//! tracks and samples both boundaries before the standard .anim codec.
use super::*;
use aurora_assets::anim::{JointMotion, PosKey, RotKey};

pub const FPS: f32 = 30.0;
pub const LAST: u32 = 1800;

#[derive(Clone)]
pub struct Track {
    pub bone: usize,
    pub enabled: bool,
    pub motion: JointMotion,
    base_position: Vec3,
}
#[derive(Clone)]
pub struct Document {
    pub name: String,
    pub meta: Animation,
    pub start: u32,
    pub end: u32,
    pub tracks: Vec<Track>,
    /// How the keys follow each other (`interpolation.rs`); linear, the
    /// native playback, unless chosen.
    pub interpolation: super::interpolation::Interpolation,
}
impl Document {
    pub fn new() -> Self {
        Self {
            name: aurora_i18n::tr!("anim-edit-new-name").to_string(),
            start: 0,
            end: 60,
            tracks: Vec::new(),
            interpolation: Default::default(),
            meta: Animation {
                base_priority: 3,
                duration: 2.0,
                emote_name: String::new(),
                loop_in: 0.0,
                loop_out: 2.0,
                looping: true,
                ends_after: None,
                ease_in: 0.3,
                ease_out: 0.3,
                hand_pose: 0,
                joints: Vec::new(),
            },
        }
    }
    /// A native animation opened in the timeline: its joints the rig knows
    /// become tracks with their keys, its timing and playback settings the
    /// document's, from frame 0 to its duration.
    pub fn from_animation(rig: &Rig, animation: &Animation, name: String) -> Result<Self, &'static str> {
        let mut document = Self::new();
        document.name = name;
        let end = if animation.duration.is_finite() {
            ((animation.duration * FPS).ceil() as u32).clamp(1, LAST)
        } else {
            return Err("anim-edit-invalid");
        };
        document.meta = Animation {
            joints: Vec::new(),
            base_priority: animation.base_priority.clamp(0, 6),
            hand_pose: animation.hand_pose.min(13),
            ..animation.clone()
        };
        for joint in &animation.joints {
            let Some(&bone) = rig.names.get(&joint.joint_name) else { continue };
            if document.tracks.iter().any(|t| t.bone == bone) || (joint.rot_keys.is_empty() && joint.pos_keys.is_empty()) {
                continue;
            }
            let base_position = joint
                .pos_keys
                .first()
                .map_or_else(|| if bone == rig.pelvis { Vec3::ZERO } else { rig.local_pos[bone] }, |k| k.position);
            document.tracks.push(Track {
                bone,
                enabled: true,
                base_position,
                motion: JointMotion {
                    joint_name: rig.canonical_names.get(bone).cloned().unwrap_or_else(|| joint.joint_name.clone()),
                    priority: joint.priority.clamp(-1, 6),
                    rot_keys: joint.rot_keys.clone(),
                    pos_keys: joint.pos_keys.clone(),
                },
            });
        }
        if document.tracks.is_empty() {
            return Err("anim-edit-no-bones");
        }
        document.start = 0;
        document.end = end;
        document.meta.duration = end as f32 / FPS;
        document.timing();
        Ok(document)
    }
    pub fn range(&mut self, start: u32, end: u32) {
        let old = self.meta.duration;
        self.start = start.min(LAST - 1);
        self.end = end.clamp(self.start + 1, LAST);
        self.meta.duration = (self.end - self.start) as f32 / FPS;
        if (self.meta.loop_out - old).abs() < 0.0001 {
            self.meta.loop_out = self.meta.duration;
        }
        self.timing();
    }
    pub fn timing(&mut self) {
        let d = self.meta.duration;
        self.meta.loop_in = self.meta.loop_in.clamp(0.0, (d - 1.0 / FPS).max(0.0));
        self.meta.loop_out = self.meta.loop_out.clamp(self.meta.loop_in + 1.0 / FPS, d);
        self.meta.ease_in = self.meta.ease_in.clamp(0.0, d);
        self.meta.ease_out = self.meta.ease_out.clamp(0.0, d);
        if !self.meta.looping && self.meta.ease_in + self.meta.ease_out > d {
            let factor = d / (self.meta.ease_in + self.meta.ease_out);
            self.meta.ease_in *= factor;
            self.meta.ease_out *= factor;
        }
    }
    pub fn record(&mut self, rig: &Rig, frame: &Frame, at: u32, bones: &[usize], translations: &[usize]) -> Result<(), &'static str> {
        if at > LAST {
            return Err("anim-edit-invalid");
        }
        let inverse = frame.model.inverse();
        let matrices: Vec<_> = frame.joints.iter().map(|m| inverse * *m).collect();
        let mut captured = Vec::new();
        for &bone in bones {
            let Some(name) = rig.canonical_names.get(bone).filter(|n| n.starts_with('m')) else {
                continue;
            };
            let Some(matrix) = matrices.get(bone) else {
                return Err("anim-edit-invalid");
            };
            let (_, q, p) = matrix.to_scale_rotation_translation();
            let parent = rig.parent[bone]
                .and_then(|j| matrices.get(j))
                .map_or(Quat::IDENTITY, |m| m.to_scale_rotation_translation().1);
            let q = (parent.inverse() * q).normalize();
            let local = if bone == rig.pelvis {
                p - rig.local_pos[bone]
            } else if let Some(j) = rig.parent[bone] {
                let (scale, _, parent_p) = matrices[j].to_scale_rotation_translation();
                if scale.abs().min_element() < 0.00001 {
                    return Err("anim-edit-invalid");
                }
                parent.inverse() * (p - parent_p) / scale
            } else {
                p
            };
            let previous = self.tracks.iter().find(|t| t.bone == bone);
            let position = bone == rig.pelvis
                || previous.is_some_and(|t| !t.motion.pos_keys.is_empty())
                || (translations.contains(&bone) && previous.is_none_or(|t| t.base_position.distance_squared(local) > 0.0001 * 0.0001));
            if !q.is_finite() || !local.is_finite() || (position && local.abs().max_element() > 5.0) {
                return Err("anim-edit-pelvis-bound");
            }
            captured.push((bone, name.clone(), q, local, position));
        }
        if captured.is_empty() {
            return Err("anim-edit-no-bones");
        }
        let time = at as f32 / FPS;
        for (bone, name, q, local, position) in captured {
            if !self.tracks.iter().any(|t| t.bone == bone) {
                self.tracks.push(Track {
                    bone,
                    enabled: true,
                    base_position: local,
                    motion: JointMotion {
                        joint_name: name,
                        priority: -1,
                        rot_keys: Vec::new(),
                        pos_keys: Vec::new(),
                    },
                });
            }
            let Some(track) = self.tracks.iter_mut().find(|t| t.bone == bone) else {
                continue;
            };
            if let Some(key) = track.motion.rot_keys.iter_mut().find(|k| (k.time - time).abs() < 0.001 / FPS) {
                key.rotation = q;
            } else {
                track.motion.rot_keys.push(RotKey { time, rotation: q });
                track.motion.rot_keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            }
            if position {
                let position = local;
                if track.motion.pos_keys.is_empty() && at > 0 {
                    track.motion.pos_keys.push(PosKey {
                        time: 0.0,
                        position: track.base_position,
                    });
                }
                if let Some(key) = track.motion.pos_keys.iter_mut().find(|k| (k.time - time).abs() < 0.001 / FPS) {
                    key.position = position;
                } else {
                    track.motion.pos_keys.push(PosKey { time, position });
                    track.motion.pos_keys.sort_by(|a, b| a.time.total_cmp(&b.time));
                }
            }
        }
        if at < self.start || at > self.end {
            self.range(self.start.min(at), self.end.max(at));
        }
        Ok(())
    }
    pub fn remove(&mut self, bone: usize, at: u32) {
        let time = at as f32 / FPS;
        if let Some(track) = self.tracks.iter_mut().find(|t| t.bone == bone) {
            track.motion.rot_keys.retain(|k| (k.time - time).abs() >= 0.001 / FPS);
            track.motion.pos_keys.retain(|k| (k.time - time).abs() >= 0.001 / FPS);
        }
    }
    pub fn frames(&self, bone: Option<usize>) -> Vec<u32> {
        let mut frames: Vec<_> = self
            .tracks
            .iter()
            .filter(|t| bone.is_none_or(|b| b == t.bone))
            .flat_map(|t| {
                t.motion
                    .rot_keys
                    .iter()
                    .map(|k| k.time)
                    .chain(t.motion.pos_keys.iter().map(|k| k.time))
            })
            .map(|t| (t * FPS).round() as u32)
            .collect();
        frames.sort();
        frames.dedup();
        frames
    }
    pub fn animation(&self) -> Result<Animation, &'static str> {
        self.build(false)
    }
    /// An empty preview releases every keyed channel. An empty upload remains
    /// invalid, but turning off the final track must not retain a stale sample.
    pub fn preview(&self) -> Result<Animation, &'static str> {
        self.build(true)
    }
    fn build(&self, allow_empty: bool) -> Result<Animation, &'static str> {
        if self.start >= self.end || self.end > LAST || !(0..=6).contains(&self.meta.base_priority) || self.meta.hand_pose > 13 {
            return Err("anim-edit-invalid");
        }
        if ![self.meta.loop_in, self.meta.loop_out, self.meta.ease_in, self.meta.ease_out]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err("anim-edit-invalid");
        }
        let mut animation = self.meta.clone();
        animation.duration = (self.end - self.start) as f32 / FPS;
        let start = self.start as f32 / FPS;
        let end = self.end as f32 / FPS;
        for track in self.tracks.iter().filter(|t| t.enabled) {
            let source = &track.motion;
            if !(-1..=6).contains(&source.priority) {
                return Err("anim-edit-invalid");
            }
            let mut joint = JointMotion {
                joint_name: source.joint_name.clone(),
                priority: source.priority,
                rot_keys: Vec::new(),
                pos_keys: Vec::new(),
            };
            if let (Some(a), Some(b)) = (source.rotation_at(start), source.rotation_at(end)) {
                joint.rot_keys.push(RotKey { time: 0.0, rotation: a });
                joint
                    .rot_keys
                    .extend(source.rot_keys.iter().filter(|k| k.time > start && k.time < end).map(|k| RotKey {
                        time: k.time - start,
                        rotation: k.rotation,
                    }));
                joint.rot_keys.push(RotKey {
                    time: animation.duration,
                    rotation: b,
                });
            }
            if let (Some(a), Some(b)) = (source.position_at(start), source.position_at(end)) {
                joint.pos_keys.push(PosKey { time: 0.0, position: a });
                joint
                    .pos_keys
                    .extend(source.pos_keys.iter().filter(|k| k.time > start && k.time < end).map(|k| PosKey {
                        time: k.time - start,
                        position: k.position,
                    }));
                joint.pos_keys.push(PosKey {
                    time: animation.duration,
                    position: b,
                });
            }
            joint.rot_keys = self.interpolation.rotations(joint.rot_keys);
            joint.pos_keys = self.interpolation.positions(joint.pos_keys);
            if !joint.rot_keys.is_empty() || !joint.pos_keys.is_empty() {
                animation.joints.push(joint);
            }
        }
        if animation.joints.is_empty() && !allow_empty {
            return Err("anim-edit-no-bones");
        }
        Ok(animation)
    }
}
