//! The SOCKS 5 proxy's user name and password (Preferences › Network),
//! kept in the system's credential store like the translation keys
//! (`world/translate.rs`), never in the settings file nor the log: the
//! Legacy Viewer's "SOCKS5" credential (LLSecAPIHandler::saveCredential in
//! LLFloaterPreferenceProxy::onBtnOk, read by LLStartUp::startLLProxy).
//! When the store is unavailable they are kept in memory until the viewer
//! exits. Removed from this computer by `uninstall.rs`.

use aurora_net::socks5::SocksAuth;

const SERVICE: &str = "Aurora Viewer proxy";
const ACCOUNT: &str = "socks5";
/// (service, account) in the credential store.
pub const STORE_ENTRY: (&str, &str) = (SERVICE, ACCOUNT);

/// Where the credential is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum State {
    /// Not looked at yet.
    #[default]
    Unknown,
    None,
    /// In the credential store.
    Stored,
    /// The store is unavailable: kept in memory until the viewer exits.
    Session,
}

/// The credential of this run: its state, and the copy kept in memory
/// when the store refused it.
#[derive(Default)]
pub struct SocksCredentials {
    state: State,
    session: Option<SocksAuth>,
}

impl SocksCredentials {
    /// The state, looked up in the store the first time (Preferences).
    pub fn state(&mut self) -> State {
        if self.state == State::Unknown {
            self.state = if load().is_some() { State::Stored } else { State::None };
        }
        self.state
    }

    /// The user name and password the user saved, or None to forget them.
    pub fn set(&mut self, auth: Option<SocksAuth>) {
        let kept = save(auth.as_ref());
        self.state = match (&auth, kept) {
            (None, _) => State::None,
            (Some(_), true) => State::Stored,
            (Some(_), false) => State::Session,
        };
        self.session = if kept { None } else { auth };
    }

    /// The credential for a login: the store's, read each time (another
    /// window may have changed it), or the one kept in memory.
    pub fn get(&self) -> Option<SocksAuth> {
        self.session.clone().or_else(load)
    }
}

fn entry() -> Option<keyring::Entry> {
    if cfg!(test) || aurora_updater::test_environment().is_some() {
        return None;
    }
    keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| log::warn!("SOCKS 5 proxy: credential store unavailable: {e}"))
        .ok()
}

/// The stored form: the user name, a line feed, the password (a user name
/// holds no line feed: `parse` and the Preferences field refuse one).
fn encode(auth: &SocksAuth) -> String {
    format!("{}\n{}", auth.username, auth.password)
}

fn parse(stored: &str) -> Option<SocksAuth> {
    let (username, password) = stored.split_once('\n')?;
    let auth = SocksAuth {
        username: username.to_owned(),
        password: password.to_owned(),
    };
    auth.valid().then_some(auth)
}

/// A user name and password the proxy may be given (RFC 1929's limits, no
/// line feed in the user name).
pub fn usable(username: &str, password: &str) -> Option<SocksAuth> {
    let auth = SocksAuth {
        username: username.trim().to_owned(),
        password: password.to_owned(),
    };
    (auth.valid() && !auth.username.contains('\n')).then_some(auth)
}

fn load() -> Option<SocksAuth> {
    match entry()?.get_password() {
        Ok(stored) => parse(&stored),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            log::warn!("SOCKS 5 proxy: credential unreadable: {e}");
            None
        }
    }
}

/// Stores or forgets the credential: true when the store holds the change.
fn save(auth: Option<&SocksAuth>) -> bool {
    let Some(entry) = entry() else {
        return false;
    };
    let result = match auth {
        Some(auth) => entry.set_password(&encode(auth)),
        None => match entry.delete_credential() {
            Err(keyring::Error::NoEntry) => Ok(()),
            other => other,
        },
    };
    result.map_err(|e| log::warn!("SOCKS 5 proxy: credential not kept: {e}")).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_form_round_trips() {
        let auth = usable(" someone ", "p:a\nss").expect("usable");
        assert_eq!(auth.username, "someone");
        assert_eq!(parse(&encode(&auth)), Some(auth));
        assert_eq!(parse("no line feed"), None);
        assert_eq!(parse("\npassword"), None, "no user name");
    }

    #[test]
    fn only_rfc_1929_credentials_are_usable() {
        assert!(usable("", "p").is_none());
        assert!(usable(&"u".repeat(256), "p").is_none());
        assert!(usable("u", &"p".repeat(256)).is_none());
        assert!(usable("u\nv", "p").is_none());
        assert!(usable("u", "").is_some());
    }

    #[test]
    fn without_a_store_they_stay_in_memory() {
        // (the tests never reach the system's store)
        let mut c = SocksCredentials::default();
        assert_eq!(c.state(), State::None);
        let auth = usable("u", "p").expect("usable");
        c.set(Some(auth.clone()));
        assert_eq!(c.state(), State::Session);
        assert_eq!(c.get(), Some(auth));
        c.set(None);
        assert_eq!((c.state(), c.get()), (State::None, None));
    }
}
