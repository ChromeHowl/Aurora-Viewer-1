//! The status beside the name plate: what we share with the other Aurora
//! viewers nearby, and how a status we receive reads. An extension of
//! Aurora, not a Legacy Viewer feature (the Legacy Viewer has no such
//! status); it travels as a ViewerEffect of Aurora's own type
//! (`aurora_net::avatar_status`).
//!
//! What may be shared is narrow on purpose. Never a window title nor a
//! process name: only an id of the table below (programs the resident
//! ticked), the "Artist - Title" of the track that plays when the music
//! switch is on, and the text the resident typed. Nothing is read from the
//! machine until the master switch is on (`collect`), and everything is
//! shown to the resident beforehand as the line others will read.
//!
//! - `sender`: when a packet goes out (on change, heartbeat, arrivals).
//! - `collect`: the music that plays and the programs that run, on a thread
//!   of their own.
//! - the plate's chip (`ui/nameplate/status_chip.rs`) draws `line`.

pub mod collect;
pub mod sender;

use crate::settings::status_share::StatusShare;
use aurora_net::avatar_status::{AvatarStatus, Category};

/// A program the resident may tick in the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Program {
    /// Sent on the wire; never reused for another program.
    pub id: u16,
    pub name: &'static str,
    /// Executable names, any case; ".exe" is optional.
    pub exes: &'static [&'static str],
    pub game: bool,
}

/// The programs a status can name. A receiver draws a name and an icon only
/// for an id it finds here, so the text on the wire is never a program's.
pub const PROGRAMS: &[Program] = &[
    Program {
        id: 1,
        name: "Blender",
        exes: &["blender.exe"],
        game: false,
    },
    Program {
        id: 2,
        name: "Substance Painter",
        exes: &["Adobe Substance 3D Painter.exe", "Substance Painter.exe"],
        game: false,
    },
    Program {
        id: 3,
        name: "Visual Studio Code",
        exes: &["Code.exe"],
        game: false,
    },
    Program {
        id: 4,
        name: "Visual Studio",
        exes: &["devenv.exe"],
        game: false,
    },
    Program {
        id: 5,
        name: "Photoshop",
        exes: &["Photoshop.exe"],
        game: false,
    },
    Program {
        id: 6,
        name: "Krita",
        exes: &["krita.exe"],
        game: false,
    },
    Program {
        id: 100,
        name: "Elden Ring",
        exes: &["eldenring.exe"],
        game: true,
    },
    Program {
        id: 101,
        name: "Counter-Strike 2",
        exes: &["cs2.exe"],
        game: true,
    },
    Program {
        id: 102,
        name: "Baldur's Gate 3",
        exes: &["bg3.exe", "bg3_dx11.exe"],
        game: true,
    },
    Program {
        id: 103,
        name: "Stardew Valley",
        exes: &["Stardew Valley.exe"],
        game: true,
    },
    Program {
        id: 104,
        name: "Cyberpunk 2077",
        exes: &["Cyberpunk2077.exe"],
        game: true,
    },
    Program {
        id: 105,
        name: "Fortnite",
        exes: &["FortniteClient-Win64-Shipping.exe"],
        game: true,
    },
];

pub fn program(id: u16) -> Option<&'static Program> {
    PROGRAMS.iter().find(|p| p.id == id)
}

fn stem(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_owned()
}

/// Whether the process `process` is `exe` of the table. Linux cuts a process
/// name to 15 characters (/proc/<pid>/comm): such a name matches the start
/// of the table's.
pub fn is_process(exe: &str, process: &str) -> bool {
    let (exe, process) = (stem(exe), stem(process));
    if exe.is_empty() || process.is_empty() {
        return false;
    }
    exe == process || (process.chars().count() == 15 && exe.starts_with(&process))
}

/// The ids of the programs of the table that `names` (the running
/// processes) contain, in the table's order.
pub fn running(names: &[String]) -> Vec<u16> {
    PROGRAMS
        .iter()
        .filter(|p| p.exes.iter().any(|exe| names.iter().any(|n| is_process(exe, n))))
        .map(|p| p.id)
        .collect()
}

/// The track that plays.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Track {
    pub artist: String,
    pub title: String,
}

/// What the machine says right now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Inputs {
    pub music: Option<Track>,
    /// Ids of the programs of the table that run.
    pub programs: Vec<u16>,
    /// We are in do not disturb: nothing is shared if the settings say so.
    pub busy: bool,
    /// We are invisible: nothing is ever shared.
    pub invisible: bool,
}

/// What we would tell the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub category: Category,
    pub program: u16,
    pub detail: String,
}

/// "Artist - Title", or the title alone; empty when the track has none.
pub fn track_text(t: &Track) -> String {
    let (artist, title) = (t.artist.trim(), t.title.trim());
    match (artist.is_empty(), title.is_empty()) {
        (_, true) => String::new(),
        (true, false) => title.to_owned(),
        (false, false) => format!("{artist} - {title}"),
    }
}

/// What to share, by the settings and what the machine says: the text the
/// resident wrote first, else the music, else the first ticked program that
/// runs. None when nothing is on, while invisible, or while busy if asked.
pub fn desired(cfg: &StatusShare, inputs: &Inputs) -> Option<Payload> {
    if !cfg.share || inputs.invisible || (cfg.hide_when_busy && inputs.busy) {
        return None;
    }
    if cfg.custom {
        let text = aurora_net::avatar_status::clean_detail(&cfg.custom_text);
        if !text.is_empty() {
            return Some(Payload {
                category: Category::Custom,
                program: 0,
                detail: text,
            });
        }
    }
    if cfg.music
        && let Some(track) = &inputs.music
    {
        let detail = if cfg.music_detail {
            aurora_net::avatar_status::clean_detail(&track_text(track))
        } else {
            String::new()
        };
        return Some(Payload {
            category: Category::Music,
            program: 0,
            detail,
        });
    }
    if cfg.programs
        && let Some(p) = PROGRAMS.iter().find(|p| cfg.allows(p.id) && inputs.programs.contains(&p.id))
    {
        let category = if p.game { Category::Game } else { Category::Program };
        return Some(Payload {
            category,
            program: p.id,
            detail: String::new(),
        });
    }
    None
}

/// What a chip shows.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusLine {
    /// A Tabler icon of `assets/tabler-icons/filled`, as the icon cache
    /// names it.
    pub icon: &'static str,
    pub text: String,
    /// 0..1: how visible, falling to 0 as the status ends.
    pub fade: f32,
}

/// The chip of a status received or ours; None for a status that says
/// nothing (paused, or cleared).
pub fn line(status: &AvatarStatus, fade: f32) -> Option<StatusLine> {
    use aurora_i18n::tr;
    if status.clear || status.paused {
        return None;
    }
    let (icon, text) = match status.category {
        Category::None => return None,
        Category::Music => (
            "headphones-filled",
            if status.detail.is_empty() {
                tr!("status-share-music-generic").to_owned()
            } else {
                tr!("status-share-music", detail = status.detail.as_str())
            },
        ),
        Category::Custom => {
            if status.detail.is_empty() {
                return None;
            }
            ("message-circle-filled", status.detail.clone())
        }
        Category::Program | Category::Game => {
            let game = status.category == Category::Game;
            // only a program the table knows is named; anything else is
            // the generic wording, whatever the packet claims
            match program(status.program) {
                Some(p) if p.game => ("device-gamepad-filled", tr!("status-share-game", name = p.name)),
                Some(p) => ("app-window-filled", tr!("status-share-program", name = p.name)),
                None if game => ("device-gamepad-filled", tr!("status-share-game-generic").to_owned()),
                None => ("app-window-filled", tr!("status-share-program-generic").to_owned()),
            }
        }
    };
    Some(StatusLine { icon, text, fade })
}

/// The packet for a payload, with its sequence number.
pub fn packet(payload: &Payload, seq: u8) -> AvatarStatus {
    AvatarStatus {
        seq,
        clear: false,
        paused: false,
        category: payload.category,
        program: payload.program,
        detail: payload.detail.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> StatusShare {
        StatusShare {
            share: true,
            music: true,
            programs: true,
            custom: true,
            custom_text: "Building".into(),
            programs_allowed: vec![1, 100],
            ..StatusShare::default()
        }
    }

    fn inputs() -> Inputs {
        Inputs {
            music: Some(Track {
                artist: "Daft Punk".into(),
                title: "Veridis Quo".into(),
            }),
            programs: vec![1, 100, 3],
            busy: false,
            invisible: false,
        }
    }

    #[test]
    fn the_table_has_unique_ids_and_names() {
        for (i, a) in PROGRAMS.iter().enumerate() {
            assert!(a.id != 0 && !a.exes.is_empty());
            assert!(PROGRAMS[i + 1..].iter().all(|b| b.id != a.id), "id {}", a.id);
        }
    }

    #[test]
    fn processes_match_by_name_without_regard_to_case_or_extension() {
        assert!(is_process("Blender.exe", "blender"));
        assert!(is_process("blender.exe", "BLENDER.EXE"));
        assert!(!is_process("blender.exe", "blender-launcher.exe"));
        assert!(!is_process("", ""));
        // Linux cuts a name to 15 characters
        assert!(is_process("Adobe Substance 3D Painter.exe", "adobe substance"));
        assert!(!is_process("Adobe Substance 3D Painter.exe", "adobe"));
        assert_eq!(
            running(&["explorer.exe".into(), "BLENDER.EXE".into(), "cs2.exe".into()]),
            vec![1, 101]
        );
    }

    #[test]
    fn nothing_is_shared_with_the_master_switch_off_while_busy_or_invisible() {
        let mut cfg = on();
        cfg.share = false;
        assert_eq!(desired(&cfg, &inputs()), None);
        let mut cfg = on();
        let busy = Inputs { busy: true, ..inputs() };
        assert_eq!(desired(&cfg, &busy), None);
        cfg.hide_when_busy = false;
        assert!(desired(&cfg, &busy).is_some());
        // invisible: never, whatever the setting says
        let hidden = Inputs {
            invisible: true,
            ..inputs()
        };
        assert_eq!(desired(&cfg, &hidden), None);
    }

    #[test]
    fn the_text_wins_over_the_music_and_the_music_over_a_program() {
        let mut cfg = on();
        assert_eq!(
            desired(&cfg, &inputs()).map(|p| (p.category, p.detail)),
            Some((Category::Custom, "Building".into()))
        );
        cfg.custom = false;
        let p = desired(&cfg, &inputs()).expect("music");
        assert_eq!((p.category, p.detail.as_str()), (Category::Music, "Daft Punk - Veridis Quo"));
        cfg.music_detail = false;
        assert_eq!(desired(&cfg, &inputs()).map(|p| p.detail), Some(String::new()));
        cfg.music = false;
        let p = desired(&cfg, &inputs()).expect("program");
        assert_eq!((p.category, p.program), (Category::Program, 1));
    }

    #[test]
    fn only_a_ticked_program_that_runs_is_shared() {
        let mut cfg = on();
        cfg.custom = false;
        cfg.music = false;
        // Visual Studio Code runs but is not ticked; the game is
        cfg.programs_allowed = vec![100];
        let p = desired(&cfg, &inputs()).expect("game");
        assert_eq!((p.category, p.program), (Category::Game, 100));
        cfg.programs_allowed = vec![4];
        assert_eq!(desired(&cfg, &inputs()), None);
        cfg.programs_allowed.clear();
        assert_eq!(desired(&cfg, &inputs()), None);
    }

    #[test]
    fn an_empty_text_or_track_shares_nothing() {
        let mut cfg = on();
        cfg.custom_text = " \u{202E} ".into();
        cfg.music = false;
        cfg.programs = false;
        assert_eq!(desired(&cfg, &inputs()), None);
        assert_eq!(
            track_text(&Track {
                artist: " A ".into(),
                title: "".into()
            }),
            ""
        );
        assert_eq!(
            track_text(&Track {
                artist: "".into(),
                title: "T".into()
            }),
            "T"
        );
    }

    #[test]
    fn a_chip_names_only_the_programs_of_the_table() {
        let status = |category, program, detail: &str| AvatarStatus {
            seq: 1,
            clear: false,
            paused: false,
            category,
            program,
            detail: detail.into(),
        };
        let l = line(&status(Category::Program, 1, ""), 1.0).expect("known");
        assert!(l.text.contains("Blender") && l.icon == "app-window-filled");
        // an id of the table wins over the category the packet claims
        let l = line(&status(Category::Program, 100, ""), 1.0).expect("game");
        assert!(l.text.contains("Elden Ring") && l.icon == "device-gamepad-filled");
        // an unknown id never shows what the packet says
        let l = line(&status(Category::Program, 999, "Evil http://x"), 1.0).expect("generic");
        assert!(!l.text.contains("Evil"));
        let l = line(&status(Category::Music, 0, "A - B"), 0.5).expect("music");
        assert!(l.text.contains("A - B") && l.fade == 0.5);
        assert!(line(&status(Category::Custom, 0, ""), 1.0).is_none());
        assert!(line(&AvatarStatus::cleared(1), 1.0).is_none());
        assert!(
            line(
                &AvatarStatus {
                    paused: true,
                    ..status(Category::Music, 0, "x")
                },
                1.0
            )
            .is_none()
        );
    }
}
