//! The SOCKS 5 proxy of Preferences › Network (the Legacy Viewer's
//! floater_preferences_proxy.xml: Socks5ProxyEnabled, Socks5ProxyHost,
//! Socks5ProxyPort, Socks5AuthType and HttpProxyType "Socks"), and the
//! network options each login applies (`net_options`, LLStartUp::
//! startLLProxy). The user name and password are not settings: they live
//! in the credential store (`socks_credentials.rs`).

use super::legacy_prefs::{LegacyPrefs, usable_host};
use aurora_net::socks5::{SocksAuth, SocksProxy};
use aurora_net::{HttpProxy, NetOptions};
use serde::{Deserialize, Serialize};

/// Socks5ProxyPort's default.
pub const SOCKS_PORT: u16 = 1080;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SocksPrefs {
    /// Socks5ProxyEnabled (off): the region traffic (UDP) through the proxy.
    pub enabled: bool,
    /// Socks5ProxyHost (empty).
    pub host: String,
    /// Socks5ProxyPort (1080).
    pub port: u16,
    /// Socks5AuthType "UserPass" (off: "None").
    pub user_pass: bool,
    /// HttpProxyType "Socks" (its default): the login, the capabilities and
    /// the downloads through the same proxy.
    pub http: bool,
}

impl Default for SocksPrefs {
    fn default() -> Self {
        SocksPrefs {
            enabled: false,
            host: String::new(),
            port: SOCKS_PORT,
            user_pass: false,
            http: true,
        }
    }
}

impl SocksPrefs {
    /// Bounds a file's values.
    pub fn sanitize(&mut self) {
        self.host = self.host.trim().chars().take(255).collect();
        if self.port == 0 {
            self.port = SOCKS_PORT;
        }
    }

    /// The proxy when it is on. Its host is checked by the association
    /// (SOCKS_INVALID_HOST on the login screen, as the Legacy Viewer);
    /// with "user name and password" and none saved, an empty one, which
    /// the association refuses (SOCKS_BAD_CREDS).
    pub fn proxy(&self, auth: Option<SocksAuth>) -> Option<SocksProxy> {
        self.enabled.then(|| SocksProxy {
            host: self.host.trim().to_owned(),
            port: self.port,
            auth: self.user_pass.then(|| {
                auth.unwrap_or(SocksAuth {
                    username: String::new(),
                    password: String::new(),
                })
            }),
        })
    }
}

/// The options of a login (startLLProxy and start_messaging_system): the
/// SOCKS 5 proxy, the HTTP traffic through it when asked, else through
/// the web proxy when asked (HttpProxyType "Web"), and the UDP port.
pub fn net_options(legacy: &LegacyPrefs, socks: &SocksPrefs, auth: Option<SocksAuth>) -> NetOptions {
    let socks = socks.proxy(auth).map(|proxy| (socks.http, proxy));
    let http_proxy = match (&socks, legacy.http_proxy()) {
        (Some((true, proxy)), _) if usable_host(&proxy.host) => HttpProxy::Socks,
        (_, Some(address)) => HttpProxy::Web(address),
        _ => HttpProxy::None,
    };
    NetOptions {
        http_proxy,
        socks: socks.map(|(_, proxy)| proxy),
        udp_port: legacy.udp_port(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> SocksAuth {
        SocksAuth {
            username: "user".into(),
            password: "pass".into(),
        }
    }

    fn socks() -> SocksPrefs {
        SocksPrefs {
            enabled: true,
            host: "proxy.example".into(),
            ..SocksPrefs::default()
        }
    }

    #[test]
    fn the_defaults_are_the_legacy_viewers() {
        let p = SocksPrefs::default();
        assert!(!p.enabled && !p.user_pass && p.http);
        assert_eq!(p.port, 1080);
        assert_eq!(p.proxy(Some(auth())), None);
        let old: SocksPrefs = serde_json::from_str(r#"{"enabled": true}"#).expect("partial file");
        assert!(old.enabled && old.http);
        assert_eq!(old.port, SOCKS_PORT);
    }

    #[test]
    fn sanitize_bounds_the_host_and_port() {
        let mut p = SocksPrefs {
            host: "  proxy  ".into(),
            port: 0,
            ..SocksPrefs::default()
        };
        p.sanitize();
        assert_eq!((p.host.as_str(), p.port), ("proxy", SOCKS_PORT));
    }

    #[test]
    fn the_credentials_go_only_with_user_and_password() {
        let mut p = socks();
        assert_eq!(p.proxy(Some(auth())).expect("on").auth, None);
        p.user_pass = true;
        assert_eq!(p.proxy(Some(auth())).expect("on").auth, Some(auth()));
        // none saved: an empty one, refused at the login
        let empty = p.proxy(None).expect("on").auth.expect("asked");
        assert!(!empty.valid());
    }

    #[test]
    fn socks_http_comes_before_the_web_proxy() {
        let web = LegacyPrefs {
            web_proxy: true,
            web_proxy_host: "10.0.0.2".into(),
            proxy_other_http: true,
            ..LegacyPrefs::default()
        };
        let none = net_options(&LegacyPrefs::default(), &SocksPrefs::default(), None);
        assert_eq!(none, NetOptions::default());
        let o = net_options(&web, &SocksPrefs::default(), None);
        assert_eq!(o.http_proxy, HttpProxy::Web("10.0.0.2:3128".into()));
        assert_eq!(o.socks, None);
        let o = net_options(&web, &socks(), None);
        assert_eq!(o.http_proxy, HttpProxy::Socks);
        assert!(o.socks.is_some());
        // the HTTP traffic left out of the SOCKS proxy: the web proxy's
        let only_udp = SocksPrefs { http: false, ..socks() };
        assert_eq!(
            net_options(&web, &only_udp, None).http_proxy,
            HttpProxy::Web("10.0.0.2:3128".into())
        );
        assert_eq!(net_options(&LegacyPrefs::default(), &only_udp, None).http_proxy, HttpProxy::None);
        // an unusable host: the UDP association reports it, HTTP stays out
        let bad = SocksPrefs {
            host: "http://proxy".into(),
            ..socks()
        };
        let o = net_options(&LegacyPrefs::default(), &bad, None);
        assert_eq!(o.http_proxy, HttpProxy::None);
        assert!(o.socks.is_some());
    }

    #[test]
    fn the_udp_port_follows_its_switch() {
        let legacy = LegacyPrefs {
            fixed_udp_port: true,
            ..LegacyPrefs::default()
        };
        assert_eq!(net_options(&legacy, &SocksPrefs::default(), None).udp_port, Some(13000));
    }
}
