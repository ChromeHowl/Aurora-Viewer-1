//! The total animation: the animation loaded as the base, with the poser's
//! changes applied to it. A new animation; the loaded one is never modified.
//!
//! A change is measured on the joint's own frame, from what the animation
//! gave at its first frame: `rotation = sampled * change` (Hamilton, as the
//! Legacy Viewer's `mRotation * mBaseRotation` in its reversed product).
//! Each rotation key then becomes `key * change`, so the motion of the file
//! stays and the joint is turned the way it was turned on screen. Applying
//! the change to the rest skeleton instead (what the old export did) turns a
//! joint whose captured rotation is not the rest one the wrong way.
use super::*;
use aurora_assets::{
    Animation, JointMotion,
    anim::{PosKey, RotKey},
};
use std::collections::HashMap;

/// An animation loaded as the poser's base, and what its first frame put on
/// each joint it drives (the point the changes are measured from).
#[derive(Clone)]
pub(super) struct AnimBase {
    pub animation: Animation,
    pub rotations: HashMap<usize, Quat>,
    pub positions: HashMap<usize, Vec3>,
}

const SAME: f32 = 0.999_999_9;

/// Put the first frame of `animation` over `start` and keep what it drove.
pub(super) fn load(rig: &Rig, animation: Animation, start: ik::LocalPose) -> (ik::LocalPose, AnimBase) {
    let bound = Arc::new(BoundAnim::bind(animation.clone(), rig));
    let mut pose = start;
    ik::animations::apply(
        rig,
        &mut pose,
        Vec3::ZERO,
        &[ik::animations::Sample {
            animation: bound.clone(),
            frame: 0,
            time_seconds: None,
        }],
    );
    let mut rotations = HashMap::new();
    let mut positions = HashMap::new();
    for (track, bone) in animation.joints.iter().zip(&bound.joints) {
        let Some(bone) = bone.filter(|b| *b < rig.len()) else { continue };
        if !track.rot_keys.is_empty() {
            rotations.insert(bone, pose.rotations[bone]);
        }
        if !track.pos_keys.is_empty() {
            positions.insert(bone, pose.positions[bone]);
        }
    }
    (
        pose,
        AnimBase {
            animation,
            rotations,
            positions,
        },
    )
}

/// The base animation with the changes of `pose` applied. `captured` is the
/// pose when posing started: a joint the animation does not drive gets a key
/// only when it differs from it. `Err` carries an interface text key.
pub(super) fn total(rig: &Rig, base: &AnimBase, pose: &data::Pose, captured: &data::Pose) -> Result<Animation, &'static str> {
    if !pose.valid(rig) {
        return Err("anim-edit-invalid");
    }
    let mut animation = base.animation.clone();
    let mut changed = false;
    for track in &mut animation.joints {
        let Some(&bone) = rig.names.get(&track.joint_name) else { continue };
        if let Some(&was) = base.rotations.get(&bone)
            && !track.rot_keys.is_empty()
        {
            let change = (was.inverse() * pose.rotations[bone]).normalize();
            if change.dot(Quat::IDENTITY).abs() < SAME {
                changed = true;
                for key in &mut track.rot_keys {
                    key.rotation = (key.rotation * change).normalize();
                }
            }
        }
        if let Some(&was) = base.positions.get(&bone)
            && !track.pos_keys.is_empty()
        {
            let delta = pose.positions[bone] - was;
            if delta.length_squared() > 1e-10 {
                changed = true;
                for key in &mut track.pos_keys {
                    key.position = (key.position + delta).clamp(Vec3::splat(-5.0), Vec3::splat(5.0));
                }
            }
        }
    }
    // joints the animation does not drive: the pose's own value, one key
    for bone in 0..rig.len() {
        let name = &rig.canonical_names[bone];
        if !name.starts_with('m') {
            continue;
        }
        let rotate = !base.rotations.contains_key(&bone) && pose.rotations[bone].dot(captured.rotations[bone]).abs() < SAME;
        let moved = !base.positions.contains_key(&bone) && pose.positions[bone].distance_squared(captured.positions[bone]) > 1e-10;
        if !rotate && !moved {
            continue;
        }
        let key = if bone == rig.pelvis {
            pose.positions[bone] - rig.local_pos[bone]
        } else {
            pose.positions[bone]
        };
        if moved && (!key.is_finite() || key.abs().max_element() > 5.0) {
            return Err("anim-edit-pelvis-bound");
        }
        let index = match animation.joints.iter().position(|t| rig.names.get(&t.joint_name) == Some(&bone)) {
            Some(index) => index,
            None => {
                animation.joints.push(JointMotion {
                    joint_name: name.clone(),
                    priority: -1,
                    rot_keys: Vec::new(),
                    pos_keys: Vec::new(),
                });
                animation.joints.len() - 1
            }
        };
        let track = &mut animation.joints[index];
        if rotate {
            track.rot_keys = vec![RotKey {
                time: 0.0,
                rotation: pose.rotations[bone],
            }];
        }
        if moved {
            track.pos_keys = vec![PosKey { time: 0.0, position: key }];
        }
        changed = true;
    }
    if changed { Ok(animation) } else { Err("poser-total-unchanged") }
}

/// The file a total animation is written as, and the bytes the upload
/// preview accepts (the same check as for any `.anim`).
pub(super) fn encode(animation: &Animation) -> Result<Vec<u8>, String> {
    let bytes = aurora_assets::import_animation::encode(animation).map_err(|e| e.to_string())?;
    aurora_assets::anim::parse_animation_upload(&bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
