//! `zdrop`, `ztake`, `mtake`, `ktake` and `kcopy` of the chat bar, run
//! against the world and the inventory (port of the matching branches of
//! cmd_line_chat, indra/newview/chatbar_as_cmdline.cpp, see
//! [`crate::cmdline::take`] for the arguments, the timers and the rule).
//!
//! Nothing is sent that the Take, Take Copy or inventory drop of the menus
//! would refuse: objects go through `check_take` and the RLVa take rule,
//! items through `contents_drop::plan`.

use std::time::Instant;

use aurora_net::NetCommand;
use aurora_net::build::{BuildCmd, derez};
use uuid::Uuid;

use super::App;
use crate::cmdline::take::{self, Candidate, Mtake, Out, Refusal, TakeCommand, Taker, Verb, Zdrop};
use crate::world::objects::ObjKey;

/// The running timers (gZDrop, gZTake, gMTake).
#[derive(Default)]
pub(super) struct ChatTake {
    zdrop: Option<Zdrop>,
    taker: Option<Taker>,
    mtake: Option<Mtake>,
}

impl ChatTake {
    pub(super) fn running(&self) -> bool {
        self.zdrop.is_some() || self.taker.is_some() || self.mtake.is_some()
    }
}

impl App {
    /// `Command::Take`.
    pub(super) fn run_take_command(&mut self, cmd: TakeCommand) {
        let now = Instant::now();
        match cmd {
            TakeCommand::Silent => {}
            TakeCommand::Help(Verb::Zdrop) => {
                self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-help-1"));
                self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-help-2"));
            }
            TakeCommand::Help(Verb::Ztake) => {
                self.report_command(aurora_i18n::tr!("cmdline-take-ztake-help-1"));
                self.report_command(aurora_i18n::tr!("cmdline-take-ztake-help-2"));
            }
            TakeCommand::Help(Verb::Mtake) => {
                self.report_command(aurora_i18n::tr!("cmdline-take-mtake-help-1"));
                self.report_command(aurora_i18n::tr!("cmdline-take-mtake-help-2"));
            }
            TakeCommand::Invalid(Verb::Zdrop, setting) => {
                self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-invalid", setting = setting));
            }
            TakeCommand::Invalid(Verb::Ztake, setting) => {
                self.report_command(aurora_i18n::tr!("cmdline-take-ztake-invalid", setting = setting));
            }
            TakeCommand::Invalid(Verb::Mtake, setting) => {
                self.report_command(aurora_i18n::tr!("cmdline-take-mtake-invalid", setting = setting));
            }
            TakeCommand::Off(Verb::Zdrop) => match self.chat_take.zdrop.take() {
                Some(z) => self.take_outputs(vec![z.stopped()]),
                None => self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-already-off")),
            },
            TakeCommand::Off(Verb::Ztake) => match self.chat_take.taker.take() {
                Some(t) => self.take_outputs(t.deactivated().into_iter().collect()),
                None => self.report_command(aurora_i18n::tr!("cmdline-take-ztake-already-off")),
            },
            TakeCommand::Off(Verb::Mtake) => {
                if self.chat_take.mtake.take().is_some() {
                    self.take_outputs(vec![Mtake::deactivated()]);
                } else {
                    self.report_command(aurora_i18n::tr!("cmdline-take-mtake-already-off"));
                }
            }
            TakeCommand::ZdropOn { object, folder } => self.zdrop_on(now, object, folder),
            TakeCommand::On { verb: Verb::Mtake, folder } => {
                if self.chat_take.mtake.is_some() {
                    self.report_command(aurora_i18n::tr!("cmdline-take-mtake-already-active"));
                    return;
                }
                self.report_command(aurora_i18n::tr!("cmdline-take-mtake-begin"));
                self.report_command(aurora_i18n::tr!("cmdline-take-verify-folder"));
                match self.root_folder(&folder) {
                    Some(id) => {
                        self.report_command(aurora_i18n::tr!("cmdline-take-found-destination", folder = folder));
                        let (mtake, out) = Mtake::new(now, id);
                        self.chat_take.mtake = Some(mtake);
                        self.take_outputs(out);
                    }
                    None => {
                        self.report_command(aurora_i18n::tr!("cmdline-take-folder-not-found", name = folder));
                        self.report_command(aurora_i18n::tr!("cmdline-take-mtake-nested"));
                    }
                }
            }
            TakeCommand::On { folder, .. } => {
                if self.chat_take.taker.is_some() {
                    self.report_command(aurora_i18n::tr!("cmdline-take-ztake-already-active"));
                    return;
                }
                self.report_command(aurora_i18n::tr!("cmdline-take-ztake-begin"));
                self.report_command(aurora_i18n::tr!("cmdline-take-verify-folder"));
                match self.root_folder(&folder) {
                    Some(id) => {
                        self.report_command(aurora_i18n::tr!("cmdline-take-found-destination", folder = folder));
                        let (taker, out) = Taker::selection(now, id);
                        self.chat_take.taker = Some(taker);
                        self.take_outputs(out);
                    }
                    None => {
                        self.report_command(aurora_i18n::tr!("cmdline-take-folder-not-found", name = folder));
                        self.report_command(aurora_i18n::tr!("cmdline-take-ztake-nested"));
                    }
                }
            }
            TakeCommand::Packager { copy, folder, objects } => self.packager_on(now, copy, folder, objects),
        }
    }

    /// LLInventoryModel::findCategoryByName: a folder directly under the
    /// inventory root.
    fn root_folder(&self, name: &str) -> Option<Uuid> {
        let inv = &self.world.inventory;
        let root = inv.folders.get(&inv.root)?;
        take::find_root_folder(
            root.children
                .iter()
                .filter_map(|id| inv.folders.get(id))
                .map(|f| (f.info.id, f.info.name.as_str())),
            name,
        )
    }

    /// findInventoryInFolder: every item below the folder, the trash left out.
    fn folder_items(&self, folder: Uuid) -> Vec<(Uuid, String)> {
        let inv = &self.world.inventory;
        let trash = crate::world::inventory::actions::system(inv, 14);
        let mut out = Vec::new();
        let mut stack = vec![folder];
        while let Some(id) = stack.pop() {
            let Some(f) = inv.folders.get(&id) else { continue };
            stack.extend(f.children.iter().copied().filter(|c| Some(*c) != trash));
            out.extend(f.items.iter().filter_map(|i| inv.items.get(i)).map(|it| (it.id, it.name.clone())));
        }
        out
    }

    fn zdrop_on(&mut self, now: Instant, object: Option<String>, folder: String) {
        if self.chat_take.zdrop.is_some() {
            self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-already-active"));
            return;
        }
        let Some(object) = object else {
            self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-need-object"));
            return;
        };
        self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-begin"));
        self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-verify-prim"));
        let Ok(id) = Uuid::parse_str(&object) else {
            self.report_command(aurora_i18n::tr!("cmdline-take-uuid-invalid"));
            return;
        };
        if self.world.objects.index_of_uuid(&id).is_none() {
            self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-not-found"));
            return;
        }
        let Some(folder_id) = self.root_folder(&folder) else {
            self.report_command(aurora_i18n::tr!("cmdline-take-folder-not-found", name = folder));
            self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-nested"));
            return;
        };
        self.report_command(aurora_i18n::tr!("cmdline-take-verify-folder"));
        let items = self.folder_items(folder_id);
        if items.is_empty() {
            self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-empty", folder = folder));
            return;
        }
        self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-found-folder", folder = folder.clone()));
        self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-found-prim", id = object.clone()));
        self.report_command(aurora_i18n::tr!(
            "cmdline-take-zdrop-transferring-from",
            folder = folder.clone(),
            id = object.clone()
        ));
        self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-warning"));
        self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-unselect"));
        self.report_command(aurora_i18n::tr!("cmdline-take-zdrop-stop-hint"));
        self.chat_take.zdrop = Some(Zdrop::new(now, items, id, object, folder));
    }

    fn packager_on(&mut self, now: Instant, copy: bool, folder: String, objects: Vec<String>) {
        let Some(folder_id) = self.root_folder(&folder) else {
            self.report_command(aurora_i18n::tr!("cmdline-take-folder-not-found", name = folder));
            self.report_command(aurora_i18n::tr!("cmdline-take-packager-nested"));
            return;
        };
        let mut keys: Vec<ObjKey> = Vec::new();
        for word in &objects {
            let Ok(id) = Uuid::parse_str(word) else {
                self.report_command(aurora_i18n::tr!("cmdline-take-uuid-invalid"));
                return;
            };
            let Some(key) = self
                .world
                .objects
                .index_of_uuid(&id)
                .map(|idx| crate::build::root_of(&self.world, idx))
                .and_then(|root| self.world.objects.get(root))
                .map(|o| o.key)
            else {
                self.report_command(aurora_i18n::tr!("cmdline-take-k-not-found"));
                return;
            };
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        if keys.is_empty() {
            self.report_command(aurora_i18n::tr!("cmdline-take-no-objects"));
            return;
        }
        if self.chat_take.taker.is_some() {
            self.report_command(aurora_i18n::tr!("cmdline-take-ztake-already-active"));
            return;
        }
        self.report_command(aurora_i18n::tr!("cmdline-take-found-destination", folder = folder));
        let (taker, out) = Taker::keys(now, folder_id, copy, keys);
        self.chat_take.taker = Some(taker);
        self.take_outputs(out);
    }

    /// The selected roots, in selection order.
    fn selected_roots(&self) -> Vec<(ObjKey, glam::Vec3)> {
        self.build
            .roots(&self.world)
            .into_iter()
            .filter_map(|i| self.world.objects.get(i))
            .map(|o| (o.key, o.scale))
            .collect()
    }

    /// Each frame: the timers of the commands that run.
    pub(super) fn poll_chat_take(&mut self) {
        if !self.chat_take.running() {
            return;
        }
        let now = Instant::now();
        if let Some(mut z) = self.chat_take.zdrop.take() {
            let present = self.world.objects.index_of_uuid(&z.object()).is_some();
            let (out, done) = z.tick(now, present);
            if !done {
                self.chat_take.zdrop = Some(z);
            }
            self.take_outputs(out);
        }
        if let Some(mut t) = self.chat_take.taker.take() {
            let selection: Vec<ObjKey> = self.selected_roots().into_iter().map(|(k, _)| k).collect();
            let (out, done) = t.tick(now, &selection);
            if !done {
                self.chat_take.taker = Some(t);
            }
            self.take_outputs(out);
        }
        if let Some(mut m) = self.chat_take.mtake.take() {
            let selection = self.selected_roots();
            let out = m.tick(now, &selection);
            self.chat_take.mtake = Some(m);
            self.take_outputs(out);
        }
    }

    fn take_outputs(&mut self, out: Vec<Out>) {
        for o in out {
            match o {
                Out::Say(text) => self.report_command(text),
                Out::Take { key, folder, copy } => self.take_object(key, folder, copy),
                Out::Rename { key, name } => self.rename_object(key, name),
                Out::Drop { item, object } => self.drop_item(item, object),
            }
        }
    }

    /// DeRezObject of one root, after the take rule: what the Take and Take
    /// Copy entries would grey out is skipped (the Legacy commands send it
    /// and leave the refusal to the server).
    fn take_object(&mut self, key: ObjKey, folder: Uuid, copy: bool) {
        let Some(idx) = self.world.objects.index_of(&key) else {
            return;
        };
        let root = crate::build::root_of(&self.world, idx);
        let Some(o) = self.world.objects.get(root) else {
            return;
        };
        let id = o.full_id.to_string();
        let candidate = Candidate {
            flags: o.update_flags,
            avatar: o.is_avatar(),
            attachment: crate::build::select_type(&self.world, root) != crate::build::SelectType::World,
            sitting_on: self.sitting_on(root),
            in_main_region: self.world.main_region == Some(o.key.region),
        };
        let (handle, local_id) = (o.key.region, o.key.local_id);
        match take::check_take(&candidate, copy) {
            Ok(()) => {}
            Err(Refusal::Avatar) => return,
            Err(Refusal::Attachment) => {
                self.report_command(aurora_i18n::tr!("cmdline-take-skipped-attachment", id = id));
                return;
            }
            Err(Refusal::Take) => {
                self.report_command(aurora_i18n::tr!("cmdline-take-skipped-take", id = id));
                return;
            }
            Err(Refusal::Copy) => {
                self.report_command(aurora_i18n::tr!("cmdline-take-skipped-copy", id = id));
                return;
            }
        }
        // RLVa refuses without a word, like the menus' greyed entries
        if !self.world.rlva_can_take_object(root, copy) {
            return;
        }
        self.send(NetCommand::Build(BuildCmd::DeRez {
            handle,
            local_ids: vec![local_id],
            destination: if copy {
                derez::ACQUIRE_TO_AGENT_INVENTORY
            } else {
                derez::TAKE_INTO_AGENT_INVENTORY
            },
            destination_id: folder,
            group_id: self.world.groups.active,
        }));
    }

    /// We sit on the linkset of `root`.
    fn sitting_on(&self, root: usize) -> bool {
        let w = &self.world;
        w.agent.seated
            && w.objects
                .index_of_uuid(&w.agent_id)
                .and_then(|i| w.objects.get(i))
                .and_then(|me| w.objects.parent_of(me))
                .is_some_and(|seat| crate::build::root_of(w, seat) == root)
    }

    /// ObjectName, only where we may modify the object (the server's check,
    /// done here too; RLVa's `@edit` is applied when the message is sent).
    fn rename_object(&mut self, key: ObjKey, name: String) {
        let Some(o) = self.world.objects.index_of(&key).and_then(|i| self.world.objects.get(i)) else {
            return;
        };
        if o.update_flags & crate::ui::context::flags::OBJECT_MODIFY == 0 {
            return;
        }
        self.send(NetCommand::Build(BuildCmd::SetName {
            handle: key.region,
            local_ids: vec![key.local_id],
            name,
        }));
    }

    /// LLToolDragAndDrop::dropInventory into the object, after the checks
    /// of an inventory drop (`contents_drop::plan`: willObjectAcceptInventory,
    /// worn and no-transfer items).
    fn drop_item(&mut self, item: Uuid, object: Uuid) {
        use crate::build::contents_drop::{self, Target};
        let Some(idx) = self.world.objects.index_of_uuid(&object) else {
            return;
        };
        let Some(name) = self.world.inventory.items.get(&item).map(|i| i.name.clone()) else {
            return;
        };
        match contents_drop::plan(&self.world, &[item], idx, true, Target::Panel) {
            Ok(drops) => {
                self.build.drop_inventory(&mut self.world, idx, drops);
                self.flush_build();
            }
            Err(reason) => {
                self.report_command(aurora_i18n::tr!(
                    "cmdline-take-drop-refused",
                    name = name,
                    reason = contents_drop::reason_text(reason).to_owned()
                ));
            }
        }
    }
}
