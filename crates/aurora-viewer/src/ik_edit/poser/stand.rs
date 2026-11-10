//! The pose stand's poses (`app/pose_stand.rs`): our avatar held still in a
//! T-pose or one of its variants, to adjust attachments and clothing.
//!
//! The Legacy Viewer's pose stand (FSFloaterPoseStand, FSPose,
//! indra/newview/fsfloaterposestand.cpp and app_settings/posestand.xml,
//! originally LGPL 2.1) starts animations on the simulator, seen by
//! everyone. Here the same list of poses is built on the rest skeleton and
//! goes through the poser's local layer, like the poser: only this viewer
//! sees it, nothing is sent, no animation asset is used.
use super::SubjectPose;
use crate::scene::anim::Rig;
use glam::Quat;
use serde::{Deserialize, Serialize};

/// The poses of posestand.xml, in the order of its names (PS_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StandPose {
    #[default]
    TPose,
    ArmsDownLegsTogether,
    ArmsDownSitting,
    ArmsDownwardLegsTogether,
    ArmsDownwardLegsApart,
    ArmsForwardLegsApart,
    ArmsForwardLegsTogether,
    ArmsStraightLegsApart,
    ArmsStraightSitting,
    ArmsUpwardLegsApart,
    ArmsUpwardLegsTogether,
}

#[derive(Clone, Copy)]
enum Arms {
    Straight,
    Down,
    Downward,
    Upward,
    Forward,
}

#[derive(Clone, Copy)]
enum Legs {
    Together,
    Apart,
    Sitting,
}

/// How far the arms come down from the T, downward and up (degrees), the
/// legs open and the thighs come up to sit.
const ARMS_DOWN: f32 = 80.0;
const ARMS_DOWNWARD: f32 = 45.0;
const ARMS_UPWARD: f32 = -45.0;
const LEGS_APART: f32 = 12.0;
const SIT: f32 = 90.0;

impl StandPose {
    pub const ALL: [StandPose; 11] = [
        StandPose::TPose,
        StandPose::ArmsDownLegsTogether,
        StandPose::ArmsDownSitting,
        StandPose::ArmsDownwardLegsTogether,
        StandPose::ArmsDownwardLegsApart,
        StandPose::ArmsForwardLegsApart,
        StandPose::ArmsForwardLegsTogether,
        StandPose::ArmsStraightLegsApart,
        StandPose::ArmsStraightSitting,
        StandPose::ArmsUpwardLegsApart,
        StandPose::ArmsUpwardLegsTogether,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StandPose::TPose => aurora_i18n::tr!("b4-stand-t-pose"),
            StandPose::ArmsDownLegsTogether => aurora_i18n::tr!("b4-stand-arms-down-legs-together"),
            StandPose::ArmsDownSitting => aurora_i18n::tr!("b4-stand-arms-down-sitting"),
            StandPose::ArmsDownwardLegsTogether => aurora_i18n::tr!("b4-stand-arms-downward-legs-together"),
            StandPose::ArmsDownwardLegsApart => aurora_i18n::tr!("b4-stand-arms-downward-legs-apart"),
            StandPose::ArmsForwardLegsApart => aurora_i18n::tr!("b4-stand-arms-forward-legs-apart"),
            StandPose::ArmsForwardLegsTogether => aurora_i18n::tr!("b4-stand-arms-forward-legs-together"),
            StandPose::ArmsStraightLegsApart => aurora_i18n::tr!("b4-stand-arms-straight-legs-apart"),
            StandPose::ArmsStraightSitting => aurora_i18n::tr!("b4-stand-arms-straight-sitting"),
            StandPose::ArmsUpwardLegsApart => aurora_i18n::tr!("b4-stand-arms-upward-legs-apart"),
            StandPose::ArmsUpwardLegsTogether => aurora_i18n::tr!("b4-stand-arms-upward-legs-together"),
        }
    }

    fn parts(self) -> (Arms, Legs) {
        match self {
            StandPose::TPose => (Arms::Straight, Legs::Together),
            StandPose::ArmsDownLegsTogether => (Arms::Down, Legs::Together),
            StandPose::ArmsDownSitting => (Arms::Down, Legs::Sitting),
            StandPose::ArmsDownwardLegsTogether => (Arms::Downward, Legs::Together),
            StandPose::ArmsDownwardLegsApart => (Arms::Downward, Legs::Apart),
            StandPose::ArmsForwardLegsApart => (Arms::Forward, Legs::Apart),
            StandPose::ArmsForwardLegsTogether => (Arms::Forward, Legs::Together),
            StandPose::ArmsStraightLegsApart => (Arms::Straight, Legs::Apart),
            StandPose::ArmsStraightSitting => (Arms::Straight, Legs::Sitting),
            StandPose::ArmsUpwardLegsApart => (Arms::Upward, Legs::Apart),
            StandPose::ArmsUpwardLegsTogether => (Arms::Upward, Legs::Together),
        }
    }

    /// The pose on `captured`, the shaped skeleton as it was when the stand
    /// opened: every rotation back to the rest skeleton (the poser's T-pose,
    /// which keeps the shape's joint positions and scales), then the arms
    /// and legs turned. In the avatar's frame X is forward, Y left and Z up,
    /// and the rest skeleton holds its arms along Y and its legs down Z.
    pub fn pose(self, rig: &Rig, captured: &SubjectPose) -> SubjectPose {
        let mut pose = captured.clone();
        pose.rotations.clone_from(&rig.local_rot);
        pose.origin.clone_from(&rig.local_rot);
        pose.locks.clear();
        let (arms, legs) = self.parts();
        let mut turn = |name: &str, delta: Quat| {
            if let Some(&joint) = rig.names.get(name) {
                pose.rotations[joint] = (rig.local_rot[joint] * delta).normalize();
            }
        };
        // a left arm (+Y) comes down turning about X by a negative angle
        let (axis_x, angle) = match arms {
            Arms::Straight => (true, 0.0),
            Arms::Down => (true, ARMS_DOWN),
            Arms::Downward => (true, ARMS_DOWNWARD),
            Arms::Upward => (true, ARMS_UPWARD),
            Arms::Forward => (false, 90.0),
        };
        let arm = |left: bool| {
            let sign = if left { -1.0 } else { 1.0 };
            if axis_x {
                Quat::from_rotation_x((sign * angle).to_radians())
            } else {
                Quat::from_rotation_z((sign * angle).to_radians())
            }
        };
        turn("mShoulderLeft", arm(true));
        turn("mShoulderRight", arm(false));
        match legs {
            Legs::Together => {}
            // a left leg (-Z) opens toward +Y turning about X by a positive angle
            Legs::Apart => {
                turn("mHipLeft", Quat::from_rotation_x(LEGS_APART.to_radians()));
                turn("mHipRight", Quat::from_rotation_x(-LEGS_APART.to_radians()));
            }
            // the thighs forward (about Y, negative), the shins down again
            Legs::Sitting => {
                for side in ["Left", "Right"] {
                    turn(&format!("mHip{side}"), Quat::from_rotation_y(-SIT.to_radians()));
                    turn(&format!("mKnee{side}"), Quat::from_rotation_y(SIT.to_radians()));
                }
            }
        }
        pose
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::avatar::AvatarLibrary;
    use glam::{Mat4, Vec3};

    fn rest(rig: &Rig) -> SubjectPose {
        SubjectPose {
            rotations: rig.local_rot.clone(),
            positions: rig.local_pos.clone(),
            scales: rig.scale.clone(),
            locks: Default::default(),
            origin: rig.local_rot.clone(),
        }
    }

    fn at(rig: &Rig, pose: &SubjectPose, name: &str) -> Vec3 {
        pose.matrices(rig, Mat4::IDENTITY)[rig.names[name]].w_axis.truncate()
    }

    #[test]
    fn each_stand_pose_puts_the_limbs_where_its_name_says() {
        let lib = AvatarLibrary::load();
        let rig = &lib.rig;
        let base = rest(rig);
        let limb = |stand: StandPose, from: &str, to: &str| {
            let pose = stand.pose(rig, &base);
            assert!(pose.valid(rig), "{stand:?}");
            (at(rig, &pose, to) - at(rig, &pose, from)).normalize()
        };
        // the T: arms out sideways, level
        let t = limb(StandPose::TPose, "mShoulderLeft", "mWristLeft");
        assert!(t.y > 0.95, "{t}");
        let t = limb(StandPose::TPose, "mShoulderRight", "mWristRight");
        assert!(t.y < -0.95, "{t}");
        // down, downward, upward and forward, both sides alike
        for side in ["Left", "Right"] {
            let (shoulder, wrist) = (format!("mShoulder{side}"), format!("mWrist{side}"));
            let down = limb(StandPose::ArmsDownLegsTogether, &shoulder, &wrist);
            assert!(down.z < -0.95, "{side} down {down}");
            let downward = limb(StandPose::ArmsDownwardLegsApart, &shoulder, &wrist);
            assert!((downward.z + 0.707).abs() < 0.05, "{side} downward {downward}");
            let upward = limb(StandPose::ArmsUpwardLegsTogether, &shoulder, &wrist);
            assert!((upward.z - 0.707).abs() < 0.05, "{side} upward {upward}");
            let forward = limb(StandPose::ArmsForwardLegsApart, &shoulder, &wrist);
            assert!(forward.x > 0.95, "{side} forward {forward}");
            // sitting: thighs forward, shins down
            let thigh = limb(StandPose::ArmsDownSitting, &format!("mHip{side}"), &format!("mKnee{side}"));
            assert!(thigh.x > 0.95, "{side} thigh {thigh}");
            let shin = limb(StandPose::ArmsStraightSitting, &format!("mKnee{side}"), &format!("mAnkle{side}"));
            assert!(shin.z < -0.95, "{side} shin {shin}");
        }
        // legs apart: the left one toward +Y, the right one toward -Y
        let left = limb(StandPose::ArmsStraightLegsApart, "mHipLeft", "mAnkleLeft");
        let right = limb(StandPose::ArmsStraightLegsApart, "mHipRight", "mAnkleRight");
        assert!(left.y > 0.1 && right.y < -0.1 && left.z < -0.9, "{left} {right}");
        let together = limb(StandPose::TPose, "mHipLeft", "mAnkleLeft");
        assert!(together.y.abs() < 0.05, "{together}");
    }

    #[test]
    fn the_stand_keeps_the_shape_and_drops_the_captured_rotations() {
        let lib = AvatarLibrary::load();
        let rig = &lib.rig;
        let mut captured = rest(rig);
        let pelvis = rig.pelvis;
        captured.positions[pelvis] += Vec3::new(0.0, 0.0, 0.1);
        captured.rotations[rig.names["mHead"]] = Quat::from_rotation_z(0.5);
        captured.locks.insert(pelvis, Mat4::IDENTITY);
        let pose = StandPose::TPose.pose(rig, &captured);
        assert_eq!(pose.positions[pelvis], captured.positions[pelvis]);
        assert_eq!(pose.rotations[rig.names["mHead"]], rig.local_rot[rig.names["mHead"]]);
        assert!(pose.locks.is_empty());
        assert_eq!(StandPose::ALL.len(), 11);
        let saved = serde_json::to_string(&StandPose::ArmsForwardLegsApart).expect("save");
        assert_eq!(
            serde_json::from_str::<StandPose>(&saved).expect("load"),
            StandPose::ArmsForwardLegsApart
        );
    }
}
