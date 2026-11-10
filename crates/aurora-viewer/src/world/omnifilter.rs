//! The Omnifilter: the user's own rules over the messages that arrive, each
//! one hiding, rewriting, highlighting or setting aside the nearby chat
//! lines, objects' lines and instant messages it matches. Port of the
//! matching of OmnifilterEngine::match (indra/newview/omnifilterengine.cpp,
//! originally LGPL 2.1, Copyright (C) 2025, Zi Ree @ Second Life) and of its
//! calls in process_chat_from_simulator (llviewermessage.cpp) and
//! LLIMProcessing::processNewMessage (llimprocessing.cpp).
//!
//! As in the Legacy Viewer: the rules are tried in their order and the first
//! enabled one that matches decides; an empty field matches anything; the
//! sender's name must match as a whole ("match full text", a regular
//! expression over the whole name), the text only somewhere in it; a rule
//! with a replacement shows "Omnifilter Replacement: …" in place of the
//! message, one without hides it. The filter is off by default.
//!
//! It generalises the fixed rule of `object_im_filter.rs` ("Block object
//! messages from elsewhere"), which stays as it is, applied first.
//!
//! Differences, for the user:
//! - the rules are kept per account (`omnifilter.json` of the account's
//!   folder) where the Legacy Viewer has one file for every account;
//! - besides hiding and replacing, a rule may highlight the message in a
//!   colour of its own, or set it aside in the filter's list of the session
//!   (Preferences › Chat › Translation and filter) instead of the chat;
//! - a rule may also name the volume or channel of a nearby line (whisper,
//!   say, shout, owner say, said to us alone, script errors);
//! - the regular expressions are linear-time (no backtracking) and bounded
//!   in size, so a pattern can never stall the viewer on a message; a
//!   pattern that does not compile disables its rule, shown as such;
//! - only messages are filtered here: the Legacy Viewer's rules over group
//!   notices, invitations, offers, teleport requests and script dialogs (and
//!   the automatic dialog replies) are not ported.

use super::World;
use crate::settings::Rgba;
use aurora_net::{ChatMessage, ChatSourceType, ChatType, InstantMessage};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

/// The account's file.
pub const FILE: &str = "omnifilter.json";
/// Most rules kept: beyond, a file written by hand is cut.
pub const MAX_RULES: usize = 200;
/// Longest pattern, in bytes.
pub const MAX_PATTERN: usize = 1024;
/// Most memory a compiled regular expression may take (its program and its
/// lazy automaton each): a pattern over it does not compile.
const REGEX_SIZE: usize = 256 * 1024;
/// Entries of the filter's list of the session.
pub const MAX_LOG: usize = 200;
/// Lines remembered as highlighted.
const MAX_MARKS: usize = 4096;
/// The file's format.
const VERSION: u32 = 1;
/// How long the rules rest after an edit before they are saved.
const SAVE_DELAY: Duration = Duration::from_secs(1);

/// How a field is compared (OmnifilterEngine::eMatchType).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchMode {
    /// The whole field.
    Exact,
    /// Somewhere in the field (the Legacy default of a new rule).
    #[default]
    Substring,
    Regex,
}

/// One field of a rule: empty matches anything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pattern {
    pub text: String,
    pub mode: MatchMode,
    /// Off by default, as a new Legacy rule.
    pub case_sensitive: bool,
}

/// Where a message comes from (the message types of OmnifilterEngine::eType
/// this filter covers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    /// A resident in nearby chat.
    NearbyChat,
    /// An object in nearby chat (said, owner say, said to us alone, errors).
    ObjectChat,
    /// A resident's instant message.
    Im,
    /// A group chat or a conference.
    GroupChat,
    /// An object's instant message (llInstantMessage).
    ObjectIm,
}

impl Source {
    pub const ALL: [Source; 5] = [
        Source::NearbyChat,
        Source::ObjectChat,
        Source::Im,
        Source::GroupChat,
        Source::ObjectIm,
    ];
}

/// How a nearby line was said (its chat type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Channel {
    Whisper,
    Say,
    Shout,
    /// llOwnerSay.
    OwnerSay,
    /// llRegionSayTo: said to us alone.
    Direct,
    /// The debug channel: script errors.
    ScriptError,
}

impl Channel {
    pub const ALL: [Channel; 6] = [
        Channel::Whisper,
        Channel::Say,
        Channel::Shout,
        Channel::OwnerSay,
        Channel::Direct,
        Channel::ScriptError,
    ];

    /// The channel of a chat type; None for those no rule names (region
    /// messages, typing).
    pub fn of(t: ChatType) -> Option<Channel> {
        match t {
            ChatType::Whisper => Some(Channel::Whisper),
            ChatType::Normal => Some(Channel::Say),
            ChatType::Shout => Some(Channel::Shout),
            ChatType::OwnerSay => Some(Channel::OwnerSay),
            ChatType::Other(9) => Some(Channel::Direct),
            ChatType::Debug => Some(Channel::ScriptError),
            _ => None,
        }
    }
}

/// What a matching rule does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// The message is not shown (a Legacy rule without replacement).
    #[default]
    Hide,
    /// "Omnifilter Replacement: …" in place of the text (mChatReplace); an
    /// empty replacement hides it, as in the Legacy Viewer.
    Replace,
    /// Shown in the rule's colour.
    Highlight,
    /// Kept in the filter's list of the session instead of the chat.
    SetAside,
}

/// A rule (OmnifilterEngine::Needle).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    pub name: String,
    /// A new rule starts disabled, as in the Legacy Viewer.
    pub enabled: bool,
    /// The kinds of message it applies to; none: all of them.
    pub sources: Vec<Source>,
    /// The volumes or channels of a nearby line it applies to; none: all.
    /// Instant messages have none, so a rule naming one never matches them.
    pub channels: Vec<Channel>,
    pub sender: Pattern,
    pub content: Pattern,
    pub action: Action,
    pub replacement: String,
    pub color: Rgba,
}

impl Default for Rule {
    fn default() -> Self {
        Rule {
            name: String::new(),
            enabled: false,
            sources: Vec::new(),
            channels: Vec::new(),
            sender: Pattern::default(),
            content: Pattern::default(),
            action: Action::Hide,
            replacement: String::new(),
            // the keyword alerts' orange (FSKeywordColor)
            color: [255, 153, 0, 255],
        }
    }
}

/// The account's rules and the filter's switch (OmnifilterEnabled, off).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RuleSet {
    pub version: u32,
    pub on: bool,
    pub rules: Vec<Rule>,
}

impl RuleSet {
    /// A file as written by any version: a rule that cannot be read is left
    /// out alone, the others kept; an unreadable file gives no rules.
    pub fn parse(bytes: &[u8]) -> (RuleSet, usize) {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            return (RuleSet::default(), 0);
        };
        let mut skipped = 0;
        let rules = value
            .get("rules")
            .and_then(serde_json::Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|r| match serde_json::from_value::<Rule>(r.clone()) {
                        Ok(rule) => Some(rule),
                        Err(_) => {
                            skipped += 1;
                            None
                        }
                    })
                    .take(MAX_RULES)
                    .collect()
            })
            .unwrap_or_default();
        let on = value.get("on").and_then(serde_json::Value::as_bool).unwrap_or(false);
        (
            RuleSet {
                version: VERSION,
                on,
                rules,
            },
            skipped,
        )
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut copy = self.clone();
        copy.version = VERSION;
        serde_json::to_vec_pretty(&copy).unwrap_or_default()
    }
}

/// A field ready to compare.
#[derive(Debug)]
enum Matcher {
    Any,
    /// Lower-cased unless case-sensitive.
    Exact(String, bool),
    Substring(String, bool),
    Regex(regex::Regex),
    /// A pattern that did not compile: the rule never matches.
    Broken,
}

impl Matcher {
    /// `whole`: the pattern must cover the whole field (the sender's name,
    /// matchStrings); otherwise anywhere in it (the text, searchStrings).
    fn new(p: &Pattern, whole: bool) -> Matcher {
        if p.text.is_empty() {
            return Matcher::Any;
        }
        if p.text.len() > MAX_PATTERN {
            return Matcher::Broken;
        }
        let folded = if p.case_sensitive { p.text.clone() } else { p.text.to_lowercase() };
        match p.mode {
            MatchMode::Exact => Matcher::Exact(folded, p.case_sensitive),
            MatchMode::Substring => Matcher::Substring(folded, p.case_sensitive),
            MatchMode::Regex => {
                let source = if whole { format!("^(?:{})$", p.text) } else { p.text.clone() };
                regex::RegexBuilder::new(&source)
                    .case_insensitive(!p.case_sensitive)
                    .size_limit(REGEX_SIZE)
                    .dfa_size_limit(REGEX_SIZE)
                    .build()
                    .map_or(Matcher::Broken, Matcher::Regex)
            }
        }
    }

    fn matches(&self, field: &str) -> bool {
        match self {
            Matcher::Any => true,
            Matcher::Broken => false,
            Matcher::Regex(re) => re.is_match(field),
            Matcher::Exact(p, cs) => {
                if *cs {
                    field == p
                } else {
                    field.to_lowercase() == *p
                }
            }
            Matcher::Substring(p, cs) => {
                if *cs {
                    field.contains(p.as_str())
                } else {
                    field.to_lowercase().contains(p.as_str())
                }
            }
        }
    }

    fn broken(&self) -> bool {
        matches!(self, Matcher::Broken)
    }
}

#[derive(Debug)]
struct Compiled {
    index: usize,
    sender: Matcher,
    content: Matcher,
}

/// What a message is matched on (OmnifilterEngine::Haystack).
#[derive(Debug, Clone, Copy)]
pub struct Haystack<'a> {
    pub source: Source,
    /// Nearby lines only.
    pub channel: Option<Channel>,
    pub sender: &'a str,
    pub text: &'a str,
}

/// What becomes of a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Hide,
    /// Shown with this text.
    Replace(String),
    Highlight(Rgba),
    SetAside,
}

/// An entry of the filter's list of the session: the rule that matched and,
/// for a message set aside, the message itself. Kept in memory only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub time: SystemTime,
    pub rule: String,
    pub source: Source,
    /// Sender and text of a message set aside; empty otherwise.
    pub from: String,
    pub text: String,
}

/// The rules of the account logged in, compiled.
#[derive(Debug, Default)]
pub struct Omnifilter {
    set: RuleSet,
    compiled: Vec<Compiled>,
    /// Indices of the rules whose pattern does not compile.
    broken: Vec<usize>,
    path: Option<PathBuf>,
    /// Edited and not saved yet, since then: saved once the typing pauses.
    dirty: Option<Instant>,
    pub log: VecDeque<LogEntry>,
    /// Bumped by every change of the rules and of the list.
    pub revision: u64,
}

impl Omnifilter {
    /// The account's rules (none before the first save).
    pub fn load(path: PathBuf) -> Omnifilter {
        let set = match std::fs::read(&path) {
            Ok(bytes) => {
                let (set, skipped) = RuleSet::parse(&bytes);
                if skipped > 0 {
                    log::warn!("omnifilter: {skipped} unreadable rule(s) left out");
                }
                set
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("omnifilter: rules not read: {e}");
                }
                RuleSet::default()
            }
        };
        let mut filter = Omnifilter::default();
        filter.path = Some(path);
        filter.install(set);
        filter
    }

    pub fn rules(&self) -> &RuleSet {
        &self.set
    }

    /// Rules whose pattern does not compile (their index).
    pub fn broken(&self) -> &[usize] {
        &self.broken
    }

    fn install(&mut self, mut set: RuleSet) {
        set.rules.truncate(MAX_RULES);
        self.compiled.clear();
        self.broken.clear();
        for (index, rule) in set.rules.iter().enumerate() {
            let c = Compiled {
                index,
                sender: Matcher::new(&rule.sender, true),
                content: Matcher::new(&rule.content, false),
            };
            if c.sender.broken() || c.content.broken() {
                self.broken.push(index);
            }
            self.compiled.push(c);
        }
        self.set = set;
        self.revision += 1;
    }

    /// The rules edited in Preferences: in use at once, saved a moment later
    /// (`flush`).
    pub fn replace(&mut self, set: RuleSet) {
        if set == self.set {
            return;
        }
        self.install(set);
        if self.path.is_some() {
            self.dirty = Some(Instant::now());
        }
    }

    /// Every frame: the rules saved off the frame thread once the edits
    /// pause.
    pub fn flush(&mut self, now: Instant) {
        if self.dirty.is_some_and(|since| now.duration_since(since) >= SAVE_DELAY) {
            self.dirty = None;
            self.save(true);
        }
    }

    fn save(&self, in_background: bool) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let bytes = self.set.to_bytes();
        let write = move || {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = crate::atomic_file::write(&path, &bytes) {
                log::warn!("omnifilter: rules not saved: {e}");
            }
        };
        if !in_background {
            write();
            return;
        }
        if let Err(e) = std::thread::Builder::new().name("omnifilter-save".into()).spawn(write) {
            log::warn!("omnifilter: rules not saved: {e}");
        }
    }

    /// The first enabled rule that matches (OmnifilterEngine::match), with
    /// what it does.
    pub fn check(&self, h: &Haystack) -> Option<(&Rule, Outcome)> {
        if !self.set.on {
            return None;
        }
        self.compiled.iter().find_map(|c| {
            let rule = self.set.rules.get(c.index)?;
            let applies = rule.enabled
                && (rule.sources.is_empty() || rule.sources.contains(&h.source))
                && (rule.channels.is_empty() || h.channel.is_some_and(|ch| rule.channels.contains(&ch)))
                && c.sender.matches(h.sender)
                && c.content.matches(h.text);
            applies.then(|| (rule, outcome(rule)))
        })
    }

    fn note(&mut self, rule: &str, source: Source, aside: Option<(&str, &str)>) {
        let (from, text) = aside.unwrap_or_default();
        self.log.push_back(LogEntry {
            time: SystemTime::now(),
            rule: rule.to_owned(),
            source,
            from: from.to_owned(),
            text: text.to_owned(),
        });
        while self.log.len() > MAX_LOG {
            self.log.pop_front();
        }
        self.revision += 1;
    }

    /// Applies the rules to a message: its text rewritten in place when a
    /// rule replaces it; None when it is not shown; else the highlight.
    pub fn apply(&mut self, h: Haystack, text: &mut String) -> Option<Option<Rgba>> {
        let (name, outcome) = match self.check(&h) {
            Some((rule, outcome)) => (rule.name.clone(), outcome),
            None => return Some(None),
        };
        match outcome {
            Outcome::Highlight(color) => Some(Some(color)),
            Outcome::Replace(with) => {
                self.note(&name, h.source, None);
                *text = with;
                Some(None)
            }
            Outcome::Hide => {
                self.note(&name, h.source, None);
                None
            }
            Outcome::SetAside => {
                self.note(&name, h.source, Some((h.sender, h.text)));
                None
            }
        }
    }

    /// Empties the list of the session.
    pub fn clear_log(&mut self) {
        self.log.clear();
        self.revision += 1;
    }
}

impl Drop for Omnifilter {
    /// An edit not saved yet (the session ends right after it).
    fn drop(&mut self) {
        if self.dirty.take().is_some() {
            self.save(false);
        }
    }
}

/// What a rule does with what it matched.
fn outcome(rule: &Rule) -> Outcome {
    match rule.action {
        Action::Hide => Outcome::Hide,
        Action::Replace if rule.replacement.trim().is_empty() => Outcome::Hide,
        Action::Replace => Outcome::Replace(aurora_i18n::tr!(
            "chatxl-omnifilter-replacement",
            replacement = rule.replacement.clone()
        )),
        Action::Highlight => Outcome::Highlight(rule.color),
        Action::SetAside => Outcome::SetAside,
    }
}

/// The lines a rule highlights, by their time and sender, for the chat's
/// colours (`ui::colors::keyword_color`), which have no access to the
/// world, like the keyword alerts' matcher (`keywords.rs`).
#[derive(Default)]
struct Marks {
    colors: HashMap<(SystemTime, Uuid), Rgba>,
    order: VecDeque<(SystemTime, Uuid)>,
}

static MARKS: RwLock<Option<Marks>> = RwLock::new(None);

fn mark(time: SystemTime, source: Uuid, color: Rgba) {
    let Ok(mut guard) = MARKS.write() else {
        return;
    };
    let marks = guard.get_or_insert_with(Marks::default);
    if marks.colors.insert((time, source), color).is_none() {
        marks.order.push_back((time, source));
    }
    while marks.order.len() > MAX_MARKS {
        if let Some(old) = marks.order.pop_front() {
            marks.colors.remove(&old);
        }
    }
}

/// The highlight of a line, when a rule gave it one.
pub fn highlight(time: SystemTime, source: Uuid) -> Option<Rgba> {
    MARKS.read().ok()?.as_ref()?.colors.get(&(time, source)).copied()
}

impl World {
    /// A nearby line, after the block list: None when a rule hides it or
    /// sets it aside, else its highlight (the text may be replaced). Our own
    /// lines and the system's are never filtered.
    pub(super) fn omnifilter_chat(&mut self, c: &mut ChatMessage) -> Option<Option<Rgba>> {
        let source = match c.source_type {
            ChatSourceType::Agent if c.source_id != self.agent_id => Source::NearbyChat,
            ChatSourceType::Object => Source::ObjectChat,
            _ => return Some(None),
        };
        let h = Haystack {
            source,
            channel: Channel::of(c.chat_type),
            sender: &c.from_name,
            text: &c.message,
        };
        let mut text = c.message.clone();
        let result = self.omnifilter.apply(h, &mut text);
        c.message = text;
        if result.is_none() {
            // their line is over: no more "is typing" (as the Legacy Viewer)
            self.nearby_typing.remove(&c.source_id);
        }
        result
    }

    /// A resident's, a group's or an object's instant message, after RLVa
    /// and the block list: true when a rule hides it or sets it aside (its
    /// text may be replaced). Notices, offers and our own are left alone.
    pub(super) fn omnifilter_im(&mut self, im: &mut InstantMessage) -> bool {
        use super::groups::dialog;
        if im.message.is_empty() || im.from_agent_id == self.agent_id {
            return false;
        }
        let source = match im.dialog {
            0 if !im.from_agent_id.is_nil() => Source::Im,
            dialog::SESSION_SEND | dialog::SESSION_INVITE => Source::GroupChat,
            19 => Source::ObjectIm,
            _ => return false,
        };
        let h = Haystack {
            source,
            channel: None,
            sender: &im.from_name,
            text: &im.message,
        };
        let mut text = im.message.clone();
        let shown = self.omnifilter.apply(h, &mut text).is_some();
        im.message = text;
        if !shown && source == Source::Im {
            self.social.set_typing(im.from_agent_id, false);
        }
        !shown
    }

    /// A line just added, highlighted when a rule asks for it.
    pub(super) fn omnifilter_mark(&self, time: SystemTime, source: Uuid, color: Option<Rgba>) {
        if let Some(color) = color {
            mark(time, source, color);
        }
    }

    /// A message just added to its conversation (`im_messages`): highlighted
    /// when a rule asks for it (hiding and replacing happened on arrival).
    pub fn omnifilter_message(&self, m: &crate::ui_sound::ImMessage) {
        let Some(line) = self.resident_line(m) else {
            return;
        };
        let source = match m.kind {
            crate::ui_sound::ImKind::Private => Source::Im,
            _ => Source::GroupChat,
        };
        // the legacy name, as on arrival (`omnifilter_im`)
        let sender = self.social.names.get(&line.source).unwrap_or(&line.from);
        let h = Haystack {
            source,
            channel: None,
            sender,
            text: &line.text,
        };
        if let Some((_, Outcome::Highlight(color))) = self.omnifilter.check(&h) {
            mark(line.time, line.source, color);
        }
    }
}

#[cfg(test)]
mod tests;
