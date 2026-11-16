//! "Import scripts": many script files of the user's disk become scripts of
//! the inventory (or of an object's contents), one after another. An Aurora
//! addition: the Legacy Viewer loads one file into an open editor
//! (LLScriptEdCore::onBtnLoadFromFile) and its bulk upload knows no script
//! (indra/newview/llviewermenufile.cpp, upload_bulk).
//!
//! Each script is made as LLScriptRecoverQueue does (recoverNext,
//! onCreateScript, onSavedScript; indra/newview/llfloaterscriptrecover.cpp,
//! originally LGPL 2.1): an item is created, its text goes up through
//! UpdateScriptAgent once the grid names the item, and the next script
//! starts only when the answer is in (the Legacy Viewer's upload pool has
//! one slot, `PoolSizeUpload`). In an object, the saved script is then put in
//! with the drop of a script (`contents_drop::plan`, LLToolDragAndDrop::
//! dad3dRezScript, LLViewerObject::saveScript), the way the Legacy Viewer's
//! particle editor injects one; the same checks as a drop apply, so does RLVa.

use super::*;
use crate::build::{BuildTool, contents_drop};
use crate::ui::scripts::import::{self, Action, Destination};
use crate::ui::scripts::import_rules::{self as import_rules, Kind, Loaded, Skip, State};
use crate::world::World;
use crate::world::inventory::{FetchState, actions};
use aurora_net::inventory::InvItem;
use aurora_net::scripts::{ScriptSaved, ScriptTarget};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Longer than the network's two 60 second waits (the item's creation, the
/// text's upload): a row that hears nothing for this long is given up.
const TIMEOUT: Duration = Duration::from_secs(150);
/// How long the destination folder may take to load before the numbers of
/// the new names are decided without all of its names.
const FOLDER_WAIT: Duration = Duration::from_secs(15);
/// FT_LSL_TEXT.
const FT_LSL_TEXT: i32 = 10;

/// What the file worker sends.
pub(super) enum Message {
    File(Loaded),
    /// Files of the pick left unread (more than `MAX_PER_PICK`).
    TooMany(usize),
}

#[derive(Default)]
pub(super) struct Import {
    pub form: import::Form,
    job: Option<mpsc::Receiver<Message>>,
    current: Option<Current>,
    /// Since when the destination folder is being waited for.
    waiting_since: Option<Instant>,
}

/// The script being made.
struct Current {
    request: Uuid,
    started: Instant,
    /// The name to give it once its text is saved ("New Script").
    rename: Option<String>,
}

/// The files of `paths`, read one after another (the worker's job).
pub(super) fn read_paths(paths: Vec<PathBuf>, convert_tabs: bool, tx: &mpsc::Sender<Message>) {
    let total = paths.len();
    for path in paths.into_iter().take(import_rules::MAX_PER_PICK) {
        let result = import_rules::read_file(&path, convert_tabs);
        if tx.send(Message::File(Loaded { path, result })).is_err() {
            return;
        }
    }
    if total > import_rules::MAX_PER_PICK {
        let _ = tx.send(Message::TooMany(total - import_rules::MAX_PER_PICK));
    }
}

/// A script of ours as the import makes it, for the checks that ask what an
/// object would be given before anything is created: full rights, as every
/// item created from our own file has.
fn probe(agent: Uuid) -> InvItem {
    InvItem {
        asset_type: i32::from(crate::build::contents::AT_LSL_TEXT),
        inv_type: i32::from(crate::build::contents::IT_LSL),
        owner: agent,
        base_mask: contents_drop::UNRESTRICTED,
        owner_mask: contents_drop::UNRESTRICTED,
        ..Default::default()
    }
}

/// Whether the object `object` takes new scripts now, with the checks of the
/// Contents tab (LLPanelContents::getState: modify rights, owner or group
/// member, RLVa) and of a drop of a script (willObjectAcceptInventory:
/// `contents_drop::accepts`, which also refuses a locked attachment and one
/// that takes only unrestricted items). Asked before anything is created.
pub(crate) fn object_gate(world: &World, build: &BuildTool, object: Uuid) -> Result<(), &'static str> {
    let idx = world.objects.index_of_uuid(&object).ok_or("rez-no-surface")?;
    if !build.contents_editable(world, idx) {
        return Err("rez-contents-denied");
    }
    contents_drop::accepts(world, idx, &probe(world.agent_id))
}

/// The drop that puts the saved script `item` into the object `object`: the
/// plan of the Contents tab's drop, `run` meaning the script runs once in
/// (the drop without Ctrl, LLToolDragAndDrop::dropScript).
pub(crate) fn place_plan(
    world: &World,
    build: &BuildTool,
    object: Uuid,
    item: Uuid,
    run: bool,
) -> Result<Vec<contents_drop::ContentsDrop>, &'static str> {
    let idx = world.objects.index_of_uuid(&object).ok_or("rez-no-surface")?;
    if !build.contents_editable(world, idx) {
        return Err("rez-contents-denied");
    }
    contents_drop::plan(world, &[item], idx, !run, contents_drop::Target::Panel)
}

impl App {
    /// The window, for the inventory folder `folder` ("New" menu).
    pub(super) fn open_script_import_folder(&mut self, folder: Uuid) {
        if !actions::destination(&self.world.inventory, folder) {
            self.script_import.form.note = aurora_i18n::tr!("script-import-dest-refused").into();
            self.script_import.form.open = true;
            return;
        }
        let form = &mut self.script_import.form;
        if !form.batch.running() {
            form.destination = Destination::Folder(folder);
            form.note.clear();
        }
        form.open = true;
    }

    /// The window, for the contents of the object `object` (Build > Contents).
    pub(super) fn open_script_import_object(&mut self, object: Uuid) {
        if let Err(reason) = object_gate(&self.world, &self.build, object) {
            self.world
                .system_notice(contents_drop::reason_text(reason), crate::world::notice_topics::Topic::Warning);
            return;
        }
        let name = self
            .build
            .props
            .get(&object)
            .map(|props| props.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| aurora_i18n::tr!("script-import-this-object").to_owned());
        let form = &mut self.script_import.form;
        if !form.batch.running() {
            form.destination = Destination::Object { object, name };
            form.note.clear();
        }
        form.open = true;
    }

    /// The window, and the work behind it, every frame.
    pub(super) fn show_script_import(&mut self, ctx: &egui::Context, p: &Palette) {
        self.script_import.form.lua_region = self.world.main().is_some_and(|region| region.lua_scripts);
        self.script_import_drain();
        self.script_import_advance();
        if let Some(action) = import::window(ctx, p, &mut self.script_import.form, &self.world.inventory) {
            self.script_import_action(action);
        }
    }

    fn script_import_action(&mut self, action: Action) {
        match action {
            Action::AddFiles => self.script_import_pick(),
            Action::Import => self.script_import_start(),
            Action::Stop => self.script_import.form.batch.cancel(),
            Action::Clear => self.script_import.form.batch.clear(true),
            Action::Open(item) => self.open_script(ScriptTarget {
                object: Uuid::nil(),
                item,
            }),
        }
    }

    /// The file dialog, with several files to pick (LLFilePicker's script
    /// filter, `FFLOAD_SCRIPT`), and their reading, off the frame thread.
    fn script_import_pick(&mut self) {
        if self.script_import.job.is_some() {
            return;
        }
        let convert = self.settings.script_editor.convert_tabs;
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("aurora-script-import".into()).spawn(move || {
            let Some(paths) = rfd::FileDialog::new()
                .add_filter("LSL / SLua", &import_rules::EXTENSIONS)
                .pick_files()
            else {
                return;
            };
            read_paths(paths, convert, &tx);
        });
        match spawned {
            Ok(_) => {
                self.script_import.job = Some(rx);
                self.script_import.form.loading = true;
            }
            Err(e) => log::warn!("script import: the file dialog could not start ({e})"),
        }
    }

    /// Files from the worker into the list.
    fn script_import_drain(&mut self) {
        let Some(rx) = self.script_import.job.as_ref() else { return };
        let (mut left_out, mut unread, mut done) = (0usize, 0usize, false);
        loop {
            match rx.try_recv() {
                Ok(Message::File(loaded)) => {
                    if !self.script_import.form.batch.add(loaded) {
                        left_out += 1;
                    }
                }
                Ok(Message::TooMany(count)) => unread += count,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
            }
        }
        let form = &mut self.script_import.form;
        if left_out + unread > 0 {
            form.note = aurora_i18n::tr!("script-import-left-out", count = left_out + unread);
        }
        if done {
            self.script_import.job = None;
            self.script_import.form.loading = false;
        }
    }

    /// The Import button: every ready row waits for its turn, once the
    /// destination takes scripts.
    fn script_import_start(&mut self) {
        let refused = match &self.script_import.form.destination {
            Destination::Folder(folder) => {
                (!actions::destination(&self.world.inventory, *folder)).then(|| aurora_i18n::tr!("script-import-dest-refused").to_owned())
            }
            Destination::Object { object, .. } => object_gate(&self.world, &self.build, *object)
                .err()
                .map(|reason| contents_drop::reason_text(reason).to_owned()),
        };
        let form = &mut self.script_import.form;
        if let Some(reason) = refused {
            form.note = reason;
            return;
        }
        form.note.clear();
        form.batch.start();
        self.script_import.waiting_since = None;
    }

    /// The folder the scripts are made in: the chosen one, or the Scripts
    /// folder for an object (the Legacy Viewer's `findCategoryUUIDForType`
    /// for a new script, then the drop into the object).
    fn script_import_folder(&self) -> Option<Uuid> {
        match &self.script_import.form.destination {
            Destination::Folder(folder) => Some(*folder),
            Destination::Object { .. } => actions::system(&self.world.inventory, FT_LSL_TEXT),
        }
    }

    /// Every row that waits fails with `reason`.
    fn script_import_fail_queued(&mut self, reason: &str) {
        let batch = &mut self.script_import.form.batch;
        while let Some(index) = batch.next() {
            batch.finish(
                index,
                State::Failed {
                    reason: reason.to_owned(),
                    kept: false,
                },
            );
        }
        self.script_import_report();
    }

    /// The next row, when none works.
    fn script_import_advance(&mut self) {
        if self
            .script_import
            .current
            .as_ref()
            .is_some_and(|current| current.started.elapsed() > TIMEOUT)
        {
            self.script_import.current = None;
            if let Some(index) = self.script_import.form.batch.working() {
                log::warn!("script import: the grid did not answer for a script");
                self.script_import.form.batch.finish(
                    index,
                    State::Failed {
                        reason: aurora_i18n::tr!("script-import-timeout").into(),
                        kept: false,
                    },
                );
                self.script_import_report();
            }
        }
        if self.script_import.current.is_some() || self.script_import.form.batch.idle() {
            return;
        }
        let Some(folder) = self
            .script_import_folder()
            .filter(|folder| actions::destination(&self.world.inventory, *folder))
        else {
            self.script_import_fail_queued(aurora_i18n::tr!("script-import-dest-refused"));
            return;
        };
        // the names the new ones must differ from are the folder's own
        let loaded = self
            .world
            .inventory
            .folders
            .get(&folder)
            .is_some_and(|f| matches!(f.state, FetchState::Fetched | FetchState::Failed));
        if !loaded {
            self.world.inventory.request(folder);
            let since = *self.script_import.waiting_since.get_or_insert_with(Instant::now);
            if since.elapsed() < FOLDER_WAIT {
                return;
            }
        }
        let object = match &self.script_import.form.destination {
            Destination::Object { object, .. } => Some(*object),
            Destination::Folder(_) => None,
        };
        if let Some(object) = object
            && let Err(reason) = object_gate(&self.world, &self.build, object)
        {
            self.script_import_fail_queued(contents_drop::reason_text(reason));
            return;
        }
        let Some(index) = self.script_import.form.batch.next() else { return };
        let (stem, slua, source) = {
            let row = &mut self.script_import.form.batch.rows[index];
            (row.name.clone(), row.slua, row.source.take())
        };
        let form = &self.script_import.form;
        if slua && !form.lua_region {
            self.script_import
                .form
                .batch
                .finish(index, State::Skipped(Skip::NoSlua));
            self.script_import_report();
            return;
        }
        let Some(source) = source else {
            self.script_import.form.batch.finish(
                index,
                State::Failed {
                    reason: aurora_i18n::tr!("script-import-state-cancelled").into(),
                    kept: false,
                },
            );
            self.script_import_report();
            return;
        };
        let kind = if slua { Kind::Slua } else { form.lsl };
        let existing: Vec<String> = self
            .world
            .inventory
            .folders
            .get(&folder)
            .map(|f| {
                f.items
                    .iter()
                    .filter_map(|id| self.world.inventory.items.get(id))
                    .map(|item| item.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let made: Vec<String> = form.batch.rows.iter().filter_map(|row| row.made_as.clone()).collect();
        let taken = import_rules::taken_names(existing.iter().map(String::as_str), made.iter().map(String::as_str));
        let name = import_rules::numbered(&stem, &taken);
        let creation = import_rules::creation_name(&name);
        let rename = (creation != name).then(|| name.clone());
        self.script_import.form.batch.rows[index].made_as = Some(name);
        let request = Uuid::new_v4();
        self.script_import.current = Some(Current {
            request,
            started: Instant::now(),
            rename,
        });
        // the creation defaults are applied on the way out (`Defaults::prepare`)
        self.send(NetCommand::ImportScriptItem {
            request,
            parent: folder,
            name: creation,
            description: super::script_recovery::description(chrono::Local::now()),
            source,
            compile: kind.compile(),
            permissions: None,
        });
    }

    /// The answer to the script being made.
    pub(super) fn script_import_event(&mut self, request: Uuid, item: Uuid, result: Result<(InvItem, ScriptSaved), String>) {
        if self.script_import.current.as_ref().is_none_or(|current| current.request != request) {
            return;
        }
        let Some(current) = self.script_import.current.take() else { return };
        let Some(index) = self.script_import.form.batch.working() else { return };
        let object = match &self.script_import.form.destination {
            Destination::Object { object, .. } => Some(*object),
            Destination::Folder(_) => None,
        };
        let run = self.script_import.form.run;
        let (state, item, placed) = match result {
            Ok((mut saved, report)) => {
                log::info!("script import: a script saved ({} compile error(s))", report.errors.len());
                self.world.inventory.add_items(vec![saved.clone()]);
                let mut state = import_rules::compile_state(report.compiled, report.checked, report.errors);
                // onSavedScript: back to the name the server could not take
                if let Some(name) = current.rename
                    && let Ok(change) = actions::patch(&self.world.inventory, saved.id, aurora_llsd::llsd_map! { "name" => name.as_str() })
                {
                    self.apply_inventory_action(crate::ui::inventory::InvAction::Edit(change));
                    saved.name = name;
                }
                // set_default_permissions: the group and everyone rights of
                // the creation defaults, once the text is saved and the item
                // is known with its asset (not while the upload is on its way)
                let rights = self.settings.inventory.creation_permissions.scripts.permissions();
                if (rights.group, rights.everyone) != (saved.group_mask, saved.everyone_mask) {
                    self.send(NetCommand::SetItemRights {
                        item: saved.clone(),
                        rights: aurora_net::inventory::upload::Permissions {
                            next_owner: saved.next_owner_mask,
                            group: rights.group,
                            everyone: rights.everyone,
                        },
                    });
                }
                if !self.demo {
                    self.send(NetCommand::FetchItems {
                        items: vec![saved.id],
                        owner: self.world.agent_id,
                    });
                }
                let mut placed = false;
                if let Some(object) = object
                    && matches!(state, State::Saved | State::Unchecked)
                {
                    match self.script_import_place(object, saved.id, run) {
                        Ok(()) => placed = true,
                        Err(reason) => {
                            state = State::Failed {
                                reason: aurora_i18n::tr!("script-import-not-added", reason = contents_drop::reason_text(reason)),
                                kept: true,
                            };
                        }
                    }
                }
                (state, Some(saved.id), placed)
            }
            Err(reason) => {
                log::warn!("script import: a script was not saved: {reason}");
                if !item.is_nil() && !self.demo {
                    self.send(NetCommand::FetchItems {
                        items: vec![item],
                        owner: self.world.agent_id,
                    });
                }
                (
                    State::Failed {
                        reason,
                        kept: !item.is_nil(),
                    },
                    (!item.is_nil()).then_some(item),
                    false,
                )
            }
        };
        let batch = &mut self.script_import.form.batch;
        if let Some(row) = batch.rows.get_mut(index) {
            row.item = item;
            row.placed = placed;
        }
        batch.finish(index, state);
        self.script_import_report();
    }

    /// The saved script `item` into the object: the drop's commands, sent
    /// through the same boundary as any (RLVa included). A script that
    /// stays in the inventory is still there if the object refuses it.
    fn script_import_place(&mut self, object: Uuid, item: Uuid, run: bool) -> Result<(), &'static str> {
        let drops = place_plan(&self.world, &self.build, object, item, run)?;
        // The script is ours with full rights: a copy goes in, nothing
        // leaves the inventory (`ContentsDrop::consume` is only for an item
        // that cannot be copied).
        for drop in drops {
            self.send(NetCommand::Build(drop.command));
        }
        self.build.contents_changed(object);
        Ok(())
    }

    /// When the list has been worked through: one line in the chat's notices.
    fn script_import_report(&mut self) {
        if !self.script_import.form.batch.idle() {
            return;
        }
        let s = self.script_import.form.batch.summary();
        if s.saved + s.failed + s.skipped == 0 {
            return;
        }
        let problems = s.compile_errors + s.failed + s.skipped;
        let topic = if problems > 0 {
            crate::world::notice_topics::Topic::Warning
        } else {
            crate::world::notice_topics::Topic::Items
        };
        self.world
            .system_notice(aurora_i18n::tr!("script-import-done", saved = s.saved, problems = problems), topic);
    }

    /// The demo's grid for an imported script: the item at once, with the
    /// compile answer a marker in the text asks for (the demo `SaveScript`'s
    /// `AURORA_DEMO_SYNTAX_ERROR`, one error per line holding it), and a
    /// refusal for `AURORA_DEMO_IMPORT_FAIL`, which leaves the empty item.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn demo_import_script(
        &mut self,
        request: Uuid,
        parent: Uuid,
        name: &str,
        description: &str,
        source: &str,
        compile: aurora_net::scripts::CompileTarget,
        permissions: Option<aurora_net::inventory::upload::Permissions>,
    ) {
        let rights = permissions.unwrap_or_default();
        let fail = source.contains("AURORA_DEMO_IMPORT_FAIL");
        let item = InvItem {
            id: Uuid::new_v4(),
            parent,
            name: name.to_owned(),
            desc: description.to_owned(),
            asset_type: 10,
            inv_type: 10,
            flags: u32::from(compile == aurora_net::scripts::CompileTarget::Luau),
            asset_id: if fail { Uuid::nil() } else { Uuid::new_v4() },
            owner: self.world.agent_id,
            creator: self.world.agent_id,
            base_mask: 0x7fff_ffff,
            owner_mask: 0x7fff_ffff,
            next_owner_mask: rights.next_owner,
            ..Default::default()
        };
        self.world.inventory.add_items(vec![item.clone()]);
        if fail {
            self.on_app_event(NetEvent::ScriptItemImported {
                request,
                item: item.id,
                result: Err(aurora_i18n::tr!("script-import-demo-refused").into()),
            });
            return;
        }
        self.demo_script_sources.insert(
            ScriptTarget {
                object: Uuid::nil(),
                item: item.id,
            },
            source.to_owned(),
        );
        let errors: Vec<String> = source
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains("AURORA_DEMO_SYNTAX_ERROR"))
            .map(|(number, _)| format!("({}, 9) : ERROR : Syntax error", number + 1))
            .collect();
        let saved = ScriptSaved {
            asset: item.asset_id,
            compiled: errors.is_empty(),
            checked: true,
            errors,
        };
        self.on_app_event(NetEvent::ScriptItemImported {
            request,
            item: item.id,
            result: Ok((item, saved)),
        });
    }

    /// AURORA_DEMO_SCRIPT_IMPORT: the files the window would be given, made
    /// by the demo itself (no file dialog), then imported. `object` goes to
    /// the contents of the demo's action object, `slua` on a region that runs
    /// SLua.
    pub(super) fn demo_script_import_steps(&mut self) {
        let Ok(mode) = std::env::var("AURORA_DEMO_SCRIPT_IMPORT") else { return };
        match self.frame_count {
            300 => {
                crate::world::inventory::demo::seed(&mut self.world.inventory, self.world.agent_id);
                if mode == "slua"
                    && let Some(region) = self.world.main_region.and_then(|h| self.world.regions.get_mut(&h))
                {
                    region.lua_scripts = true;
                }
                if mode == "object" {
                    let object = crate::demo::action_id(970);
                    let agent = self.world.agent_id;
                    if let Some(idx) = self.world.objects.index_of_uuid(&object)
                        && let Some(o) = self.world.objects.get_mut(idx)
                    {
                        o.owner_id = agent;
                        o.update_flags |= crate::ui::context::flags::OBJECT_MODIFY | crate::ui::context::flags::OBJECT_YOU_OWNER;
                    }
                    self.open_script_import_object(object);
                } else {
                    self.open_script_import_folder(Uuid::from_u128(8000));
                }
                let dir = demo_files();
                let paths: Vec<PathBuf> = demo_names().iter().map(|name| dir.join(name)).collect();
                let (tx, rx) = mpsc::channel();
                read_paths(paths, false, &tx);
                drop(tx);
                self.script_import.job = Some(rx);
            }
            340 => self.script_import_start(),
            _ => {}
        }
    }
}

/// The files of the demo, in the order they are listed.
fn demo_names() -> [&'static str; 11] {
    [
        "door.lsl",
        "hud menu.lsl",
        "particles.luau",
        "New Script.lsl",
        "lamp.lsl",
        "lamp.txt",
        "m\u{fc}ller.lsl",
        "broken.lsl",
        "empty.lsl",
        "binary.lsl",
        "refused.lsl",
    ]
}

/// The demo's files, written to a folder of their own below the temporary
/// folder (the viewer never reads another).
fn demo_files() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aurora-demo-script-import-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("script import demo: no folder ({e})");
        return dir;
    }
    let door = "default\n{\n    state_entry()\n    {\n        llSay(0, \"Hello\");\n    }\n}\n";
    let files: [(&str, Vec<u8>); 11] = [
        ("door.lsl", door.as_bytes().to_vec()),
        // a BOM and Windows line ends, with two lines the demo grid rejects
        (
            "hud menu.lsl",
            b"\xef\xbb\xbfdefault\r\n{\r\n    state_entry()\r\n    {\r\n        AURORA_DEMO_SYNTAX_ERROR\r\n        AURORA_DEMO_SYNTAX_ERROR\r\n    }\r\n}\r\n".to_vec(),
        ),
        ("particles.luau", b"local function main()\n    print(\"hello\")\nend\nmain()\n".to_vec()),
        ("New Script.lsl", door.as_bytes().to_vec()),
        ("lamp.lsl", door.as_bytes().to_vec()),
        ("lamp.txt", door.as_bytes().to_vec()),
        ("m\u{fc}ller.lsl", door.as_bytes().to_vec()),
        ("broken.lsl", vec![b'd', 0xe9, b'f']),
        ("empty.lsl", Vec::new()),
        ("binary.lsl", vec![b'a', 0, b'b']),
        ("refused.lsl", b"// AURORA_DEMO_IMPORT_FAIL\ndefault { state_entry() {} }\n".to_vec()),
    ];
    for (name, bytes) in files {
        if let Err(e) = std::fs::write(dir.join(name), bytes) {
            log::warn!("script import demo: {name} not written ({e})");
        }
    }
    dir
}

#[cfg(test)]
#[path = "script_import_tests.rs"]
mod tests;
