//! Preferences › Chat › Translation and filter, for the account logged in:
//! the translation keys (`world/translate.rs`), between the window and the
//! world.
use super::*;
use crate::settings::chat_translation::Service;

impl App {
    pub(super) fn sync_filter_and_translation_options(&mut self) {
        let o = &mut self.options_ui;
        // the keys typed go straight to the translation thread, which keeps
        // them in the credential store
        let translation = &mut self.world.translation;
        for (service, key) in std::mem::take(&mut o.translation_key_edits) {
            translation.set_key(service, key);
        }
        if std::mem::take(&mut o.translation_probe) {
            translation.probe();
        }
        for (i, service) in Service::ALL.into_iter().enumerate() {
            o.translation_keys[i] = translation.key_state(service);
        }
    }
}
