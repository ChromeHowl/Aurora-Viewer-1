//! The JPEG 2000 data of the textures a backup carries: each one fetched at
//! its full resolution, then read as the grid sent it (the file holds the
//! original data, never a decoded image: FSFloaterObjectExport::
//! saveFormattedImage). Same wait as the Collada textures
//! (`TextureSaver`): a texture that does not arrive within a minute is left
//! out of the file, the others are still written.

use crate::scene::Scene;
use crossbeam_channel::Receiver;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// TEXTURE_DOWNLOAD_TIMEOUT.
const TIMEOUT: Duration = Duration::from_secs(60);
/// Textures fetched at once.
const PARALLEL: usize = 4;
/// The pixels asked of a texture: its full size.
const FULL_SIZE_PX: f32 = 16384.0;

struct Job {
    id: Uuid,
    since: Instant,
    reading: Option<Receiver<Result<Vec<u8>, String>>>,
}

/// The data of one backup's textures, read over the next frames.
pub struct J2cFetch {
    queue: VecDeque<Uuid>,
    active: Vec<Job>,
    pub total: usize,
    /// The data read, by texture.
    pub data: HashMap<Uuid, Vec<u8>>,
    /// How many textures are left out.
    pub missing: usize,
    /// To keep resident while they are read (drained by the app).
    pub wanted_images: HashSet<Uuid>,
}

impl J2cFetch {
    pub fn new(ids: &[Uuid]) -> Self {
        Self {
            queue: ids.iter().copied().collect(),
            active: Vec::new(),
            total: ids.len(),
            data: HashMap::new(),
            missing: 0,
            wanted_images: HashSet::new(),
        }
    }

    pub fn done(&self) -> bool {
        self.queue.is_empty() && self.active.is_empty()
    }

    /// Textures not read or given up yet.
    pub fn left(&self) -> usize {
        self.total.saturating_sub(self.data.len() + self.missing)
    }

    /// One frame of work: start, watch and finish the textures.
    pub fn poll(&mut self, ctx: &egui::Context, scene: &mut Scene, now: Instant) {
        while self.active.len() < PARALLEL
            && let Some(id) = self.queue.pop_front()
        {
            self.active.push(Job {
                id,
                since: now,
                reading: None,
            });
        }
        let mut finished = Vec::new();
        for (n, job) in self.active.iter_mut().enumerate() {
            if let Some(rx) = &job.reading {
                if let Ok(result) = rx.try_recv() {
                    finished.push((n, result));
                }
                continue;
            }
            if now.duration_since(job.since) > TIMEOUT {
                log::warn!("object export: texture timed out");
                finished.push((n, Err("timed out".into())));
                continue;
            }
            self.wanted_images.insert(job.id);
            scene.textures.note_usage_fixed(&job.id, FULL_SIZE_PX);
            let source = scene.textures.preview_source(&job.id);
            // a texture the viewer made has no grid data to keep
            if source.local.is_some() || source.procedural {
                finished.push((n, Err("made by the viewer".into())));
                continue;
            }
            // the original's data (discard 0), never a smaller level
            if source.decoded != Some(0) {
                continue;
            }
            let (tx, rx) = crossbeam_channel::bounded(1);
            job.reading = Some(rx);
            let wake = ctx.clone();
            scene.jobs.execute(move || {
                let data = if source.bytes.is_empty() {
                    crate::cache::read(&source.jpeg).map(Arc::new).map_err(|e| e.to_string())
                } else {
                    Ok(source.bytes.clone())
                };
                let _ = tx.send(data.map(|d| d.to_vec()));
                wake.request_repaint();
            });
        }
        for (n, result) in finished.into_iter().rev() {
            let job = self.active.swap_remove(n);
            match result {
                Ok(bytes) if !bytes.is_empty() => {
                    self.data.insert(job.id, bytes);
                }
                Ok(_) => self.missing += 1,
                Err(reason) => {
                    log::warn!("object export: a texture is not in the backup: {reason}");
                    self.missing += 1;
                }
            }
        }
    }
}
