//! What the machine says for the status (`status_share::Inputs`): the track
//! that plays and the programs of the table that run. A thread of its own
//! reads them every few seconds, only what the settings ask for, and only
//! while the master switch is on: with it off the thread does not exist.
//!
//! - Music: the Windows media session (Spotify, browsers, players that
//!   publish to the system media overlay), only while it is playing. Other
//!   systems have none yet.
//! - Programs: the process list, kept only as far as it matches the table
//!   (`status_share::running`); the names of the other processes are
//!   dropped at once and never stored, logged or sent.

use super::Track;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Seconds between two readings.
const PERIOD_MS: u64 = 5000;
const STEP_MS: u64 = 250;

#[derive(Default)]
struct Wants {
    music: AtomicBool,
    programs: AtomicBool,
    stop: AtomicBool,
}

#[derive(Default)]
struct Shared {
    music: Option<Track>,
    programs: Vec<u16>,
}

/// The reading thread. Dropping it stops the thread within a step.
pub struct Collector {
    wants: Arc<Wants>,
    shared: Arc<Mutex<Shared>>,
}

impl Collector {
    pub fn start() -> Collector {
        let wants = Arc::new(Wants::default());
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (w, s) = (wants.clone(), shared.clone());
        if let Err(e) = std::thread::Builder::new().name("status-share".into()).spawn(move || run(&w, &s)) {
            log::warn!("status share: no reading thread ({e})");
        }
        Collector { wants, shared }
    }

    /// What to read from now on.
    pub fn want(&self, music: bool, programs: bool) {
        self.wants.music.store(music, Ordering::Relaxed);
        self.wants.programs.store(programs, Ordering::Relaxed);
    }

    /// The last reading: the track that plays and the ids of the programs
    /// that run.
    pub fn read(&self) -> (Option<Track>, Vec<u16>) {
        let s = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        (s.music.clone(), s.programs.clone())
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.wants.stop.store(true, Ordering::Relaxed);
    }
}

fn run(wants: &Wants, shared: &Mutex<Shared>) {
    init();
    while !wants.stop.load(Ordering::Relaxed) {
        let music = if wants.music.load(Ordering::Relaxed) { now_playing() } else { None };
        let programs = if wants.programs.load(Ordering::Relaxed) {
            super::running(&process_names())
        } else {
            Vec::new()
        };
        {
            let mut s = shared.lock().unwrap_or_else(PoisonError::into_inner);
            s.music = music;
            s.programs = programs;
        }
        let mut waited = 0;
        while waited < PERIOD_MS && !wants.stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(STEP_MS));
            waited += STEP_MS;
        }
    }
}

#[cfg(windows)]
fn init() {
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
    // SAFETY: called once at the start of this thread, which owns its
    // apartment; WinRT stays initialized until the thread ends.
    if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        log::warn!("status share: RoInitialize failed ({e})");
    }
}

#[cfg(not(windows))]
fn init() {}

/// The track of the system's current media session while it plays.
#[cfg(windows)]
fn now_playing() -> Option<Track> {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as Manager, GlobalSystemMediaTransportControlsSessionPlaybackStatus as Playback,
    };
    let manager = Manager::RequestAsync().ok()?.join().ok()?;
    let session = manager.GetCurrentSession().ok()?;
    if session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? != Playback::Playing {
        return None;
    }
    let properties = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = properties.Title().ok()?.to_string();
    let artist = properties.Artist().ok()?.to_string();
    (!title.trim().is_empty()).then_some(Track { artist, title })
}

#[cfg(not(windows))]
fn now_playing() -> Option<Track> {
    None
}

/// The names of the running processes.
#[cfg(windows)]
fn process_names() -> Vec<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    let mut out = Vec::new();
    // SAFETY: the snapshot is checked and closed; the entry is sized as the
    // calls expect, and its name is read up to its terminating zero within
    // the array.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            out.push(String::from_utf16_lossy(&entry.szExeFile[..len]));
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    out
}

#[cfg(target_os = "linux")]
fn process_names() -> Vec<String> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit())))
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
        .map(|name| name.trim().to_owned())
        .collect()
}

#[cfg(not(any(windows, target_os = "linux")))]
fn process_names() -> Vec<String> {
    Vec::new()
}
