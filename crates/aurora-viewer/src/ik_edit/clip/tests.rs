use super::*;
use crate::scene::avatar::AvatarLibrary;

fn frame(lib: &AvatarLibrary, rot: &[Quat], positions: &[Vec3], scale: &[Vec3]) -> Frame {
    let model = Mat4::from_rotation_translation(Quat::from_euler(EulerRot::ZYX, 0.4, -0.2, 0.1), Vec3::new(132.0, 126.0, 40.0));
    Frame {
        model,
        joints: lib
            .rig
            .world_with(rot, Vec3::ZERO, positions, scale)
            .iter()
            .map(|m| model * *m)
            .collect(),
        at: Instant::now(),
    }
}
#[test]
fn native_capture_preserves_local_rotations_scaled_offsets_and_pelvis_position_without_locking_other_shape_positions() {
    let lib = AvatarLibrary::load();
    let rig = &lib.rig;
    let shoulder = rig.names["mShoulderLeft"];
    let elbow = rig.names["mElbowLeft"];
    let mut rot = rig.local_rot.clone();
    rot[shoulder] = Quat::from_rotation_x(0.4);
    rot[elbow] = Quat::from_rotation_z(-0.6);
    let mut positions = rig.local_pos.clone();
    positions[rig.pelvis] += Vec3::new(0.1, -0.2, 0.3);
    let mut scale = rig.scale.clone();
    scale[shoulder] = Vec3::new(1.2, 0.8, 1.4);
    let f = frame(&lib, &rot, &positions, &scale);
    let mut doc = Document::new();
    doc.record(rig, &f, 0, &[shoulder, elbow, rig.pelvis], &[]).unwrap();
    let animation = doc.animation().unwrap();
    let track = animation.joints.iter().find(|j| j.joint_name == "mElbowLeft").unwrap();
    assert!(track.rot_keys[0].rotation.dot(rot[elbow]).abs() > 0.99999);
    assert!(track.pos_keys.is_empty());
    let pelvis = animation.joints.iter().find(|j| j.joint_name == "mPelvis").unwrap();
    assert!(pelvis.pos_keys[0].position.abs_diff_eq(Vec3::new(0.1, -0.2, 0.3), 3e-5));
    rot[elbow] = Quat::from_rotation_z(-0.8);
    doc.record(rig, &frame(&lib, &rot, &positions, &scale), 60, &[elbow], &[elbow])
        .unwrap();
    assert!(
        doc.tracks.iter().find(|t| t.bone == elbow).unwrap().motion.pos_keys.is_empty(),
        "direct rotation alone must not freeze this shape's joint positions"
    );
    positions[elbow] += Vec3::new(0.02, 0.0, -0.01);
    let f = frame(&lib, &rot, &positions, &scale);
    doc.record(rig, &f, 30, &[elbow], &[elbow]).unwrap();
    let track = &doc.tracks.iter().find(|t| t.bone == elbow).unwrap().motion;
    assert_eq!(track.pos_keys.len(), 2);
    assert!(track.pos_keys[0].position.abs_diff_eq(rig.local_pos[elbow], 4e-5));
    assert!(track.pos_keys[1].position.abs_diff_eq(positions[elbow], 4e-5));
}
#[test]
fn crop_and_native_codec_round_trip_priorities_timing_keys_and_classic_metadata() {
    let lib = AvatarLibrary::load();
    let rig = &lib.rig;
    let bone = rig.names["mHead"];
    let mut rotations = rig.local_rot.clone();
    let mut doc = Document::new();
    doc.record(rig, &frame(&lib, &rotations, &rig.local_pos, &rig.scale), 0, &[bone], &[])
        .unwrap();
    rotations[bone] = Quat::from_rotation_z(1.0);
    doc.record(rig, &frame(&lib, &rotations, &rig.local_pos, &rig.scale), 60, &[bone], &[])
        .unwrap();
    doc.tracks[0].motion.priority = 5;
    doc.meta.base_priority = 3;
    doc.meta.ease_in = 0.2;
    doc.meta.ease_out = 0.3;
    doc.meta.hand_pose = 2;
    doc.meta.emote_name = "express_smile".into();
    doc.range(15, 45);
    doc.meta.loop_in = 0.1;
    doc.meta.loop_out = 0.9;
    let animation = doc.animation().unwrap();
    assert_eq!(animation.duration, 1.0);
    assert!(animation.joints[0].rot_keys[0].rotation.dot(Quat::from_rotation_z(0.25)).abs() > 0.9999);
    let bytes = aurora_assets::import_animation::encode(&animation).unwrap();
    let decoded = aurora_assets::anim::parse_animation_upload(&bytes).unwrap();
    assert_eq!(decoded.base_priority, 3);
    assert_eq!(decoded.joints[0].priority, 5);
    assert_eq!(decoded.duration, 1.0);
    assert_eq!(decoded.hand_pose, 2);
    assert_eq!(decoded.emote_name, "express_smile");
    assert_eq!(decoded.loop_in, 0.1);
    assert_eq!(decoded.loop_out, 0.9);
    assert_eq!(decoded.ease_in, 0.2);
    assert_eq!(decoded.ease_out, 0.3);
    assert_eq!(doc.frames(Some(bone)), vec![0, 60]);
    assert!(decoded.joints[0].rot_keys[1].rotation.dot(Quat::from_rotation_z(0.75)).abs() > 0.9999);
    doc.tracks[0].enabled = false;
    assert!(doc.animation().is_err());
    assert!(
        doc.preview().unwrap().joints.is_empty(),
        "the last disabled track releases its preview channel"
    );
}
#[test]
fn native_capture_rejects_out_of_range_positions_atomically() {
    let lib = AvatarLibrary::load();
    let rig = &lib.rig;
    let mut positions = rig.local_pos.clone();
    positions[rig.pelvis].x += 5.1;
    let mut doc = Document::new();
    assert!(
        doc.record(
            rig,
            &frame(&lib, &rig.local_rot, &positions, &rig.scale),
            0,
            &[rig.names["mHead"], rig.pelvis],
            &[]
        )
        .is_err()
    );
    assert!(doc.tracks.is_empty());
}

#[test]
fn recorded_ik_chains_release_only_their_pose_goals_after_the_new_sample_and_leaving_restores_contacts() {
    let lib = Arc::new(AvatarLibrary::load());
    let rig = lib.rig.clone();
    let world = World::new(lib.clone());
    let original = frame(&lib, &rig.local_rot, &rig.local_pos, &rig.scale);
    let mut editor = Editor::default();
    editor.toggle();
    editor.select(rig.names["mWristLeft"], &rig, &original);
    editor.commit();
    let saved = editor.goals[0].clone();
    editor.enter_clip(&world, &original);
    let ctx = egui::Context::default();
    let (jobs, _) = Jobs::new();
    let _inline = crate::scene::jobs::execute_inline_on_this_thread();
    editor.tick_clip(&jobs, &ctx, rig.clone());
    editor.tick_clip(&jobs, &ctx, rig.clone());
    assert!(editor.goals.is_empty());
    editor.clip_seek(30);
    editor.select(rig.names["mWristLeft"], &rig, &original);
    editor.position += Vec3::new(0.04, 0.02, -0.01);
    editor.weight = 0.5;
    editor.commit();
    let wrist = editor.goals[0].clone();
    let path = ik::chain(&rig, wrist.bone, wrist.root).unwrap();
    editor.select(rig.names["mHead"], &rig, &original);
    editor.mode = Mode::Direct;
    editor.degrees.z += 20.0;
    editor.orient = true;
    editor.commit();
    let mut solved = ik::LocalPose {
        rotations: rig.local_rot.clone(),
        positions: rig.local_pos.clone(),
    };
    assert!(ik::apply_goal(&rig, &mut solved, Vec3::ZERO, &rig.scale, &wrist));
    let posed = frame(&lib, &solved.rotations, &solved.positions, &rig.scale);
    editor.clip.capture = Some(Capture::Bone(wrist.bone));
    editor.capture_clip(&world, &posed);
    assert_eq!(editor.goals.len(), 2, "hold the pose while the new preview is building");
    for bone in &path {
        assert!(editor.clip.document.as_ref().unwrap().frames(Some(*bone)).contains(&30));
    }
    editor.tick_clip(&jobs, &ctx, rig.clone());
    editor.tick_clip(&jobs, &ctx, rig.clone());
    assert!(editor.goals.iter().all(|g| g.bone != wrist.bone));
    assert!(editor.goals.iter().any(|g| g.bone == rig.names["mHead"]));
    editor.leave_clip();
    assert_eq!(editor.goals.len(), 1);
    assert_eq!(editor.goals[0].bone, saved.bone);
    assert_eq!(editor.goals[0].position, saved.position);
    editor.clip.clear_after = Some(editor.clip.revision);
    editor.clip.clear_bones = vec![saved.bone];
    let document = editor.clip.document.as_ref().unwrap();
    editor
        .clip
        .tx
        .send(files::Output::Preview {
            revision: editor.clip.revision,
            start: document.start,
            end: document.end,
            result: Ok(Arc::new(BoundAnim::bind(document.animation().unwrap(), &rig))),
        })
        .unwrap();
    editor.tick_clip(&jobs, &ctx, rig.clone());
    assert_eq!(editor.goals.len(), 1, "inactive creation cannot remove restored contacts");
    editor.toggle();
    editor.toggle();
    assert!(editor.clip.document.is_some());
    assert!(!editor.clip.active);
}

#[test]
fn current_ik_pose_can_be_added_to_an_existing_draft_and_new_drafts_preserve_previous_keys() {
    let lib = Arc::new(AvatarLibrary::load());
    let rig = lib.rig.clone();
    let world = World::new(lib.clone());
    let original = frame(&lib, &rig.local_rot, &rig.local_pos, &rig.scale);
    let mut editor = Editor::default();
    editor.toggle();
    editor.select(rig.names["mHead"], &rig, &original);
    editor.new_clip(&world, &original);
    editor.leave_clip();
    let old = editor.clip.document.as_ref().unwrap().clone();
    editor.clip.frame = 30;
    let mut rotations = rig.local_rot.clone();
    rotations[rig.names["mHead"]] = Quat::from_rotation_z(0.6);
    let posed = frame(&lib, &rotations, &rig.local_pos, &rig.scale);
    editor.capture_ik_clip(&world, &posed);
    assert!(editor.clip.active);
    assert!(editor.clip.waiting_source());
    assert_eq!(editor.clip.document.as_ref().unwrap().frames(Some(rig.names["mHead"])), vec![0, 30]);
    assert_eq!(
        old.frames(Some(rig.names["mHead"])),
        vec![0],
        "worker snapshots and retained drafts are immutable"
    );
    editor.leave_clip();
    editor.new_clip(&world, &original);
    assert_eq!(editor.clip.history.len(), 1);
    assert_eq!(editor.clip.history[0].frames(Some(rig.names["mHead"])), vec![0, 30]);
    assert_eq!(editor.clip.document.as_ref().unwrap().frames(Some(rig.names["mHead"])), vec![0]);
}

#[test]
fn creation_records_the_rest_skeleton_never_the_animation_playing_on_the_avatar() {
    let lib = Arc::new(AvatarLibrary::load());
    let rig = lib.rig.clone();
    let world = World::new(lib.clone());
    let elbow = rig.names["mElbowLeft"];
    let mut rotations = rig.local_rot.clone();
    rotations[elbow] = Quat::from_euler(EulerRot::ZYX, 0.9, -0.4, 0.7);
    let mut positions = rig.local_pos.clone();
    positions[rig.pelvis] += Vec3::new(0.1, -0.2, 0.3);
    let playing = frame(&lib, &rotations, &positions, &rig.scale);
    let at_rest = |editor: &Editor, time: f32| {
        let document = editor.clip.document.as_ref().unwrap();
        let track = document.tracks.iter().find(|t| t.bone == elbow).unwrap();
        track.motion.rotation_at(time).unwrap().dot(rig.local_rot[elbow]).abs() > 0.9999
    };
    let mut editor = Editor::default();
    editor.toggle();
    editor.new_clip(&world, &playing);
    assert!(at_rest(&editor, 0.0), "a new animation");
    let document = editor.clip.document.as_ref().unwrap();
    let pelvis = document.tracks.iter().find(|t| t.bone == rig.pelvis).unwrap();
    assert!(pelvis.motion.position_at(0.0).unwrap().abs_diff_eq(Vec3::ZERO, 1e-4));
    editor.leave_clip();
    editor.clip.frame = 30;
    editor.capture_ik_clip(&world, &playing);
    assert!(at_rest(&editor, 1.0), "the IK capture");
    editor.leave_clip();
    editor.clip.document = None;
    editor.enter_clip(&world, &playing);
    assert!(at_rest(&editor, 0.0), "entering without a draft");

    // while the first preview builds the avatar still shows its source, and
    // a key cannot read it
    assert!(editor.clip.waiting_source());
    assert!(!editor.request().suppress_live);
    editor.clip.frame = 45;
    editor.clip.capture = Some(Capture::Pose);
    editor.capture_clip(&world, &playing);
    assert_eq!(editor.clip.document.as_ref().unwrap().frames(Some(elbow)), vec![0]);
    let ctx = egui::Context::default();
    let (jobs, _) = Jobs::new();
    let _inline = crate::scene::jobs::execute_inline_on_this_thread();
    editor.tick_clip(&jobs, &ctx, rig.clone());
    editor.tick_clip(&jobs, &ctx, rig.clone());
    assert!(!editor.clip.waiting_source());
    assert!(
        editor.request().suppress_live,
        "disabled tracks show the rest skeleton, not the animation"
    );
}

#[test]
fn creation_bakes_the_contacts_the_user_set_on_the_rest_skeleton() {
    let lib = Arc::new(AvatarLibrary::load());
    let rig = lib.rig.clone();
    let world = World::new(lib.clone());
    let wrist = rig.names["mWristLeft"];
    let original = frame(&lib, &rig.local_rot, &rig.local_pos, &rig.scale);
    let mut editor = Editor::default();
    editor.toggle();
    editor.select(wrist, &rig, &original);
    editor.position += Vec3::new(0.04, 0.02, -0.01);
    editor.commit();
    let goal = editor.request().goals[0].clone();
    let mut solved = ik::LocalPose {
        rotations: rig.local_rot.clone(),
        positions: rig.local_pos.clone(),
    };
    assert!(ik::apply_goal(&rig, &mut solved, Vec3::ZERO, &rig.scale, &goal));
    let mut rotations = rig.local_rot.clone();
    rotations[rig.names["mHead"]] = Quat::from_rotation_x(0.8);
    editor.new_clip(&world, &frame(&lib, &rotations, &rig.local_pos, &rig.scale));
    let document = editor.clip.document.as_ref().unwrap();
    let key = |bone: usize| {
        let track = document.tracks.iter().find(|t| t.bone == bone).unwrap();
        track.motion.rotation_at(0.0).unwrap()
    };
    let elbow = rig.names["mElbowLeft"];
    assert!(
        solved.rotations[elbow].dot(rig.local_rot[elbow]).abs() < 0.99999,
        "the contact moves the arm"
    );
    assert!(key(elbow).dot(solved.rotations[elbow]).abs() > 0.9999, "and is in the key");
    let head = rig.names["mHead"];
    assert!(key(head).dot(rig.local_rot[head]).abs() > 0.9999, "the played head is not");
}
fn opened_animation(joints: Vec<aurora_assets::JointMotion>) -> Animation {
    Animation {
        base_priority: 9,
        duration: 1.5,
        emote_name: String::new(),
        loop_in: 0.2,
        loop_out: 1.0,
        looping: true,
        ends_after: None,
        ease_in: 0.1,
        ease_out: 0.1,
        hand_pose: 20,
        joints,
    }
}
fn head_motion() -> aurora_assets::JointMotion {
    use aurora_assets::anim::RotKey;
    aurora_assets::JointMotion {
        joint_name: "mHead".into(),
        priority: 3,
        rot_keys: vec![
            RotKey {
                time: 0.0,
                rotation: Quat::IDENTITY,
            },
            RotKey {
                time: 1.0,
                rotation: Quat::from_rotation_z(0.6),
            },
            RotKey {
                time: 1.5,
                rotation: Quat::from_rotation_z(0.2),
            },
        ],
        pos_keys: Vec::new(),
    }
}
#[test]
fn an_opened_animation_becomes_the_timeline_and_exports_its_keys() {
    let lib = AvatarLibrary::load();
    let rig = &lib.rig;
    let unknown = aurora_assets::JointMotion {
        joint_name: "notAJoint".into(),
        ..head_motion()
    };
    let document = Document::from_animation(rig, &opened_animation(vec![head_motion(), unknown.clone()]), "Wave".into()).expect("opened");
    assert_eq!(document.name, "Wave");
    assert_eq!(document.tracks.len(), 1, "joints the rig does not know are left out");
    assert_eq!(document.tracks[0].bone, rig.names["mHead"]);
    assert_eq!((document.start, document.end), (0, 45));
    assert_eq!((document.meta.base_priority, document.meta.hand_pose), (6, 13));
    assert!((document.meta.loop_in - 0.2).abs() < 1e-6 && document.meta.looping);
    assert_eq!(document.frames(None), vec![0, 30, 45]);
    let exported = document.animation().expect("export");
    let head = &exported.joints[0];
    assert_eq!(head.rot_keys.len(), 3);
    assert!(head.rot_keys[1].rotation.angle_between(Quat::from_rotation_z(0.6)) < 1e-3);
    assert!(Document::from_animation(rig, &opened_animation(vec![unknown]), "None".into()).is_err());
}
#[test]
fn the_chosen_interpolation_is_written_into_the_exported_keys() {
    let lib = AvatarLibrary::load();
    let rig = &lib.rig;
    let mut document = Document::from_animation(rig, &opened_animation(vec![head_motion()]), "Wave".into()).expect("opened");
    let keys = |d: &Document| d.animation().expect("export").joints[0].rot_keys.clone();
    assert_eq!(keys(&document).len(), 3);
    document.interpolation = interpolation::Interpolation::Step;
    // one held key before each later key
    assert_eq!(keys(&document).len(), 5);
    document.interpolation = interpolation::Interpolation::Spline;
    // a key on every frame from 0 to 45
    assert_eq!(keys(&document).len(), 46);
    assert!(document.preview().is_ok());
}
#[test]
fn the_draft_is_autosaved_after_a_change_at_most_every_five_seconds() {
    let t0 = Instant::now();
    assert!(autosave_due(1, None, None, t0));
    assert!(!autosave_due(1, Some(1), None, t0), "already written");
    assert!(!autosave_due(2, Some(1), Some(t0), t0 + std::time::Duration::from_secs(2)));
    assert!(autosave_due(2, Some(1), Some(t0), t0 + AUTOSAVE_EVERY));
}
