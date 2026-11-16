//! The camera mode, said in words (`camera/mode_status.rs` decides which):
//! the pill that shows for a moment when the mode changes ("Mouselook · Esc
//! to leave"), the chip that stays for the modes the user has to leave, and
//! the note in the Controls window's header. All text goes through
//! `aurora_i18n::tr!`; the surface is `widgets::overlay`, the colours the
//! `Palette`'s. Both are elided with the whole text on hover, so a large
//! text size or a long translation never clips.

use crate::camera::mode_status::Status;
use crate::theme::Palette;
use crate::ui::widgets;
use egui::{Align2, Margin, RichText, Sense};

/// The keys that leave a mode, as the user's bindings say; none when the
/// action is unbound.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keys {
    /// Reset camera (Esc by default): leaves mouselook and the flycam.
    pub reset: String,
    /// Camera Lock, when bound.
    pub lock: Option<String>,
    /// Play / stop the Camera Path, when bound.
    pub path: Option<String>,
}

pub fn name(status: Status) -> &'static str {
    match status {
        Status::Director => aurora_i18n::tr!("camera-mode-director"),
        Status::PathPlaying => aurora_i18n::tr!("camera-mode-path-playing"),
        Status::PathRecording => aurora_i18n::tr!("camera-mode-path-recording"),
        Status::Locked => aurora_i18n::tr!("camera-mode-locked"),
        Status::Flycam => aurora_i18n::tr!("camera-mode-flycam"),
        Status::Free => aurora_i18n::tr!("camera-mode-free"),
        Status::Mouselook => aurora_i18n::tr!("camera-mode-mouselook"),
        Status::ScriptedFollow => aurora_i18n::tr!("camera-mode-follow"),
        Status::Seated => aurora_i18n::tr!("camera-mode-seated"),
        Status::ObjectView => aurora_i18n::tr!("camera-mode-object-view"),
    }
}

/// One icon per mode, the one the function already has elsewhere
/// (Director's `movie`, the path's `route`, the lock's `lock-filled`,
/// mouselook's `eye-filled`, the free camera's and the flycam's
/// `video-filled`).
pub fn icon(status: Option<Status>) -> &'static str {
    match status {
        Some(Status::Director) => "movie",
        Some(Status::PathPlaying) => "route",
        Some(Status::PathRecording) => "player-record-filled",
        Some(Status::Locked) => "lock-filled",
        Some(Status::Flycam | Status::Free) => "video-filled",
        Some(Status::Mouselook) => "eye-filled",
        Some(Status::ScriptedFollow) => "focus-centered",
        Some(Status::Seated) => "armchair",
        Some(Status::ObjectView) => "box",
        None => "camera-filled",
    }
}

/// How to leave a mode, where the viewer knows a way that works from every
/// state it can be in: the key of Reset camera leaves mouselook and the
/// flycam (`App::reset_camera_view`), the free camera has its Back button,
/// the lock and the path their key or their window. A script's camera, a
/// seat's and the object view have none worth naming.
pub fn how_to_leave(status: Status, keys: &Keys) -> Option<String> {
    match status {
        Status::Mouselook | Status::Flycam => Some(aurora_i18n::tr!("camera-mode-leave-key", key = keys.reset.clone())),
        Status::Free => Some(aurora_i18n::tr!("camera-mode-leave-back").to_owned()),
        Status::Locked => Some(match &keys.lock {
            Some(key) => aurora_i18n::tr!("camera-mode-leave-key", key = key.clone()),
            None => aurora_i18n::tr!("camera-mode-leave-lock-menu").to_owned(),
        }),
        Status::PathPlaying => Some(match &keys.path {
            Some(key) => aurora_i18n::tr!("camera-mode-leave-key", key = key.clone()),
            None => aurora_i18n::tr!("camera-mode-leave-path-window").to_owned(),
        }),
        Status::PathRecording => Some(aurora_i18n::tr!("camera-mode-leave-path-window").to_owned()),
        Status::Director => Some(aurora_i18n::tr!("camera-mode-leave-director").to_owned()),
        Status::ScriptedFollow | Status::Seated | Status::ObjectView => None,
    }
}

/// "Mouselook · Esc to leave", or the name alone.
pub fn line(name: &str, how: Option<&str>) -> String {
    match how {
        Some(how) => aurora_i18n::tr!("camera-mode-line", name = name.to_owned(), how = how.to_owned()),
        None => name.to_owned(),
    }
}

/// The text of the pill for a mode, or for the return to the normal view.
pub fn pill_text(status: Option<Status>, keys: &Keys) -> String {
    match status {
        Some(s) => line(name(s), how_to_leave(s, keys).as_deref()),
        None => aurora_i18n::tr!("camera-mode-normal").to_owned(),
    }
}

/// What a chip does when clicked, for its tooltip; none for the modes the
/// chip only names.
pub fn click_leaves(status: Status) -> bool {
    matches!(status, Status::PathPlaying | Status::PathRecording | Status::Locked | Status::Free)
}

/// What `show` draws.
pub struct Badge<'a> {
    pub id: &'static str,
    pub icon: &'a str,
    pub text: &'a str,
    /// A click leaves the mode (a chip); a pill takes no click.
    pub clickable: bool,
    /// The hover text of a clickable chip: the whole text and what a click does.
    pub tip: &'a str,
}

/// The pill or the chip: icon and text on the windows' glass, at the top
/// centre of `room`. True when it was clicked. The text is cut at 70 % of
/// the room, its whole on hover.
pub fn show(ctx: &egui::Context, p: &Palette, room: egui::Rect, badge: &Badge) -> bool {
    let max = (room.width() * 0.7).max(180.0);
    let mut clicked = false;
    egui::Area::new(egui::Id::new(badge.id))
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(egui::pos2(room.center().x, room.top() + 12.0))
        .interactable(badge.clickable)
        .show(ctx, |ui| {
            widgets::overlay(ui, p, Margin::symmetric(12, 6), |ui| {
                ui.set_max_width(max);
                ui.horizontal(|ui| {
                    widgets::glyph(ui, badge.icon, p.violet_light, 15.0);
                    let label = widgets::elided_label(ui, RichText::new(badge.text).size(13.0).color(p.ink), (max - 48.0).max(60.0));
                    if badge.clickable {
                        let response = ui
                            .add(label.sense(Sense::click()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(badge.tip);
                        clicked = response.clicked();
                    } else {
                        ui.add(label);
                    }
                });
            });
        });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Keys {
        Keys {
            reset: "Esc".into(),
            lock: Some("Alt+Shift+L".into()),
            path: None,
        }
    }

    #[test]
    fn a_mode_with_a_bound_key_names_the_key() {
        let text = pill_text(Some(Status::Mouselook), &keys());
        assert!(text.contains(name(Status::Mouselook)), "{text}");
        assert!(text.contains("Esc"), "{text}");
        let lock = pill_text(Some(Status::Locked), &keys());
        assert!(lock.contains("Alt+Shift+L"), "{lock}");
    }

    #[test]
    fn an_unbound_key_falls_back_to_the_menu_or_the_window() {
        let none = Keys {
            reset: "Esc".into(),
            lock: None,
            path: None,
        };
        let lock = pill_text(Some(Status::Locked), &none);
        assert!(lock.contains(aurora_i18n::tr!("camera-mode-leave-lock-menu")), "{lock}");
        let path = pill_text(Some(Status::PathPlaying), &none);
        assert!(path.contains(aurora_i18n::tr!("camera-mode-leave-path-window")), "{path}");
    }

    #[test]
    fn modes_with_no_way_out_to_name_show_their_name_alone() {
        for s in [Status::ScriptedFollow, Status::Seated, Status::ObjectView] {
            assert_eq!(pill_text(Some(s), &keys()), name(s));
        }
    }

    #[test]
    fn the_return_to_the_plain_view_has_its_own_text() {
        assert_eq!(pill_text(None, &keys()), aurora_i18n::tr!("camera-mode-normal"));
    }

    #[test]
    fn every_mode_has_a_name_an_icon_and_the_held_ones_are_named_apart() {
        let all = [
            Status::Director,
            Status::PathPlaying,
            Status::PathRecording,
            Status::Locked,
            Status::Flycam,
            Status::Free,
            Status::Mouselook,
            Status::ScriptedFollow,
            Status::Seated,
            Status::ObjectView,
        ];
        let mut names: Vec<&str> = all.iter().map(|s| name(*s)).collect();
        assert!(names.iter().all(|n| !n.is_empty()));
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len(), "two modes with one name");
        assert!(all.iter().all(|s| !icon(Some(*s)).is_empty()));
        assert!(click_leaves(Status::Locked) && !click_leaves(Status::Director));
    }
}
