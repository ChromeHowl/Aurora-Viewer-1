//! Black Dragon pose files (`.xml`, LLSD XML): reading and writing.
//! Everything this viewer knows of that format is in this module; the rest
//! of the poser only sees a `Document`.
//!
//! What the module relies on:
//!
//! 1. Version 3: ONE document, a map. `version` is a map whose `value` is 3;
//!    every other key is a joint name whose value is a map with `rotation`,
//!    `position`, `scale` (three reals each) and, on skeleton joints only,
//!    `enabled` (boolean). Collision volumes and attachment points have no
//!    `enabled`.
//! 2. Legacy: SEVERAL documents one after another in the file, one map per
//!    joint, the joint's name under `bone`, then the same values.
//! 3. Detection: a document with a string under `bone` is the legacy
//!    layout. Otherwise the file is version 3 when `version.value` is 3,
//!    there is no `startFromTeePose` key (that is the Legacy Viewer's own
//!    poser format, not read here), and at least one joint entry has a
//!    rotation or a position and no `enabled`.
//! 4. `rotation` is Euler angles in radians stored as `[roll, yaw, pitch]`:
//!    the second and third values are swapped against the viewer's usual
//!    `(roll, pitch, yaw)`. The angles are those of `LLQuaternion::
//!    getEulerAngles` / `setEulerAngles`: roll about X applied first, then
//!    pitch about Y, then yaw about Z, i.e. `Rz(yaw) · Ry(pitch) · Rx(roll)`
//!    on column vectors, the same as `ll_euler` of the animation reader
//!    (aurora-assets/src/anim.rs). Black Dragon writes them with
//!    `getEulerAngles(&x, &z, &y)` and reads them with `setEulerAngles(x, z, y)`.
//! 5. Every value is the joint's ABSOLUTE local transform: the complete
//!    rotation in its parent's frame, its position there in metres, its own
//!    scale. A `Document` holds a position offset and a scale ratio, so
//!    they are taken against the rest skeleton and land on the subject's
//!    own skeleton. A file made on an avatar whose shape or rigged mesh
//!    moves joints carries that difference: the "rotations only" load modes
//!    leave it out.
//! 6. A skeleton joint whose `enabled` is false is not driven by the pose:
//!    it is not loaded.
//! 7. A bone this skeleton does not have, a second entry for a bone and an
//!    entry whose numbers are not usable are skipped, not an error; unknown
//!    keys are ignored. A file with no usable joint is refused.
//! 8. Writing: version 3, every `m` joint of the skeleton with
//!    `enabled` true, then the other joints (collision volumes) without
//!    it, in `black_dragon/<URI-escaped pose name>.xml` under the pose
//!    library, to be copied to Black Dragon's poses folder.
//!
//! Points 4 to 6 were checked against the format's description and unit
//! tests only: load and save a real file both ways before relying on them.
use super::*;
use aurora_llsd::Llsd;
use std::collections::HashSet;

/// Positions and scales closer than this to the rest skeleton are the rest.
const SAME: f32 = 1e-4;
/// The folder of the copies, in the pose library.
pub(super) const FOLDER: &str = "black_dragon";

/// One joint entry as the file gives it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Joint {
    pub bone: String,
    pub rotation: Option<Quat>,
    pub position: Option<Vec3>,
    pub scale: Option<Vec3>,
    /// None on the joints that carry no flag (point 1).
    pub enabled: Option<bool>,
}

/// Point 4: stored `[roll, yaw, pitch]`.
fn euler_to_quat(v: [f32; 3]) -> Quat {
    let [roll, yaw, pitch] = v;
    Quat::from_euler(EulerRot::ZYX, yaw, pitch, roll).normalize()
}

fn quat_to_euler(q: Quat) -> [f32; 3] {
    let (yaw, pitch, roll) = q.to_euler(EulerRot::ZYX);
    [roll, yaw, pitch]
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

fn rotation(value: &Llsd) -> Option<Quat> {
    vec3(value).map(|v| euler_to_quat(v.to_array()))
}

fn joint(bone: &str, entry: &Llsd) -> Option<Joint> {
    entry.as_map()?;
    let joint = Joint {
        bone: bone.trim().to_owned(),
        rotation: rotation(entry.get("rotation")),
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

/// The `<llsd>…</llsd>` documents of a file, in order (point 2).
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

fn legacy(document: &Llsd) -> bool {
    matches!(document.get("bone"), Llsd::String(_))
}

/// Point 3, for a document that is not the legacy layout.
fn version_3(document: &Llsd) -> bool {
    let Some(map) = document.as_map() else { return false };
    document.get("version").get("value").as_i32() == 3
        && !document.has("startFromTeePose")
        && map
            .iter()
            .filter(|(key, _)| key.as_str() != "version")
            .any(|(_, entry)| entry.is_map() && (entry.has("rotation") || entry.has("position")) && !entry.has("enabled"))
}

/// The joint entries of a Black Dragon pose file; an error when the file
/// is not one (point 3) or holds nothing usable.
pub(super) fn parse(bytes: &[u8]) -> Result<Vec<Joint>, String> {
    let invalid = || aurora_i18n::tr!("wornposer-bd-invalid").to_string();
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let mut joints = Vec::new();
    for document in documents(text) {
        if legacy(&document) {
            joints.extend(joint(document.get("bone").as_str(), &document));
        } else if version_3(&document)
            && let Some(map) = document.as_map()
        {
            joints.extend(
                map.iter()
                    .filter(|(key, _)| key.as_str() != "version")
                    .filter_map(|(key, entry)| joint(key, entry)),
            );
        }
    }
    if joints.is_empty() {
        return Err(invalid());
    }
    Ok(joints)
}

/// Whether `bytes` look like XML and not an Aurora pose (JSON).
pub(super) fn looks_like(bytes: &[u8]) -> bool {
    bytes.iter().find(|b| !b.is_ascii_whitespace() && ![0xEF, 0xBB, 0xBF].contains(*b)) == Some(&b'<')
}

/// The entries as the parts of a pose document on `rig` (points 5 to 7):
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

/// The pose as joint entries (point 8): the skeleton joints first.
/// `pose` is on the rest skeleton, as the BVH export takes it.
pub(super) fn joints_of(rig: &Rig, pose: &SubjectPose) -> Vec<Joint> {
    let skeleton = |bone: &usize| rig.canonical_names[*bone].starts_with('m');
    let entry = |bone: usize, enabled: Option<bool>| Joint {
        bone: rig.canonical_names[bone].clone(),
        rotation: Some(pose.rotations[bone]),
        position: Some(pose.positions[bone]),
        scale: Some(pose.scales[bone]),
        enabled,
    };
    (0..rig.len())
        .filter(skeleton)
        .map(|bone| entry(bone, Some(true)))
        .chain((0..rig.len()).filter(|bone| !skeleton(bone)).map(|bone| entry(bone, None)))
        .collect()
}

/// The version 3 file of `joints` (point 8).
pub(super) fn encode(joints: &[Joint]) -> String {
    let triple = |v: [f32; 3]| Llsd::Array(v.iter().map(|n| Llsd::Real(f64::from(*n))).collect());
    let mut record = Llsd::new_map();
    let mut version = Llsd::new_map();
    version.insert("value", 3);
    record.insert("version", version);
    for j in joints {
        let mut entry = Llsd::new_map();
        if let Some(rotation) = j.rotation {
            entry.insert("rotation", triple(quat_to_euler(rotation)));
        }
        if let Some(position) = j.position {
            entry.insert("position", triple(position.to_array()));
        }
        if let Some(scale) = j.scale {
            entry.insert("scale", triple(scale.to_array()));
        }
        if let Some(enabled) = j.enabled {
            entry.insert("enabled", enabled);
        }
        record.insert(j.bone.as_str(), entry);
    }
    aurora_llsd::to_pretty_xml_string(&record)
}

/// The file name of a copy: the pose name URI-escaped (everything but
/// letters, digits and `-._~` as `%XX` of its UTF-8 bytes), then `.xml`.
pub(super) fn file_name(pose: &str) -> String {
    let mut out = String::with_capacity(pose.len() + 4);
    for byte in pose.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out.push_str(".xml");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::avatar::AvatarLibrary;

    fn close(a: Quat, b: Quat) -> bool {
        a.dot(b).abs() > 0.99999
    }

    const V3: &str = r#"<?xml version="1.0" ?>
<llsd><map>
  <key>version</key><map><key>value</key><integer>3</integer></map>
  <key>mHead</key><map>
    <key>rotation</key><array><real>0.1</real><real>0.2</real><real>0.3</real></array>
    <key>position</key><array><real>0</real><real>0</real><real>0.076</real></array>
    <key>scale</key><array><real>1</real><real>1</real><real>1</real></array>
    <key>enabled</key><boolean>1</boolean>
    <key>time</key><real>1.5</real>
  </map>
  <key>mNeck</key><map>
    <key>rotation</key><array><real>0</real><real>0.5</real><real>0</real></array>
    <key>enabled</key><boolean>0</boolean>
  </map>
  <key>HEAD</key><map>
    <key>rotation</key><array><real>0</real><real>0</real><real>0</real></array>
    <key>position</key><array><real>0.02</real><real>0</real><real>0.07</real></array>
    <key>scale</key><array><real>0.11</real><real>0.09</real><real>0.12</real></array>
  </map>
</map></llsd>"#;

    const LEGACY: &str = "<llsd><map><key>bone</key><string>mPelvis</string><key>rotation</key><array><real>0</real><real>0.5</real><real>0</real></array></map></llsd>\n\
<llsd><map><key>bone</key><string>mTorso</string><key>rotation</key><array><real>0.25</real><real>0</real><real>0</real></array><key>scale</key><array><real>1</real><real>1</real><real>1</real></array></map></llsd>\n";

    /// Point 4 by hand. The stored vector (π/2, π/2, 0) is roll π/2 and
    /// yaw π/2. Roll first takes X to X and Y to Z; yaw then takes X to Y
    /// and leaves Z: X goes to Y, Y to Z. With `ll_euler`'s closed form
    /// (aurora-assets/src/anim.rs: sr = cr = sy = cy = √½, sp = 0, cp = 1)
    /// that is x = sr·cy = ½, y = sr·sy = ½, z = cr·sy = ½, w = cr·cy = ½.
    /// Read as (roll, pitch, yaw), or composed the other way round, X
    /// would go elsewhere.
    #[test]
    fn the_stored_euler_vector_is_roll_yaw_pitch() {
        use std::f32::consts::FRAC_PI_2;
        let q = euler_to_quat([FRAC_PI_2, FRAC_PI_2, 0.0]);
        assert!(close(q, Quat::from_xyzw(0.5, 0.5, 0.5, 0.5)));
        assert!((q * Vec3::X - Vec3::Y).length() < 1e-5);
        assert!((q * Vec3::Y - Vec3::Z).length() < 1e-5);
        // each stored component alone turns about its axis: X, Z, Y
        assert!(close(euler_to_quat([0.3, 0.0, 0.0]), Quat::from_rotation_x(0.3)));
        assert!(close(euler_to_quat([0.0, 0.3, 0.0]), Quat::from_rotation_z(0.3)));
        assert!(close(euler_to_quat([0.0, 0.0, 0.3]), Quat::from_rotation_y(0.3)));
        // roll applied first, yaw last
        let built = Quat::from_rotation_z(1.1) * Quat::from_rotation_y(-0.7) * Quat::from_rotation_x(0.4);
        assert!(close(euler_to_quat([0.4, 1.1, -0.7]), built));
        let [roll, yaw, pitch] = quat_to_euler(built);
        assert!((roll - 0.4).abs() < 1e-5 && (yaw - 1.1).abs() < 1e-5 && (pitch + 0.7).abs() < 1e-5);
    }

    #[test]
    fn version_3_reads_every_joint_entry() {
        let joints = parse(V3.as_bytes()).unwrap();
        // the map's keys in order: HEAD, mHead, mNeck
        assert_eq!(
            joints.iter().map(|j| j.bone.as_str()).collect::<Vec<_>>(),
            ["HEAD", "mHead", "mNeck"]
        );
        assert_eq!(joints[0].enabled, None);
        assert_eq!(joints[0].scale, Some(Vec3::new(0.11, 0.09, 0.12)));
        assert_eq!(joints[1].enabled, Some(true));
        assert_eq!(joints[1].position, Some(Vec3::new(0.0, 0.0, 0.076)));
        // stored (roll 0.1, yaw 0.2, pitch 0.3)
        let head = Quat::from_rotation_z(0.2) * Quat::from_rotation_y(0.3) * Quat::from_rotation_x(0.1);
        assert!(close(joints[1].rotation.unwrap(), head));
        assert!(close(joints[2].rotation.unwrap(), Quat::from_rotation_z(0.5)));
        assert_eq!((joints[2].enabled, joints[2].position), (Some(false), None));
    }

    #[test]
    fn the_legacy_layout_reads_one_document_per_bone() {
        let joints = parse(LEGACY.as_bytes()).unwrap();
        assert_eq!(joints.iter().map(|j| j.bone.as_str()).collect::<Vec<_>>(), ["mPelvis", "mTorso"]);
        assert!(close(joints[0].rotation.unwrap(), Quat::from_rotation_z(0.5)));
        assert!(close(joints[1].rotation.unwrap(), Quat::from_rotation_x(0.25)));
        assert!(joints.iter().all(|j| j.enabled.is_none()));
    }

    #[test]
    fn what_is_not_a_black_dragon_pose_is_refused() {
        let entry = "<key>mHead</key><map><key>rotation</key><array><real>0</real><real>0</real><real>0</real></array></map>";
        let file = |head: &str, body: &str| format!("<llsd><map>{head}{body}</map></llsd>");
        let v3 = "<key>version</key><map><key>value</key><integer>3</integer></map>";
        assert!(parse(file(v3, entry).as_bytes()).is_ok());
        // another version, no version, the Legacy Viewer's own format
        assert!(parse(file("<key>version</key><map><key>value</key><integer>2</integer></map>", entry).as_bytes()).is_err());
        assert!(parse(file("", entry).as_bytes()).is_err());
        let tee = "<key>startFromTeePose</key><map><key>value</key><boolean>1</boolean></map>";
        assert!(parse(file(&format!("{v3}{tee}"), entry).as_bytes()).is_err());
        // every entry carries `enabled`: not version 3 either
        let flagged = "<key>mHead</key><map><key>rotation</key><array><real>0</real><real>0</real><real>0</real></array><key>enabled</key><boolean>1</boolean></map>";
        assert!(parse(file(v3, flagged).as_bytes()).is_err());
        assert!(parse(file(v3, "").as_bytes()).is_err());
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

    #[test]
    fn a_saved_pose_reads_back_the_same() {
        let lib = AvatarLibrary::load();
        let rig = &lib.rig;
        let mut pose = SubjectPose {
            rotations: rig.local_rot.clone(),
            positions: rig.local_pos.clone(),
            scales: rig.scale.clone(),
            locks: Default::default(),
            origin: rig.local_rot.clone(),
        };
        let (head, pelvis, hand) = (rig.names["mHead"], rig.pelvis, rig.names["mWristLeft"]);
        pose.rotations[head] = (rig.local_rot[head] * Quat::from_euler(EulerRot::XYZ, 0.4, -0.7, 1.1)).normalize();
        pose.rotations[hand] = (rig.local_rot[hand] * Quat::from_euler(EulerRot::XYZ, -2.0, 0.3, -1.2)).normalize();
        pose.positions[pelvis] += Vec3::new(0.0, 0.1, -0.25);
        pose.scales[head] *= Vec3::new(1.2, 1.0, 0.8);
        let written = joints_of(rig, &pose);
        // every joint once: the skeleton's with the flag, the others without
        assert_eq!(written.len(), rig.len());
        assert!(written.iter().all(|j| j.enabled == j.bone.starts_with('m').then_some(true)));
        assert!(
            written.iter().any(|j| j.enabled.is_none()),
            "detection needs an entry without the flag"
        );
        let text = encode(&written);
        assert!(text.contains("<key>version</key>"));
        let read = parse(text.as_bytes()).unwrap();
        assert_eq!(read.len(), written.len());
        let parts = document_joints(rig, &read);
        assert_eq!(parts.len(), rig.len());
        for (name, rotation, offset, ratio) in &parts {
            let bone = rig.names[name];
            assert!(close(*rotation, pose.rotations[bone]), "{name}");
            assert!((rig.local_pos[bone] + *offset - pose.positions[bone]).length() < 1e-3, "{name}");
            assert!((rig.scale[bone] * *ratio - pose.scales[bone]).length() < 1e-3, "{name}");
        }
        assert!(document::Document::from_joints("Round trip".into(), 4, parts).valid(rig));
    }

    #[test]
    fn the_copy_s_file_name_is_uri_escaped() {
        assert_eq!(file_name("Portrait-1_a.b~"), "Portrait-1_a.b~.xml");
        assert_eq!(file_name("Sit down & smile"), "Sit%20down%20%26%20smile.xml");
        assert_eq!(file_name("Été/..\\x"), "%C3%89t%C3%A9%2F..%5Cx.xml");
    }
}
