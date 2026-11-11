//! Preferences › Chat › Translation and filter: the translation of
//! incoming chat (the Legacy Viewer's floater_translation_settings.xml,
//! `world/translate.rs`) and the Omnifilter's rules of the account
//! (floater_omnifilter.xml, `world/omnifilter.rs`).

use super::super::cx::{Cx, Nothing, field};
use super::super::{COMBO_W, OptionsUi, combo, swatch};
use crate::settings::chat_translation::Service;
use crate::theme::Palette;
use crate::ui::widgets;
use crate::world::omnifilter::{Action, Channel, DEFAULT_SET, MatchMode, Pattern, Rule, RuleSet, SetError, Source, legacy};
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

/// Height of the lists before they scroll.
const LIST_H: f32 = 180.0;

pub(super) fn page(cx: &mut Cx) {
    translation(cx);
    omnifilter(cx);
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

fn omnifilter(cx: &mut Cx) {
    let p = cx.p;
    cx.group(tr!("chatxl-omnifilter"), |cx| {
        cx.note(tr!("chatxl-omnifilter-intro"));
        cx.keywords("Omnifilter regex").below().row_with(
            tr!("chatxl-omnifilter-rules"),
            tr!("chatxl-omnifilter-rules-hint"),
            &Nothing,
            |ui, _, st, _| {
                rules(ui, &p, st);
                false
            },
        );
        cx.keywords("Omnifilter").below().row_with(
            tr!("chatxl-omnifilter-log"),
            tr!("chatxl-omnifilter-log-hint"),
            &Nothing,
            |ui, _, st, _| {
                log(ui, &p, st);
                false
            },
        );
    });
}

fn muted(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(p.muted));
}

/// The account's rules: the filter's switch, the list (on, name, order,
/// removal) and the rule open below it.
fn rules(ui: &mut egui::Ui, p: &Palette, st: &mut OptionsUi) {
    let Some(set) = st.omnifilter.as_ref() else {
        muted(ui, p, tr!("chatxl-omnifilter-logged-out"));
        return;
    };
    let mut edited: RuleSet = set.clone();
    let broken = st.omnifilter_broken.clone();
    let mut open = st.omnifilter_open.filter(|i| *i < edited.rules.len());
    ui.vertical(|ui| {
        widgets::switch_labeled(ui, p, &mut edited.on, tr!("chatxl-omnifilter-on"));
        ui.add_space(4.0);
        if rule_sets(ui, p, st, &mut edited) {
            open = None;
        }
        ui.add_space(4.0);
        if edited.rules.is_empty() {
            muted(ui, p, tr!("chatxl-omnifilter-empty"));
        }
        let mut action: Option<(usize, i32)> = None;
        egui::ScrollArea::vertical()
            .id_salt("omnifilter_rules")
            .max_height(LIST_H)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let count = edited.rules.len();
                for (i, rule) in edited.rules.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        widgets::checkbox(ui, p, &mut rule.enabled, "");
                        if widgets::icon_button(ui, p, "arrow-up", tr!("chatxl-omnifilter-up"), i > 0) {
                            action = Some((i, -1));
                        }
                        if widgets::icon_button(ui, p, "arrow-down", tr!("chatxl-omnifilter-down"), i + 1 < count) {
                            action = Some((i, 1));
                        }
                        if widgets::icon_button(ui, p, "trash-filled", tr!("common-remove"), true) {
                            action = Some((i, 0));
                        }
                        let name = if rule.name.trim().is_empty() {
                            tr!("chatxl-omnifilter-unnamed").to_owned()
                        } else {
                            rule.name.clone()
                        };
                        let color = if broken.contains(&i) { p.danger } else { p.ink };
                        let text = RichText::new(name).size(12.5).color(color);
                        let label = ui.selectable_label(open == Some(i), text);
                        let label = if broken.contains(&i) {
                            label.on_hover_text(tr!("chatxl-omnifilter-broken"))
                        } else {
                            label
                        };
                        if label.clicked() {
                            open = if open == Some(i) { None } else { Some(i) };
                        }
                    });
                }
            });
        match action {
            Some((i, 0)) => {
                edited.rules.remove(i);
                open = None;
            }
            Some((i, d)) => {
                let j = if d < 0 { i - 1 } else { i + 1 };
                edited.rules.swap(i, j);
                open = Some(j);
            }
            None => {}
        }
        let room = edited.rules.len() < crate::world::omnifilter::MAX_RULES;
        if ui
            .add_enabled_ui(room, |ui| {
                widgets::action_button(
                    ui,
                    p,
                    "plus",
                    tr!("chatxl-omnifilter-add"),
                    widgets::ActionKind::Flat,
                    Vec2::new(0.0, 26.0),
                )
            })
            .inner
            .clicked()
        {
            edited.rules.push(Rule {
                name: tr!("chatxl-omnifilter-new-name", number = edited.rules.len() + 1),
                ..Rule::default()
            });
            open = Some(edited.rules.len() - 1);
        }
        // a Legacy Omnifilter notecard, as a file: a new rule set, as the
        // Legacy Viewer's import from a notecard
        if let Some((name, added)) = poll_import(st) {
            let count = added.len().min(crate::world::omnifilter::MAX_RULES);
            let name = edited.free_name(&name);
            st.omnifilter_note = Some(match edited.add(&name, added) {
                Ok(()) => {
                    open = None;
                    tr!("nb-omnifilter-imported-set", count = count as i64, name = name)
                }
                Err(e) => set_error(e).to_owned(),
            });
        }
        ui.horizontal_wrapped(|ui| {
            let size = Vec2::new(0.0, 26.0);
            let busy = st.omnifilter_import.is_some();
            let sets_room = edited.sets.len() < crate::world::omnifilter::MAX_SETS;
            if ui
                .add_enabled_ui(sets_room && !busy, |ui| {
                    widgets::action_button(ui, p, "file-import", tr!("l2s-omnifilter-import"), widgets::ActionKind::Flat, size)
                })
                .inner
                .clicked()
            {
                st.omnifilter_import = Some(import_file(ui.ctx()));
            }
            if ui
                .add_enabled_ui(!edited.rules.is_empty(), |ui| {
                    widgets::action_button(ui, p, "file-export", tr!("l2s-omnifilter-export"), widgets::ActionKind::Flat, size)
                })
                .inner
                .clicked()
            {
                export_file(&edited.rules, edited.current_name());
            }
            // the same notecard in the inventory (the Legacy Viewer's own
            // place for it)
            if ui
                .add_enabled_ui(sets_room, |ui| {
                    widgets::action_button(
                        ui,
                        p,
                        "file-text-filled",
                        tr!("nb-omnifilter-load-notecard"),
                        widgets::ActionKind::Flat,
                        size,
                    )
                })
                .inner
                .clicked()
            {
                st.omnifilter_notecard = Some(crate::ui::inventory::context_notecards::Mode::Load);
            }
            if ui
                .add_enabled_ui(!edited.rules.is_empty(), |ui| {
                    widgets::action_button(
                        ui,
                        p,
                        "file-text-filled",
                        tr!("nb-omnifilter-save-notecard"),
                        widgets::ActionKind::Flat,
                        size,
                    )
                })
                .inner
                .clicked()
            {
                st.omnifilter_notecard = Some(crate::ui::inventory::context_notecards::Mode::Export);
            }
        });
        if let Some(note) = &st.omnifilter_note {
            muted(ui, p, note);
        }
        if let Some(i) = open
            && let Some(rule) = edited.rules.get_mut(i)
        {
            ui.add_space(6.0);
            widgets::divider(ui, p);
            rule_editor(ui, p, rule, broken.contains(&i));
        }
    });
    st.omnifilter_open = open;
    if Some(&edited) != st.omnifilter.as_ref() {
        st.omnifilter_edit = Some(edited.clone());
        st.omnifilter = Some(edited);
    }
}

/// What a rule set could not become.
fn set_error(e: SetError) -> &'static str {
    match e {
        SetError::NoName => tr!("nb-omnifilter-name-needed"),
        SetError::Taken => tr!("nb-omnifilter-name-taken"),
        SetError::Full => tr!("nb-omnifilter-sets-full"),
    }
}

/// The rule sets (floater_omnifilter.xml's rule set row,
/// OmnifilterEngine::setCurrentRuleSet / addNewRuleSet / addClonedRuleSet /
/// removeCurrentRuleSet): the one in use, a new one empty or copied from
/// it, and the removal of the one in use (never the Default one). True when
/// the set in use changed.
fn rule_sets(ui: &mut egui::Ui, p: &Palette, st: &mut OptionsUi, edited: &mut RuleSet) -> bool {
    let mut switched = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr!("nb-omnifilter-rule-set")).size(12.0).color(p.ink));
        let names: Vec<String> = edited
            .set_names()
            .iter()
            .enumerate()
            .map(|(i, name)| {
                if i == 0 && *name == DEFAULT_SET {
                    tr!("nb-omnifilter-default-set").to_owned()
                } else {
                    (*name).to_owned()
                }
            })
            .collect();
        let choices: Vec<(usize, &str)> = names.iter().enumerate().map(|(i, n)| (i, n.as_str())).collect();
        let mut index = edited.current;
        if combo(ui, &mut index, &choices) {
            edited.select(index);
            switched = true;
        }
        if widgets::icon_button(ui, p, "trash-filled", tr!("nb-omnifilter-remove-set"), edited.current != 0) && edited.remove_current() {
            switched = true;
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut st.omnifilter_set_name)
                .desired_width(COMBO_W)
                .char_limit(crate::world::omnifilter::MAX_SET_NAME)
                .hint_text(tr!("nb-omnifilter-set-name")),
        );
        let named = !st.omnifilter_set_name.trim().is_empty();
        let size = Vec2::new(0.0, 24.0);
        let new = ui
            .add_enabled_ui(named, |ui| {
                widgets::action_button(ui, p, "plus", tr!("nb-omnifilter-new-set"), widgets::ActionKind::Flat, size)
            })
            .inner
            .clicked();
        let copy = ui
            .add_enabled_ui(named, |ui| {
                widgets::action_button(ui, p, "copy-filled", tr!("nb-omnifilter-copy-set"), widgets::ActionKind::Flat, size)
            })
            .inner
            .clicked();
        if new || copy {
            let rules = if copy { edited.rules.clone() } else { Vec::new() };
            match edited.add(&st.omnifilter_set_name, rules) {
                Ok(()) => {
                    st.omnifilter_set_name.clear();
                    st.omnifilter_note = None;
                    switched = true;
                }
                Err(e) => st.omnifilter_note = Some(set_error(e).to_owned()),
            }
        }
    });
    switched
}

/// The notecard's file name (its Legacy inventory name, "[Omnifilter] …"),
/// without the characters a file name may not hold.
fn notecard_file(set: &str) -> String {
    let name = crate::ui::omnifilter_notecards::notecard_name(set);
    let clean: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    format!("{clean}.txt")
}

/// A Legacy Omnifilter notecard chosen and read off the frame thread.
#[allow(clippy::type_complexity)]
fn import_file(ctx: &egui::Context) -> std::sync::mpsc::Receiver<Option<Result<(String, Vec<Rule>), ()>>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    let title = tr!("l2s-omnifilter-import").to_owned();
    let spawned = std::thread::Builder::new().name("omnifilter-import".into()).spawn(move || {
        let result = rfd::FileDialog::new()
            .set_title(title)
            .add_filter("Omnifilter", &["txt", "xml", "notecard"])
            .pick_file()
            .map(|path| {
                use std::io::Read;
                let mut bytes = Vec::new();
                let read = std::fs::File::open(&path).and_then(|f| f.take(legacy::MAX_BYTES as u64 + 1).read_to_end(&mut bytes));
                match read {
                    Ok(_) => legacy::import(&bytes).ok_or(()),
                    Err(e) => {
                        log::warn!("omnifilter: notecard not read: {e}");
                        Err(())
                    }
                }
            });
        let _ = sender.send(result);
        ctx.request_repaint();
    });
    if let Err(e) = spawned {
        log::warn!("omnifilter: import not started: {e}");
    }
    receiver
}

/// The import's answer, once: the rule set's name and rules read (the note
/// says a failure).
fn poll_import(st: &mut OptionsUi) -> Option<(String, Vec<Rule>)> {
    let answer = st.omnifilter_import.as_ref()?.try_recv();
    match answer {
        Err(std::sync::mpsc::TryRecvError::Empty) => None,
        Err(std::sync::mpsc::TryRecvError::Disconnected) | Ok(None) => {
            st.omnifilter_import = None;
            None
        }
        Ok(Some(result)) => {
            st.omnifilter_import = None;
            match result {
                Ok(found) => Some(found),
                Err(()) => {
                    st.omnifilter_note = Some(tr!("l2s-omnifilter-not-notecard").to_owned());
                    None
                }
            }
        }
    }
}

/// The rules written as a Legacy Omnifilter notecard named after their set,
/// to a file chosen off the frame thread.
fn export_file(rules: &[Rule], set: &str) {
    let text = legacy::export(rules, set);
    let file = notecard_file(set);
    let title = tr!("l2s-omnifilter-export").to_owned();
    let spawned = std::thread::Builder::new().name("omnifilter-export".into()).spawn(move || {
        let Some(path) = rfd::FileDialog::new()
            .set_title(title)
            .set_file_name(file)
            .add_filter("Omnifilter", &["txt"])
            .save_file()
        else {
            return;
        };
        if let Err(e) = crate::atomic_file::write(&path, text.as_bytes()) {
            log::warn!("omnifilter: notecard not written: {e}");
        }
    });
    if let Err(e) = spawned {
        log::warn!("omnifilter: export not started: {e}");
    }
}

fn source_label(s: Source) -> &'static str {
    match s {
        Source::NearbyChat => tr!("chatxl-source-nearby"),
        Source::ObjectChat => tr!("chatxl-source-object"),
        Source::Im => tr!("chatxl-source-im"),
        Source::GroupChat => tr!("chatxl-source-group"),
        Source::ObjectIm => tr!("chatxl-source-object-im"),
        Source::GroupNotice => tr!("l2s-source-notice"),
        Source::GroupInvite => tr!("l2s-source-invite"),
        Source::InventoryOffer => tr!("l2s-source-offer"),
        Source::FriendshipOffer => tr!("l2s-source-friendship"),
        Source::Lure => tr!("l2s-source-lure"),
        Source::TeleportRequest => tr!("l2s-source-tp-request"),
        Source::ScriptDialog => tr!("l2s-source-dialog"),
    }
}

fn channel_label(c: Channel) -> &'static str {
    match c {
        Channel::Whisper => tr!("chatxl-channel-whisper"),
        Channel::Say => tr!("chatxl-channel-say"),
        Channel::Shout => tr!("chatxl-channel-shout"),
        Channel::OwnerSay => tr!("chatxl-channel-owner-say"),
        Channel::Direct => tr!("chatxl-channel-direct"),
        Channel::ScriptError => tr!("chatxl-channel-script-error"),
    }
}

/// Ticks for the members of `all` kept in `chosen` (none ticked: all).
fn pick<T: Copy + PartialEq>(ui: &mut egui::Ui, p: &Palette, all: &[T], chosen: &mut Vec<T>, label: fn(T) -> &'static str) {
    ui.horizontal_wrapped(|ui| {
        for item in all {
            let mut on = chosen.contains(item);
            if widgets::checkbox(ui, p, &mut on, label(*item)).changed() {
                if on {
                    chosen.push(*item);
                } else {
                    chosen.retain(|c| c != item);
                }
            }
        }
    });
}

/// A pattern: its text, how it compares, its case.
fn pattern(ui: &mut egui::Ui, p: &Palette, label: &str, pat: &mut Pattern) {
    ui.label(RichText::new(label).size(12.0).color(p.ink));
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut pat.text)
                .desired_width(COMBO_W)
                .hint_text(tr!("chatxl-omnifilter-any")),
        );
        let modes = [
            (tr!("chatxl-mode-exact"), false),
            (tr!("chatxl-mode-substring"), false),
            (tr!("chatxl-mode-regex"), false),
        ];
        let mut i = match pat.mode {
            MatchMode::Exact => 0,
            MatchMode::Substring => 1,
            MatchMode::Regex => 2,
        };
        if widgets::segmented(ui, p, &mut i, &modes) {
            pat.mode = [MatchMode::Exact, MatchMode::Substring, MatchMode::Regex][i.min(2)];
        }
        widgets::checkbox(ui, p, &mut pat.case_sensitive, tr!("chatxl-case-sensitive"));
    });
}

fn rule_editor(ui: &mut egui::Ui, p: &Palette, rule: &mut Rule, broken: bool) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr!("common-name")).size(12.0).color(p.ink));
        ui.add(egui::TextEdit::singleline(&mut rule.name).desired_width(COMBO_W));
    });
    if broken {
        ui.label(RichText::new(tr!("chatxl-omnifilter-broken")).size(11.5).color(p.danger));
    }
    ui.label(RichText::new(tr!("chatxl-omnifilter-sources")).size(12.0).color(p.ink));
    pick(ui, p, &Source::ALL, &mut rule.sources, source_label);
    ui.label(RichText::new(tr!("chatxl-omnifilter-channels")).size(12.0).color(p.ink));
    pick(ui, p, &Channel::ALL, &mut rule.channels, channel_label);
    pattern(ui, p, tr!("chatxl-omnifilter-sender"), &mut rule.sender);
    pattern(ui, p, tr!("chatxl-omnifilter-content"), &mut rule.content);
    ui.label(RichText::new(tr!("chatxl-omnifilter-action")).size(12.0).color(p.ink));
    ui.horizontal(|ui| {
        let actions = [
            (tr!("chatxl-action-hide"), false),
            (tr!("chatxl-action-replace"), false),
            (tr!("chatxl-action-highlight"), false),
            (tr!("chatxl-action-set-aside"), false),
        ];
        let all = [Action::Hide, Action::Replace, Action::Highlight, Action::SetAside];
        let mut i = all.iter().position(|a| *a == rule.action).unwrap_or(0);
        if widgets::segmented(ui, p, &mut i, &actions) {
            rule.action = all[i.min(3)];
        }
        match rule.action {
            Action::Replace => {
                ui.add(
                    egui::TextEdit::singleline(&mut rule.replacement)
                        .desired_width(COMBO_W)
                        .hint_text(tr!("chatxl-replacement-hint")),
                );
            }
            Action::Highlight => {
                swatch(ui, p, "omnifilter-color", &mut rule.color);
            }
            Action::Hide | Action::SetAside => {}
        }
    });
}

/// The filter's list of the session: the rule that matched each message,
/// and the messages set aside.
fn log(ui: &mut egui::Ui, p: &Palette, st: &mut OptionsUi) {
    if st.omnifilter.is_none() {
        muted(ui, p, tr!("chatxl-omnifilter-logged-out"));
        return;
    }
    ui.vertical(|ui| {
        if st.omnifilter_log.is_empty() {
            muted(ui, p, tr!("chatxl-omnifilter-log-empty"));
        }
        egui::ScrollArea::vertical()
            .id_salt("omnifilter_log")
            .max_height(LIST_H)
            .auto_shrink([false, true])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for entry in &st.omnifilter_log {
                    let when = crate::world::chat_log::format::Slt::of(entry.time);
                    let head = format!(
                        "{:02}:{:02} · {} · {}",
                        when.hour,
                        when.minute,
                        entry.rule,
                        source_label(entry.source)
                    );
                    ui.label(RichText::new(head).size(11.5).color(p.muted));
                    if !entry.text.is_empty() {
                        ui.label(RichText::new(format!("{}: {}", entry.from, entry.text)).size(12.5).color(p.ink));
                    }
                }
            });
        if !st.omnifilter_log.is_empty() && widgets::flat_button(ui, p, tr!("common-clear")).clicked() {
            st.omnifilter_clear_log = true;
        }
    });
}
