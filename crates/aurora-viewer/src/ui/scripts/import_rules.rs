//! The rules of "Import scripts": which files become which scripts, under
//! which names, and the queue that sends them one after another. Everything
//! here is pure (no window, no network), so that the tests cover it.
//!
//! The Legacy Viewer loads one file into an open editor
//! (LLScriptEdCore::onBtnLoadFromFile, indra/newview/llpreviewscript.cpp,
//! originally LGPL 2.1) and has no way to create scripts from several
//! files: this is an Aurora addition. What it follows from the Legacy
//! Viewer:
//! - the item name rule, LLInventoryObject::correctInventoryName
//!   (indra/llinventory/llinventory.cpp): at most 63 bytes
//!   (DB_INV_ITEM_NAME_STR_LEN), no `|`, no control or non-ASCII character
//!   (here a non-ASCII character becomes `_`, the owner's choice, where the
//!   Legacy Viewer writes a space);
//! - the "New Script" name the server cannot take for a new script
//!   (fixNewScriptDefaultName, indra/newview/llfloaterscriptrecover.cpp):
//!   created as `New_Script`, renamed once its text is saved;
//! - one upload at a time (`PoolSizeUpload` is 1 in the Legacy Viewer).

use aurora_net::scripts::{CompileTarget, MAX_SOURCE};
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Files read from one pick of the file dialog (as the Imports window's 256).
pub const MAX_PER_PICK: usize = 256;
/// Rows of the list, finished ones included.
pub const MAX_ROWS: usize = 512;
/// The text kept for the rows that wait or work.
pub const MAX_TOTAL_TEXT: usize = 64 * 1024 * 1024;
/// DB_INV_ITEM_NAME_STR_LEN (indra/llmessage/lldbstrings.h).
pub const MAX_NAME: usize = 63;
/// NEW_LSL_NAME (indra/newview/llfloaterscriptrecover.cpp).
pub const NEW_SCRIPT: &str = "New Script";
/// What the Legacy Viewer's script filter lists (FFLOAD_SCRIPT: lsl, lua,
/// luau) and Aurora's Load from file adds: txt.
pub const EXTENSIONS: [&str; 4] = ["lsl", "luau", "lua", "txt"];

/// What a script is compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// LSL on Mono, the Legacy Viewer's choice for an item of the inventory.
    Mono,
    /// LSL on the old LSL2 virtual machine.
    Lsl,
    /// SLua source, run by Luau.
    Slua,
}

impl Kind {
    pub fn compile(self) -> CompileTarget {
        match self {
            Kind::Mono => CompileTarget::Mono,
            Kind::Lsl => CompileTarget::Lsl2,
            Kind::Slua => CompileTarget::Luau,
        }
    }
}

/// Whether a file is SLua, by its extension (`.lua`, `.luau`, in any case).
pub fn is_slua(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua") || ext.eq_ignore_ascii_case("luau"))
}

/// Why a file is not imported. Text for the window: `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    Unreadable(String),
    TooLarge,
    NotUtf8,
    Binary,
    Empty,
    TooMany,
    TooMuch,
    NoSlua,
}

impl Skip {
    pub fn text(&self) -> String {
        match self {
            Skip::Unreadable(reason) => aurora_i18n::tr!("script-import-skip-unreadable", reason = reason.as_str()),
            Skip::TooLarge => aurora_i18n::tr!("script-import-skip-too-large").into(),
            Skip::NotUtf8 => aurora_i18n::tr!("script-import-skip-utf8").into(),
            Skip::Binary => aurora_i18n::tr!("script-import-skip-binary").into(),
            Skip::Empty => aurora_i18n::tr!("script-import-skip-empty").into(),
            Skip::TooMany => aurora_i18n::tr!("script-import-skip-too-many").into(),
            Skip::TooMuch => aurora_i18n::tr!("script-import-skip-too-much").into(),
            Skip::NoSlua => aurora_i18n::tr!("script-import-skip-no-slua").into(),
        }
    }
}

/// Line endings of a text, all as LF (the Legacy Viewer's loader reads line
/// by line and joins them with LF).
pub fn normalise_lines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The script a file's bytes make: at most 1 MiB, UTF-8 (a BOM is dropped),
/// no NUL byte, lines ending in LF, tabs as four spaces when asked, and some
/// text in it.
pub fn decode(bytes: &[u8], convert_tabs: bool) -> Result<String, Skip> {
    if bytes.len() > MAX_SOURCE {
        return Err(Skip::TooLarge);
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if bytes.contains(&0) {
        return Err(Skip::Binary);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Skip::NotUtf8)?;
    let mut source = normalise_lines(text);
    if convert_tabs {
        source = source.replace('\t', "    ");
    }
    if source.trim().is_empty() {
        return Err(Skip::Empty);
    }
    if source.len() > MAX_SOURCE {
        return Err(Skip::TooLarge);
    }
    Ok(source)
}

/// `decode` of a file, reading no more than one byte past the limit.
pub fn read_file(path: &Path, convert_tabs: bool) -> Result<String, Skip> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_SOURCE as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| Skip::Unreadable(e.to_string()))?;
    decode(&bytes, convert_tabs)
}

/// The name an item gets from a name (a file name without its extension):
/// the Legacy rule, `|` a space, any other control or non-ASCII character
/// a `_`, trimmed, at most 63 bytes. A name with nothing left is "Script".
pub fn clean_name(raw: &str) -> String {
    let mapped: String = raw
        .chars()
        .map(|c| match c {
            '|' => ' ',
            c if c.is_ascii() && !c.is_ascii_control() => c,
            _ => '_',
        })
        .collect();
    // ASCII only: bytes and characters are the same
    let name = mapped.trim();
    let name = &name[..name.len().min(MAX_NAME)];
    let name = name.trim_end();
    if name.is_empty() { "Script".to_owned() } else { name.to_owned() }
}

/// The name of the script a file makes: its name without the extension.
pub fn item_name(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    clean_name(&stem)
}

/// The name an item is created with: "New Script" is created as
/// "New_Script" and renamed afterwards (fixNewScriptDefaultName).
pub fn creation_name(name: &str) -> String {
    if name == NEW_SCRIPT {
        name.replace(' ', "_")
    } else {
        name.to_owned()
    }
}

/// The comparison key of a name: item names of one folder differ by more
/// than their case here, so that a numbered name never looks like another.
pub fn key(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// `name`, or `name 2`, `name 3`... when it is one of `taken` (keys, see
/// `key`): an import never replaces a script, and never makes two of the
/// same name in a batch. The number is kept within the 63 bytes.
pub fn numbered(name: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(&key(name)) {
        return name.to_owned();
    }
    let mut n = 2u32;
    loop {
        let suffix = format!(" {n}");
        let keep = MAX_NAME.saturating_sub(suffix.len());
        let base = &name[..name.len().min(keep)];
        let candidate = format!("{}{suffix}", base.trim_end());
        if !taken.contains(&key(&candidate)) {
            return candidate;
        }
        n += 1;
    }
}

/// What one row of the list is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Read, not started: the Import button starts every such row.
    Ready,
    /// Started, waiting for its turn.
    Queued,
    /// Its item is being created and its text saved.
    Working,
    /// Saved, and compiled where the grid compiles.
    Saved,
    /// Saved; the grid does not compile scripts of the inventory (OpenSim).
    Unchecked,
    /// Saved; the grid reported these errors.
    CompileFailed(Vec<String>),
    /// Not saved. `kept`: an empty script of this name exists in the
    /// inventory (it was created, its text did not reach the grid).
    Failed {
        reason: String,
        kept: bool,
    },
    Skipped(Skip),
    Cancelled,
}

impl State {
    /// No longer waiting for anything.
    pub fn finished(&self) -> bool {
        !matches!(self, State::Ready | State::Queued | State::Working)
    }

    /// The script exists with its text.
    pub fn saved(&self) -> bool {
        matches!(self, State::Saved | State::Unchecked | State::CompileFailed(_))
    }
}

/// One file of the list.
#[derive(Debug, Clone)]
pub struct Row {
    pub path: PathBuf,
    /// The file's name, for the list.
    pub file: String,
    /// The item name the file makes (before any number is added).
    pub name: String,
    pub slua: bool,
    pub bytes: usize,
    /// The text, until the row starts.
    pub source: Option<String>,
    pub state: State,
    /// The item, once created.
    pub item: Option<uuid::Uuid>,
    /// Object destination: the item went into the object's contents.
    pub placed: bool,
    /// The name the item was made with, once started.
    pub made_as: Option<String>,
}

impl Row {
    pub fn kind(&self, lsl: Kind) -> Kind {
        if self.slua { Kind::Slua } else { lsl }
    }
}

/// A file read by the worker.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub path: PathBuf,
    pub result: Result<String, Skip>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub saved: usize,
    pub compile_errors: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cancelled: usize,
    pub waiting: usize,
}

/// The list and its queue: one row at a time works.
#[derive(Debug, Default)]
pub struct Batch {
    pub rows: Vec<Row>,
    working: Option<usize>,
}

impl Batch {
    fn retained(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| !row.state.finished())
            .map(|row| row.source.as_ref().map_or(0, String::len))
            .sum()
    }

    /// Add a file read. False when the file is left out: its path is
    /// already waiting in the list, or the list is full.
    pub fn add(&mut self, loaded: Loaded) -> bool {
        if self.rows.len() >= MAX_ROWS {
            return false;
        }
        if self.rows.iter().any(|row| row.path == loaded.path && !row.state.finished()) {
            return false;
        }
        let file = loaded
            .path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (bytes, source, state) = match loaded.result {
            Ok(text) if self.retained() + text.len() > MAX_TOTAL_TEXT => (text.len(), None, State::Skipped(Skip::TooMuch)),
            Ok(text) => (text.len(), Some(text), State::Ready),
            Err(skip) => (0, None, State::Skipped(skip)),
        };
        self.rows.push(Row {
            name: item_name(&loaded.path),
            slua: is_slua(&loaded.path),
            path: loaded.path,
            file,
            bytes,
            source,
            state,
            item: None,
            placed: false,
            made_as: None,
        });
        true
    }

    /// Every ready row waits for its turn.
    pub fn start(&mut self) {
        for row in &mut self.rows {
            if row.state == State::Ready {
                row.state = State::Queued;
            }
        }
    }

    /// The row whose turn it is, when none works: the first queued one, now
    /// working. One upload at a time, like the Legacy Viewer's pool of one.
    pub fn next(&mut self) -> Option<usize> {
        if self.working.is_some() {
            return None;
        }
        let index = self.rows.iter().position(|row| row.state == State::Queued)?;
        self.rows[index].state = State::Working;
        self.working = Some(index);
        Some(index)
    }

    pub fn working(&self) -> Option<usize> {
        self.working
    }

    /// The working row ended in `state`; the next one may start.
    pub fn finish(&mut self, index: usize, state: State) {
        if let Some(row) = self.rows.get_mut(index) {
            row.state = state;
            row.source = None;
        }
        if self.working == Some(index) {
            self.working = None;
        }
    }

    /// Stop what has not started; the working row ends by itself.
    pub fn cancel(&mut self) {
        for row in &mut self.rows {
            if row.state == State::Queued {
                row.state = State::Cancelled;
                row.source = None;
            }
        }
    }

    /// Nothing waits and nothing works.
    pub fn idle(&self) -> bool {
        self.working.is_none() && !self.rows.iter().any(|row| row.state == State::Queued)
    }

    /// Something started and not finished.
    pub fn running(&self) -> bool {
        !self.idle()
    }

    /// Drop the finished rows (and the ready ones when `all`), never one
    /// that works or waits.
    pub fn clear(&mut self, all: bool) {
        let working = self.working.and_then(|i| self.rows.get(i)).map(|row| row.path.clone());
        self.rows.retain(|row| match row.state {
            State::Queued | State::Working => true,
            State::Ready => !all,
            _ => false,
        });
        self.working = working.and_then(|path| self.rows.iter().position(|row| row.path == path && row.state == State::Working));
    }

    pub fn ready(&self) -> usize {
        self.rows.iter().filter(|row| row.state == State::Ready).count()
    }

    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();
        for row in &self.rows {
            match &row.state {
                State::Saved | State::Unchecked => s.saved += 1,
                State::CompileFailed(_) => {
                    s.saved += 1;
                    s.compile_errors += 1;
                }
                State::Failed { .. } => s.failed += 1,
                State::Skipped(_) => s.skipped += 1,
                State::Cancelled => s.cancelled += 1,
                State::Ready | State::Queued | State::Working => s.waiting += 1,
            }
        }
        s
    }
}

/// The names of the items of the destination and those taken in this batch,
/// as comparison keys.
pub fn taken_names<'a>(existing: impl IntoIterator<Item = &'a str>, made: impl IntoIterator<Item = &'a str>) -> HashSet<String> {
    existing.into_iter().chain(made).map(key).collect()
}

/// The state a saved script's compile answer makes.
pub fn compile_state(compiled: bool, checked: bool, errors: Vec<String>) -> State {
    if !compiled || !errors.is_empty() {
        State::CompileFailed(errors)
    } else if checked {
        State::Saved
    } else {
        State::Unchecked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(path: &str, text: &str) -> Loaded {
        Loaded {
            path: PathBuf::from(path),
            result: Ok(text.to_owned()),
        }
    }

    #[test]
    fn names_follow_the_legacy_rule_with_underscores_for_non_ascii() {
        assert_eq!(clean_name("door"), "door");
        assert_eq!(clean_name("  door  "), "door");
        // a character is one underscore, not one per byte
        assert_eq!(clean_name("porte\u{e9}"), "porte_");
        assert_eq!(clean_name("\u{65e5}\u{672c}\u{8a9e}"), "___");
        assert_eq!(clean_name("a|b"), "a b");
        assert_eq!(clean_name("a\tb"), "a_b");
        assert_eq!(clean_name("a\u{7f}b"), "a_b");
        assert_eq!(clean_name(""), "Script");
        assert_eq!(clean_name("|||"), "Script");
        let long = "x".repeat(100);
        assert_eq!(clean_name(&long).len(), 63);
        // never ends in the space the cut left
        let cut = format!("{} {}", "y".repeat(62), "z");
        assert_eq!(clean_name(&cut), "y".repeat(62));
    }

    #[test]
    fn a_file_makes_the_name_of_its_stem() {
        assert_eq!(item_name(Path::new("C:/work/door.v2.lsl")), "door.v2");
        assert_eq!(item_name(Path::new("hud.LUAU")), "hud");
        assert_eq!(item_name(Path::new("\u{30b9}\u{30af}.lsl")), "__");
        assert_eq!(item_name(Path::new(".lsl")), ".lsl");
    }

    #[test]
    fn new_script_is_created_with_an_underscore_and_only_that_name() {
        assert_eq!(creation_name("New Script"), "New_Script");
        assert_eq!(creation_name("New Script 2"), "New Script 2");
        assert_eq!(creation_name("Door"), "Door");
    }

    #[test]
    fn a_taken_name_gets_a_number_that_stays_within_the_limit() {
        let mut taken: HashSet<String> = ["door".to_owned()].into();
        assert_eq!(numbered("Door", &taken), "Door 2");
        assert_eq!(numbered("lamp", &taken), "lamp");
        taken.insert(key("Door 2"));
        taken.insert(key("Door 3"));
        assert_eq!(numbered("door", &taken), "door 4");
        let long = "n".repeat(63);
        let taken: HashSet<String> = [key(&long)].into();
        let named = numbered(&long, &taken);
        assert!(named.len() <= 63, "{named}");
        assert!(named.ends_with(" 2"));
    }

    #[test]
    fn the_extension_tells_slua_from_lsl() {
        for (path, slua) in [
            ("a.lsl", false),
            ("a.txt", false),
            ("a", false),
            ("a.lua", true),
            ("a.LUAU", true),
            ("a.luau.txt", false),
        ] {
            assert_eq!(is_slua(Path::new(path)), slua, "{path}");
        }
        assert_eq!(Kind::Mono.compile(), CompileTarget::Mono);
        assert_eq!(Kind::Lsl.compile(), CompileTarget::Lsl2);
        assert_eq!(Kind::Slua.compile(), CompileTarget::Luau);
    }

    #[test]
    fn bytes_become_text_with_the_bom_dropped_and_lines_in_lf() {
        assert_eq!(decode(b"\xef\xbb\xbfdefault\r\n{\r\n}\r\n", false).unwrap(), "default\n{\n}\n");
        assert_eq!(decode(b"a\rb", false).unwrap(), "a\nb");
        assert_eq!(decode(b"\tx", true).unwrap(), "    x");
        assert_eq!(decode(b"\tx", false).unwrap(), "\tx");
        assert_eq!(decode("h\u{e9}llo".as_bytes(), false).unwrap(), "h\u{e9}llo");
    }

    #[test]
    fn a_file_that_is_not_a_script_text_is_refused_with_its_reason() {
        assert_eq!(decode(b"\xff\xfeh\0i\0", false), Err(Skip::Binary));
        assert_eq!(decode(&[0x68, 0xe9, 0x6c], false), Err(Skip::NotUtf8));
        assert_eq!(decode(b"", false), Err(Skip::Empty));
        assert_eq!(decode(b"\xef\xbb\xbf  \r\n\t", false), Err(Skip::Empty));
        let limit = vec![b'a'; MAX_SOURCE];
        assert!(decode(&limit, false).is_ok());
        let over = vec![b'a'; MAX_SOURCE + 1];
        assert_eq!(decode(&over, false), Err(Skip::TooLarge));
        // the tabs may push a file over the limit
        let tabs = vec![b'\t'; MAX_SOURCE / 2];
        let mut text = tabs;
        text.push(b'x');
        assert_eq!(decode(&text, true), Err(Skip::TooLarge));
    }

    #[test]
    fn a_file_is_read_from_the_disk_and_a_missing_one_is_unreadable() {
        let dir = std::env::temp_dir().join(format!("aurora-script-import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("folder");
        let file = dir.join("door.lsl");
        std::fs::write(&file, b"\xef\xbb\xbfdefault\r\n{\r\n}\r\n").expect("file");
        assert_eq!(read_file(&file, false).unwrap(), "default\n{\n}\n");
        assert!(matches!(read_file(&dir.join("none.lsl"), false), Err(Skip::Unreadable(_))));
        let big = dir.join("big.lsl");
        std::fs::write(&big, vec![b'a'; MAX_SOURCE + 10]).expect("big");
        assert_eq!(read_file(&big, false), Err(Skip::TooLarge));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rows_wait_for_import_and_the_queue_runs_one_at_a_time_in_order() {
        let mut batch = Batch::default();
        for name in ["b.lsl", "a.lsl", "c.luau"] {
            assert!(batch.add(loaded(name, "x")));
        }
        assert!(batch.rows.iter().all(|row| row.state == State::Ready));
        assert_eq!(batch.next(), None, "nothing starts before Import");
        assert!(batch.idle());
        batch.start();
        assert!(batch.running());
        assert_eq!(batch.next(), Some(0));
        assert_eq!(batch.next(), None, "one upload in flight");
        assert_eq!(batch.working(), Some(0));
        batch.finish(0, State::Saved);
        assert_eq!(batch.rows[0].source, None, "the text is let go");
        assert_eq!(batch.next(), Some(1));
        batch.finish(
            1,
            State::Failed {
                reason: "no".into(),
                kept: false,
            },
        );
        assert_eq!(batch.next(), Some(2));
        assert!(!batch.idle());
        batch.finish(2, State::CompileFailed(vec!["(1, 1) : ERROR : x".into()]));
        assert!(batch.idle());
        assert_eq!(
            batch.summary(),
            Summary {
                saved: 2,
                compile_errors: 1,
                failed: 1,
                ..Summary::default()
            }
        );
    }

    #[test]
    fn cancel_stops_what_waits_and_lets_the_working_row_end() {
        let mut batch = Batch::default();
        for name in ["a.lsl", "b.lsl", "c.lsl"] {
            batch.add(loaded(name, "x"));
        }
        batch.start();
        assert_eq!(batch.next(), Some(0));
        batch.cancel();
        assert_eq!(batch.rows[1].state, State::Cancelled);
        assert_eq!(batch.rows[2].state, State::Cancelled);
        assert_eq!(batch.rows[0].state, State::Working);
        assert!(batch.running(), "the working row is not done");
        batch.finish(0, State::Saved);
        assert!(batch.idle());
        assert_eq!(batch.next(), None);
        assert_eq!(batch.summary().cancelled, 2);
    }

    #[test]
    fn a_path_already_waiting_is_left_out_but_a_finished_one_may_come_again() {
        let mut batch = Batch::default();
        assert!(batch.add(loaded("a.lsl", "x")));
        assert!(!batch.add(loaded("a.lsl", "x")), "already in the list");
        batch.start();
        let index = batch.next().unwrap();
        assert!(!batch.add(loaded("a.lsl", "x")), "working");
        batch.finish(index, State::Saved);
        assert!(batch.add(loaded("a.lsl", "x")), "imported again, as a new script");
        assert_eq!(batch.rows.len(), 2);
    }

    #[test]
    fn the_list_and_its_text_have_limits() {
        let mut batch = Batch::default();
        for i in 0..MAX_ROWS {
            assert!(batch.add(Loaded {
                path: PathBuf::from(format!("{i}.lsl")),
                result: Ok("x".into())
            }));
        }
        assert!(!batch.add(loaded("one-more.lsl", "x")), "the list is full");
        let mut batch = Batch::default();
        let chunk = "a".repeat(MAX_SOURCE);
        for i in 0..=(MAX_TOTAL_TEXT / MAX_SOURCE) {
            batch.add(Loaded {
                path: PathBuf::from(format!("{i}.lsl")),
                result: Ok(chunk.clone()),
            });
        }
        let last = batch.rows.last().unwrap();
        assert_eq!(last.state, State::Skipped(Skip::TooMuch));
        assert_eq!(last.source, None);
        // a file refused by the reader is a skipped row, with no text
        batch.add(Loaded {
            path: PathBuf::from("bin.lsl"),
            result: Err(Skip::Binary),
        });
        assert_eq!(batch.rows.last().unwrap().state, State::Skipped(Skip::Binary));
    }

    #[test]
    fn clear_drops_the_finished_rows_and_keeps_the_ones_that_wait_or_work() {
        let mut batch = Batch::default();
        for name in ["a.lsl", "b.lsl", "c.lsl", "d.lsl"] {
            batch.add(loaded(name, "x"));
        }
        batch.rows[0].state = State::Saved;
        batch.start();
        let index = batch.next().unwrap();
        assert_eq!(index, 1);
        batch.add(loaded("e.lsl", "x"));
        batch.clear(false);
        let names: Vec<_> = batch.rows.iter().map(|r| r.file.as_str()).collect();
        assert_eq!(names, ["b.lsl", "c.lsl", "d.lsl", "e.lsl"]);
        assert_eq!(batch.working(), Some(0), "the working row moved up");
        batch.clear(true);
        assert_eq!(batch.rows.len(), 3);
    }

    #[test]
    fn the_compile_answer_makes_the_state() {
        assert_eq!(compile_state(true, true, vec![]), State::Saved);
        assert_eq!(compile_state(true, false, vec![]), State::Unchecked);
        assert_eq!(
            compile_state(false, true, vec!["(5, 9) : ERROR : Syntax error".into()]),
            State::CompileFailed(vec!["(5, 9) : ERROR : Syntax error".into()])
        );
        assert_eq!(compile_state(false, true, vec![]), State::CompileFailed(vec![]));
        assert!(compile_state(false, true, vec![]).saved());
        assert!(!State::Cancelled.saved());
        assert!(State::Cancelled.finished());
        assert!(!State::Working.finished());
    }

    #[test]
    fn names_taken_are_compared_without_case() {
        let taken = taken_names(["Door", "lamp"], ["HUD"]);
        assert!(taken.contains("door") && taken.contains("hud") && taken.contains("lamp"));
        assert_eq!(numbered("DOOR", &taken), "DOOR 2");
    }
}
