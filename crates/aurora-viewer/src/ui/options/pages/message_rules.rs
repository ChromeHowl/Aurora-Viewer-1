//! Preferences › Chat › Translation and filter: the translation of
//! incoming chat (the Legacy Viewer's floater_translation_settings.xml,
//! `world/translate.rs`). The Omnifilter has its own window (`ui/omnifilter.rs`).

use super::super::cx::{Cx, Nothing, field};
use super::super::{COMBO_W, OptionsUi, combo};
use crate::settings::chat_translation::Service;
use crate::theme::Palette;
use crate::ui::widgets;
use crate::world::translate::{KeyState, LANGUAGES};
use aurora_i18n::tr;
use egui::{RichText, Vec2};

/// The tab of the Chat page that holds this page (`pages::tabs`).
pub(crate) const TAB: usize = 7;

/// Where each service gives its keys.
fn key_site(service: Service) -> &'static str {
    match service {
        Service::Azure => "https://learn.microsoft.com/azure/ai-services/translator/create-translator-resource",
        Service::Google => "https://docs.cloud.google.com/translate/docs/authentication#api-keys",
        Service::DeepL => "https://www.deepl.com/pro-api",
    }
}

pub(super) fn page(cx: &mut Cx) {
    translation(cx);
}

fn translation(cx: &mut Cx) {
    let p = cx.p;
    cx.group(tr!("chatxl-translation"), |cx| {
        // the privacy note, always in sight
        cx.note(tr!("chatxl-privacy"));
        cx.toggle(tr!("chatxl-on"), tr!("chatxl-on-hint"), &field!(chat_translation.on));
        let services: Vec<(Service, &str)> = Service::ALL.iter().map(|s| (*s, s.name())).collect();
        cx.combo(tr!("chatxl-service"), "", &field!(chat_translation.service), &services);
        cx.keywords("language").row(
            tr!("chatxl-language"),
            tr!("chatxl-language-hint"),
            &field!(chat_translation.language),
            |ui, s| {
                let mut choices: Vec<(usize, &str)> = vec![(0, tr!("chatxl-language-interface"))];
                choices.extend(LANGUAGES.iter().enumerate().map(|(i, (_, name))| (i + 1, *name)));
                let lang = &mut s.chat_translation.language;
                let mut index = LANGUAGES.iter().position(|(code, _)| code == lang).map_or(0, |i| i + 1);
                let changed = combo(ui, &mut index, &choices);
                if changed {
                    *lang = index.checked_sub(1).map_or_else(String::new, |i| LANGUAGES[i].0.to_owned());
                }
                changed
            },
        );
        let azure = cx.s.chat_translation.service == Service::Azure;
        cx.conditional(azure, |cx| {
            cx.row(
                tr!("chatxl-azure-endpoint"),
                tr!("chatxl-azure-endpoint-hint"),
                &field!(chat_translation.azure_endpoint),
                |ui, s| {
                    let ok = crate::world::translate::azure_endpoint(&s.chat_translation.azure_endpoint).is_some();
                    ui.add(
                        egui::TextEdit::singleline(&mut s.chat_translation.azure_endpoint)
                            .desired_width(COMBO_W)
                            .text_color(if ok { p.ink } else { p.danger }),
                    )
                    .changed()
                },
            );
            cx.row(
                tr!("chatxl-azure-region"),
                tr!("chatxl-azure-region-hint"),
                &field!(chat_translation.azure_region),
                |ui, s| {
                    ui.add(
                        egui::TextEdit::singleline(&mut s.chat_translation.azure_region)
                            .desired_width(COMBO_W)
                            .hint_text("westeurope"),
                    )
                    .changed()
                },
            );
        });
        let service = cx.s.chat_translation.service;
        cx.keywords("key API").wide().row_with(
            &tr!("chatxl-key", service = service.name()),
            tr!("chatxl-key-hint"),
            &Nothing,
            |ui, _, st, _| {
                key_row(ui, &p, st, service);
                false
            },
        );
    });
}

/// The key of the service chosen: its state, a masked field, "Save" and
/// "Forget"; the key typed leaves the window at once, for the credential
/// store.
fn key_row(ui: &mut egui::Ui, p: &Palette, st: &mut OptionsUi, service: Service) {
    let i = Service::ALL.iter().position(|s| *s == service).unwrap_or(0);
    let state = st.translation_keys[i];
    if state == KeyState::Unknown {
        st.translation_probe = true;
    }
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut st.translation_key_typed)
                    .password(true)
                    .desired_width(COMBO_W)
                    .hint_text(tr!("chatxl-key-placeholder")),
            );
            let usable = crate::world::translate::usable_key(&st.translation_key_typed).is_some();
            if ui
                .add_enabled_ui(usable, |ui| widgets::flat_button(ui, p, tr!("common-save")))
                .inner
                .clicked()
            {
                let key = std::mem::take(&mut st.translation_key_typed);
                st.translation_key_edits.push((service, Some(key)));
            }
            let stored = matches!(state, KeyState::Stored | KeyState::Session);
            if ui
                .add_enabled_ui(stored, |ui| widgets::flat_button(ui, p, tr!("chatxl-key-forget")))
                .inner
                .clicked()
            {
                st.translation_key_edits.push((service, None));
            }
            let site = key_site(service);
            if widgets::action_button(
                ui,
                p,
                "external-link",
                tr!("chatxl-key-get"),
                widgets::ActionKind::Flat,
                Vec2::new(0.0, 24.0),
            )
            .on_hover_text(site)
            .clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(site));
            }
        });
        let (text, color) = match state {
            KeyState::Unknown => (tr!("chatxl-key-checking"), p.muted),
            KeyState::None => (tr!("chatxl-key-none"), p.muted),
            KeyState::Stored => (tr!("chatxl-key-stored"), p.muted),
            KeyState::Session => (tr!("chatxl-key-session"), p.danger),
        };
        ui.label(RichText::new(text).size(11.5).color(color));
    });
}
