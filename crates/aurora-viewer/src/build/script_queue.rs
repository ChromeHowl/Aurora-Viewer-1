//! The script actions of a selection: "Reset / Run / Stop / Remove
//! scripts" of the object menu (Tools.SelectedScriptAction). Port of
//! queue_actions (indra/newview/llviewermenu.cpp) and LLFloaterScriptQueue
//! with its reset, run, not-run and delete queues
//! (indra/newview/llcompilequeue.cpp), originally LGPL 2.1: every prim of
//! the selection that is scripted and that we may modify, one after the
//! other, its inventory listed, then each of its scripts reset, set
//! running or not, or removed, with the lines of the progress window.
//!
//! Recompile (Mono or LSL) follows LLFloaterCompileQueue::processScript:
//! one script at a time, each skipped unless we may both modify and copy
//! it, its associated experience and its source fetched, then saved again
//! for the chosen target, set running, with the result's lines. Luau is not
//! offered: Aurora does not compile it.

use super::BuildTool;
use super::contents::AT_LSL_TEXT;
use crate::ui::context::flags;
use crate::world::World;
use aurora_net::build::BuildCmd;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// QUEUE_INVENTORY_FETCH_TIMEOUT: an object whose inventory does not come
/// is skipped after this long.
const FETCH_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    Reset,
    Run,
    Stop,
    Delete,
    /// Recompile, to Mono or to LSL ("compile mono" / "compile lsl").
    Compile {
        mono: bool,
    },
}

impl QueueKind {
    /// ResetQueueTitle, RunQueueTitle, NotRunQueueTitle, DeleteQueueTitle.
    pub fn title(self) -> &'static str {
        match self {
            QueueKind::Reset => aurora_i18n::tr!("script-queue-reset-title"),
            QueueKind::Run => aurora_i18n::tr!("script-queue-run-title"),
            QueueKind::Stop => aurora_i18n::tr!("script-queue-stop-title"),
            QueueKind::Delete => aurora_i18n::tr!("script-queue-delete-title"),
            QueueKind::Compile { .. } => aurora_i18n::tr!("b7-compile-title"),
        }
    }

    /// The [START] of "Starting [START] of [COUNT] items." (…QueueStart).
    fn start(self) -> &'static str {
        match self {
            QueueKind::Reset => aurora_i18n::tr!("script-queue-reset-start"),
            QueueKind::Run => aurora_i18n::tr!("script-queue-run-start"),
            QueueKind::Stop => aurora_i18n::tr!("script-queue-stop-start"),
            QueueKind::Delete => aurora_i18n::tr!("script-queue-delete-start"),
            QueueKind::Compile { .. } => aurora_i18n::tr!("b7-compile-start"),
        }
    }

    /// The line of each script ("Resetting: name"…).
    fn line(self, script: &str) -> String {
        let script = script.to_string();
        match self {
            QueueKind::Reset => aurora_i18n::tr!("script-queue-resetting", script = script),
            QueueKind::Run => aurora_i18n::tr!("script-queue-running", script = script),
            QueueKind::Stop => aurora_i18n::tr!("script-queue-not-running", script = script),
            QueueKind::Delete => aurora_i18n::tr!("script-queue-deleting", script = script),
            QueueKind::Compile { .. } => aurora_i18n::tr!("b7-compile-compiling", script = script),
        }
    }

    /// Cannot<msg>SelectObjectsNoScripts / NoPermission, when no prim of the
    /// selection is both scripted and ours to modify.
    pub fn refusal(self, scripted: bool) -> String {
        let first = match self {
            QueueKind::Reset => aurora_i18n::tr!("script-queue-cannot-reset"),
            QueueKind::Run => aurora_i18n::tr!("script-queue-cannot-run"),
            QueueKind::Stop => aurora_i18n::tr!("script-queue-cannot-stop"),
            QueueKind::Delete => aurora_i18n::tr!("script-queue-cannot-delete"),
            QueueKind::Compile { .. } => aurora_i18n::tr!("b7-compile-cannot"),
        };
        let second = if scripted {
            aurora_i18n::tr!("script-queue-select-modifiable-scripts")
        } else {
            aurora_i18n::tr!("script-queue-select-scripts")
        };
        format!("{first}\n\n{second}")
    }
}

/// queue_actions / QueueObjects: the prims `flags_of` reports scripted and
/// modifiable, with their names. Err(scripted) when there is none: whether
/// the last prim looked at was scripted picks the refusal, as there.
pub fn queue_objects(prims: &[(Uuid, u32, String)]) -> Result<Vec<(Uuid, String)>, bool> {
    let mut scripted = false;
    let mut out = Vec::new();
    for (id, update_flags, name) in prims {
        scripted = update_flags & flags::SCRIPTED != 0;
        if scripted && update_flags & flags::OBJECT_MODIFY != 0 {
            out.push((*id, name.clone()));
        }
    }
    if out.is_empty() { Err(scripted) } else { Ok(out) }
}

/// A queue under way and the lines of its window.
#[derive(Debug, Clone)]
pub struct ScriptQueue {
    pub kind: QueueKind,
    pub lines: Vec<String>,
    /// Every object went through ("Done.", the Close button enabled).
    pub done: bool,
    objects: VecDeque<(Uuid, String)>,
    /// The object whose inventory is awaited, and since when.
    current: Option<(Uuid, String, Instant)>,
    /// Recompile: the scripts still to do, and the one under way.
    scripts: VecDeque<Script>,
    compiling: Option<Compiling>,
}

/// A script of the recompile queue: its prim, item, name and whether we
/// may both modify and copy it (LLPermissions::allowModifyBy and
/// allowCopyBy, through `actions::task_agent_mask`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub object: Uuid,
    pub item: Uuid,
    pub name: String,
    pub allowed: bool,
}

/// The step a recompiled script is at: the request awaited.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    Experience(Uuid),
    Source { request: Uuid, experience: Uuid },
    Save(Uuid),
}

#[derive(Debug, Clone)]
struct Compiling {
    script: Script,
    stage: Stage,
    since: Instant,
}

/// What the app must send for the recompile queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileRequest {
    /// The script's associated experience (GetMetadata).
    Experience { request: Uuid, object: Uuid, item: Uuid },
    /// The script's source.
    Source { request: Uuid, object: Uuid, item: Uuid },
    /// The source saved again (UpdateScriptTask): Mono or LSL, running.
    Save {
        request: Uuid,
        object: Uuid,
        item: Uuid,
        mono: bool,
        experience: Uuid,
        source: String,
    },
}

/// Whether we may recompile a script of an object's contents: modify and
/// copy for us (processScript's check), and the object ours to modify.
pub fn may_recompile(item: &aurora_net::build::TaskItem, agent: Uuid, object_modifiable: bool) -> bool {
    use aurora_net::build::perm;
    let mask = crate::world::inventory::actions::task_agent_mask(item, agent);
    object_modifiable && mask & perm::MODIFY != 0 && mask & perm::COPY != 0
}

impl ScriptQueue {
    /// onScriptModifyConfirmation: "Starting [START] of [COUNT] items."
    pub fn new(kind: QueueKind, objects: Vec<(Uuid, String)>) -> ScriptQueue {
        let line = aurora_i18n::tr!("script-queue-starting", start = kind.start().to_string(), count = objects.len());
        ScriptQueue {
            kind,
            lines: vec![line],
            done: false,
            objects: objects.into(),
            current: None,
            scripts: VecDeque::new(),
            compiling: None,
        }
    }

    /// Whether a reply of the network is the recompile queue's.
    pub fn owns(&self, request: Uuid) -> bool {
        self.compiling.as_ref().is_some_and(|c| match c.stage {
            Stage::Experience(r) | Stage::Source { request: r, .. } | Stage::Save(r) => r == request,
        })
    }

    /// The next script of the recompile queue, when none is under way: its
    /// first request, or a skip line for one we may not recompile. A step
    /// awaited for too long is given up (QUEUE_INVENTORY_FETCH_TIMEOUT).
    pub fn next_compile(&mut self, now: Instant) -> Option<CompileRequest> {
        if let Some(c) = &self.compiling {
            if now.duration_since(c.since) <= FETCH_TIMEOUT {
                return None;
            }
            let name = c.script.name.clone();
            self.lines.push(aurora_i18n::tr!("script-queue-timeout", object = name));
            self.compiling = None;
        }
        while let Some(script) = self.scripts.pop_front() {
            if !script.allowed {
                self.lines.push(aurora_i18n::tr!("b7-compile-skipping", script = script.name));
                continue;
            }
            self.lines.push(self.kind.line(&script.name));
            let request = Uuid::new_v4();
            let out = CompileRequest::Experience {
                request,
                object: script.object,
                item: script.item,
            };
            self.compiling = Some(Compiling {
                script,
                stage: Stage::Experience(request),
                since: now,
            });
            return Some(out);
        }
        None
    }

    /// The associated experience came (nil: none, or unknown).
    pub fn experience(&mut self, request: Uuid, experience: Result<Uuid, String>, now: Instant) -> Option<CompileRequest> {
        let c = self.compiling.as_mut().filter(|c| c.stage == Stage::Experience(request))?;
        let next = Uuid::new_v4();
        c.stage = Stage::Source {
            request: next,
            experience: experience.unwrap_or_default(),
        };
        c.since = now;
        Some(CompileRequest::Source {
            request: next,
            object: c.script.object,
            item: c.script.item,
        })
    }

    /// The source came: saved again, or the failure's line.
    pub fn source(&mut self, request: Uuid, source: Result<String, String>, now: Instant) -> Option<CompileRequest> {
        let c = self.compiling.as_mut()?;
        let Stage::Source { request: r, experience } = c.stage else {
            return None;
        };
        if r != request {
            return None;
        }
        let source = match source {
            Ok(source) => source,
            Err(reason) => {
                self.lines.push(format!("{} {}", reason, c.script.name));
                self.compiling = None;
                return None;
            }
        };
        let mono = matches!(self.kind, QueueKind::Compile { mono: true });
        let next = Uuid::new_v4();
        c.stage = Stage::Save(next);
        c.since = now;
        Some(CompileRequest::Save {
            request: next,
            object: c.script.object,
            item: c.script.item,
            mono,
            experience,
            source,
        })
    }

    /// The save came back: "Compilation of … succeeded", or "failed:" with
    /// the grid's lines.
    pub fn saved(&mut self, request: Uuid, result: Result<(bool, Vec<String>), String>) {
        let Some(c) = self.compiling.take_if(|c| c.stage == Stage::Save(request)) else {
            return;
        };
        let name = c.script.name;
        match result {
            Ok((true, _)) => self.lines.push(aurora_i18n::tr!("b7-compile-success", script = name)),
            Ok((false, errors)) => {
                self.lines.push(aurora_i18n::tr!("b7-compile-failure", script = name));
                self.lines.extend(errors.into_iter().map(|e| e.replace('\n', "")));
            }
            Err(reason) => self.lines.push(format!("{reason} {name}")),
        }
    }

    /// The script under way cannot go on (its object is gone).
    pub fn give_up(&mut self) {
        if let Some(c) = self.compiling.take() {
            self.lines.push(aurora_i18n::tr!("script-queue-timeout", object = c.script.name));
        }
    }

    /// Whether the recompile queue is still working.
    fn compile_busy(&self) -> bool {
        self.compiling.is_some() || !self.scripts.is_empty()
    }

    /// objectScriptProcessingQueueCoro, one frame of it: the next object's
    /// inventory is asked for ("Loading inventory for: …"); once it is
    /// there, its scripts get the action.
    pub fn step(&mut self, build: &mut BuildTool, world: &World, now: Instant) {
        while !self.done {
            let Some((id, name, since)) = self.current.clone() else {
                let Some((id, name)) = self.objects.pop_front() else {
                    if self.compile_busy() {
                        return;
                    }
                    self.lines.push(aurora_i18n::tr!("script-queue-done").into());
                    self.done = true;
                    return;
                };
                // an object gone since is skipped without a line
                let Some(idx) = world.objects.index_of_uuid(&id) else {
                    continue;
                };
                build.request_contents(world, idx, false);
                self.lines.push(aurora_i18n::tr!("script-queue-loading", object = name.clone()));
                self.current = Some((id, name, now));
                continue;
            };
            let Some(idx) = world.objects.index_of_uuid(&id) else {
                self.current = None;
                continue;
            };
            let listed = build
                .contents
                .objects
                .get(&id)
                .is_some_and(|inv| !inv.loading && inv.requested.is_some());
            if !listed {
                if now.duration_since(since) > FETCH_TIMEOUT {
                    self.lines.push(aurora_i18n::tr!("script-queue-timeout", object = name));
                    self.current = None;
                    continue;
                }
                // asked again when an answer got lost (contents::RETRY)
                build.request_contents(world, idx, false);
                return;
            }
            self.current = None;
            let items: Vec<&aurora_net::build::TaskItem> = build.contents.objects[&id]
                .items
                .iter()
                .filter(|i| i.asset_type == AT_LSL_TEXT)
                .collect();
            if let QueueKind::Compile { .. } = self.kind {
                let modifiable = build.can_modify(world, idx);
                let agent = world.agent_id;
                self.scripts.extend(items.iter().map(|i| Script {
                    object: id,
                    item: i.item_id,
                    name: i.name.clone(),
                    allowed: may_recompile(i, agent, modifiable),
                }));
                // the scripts go one at a time (next_compile); the next
                // object's inventory waits for them
                return;
            }
            let scripts: Vec<(Uuid, String)> = items.iter().map(|i| (i.item_id, i.name.clone())).collect();
            let Some(o) = world.objects.get(idx) else { continue };
            let handle = o.key.region;
            for (item_id, script) in scripts {
                self.lines.push(self.kind.line(&script));
                match self.kind {
                    QueueKind::Reset => build.send(BuildCmd::ScriptReset {
                        handle,
                        object_id: id,
                        item_id,
                    }),
                    QueueKind::Run | QueueKind::Stop => build.set_script_running(world, idx, item_id, self.kind == QueueKind::Run),
                    QueueKind::Delete => build.remove_item(world, idx, item_id),
                    QueueKind::Compile { .. } => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_scripted_prims_we_may_modify_are_queued() {
        let (a, b, c) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let scripted = flags::SCRIPTED | flags::OBJECT_MODIFY;
        let prims = [
            (a, scripted, "Door".to_string()),
            (b, flags::OBJECT_MODIFY, "Frame".to_string()),
            (c, scripted, "Handle".to_string()),
        ];
        assert_eq!(queue_objects(&prims), Ok(vec![(a, "Door".to_string()), (c, "Handle".to_string())]));
        // nothing scripted / scripted but not ours to modify
        assert_eq!(queue_objects(&[(b, flags::OBJECT_MODIFY, String::new())]), Err(false));
        assert_eq!(queue_objects(&[(a, flags::SCRIPTED, String::new())]), Err(true));
    }

    #[test]
    fn the_refusal_names_the_action_and_what_to_select() {
        let text = QueueKind::Reset.refusal(false);
        assert!(text.starts_with(aurora_i18n::tr!("script-queue-cannot-reset")));
        assert!(text.ends_with(aurora_i18n::tr!("script-queue-select-scripts")));
        assert!(
            QueueKind::Delete
                .refusal(true)
                .ends_with(aurora_i18n::tr!("script-queue-select-modifiable-scripts"))
        );
    }

    #[test]
    fn recompile_needs_modify_and_copy_on_the_script_and_a_modifiable_object() {
        use aurora_net::build::{TaskItem, perm};
        let me = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        let full = perm::MODIFY | perm::COPY | perm::TRANSFER | perm::MOVE;
        let item = |owner: Uuid, owner_mask: u32, everyone_mask: u32, group_owned: bool| TaskItem {
            owner_id: owner,
            owner_mask,
            everyone_mask,
            group_owned,
            ..Default::default()
        };
        // the owner's, full permissions
        assert!(may_recompile(&item(me, full, 0, false), me, true));
        // no copy, no modify, no transfer
        assert!(!may_recompile(&item(me, full & !perm::COPY, 0, false), me, true));
        assert!(!may_recompile(&item(me, full & !perm::MODIFY, 0, false), me, true));
        assert!(may_recompile(&item(me, full & !perm::TRANSFER, 0, false), me, true));
        // someone else's: their everyone mask decides
        assert!(!may_recompile(&item(other, full, 0, false), me, true));
        assert!(may_recompile(&item(other, full, perm::MODIFY | perm::COPY, false), me, true));
        // group-owned: read as someone else's (task_agent_mask)
        assert!(!may_recompile(&item(Uuid::from_u128(3), full, 0, true), me, true));
        // an object we may not modify
        assert!(!may_recompile(&item(me, full, 0, false), me, false));
    }

    #[test]
    fn recompile_goes_script_by_script_through_experience_source_and_save() {
        let now = Instant::now();
        let mut q = ScriptQueue::new(QueueKind::Compile { mono: true }, Vec::new());
        let script = |n: u128, allowed: bool| Script {
            object: Uuid::from_u128(10),
            item: Uuid::from_u128(n),
            name: format!("s{n}"),
            allowed,
        };
        q.scripts.extend([script(1, false), script(2, true), script(3, true)]);
        let Some(CompileRequest::Experience { request, item, .. }) = q.next_compile(now) else {
            panic!()
        };
        assert_eq!(item, Uuid::from_u128(2));
        assert!(q.lines.iter().any(|l| l.contains("s1")), "the refused script has its skip line");
        assert!(q.owns(request) && !q.owns(Uuid::from_u128(99)));
        assert_eq!(q.next_compile(now), None, "one at a time");
        let experience = Uuid::from_u128(77);
        let Some(CompileRequest::Source { request, .. }) = q.experience(request, Ok(experience), now) else {
            panic!()
        };
        let Some(CompileRequest::Save {
            request,
            mono,
            experience: kept,
            source,
            ..
        }) = q.source(request, Ok("default {}".into()), now)
        else {
            panic!()
        };
        assert!(mono);
        assert_eq!(kept, experience, "the associated experience stays");
        assert_eq!(source, "default {}");
        q.saved(request, Ok((false, vec!["(0, 0) : ERROR : x\n".into()])));
        assert_eq!(q.lines.last().map(String::as_str), Some("(0, 0) : ERROR : x"));
        assert!(!q.owns(request));
        // the next one; a lost reply is given up after the timeout
        let Some(CompileRequest::Experience { .. }) = q.next_compile(now) else {
            panic!()
        };
        assert_eq!(q.next_compile(now + FETCH_TIMEOUT + Duration::from_secs(1)), None);
        assert!(!q.compile_busy());
    }
}
