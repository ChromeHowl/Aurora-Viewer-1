//! The viewfinder's key list: the keys that do something in the shot mode
//! now chosen (Photo and its variants, Path), as keycaps (`widgets::keycap`,
//! the same caps as the shortcuts list), in a sheet on the right (the K key,
//! or the quick menu's Keys tile). It follows the mode while it is open, and
//! names the arrow keys of the menu, the mode list and the sheets. The rows are pure
//! (`rows`); the keys themselves are those `app::viewfinder::viewfinder_key`
//! handles.

use super::sheets::sheet;
use super::{Viewfinder, regular};
use crate::keybinds::{Binding, Input};
use crate::snapshot::viewfinder::ShotChoice;
use crate::theme::Palette;
use crate::ui::widgets::keycap;
use egui::{Align2, Rect, Sense, Vec2};
use winit::keyboard::KeyCode;

/// How a row's keys read together: either of them, or from the first to the
/// last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    Or,
    To,
}

/// A line of the list: its keys and what they do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRow {
    pub keys: Vec<Binding>,
    pub join: Join,
    pub text: &'static str,
}

/// What decides the rows.
pub struct KeyContext<'a> {
    /// The mode now chosen.
    pub mode: ShotChoice,
    /// The quick menu's keys, as the viewfinder names them ("Q / Tab" or
    /// "Tab" when a shortcut of the viewer has Q).
    pub menu: &'a str,
    /// The first person view's key, when the viewer allows it.
    pub first_person: Option<Binding>,
}

pub fn key(code: KeyCode) -> Binding {
    Binding {
        input: Input::key(code),
        ctrl: false,
        shift: false,
        alt: false,
    }
}

/// The Path mode's keys: the shutter places a waypoint, and the photo's
/// focus, exposure, white balance, style and focal keys do nothing there.
pub fn is_path(mode: ShotChoice) -> bool {
    mode == ShotChoice::Path
}

/// The rows for `cx`.
pub fn rows(cx: &KeyContext) -> Vec<KeyRow> {
    let path = is_path(cx.mode);
    let mut v = Vec::new();
    let mut row = |keys: Vec<Binding>, join: Join, text: &'static str| v.push(KeyRow { keys, join, text });
    row(
        vec![key(KeyCode::Enter)],
        Join::Or,
        if path {
            aurora_i18n::tr!("vfx-key-waypoint")
        } else {
            aurora_i18n::tr!("vfx-key-shutter")
        },
    );
    if !path {
        row(vec![key(KeyCode::KeyN)], Join::Or, aurora_i18n::tr!("vfx-key-af"));
        row(vec![key(KeyCode::KeyV)], Join::Or, aurora_i18n::tr!("vfx-key-area"));
        row(vec![key(KeyCode::KeyL)], Join::Or, aurora_i18n::tr!("vfx-key-ael"));
        row(vec![key(KeyCode::KeyB)], Join::Or, aurora_i18n::tr!("vfx-key-wb"));
        row(vec![key(KeyCode::KeyT)], Join::Or, aurora_i18n::tr!("vfx-key-style"));
        row(
            vec![key(KeyCode::Digit1), key(KeyCode::Digit5)],
            Join::To,
            aurora_i18n::tr!("vfx-key-focal"),
        );
    }
    row(vec![key(KeyCode::KeyP)], Join::Or, aurora_i18n::tr!("vfx-key-mode"));
    row(menu_keys(cx.menu), Join::Or, aurora_i18n::tr!("vfx-key-menu"));
    row(vec![key(KeyCode::KeyG)], Join::Or, aurora_i18n::tr!("vfx-key-guide"));
    row(vec![key(KeyCode::KeyI)], Join::Or, aurora_i18n::tr!("vfx-key-display"));
    if let Some(b) = &cx.first_person {
        row(vec![b.clone()], Join::Or, aurora_i18n::tr!("vfx-key-first-person"));
    }
    row(
        vec![
            key(KeyCode::ArrowUp),
            key(KeyCode::ArrowDown),
            key(KeyCode::ArrowLeft),
            key(KeyCode::ArrowRight),
        ],
        Join::Or,
        aurora_i18n::tr!("vfx-key-arrows"),
    );
    row(vec![key(KeyCode::KeyK)], Join::Or, aurora_i18n::tr!("vfx-key-keys"));
    row(
        vec![key(KeyCode::KeyH), key(KeyCode::F1)],
        Join::Or,
        aurora_i18n::tr!("vfx-key-help"),
    );
    row(vec![key(KeyCode::Escape)], Join::Or, aurora_i18n::tr!("vfx-key-escape"));
    v
}

/// The menu's keys from the label the viewfinder names them by.
fn menu_keys(label: &str) -> Vec<Binding> {
    if label.split('/').any(|k| k.trim() == "Q") {
        vec![key(KeyCode::KeyQ), key(KeyCode::Tab)]
    } else {
        vec![key(KeyCode::Tab)]
    }
}

/// The mode's name over the list.
fn mode_name(mode: ShotChoice) -> String {
    super::modes::label(mode).0
}

/// The caps of a row's keys and the width they take with the signs between.
fn lay(ui: &egui::Ui, p: &Palette, row: &KeyRow) -> (Vec<Vec<keycap::Keycap>>, f32) {
    let caps: Vec<Vec<keycap::Keycap>> = row.keys.iter().map(|b| keycap::caps(ui, b, p.panel)).collect();
    let signs = caps.len().saturating_sub(1) as f32 * SIGN;
    let width = caps.iter().map(|c| keycap::row_width(c)).sum::<f32>() + signs;
    (caps, width)
}

/// Room for the "/" or "–" between two keys.
const SIGN: f32 = 14.0;
/// A line's height, and what follows the keys.
const LINE: f32 = 22.0;
const AFTER_KEYS: f32 = 10.0;

impl Viewfinder {
    /// The keys of the mode now chosen.
    pub(super) fn keys_sheet(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        screen: Rect,
        mode: ShotChoice,
        menu: &str,
        first_person: Option<Binding>,
    ) {
        // (this sheet has no row of its own: the arrows only walk the others')
        self.sheet_nav(1);
        let list = rows(&KeyContext { mode, menu, first_person });
        let title = format!("{} · {}", aurora_i18n::tr!("vfx-keys-title"), mode_name(mode));
        let open = sheet(ctx, p, screen, "vf_keys_sheet", "keyboard-filled", &title, |ui| {
            let laid: Vec<_> = list.iter().map(|r| lay(ui, p, r)).collect();
            let column = laid.iter().map(|(_, w)| *w).fold(0.0, f32::max);
            ui.spacing_mut().item_spacing.y = 0.0;
            for (row, (caps, _)) in list.iter().zip(&laid) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(column + AFTER_KEYS, LINE), Sense::hover());
                    let mut x = rect.left();
                    let sign = match row.join {
                        Join::Or => "/",
                        Join::To => "–",
                    };
                    for (i, cap) in caps.iter().enumerate() {
                        if i > 0 {
                            ui.painter().text(
                                egui::pos2(x + SIGN / 2.0, rect.center().y),
                                Align2::CENTER_CENTER,
                                sign,
                                regular(12.0),
                                p.muted,
                            );
                            x += SIGN;
                        }
                        keycap::paint_row(ui, p, cap, egui::pos2(x, rect.center().y), p.panel);
                        x += keycap::row_width(cap);
                    }
                    ui.label(egui::RichText::new(row.text).color(p.ink));
                });
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new(aurora_i18n::tr!("vfx-keys-foot")).size(11.0).color(p.muted));
        });
        if !open {
            self.sheet = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx(mode: ShotChoice) -> KeyContext<'static> {
        KeyContext {
            mode,
            menu: "Q / Tab",
            first_person: None,
        }
    }

    fn has(rows: &[KeyRow], code: KeyCode) -> bool {
        rows.iter().any(|r| r.keys.contains(&key(code)))
    }

    #[test]
    fn the_photo_modes_list_the_photo_keys() {
        for mode in [
            ShotChoice::Photo,
            ShotChoice::FullSet,
            ShotChoice::Pano,
            ShotChoice::Depth,
            ShotChoice::Bank(0),
        ] {
            let r = rows(&cx(mode));
            for code in [
                KeyCode::KeyN,
                KeyCode::KeyV,
                KeyCode::KeyL,
                KeyCode::KeyB,
                KeyCode::KeyT,
                KeyCode::Digit1,
                KeyCode::Enter,
            ] {
                assert!(has(&r, code), "{mode:?} {code:?}");
            }
        }
    }

    #[test]
    fn the_path_mode_leaves_out_what_does_nothing_there() {
        let photo = rows(&cx(ShotChoice::Photo));
        let path = rows(&cx(ShotChoice::Path));
        for code in [
            KeyCode::KeyN,
            KeyCode::KeyV,
            KeyCode::KeyL,
            KeyCode::KeyB,
            KeyCode::KeyT,
            KeyCode::Digit1,
            KeyCode::Digit5,
        ] {
            assert!(!has(&path, code), "{code:?}");
        }
        for code in [KeyCode::Enter, KeyCode::KeyP, KeyCode::KeyK, KeyCode::Escape, KeyCode::KeyG] {
            assert!(has(&path, code), "{code:?}");
        }
        assert!(path.len() < photo.len());
        let enter = |rows: &[KeyRow]| rows.iter().find(|r| r.keys == vec![key(KeyCode::Enter)]).map(|r| r.text);
        assert_ne!(enter(&photo), enter(&path), "the shutter says what it does in each mode");
    }

    #[test]
    fn the_arrows_are_named_in_every_mode() {
        for mode in [ShotChoice::Photo, ShotChoice::Path] {
            let r = rows(&cx(mode));
            assert!(has(&r, KeyCode::ArrowUp) && has(&r, KeyCode::ArrowRight), "{mode:?}");
        }
    }

    #[test]
    fn the_menu_keys_follow_the_viewfinders_label() {
        assert_eq!(menu_keys("Q / Tab"), vec![key(KeyCode::KeyQ), key(KeyCode::Tab)]);
        assert_eq!(menu_keys("Tab"), vec![key(KeyCode::Tab)], "a shortcut of the viewer has Q");
    }

    #[test]
    fn the_first_person_key_is_listed_only_when_it_is_allowed() {
        let with = rows(&KeyContext {
            first_person: Some(key(KeyCode::KeyM)),
            ..cx(ShotChoice::Photo)
        });
        assert!(has(&with, KeyCode::KeyM));
        assert!(!has(&rows(&cx(ShotChoice::Photo)), KeyCode::KeyM));
    }

    #[test]
    fn no_key_is_listed_twice() {
        for mode in [ShotChoice::Photo, ShotChoice::Path] {
            let r = rows(&KeyContext {
                first_person: Some(key(KeyCode::KeyM)),
                ..cx(mode)
            });
            let mut all: Vec<&Binding> = r.iter().flat_map(|r| &r.keys).collect();
            let n = all.len();
            all.sort_by_key(|b| format!("{:?}", b.input));
            all.dedup();
            assert_eq!(all.len(), n, "{mode:?}");
        }
    }
}
