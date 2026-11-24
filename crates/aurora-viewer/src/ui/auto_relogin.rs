//! The countdown of an automatic reconnection, over the login screen: what
//! happened, when the next attempt is made, and the three ways out
//! (`auto_relogin.rs` for the rules, `app/auto_relogin.rs` for the login).

use super::widgets;
use crate::auto_relogin::Cause;
use crate::theme::Palette;
use egui::{Id, RichText};

/// What the card shows.
pub struct View<'a> {
    pub cause: Cause,
    /// Where the login goes back to (the region's name), when known.
    pub region: Option<&'a str>,
    /// Seconds before the attempt, and what is left of this wait (0..1).
    pub seconds: u64,
    pub fraction: f32,
    /// The number of the attempt counted down to, from 1.
    pub attempt: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Stop the attempts; "Reconnect" stays on the login screen.
    Cancel,
    TryNow,
    /// Stop the attempts and forget the place: the usual login screen.
    LoginScreen,
}

/// The seconds left, rounded up: a count never shows 0 before it acts.
pub fn seconds_left(left: std::time::Duration) -> u64 {
    left.as_secs() + u64::from(left.subsec_nanos() > 0)
}

pub fn show(ctx: &egui::Context, p: &Palette, v: &View<'_>) -> Option<Action> {
    let mut action = None;
    let response = widgets::Modal::new(Id::unique("auto_relogin")).login_surface(p).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            widgets::glyph(ui, "wifi-off", p.amber, 18.0);
            ui.label(
                RichText::new(aurora_i18n::tr!("stay-online-relogin-title"))
                    .size(14.0)
                    .strong()
                    .color(p.ink),
            );
        });
        ui.add_space(6.0);
        let what = match v.cause {
            Cause::LinkLost => aurora_i18n::tr!("stay-online-relogin-lost"),
            Cause::RegionRestart => aurora_i18n::tr!("stay-online-relogin-restart"),
            Cause::NoAnswer => aurora_i18n::tr!("stay-online-relogin-no-answer"),
        };
        ui.label(RichText::new(what).size(12.5).color(p.muted));
        if let Some(region) = v.region {
            ui.label(
                RichText::new(aurora_i18n::tr!("stay-online-relogin-place", region = region))
                    .size(12.5)
                    .color(p.muted),
            );
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new(aurora_i18n::tr!(
                "stay-online-relogin-count",
                attempt = i64::from(v.attempt),
                seconds = v.seconds as i64
            ))
            .size(13.0)
            .color(p.ink),
        );
        ui.add_space(6.0);
        let width = ui.available_width();
        widgets::countdown_bar(ui, p, "auto_relogin_bar", v.fraction, width, p.amber);
        ui.add_space(14.0);
        let buttons = [
            widgets::DialogButton {
                icon: "",
                label: aurora_i18n::tr!("stay-online-relogin-login-screen"),
                kind: widgets::ActionKind::Flat,
                enabled: true,
            },
            widgets::DialogButton {
                icon: "",
                label: aurora_i18n::tr!("common-cancel"),
                kind: widgets::ActionKind::Flat,
                enabled: true,
            },
            widgets::DialogButton {
                icon: "refresh",
                label: aurora_i18n::tr!("stay-online-relogin-try-now"),
                kind: widgets::ActionKind::Primary,
                enabled: true,
            },
        ];
        action = match widgets::dialog_buttons(ui, p, &buttons, 28.0) {
            Some(0) => Some(Action::LoginScreen),
            Some(1) => Some(Action::Cancel),
            Some(_) => Some(Action::TryNow),
            None => None,
        };
    });
    // Escape, a click outside: the attempts stop, nothing else changes
    if action.is_none() && response.should_close() {
        action = Some(Action::Cancel);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_count_rounds_up() {
        assert_eq!(seconds_left(Duration::from_millis(29_400)), 30);
        assert_eq!(seconds_left(Duration::from_secs(30)), 30);
        assert_eq!(seconds_left(Duration::from_millis(1)), 1);
        assert_eq!(seconds_left(Duration::ZERO), 0);
    }
}
