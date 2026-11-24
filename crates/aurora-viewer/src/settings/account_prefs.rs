//! Account binding for the preferences the Legacy Viewer keeps per account
//! (settings_per_account.xml) and Aurora kept shared: the keyword alerts
//! (FSKeyword*, UISndFSKeywordSound), the chat and IM logging options
//! (KeepConversationLogTranscripts, LogNearbyChat, LogShowHistory,
//! LogFileNamewithDate) and the crouch toggle (FSCrouchToggle).
//!
//! Same scheme as the auto-response block (`autoresponse_account.rs`) and the
//! avatar height options (`hover_height.rs`): the former shared values go to
//! the first account that logs in, and an account without saved options
//! starts from the defaults.
use super::Settings;
use crate::keywords::KeywordSettings;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AccountPrefs {
    keywords: KeywordSettings,
    log_im: bool,
    log_nearby: bool,
    log_show_history: bool,
    log_dated_names: bool,
    crouch_toggle: bool,
}

impl Default for AccountPrefs {
    fn default() -> Self {
        Self::current(&Settings::default())
    }
}

impl AccountPrefs {
    fn current(settings: &Settings) -> Self {
        Self {
            keywords: settings.keywords.clone(),
            log_im: settings.log_im,
            log_nearby: settings.log_nearby,
            log_show_history: settings.log_show_history,
            log_dated_names: settings.log_dated_names,
            crouch_toggle: settings.crouch_toggle,
        }
    }

    fn apply(self, settings: &mut Settings) {
        settings.keywords = self.keywords;
        settings.log_im = self.log_im;
        settings.log_nearby = self.log_nearby;
        settings.log_show_history = self.log_show_history;
        settings.log_dated_names = self.log_dated_names;
        settings.crouch_toggle = self.crouch_toggle;
    }
}

impl Settings {
    /// Bind before the first world update. The caller installs the keyword
    /// alerts again (`keywords::set`), which only follows a settings change.
    pub(crate) fn select_prefs_account(&mut self, account: Uuid) {
        self.remember_prefs_account();
        let chosen = match &self.prefs_accounts {
            None => AccountPrefs::current(self),
            Some(accounts) => accounts.get(&account).cloned().unwrap_or_default(),
        };
        self.prefs_account = Some(account);
        chosen.apply(self);
        self.remember_prefs_account();
    }

    /// Logout: stop login-screen edits from changing the last account's values.
    pub(crate) fn release_prefs_account(&mut self) {
        if self.prefs_account.is_none() {
            return;
        }
        self.remember_prefs_account();
        self.prefs_account = None;
        AccountPrefs::default().apply(self);
    }

    /// Every settings save captures what the Options window changed, so one
    /// avatar's choices cannot be written over another's.
    pub(super) fn remember_prefs_account(&mut self) {
        if let Some(account) = self.prefs_account {
            let prefs = AccountPrefs::current(self);
            self.prefs_accounts.get_or_insert_with(Default::default).insert(account, prefs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restart(settings: &Settings) -> Settings {
        serde_json::from_slice(&serde_json::to_vec(&settings.to_saved()).unwrap()).unwrap()
    }

    #[test]
    fn each_account_keeps_its_own_preferences_across_a_switch_a_save_and_a_restart() {
        let (first, second) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut settings = Settings::default();
        settings.keywords.words = "shared before any login".into();
        settings.select_prefs_account(first);
        // the former shared values go to the first account
        assert_eq!(settings.keywords.words, "shared before any login");
        settings.keywords.on = true;
        settings.keywords.words = "first".into();
        settings.log_nearby = true;
        settings.log_dated_names = true;
        settings.crouch_toggle = true;
        settings.select_prefs_account(second);
        // an account without saved options starts from the defaults
        assert_eq!(AccountPrefs::current(&settings), AccountPrefs::default());
        assert!(!settings.keywords.on && settings.keywords.words.is_empty());
        settings.keywords.words = "second".into();
        settings.log_im = !settings.log_im;
        settings.select_prefs_account(first);
        assert_eq!(settings.keywords.words, "first");
        assert!(settings.keywords.on && settings.log_nearby && settings.log_dated_names && settings.crouch_toggle);
        settings.select_prefs_account(second);
        assert_eq!(settings.keywords.words, "second");
        assert!(!settings.crouch_toggle && !settings.log_nearby);
        let mut loaded = restart(&settings);
        loaded.select_prefs_account(first);
        assert_eq!(loaded.keywords.words, "first");
        assert!(loaded.crouch_toggle);
        loaded.select_prefs_account(second);
        assert_eq!(loaded.keywords.words, "second");
        assert!(!loaded.crouch_toggle);
    }

    #[test]
    fn a_reset_clears_this_accounts_preferences_and_keeps_the_others() {
        let (other, mine) = (Uuid::from_u128(5), Uuid::from_u128(6));
        let mut settings = Settings::default();
        settings.select_prefs_account(other);
        settings.keywords.words = "keep".into();
        settings.select_prefs_account(mine);
        settings.keywords.words = "mine".into();
        settings.reset_keeping_account();
        assert_eq!(AccountPrefs::current(&settings), AccountPrefs::default());
        settings.select_prefs_account(other);
        assert_eq!(settings.keywords.words, "keep");
    }

    #[test]
    fn an_old_file_without_accounts_still_loads_its_values() {
        let mut settings: Settings = serde_json::from_value(serde_json::json!({
            "keywords": {"words": "from the old file"},
            "crouch_toggle": true
        }))
        .unwrap();
        assert!(settings.prefs_accounts.is_none());
        settings.select_prefs_account(Uuid::from_u128(9));
        assert_eq!(settings.keywords.words, "from the old file");
        assert!(settings.crouch_toggle);
    }
}
