//! Preferences › Network › Proxy and port: the SOCKS 5 proxy's rows
//! (floater_preferences_proxy.xml: "Use SOCKS 5 Proxy for UDP traffic",
//! its host and port, the authentication and HttpProxyType "Socks";
//! `settings/socks_proxy.rs`). The user name and password leave the window
//! at once for the credential store (`socks_credentials.rs`).

use super::super::cx::{Cx, Nothing, field, fields};
use super::super::{COMBO_W, OptionsUi, SLIDER_W};
use crate::socks_credentials::State;
use crate::theme::Palette;
use crate::ui::widgets;
use aurora_i18n::tr;
use egui::RichText;

/// The rows, inside the proxy group.
pub fn rows(cx: &mut Cx) {
    let p = cx.p;
    cx.toggle(tr!("l4-socks"), tr!("l4-socks-hint"), &field!(socks_proxy.enabled));
    let off = !cx.s.socks_proxy.enabled;
    cx.greyed(off);
    cx.row(
        tr!("l4-socks-address"),
        tr!("l4-socks-address-hint"),
        &fields!(socks_proxy.host, socks_proxy.port),
        |ui, s| {
            let socks = &mut s.socks_proxy;
            let mut changed = ui
                .add(
                    egui::TextEdit::singleline(&mut socks.host)
                        .desired_width(SLIDER_W - 70.0)
                        .char_limit(255)
                        .hint_text("socks.example.org"),
                )
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut socks.port).range(1..=u16::MAX).speed(1.0))
                .changed();
            changed
        },
    );
    cx.toggle(tr!("l4-socks-http"), tr!("l4-socks-http-hint"), &field!(socks_proxy.http));
    cx.toggle(
        tr!("l4-socks-user-pass"),
        tr!("l4-socks-user-pass-hint"),
        &field!(socks_proxy.user_pass),
    );
    cx.greyed(off || !cx.s.socks_proxy.user_pass);
    cx.wide().row_with(
        tr!("l4-socks-credential"),
        tr!("l4-socks-credential-hint"),
        &Nothing,
        |ui, _, st, _| {
            credential(ui, &p, st);
            false
        },
    );
    cx.greyed(false);
}

/// The user name, the masked password, "Save" and "Forget", and where the
/// saved pair is.
fn credential(ui: &mut egui::Ui, p: &Palette, st: &mut OptionsUi) {
    if st.socks_state == State::Unknown {
        st.socks_probe = true;
    }
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut st.socks_user_typed)
                    .desired_width(COMBO_W / 2.0)
                    .char_limit(255)
                    .hint_text(tr!("l4-socks-username")),
            );
            ui.add(
                egui::TextEdit::singleline(&mut st.socks_pass_typed)
                    .password(true)
                    .desired_width(COMBO_W / 2.0)
                    .char_limit(255)
                    .hint_text(tr!("l4-socks-password")),
            );
            let usable = crate::socks_credentials::usable(&st.socks_user_typed, &st.socks_pass_typed);
            if ui
                .add_enabled_ui(usable.is_some(), |ui| widgets::flat_button(ui, p, tr!("common-save")))
                .inner
                .clicked()
            {
                st.socks_user_typed.clear();
                st.socks_pass_typed.clear();
                st.socks_edit = Some(usable);
            }
            let saved = matches!(st.socks_state, State::Stored | State::Session);
            if ui
                .add_enabled_ui(saved, |ui| widgets::flat_button(ui, p, tr!("l4-socks-forget")))
                .inner
                .clicked()
            {
                st.socks_edit = Some(None);
            }
        });
        let (text, color) = match st.socks_state {
            State::Unknown => (tr!("l4-socks-checking"), p.muted),
            State::None => (tr!("l4-socks-none"), p.muted),
            State::Stored => (tr!("l4-socks-stored"), p.muted),
            State::Session => (tr!("l4-socks-session"), p.danger),
        };
        ui.label(RichText::new(text).size(11.5).color(color));
    });
}
