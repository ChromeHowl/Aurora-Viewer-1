//! `AURORA_DEMO_PHOTO_CAMERA=<what>[:down=<n>][:turn=<n>]`, with the
//! viewfinder open (`AURORA_DEMO_SNAPSHOT=viewfinder`): the photo camera's
//! quick menu, mode list or one of its sheets opens at frame 600, then the
//! arrow keys go through the viewfinder's own key path (`viewfinder_key`):
//! `down` presses of Down from frame 610 (Right in the quick menu), then
//! `turn` presses of Right (Left when negative) on the row reached. `follow`
//! shows the camera pad with the right hand followed (in the demo's own
//! settings, never the user's): with `AURORA_UI_FONT=OpenDyslexic` the
//! Follow rows' labels must stay inside their cells. The log gives what is
//! open and the row the keys are on at frame 700.

use super::App;
use crate::ui::viewfinder::Sheet;
use winit::keyboard::{Key, KeyCode, NamedKey};

/// What the scenario opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    Menu,
    Sheet(Sheet),
    /// The Guide sheet with the spiral (its corner row shows).
    GuideSpiral,
    /// The camera pad, the right hand followed.
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Scenario {
    open: Open,
    down: u32,
    turn: i32,
}

/// The first frame of the scenario, and of its keys.
const OPEN_AT: u64 = 600;
const KEYS_AT: u64 = 610;
/// The keys and the opened part are logged then.
const LOG_AT: u64 = 700;

fn parse(value: &str) -> Option<Scenario> {
    let mut parts = value.trim().split(':');
    let open = match parts.next()? {
        "menu" => Open::Menu,
        "modes" => Open::Sheet(Sheet::Modes),
        "style" => Open::Sheet(Sheet::Style),
        "guide" => Open::Sheet(Sheet::Guide),
        "guide-spiral" => Open::GuideSpiral,
        "lens" => Open::Sheet(Sheet::Lens),
        "bank" => Open::Sheet(Sheet::Bank(0)),
        "follow" => Open::Follow,
        _ => return None,
    };
    let mut scenario = Scenario { open, down: 0, turn: 0 };
    for part in parts {
        match part.split_once('=') {
            Some(("down", n)) => scenario.down = n.parse().ok()?.min(20),
            Some(("turn", n)) => scenario.turn = n.parse::<i32>().ok()?.clamp(-20, 20),
            _ => return None,
        }
    }
    Some(scenario)
}

/// The key pressed at `frame`, one every other frame (the press, then its
/// release drawn): `down` moves, then `turn` turns.
fn key_at(s: &Scenario, frame: u64) -> Option<KeyCode> {
    let step = frame.checked_sub(KEYS_AT)?;
    if step % 2 != 0 {
        return None;
    }
    let i = step / 2;
    let down = u64::from(s.down);
    if i < down {
        // (the quick menu's tiles go right; Down jumps a row of nine)
        return Some(if s.open == Open::Menu {
            KeyCode::ArrowRight
        } else {
            KeyCode::ArrowDown
        });
    }
    (i - down < u64::from(s.turn.unsigned_abs())).then_some(if s.turn > 0 { KeyCode::ArrowRight } else { KeyCode::ArrowLeft })
}

fn named(code: KeyCode) -> NamedKey {
    match code {
        KeyCode::ArrowRight => NamedKey::ArrowRight,
        KeyCode::ArrowLeft => NamedKey::ArrowLeft,
        _ => NamedKey::ArrowDown,
    }
}

impl App {
    pub(super) fn photo_camera_demo(&mut self) {
        if !self.demo || !(OPEN_AT..=LOG_AT).contains(&self.frame_count) || !self.viewfinder_on() {
            return;
        }
        let Some(s) = std::env::var("AURORA_DEMO_PHOTO_CAMERA").ok().as_deref().and_then(parse) else {
            return;
        };
        if self.frame_count == OPEN_AT {
            let mut out = Vec::new();
            let vf = &mut self.snapshots.vf;
            match s.open {
                Open::Menu => vf.menu = true,
                Open::Sheet(sheet) => vf.open_sheet(sheet, &mut out),
                Open::GuideSpiral => {
                    self.settings.snapshot.guide = crate::snapshot::Guide::Spiral;
                    vf.open_sheet(Sheet::Guide, &mut out);
                }
                Open::Follow => {
                    self.settings.snapshot.viewfinder.pad = true;
                    self.settings.camera.extra.follow_joint = crate::camera::extra::FollowJoint::RightHand;
                }
            }
            log::info!("photo camera demo: {:?} open", s.open);
        }
        if let Some(code) = key_at(&s, self.frame_count) {
            let key = Key::Named(named(code));
            self.viewfinder_key(code, &key, true, false);
            self.viewfinder_key(code, &key, false, false);
        }
        if self.frame_count == LOG_AT {
            let vf = &self.snapshots.vf;
            log::info!(
                "photo camera demo: menu {} (tile {}), sheet {:?} (row {}), mode row {}",
                vf.menu,
                vf.tile_cursor,
                vf.sheet,
                vf.sheet_row,
                vf.mode_cursor
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scenario_reads_what_to_open_and_the_keys() {
        assert_eq!(
            parse("guide:down=2:turn=-1"),
            Some(Scenario {
                open: Open::Sheet(Sheet::Guide),
                down: 2,
                turn: -1
            })
        );
        assert_eq!(parse("bank").map(|s| s.open), Some(Open::Sheet(Sheet::Bank(0))));
        assert_eq!(parse("follow").map(|s| s.open), Some(Open::Follow));
        assert_eq!(parse("menu:down=99").map(|s| s.down), Some(20), "a bounded walk");
        assert_eq!(parse("nothing"), None);
        assert_eq!(parse("lens:up=1"), None);
    }

    #[test]
    fn the_keys_come_one_every_other_frame_moves_first() {
        let s = parse("lens:down=1:turn=-2").expect("parses");
        let keys: Vec<(u64, KeyCode)> = (OPEN_AT..=LOG_AT).filter_map(|f| key_at(&s, f).map(|k| (f, k))).collect();
        assert_eq!(
            keys,
            vec![
                (KEYS_AT, KeyCode::ArrowDown),
                (KEYS_AT + 2, KeyCode::ArrowLeft),
                (KEYS_AT + 4, KeyCode::ArrowLeft)
            ]
        );
        let menu = parse("menu:down=1").expect("parses");
        assert_eq!(key_at(&menu, KEYS_AT), Some(KeyCode::ArrowRight), "the tiles go right");
    }
}
