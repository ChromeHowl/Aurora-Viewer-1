//! Native files and codec work are prepared off the frame thread. Opening the
//! existing upload inspector never starts a paid upload automatically.
use super::*;
use std::{io::Read, path::PathBuf};

pub struct Prepared {
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
    pub animation: Arc<Animation>,
    /// A pose from the poser, not a clip: its upload has its own folder.
    pub pose: bool,
}
#[derive(Clone, Copy)]
pub(super) enum Action {
    Load,
    LoadProject,
    SaveProject,
    Save { last_key_only: bool },
    Import { last_key_only: bool },
}
impl Action {
    /// Opening a file brings a document in and needs none to start from.
    pub(super) fn loads(self) -> bool {
        matches!(self, Self::Load | Self::LoadProject)
    }
}
pub(super) enum Output {
    Preview {
        revision: u64,
        start: u32,
        end: u32,
        result: Result<Arc<BoundAnim>, &'static str>,
    },
    Prepared(Prepared),
    Loaded(Document),
    Saved(PathBuf),
    /// The autosaved draft was written at this revision.
    Autosaved(u64),
    Cancelled,
    Failed(String),
}
pub(super) fn read_bounded(mut reader: impl Read, maximum: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(maximum.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > maximum {
        return Err(aurora_i18n::text("anim-edit-file-too-large").to_owned());
    }
    Ok(bytes)
}
fn read_file(path: impl AsRef<std::path::Path>) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    read_bounded(file, document::MAX_PROJECT_BYTES)
}
pub(super) fn load_document(
    bytes: &[u8],
    extension: &str,
    rig: &Rig,
    skeleton: Option<&aurora_assets::Skeleton>,
) -> Result<Document, String> {
    let animation = if extension.eq_ignore_ascii_case("bvh") {
        let skeleton = skeleton.ok_or_else(|| aurora_i18n::text("anim-edit-bvh-unavailable").to_owned())?;
        aurora_assets::import_animation::parse_bvh_for_edit(bytes, skeleton)
            .map_err(|error| error.to_string())?
            .animation
    } else if extension.eq_ignore_ascii_case("anim") {
        aurora_assets::anim::parse_animation_upload(bytes).map_err(|error| error.to_string())?
    } else {
        return Err(aurora_i18n::text("anim-edit-invalid").to_owned());
    };
    Document::from_animation(animation, rig).map_err(|key| aurora_i18n::text(key).to_owned())
}
/// Write through a temporary file beside the target, then rename it over:
/// a failed or interrupted save leaves the previous file whole.
pub(super) fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let result = std::fs::write(&temp, bytes)
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|error| error.to_string());
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
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
pub(super) fn start(
    jobs: &Jobs,
    ctx: &egui::Context,
    tx: crossbeam_channel::Sender<Output>,
    document: Option<Arc<Document>>,
    action: Action,
    rig: Arc<Rig>,
    skeleton: Option<Arc<aurora_assets::Skeleton>>,
) {
    let ctx = ctx.clone();
    jobs.execute(move || {
        let work = || -> Result<Output, String> {
            if matches!(action, Action::Load) {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter(aurora_i18n::tr!("anim-edit-file-filter"), &["anim"])
                    .add_filter(aurora_i18n::tr!("anim-edit-bvh-filter"), &["bvh"])
                    .pick_file()
                else {
                    return Ok(Output::Cancelled);
                };
                let bytes = read_file(&path)?;
                let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default();
                return load_document(&bytes, extension, &rig, skeleton.as_deref()).map(Output::Loaded);
            }
            if matches!(action, Action::LoadProject) {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter(aurora_i18n::tr!("anim-edit-project-filter"), &["auroraanim"])
                    .pick_file()
                else {
                    return Ok(Output::Cancelled);
                };
                let bytes = read_file(path)?;
                return Document::from_project_json(&bytes, &rig)
                    .map(Output::Loaded)
                    .map_err(|key| aurora_i18n::text(key).to_owned());
            }
            // every action below saves or exports the current document
            let document = document.ok_or_else(|| aurora_i18n::text("anim-edit-invalid").to_owned())?;
            if matches!(action, Action::SaveProject) {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter(aurora_i18n::tr!("anim-edit-project-filter"), &["auroraanim"])
                    .set_file_name(format!("{}.auroraanim", basename(&document.name)))
                    .save_file()
                else {
                    return Ok(Output::Cancelled);
                };
                let bytes = document.project_json().map_err(|key| aurora_i18n::text(key).to_owned())?;
                write_atomic(&path, &bytes)?;
                return Ok(Output::Saved(path));
            }
            let last_key_only = match action {
                Action::Save { last_key_only } | Action::Import { last_key_only } => last_key_only,
                _ => false,
            };
            let animation = document
                .animation_for_export(last_key_only)
                .map_err(|k| aurora_i18n::text(k).to_owned())?;
            let bytes = aurora_assets::import_animation::encode(&animation).map_err(|e| e.to_string())?;
            let animation = aurora_assets::anim::parse_animation_upload(&bytes).map_err(|e| e.to_string())?;
            let name = basename(&document.name);
            Ok(match action {
                Action::Load => unreachable!("file loading returned above"),
                Action::LoadProject | Action::SaveProject => unreachable!("project file operation returned above"),
                Action::Import { .. } => Output::Prepared(Prepared {
                    name,
                    bytes: Arc::new(bytes),
                    animation: Arc::new(animation),
                    pose: false,
                }),
                Action::Save { .. } => {
                    let Some(path) = rfd::FileDialog::new()
                        .add_filter(aurora_i18n::tr!("anim-edit-file-filter"), &["anim"])
                        .set_file_name(format!("{name}.anim"))
                        .save_file()
                    else {
                        return Ok(Output::Cancelled);
                    };
                    write_atomic(&path, &bytes)?;
                    Output::Saved(path)
                }
            })
        };
        let output = work().unwrap_or_else(Output::Failed);
        let _ = tx.send(output);
        ctx.request_repaint();
    });
}

/// The account's autosaved draft, a project file in its recovery folder.
pub(super) const RECOVERY: &str = "recovery.auroraanim";

/// The recovery file the first autosave of a session moved aside.
pub(super) const RECOVERY_PREVIOUS: &str = "recovery.previous.auroraanim";

/// Keep a recovery file the user has not opened: the first write of a
/// session moves it to `RECOVERY_PREVIOUS` (replacing the one before), so
/// the new draft never overwrites it.
pub(super) fn rotate_recovery(dir: &std::path::Path) -> std::io::Result<()> {
    let current = dir.join(RECOVERY);
    if current.is_file() {
        std::fs::rename(&current, dir.join(RECOVERY_PREVIOUS))?;
    }
    Ok(())
}

/// Write the draft as a project file in `dir`, atomically: a crash while
/// writing keeps the previous one. A failure is logged, never shown.
pub(super) fn autosave(
    jobs: &Jobs,
    tx: crossbeam_channel::Sender<Output>,
    document: Arc<Document>,
    revision: u64,
    dir: PathBuf,
    first: bool,
) {
    jobs.execute(move || {
        if first && let Err(error) = rotate_recovery(&dir) {
            log::warn!("animation draft autosave: keeping the previous draft: {error}");
            return;
        }
        let bytes = match document.project_json() {
            Ok(bytes) => bytes,
            Err(key) => {
                log::warn!("animation draft autosave: {key}");
                return;
            }
        };
        let path = dir.join(RECOVERY);
        match std::fs::create_dir_all(&dir)
            .map_err(|error| error.to_string())
            .and_then(|()| write_atomic(&path, &bytes))
        {
            Ok(()) => {
                let _ = tx.send(Output::Autosaved(revision));
            }
            Err(error) => log::warn!("animation draft autosave to {}: {error}", path.display()),
        }
    });
}

/// Read the autosaved draft back as a document (`Output::Loaded`).
pub(super) fn recover(jobs: &Jobs, ctx: &egui::Context, tx: crossbeam_channel::Sender<Output>, path: PathBuf, rig: Arc<Rig>) {
    let ctx = ctx.clone();
    jobs.execute(move || {
        let output = read_file(&path)
            .and_then(|bytes| Document::from_project_json(&bytes, &rig).map_err(|key| aurora_i18n::text(key).to_owned()))
            .map_or_else(Output::Failed, Output::Loaded);
        let _ = tx.send(output);
        ctx.request_repaint();
    });
}
