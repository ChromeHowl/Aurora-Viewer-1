//! Animations for the poser: a loaded animation's first frame as the pose,
//! and the whole pose written as a new held-pose animation.
use super::*;

/// Put the first frame of `animation` over `start`.
pub(super) fn load(rig: &Rig, animation: aurora_assets::Animation, start: ik::LocalPose) -> ik::LocalPose {
    let bound = Arc::new(BoundAnim::bind(animation, rig));
    let mut pose = start;
    ik::animations::apply(
        rig,
        &mut pose,
        Vec3::ZERO,
        &[ik::animations::Sample {
            animation: bound,
            frame: 0,
            time_seconds: None,
        }],
    );
    pose
}

/// The whole pose as a one-frame animation that holds it: every joint the
/// poser controls gets a rotation key (fingers and Bento joints included,
/// edited or not), and a position key where it was moved (the pelvis
/// always). A joint switched off in the poser is the live animation's, so
/// it gets no key. `whole` is `Session::whole`. `Err` carries an interface
/// text key.
pub(super) fn whole(rig: &Rig, whole: &data::Pose, priority: i32) -> Result<aurora_assets::Animation, &'static str> {
    let mut document = clip::document::Document::new();
    document.meta.base_priority = priority;
    let frame = Frame {
        model: Mat4::IDENTITY,
        joints: whole.matrices(rig, Mat4::IDENTITY),
        at: Instant::now(),
    };
    let bones: Vec<usize> = (0..rig.len()).filter(|b| whole.enabled[*b]).collect();
    let moved: Vec<usize> = bones
        .iter()
        .copied()
        .filter(|b| whole.positions[*b].distance_squared(rig.local_pos[*b]) > 1e-10)
        .collect();
    document.record(rig, &frame, 0, &bones, &moved)?;
    document.animation()
}

/// `whole` as the standalone animation Save as animation… writes: the
/// whole pose, absolute rotations for every joint the poser controls (never a
/// difference from a base pose), at `priority`, held while it plays when
/// `looping` and ending at its 30 s length otherwise. The poser models
/// neither a hand pose nor an expression, so the file carries neither.
pub(super) fn standalone(rig: &Rig, pose: &data::Pose, priority: i32, looping: bool) -> Result<aurora_assets::Animation, &'static str> {
    let mut animation = whole(rig, pose, priority)?;
    animation.looping = looping;
    Ok(animation)
}

/// The bytes of an `.anim` file, checked with the reader the upload preview
/// uses.
pub(super) fn encode(animation: &aurora_assets::Animation) -> Result<Vec<u8>, String> {
    let bytes = aurora_assets::import_animation::encode(animation).map_err(|e| e.to_string())?;
    aurora_assets::anim::parse_animation_upload(&bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
