//! Native files and codec work are prepared off the frame thread. Opening the
//! existing upload inspector never starts a paid upload automatically.
use super::*;
use std::path::PathBuf;

pub struct Prepared {
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
    pub animation: Arc<Animation>,
    /// A pose from the poser, not a clip: its upload has its own folder.
    pub pose: bool,
}
#[derive(Clone, Copy)]
pub(super) enum Action {
    Save,
    Import,
}
pub(super) enum Output {
    Preview {
        revision: u64,
        start: u32,
        end: u32,
        result: Result<Arc<BoundAnim>, &'static str>,
    },
    Prepared(Prepared),
    Saved(PathBuf),
    /// A native animation read for the timeline: its name and contents.
    Opened(String, Animation),
    /// The autosaved draft was written at this revision.
    Autosaved(u64),
    Cancelled,
    Failed(String),
}
pub fn basename(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let name = name.trim().trim_end_matches('.');
    if name.is_empty() {
        "animation".into()
    } else {
        name.chars().take(100).collect()
    }
}
pub(super) fn start(jobs: &Jobs, ctx: &egui::Context, tx: crossbeam_channel::Sender<Output>, document: Arc<Document>, action: Action) {
    let ctx = ctx.clone();
    jobs.execute(move || {
        let work = || -> Result<Output, String> {
            let animation = document.animation().map_err(|k| aurora_i18n::text(k).to_owned())?;
            let bytes = aurora_assets::import_animation::encode(&animation).map_err(|e| e.to_string())?;
            let animation = aurora_assets::anim::parse_animation_upload(&bytes).map_err(|e| e.to_string())?;
            let name = basename(&document.name);
            Ok(match action {
                Action::Import => Output::Prepared(Prepared {
                    name,
                    bytes: Arc::new(bytes),
                    animation: Arc::new(animation),
                    pose: false,
                }),
                Action::Save => {
                    let Some(path) = rfd::FileDialog::new()
                        .add_filter(aurora_i18n::tr!("anim-edit-file-filter"), &["anim"])
                        .set_file_name(format!("{name}.anim"))
                        .save_file()
                    else {
                        return Ok(Output::Cancelled);
                    };
                    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
                    Output::Saved(path)
                }
            })
        };
        let output = work().unwrap_or_else(Output::Failed);
        let _ = tx.send(output);
        ctx.request_repaint();
    });
}

/// The largest native animation the timeline opens (as the poser's loads).
const MAX_OPEN: u64 = 4 * 1024 * 1024;
/// The autosaved draft of the account (`recovery_dir`).
pub(super) const RECOVERY: &str = "recovery.anim";

/// Read a native animation for the timeline: `path`, or one the user picks.
pub(super) fn open(jobs: &Jobs, ctx: &egui::Context, tx: crossbeam_channel::Sender<Output>, path: Option<PathBuf>) {
    use std::io::Read as _;
    let ctx = ctx.clone();
    jobs.execute(move || {
        let path = path.or_else(|| {
            rfd::FileDialog::new()
                .add_filter(aurora_i18n::tr!("anim-edit-file-filter"), &["anim"])
                .pick_file()
        });
        let output = match path {
            None => Output::Cancelled,
            Some(path) => {
                let mut bytes = Vec::new();
                let read = std::fs::File::open(&path).and_then(|f| f.take(MAX_OPEN + 1).read_to_end(&mut bytes));
                match read {
                    Err(e) => Output::Failed(e.to_string()),
                    Ok(_) if bytes.len() as u64 > MAX_OPEN => Output::Failed(aurora_i18n::tr!("b4-clip-too-large").to_owned()),
                    Ok(_) => match aurora_assets::anim::parse_animation(&bytes) {
                        Ok(animation) => {
                            let name = path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                            Output::Opened(basename(&name), animation)
                        }
                        Err(e) => Output::Failed(e.to_string()),
                    },
                }
            }
        };
        let _ = tx.send(output);
        ctx.request_repaint();
    });
}

/// Write the draft as a native animation in `dir`, through a temporary
/// file so that a crash while writing keeps the previous one.
pub(super) fn autosave(jobs: &Jobs, tx: crossbeam_channel::Sender<Output>, document: Arc<Document>, revision: u64, dir: PathBuf) {
    jobs.execute(move || {
        let Ok(animation) = document.animation() else { return };
        let bytes = match aurora_assets::import_animation::encode(&animation) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("animation draft autosave: {e}");
                return;
            }
        };
        let path = dir.join(RECOVERY);
        let temporary = dir.join(format!("{RECOVERY}.tmp"));
        let written = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&temporary, &bytes))
            .and_then(|_| std::fs::rename(&temporary, &path));
        match written {
            Ok(()) => {
                let _ = tx.send(Output::Autosaved(revision));
            }
            Err(e) => log::warn!("animation draft autosave to {}: {e}", path.display()),
        }
    });
}
