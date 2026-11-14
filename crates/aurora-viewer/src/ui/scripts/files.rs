//! LLScriptEdCore::loadScriptText / writeToFile / openInExternalEditor and
//! LLLiveLSLFile (Firestorm, LGPL 2.1). Dialogs and file IO run off the UI thread.

use aurora_net::scripts::MAX_SOURCE;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

pub(super) enum Done {
    Opened(String),
    Saved,
    External(Watch),
    Failed(String),
    Cancelled,
}
pub(super) struct Job(pub mpsc::Receiver<Done>);

fn failure(error: impl std::fmt::Display) -> Done {
    Done::Failed(aurora_i18n::tr!("script-file-error", reason = error.to_string()))
}

pub(super) fn read(path: &Path, convert_tabs: bool) -> Result<String, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_SOURCE as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SOURCE {
        return Err(aurora_i18n::tr!("script-source-too-large").into());
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| aurora_i18n::tr!("script-file-utf8").to_owned())?;
    let source = if convert_tabs { text.replace('\t', "    ") } else { text.into() };
    if source.len() > MAX_SOURCE {
        return Err(aurora_i18n::tr!("script-source-too-large").into());
    }
    Ok(source)
}

pub(super) fn open(convert_tabs: bool) -> Job {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // LLFilePicker's script filter: LSL and SLua (*.lsl; *.lua; *.luau)
        let done = match rfd::FileDialog::new()
            .add_filter("LSL / SLua", &["lsl", "luau", "lua", "txt"])
            .pick_file()
        {
            None => Done::Cancelled,
            Some(path) => match read(&path, convert_tabs) {
                Ok(source) => Done::Opened(source),
                Err(e) => failure(e),
            },
        };
        let _ = tx.send(done);
    });
    Job(rx)
}

/// The file extension of a script: `.luau` for SLua (LLPreviewLSL /
/// LLLiveLSLEditor::onSaveToFile's script_extension).
fn extension(lua: bool) -> &'static str {
    if lua { "luau" } else { "lsl" }
}

pub(super) fn save(name: String, source: String, lua: bool) -> Job {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let done = match rfd::FileDialog::new()
            .set_file_name(format!("{}.{}", safe_name(&name), extension(lua)))
            .add_filter(if lua { "SLua" } else { "LSL" }, &[extension(lua)])
            .save_file()
        {
            None => Done::Cancelled,
            Some(path) => match fs::write(path, source) {
                Ok(()) => Done::Saved,
                Err(e) => failure(e),
            },
        };
        let _ = tx.send(done);
    });
    Job(rx)
}

fn safe_name(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"\\/|?*".contains(*c))
        .take(80)
        .collect();
    let name = name.trim_matches([' ', '.']);
    if name.is_empty() { "script".into() } else { name.into() }
}

/// Parse an executable and arguments, without invoking a command shell.
/// Quotes protect spaces; Windows backslashes are preserved except before ".
fn command(text: &str, path: &Path) -> Result<(String, Vec<String>), String> {
    let mut args = Vec::new();
    let mut arg = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut started = false;
    while let Some(c) = chars.next() {
        if c == '"' {
            quoted = !quoted;
            started = true;
        } else if c == '\\' {
            let mut count = 1;
            while chars.peek() == Some(&'\\') {
                chars.next();
                count += 1;
            }
            if chars.peek() == Some(&'"') {
                arg.push_str(&"\\".repeat(count / 2));
                if count % 2 == 1 {
                    chars.next();
                    arg.push('"');
                }
            } else {
                arg.push_str(&"\\".repeat(count));
            }
            started = true;
        } else if c.is_whitespace() && !quoted {
            if started {
                args.push(std::mem::take(&mut arg));
                started = false;
            }
        } else {
            arg.push(c);
            started = true;
        }
    }
    if quoted {
        return Err(aurora_i18n::tr!("script-external-command-error").into());
    }
    if started {
        args.push(arg);
    }
    if args.is_empty() || args[0].is_empty() {
        return Err(aurora_i18n::tr!("script-external-not-set").into());
    }
    let executable = args.remove(0);
    let file = path.to_string_lossy();
    let mut inserted = false;
    for arg in &mut args {
        if arg.contains("%s") {
            *arg = arg.replace("%s", &file);
            inserted = true;
        }
    }
    if !inserted {
        args.push(file.into_owned());
    }
    Ok((executable, args))
}

/// A new folder readable by its owner only; one already there (planted by
/// another account) is refused. The shared temp folder of Linux and macOS
/// is open to every account, and a script can hold a key or a private URL:
/// the folder's 0700 is what keeps it private once an editor saves by
/// writing a new file (with the umask's 0644) and renaming it over ours.
/// Windows' temp folder is already per user.
fn create_private_dir(directory: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(directory)
    }
    #[cfg(not(unix))]
    fs::create_dir(directory)
}

/// The script written into a private folder, readable by its owner only:
/// a new file, or (`new` false) the file of the same script again.
fn write_private_file(path: &Path, source: &str, new: bool) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new();
    file.write(true);
    if new {
        file.create_new(true);
    } else {
        file.create(true).truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        file.mode(0o600);
    }
    file.open(path)?.write_all(source.as_bytes())
}

/// The session's private folder of the files given to external editors,
/// made on the first use: the scripts lie in it directly, as in the Legacy
/// Viewer's temp folder, so that the script editor link's `script.list`
/// can name it (`temp_dir`) and an editor find a script by its id there.
/// A fresh name each session, refused if it exists, keeps other accounts
/// out of it.
pub(crate) fn session_dir() -> std::io::Result<PathBuf> {
    let mut dir = SESSION_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(dir) = dir.as_ref() {
        return Ok(dir.clone());
    }
    let made = std::env::temp_dir().join(format!("{SESSION_PREFIX}{}", uuid::Uuid::new_v4()));
    create_private_dir(&made)?;
    *dir = Some(made.clone());
    Ok(made)
}

/// The folder `session_dir` made in this run, if it made one.
static SESSION_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
const SESSION_PREFIX: &str = "AuroraScripts-";

/// At exit: this run's folder of external editor files is removed, as the
/// Legacy Viewer deletes its temporary script files (LLLiveLSLFile's
/// destructor). Only the folder this run made, never another run's nor
/// anything else; nothing when none was made or it is already gone.
pub(crate) fn remove_session_dir() {
    let made = SESSION_DIR.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    if let Some(dir) = made
        && let Err(e) = remove_own_dir(&dir)
    {
        log::warn!("script editor: temporary folder {} not removed: {e}", dir.display());
    }
}

/// Removes `dir` and what it holds when it is a real folder of ours (no
/// link to elsewhere, our name); a missing one is fine.
fn remove_own_dir(dir: &Path) -> std::io::Result<()> {
    let ours = dir
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(SESSION_PREFIX));
    if !ours {
        return Err(std::io::Error::other("not a script editor folder"));
    }
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.file_type().is_dir() => fs::remove_dir_all(dir),
        Ok(_) => Err(std::io::Error::other("not a folder")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// The external editing sessions of each file: the last one to start owns
/// the file and removes it when it ends, so that a script opened again
/// before the previous watch stops keeps its file.
fn claim(path: &Path, owner: Option<u64>) -> u64 {
    static OWNERS: Mutex<Option<HashMap<PathBuf, u64>>> = Mutex::new(None);
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut owners = OWNERS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let owners = owners.get_or_insert_with(HashMap::new);
    match owner {
        None => {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            owners.insert(path.to_path_buf(), id);
            id
        }
        // releasing: still the owner? (1 yes, 0 no)
        Some(id) => {
            if owners.get(path) == Some(&id) {
                owners.remove(path);
                1
            } else {
                0
            }
        }
    }
}

pub(super) struct Watch {
    pub changes: mpsc::Receiver<Result<String, String>>,
    stop: Arc<AtomicBool>,
}
impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub(super) fn external(name: String, source: String, command_line: String, convert_tabs: bool, lua: bool) -> Job {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let directory = std::env::temp_dir().join(format!("AuroraScript-{}", uuid::Uuid::new_v4()));
        let path = directory.join(format!("{}.{}", safe_name(&name), extension(lua)));
        let command_line = if command_line.trim().is_empty() {
            std::env::var("LL_SCRIPT_EDITOR").unwrap_or_default()
        } else {
            command_line
        };
        let started = (|| -> Result<Option<std::time::SystemTime>, String> {
            let (executable, args) = command(&command_line, &path)?;
            write_private_file(&path, &source, false).map_err(|e| e.to_string())?;
            // Capture our write before launching: even an editor that saves
            // immediately must trigger the first reload.
            let stamp = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
            crate::open_path::spawn(std::process::Command::new(executable).args(args)).map_err(|e| e.to_string())?;
            Ok(stamp)
        })();
        let mut stamp = match started {
            Ok(stamp) => stamp,
            Err(e) => {
                let _ = tx.send(failure(e));
                if claim(&path, Some(owner)) == 1 {
                    let _ = fs::remove_file(&path);
                }
                return;
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (changed_tx, changed_rx) = mpsc::channel();
        let _ = tx.send(Done::External(Watch {
            changes: changed_rx,
            stop: stop.clone(),
        }));
        let mut previous = source;
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(500));
            let next = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
            if next.is_none() || next == stamp {
                continue;
            }
            stamp = next;
            match read(&path, convert_tabs) {
                Ok(source) if source != previous => {
                    previous.clone_from(&source);
                    if changed_tx.send(Ok(source)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    if changed_tx.send(Err(e)).is_err() {
                        break;
                    }
                }
                _ => {}
            }
        }
        if claim(&path, Some(owner)) == 1 {
            let _ = fs::remove_file(&path);
        }
    });
    Job(rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_arguments_keep_spaces_backslashes_and_do_not_use_a_shell() {
        let (exe, args) = command(
            r#""C:\Program Files\editor.exe" --wait "%s" "$literal""#,
            Path::new(r"C:\temp\a script.lsl"),
        )
        .unwrap();
        assert_eq!(exe, r"C:\Program Files\editor.exe");
        assert_eq!(args, ["--wait", r"C:\temp\a script.lsl", "$literal"]);
        assert!(command("\"unterminated", Path::new("s.lsl")).is_err());
        assert_eq!(safe_name("a: / b?."), "a  b");
    }
    #[test]
    fn file_import_is_bounded_utf8_with_optional_tab_conversion() {
        let directory = std::env::temp_dir().join(format!("aurora-script-file-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("source.lsl");
        fs::write(&path, b"\xef\xbb\xbf\tcode\r\n").unwrap();
        assert_eq!(read(&path, true).unwrap(), "    code\r\n");
        assert_eq!(read(&path, false).unwrap(), "\tcode\r\n");
        fs::write(&path, [0xff]).unwrap();
        assert!(read(&path, true).is_err());
        fs::write(&path, vec![b'x'; MAX_SOURCE + 1]).unwrap();
        assert!(read(&path, false).is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn the_last_editing_session_of_a_file_removes_it() {
        let path = Path::new("claim-test.lsl");
        let first = claim(path, None);
        let second = claim(path, None);
        assert_eq!(claim(path, Some(first)), 0, "a newer session owns the file");
        assert_eq!(claim(path, Some(second)), 1);
        assert_eq!(claim(path, Some(second)), 0, "released once");
    }

    #[test]
    fn the_session_folder_is_private_and_kept() {
        let a = session_dir().unwrap();
        assert_eq!(session_dir().unwrap(), a);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&a).unwrap().permissions().mode() & 0o777, 0o700);
        }
    }

    #[test]
    fn external_export_is_private_and_never_reuses_a_path() {
        let directory = std::env::temp_dir().join(format!("aurora-script-export-test-{}", uuid::Uuid::new_v4()));
        let path = directory.join("source.lsl");
        create_private_dir(&directory).unwrap();
        write_private_file(&path, "default {}", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "default {}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&directory), 0o700);
            assert_eq!(mode(&path), 0o600);
        }
        // the file of the same script is written again in place
        write_private_file(&path, "default { }", false).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "default { }");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // A folder already there (planted by another account) is refused.
        assert!(create_private_dir(&directory).is_err());
        assert!(write_private_file(&path, "x", true).is_err(), "a new file never replaces one");
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn only_a_real_script_editor_folder_is_removed_at_exit() {
        let base = std::env::temp_dir().join(format!("aurora-script-exit-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&base).unwrap();
        // a folder of another name is never removed
        let other = base.join("Other");
        fs::create_dir(&other).unwrap();
        assert!(remove_own_dir(&other).is_err());
        assert!(other.is_dir());
        // ours goes with its files; gone already is fine
        let ours = base.join(format!("{SESSION_PREFIX}{}", uuid::Uuid::new_v4()));
        create_private_dir(&ours).unwrap();
        write_private_file(&ours.join("a.lsl"), "default {}", true).unwrap();
        remove_own_dir(&ours).unwrap();
        assert!(!ours.exists());
        remove_own_dir(&ours).unwrap();
        // a link with our name is not followed: what it points at stays
        #[cfg(unix)]
        {
            let keep = other.join("keep.lsl");
            fs::write(&keep, "x").unwrap();
            std::os::unix::fs::symlink(&other, &ours).unwrap();
            assert!(remove_own_dir(&ours).is_err());
            assert!(keep.exists());
            fs::remove_file(&ours).unwrap();
        }
        fs::remove_dir_all(base).unwrap();
    }
}
