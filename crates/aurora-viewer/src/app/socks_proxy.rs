//! Preferences › Network's SOCKS 5 proxy between the window and the
//! credential store (`socks_credentials.rs`), and the network options each
//! login applies (`settings/socks_proxy.rs`).
use super::*;

impl App {
    pub(super) fn sync_socks_options(&mut self) {
        let o = &mut self.options_ui;
        if let Some(edit) = o.socks_edit.take() {
            self.socks_credentials.set(edit);
            o.socks_state = self.socks_credentials.state();
        }
        if std::mem::take(&mut o.socks_probe) {
            o.socks_state = self.socks_credentials.state();
        }
    }

    /// The proxy and the UDP port for the next login (LLStartUp::
    /// startLLProxy runs at each login): the credential is read only when
    /// the proxy asks for one.
    pub(super) fn network_options(&self) -> aurora_net::NetOptions {
        let socks = &self.settings.socks_proxy;
        let auth = (socks.enabled && socks.user_pass).then(|| self.socks_credentials.get()).flatten();
        crate::settings::socks_proxy::net_options(&self.settings.legacy_prefs, socks, auth)
    }
}
