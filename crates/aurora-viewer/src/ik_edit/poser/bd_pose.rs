//! Black Dragon pose files in their older layout (`.xml`, LLSD XML):
//! reading only. Nothing is written in this format: Save writes the Aurora
//! pose file and Save as animation writes an `.anim`.
//!
//! The newer layout (one map of joints with `version.value` 3) is read by
//! `legacy_pose`, which handles the Alchemy and Legacy Viewer files; this
//! module reads what that one cannot, the layout that has SEVERAL
//! documents one after another in the file:
//!
//! 1. One `<llsd>` map per joint, the joint's name under `bone`, then
//!    `rotation`, `position` and `scale` (three reals each) and, on
//!    skeleton joints, `enabled` (boolean). Unknown keys are ignored.
//! 2. `rotation` is stored `[roll, yaw, pitch]`, the second and third
//!    values swapped against `(roll, pitch, yaw)`: the viewer writes it with
//!    `getEulerAngles(&x, &z, &y)` and reads it with `setEulerAngles(x, z,
//!    y)`, and the matrix of `LLMatrix3(roll, pitch, yaw)` is the one
//!    `legacy_pose` uses for the same files (a column form `Rx · Ry · Rz`).
//! 3. Every value is the joint's ABSOLUTE local transform: its complete
//!    rotation in its parent's frame, its position there in metres, its own
//!    scale. A `Document` holds a position offset and a scale ratio, so
//!    they are taken against the rest skeleton and land on the subject's
//!    own skeleton. A file made on an avatar whose shape or rigged mesh
//!    moves joints carries that difference: the "rotations only" load modes
//!    leave it out.
//! 4. A joint whose `enabled` is false is not driven by the pose: it is not
//!    loaded.
//! 5. A bone this skeleton does not have, a second entry for a bone and an
//!    entry whose numbers are not usable are skipped, not an error. A file
//!    with no usable joint is refused.
//!
//! Points 2 and 3 were checked against the format's description and unit
//! tests only: load a real file of this layout before relying on them.
use super::*;
use aurora_llsd::Llsd;
use std::collections::HashSet;

/// Positions and scales closer than this to the rest skeleton are the rest.
const SAME: f32 = 1e-4;

/// One joint entry as the file gives it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Joint {
    pub bone: String,
    pub rotation: Option<Quat>,
    pub position: Option<Vec3>,
    pub scale: Option<Vec3>,
    /// None on the joints that carry no flag.
    pub enabled: Option<bool>,
}

/// Point 2: stored `[roll, yaw, pitch]`.
fn euler_to_quat(v: [f32; 3]) -> Quat {
    let [roll, yaw, pitch] = v;
    Quat::from_euler(EulerRot::XYZ, roll, pitch, yaw).normalize()
}

fn reals(value: &Llsd) -> Option<Vec<f32>> {
    let list = value.as_array();
    let numbers: Vec<f32> = list
        .iter()
        .filter(|v| matches!(v, Llsd::Real(_) | Llsd::Integer(_)))
        .map(Llsd::as_f32)
        .collect();
    (numbers.len() == list.len() && numbers.iter().all(|n| n.is_finite())).then_some(numbers)
}

fn vec3(value: &Llsd) -> Option<Vec3> {
    match reals(value)?.as_slice() {
        [x, y, z] => Some(Vec3::new(*x, *y, *z)),
        _ => None,
    }
}

fn joint(entry: &Llsd) -> Option<Joint> {
    entry.as_map()?;
    let joint = Joint {
        bone: entry.get("bone").as_str().trim().to_owned(),
        rotation: vec3(entry.get("rotation")).map(|v| euler_to_quat(v.to_array())),
        position: vec3(entry.get("position")),
        scale: vec3(entry.get("scale")),
        enabled: match entry.get("enabled") {
            Llsd::Boolean(on) => Some(*on),
            Llsd::Integer(on) => Some(*on != 0),
            _ => None,
        },
    };
    (!joint.bone.is_empty() && (joint.rotation.is_some() || joint.position.is_some() || joint.scale.is_some())).then_some(joint)
}

/// The `<llsd>…</llsd>` documents of a file, in order (point 1).
fn documents(text: &str) -> Vec<Llsd> {
    const END: &str = "</llsd>";
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<llsd") {
        let Some(len) = rest[start..].find(END) else { break };
        let end = start + len + END.len();
        // one unreadable document does not refuse the others
        if let Ok(value) = aurora_llsd::from_xml(&rest.as_bytes()[start..end]) {
            out.push(value);
        }
        rest = &rest[end..];
    }
    out
}

/// The joint entries of a pose file of this layout; an error when the file
/// is not one or holds nothing usable.
pub(super) fn parse(bytes: &[u8]) -> Result<Vec<Joint>, String> {
    let invalid = || aurora_i18n::tr!("poser-bd-invalid").to_string();
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let joints: Vec<Joint> = documents(text)
        .iter()
        .filter(|document| matches!(document.get("bone"), Llsd::String(_)))
        .filter_map(joint)
        .collect();
    if joints.is_empty() {
        return Err(invalid());
    }
    Ok(joints)
}

/// Whether `bytes` look like XML and not an Aurora pose (JSON).
pub(super) fn looks_like(bytes: &[u8]) -> bool {
    bytes.iter().find(|b| !b.is_ascii_whitespace() && ![0xEF, 0xBB, 0xBF].contains(*b)) == Some(&b'<')
}

/// The entries as the parts of a pose document on `rig` (points 3 to 5):
/// name, complete rotation, position offset and scale ratio.
pub(super) fn document_joints(rig: &Rig, joints: &[Joint]) -> Vec<(String, Quat, Vec3, Vec3)> {
    let mut seen = HashSet::new();
    joints
        .iter()
        .filter(|j| j.enabled != Some(false))
        .filter_map(|j| {
            let bone = *rig.names.get(&j.bone)?;
            if !seen.insert(bone) {
                return None;
            }
            let offset = j.position.map_or(Vec3::ZERO, |p| p - rig.local_pos[bone]);
            let ratio = j.scale.map_or(Vec3::ONE, |s| s / rig.scale[bone]);
            let offset = if offset.is_finite() && offset.abs().max_element() > SAME && offset.abs().max_element() <= 100.0 {
                offset
            } else {
                Vec3::ZERO
            };
            let ratio = if data::scale_valid(ratio) && (ratio - Vec3::ONE).abs().max_element() > SAME {
                ratio
            } else {
                Vec3::ONE
            };
            Some((
                rig.canonical_names[bone].clone(),
                j.rotation.unwrap_or(rig.local_rot[bone]),
                offset,
                ratio,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::avatar::AvatarLibrary;

    fn close(a: Quat, b: Quat) -> bool {
        a.dot(b).abs() > 0.99999
    }

    const FILE: &str = "<llsd><map><key>bone</key><string>mPelvis</string><key>rotation</key><array><real>0</real><real>0.5</real><real>0</real></array></map></llsd>\n\
<llsd><map><key>bone</key><string>mTorso</string><key>rotation</key><array><real>0.25</real><real>0</real><real>0</real></array><key>scale</key><array><real>1</real><real>1</real><real>1</real></array><key>enabled</key><boolean>1</boolean></map></llsd>\n";

    /// Point 2 by hand: each stored component alone turns about its axis,
    /// roll about X, the second value (yaw) about Z, the third (pitch)
    /// about Y, the way `legacy_pose` reads the same values.
    #[test]
    fn the_stored_euler_vector_is_roll_yaw_pitch() {
        assert!(close(euler_to_quat([0.3, 0.0, 0.0]), Quat::from_rotation_x(0.3)));
        assert!(close(euler_to_quat([0.0, 0.3, 0.0]), Quat::from_rotation_z(0.3)));
        assert!(close(euler_to_quat([0.0, 0.0, 0.3]), Quat::from_rotation_y(0.3)));
        let built = Quat::from_rotation_x(0.4) * Quat::from_rotation_y(-0.7) * Quat::from_rotation_z(1.1);
        assert!(close(euler_to_quat([0.4, 1.1, -0.7]), built));
        // the same matrix as the rest of the poser's Legacy files
        let (roll, yaw, pitch) = (0.4, 1.1, -0.7);
        assert!(close(
            euler_to_quat([roll, yaw, pitch]),
            Quat::from_euler(EulerRot::XYZ, roll, pitch, yaw)
        ));
    }

    #[test]
    fn the_layout_reads_one_document_per_bone() {
        let joints = parse(FILE.as_bytes()).unwrap();
        assert_eq!(joints.iter().map(|j| j.bone.as_str()).collect::<Vec<_>>(), ["mPelvis", "mTorso"]);
        assert!(close(joints[0].rotation.unwrap(), Quat::from_rotation_z(0.5)));
        assert!(close(joints[1].rotation.unwrap(), Quat::from_rotation_x(0.25)));
        assert_eq!((joints[0].enabled, joints[1].enabled), (None, Some(true)));
        assert_eq!(joints[1].scale, Some(Vec3::ONE));
    }

    #[test]
    fn what_is_not_this_layout_is_refused() {
        // the single-map layout is `legacy_pose`'s
        let single = "<llsd><map><key>version</key><map><key>value</key><integer>3</integer></map><key>mHead</key><map><key>rotation</key><array><real>0</real><real>0</real><real>0</real></array></map></map></llsd>";
        assert!(parse(single.as_bytes()).is_err());
        assert!(parse(b"").is_err());
        assert!(parse(b"{\"version\":1}").is_err());
        assert!(parse(&[0xFF, 0xFE, 0x00]).is_err());
        // numbers that are not usable leave nothing
        let bad = "<llsd><map><key>bone</key><string>mHead</string><key>rotation</key><array><real>nan</real><real>0</real><real>0</real></array></map></llsd>";
        assert!(parse(bad.as_bytes()).is_err());
        assert!(looks_like(b"\n  <?xml version=\"1.0\"?><llsd/>"));
        assert!(!looks_like(b" {\"version\":1}"));
    }

    #[test]
    fn an_entry_with_unusable_numbers_does_not_refuse_the_file() {
        let file = format!(
            "<llsd><map><key>bone</key><string>mHead</string><key>rotation</key><array><real>0</real></array></map></llsd>\n{FILE}"
        );
        assert_eq!(parse(file.as_bytes()).unwrap().len(), 2);
    }

    #[test]
    fn entries_become_document_joints_on_the_skeleton() {
        let lib = AvatarLibrary::load();
        let rig = &lib.rig;
        let head = rig.names["mHead"];
        let entry = |bone: &str, enabled: Option<bool>| Joint {
            bone: bone.into(),
            rotation: Some(Quat::from_rotation_x(0.3)),
            position: Some(rig.local_pos[head] + Vec3::new(0.0, 0.0, 0.05)),
            scale: Some(rig.scale[head] * 2.0),
            enabled,
        };
        let parts = document_joints(
            rig,
            &[
                entry("mHead", Some(true)),
                entry("mHead", Some(true)),
                entry("mNoSuchBone", Some(true)),
                entry("mNeck", Some(false)),
            ],
        );
        // unknown, repeated and disabled entries are left out
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].0, "mHead");
        assert!((parts[0].2 - Vec3::new(0.0, 0.0, 0.05)).length() < 1e-5);
        assert!((parts[0].3 - Vec3::splat(2.0)).length() < 1e-5);
        // no flag: loaded; no rotation, position or scale: the rest skeleton's
        let bare = Joint {
            rotation: None,
            position: None,
            scale: Some(rig.scale[head]),
            ..entry("mHead", None)
        };
        let parts = document_joints(rig, &[bare]);
        assert!(close(parts[0].1, rig.local_rot[head]));
        assert_eq!((parts[0].2, parts[0].3), (Vec3::ZERO, Vec3::ONE));
    }
}
