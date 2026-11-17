//! Pose library, native animations and BVH files use the background job pool.
use super::*;
use document::{Document, LoadMode};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone)]
pub(super) struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub date: u64,
}
impl Entry {
    pub fn date_label(&self) -> String {
        // Proleptic Gregorian civil date from an epoch-day count (Hinnant).
        let z = self.date as i64 / 86400 + 719468;
        let era = z / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let mut year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = mp + if mp < 10 { 3 } else { -9 };
        year += i64::from(month <= 2);
        format!("{year:04}-{month:02}-{day:02}")
    }
}
pub(super) enum Loaded {
    Pose(Document, LoadMode),
    /// The animation, how it loads, and the name its file gives the field.
    Animation(aurora_assets::Animation, LoadMode, String),
}
enum Output {
    Listed(Vec<Entry>),
    Loaded(Uuid, Loaded),
    Overwrite(Save),
    Saved(PathBuf),
    /// A standalone `.anim` written by Save as animation…
    AnimationSaved(PathBuf),
    /// Saved outside the library by Save as…
    SavedTo(PathBuf),
    /// A folder of Legacy Viewer poses converted into the library.
    ImportedFolder(legacy_pose::Imported),
    Failed(String),
    Cancelled,
}
pub(super) enum Job {
    List,
    Load(Option<PathBuf>, LoadMode, Uuid),
    Save(Save),
    /// Save as animation…: the animation and the name the dialog offers.
    SaveAnimation(Arc<aurora_assets::Animation>, String),
    /// Convert a folder of Legacy Viewer or Alchemy poses into the library:
    /// the one given, or the one chosen in the folder dialog.
    ImportLegacyFolder(Option<PathBuf>),
}
/// Whether a library entry named `name` shows under the search `filter`
/// (case-insensitive, anywhere in the name).
pub(super) fn filter_matches(name: &str, filter: &str) -> bool {
    name.to_lowercase().contains(&filter.to_lowercase())
}
/// A save request. `pick` asks for the file in the system dialog (Save as…)
/// instead of the library; the dialog asks about an existing file itself, so
/// `confirm` only applies to the library.
#[derive(Clone)]
pub(super) struct Save {
    pub document: Document,
    pub confirm: bool,
    pub pick: bool,
}
/// The pose file a Save as… writes: the chosen name, with
/// `.aurora-pose.json` added when it does not already end in `.json`, so
/// that the library and Open a file find it.
fn pose_file(path: PathBuf) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name.ends_with(".json") {
        path
    } else {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".aurora-pose.json");
        path.with_file_name(name)
    }
}
/// The file a Save as animation… writes: the chosen name, with `.anim`
/// added unless it already ends in it.
fn anim_file(path: PathBuf) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name.ends_with(".anim") {
        path
    } else {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".anim");
        path.with_file_name(name)
    }
}
/// Write the `.anim` of `animation` at `path` (extension added when missing),
/// checked with the reader the upload window uses, through a temporary file
/// so that a failure leaves nothing half written. Never uploads.
pub(super) fn write_animation(path: PathBuf, animation: &aurora_assets::Animation) -> Result<PathBuf, String> {
    let bytes = total::encode(animation)?;
    let path = anim_file(path);
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let temp = dir.join(format!("{}.tmp", Uuid::new_v4()));
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(e.to_string());
    }
    Ok(path)
}
pub(super) struct Service {
    pub entries: Vec<Entry>,
    pub busy: bool,
    pub message: Option<String>,
    pub selected: Option<usize>,
    pub saved_path: Option<PathBuf>,
    pub filter: String,
    /// The saved pose was just selected: the library list scrolls to it once.
    pub scroll_to_selected: bool,
    pub job: Option<Job>,
    pub ready: bool,
    pub overwrite: Option<Save>,
    /// Loads that arrived without a job (an inventory animation), applied
    /// by the next `tick` like the files'.
    pub(super) injected: Vec<(Uuid, Loaded)>,
    tx: crossbeam_channel::Sender<Output>,
    rx: crossbeam_channel::Receiver<Output>,
}
impl Default for Service {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            entries: Vec::new(),
            busy: false,
            message: None,
            selected: None,
            saved_path: None,
            filter: String::new(),
            scroll_to_selected: false,
            job: None,
            ready: false,
            overwrite: None,
            injected: Vec::new(),
            tx,
            rx,
        }
    }
}
/// Write `bytes` to `path` in `dir` through a temporary file, so that a
/// failure leaves no half-written pose in the library.
pub(super) fn write_atomic(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = dir.join(format!("{}.tmp", Uuid::new_v4()));
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(e.to_string());
    }
    Ok(())
}
pub(super) fn read(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(aurora_i18n::tr!("poser-file-too-large").to_string());
    }
    Ok(bytes)
}
fn list(root: &Path, rig: &Rig) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    // the library of a long-time Legacy Viewer user can hold thousands of poses
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())?.take(4096) {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_file()
            && path.file_name().is_some_and(|n| n.to_string_lossy().ends_with(".aurora-pose.json"))
            && let Ok(document) = read(&path).and_then(|bytes| serde_json::from_slice::<Document>(&bytes).map_err(|e| e.to_string()))
        {
            if !document.valid(rig) {
                continue;
            }
            let date = path
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map_or(0, |t| t.as_secs());
            out.push(Entry {
                path,
                name: document.name,
                date,
            });
        }
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}
impl Service {
    pub(super) fn inject(&mut self, subject: Uuid, loaded: Loaded) {
        self.injected.push((subject, loaded));
    }
    pub fn tick(
        &mut self,
        jobs: &Jobs,
        ctx: &egui::Context,
        owner: Uuid,
        rig: Arc<Rig>,
        skeleton: Option<&aurora_assets::Skeleton>,
    ) -> Vec<(Uuid, Loaded)> {
        let mut loaded = std::mem::take(&mut self.injected);
        while let Ok(output) = self.rx.try_recv() {
            self.busy = false;
            match output {
                Output::Listed(entries) => {
                    let saved_path = self.saved_path.take();
                    let kept = self.selected.and_then(|i| self.entries.get(i)).map(|e| e.path.clone());
                    let from_save = kept.is_none() && saved_path.is_some();
                    let selected = kept.or(saved_path);
                    self.selected = selected.and_then(|path| entries.iter().position(|e| e.path == path));
                    // the pose just saved: shown even under a search that
                    // would hide it, and scrolled to once
                    if from_save && let Some(entry) = self.selected.and_then(|i| entries.get(i)) {
                        if !filter_matches(&entry.name, &self.filter) {
                            self.filter.clear();
                        }
                        self.scroll_to_selected = true;
                    }
                    self.entries = entries;
                    self.ready = true;
                }
                Output::Loaded(subject, value) => loaded.push((subject, value)),
                Output::Overwrite(save) => self.overwrite = Some(save),
                Output::Saved(path) => {
                    self.ready = false;
                    self.selected = None;
                    self.message = Some(aurora_i18n::tr!("poser-saved-to", path = path.display().to_string()).to_string());
                    self.saved_path = Some(path);
                }
                Output::AnimationSaved(path) => {
                    self.message = Some(aurora_i18n::tr!("poser-animation-saved", path = path.display().to_string()).to_string());
                    self.ready = true;
                }
                Output::SavedTo(path) => {
                    self.ready = false;
                    let path = path.display().to_string();
                    self.message = Some(aurora_i18n::tr!("poser-saved-to", path = path.as_str()).to_string());
                }
                Output::ImportedFolder(imported) => {
                    self.ready = false;
                    self.message = Some(
                        aurora_i18n::tr!(
                            "poser-imported",
                            added = imported.added,
                            kept = imported.kept,
                            unreadable = imported.unreadable
                        )
                        .to_string(),
                    );
                }
                Output::Failed(message) => {
                    self.message = Some(message);
                    self.ready = true;
                }
                Output::Cancelled => {}
            }
        }
        if self.busy {
            return loaded;
        }
        let Some(job) = self.job.take().or_else(|| (!self.ready).then_some(Job::List)) else {
            return loaded;
        };
        self.busy = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let skeleton = skeleton.cloned().map(Arc::new);
        let root = crate::settings::account_dir(&owner).join("poses");
        jobs.execute(move || {
            let work = || -> Result<Output, String> {
                Ok(match job {
                    Job::List => Output::Listed(list(&root, &rig)?),
                    Job::Load(path, mode, subject) => {
                        let path = match path {
                            Some(p) => p,
                            None => {
                                let mut dialog = rfd::FileDialog::new();
                                if root.is_dir() {
                                    dialog = dialog.set_directory(&root);
                                }
                                let Some(p) = dialog
                                    .add_filter(aurora_i18n::tr!("wornposer-file-filter"), &["json", "anim", "bvh", "xml"])
                                    .pick_file()
                                else {
                                    return Ok(Output::Cancelled);
                                };
                                p
                            }
                        };
                        let bytes = read(&path)?;
                        // the field's name: the file's, as the field takes it
                        let stem: String = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default()
                            .chars()
                            .filter(|c| !c.is_control() && *c != '|')
                            .take(100)
                            .collect();
                        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("anim")) {
                            Output::Loaded(
                                subject,
                                Loaded::Animation(
                                    aurora_assets::anim::parse_animation_upload(&bytes).map_err(|e| e.to_string())?,
                                    mode,
                                    stem,
                                ),
                            )
                        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bvh")) {
                            let skeleton = skeleton.as_ref().ok_or_else(|| aurora_i18n::tr!("anim-edit-invalid").to_string())?;
                            Output::Loaded(
                                subject,
                                Loaded::Animation(
                                    aurora_assets::import_animation::parse_bvh_pose(&bytes, skeleton)
                                        .map_err(|e| e.to_string())?
                                        .animation,
                                    mode,
                                    stem,
                                ),
                            )
                        } else if legacy_pose::is_pose_file(&path) || bd_pose::looks_like(&bytes) {
                            let name: String = path
                                .file_stem()
                                .map(|n| n.to_string_lossy().chars().filter(|c| !c.is_control()).take(100).collect())
                                .unwrap_or_default();
                            let document = match bd_pose::parse(&bytes) {
                                // a Black Dragon pose, named after its file
                                Ok(entries) => {
                                    let joints = bd_pose::document_joints(&rig, &entries);
                                    let name = if name.trim().is_empty() {
                                        aurora_i18n::tr!("wornposer-bd-unnamed").to_string()
                                    } else {
                                        name
                                    };
                                    let document = Document::from_joints(name, 4, joints);
                                    if !document.valid(&rig) {
                                        return Err(aurora_i18n::tr!("wornposer-bd-invalid").to_string());
                                    }
                                    document
                                }
                                // the Legacy Viewer's and Alchemy's own poser files,
                                // which are not read as Black Dragon's
                                Err(_) if legacy_pose::is_pose_file(&path) => {
                                    legacy_pose::document(&bytes, legacy_pose::pose_name(&path), &rig)?
                                }
                                Err(e) => return Err(e),
                            };
                            Output::Loaded(subject, Loaded::Pose(document, mode))
                        } else {
                            let document: Document = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                            if !document.valid(&rig) {
                                return Err(aurora_i18n::tr!("anim-edit-invalid").to_string());
                            }
                            Output::Loaded(subject, Loaded::Pose(document, mode))
                        }
                    }
                    Job::Save(save) => {
                        let Save { document, confirm, pick } = &save;
                        let (confirm, pick) = (*confirm, *pick);
                        if !document.valid(&rig) {
                            return Err(aurora_i18n::tr!("anim-edit-invalid").to_string());
                        }
                        let name = clip::files::basename(&document.name);
                        let path = if pick {
                            // Best effort: the dialog opens in the library when it exists.
                            let _ = std::fs::create_dir_all(&root);
                            let Some(path) = rfd::FileDialog::new()
                                .set_directory(&root)
                                .set_file_name(format!("{name}.aurora-pose.json"))
                                .add_filter(aurora_i18n::tr!("poser-pose-filter"), &["json"])
                                .save_file()
                            else {
                                return Ok(Output::Cancelled);
                            };
                            pose_file(path)
                        } else {
                            root.join(format!("{name}.aurora-pose.json"))
                        };
                        let dir = path.parent().unwrap_or(&root).to_path_buf();
                        if confirm && !pick && path.exists() {
                            return Ok(Output::Overwrite(save));
                        }
                        let bytes = serde_json::to_vec_pretty(document).map_err(|e| e.to_string())?;
                        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                        let temp = dir.join(format!("{}.tmp", Uuid::new_v4()));
                        std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
                        if let Err(e) = std::fs::rename(&temp, &path) {
                            let _ = std::fs::remove_file(&temp);
                            return Err(e.to_string());
                        }
                        if pick && dir != root {
                            Output::SavedTo(path)
                        } else {
                            Output::Saved(path)
                        }
                    }
                    Job::SaveAnimation(animation, name) => {
                        let _ = std::fs::create_dir_all(&root);
                        let Some(path) = rfd::FileDialog::new()
                            .set_directory(&root)
                            .set_file_name(format!("{}.anim", clip::files::basename(&name)))
                            .add_filter(aurora_i18n::tr!("poser-animation-filter"), &["anim"])
                            .save_file()
                        else {
                            return Ok(Output::Cancelled);
                        };
                        Output::AnimationSaved(write_animation(path, &animation)?)
                    }
                    Job::ImportLegacyFolder(folder) => {
                        let folder = match folder {
                            Some(folder) => folder,
                            None => {
                                let mut dialog = rfd::FileDialog::new().set_title(aurora_i18n::tr!("poser-import-legacy-title"));
                                if let Some(dir) = legacy_pose::default_folder() {
                                    dialog = dialog.set_directory(dir);
                                }
                                let Some(folder) = dialog.pick_folder() else {
                                    return Ok(Output::Cancelled);
                                };
                                folder
                            }
                        };
                        Output::ImportedFolder(legacy_pose::import_folder(&folder, &root, &rig)?)
                    }
                })
            };
            let _ = tx.send(work().unwrap_or_else(Output::Failed));
            ctx.request_repaint();
        });
        loaded
    }
}
#[cfg(test)]
mod save_as_tests {
    use super::{anim_file, pose_file};
    use std::path::{Path, PathBuf};

    #[test]
    fn pose_file_adds_the_pose_extension_unless_json() {
        assert_eq!(pose_file(PathBuf::from("/d/Wave")), Path::new("/d/Wave.aurora-pose.json"));
        assert_eq!(pose_file(PathBuf::from("/d/Wave.v2")), Path::new("/d/Wave.v2.aurora-pose.json"));
        assert_eq!(
            pose_file(PathBuf::from("/d/Wave.aurora-pose.json")),
            Path::new("/d/Wave.aurora-pose.json")
        );
        assert_eq!(pose_file(PathBuf::from("/d/Wave.JSON")), Path::new("/d/Wave.JSON"));
    }
    #[test]
    fn anim_file_adds_the_extension_unless_anim() {
        assert_eq!(anim_file(PathBuf::from("/d/Wave")), Path::new("/d/Wave.anim"));
        assert_eq!(anim_file(PathBuf::from("/d/Wave.v2")), Path::new("/d/Wave.v2.anim"));
        assert_eq!(anim_file(PathBuf::from("/d/Wave.ANIM")), Path::new("/d/Wave.ANIM"));
        assert_eq!(
            anim_file(PathBuf::from("/d/Wave.aurora-pose.json")),
            Path::new("/d/Wave.aurora-pose.json.anim")
        );
    }
}
