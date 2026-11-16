//! A deterministic nearby resident for the offline consent scenarios and tests.
use super::*;
pub const PEER: Uuid = Uuid::from_u128(0x727_aaaa);
pub fn peer() -> ObjectUpdate {
    let mut object = action_avatar(false);
    object.full_id = PEER;
    object.local_id = 9727;
    object.position += Vec3::new(1.5, 0.5, 0.0);
    object.name_values = "FirstName STRING RW SV Nova\nLastName STRING RW SV Aurora".into();
    object
}

/// AURORA_DEMO_POSER=compact-crowd: five more residents around you, each
/// with a display name and an account (short to long), who all accept.
pub const CROWD: [(&str, &str, &str); 5] = [
    ("Lumi", "lumi.starling", "Starling"),
    ("Kai Moonshadow", "kai.moonshadow", "Moonshadow"),
    ("Seraphina Delacroix-Vale", "seraphinadelacroix.vale", "Vale"),
    ("Bo", "bo.resident", "Resident"),
    ("Tomasz Wierzbicki", "tomaszwierzbicki", "Resident"),
];
pub fn crowd_id(i: usize) -> Uuid {
    Uuid::from_u128(0x727_bbb0 + i as u128)
}
pub fn crowd() -> Vec<ObjectUpdate> {
    (0..CROWD.len())
        .map(|i| {
            let mut object = action_avatar(false);
            object.full_id = crowd_id(i);
            object.local_id = 9740 + i as u32;
            // an arc in front of you, 2 to 7 m away
            let angle = -0.9 + 0.45 * i as f32;
            let distance = 2.0 + 1.2 * i as f32;
            object.position += Vec3::new(angle.cos() * distance, angle.sin() * distance, 0.0);
            let (display, _, last) = CROWD[i];
            let first = display.split(' ').next().unwrap_or(display);
            object.name_values = format!("FirstName STRING RW SV {first}\nLastName STRING RW SV {last}");
            object
        })
        .collect()
}

/// AURORA_DEMO_POSER=inventory-load: five animation items of the demo
/// inventory, one with full permissions and four the poser's Inventory load
/// refuses (no copy, no modify, no transfer, someone else's). Their assets
/// are the demo's own, made up here, so the demo may name them.
pub const INV_ANIM_FIRST: u128 = 0x727_c001;
pub fn inventory_animations(agent: Uuid, parent: Uuid) -> Vec<aurora_net::inventory::InvItem> {
    use crate::world::inventory::actions::{COPY, FULL, MODIFY, TRANSFER};
    let other = Uuid::from_u128(0x727_c0ff);
    [
        ("Wave (full permissions)", agent, FULL, 0),
        ("Stretch (no copy)", agent, FULL & !COPY, 0),
        ("Bow (no modify)", agent, FULL & !MODIFY, 0),
        ("Sit (no transfer)", agent, FULL & !TRANSFER, 0),
        ("Dance (someone else's)", other, FULL, 0),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (name, owner, mask, everyone))| aurora_net::inventory::InvItem {
        id: Uuid::from_u128(INV_ANIM_FIRST + i as u128),
        parent,
        name: name.into(),
        asset_type: 20,
        inv_type: 19,
        asset_id: Uuid::from_u128(0x727_d001 + i as u128),
        creator: owner,
        owner,
        base_mask: 0x7fff_ffff,
        owner_mask: mask,
        everyone_mask: everyone,
        ..Default::default()
    })
    .collect()
}

/// The motion of the demo's full-permission animation: the head turned and
/// the right arm raised at the first frame, back to rest at the second, and
/// a joint no avatar has, so that the load's notes show.
pub fn inventory_animation() -> aurora_assets::Animation {
    use aurora_assets::{Animation, JointMotion, anim::RotKey};
    let joint = |name: &str, first: Quat| JointMotion {
        joint_name: name.into(),
        priority: -1,
        rot_keys: vec![
            RotKey {
                time: 0.0,
                rotation: first,
            },
            RotKey {
                time: 1.0,
                rotation: Quat::IDENTITY,
            },
        ],
        pos_keys: Vec::new(),
    };
    Animation {
        base_priority: 3,
        duration: 1.0,
        emote_name: String::new(),
        loop_in: 0.0,
        loop_out: 1.0,
        looping: true,
        ends_after: None,
        ease_in: 0.3,
        ease_out: 0.3,
        hand_pose: 0,
        joints: vec![
            joint("mHead", Quat::from_rotation_z(0.6)),
            joint("mShoulderRight", Quat::from_rotation_x(-1.2)),
            joint("mNotARealJoint", Quat::from_rotation_y(0.4)),
        ],
    }
}
