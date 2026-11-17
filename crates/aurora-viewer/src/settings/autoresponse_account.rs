//! Account binding for the auto-response block: the away, do-not-disturb,
//! auto-response, muted-avatar and rejection messages with their switches and
//! the away timers. The Legacy Viewer keeps them per account
//! (settings_per_account.xml: DoNotDisturbModeResponse, FSAutorespondMode*,
//! FSAwayAvatarResponse, FSMutedAvatarResponse, FSRejectTeleportOffers*,
//! FSRejectFriendshipRequests*, VoiceCallsFriendsOnly...), so an account does
//! not answer with another one's text.
//!
//! Same scheme as the avatar height options (`hover_height.rs`): the former
//! shared block goes to the first account that logs in, and an account
//! without saved options starts from the defaults.
use super::Settings;
use uuid::Uuid;

impl Settings {
    /// Bind before the first world update.
    pub(crate) fn select_autoresponse_account(&mut self, account: Uuid) {
        self.remember_autoresponse_account();
        let chosen = match &self.autoresponse_accounts {
            None => self.autoresponse.clone(),
            Some(accounts) => accounts.get(&account).cloned().unwrap_or_default(),
        };
        self.autoresponse_account = Some(account);
        self.autoresponse = chosen;
        self.remember_autoresponse_account();
    }

    /// Every settings save captures what the Options window changed, so the
    /// block cannot be written over another avatar's.
    pub(super) fn remember_autoresponse_account(&mut self) {
        if let Some(account) = self.autoresponse_account {
            let block = self.autoresponse.clone();
            self.autoresponse_accounts
                .get_or_insert_with(Default::default)
                .insert(account, block);
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
    fn each_account_keeps_its_own_messages_across_a_switch_a_save_and_a_restart() {
        let (first, second) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut settings = Settings::default();
        settings.autoresponse.away_text = "shared before any login".into();
        settings.select_autoresponse_account(first);
        // the former shared block goes to the first account
        assert_eq!(settings.autoresponse.away_text, "shared before any login");
        settings.autoresponse.away_text = "first is away".into();
        settings.autoresponse.autorespond_text = "first answers".into();
        settings.autoresponse.send_away = true;
        settings.select_autoresponse_account(second);
        // an account without saved options starts from the defaults
        assert_eq!(settings.autoresponse, Default::default());
        settings.autoresponse.away_text = "second is away".into();
        settings.select_autoresponse_account(first);
        assert_eq!(settings.autoresponse.away_text, "first is away");
        assert_eq!(settings.autoresponse.autorespond_text, "first answers");
        assert!(settings.autoresponse.send_away);
        settings.select_autoresponse_account(second);
        assert_eq!(settings.autoresponse.away_text, "second is away");
        let mut loaded = restart(&settings);
        loaded.select_autoresponse_account(first);
        assert_eq!(loaded.autoresponse.away_text, "first is away");
        loaded.select_autoresponse_account(second);
        assert_eq!(loaded.autoresponse.away_text, "second is away");
    }

    #[test]
    fn a_reset_keeps_the_accounts_blocks() {
        let account = Uuid::from_u128(5);
        let mut settings = Settings::default();
        settings.select_autoresponse_account(account);
        settings.autoresponse.dnd_text = "busy".into();
        settings.remember_autoresponse_account();
        settings.reset_keeping_account();
        settings.select_autoresponse_account(account);
        assert_eq!(settings.autoresponse.dnd_text, "busy");
    }

    #[test]
    fn an_old_file_without_accounts_still_loads_its_block() {
        let settings: Settings = serde_json::from_value(serde_json::json!({
            "autoresponse": {"away_text": "from the old file"}
        }))
        .unwrap();
        assert!(settings.autoresponse_accounts.is_none());
        let mut settings = settings;
        settings.select_autoresponse_account(Uuid::from_u128(9));
        assert_eq!(settings.autoresponse.away_text, "from the old file");
    }
}
