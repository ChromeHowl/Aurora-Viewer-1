//! The Omnifilter's rule set in an inventory notecard, as the Legacy
//! Viewer's export and import buttons (OmnifilterEngine::
//! exportRuleSetToNotecard, exportNotecardCreatedCallback,
//! validateImportNotecard and importRuleSetFromNotecard,
//! indra/newview/omnifilterengine.cpp, originally LGPL 2.1): the rule set in
//! use is written to a new notecard "[Omnifilter] <name>" of the Notecards
//! folder, which must not hold one of that name; a notecard of the inventory
//! is read into a new rule set. The text is the Legacy notecard's
//! (`world/omnifilter/legacy.rs`), so either viewer reads the other's.
//!
//! The transfers are the inventory's own (LLPreviewNotecard's save and
//! load): a notecard is read only when the agent may copy it, as the
//! notecard preview requires (`context_notecards::available`).

use crate::{
    theme::Palette,
    ui::{
        inventory::context_notecards::{self, Mode, picker::Picker},
        widgets,
    },
    world::{
        inventory::{Inventory, actions},
        omnifilter::{MAX_SET_NAME, Rule, RuleSet, legacy},
    },
};
use aurora_net::{
    NetCommand,
    inventory::{InvItem, notecard::Notecard},
};
use uuid::Uuid;

/// OmnifilterEngine::FILTER_NAME: the start of an Omnifilter notecard's
/// name, which the picker's filter starts with.
pub const FILTER_NAME: &str = "[Omnifilter]";

/// The inventory name of the notecard of the rule set `name`.
pub fn notecard_name(name: &str) -> String {
    let name: String = name.trim().chars().take(MAX_SET_NAME).collect();
    format!("{FILTER_NAME} {name}").chars().take(63).collect()
}

#[derive(Default)]
pub(crate) struct State {
    mode: Option<Mode>,
    name: String,
    picker: Picker,
    selected: Option<Uuid>,
    filter: String,
    saving: Option<Uuid>,
    loading: Option<Uuid>,
    /// What became of the last transfer, for the preferences' note.
    pub message: Option<String>,
}

impl State {
    /// Save (`Mode::Export`, the rule set's name offered) or load.
    pub(crate) fn open(&mut self, mode: Mode, name: &str) {
        if self.busy() {
            return;
        }
        self.mode = Some(mode);
        self.name = name.chars().take(MAX_SET_NAME).collect();
        self.picker = Default::default();
        self.selected = None;
        self.filter = FILTER_NAME.to_owned();
    }

    pub(crate) fn busy(&self) -> bool {
        self.saving.is_some() || self.loading.is_some()
    }

    /// The new notecard of the rules in use, named after `self.name`, in the
    /// Notecards folder (exportRuleSetToNotecard's -1: no such folder, -2:
    /// a notecard of that name is there already).
    fn export(&mut self, inv: &Inventory, set: &RuleSet) -> Result<NetCommand, String> {
        let name = self.name.trim().to_owned();
        if name.is_empty() || self.busy() {
            return Err(aurora_i18n::tr!("nb-omnifilter-name-needed").into());
        }
        let Some(folder) = actions::system(inv, 7) else {
            return Err(aurora_i18n::tr!("nb-omnifilter-no-notecards").into());
        };
        let item_name = notecard_name(&name);
        if inv
            .items
            .values()
            .any(|it| it.name == item_name && actions::under(inv, it.id, folder))
        {
            return Err(aurora_i18n::tr!("nb-omnifilter-notecard-exists", name = item_name));
        }
        let text = legacy::export(&set.rules, &name);
        Notecard::validate_text(&text)?;
        let data = Notecard::decode(text.as_bytes()).and_then(|note| note.encode(&text))?;
        let request = Uuid::new_v4();
        self.saving = Some(request);
        Ok(NetCommand::CreateNotecardItem {
            request,
            parent: folder,
            name: item_name,
            data,
            permissions: None,
        })
    }

    fn load(&mut self, inv: &Inventory, item: Uuid) -> Option<NetCommand> {
        if self.busy() || !inv.can_preview(7) {
            return None;
        }
        let item = context_notecards::available(inv, item)?.clone();
        self.loading = Some(item.id);
        Some(NetCommand::PreviewInventoryItem(item))
    }

    /// The answer to a notecard creation: true when it was ours.
    pub(crate) fn saved(&mut self, request: Uuid, result: &Result<InvItem, String>) -> bool {
        if self.saving != Some(request) {
            return false;
        }
        self.saving = None;
        self.message = Some(match result {
            Ok(item) => aurora_i18n::tr!("nb-omnifilter-saved", name = item.name.clone()),
            Err(reason) => reason.clone(),
        });
        true
    }

    /// A notecard read for us: the rule set's name and rules it holds
    /// (validateImportNotecard), or the reason it gives none.
    pub(crate) fn loaded(&mut self, item: Uuid, result: &Result<Vec<u8>, String>) -> Option<(String, Vec<Rule>)> {
        if self.loading != Some(item) {
            return None;
        }
        self.loading = None;
        let read = result
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|bytes| Notecard::decode(bytes))
            .and_then(|note| legacy::import(note.text.as_bytes()).ok_or_else(|| aurora_i18n::tr!("l2s-omnifilter-not-notecard").into()));
        match read {
            Ok(found) => Some(found),
            Err(reason) => {
                self.message = Some(reason);
                None
            }
        }
    }

    /// The picker or the name to save under, while open; the command to
    /// send, once chosen.
    pub(crate) fn show(&mut self, ctx: &egui::Context, p: &Palette, inv: &mut Inventory, set: Option<&RuleSet>) -> Option<NetCommand> {
        let Some(set) = set else {
            self.mode = None;
            return None;
        };
        let mut command = None;
        match self.mode {
            Some(Mode::Load) => {
                let enabled = !self.busy() && inv.can_preview(7);
                let (close, chosen) = self.picker.show(ctx, p, inv, &mut self.filter, &mut self.selected, enabled);
                if let Some(item) = chosen {
                    command = self.load(inv, item);
                }
                if close || command.is_some() {
                    self.mode = None;
                }
            }
            Some(Mode::Export) => {
                let mut close = false;
                let mut save = false;
                let modal = widgets::Modal::new(egui::Id::unique("omnifilter-notecard-export")).show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.label(egui::RichText::new(aurora_i18n::tr!("nb-omnifilter-save-notecard")).color(p.violet_light));
                    ui.label(aurora_i18n::tr!("nb-omnifilter-save-help"));
                    ui.label(aurora_i18n::tr!("common-name-2"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.name)
                            .char_limit(MAX_SET_NAME)
                            .desired_width(ui.available_width()),
                    );
                    ui.horizontal(|ui| {
                        close |= ui.button(aurora_i18n::tr!("common-cancel")).clicked();
                        save = widgets::default_button(
                            ui.add_enabled(!self.name.trim().is_empty(), egui::Button::new(aurora_i18n::tr!("common-save"))),
                        )
                        .clicked();
                    });
                });
                if save {
                    match self.export(inv, set) {
                        Ok(cmd) => {
                            command = Some(cmd);
                            self.message = Some(aurora_i18n::tr!("notecard-saving").into());
                        }
                        Err(reason) => self.message = Some(reason),
                    }
                    close = true;
                }
                if close || modal.should_close() {
                    self.mode = None;
                }
            }
            None => {}
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::omnifilter::{Action, Rule};

    fn inventory() -> Inventory {
        let mut inv = Inventory {
            root: Uuid::from_u128(1),
            ..Default::default()
        };
        crate::world::inventory::demo::seed(&mut inv, Uuid::from_u128(2));
        // the demo inventory has no Notecards folder: add the system one
        let folder = Uuid::from_u128(9_007);
        inv.folders.insert(
            folder,
            crate::world::inventory::Folder {
                info: aurora_net::inventory::InvFolder {
                    id: folder,
                    parent: inv.root,
                    name: "Notecards".into(),
                    type_default: 7,
                    version: 1,
                    ..Default::default()
                },
                children: Vec::new(),
                items: Vec::new(),
                state: crate::world::inventory::FetchState::Fetched,
                library: false,
            },
        );
        let root = inv.root;
        if let Some(parent) = inv.folders.get_mut(&root) {
            parent.children.push(folder);
        }
        inv
    }

    fn rules() -> RuleSet {
        let mut set = RuleSet {
            on: true,
            rules: vec![Rule {
                name: "spam".into(),
                enabled: true,
                action: Action::Hide,
                ..Rule::default()
            }],
            ..RuleSet::default()
        };
        set.normalize();
        set
    }

    #[test]
    fn the_notecard_is_named_as_the_legacy_viewer_names_it() {
        assert_eq!(notecard_name(" Club "), "[Omnifilter] Club");
        assert!(notecard_name(&"x".repeat(200)).chars().count() <= 63);
    }

    #[test]
    fn saving_goes_to_the_notecards_folder_once_and_refuses_a_taken_name() {
        let mut inv = inventory();
        let folder = actions::system(&inv, 7).expect("Notecards folder");
        let mut state = State::default();
        state.open(Mode::Export, "Club");
        let set = rules();
        let Ok(NetCommand::CreateNotecardItem {
            request,
            parent,
            name,
            data,
            ..
        }) = state.export(&inv, &set)
        else {
            panic!("a notecard is made");
        };
        assert_eq!((parent, name.as_str()), (folder, "[Omnifilter] Club"));
        // what it holds reads back as the same rules, under the set's name
        let note = Notecard::decode(&data).expect("notecard");
        let (read_name, read) = legacy::import(note.text.as_bytes()).expect("Legacy notecard");
        assert_eq!((read_name.as_str(), read.len()), ("Club", 1));
        // one at a time
        assert!(state.export(&inv, &set).is_err());
        let item = InvItem {
            id: Uuid::from_u128(0x77),
            parent: folder,
            name,
            asset_type: 7,
            ..Default::default()
        };
        assert!(!state.saved(Uuid::from_u128(5), &Ok(item.clone())), "not ours");
        assert!(state.saved(request, &Ok(item.clone())));
        inv.add_items(vec![item]);
        assert!(state.export(&inv, &set).is_err(), "the name is taken now");
        state.name = " ".into();
        assert!(state.export(&inv, &set).is_err());
    }

    #[test]
    fn loading_needs_a_copyable_notecard_and_answers_only_its_own_request() {
        let inv = inventory();
        let note = inv
            .items
            .values()
            .find(|it| it.asset_type == 7 && actions::held(&inv, it) & actions::COPY != 0)
            .expect("a notecard")
            .clone();
        let mut state = State::default();
        assert!(state.load(&inv, Uuid::from_u128(0xdead)).is_none());
        assert!(matches!(state.load(&inv, note.id), Some(NetCommand::PreviewInventoryItem(_))));
        assert!(state.load(&inv, note.id).is_none(), "one at a time");
        let text = legacy::export(&rules().rules, "Club");
        let bytes = Notecard::decode(text.as_bytes()).and_then(|n| n.encode(&text)).expect("encoded");
        assert!(state.loaded(Uuid::from_u128(9), &Ok(bytes.clone())).is_none());
        let (name, read) = state.loaded(note.id, &Ok(bytes)).expect("read");
        assert_eq!((name.as_str(), read.len()), ("Club", 1));
        // not an Omnifilter notecard: a message, nothing read
        assert!(state.load(&inv, note.id).is_some());
        let other = Notecard::decode(b"hello").and_then(|n| n.encode("hello")).expect("encoded");
        assert!(state.loaded(note.id, &Ok(other)).is_none());
        assert!(state.message.is_some());
    }
}
