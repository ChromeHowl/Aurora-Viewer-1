//! Offline status chips beside the name plates (`AURORA_DEMO_STATUS`,
//! docs/TESTING.md): the residents of the plates scenario (`AURORA_DEMO_TAGS`,
//! brought along by this variable) share a status as if other Aurora viewers
//! had sent it, through the same event path as the network.
//!
//! `1`: music, a program, a game, a text and a long track on the five full
//! plates (the compact and far plates have no chip). `music`, `program`,
//! `custom`, `long`: the same kind on all five. `cycle`: the kinds in turn
//! every six seconds, with a new sequence number each time. `open`: as `1`
//! with every chip opened, as under the pointer. `hostile`: packets made by
//! hand with bidirectional overrides, control characters, an unknown program
//! and a forged claim, to see them drawn plain. `expire`: one packet of 6 s,
//! never sent again, to see the chips fade and go. `self`: ours.

use crate::world::World;
use aurora_net::NetEvent;
use aurora_net::avatar_status::{self, AvatarStatus, Category};
use uuid::Uuid;

/// The five residents of the full row: Nova, Tess, Orion (a friend of the
/// buddy list), Lune and Brume (`demo_tags`).
fn row() -> [Uuid; 5] {
    let tags = |n: u128| Uuid::from_u128(0xA0E0_7A65_0000_0000_0000_0000_0000_0000 | n);
    [
        tags(1),
        tags(2),
        Uuid::from_u128(0xD0D0_0000_0000_0000_0000_0000_0000_0000 | 102),
        tags(4),
        tags(5),
    ]
}

/// Frames between two rounds of `cycle` (six seconds at 60 frames a second).
const ROUND: u64 = 360;
/// Frame at which the scenario starts: the residents are in the scene.
const START: u64 = 600;
const LONG: &str = "Some Very Long Artist Name - An Even Longer Title Of A Track That Does Not End Soon Enough For A Chip";

/// Whether the chips are shown opened, as under the pointer.
pub fn open() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("AURORA_DEMO").is_some() && std::env::var("AURORA_DEMO_STATUS").is_ok_and(|v| v.trim() == "open"))
}

fn music(seq: u8, text: &str) -> AvatarStatus {
    AvatarStatus {
        seq,
        clear: false,
        paused: false,
        category: Category::Music,
        program: 0,
        detail: text.into(),
    }
}

fn program(seq: u8, id: u16) -> AvatarStatus {
    let game = crate::status_share::program(id).is_some_and(|p| p.game);
    AvatarStatus {
        category: if game { Category::Game } else { Category::Program },
        program: id,
        detail: String::new(),
        ..music(seq, "")
    }
}

fn custom(seq: u8, text: &str) -> AvatarStatus {
    AvatarStatus {
        category: Category::Custom,
        ..music(seq, text)
    }
}

/// The `n`-th kind of status, with sequence number `seq`.
fn kind(n: usize, seq: u8) -> AvatarStatus {
    match n % 5 {
        0 => music(seq, "Daft Punk - Veridis Quo"),
        1 => program(seq, 1),
        2 => program(seq, 100),
        3 => custom(seq, "Building a skybox"),
        _ => music(seq, LONG),
    }
}

fn send(world: &mut World, source: Uuid, status: AvatarStatus, duration: f32) {
    world.apply(NetEvent::AvatarStatus { source, status, duration });
}

/// A packet as another client could write it, with `text` as its detail.
fn raw(seq: u8, category: u8, program: u16, text: &[u8]) -> Vec<u8> {
    let mut b = b"AS\x01".to_vec();
    b.extend_from_slice(&[0, seq, category]);
    b.extend_from_slice(&program.to_le_bytes());
    b.push(text.len() as u8);
    b.extend_from_slice(text);
    b
}

pub fn tick(world: &mut World, frame: u64, mode: &str) {
    if frame < START {
        return;
    }
    let round = frame - START;
    let seq = (round / ROUND % 200) as u8 + 1;
    let residents = row();
    match mode {
        "self" => {
            if round == 0 {
                world
                    .avatar_status
                    .set_own(world.agent_id, Some(music(1, "Daft Punk - Veridis Quo")), std::time::Instant::now());
            }
        }
        "expire" => {
            if round == 0 {
                for (i, r) in residents.iter().enumerate() {
                    send(world, *r, kind(i, 1), 6.0);
                }
            }
        }
        "hostile" => {
            if round == 0 {
                let packets = [
                    raw(1, 2, 0, "Evil \u{202E}gpj.exe\u{7} click here".as_bytes()),
                    raw(1, 1, 4242, b"Totally Safe Program https://example.invalid"),
                    raw(1, 4, 0, "line one\nline two\t\u{200B}tabbed   spaces".as_bytes()),
                    raw(1, 4, 0, &[b'x'; 100]),
                    // not a status at all: refused by the decoder
                    raw(9, 9, 0, b"bad category"),
                ];
                for (r, bytes) in residents.iter().zip(packets) {
                    match avatar_status::decode(&bytes) {
                        Some(status) => send(world, *r, status, 300.0),
                        None => log::info!("demo status: a packet the decoder refused"),
                    }
                }
            }
        }
        _ => {
            let cycling = mode == "cycle";
            if round.is_multiple_of(ROUND) && (cycling || round == 0) {
                let turn = if cycling { (round / ROUND) as usize } else { 0 };
                for (i, r) in residents.iter().enumerate() {
                    let n = match mode {
                        "music" => 0,
                        "program" => 1,
                        "custom" => 3,
                        "long" => 4,
                        _ => i + turn,
                    };
                    send(world, *r, kind(n, seq), 300.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scenario_residents_are_the_plates_full_row() {
        let r = row();
        assert!(r.iter().enumerate().all(|(i, a)| r[i + 1..].iter().all(|b| a != b)));
    }

    #[test]
    fn the_hostile_packets_come_out_plain_and_the_bad_one_is_refused() {
        let evil = avatar_status::decode(&raw(1, 2, 0, "Evil \u{202E}gpj.exe\u{7} click".as_bytes())).expect("decodes");
        assert_eq!(evil.detail, "Evil gpj.exe click");
        let lines = avatar_status::decode(&raw(1, 4, 0, "a\nb\t\u{200B}c   d".as_bytes())).expect("decodes");
        assert_eq!(lines.detail, "a b c d");
        assert!(avatar_status::decode(&raw(9, 9, 0, b"x")).is_none());
        // an unknown program is the generic line, never the packet's text
        let unknown = avatar_status::decode(&raw(1, 1, 4242, b"Safe")).expect("decodes");
        let line = crate::status_share::line(&unknown, 1.0).expect("a line");
        assert!(!line.text.contains("Safe"));
    }
}
