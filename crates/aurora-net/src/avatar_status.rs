//! The status an Aurora viewer shows beside its avatar's name plate: what
//! the resident listens to, uses or wrote, carried to the other Aurora
//! viewers nearby in a ViewerEffect of an Aurora-only type.
//!
//! This is an extension, not a Legacy Viewer port. The block is the Legacy
//! Viewer's (LLHUDManager::processViewerEffect, indra/newview/llhudmanager.cpp,
//! originally LGPL 2.1: `ID`, `AgentID`, `Type`, `Duration`, `Color`,
//! `TypeData`), sent unreliably like the look-at; only the type number
//! (`HUD_EFFECT_AURORA_STATUS`) and the payload are Aurora's. A viewer that
//! does not know the type drops the effect (LLHUDObject::addHUDEffect's
//! default case logs "Unknown type of hud effect" and returns).
//!
//! The payload is untrusted text from anyone: the sender id of an effect is
//! written by the client, so a receiver draws it as plain, clamped text
//! (`clean_detail`) and never lets it grant a mark, a link or a permission.
//!
//! Wire format, version 1 (at most 109 bytes, the effect allows 255):
//!
//! | offset | size | field                                              |
//! |--------|------|----------------------------------------------------|
//! | 0      | 2    | magic "AS"                                         |
//! | 2      | 1    | version (1)                                        |
//! | 3      | 1    | flags: bit 0 clear (no status), bit 1 paused       |
//! | 4      | 1    | sequence number, wrapping                          |
//! | 5      | 1    | category (`Category`)                              |
//! | 6      | 2    | program id, little endian (0: none or custom)      |
//! | 8      | 1    | detail length `n`, 0 to 100                        |
//! | 9      | n    | UTF-8 detail                                       |
//!
//! Bytes after the detail are ignored, room for later fields.

/// The effect type of a status. The Legacy Viewer's LLHUDObject enum ends at
/// 19 (LL_HUD_EFFECT_RESET_SKELETON); this is far from it, and the payload's
/// magic tells the two apart if the enum ever reaches it.
pub const HUD_EFFECT_AURORA_STATUS: u8 = 0xA7;
const MAGIC: [u8; 2] = *b"AS";
const VERSION: u8 = 1;
const HEADER: usize = 9;
/// Longest detail, in bytes.
pub const MAX_DETAIL: usize = 100;
const FLAG_CLEAR: u8 = 1;
const FLAG_PAUSED: u8 = 2;

/// What a status is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    None,
    /// A program of the shared table (`program`).
    Program,
    Music,
    /// A game of the shared table (`program`).
    Game,
    /// Text the resident wrote.
    Custom,
}

impl Category {
    fn from_byte(b: u8) -> Option<Category> {
        Some(match b {
            0 => Category::None,
            1 => Category::Program,
            2 => Category::Music,
            3 => Category::Game,
            4 => Category::Custom,
            _ => return None,
        })
    }

    fn byte(self) -> u8 {
        match self {
            Category::None => 0,
            Category::Program => 1,
            Category::Music => 2,
            Category::Game => 3,
            Category::Custom => 4,
        }
    }
}

/// One status packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarStatus {
    pub seq: u8,
    /// No status any more: the receiver removes the chip.
    pub clear: bool,
    pub paused: bool,
    pub category: Category,
    /// An id of the viewer's program table; 0 for none or custom text.
    pub program: u16,
    /// "Artist - Title" for music, the resident's text for custom, empty
    /// for a program. Already cleaned.
    pub detail: String,
}

impl AvatarStatus {
    /// The packet that takes a status away.
    pub fn cleared(seq: u8) -> AvatarStatus {
        AvatarStatus {
            seq,
            clear: true,
            paused: false,
            category: Category::None,
            program: 0,
            detail: String::new(),
        }
    }
}

/// Whether `c` is dropped from a detail: control characters, and the
/// invisible ones that reorder or hide text (bidirectional overrides and
/// isolates, zero-width characters, the byte order mark).
fn invisible(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206F}' | '\u{FEFF}')
}

/// A detail made safe to draw as one plain line: whitespace runs become one
/// space, invisible and control characters go, and the rest is cut to
/// `MAX_DETAIL` bytes on a character boundary.
pub fn clean_detail(text: &str) -> String {
    let spaced: String = text
        .chars()
        .filter_map(|c| {
            if c.is_whitespace() {
                Some(' ')
            } else if invisible(c) {
                None
            } else {
                Some(c)
            }
        })
        .collect();
    let joined = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut end = joined.len().min(MAX_DETAIL);
    while !joined.is_char_boundary(end) {
        end -= 1;
    }
    joined[..end].to_owned()
}

/// The bytes of a status (the effect's `TypeData`).
pub fn encode(s: &AvatarStatus) -> Vec<u8> {
    let detail = clean_detail(&s.detail);
    let mut out = Vec::with_capacity(HEADER + detail.len());
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    let mut flags = 0;
    if s.clear {
        flags |= FLAG_CLEAR;
    }
    if s.paused {
        flags |= FLAG_PAUSED;
    }
    out.push(flags);
    out.push(s.seq);
    out.push(s.category.byte());
    out.extend_from_slice(&s.program.to_le_bytes());
    out.push(detail.len() as u8);
    out.extend_from_slice(detail.as_bytes());
    out
}

/// A status from the bytes of an effect, or None for a bad magic, an unknown
/// version or category, a truncated packet or text that is not UTF-8.
pub fn decode(data: &[u8]) -> Option<AvatarStatus> {
    if data.len() < HEADER || data[0..2] != MAGIC || data[2] != VERSION {
        return None;
    }
    let n = usize::from(data[8]);
    if n > MAX_DETAIL || data.len() < HEADER + n {
        return None;
    }
    let detail = std::str::from_utf8(&data[HEADER..HEADER + n]).ok()?;
    Some(AvatarStatus {
        seq: data[4],
        clear: data[3] & FLAG_CLEAR != 0,
        paused: data[3] & FLAG_PAUSED != 0,
        category: Category::from_byte(data[5])?,
        program: u16::from_le_bytes([data[6], data[7]]),
        detail: clean_detail(detail),
    })
}

/// Whether sequence number `new` is newer than `last`, wrapping: a value
/// less than half the range behind is old.
pub fn newer(last: u8, new: u8) -> bool {
    let d = new.wrapping_sub(last);
    d != 0 && d < 128
}

#[cfg(test)]
mod tests {
    use super::*;

    fn music() -> AvatarStatus {
        AvatarStatus {
            seq: 7,
            clear: false,
            paused: false,
            category: Category::Music,
            program: 0,
            detail: "Daft Punk - Veridis Quo".into(),
        }
    }

    #[test]
    fn a_status_round_trips() {
        let s = music();
        let bytes = encode(&s);
        assert_eq!(&bytes[0..3], b"AS\x01");
        assert_eq!(decode(&bytes), Some(s));
        let p = AvatarStatus {
            category: Category::Program,
            program: 0x1234,
            detail: String::new(),
            paused: true,
            ..music()
        };
        assert_eq!(decode(&encode(&p)), Some(p));
        let c = AvatarStatus::cleared(250);
        assert_eq!(decode(&encode(&c)), Some(c));
    }

    #[test]
    fn bad_packets_are_refused() {
        let good = encode(&music());
        assert!(decode(&good[..8]).is_none(), "shorter than the header");
        assert!(decode(&good[..good.len() - 1]).is_none(), "truncated detail");
        let mut bad = good.clone();
        bad[0] = b'X';
        assert!(decode(&bad).is_none(), "magic");
        let mut bad = good.clone();
        bad[2] = 2;
        assert!(decode(&bad).is_none(), "version");
        let mut bad = good.clone();
        bad[5] = 9;
        assert!(decode(&bad).is_none(), "category");
        let mut bad = good.clone();
        bad[8] = 101;
        assert!(decode(&bad).is_none(), "detail longer than allowed");
        let mut bad = good.clone();
        bad[9] = 0xFF;
        assert!(decode(&bad).is_none(), "not UTF-8");
    }

    #[test]
    fn later_fields_are_ignored() {
        let mut bytes = encode(&music());
        bytes.extend_from_slice(&[1, 2, 3]);
        assert_eq!(decode(&bytes), Some(music()));
    }

    #[test]
    fn hostile_text_is_made_plain() {
        let s = AvatarStatus {
            detail: "a\u{202E}b\u{0007}c\u{200B}d\n\te   f\u{2066}".into(),
            ..music()
        };
        // a hand-made packet, as another client could send it
        let mut bytes = encode(&AvatarStatus {
            detail: String::new(),
            ..s.clone()
        });
        let raw = "x\u{202E}y\u{7}z\n  w";
        bytes[8] = raw.len() as u8;
        bytes.extend_from_slice(raw.as_bytes());
        assert_eq!(decode(&bytes).map(|d| d.detail), Some("xyz w".to_owned()));
        assert_eq!(clean_detail(&s.detail), "abcd e f");
    }

    #[test]
    fn long_text_is_cut_on_a_character() {
        let long = "é".repeat(200);
        let cut = clean_detail(&long);
        assert!(cut.len() <= MAX_DETAIL && cut.len() >= MAX_DETAIL - 1);
        assert!(cut.chars().all(|c| c == 'é'));
        let s = AvatarStatus { detail: long, ..music() };
        assert!(encode(&s).len() <= 255);
        assert_eq!(decode(&encode(&s)).map(|d| d.detail), Some(cut));
    }

    #[test]
    fn sequence_numbers_wrap() {
        assert!(newer(1, 2));
        assert!(!newer(2, 2));
        assert!(!newer(2, 1));
        assert!(newer(250, 3), "past the wrap");
        assert!(!newer(3, 250), "an old packet after the wrap");
        assert!(newer(0, 127));
        assert!(!newer(0, 128));
    }
}
