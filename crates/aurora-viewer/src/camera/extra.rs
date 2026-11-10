//! More camera options (Preferences › Camera, the Cameratools window): the
//! camera on a body part, the roll with the head, three third-person
//! choices and the mouselook pack. Their own settings block
//! (`CameraSettings::extra`), every field defaulted so older settings files
//! load; all off by default, the camera then behaves as before.
//!
//! The Legacy Viewer's names are given on each field. RLVa camera
//! restrictions win over all of them: `CameraExtra::under_rlva` is what is
//! left while one is in force (`Rlva::camera_settings`).

use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// The most a mouselook offset moves the eye or the point looked at, metres.
pub const MOUSELOOK_OFFSET: f32 = 2.0;
/// Limits of the head scale in mouselook (MouselookExperimentalHeadScaling).
pub const HEAD_SCALE: std::ops::RangeInclusive<f32> = 0.01..=1.0;
/// Half-life of the roll following the head, seconds.
pub const HEAD_ROLL_HALF_LIFE: f32 = 0.12;

/// The body part the third-person camera follows (FSCameraFollowJoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FollowJoint {
    /// The avatar as a whole: the normal camera.
    #[default]
    Avatar,
    Head,
    Chest,
    Pelvis,
    LeftHand,
    RightHand,
    LeftFoot,
    RightFoot,
}

impl FollowJoint {
    pub const ALL: [FollowJoint; 8] = [
        FollowJoint::Avatar,
        FollowJoint::Head,
        FollowJoint::Chest,
        FollowJoint::Pelvis,
        FollowJoint::LeftHand,
        FollowJoint::RightHand,
        FollowJoint::LeftFoot,
        FollowJoint::RightFoot,
    ];

    /// The skeleton joint (avatar_skeleton.xml); none for the whole avatar.
    pub fn joint(self) -> Option<&'static str> {
        Some(match self {
            FollowJoint::Avatar => return None,
            FollowJoint::Head => "mHead",
            FollowJoint::Chest => "mChest",
            FollowJoint::Pelvis => "mPelvis",
            FollowJoint::LeftHand => "mWristLeft",
            FollowJoint::RightHand => "mWristRight",
            FollowJoint::LeftFoot => "mFootLeft",
            FollowJoint::RightFoot => "mFootRight",
        })
    }

    pub fn label(self) -> &'static str {
        use aurora_i18n::tr;
        match self {
            FollowJoint::Avatar => tr!("camx-joint-avatar"),
            FollowJoint::Head => tr!("camx-joint-head"),
            FollowJoint::Chest => tr!("camx-joint-chest"),
            FollowJoint::Pelvis => tr!("camx-joint-pelvis"),
            FollowJoint::LeftHand => tr!("camx-joint-left-hand"),
            FollowJoint::RightHand => tr!("camx-joint-right-hand"),
            FollowJoint::LeftFoot => tr!("camx-joint-left-foot"),
            FollowJoint::RightFoot => tr!("camx-joint-right-foot"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraExtra {
    /// FSCameraFollowJoint: in third person the camera looks at this body
    /// part as it is animated, keeping its offset from it.
    pub follow_joint: FollowJoint,
    /// FSCinematicCamera: in third person the view rolls with the head.
    pub head_roll: bool,
    /// FSCinematicCameraRollDamping: how much of the head's roll is held
    /// back standing (0 = all of it shows, 1 = none).
    pub roll_damping: f32,
    /// FSCinematicCameraRollDampingSitting: the same, seated.
    pub roll_damping_sitting: f32,
    /// FSCameraSmoothLargeMovements: the smoothing also glides over a jump
    /// of the camera's goal of more than 50 m in a frame.
    pub smooth_large_moves: bool,
    /// FSInvertThirdPersonCameraPitch: a vertical drag orbits the other way.
    pub invert_orbit_pitch: bool,
    /// FSAllowCameraFlipOnSit: on sitting, the camera goes behind the
    /// seated avatar instead of keeping its direction.
    pub flip_on_sit: bool,
    /// FSMouselookCameraOffsetX/Y/Z: the eye moved from the head in
    /// mouselook (x forward, y left, z up of the heading), metres.
    pub mouselook_camera_offset: [f32; 3],
    /// FSMouselookFocusOffsetX/Y/Z: the point looked at moved the same way;
    /// equal to the camera offset, the view moves without turning.
    pub mouselook_focus_offset: [f32; 3],
    /// FSReferenceMouselookSmoothing (MouseSmooth): the mouse's motion in
    /// mouselook smoothed as the reference viewer does.
    pub mouselook_smoothing: bool,
    /// FSRealisticMouselook: the eye follows the animated eyes and the view
    /// rolls with the head.
    pub realistic_mouselook: bool,
    /// MouselookExperimentalHeadScaling: with the avatar shown in mouselook,
    /// its head is drawn shrunk so it never hides the view.
    pub shrink_head: bool,
    /// FSMouselookHeadScale: the head's size then (`HEAD_SCALE`).
    pub head_scale: f32,
}

impl Default for CameraExtra {
    fn default() -> Self {
        CameraExtra {
            follow_joint: FollowJoint::Avatar,
            head_roll: false,
            roll_damping: 0.5,
            roll_damping_sitting: 0.75,
            smooth_large_moves: false,
            invert_orbit_pitch: false,
            flip_on_sit: false,
            mouselook_camera_offset: [0.0; 3],
            mouselook_focus_offset: [0.0; 3],
            mouselook_smoothing: false,
            realistic_mouselook: false,
            shrink_head: false,
            head_scale: 0.1,
        }
    }
}

impl CameraExtra {
    /// The camera needs this frame's pose of our avatar (a joint, the head).
    pub fn uses_pose(&self) -> bool {
        self.follow_joint != FollowJoint::Avatar || self.head_roll || self.realistic_mouselook
    }

    /// What is left while an RLVa camera restriction is in force: nothing
    /// that places or turns the view; the way the mouse moves it stays.
    pub fn under_rlva(&self) -> CameraExtra {
        CameraExtra {
            invert_orbit_pitch: self.invert_orbit_pitch,
            mouselook_smoothing: self.mouselook_smoothing,
            shrink_head: self.shrink_head,
            head_scale: self.head_scale,
            ..CameraExtra::default()
        }
    }

    /// Values from an edited file: finite and in range.
    pub fn sanitized(mut self) -> CameraExtra {
        let d = CameraExtra::default();
        let fix = |v: f32, range: std::ops::RangeInclusive<f32>, def: f32| {
            if v.is_finite() {
                v.clamp(*range.start(), *range.end())
            } else {
                def
            }
        };
        self.roll_damping = fix(self.roll_damping, 0.0..=1.0, d.roll_damping);
        self.roll_damping_sitting = fix(self.roll_damping_sitting, 0.0..=1.0, d.roll_damping_sitting);
        self.head_scale = fix(self.head_scale, HEAD_SCALE, d.head_scale);
        for v in self
            .mouselook_camera_offset
            .iter_mut()
            .chain(self.mouselook_focus_offset.iter_mut())
        {
            *v = fix(*v, -MOUSELOOK_OFFSET..=MOUSELOOK_OFFSET, 0.0);
        }
        self
    }

    /// The roll the view takes from the head this frame: none when the
    /// options leave it alone.
    pub fn roll_damping_for(&self, seated: bool) -> f32 {
        if seated { self.roll_damping_sitting } else { self.roll_damping }
    }
}

/// The head's roll as seen along `forward`: the signed angle (radians, in
/// the sense of `Camera::roll`, the view's up turned about its direction)
/// from the view's reference up to the head's up, both seen across the
/// view. A view rolled by it has its up along the head's. None
/// when the head's up runs along the view (looked at from above or below):
/// no roll can be read there.
pub fn head_roll(head_up: Vec3, forward: Vec3, reference_up: Vec3) -> Option<f32> {
    let f = forward.try_normalize()?;
    let across = |v: Vec3| (v - f * v.dot(f)).try_normalize();
    let head = head_up - f * head_up.dot(f);
    if head.length() < 0.3 {
        return None;
    }
    let (head, reference) = (across(head)?, across(reference_up)?);
    let roll = reference.cross(head).dot(f).atan2(reference.dot(head));
    Some(roll.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2))
}

/// The roll shown: the head's, less the damping, reached smoothly.
pub fn follow_roll(current: f32, target: f32, damping: f32, dt: f32) -> f32 {
    let goal = target * (1.0 - damping.clamp(0.0, 1.0));
    let k = (1.0 - 2f32.powf(-dt.max(0.0) / HEAD_ROLL_HALF_LIFE)).clamp(0.0, 1.0);
    current + (goal - current) * k
}

/// The head's up in the world: the skeleton's head rotation turned by the
/// avatar's.
pub fn head_up(avatar_rotation: Quat, head_rotation: Quat) -> Vec3 {
    (avatar_rotation * head_rotation) * Vec3::Z
}

/// The mouse smoothing of the reference viewer in mouselook. Port of the
/// MouseSmooth branch of LLViewerWindow::updateMouseDelta
/// (indra/newview/llviewerwindow.cpp, originally LGPL 2.1): the motion
/// eases toward the mouse's at 16 per second and is used rounded to whole
/// pixels; it goes on easing out a moment once the mouse stops.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MouseSmoothing {
    /// The mouse's motion since the last frame, pixels.
    raw: Vec2,
    /// The smoothed motion (fdx, fdy).
    filtered: Vec2,
    sensitivity: f32,
}

impl MouseSmoothing {
    pub fn push(&mut self, dx: f32, dy: f32, sensitivity: f32) {
        self.raw += Vec2::new(dx, dy);
        self.sensitivity = sensitivity;
    }

    /// The motion to apply this frame (pixels) and the sensitivity; none
    /// once it rests.
    pub fn step(&mut self, dt: f32) -> Option<(Vec2, f32)> {
        const AMOUNT: f32 = 16.0;
        let k = (dt.max(0.0) * AMOUNT).min(1.0);
        self.filtered += (std::mem::take(&mut self.raw) - self.filtered) * k;
        let rounded = self.filtered.round();
        if rounded == Vec2::ZERO {
            if self.filtered.abs().max_element() < 0.5 {
                self.filtered = Vec2::ZERO;
            }
            return None;
        }
        Some((rounded, self.sensitivity))
    }

    pub fn clear(&mut self) {
        *self = MouseSmoothing::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_4;

    #[test]
    fn everything_is_off_by_default_and_old_files_load() {
        let old: CameraExtra = serde_json::from_str("{}").unwrap();
        assert_eq!(old, CameraExtra::default());
        assert!(!old.uses_pose());
        assert_eq!(old.follow_joint.joint(), None);
        let settings: super::super::CameraSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.extra, CameraExtra::default());
        // a choice survives a save
        let chosen = CameraExtra {
            follow_joint: FollowJoint::Pelvis,
            head_roll: true,
            ..Default::default()
        };
        let back: CameraExtra = serde_json::from_str(&serde_json::to_string(&chosen).unwrap()).unwrap();
        assert_eq!(back, chosen);
        assert!(back.uses_pose());
        assert_eq!(back.follow_joint.joint(), Some("mPelvis"));
    }

    #[test]
    fn an_edited_file_is_brought_back_in_range() {
        let edited = CameraExtra {
            roll_damping: 4.0,
            roll_damping_sitting: f32::NAN,
            head_scale: 0.0,
            mouselook_camera_offset: [9.0, -9.0, f32::INFINITY],
            mouselook_focus_offset: [0.5, 0.0, -0.2],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(edited.roll_damping, 1.0);
        assert_eq!(edited.roll_damping_sitting, CameraExtra::default().roll_damping_sitting);
        assert_eq!(edited.head_scale, *HEAD_SCALE.start());
        assert_eq!(edited.mouselook_camera_offset, [MOUSELOOK_OFFSET, -MOUSELOOK_OFFSET, 0.0]);
        assert_eq!(edited.mouselook_focus_offset, [0.5, 0.0, -0.2]);
    }

    #[test]
    fn rlva_leaves_nothing_that_places_or_turns_the_view() {
        let all = CameraExtra {
            follow_joint: FollowJoint::Head,
            head_roll: true,
            smooth_large_moves: true,
            invert_orbit_pitch: true,
            flip_on_sit: true,
            mouselook_camera_offset: [1.0, 0.0, 0.0],
            mouselook_focus_offset: [1.0, 0.0, 0.0],
            mouselook_smoothing: true,
            realistic_mouselook: true,
            shrink_head: true,
            head_scale: 0.3,
            ..Default::default()
        };
        let left = all.under_rlva();
        assert!(!left.uses_pose());
        assert_eq!(left.follow_joint, FollowJoint::Avatar);
        assert!(!left.smooth_large_moves && !left.flip_on_sit);
        assert_eq!(left.mouselook_camera_offset, [0.0; 3]);
        assert_eq!(left.mouselook_focus_offset, [0.0; 3]);
        // how the mouse moves the view, and how the head is drawn, stay
        assert!(left.invert_orbit_pitch && left.mouselook_smoothing && left.shrink_head);
        assert_eq!(left.head_scale, 0.3);
    }

    #[test]
    fn the_head_roll_is_read_across_the_view() {
        let forward = Vec3::X;
        assert_eq!(head_roll(Vec3::Z, forward, Vec3::Z), Some(0.0));
        // the head leans to its right as seen from behind: the view's up,
        // turned by the roll, is the head's
        let leaned = Quat::from_axis_angle(forward, FRAC_PI_4) * Vec3::Z;
        let roll = head_roll(leaned, forward, Vec3::Z).unwrap();
        assert!((roll - FRAC_PI_4).abs() < 1e-5, "{roll}");
        assert!((Quat::from_axis_angle(forward, roll) * Vec3::Z).abs_diff_eq(leaned, 1e-5));
        let other = Quat::from_axis_angle(forward, -FRAC_PI_4) * Vec3::Z;
        assert!((head_roll(other, forward, Vec3::Z).unwrap() + FRAC_PI_4).abs() < 1e-5);
        // a nod toward the camera is no roll
        let nod = Quat::from_rotation_y(0.4) * Vec3::Z;
        assert!(head_roll(nod, forward, Vec3::Z).unwrap().abs() < 1e-5);
        // looked at from above: unreadable
        assert_eq!(head_roll(Vec3::Z, -Vec3::Z, Vec3::Y), None);
        // lying on its side: held to a quarter turn
        let lying = head_roll(Vec3::Y, forward, Vec3::Z).unwrap();
        assert!(lying.abs() <= std::f32::consts::FRAC_PI_2 + 1e-6);
        // the avatar's turn is the head's too
        let up = head_up(Quat::from_rotation_x(0.3), Quat::IDENTITY);
        assert!((head_roll(up, forward, Vec3::Z).unwrap() - 0.3).abs() < 1e-5);
    }

    #[test]
    fn the_roll_is_damped_and_eased() {
        let mut roll = 0.0;
        for _ in 0..240 {
            roll = follow_roll(roll, 0.4, 0.5, 1.0 / 60.0);
        }
        assert!((roll - 0.2).abs() < 1e-4, "{roll}");
        // one frame moves part of the way only
        let first = follow_roll(0.0, 0.4, 0.0, 1.0 / 60.0);
        assert!(first > 0.0 && first < 0.1, "{first}");
        // fully damped: back to level
        for _ in 0..240 {
            roll = follow_roll(roll, 0.4, 1.0, 1.0 / 60.0);
        }
        assert!(roll.abs() < 1e-4);
        let extra = CameraExtra::default();
        assert_eq!(extra.roll_damping_for(false), extra.roll_damping);
        assert_eq!(extra.roll_damping_for(true), extra.roll_damping_sitting);
    }

    #[test]
    fn the_mouse_smoothing_eases_in_and_out_in_whole_pixels() {
        let mut m = MouseSmoothing::default();
        assert_eq!(m.step(1.0 / 60.0), None, "at rest");
        // a steady 30 px per frame: the motion climbs to it
        let mut last = 0.0;
        for _ in 0..30 {
            m.push(30.0, 0.0, 1.5);
            let (d, sensitivity) = m.step(1.0 / 60.0).unwrap();
            assert_eq!(sensitivity, 1.5);
            assert!(d.x >= last && d.x <= 30.0 && d.x.fract() == 0.0);
            last = d.x;
        }
        assert_eq!(last, 30.0);
        // the mouse stops: it eases out, then rests
        let mut frames = 0;
        while m.step(1.0 / 60.0).is_some() {
            frames += 1;
            assert!(frames < 120);
        }
        assert!(frames > 3, "eased out over {frames} frames");
        assert_eq!(m.step(1.0 / 60.0), None);
        // a long frame takes the mouse's motion at once
        m.push(0.0, -12.0, 1.0);
        assert_eq!(m.step(0.5).map(|s| s.0), Some(Vec2::new(0.0, -12.0)));
        m.clear();
        assert_eq!(m, MouseSmoothing::default());
    }
}
