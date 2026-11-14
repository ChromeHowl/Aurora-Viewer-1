// Copyright (c) 2010 Modular Systems. All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice,
//      this list of conditions and the following disclaimer.
//   2. Redistributions in binary form must reproduce the above copyright
//      notice, this list of conditions and the following disclaimer in the
//      documentation and/or other materials provided with the distribution.
//   3. Neither the name Modular Systems nor the names of its contributors may
//      be used to endorse or promote products derived from this software
//      without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY MODULAR SYSTEMS AND CONTRIBUTORS "AS IS" AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL MODULAR SYSTEMS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.

//! `zdrop`, `ztake`, `mtake`, `ktake` and `kcopy` of the chat bar: bulk
//! transfers between the inventory and objects in the world.
//!
//! Port of the `zdrop`, `ztake`, `mtake`, `ktake` and `kcopy` branches of
//! cmd_line_chat and of the timers they start, JCZdrop, JCZtake and TMZtake
//! (indra/newview/chatbar_as_cmdline.cpp, originally Modular Systems BSD,
//! distributed with the Legacy Viewer). This module holds what needs no
//! world: the arguments, the timers as state machines that say what to do
//! at each tick, and the take rule. `app/chat_take.rs` runs them.
//!
//! Differences from the Legacy Viewer, all stricter or clearer:
//! - the Legacy commands send whatever is selected and let the server
//!   refuse; here an object the Take / Take Copy menu entries would grey out
//!   is skipped with a message (`check_take`, the rule of `ObjectFacts`);
//! - `zdrop` goes through `contents_drop::plan`, the checks of an inventory
//!   drop on an object (willObjectAcceptInventory), which the Legacy
//!   `dropInventory` call skips;
//! - `ztake` goes back to looking at the selection after a batch (its
//!   message promises it; the Legacy timer stays in the take state and says
//!   "no objects to take" every 0.66 s, never looking again);
//! - `mtake` renames and takes root objects only (the Legacy timer visits
//!   every prim of the selection, children included);
//! - `zdrop off` says it stopped (the Legacy destructor says "Completed").

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::scan::Scanner;
use crate::ui::context::ObjectFacts;
use crate::ui::context::flags::{AFFECTS_NAVMESH, OBJECT_TRANSFER, OBJECT_YOU_OWNER};
use crate::world::objects::ObjKey;

/// JCZdrop: one item per second.
pub const ZDROP_PERIOD: Duration = Duration::from_secs(1);
/// JCZtake.
pub const ZTAKE_PERIOD: Duration = Duration::from_millis(660);
/// TMZtake.
pub const MTAKE_PERIOD: Duration = Duration::from_millis(330);
/// The countdown of ztake and mtake, in ticks.
const COUNTDOWN: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Zdrop,
    Ztake,
    Mtake,
}

/// What was typed after `zdrop`, `ztake`, `mtake`, `ktake` or `kcopy`.
#[derive(Debug, Clone, PartialEq)]
pub enum TakeCommand {
    /// Nothing after the command: what it does.
    Help(Verb),
    Off(Verb),
    /// Something other than `on` or `off`.
    Invalid(Verb, String),
    /// `zdrop on object folder`: `object` is the word typed, still unchecked.
    ZdropOn {
        object: Option<String>,
        folder: String,
    },
    /// `ztake on folder`, `mtake on folder`.
    On {
        verb: Verb,
        folder: String,
    },
    /// `ktake folder uuid...` (`copy` false) and `kcopy` (`copy` true): the
    /// UUIDs are the words typed, still unchecked.
    Packager {
        copy: bool,
        folder: String,
        objects: Vec<String>,
    },
    /// `ktake` alone: the Legacy Viewer says nothing.
    Silent,
}

/// The rest of the line, one space between the words (the Legacy
/// `folder = folder + tmp + " "` loop, minus the last space).
fn rest_words(i: &mut Scanner) -> String {
    let mut words = Vec::new();
    while let Some(w) = i.word() {
        words.push(w);
    }
    words.join(" ")
}

/// `command` is the first word of the line, `i` reads the rest. `None`
/// when it is not one of these commands.
pub fn parse(command: &str, i: &mut Scanner) -> Option<TakeCommand> {
    let verb = match command {
        "zdrop" => Verb::Zdrop,
        "ztake" => Verb::Ztake,
        "mtake" => Verb::Mtake,
        "ktake" | "kcopy" => {
            let Some(folder) = i.word() else {
                return Some(TakeCommand::Silent);
            };
            let mut objects = Vec::new();
            while let Some(w) = i.word() {
                objects.push(w.to_owned());
            }
            return Some(TakeCommand::Packager {
                copy: command == "kcopy",
                folder: folder.to_owned(),
                objects,
            });
        }
        _ => return None,
    };
    let Some(setting) = i.word() else {
        return Some(TakeCommand::Help(verb));
    };
    Some(match setting {
        "on" if verb == Verb::Zdrop => TakeCommand::ZdropOn {
            object: i.word().map(str::to_owned),
            folder: rest_words(i),
        },
        "on" => TakeCommand::On {
            verb,
            folder: rest_words(i),
        },
        "off" => TakeCommand::Off(verb),
        other => TakeCommand::Invalid(verb, other.to_owned()),
    })
}

/// LLInventoryModel::findCategoryByName: a folder directly under the root,
/// by its exact name; the first one.
pub fn find_root_folder<'a>(children: impl IntoIterator<Item = (Uuid, &'a str)>, name: &str) -> Option<Uuid> {
    children.into_iter().find(|(_, n)| *n == name).map(|(id, _)| id)
}

/// The name mtake gives an object: its dimensions as `%f` prints them with
/// the zeros and the point at the end removed (`1x2.5x0.25`).
pub fn dimension_name(scale: glam::Vec3) -> String {
    let part = |v: f32| {
        let s = format!("{:.6}", f64::from(v));
        let s = s.trim_end_matches('0');
        s.trim_end_matches('.').to_owned()
    };
    format!("{}x{}x{}", part(scale.x), part(scale.y), part(scale.z))
}

/// Why an object is not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Avatars are skipped without a word (get_derezzable_objects).
    Avatar,
    Attachment,
    /// Not ours, or not modifiable and transferable.
    Take,
    /// Not copyable, or not transferable to us.
    Copy,
}

/// What the take rule reads of an object (its root).
#[derive(Debug, Clone, Copy, Default)]
pub struct Candidate {
    /// The update flags of the root.
    pub flags: u32,
    pub avatar: bool,
    /// Worn by us or anyone.
    pub attachment: bool,
    /// We sit on it.
    pub sitting_on: bool,
    /// In the agent's region (a permanent pathfinding object counts there).
    pub in_main_region: bool,
}

/// Whether `ztake`, `mtake` and `ktake` (`copy` false: DRD_TAKE) or `kcopy`
/// (`copy`: DRD_ACQUIRE) may send the object. The rule of the Take and Take
/// Copy menu entries (`ObjectFacts::from_flags`: enable_take,
/// enable_object_take_copy) with what get_derezzable_objects adds: a
/// permanent pathfinding object is not taken (isPermanentEnforced), and a
/// copy needs the transfer right as well unless it is ours
/// (allowTransferTo && permCopy). Stricter than the Legacy commands, which
/// send the message whatever the rights are.
pub fn check_take(c: &Candidate, copy: bool) -> Result<(), Refusal> {
    if c.avatar {
        return Err(Refusal::Avatar);
    }
    if c.attachment {
        return Err(Refusal::Attachment);
    }
    let facts = ObjectFacts::from_flags(c.flags, 0, c.flags, &[c.flags], c.sitting_on, false);
    if copy {
        let ours = c.flags & OBJECT_YOU_OWNER != 0;
        if !facts.take_copy || !(ours || c.flags & OBJECT_TRANSFER != 0) {
            return Err(Refusal::Copy);
        }
    } else if !facts.take || (c.flags & AFFECTS_NAVMESH != 0 && c.in_main_region) {
        return Err(Refusal::Take);
    }
    Ok(())
}

/// What a timer asks the app to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// A line in the local chat.
    Say(String),
    /// DeRezObject of the root `key` into `folder`; `copy` is DRD_ACQUIRE
    /// instead of DRD_TAKE.
    Take { key: ObjKey, folder: Uuid, copy: bool },
    /// ObjectName.
    Rename { key: ObjKey, name: String },
    /// Drop the inventory item `item` into the object `object`.
    Drop { item: Uuid, object: Uuid },
}

fn say(text: impl Into<String>) -> Out {
    Out::Say(text.into())
}

/// JCZdrop: the items of a folder, one per tick, dropped into an object.
#[derive(Debug)]
pub struct Zdrop {
    next: Instant,
    /// Item and name; the last one goes first, as the Legacy stack does.
    stack: Vec<(Uuid, String)>,
    object: Uuid,
    /// The object as typed, for the messages.
    object_text: String,
    folder: String,
}

impl Zdrop {
    pub fn new(now: Instant, items: Vec<(Uuid, String)>, object: Uuid, object_text: String, folder: String) -> Self {
        Self {
            next: now + ZDROP_PERIOD,
            stack: items,
            object,
            object_text,
            folder,
        }
    }

    /// `object_present`: the object is still in the world. The timer is over
    /// when the second value is true.
    pub fn tick(&mut self, now: Instant, object_present: bool) -> (Vec<Out>, bool) {
        if now < self.next {
            return (Vec::new(), false);
        }
        self.next = now + ZDROP_PERIOD;
        let Some((item, name)) = self.stack.pop() else {
            return (Vec::new(), true);
        };
        if !object_present {
            return (
                vec![
                    say(aurora_i18n::tr!("cmdline-take-zdrop-gone", id = self.object_text.clone())),
                    say(aurora_i18n::tr!("cmdline-take-zdrop-gone-why")),
                    say(aurora_i18n::tr!(
                        "cmdline-take-zdrop-aborted",
                        folder = self.folder.clone(),
                        id = self.object_text.clone()
                    )),
                ],
                true,
            );
        }
        let mut out = vec![
            say(aurora_i18n::tr!("cmdline-take-zdrop-transferring", name = name)),
            Out::Drop { item, object: self.object },
        ];
        let done = self.stack.is_empty();
        if done {
            out.push(self.completed());
        }
        (out, done)
    }

    /// The object the items go to.
    pub fn object(&self) -> Uuid {
        self.object
    }

    fn completed(&self) -> Out {
        say(aurora_i18n::tr!(
            "cmdline-take-zdrop-completed",
            folder = self.folder.clone(),
            id = self.object_text.clone()
        ))
    }

    /// `zdrop off`.
    pub fn stopped(&self) -> Out {
        say(aurora_i18n::tr!(
            "cmdline-take-zdrop-stopped",
            folder = self.folder.clone(),
            id = self.object_text.clone()
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TakerState {
    Countdown,
    Selection,
    Take,
}

/// JCZtake: roots taken into a folder, one per tick. Started by `ztake on`
/// (the selection, after a countdown) or by `ktake` / `kcopy` (a list of
/// objects, at once: the "package" mode of the Legacy class).
#[derive(Debug)]
pub struct Taker {
    state: TakerState,
    next: Instant,
    countdown: u32,
    done: HashSet<ObjKey>,
    queue: VecDeque<ObjKey>,
    folder: Uuid,
    copy: bool,
    package: bool,
}

impl Taker {
    /// `ztake on`: the first lines of the constructor.
    pub fn selection(now: Instant, folder: Uuid) -> (Self, Vec<Out>) {
        let taker = Self {
            state: TakerState::Countdown,
            next: now + ZTAKE_PERIOD,
            countdown: COUNTDOWN,
            done: HashSet::new(),
            queue: VecDeque::new(),
            folder,
            copy: false,
            package: false,
        };
        (taker, vec![say(aurora_i18n::tr!("cmdline-take-ztake-activated"))])
    }

    /// `ktake` / `kcopy` with the objects to take.
    pub fn keys(now: Instant, folder: Uuid, copy: bool, keys: Vec<ObjKey>) -> (Self, Vec<Out>) {
        let taker = Self {
            state: TakerState::Take,
            next: now + ZTAKE_PERIOD,
            countdown: 0,
            done: HashSet::new(),
            queue: keys.into(),
            folder,
            copy,
            package: true,
        };
        (taker, vec![say(aurora_i18n::tr!("cmdline-take-packager-started"))])
    }

    /// What the destructor says: `ztake off`.
    pub fn deactivated(&self) -> Option<Out> {
        (!self.package).then(|| say(aurora_i18n::tr!("cmdline-take-ztake-deactivated")))
    }

    /// `selection`: the roots selected now. The timer is over when the
    /// second value is true.
    pub fn tick(&mut self, now: Instant, selection: &[ObjKey]) -> (Vec<Out>, bool) {
        if now < self.next {
            return (Vec::new(), false);
        }
        self.next = now + ZTAKE_PERIOD;
        let mut out = Vec::new();
        match self.state {
            TakerState::Countdown => {
                out.push(say(aurora_i18n::tr!("cmdline-take-countdown", n = self.countdown.to_string())));
                self.countdown -= 1;
                if self.countdown == 0 {
                    self.state = TakerState::Selection;
                }
            }
            TakerState::Selection => {
                for &key in selection {
                    if self.done.insert(key) {
                        self.queue.push_back(key);
                    }
                }
                if !self.queue.is_empty() {
                    self.state = TakerState::Take;
                }
            }
            TakerState::Take => {
                let Some(key) = self.queue.pop_front() else {
                    if self.package {
                        out.push(say(aurora_i18n::tr!("cmdline-take-packager-none")));
                        return (out, true);
                    }
                    self.state = TakerState::Selection;
                    return (out, false);
                };
                out.push(Out::Take {
                    key,
                    folder: self.folder,
                    copy: self.copy,
                });
                let left = self.queue.len();
                if left % 10 == 0 {
                    if left == 0 {
                        if self.package {
                            out.push(say(aurora_i18n::tr!("cmdline-take-ktake-done")));
                            return (out, true);
                        }
                        out.push(say(aurora_i18n::tr!("cmdline-take-ztake-done")));
                        self.state = TakerState::Selection;
                    } else if self.package {
                        out.push(say(aurora_i18n::tr!("cmdline-take-packager-left", n = left.to_string())));
                    } else {
                        out.push(say(aurora_i18n::tr!("cmdline-take-ztake-left", n = left.to_string())));
                    }
                }
            }
        }
        (out, false)
    }
}

/// TMZtake: renames the selected roots to their dimensions, then takes them
/// into a folder, and goes on with what is selected next.
#[derive(Debug)]
pub struct Mtake {
    next: Instant,
    countdown: u32,
    done: HashSet<ObjKey>,
    queue: VecDeque<ObjKey>,
    folder: Uuid,
}

impl Mtake {
    pub fn new(now: Instant, folder: Uuid) -> (Self, Vec<Out>) {
        let mtake = Self {
            next: now + MTAKE_PERIOD,
            countdown: COUNTDOWN,
            done: HashSet::new(),
            queue: VecDeque::new(),
            folder,
        };
        (mtake, vec![say(aurora_i18n::tr!("cmdline-take-mtake-activated"))])
    }

    pub fn deactivated() -> Out {
        say(aurora_i18n::tr!("cmdline-take-mtake-deactivated"))
    }

    /// `selection`: the selected roots with their scale.
    pub fn tick(&mut self, now: Instant, selection: &[(ObjKey, glam::Vec3)]) -> Vec<Out> {
        if now < self.next {
            return Vec::new();
        }
        self.next = now + MTAKE_PERIOD;
        let mut out = Vec::new();
        for &(key, scale) in selection {
            if self.done.insert(key) {
                out.push(Out::Rename {
                    key,
                    name: dimension_name(scale),
                });
                self.queue.push_back(key);
            }
        }
        if self.countdown > 0 {
            out.push(say(aurora_i18n::tr!("cmdline-take-countdown", n = self.countdown.to_string())));
            self.countdown -= 1;
        } else if let Some(key) = self.queue.pop_front() {
            out.push(Out::Take {
                key,
                folder: self.folder,
                copy: false,
            });
            let left = self.queue.len();
            if left % 10 == 0 {
                out.push(say(if left == 0 {
                    aurora_i18n::tr!("cmdline-take-mtake-done").to_string()
                } else {
                    aurora_i18n::tr!("cmdline-take-mtake-left", n = left.to_string())
                }));
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "take_tests.rs"]
mod tests;
