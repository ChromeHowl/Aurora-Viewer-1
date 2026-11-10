//! Preferences › Chat › Translation and filter, for the account logged in:
//! the Omnifilter's rules and list (`world/omnifilter.rs`) and the
//! translation keys (`world/translate.rs`), between the window and the
//! world.
use super::*;
use crate::settings::chat_translation::Service;

impl App {
    pub(super) fn sync_filter_and_translation_options(&mut self) {
        let in_world = self.in_world();
        let o = &mut self.options_ui;
        if let Some(set) = o.omnifilter_edit.take()
            && in_world
        {
            self.world.omnifilter.replace(set);
        }
        self.world.omnifilter.flush(Instant::now());
        if std::mem::take(&mut o.omnifilter_clear_log) {
            self.world.omnifilter.clear_log();
        }
        let filter = &self.world.omnifilter;
        let rules = in_world.then(|| filter.rules());
        if o.omnifilter.as_ref() != rules {
            o.omnifilter = rules.cloned();
            o.omnifilter_broken = filter.broken().to_vec();
        }
        // the messages set aside, as the chat would show them under RLVa
        // (names and places hidden)
        let key = (filter.revision, self.world.rlva.revision);
        if o.omnifilter_log_key != key {
            o.omnifilter_log_key = key;
            let world = &self.world;
            o.omnifilter_log = filter
                .log
                .iter()
                .map(|e| crate::world::omnifilter::LogEntry {
                    from: world.rlva_filter_text(&e.from).into_owned(),
                    text: world.rlva_filter_text(&e.text).into_owned(),
                    ..e.clone()
                })
                .collect();
        }
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
