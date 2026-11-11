//! The rows of `settings/legacy_prefs.rs`, each on the page where the
//! Legacy Viewer has it: the tooltip delay, the HUDs' size and the
//! simulator version (Layout; panel_preferences_UI.xml), the music's fade
//! (Sound; panel_preferences_sound.xml), the microphone released while muted
//! (Sound › Voice), scripts opening the map (Privacy), JavaScript, the proxy
//! and the UDP port (Network; panel_preferences_setup.xml,
//! floater_preferences_proxy.xml).

use super::super::SLIDER_W;
use super::super::cx::{Cx, field, fields};
use crate::settings::legacy_prefs::{HUD_SCALE, MAX_STREAM_FADE};

/// Layout › Bars and windows.
pub fn interface(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("b9-interface-group"), |cx| {
        cx.slider(
            tr!("b9-tooltip-delay"),
            tr!("b9-tooltip-delay-hint"),
            &field!(legacy_prefs.tooltip_delay),
            0.0..=5.0,
            " s",
        );
        cx.slider(
            tr!("b9-hud-scale"),
            tr!("b9-hud-scale-hint"),
            &field!(legacy_prefs.hud_scale),
            HUD_SCALE,
            " ×",
        );
        cx.toggle(
            tr!("b9-sim-version"),
            tr!("b9-sim-version-hint"),
            &field!(legacy_prefs.sim_version_in_bar),
        );
    });
}

/// Sound › Volumes: the parcel music's fade.
pub fn music_fade(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("b9-music-fade-group"), |cx| {
        cx.toggle(tr!("b9-music-fade"), tr!("b9-music-fade-hint"), &field!(legacy_prefs.stream_fade));
        let off = !cx.s.legacy_prefs.stream_fade;
        cx.subs(off, |cx| {
            cx.slider(
                tr!("b9-music-fade-in"),
                "",
                &field!(legacy_prefs.stream_fade_in),
                0.0..=MAX_STREAM_FADE,
                " s",
            );
            cx.slider(
                tr!("b9-music-fade-out"),
                "",
                &field!(legacy_prefs.stream_fade_out),
                0.0..=MAX_STREAM_FADE,
                " s",
            );
        });
    });
}

/// Sound › Voice: the microphone released while muted.
pub fn microphone(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("b9-microphone-group"), |cx| {
        cx.toggle(
            tr!("b9-release-mic"),
            tr!("b9-release-mic-hint"),
            &field!(legacy_prefs.release_mic_when_muted),
        );
    });
}

/// Privacy › Scripts and content: llMapDestination may open the map.
pub fn scripts(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.toggle(
        tr!("b9-scripts-show-ui"),
        tr!("b9-scripts-show-ui-hint"),
        &field!(legacy_prefs.scripts_can_show_ui),
    );
}

/// Network › Web browser: JavaScript; then the proxy and the UDP port.
pub fn network(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("b9-web-group"), |cx| {
        cx.toggle(
            tr!("b9-javascript"),
            tr!("b9-javascript-hint"),
            &field!(legacy_prefs.browser_javascript),
        );
    });
    cx.group(tr!("b9-proxy-group"), |cx| {
        cx.note(tr!("b9-proxy-note"));
        cx.toggle(tr!("b9-web-proxy"), tr!("b9-web-proxy-hint"), &field!(legacy_prefs.web_proxy));
        cx.greyed(!cx.s.legacy_prefs.web_proxy);
        cx.row(
            tr!("b9-proxy-address"),
            tr!("b9-proxy-address-hint"),
            &fields!(legacy_prefs.web_proxy_host, legacy_prefs.web_proxy_port),
            |ui, s| {
                let p = &mut s.legacy_prefs;
                let mut changed = ui
                    .add(
                        egui::TextEdit::singleline(&mut p.web_proxy_host)
                            .desired_width(SLIDER_W - 70.0)
                            .char_limit(255)
                            .hint_text("proxy.example.org"),
                    )
                    .changed();
                changed |= ui
                    .add(egui::DragValue::new(&mut p.web_proxy_port).range(1..=u16::MAX).speed(1.0))
                    .changed();
                changed
            },
        );
        cx.toggle(
            tr!("b9-proxy-other"),
            tr!("b9-proxy-other-hint"),
            &field!(legacy_prefs.proxy_other_http),
        );
        cx.greyed(false);
        super::socks_proxy::rows(cx);
        cx.toggle(
            tr!("b9-fixed-port"),
            tr!("b9-fixed-port-hint"),
            &field!(legacy_prefs.fixed_udp_port),
        );
        cx.greyed(!cx.s.legacy_prefs.fixed_udp_port);
        cx.row(tr!("b9-port"), "", &field!(legacy_prefs.udp_port), |ui, s| {
            ui.add(egui::DragValue::new(&mut s.legacy_prefs.udp_port).range(1..=u16::MAX).speed(1.0))
                .changed()
        });
        cx.greyed(false);
    });
}
