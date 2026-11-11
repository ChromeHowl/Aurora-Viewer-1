//! Small preferences of the Legacy Viewer that Aurora had no setting for:
//! the interface's tooltip delay, the size of the HUDs, the web proxy and a
//! fixed UDP port, the web browser's JavaScript, scripts opening the map,
//! the microphone released while muted, the simulator version in the top
//! bar and the fade of the parcel music (the RLVa texts live with the other
//! RLVa preferences, `world/rlva/strings.rs`). Each field names
//! its Legacy setting (indra/newview/app_settings/settings.xml) and keeps its
//! default, unless the comment says why not.

use serde::{Deserialize, Serialize};

/// ToolTipDelay: seconds before a tooltip of the interface shows.
pub const TOOLTIP_DELAY: f32 = 0.7;
/// HUDScaleFactor's range in Preferences (panel_preferences_UI.xml).
pub const HUD_SCALE: std::ops::RangeInclusive<f32> = 1.0..=2.0;
/// BrowserProxyPort's default.
pub const PROXY_PORT: u16 = 3128;
/// ConnectionPort's default.
pub const UDP_PORT: u16 = 13000;
/// FSAudioMusicFadeIn / FSAudioMusicFadeOut defaults (seconds).
pub const STREAM_FADE_IN: f32 = 3.0;
pub const STREAM_FADE_OUT: f32 = 2.0;
/// Longest fade offered (Preferences › Sound, panel_preferences_sound.xml
/// stops its spinners at 10 s).
pub const MAX_STREAM_FADE: f32 = 10.0;

/// The preferences (see the module).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LegacyPrefs {
    /// ToolTipDelay (0.7 s): how long the pointer rests on a control before
    /// its tooltip shows.
    pub tooltip_delay: f32,
    /// HUDScaleFactor (1, 1 to 2): the size of every HUD on the screen
    /// (LLAgentCamera::getAgentHUDTargetZoom).
    pub hud_scale: f32,
    /// BrowserProxyEnabled (off): web pages go through an HTTP proxy.
    pub web_proxy: bool,
    /// BrowserProxyAddress (empty): the proxy's host name or address.
    pub web_proxy_host: String,
    /// BrowserProxyPort (3128).
    pub web_proxy_port: u16,
    /// HttpProxyType "Web" (with BrowserProxyEnabled): the viewer's other
    /// HTTP traffic (login, capabilities, assets) through the same proxy.
    /// Its "Socks" choice is `settings/socks_proxy.rs`'s, which comes first.
    pub proxy_other_http: bool,
    /// ConnectionPortEnabled (off): the region traffic leaves from a fixed
    /// UDP port, for a firewall or a router.
    pub fixed_udp_port: bool,
    /// ConnectionPort (13000).
    pub udp_port: u16,
    /// BrowserJavascriptEnabled (on): JavaScript in the built-in browser and
    /// the other web windows.
    pub browser_javascript: bool,
    /// ScriptsCanShowUI (on): llMapDestination opens the world map.
    pub scripts_can_show_ui: bool,
    /// StopMicCaptureWhenMuted (on): the microphone is closed while we do
    /// not speak, so that a Bluetooth headset leaves its call mode.
    pub release_mic_when_muted: bool,
    /// FSStatusbarShowSimulatorVersion (off): the simulator's version after
    /// the region's name in the top bar.
    pub sim_version_in_bar: bool,
    /// FSFadeAudioStream (on): the parcel music fades out and in when it
    /// changes.
    pub stream_fade: bool,
    /// FSAudioMusicFadeIn (3 s).
    pub stream_fade_in: f32,
    /// FSAudioMusicFadeOut (2 s).
    pub stream_fade_out: f32,
}

impl Default for LegacyPrefs {
    fn default() -> Self {
        LegacyPrefs {
            tooltip_delay: TOOLTIP_DELAY,
            hud_scale: 1.0,
            web_proxy: false,
            web_proxy_host: String::new(),
            web_proxy_port: PROXY_PORT,
            proxy_other_http: false,
            fixed_udp_port: false,
            udp_port: UDP_PORT,
            browser_javascript: true,
            scripts_can_show_ui: true,
            release_mic_when_muted: true,
            sim_version_in_bar: false,
            stream_fade: true,
            stream_fade_in: STREAM_FADE_IN,
            stream_fade_out: STREAM_FADE_OUT,
        }
    }
}

/// A proxy's host as the user may type it: a name or an address, without
/// a scheme, a path, spaces or control characters (the Legacy Viewer
/// resolves the name as it is, LLHost::setHostByName).
pub fn usable_host(host: &str) -> bool {
    !host.is_empty()
        && !host.contains("://")
        && !host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '/' | '@' | '?' | '#'))
}

fn finite_or(value: f32, default: f32) -> f32 {
    if value.is_finite() { value } else { default }
}

impl LegacyPrefs {
    /// Bounds a file's values.
    pub fn sanitize(&mut self) {
        self.tooltip_delay = finite_or(self.tooltip_delay, TOOLTIP_DELAY).clamp(0.0, 5.0);
        self.hud_scale = finite_or(self.hud_scale, 1.0).clamp(*HUD_SCALE.start(), *HUD_SCALE.end());
        self.web_proxy_host = self.web_proxy_host.trim().chars().take(255).collect();
        if self.web_proxy_port == 0 {
            self.web_proxy_port = PROXY_PORT;
        }
        if self.udp_port == 0 {
            self.udp_port = UDP_PORT;
        }
        self.stream_fade_in = finite_or(self.stream_fade_in, STREAM_FADE_IN).clamp(0.0, MAX_STREAM_FADE);
        self.stream_fade_out = finite_or(self.stream_fade_out, STREAM_FADE_OUT).clamp(0.0, MAX_STREAM_FADE);
    }

    /// The web proxy, as `host:port`, when it is on and its host usable
    /// (`usable_host`).
    pub fn web_proxy_address(&self) -> Option<String> {
        if !self.web_proxy {
            return None;
        }
        let host = self.web_proxy_host.trim();
        (usable_host(host) && self.web_proxy_port != 0).then(|| {
            // an IPv6 address goes between brackets
            if host.contains(':') && !host.starts_with('[') {
                format!("[{host}]:{}", self.web_proxy_port)
            } else {
                format!("{host}:{}", self.web_proxy_port)
            }
        })
    }

    /// The proxy of the viewer's other HTTP traffic (HttpProxyType "Web"
    /// with BrowserProxyEnabled, LLStartUp::startLLProxy).
    pub fn http_proxy(&self) -> Option<String> {
        self.proxy_other_http.then(|| self.web_proxy_address()).flatten()
    }

    /// The UDP port asked (ConnectionPortEnabled); None for one chosen by
    /// the system.
    pub fn udp_port(&self) -> Option<u16> {
        (self.fixed_udp_port && self.udp_port != 0).then_some(self.udp_port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_legacy_viewers() {
        let p = LegacyPrefs::default();
        assert_eq!(p.tooltip_delay, 0.7);
        assert_eq!(p.hud_scale, 1.0);
        assert!(!p.web_proxy && !p.proxy_other_http && !p.fixed_udp_port && !p.sim_version_in_bar);
        assert!(p.browser_javascript && p.scripts_can_show_ui && p.release_mic_when_muted && p.stream_fade);
        assert_eq!((p.web_proxy_port, p.udp_port), (3128, 13000));
        assert_eq!((p.stream_fade_in, p.stream_fade_out), (3.0, 2.0));
        assert_eq!(p.http_proxy(), None);
        assert_eq!(p.udp_port(), None);
    }

    #[test]
    fn an_old_file_loads_with_the_defaults() {
        let p: LegacyPrefs = serde_json::from_str(r#"{"hud_scale": 1.5}"#).expect("partial file");
        assert_eq!(p.hud_scale, 1.5);
        assert_eq!(p.tooltip_delay, TOOLTIP_DELAY);
        assert!(p.stream_fade);
    }

    #[test]
    fn sanitize_bounds_every_value() {
        let mut p = LegacyPrefs {
            tooltip_delay: f32::NAN,
            hud_scale: 9.0,
            web_proxy_port: 0,
            udp_port: 0,
            stream_fade_in: -1.0,
            stream_fade_out: f32::INFINITY,
            web_proxy_host: "  proxy.example  ".into(),
            ..LegacyPrefs::default()
        };
        p.sanitize();
        assert_eq!(p.tooltip_delay, TOOLTIP_DELAY);
        assert_eq!(p.hud_scale, 2.0);
        assert_eq!((p.web_proxy_port, p.udp_port), (PROXY_PORT, UDP_PORT));
        assert_eq!((p.stream_fade_in, p.stream_fade_out), (0.0, STREAM_FADE_OUT));
        assert_eq!(p.web_proxy_host, "proxy.example");
    }

    #[test]
    fn the_proxy_needs_its_switch_and_a_plain_host() {
        let mut p = LegacyPrefs {
            web_proxy_host: "10.0.0.2".into(),
            web_proxy_port: 8080,
            ..LegacyPrefs::default()
        };
        assert_eq!(p.web_proxy_address(), None, "off");
        p.web_proxy = true;
        assert_eq!(p.web_proxy_address().as_deref(), Some("10.0.0.2:8080"));
        assert_eq!(p.http_proxy(), None, "the other traffic has its own switch");
        p.proxy_other_http = true;
        assert_eq!(p.http_proxy().as_deref(), Some("10.0.0.2:8080"));
        p.web_proxy_host = "::1".into();
        assert_eq!(p.web_proxy_address().as_deref(), Some("[::1]:8080"));
        for bad in ["", "http://proxy", "proxy host", "user@proxy", "proxy/path"] {
            p.web_proxy_host = bad.into();
            assert_eq!(p.web_proxy_address(), None, "{bad:?}");
        }
    }

    #[test]
    fn the_fixed_port_needs_its_switch() {
        let mut p = LegacyPrefs::default();
        assert_eq!(p.udp_port(), None);
        p.fixed_udp_port = true;
        assert_eq!(p.udp_port(), Some(13000));
    }
}
