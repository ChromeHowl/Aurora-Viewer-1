//! "Save as ▸ Backup / Collada" of a linkset (the Legacy Viewer's
//! FSFloaterObjectExport and ColladaExportFloater, behaviour only). The
//! window reads the full ObjectProperties of every prim (and, for Collada,
//! the meshes' headers and geometry), applies the creator rule to each prim
//! and each texture ([`plan`]), tells the user what is left out and why, and
//! writes the file chosen in a save dialog, off the main thread.

pub mod dae;
pub mod oxp;
pub mod plan;
#[cfg(test)]
mod tests;

use crate::build::BuildTool;
use crate::scene::meshes::{MeshStreamer, MeshView};
use crate::theme::Palette;
use crate::ui::widgets;
use crate::world::World;
use crate::world::inventory::actions::{self, ExportFormat, ExportRefusal};
use aurora_net::NetCommand;
use glam::{Mat4, Quat, Vec3};
use plan::{LeftOut, Plan, PrimInput};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// How long the window waits for properties and meshes before it plans
/// with what came (what is still missing is then refused).
const WAIT: Duration = Duration::from_secs(20);

struct Job {
    format: ExportFormat,
    /// The clicked prim; its linkset is exported.
    clicked: Uuid,
    asked: bool,
    since: Instant,
    plan: Option<Plan>,
    name: String,
    saving: Option<crossbeam_channel::Receiver<String>>,
    message: String,
}

#[derive(Default)]
pub struct ExportUi {
    job: Option<Job>,
}

/// What the window needs from the app each frame.
pub struct Sources<'a> {
    pub world: &'a World,
    pub build: &'a BuildTool,
    pub meshes: &'a mut MeshStreamer,
    pub texture_cache: &'a Path,
    pub grid: String,
}

/// The linkset root of a prim: its parents up to the one sitting on no
/// other prim (an attachment's root is the prim on the avatar).
fn root_of(world: &World, idx: usize) -> usize {
    let mut i = idx;
    for _ in 0..256 {
        match world.objects.get(i).and_then(|o| world.objects.parent_of(o)) {
            Some(parent) if world.objects.get(parent).is_some_and(|o| !o.is_avatar()) => i = parent,
            _ => break,
        }
    }
    i
}

fn mesh_of(p: &PrimInput) -> Option<Uuid> {
    p.volume.sculpt.or(p.extra.sculpt).filter(|s| s.is_mesh()).map(|s| s.texture)
}

/// The prims of the clicked prim's linkset, root first.
fn inputs(world: &World, build: &BuildTool, meshes: &MeshStreamer, clicked: Uuid) -> Vec<PrimInput> {
    let Some(idx) = world.objects.index_of_uuid(&clicked) else {
        return Vec::new();
    };
    let root = root_of(world, idx);
    let root_id = world.objects.get(root).map_or(Uuid::nil(), |o| o.full_id);
    crate::build::family(world, root)
        .into_iter()
        .filter_map(|i| world.objects.get(i))
        .filter(|o| !o.is_avatar())
        .map(|o| {
            let sculpt = o.volume.sculpt.or(o.extra.sculpt);
            let faces = match sculpt {
                Some(s) if s.is_mesh() => o.te.as_ref().map_or(1, |te| te.faces.len().min(8)),
                Some(_) => 1,
                None => aurora_prim::volume::num_faces(&o.volume),
            };
            PrimInput {
                id: o.full_id,
                parent: (o.full_id != root_id).then_some(root_id),
                props: build.props.get(&o.full_id).filter(|p| p.full).cloned(),
                mesh_creator: sculpt
                    .filter(|s| s.is_mesh())
                    .and_then(|s| meshes.view(&s.texture))
                    .map(|v| v.header.creator),
                material: o.prim_material,
                click_action: o.click_action,
                flags: o.update_flags,
                attachment_point: if o.full_id == root_id { o.attachment_point() } else { 0 },
                position: o.position,
                rotation: o.rotation,
                scale: o.scale,
                volume: o.volume,
                faces,
                te: o.te.as_deref().cloned().unwrap_or_default(),
                extra: o.extra.clone(),
            }
        })
        .collect()
}

fn reason(why: LeftOut) -> &'static str {
    aurora_i18n::text(match why {
        LeftOut::Rights(ExportRefusal::NotOwner) => "b8-export-why-not-owner",
        LeftOut::Rights(ExportRefusal::NotCreator) => "b8-export-why-not-creator",
        LeftOut::Rights(ExportRefusal::NotFullPermission) => "b8-export-why-not-full",
        LeftOut::Rights(ExportRefusal::MeshInBackup) => "b8-export-why-mesh-backup",
        LeftOut::Rights(ExportRefusal::MeshCreator) => "b8-export-why-mesh-creator",
        LeftOut::Rights(ExportRefusal::SculptCreator) => "b8-export-why-sculpt",
        LeftOut::Geometry => "b8-export-why-geometry",
    })
}

/// The file name offered in the save dialog (LLDir::getScrubbedFileName).
pub fn file_name(name: &str, format: ExportFormat) -> String {
    let base: String = name
        .trim()
        .chars()
        .map(|c| if c.is_control() || r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let base = if base.is_empty() { "object".to_owned() } else { base };
    format!(
        "{base}.{}",
        match format {
            ExportFormat::Backup => "oxp",
            ExportFormat::Collada => "dae",
        }
    )
}

/// Each prim's transform relative to the root's position: the root keeps
/// its rotation, a child is placed by the root's rotation (children's
/// positions and rotations are relative to the root, as the simulator
/// sends them). The scale is in the transform.
pub fn transforms(prims: &[PrimInput]) -> Vec<Mat4> {
    let root_rotation = prims.first().map_or(Quat::IDENTITY, |r| r.rotation);
    prims
        .iter()
        .map(|p| {
            let local =
                Mat4::from_scale_rotation_translation(p.scale, p.rotation, if p.parent.is_some() { p.position } else { Vec3::ZERO });
            if p.parent.is_some() {
                Mat4::from_quat(root_rotation) * local
            } else {
                local
            }
        })
        .collect()
}

enum Shape {
    Prim(aurora_prim::params::VolumeParams),
    Mesh(MeshView),
}

/// The Collada faces of one prim (off the main thread).
fn faces(shape: &Shape, colors: &[[f32; 4]]) -> Vec<dae::Face> {
    let color = |i: usize| colors.get(i).copied().unwrap_or([1.0; 4]);
    match shape {
        Shape::Prim(volume) => aurora_prim::volume::generate_volume(volume, aurora_prim::volume::DETAIL_SCALES[3])
            .faces
            .into_iter()
            .enumerate()
            .map(|(i, f)| dae::Face {
                positions: f.positions,
                normals: f.normals,
                uvs: f.uvs,
                indices: f.indices,
                color: color(i),
            })
            .collect(),
        Shape::Mesh(view) => view
            .lod_section(3)
            .and_then(|section| aurora_assets::decode_mesh_lod(section).ok())
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, f)| dae::Face {
                positions: f.positions,
                normals: f.normals,
                uvs: f.uvs,
                indices: f.indices,
                color: color(i),
            })
            .collect(),
    }
}

impl ExportUi {
    pub fn open(&mut self, format: ExportFormat, clicked: Uuid) {
        self.job = Some(Job {
            format,
            clicked,
            asked: false,
            since: Instant::now(),
            plan: None,
            name: String::new(),
            saving: None,
            message: String::new(),
        });
    }

    /// Draw the window; returns the property requests to send.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, src: Sources<'_>) -> Vec<NetCommand> {
        let mut commands = Vec::new();
        let Some(job) = self.job.as_mut() else {
            return commands;
        };
        if let Some(rx) = &job.saving
            && let Ok(message) = rx.try_recv()
        {
            job.message = message;
            job.saving = None;
        }
        let prims = inputs(src.world, src.build, src.meshes, job.clicked);
        if !job.asked && !prims.is_empty() {
            job.asked = true;
            job.since = Instant::now();
            for prim in &prims {
                if let Some(o) = src.world.objects.index_of_uuid(&prim.id).and_then(|i| src.world.objects.get(i)) {
                    commands.extend(src.build.prim_properties_query(src.world, o.key));
                }
                if let Some(mesh) = mesh_of(prim).filter(|_| job.format == ExportFormat::Collada) {
                    src.meshes.want(mesh, 3);
                }
            }
        }
        let has_geometry = |prim: &PrimInput| mesh_of(prim).is_none_or(|m| src.meshes.view(&m).is_some_and(|v| v.lod_section(3).is_some()));
        let ready = !prims.is_empty()
            && prims
                .iter()
                .all(|prim| prim.props.is_some() && (job.format == ExportFormat::Backup || has_geometry(prim)));
        if job.plan.is_none() && (ready || (job.asked && job.since.elapsed() > WAIT)) {
            job.name = prims.first().map(|r| r.name().to_owned()).unwrap_or_default();
            job.plan = Some(plan::plan(&src.world.inventory, prims, job.format, has_geometry));
        } else if job.plan.is_none() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        let mut open = true;
        let mut save = false;
        let title = aurora_i18n::text(match job.format {
            ExportFormat::Backup => "b8-export-title-backup",
            ExportFormat::Collada => "b8-export-title-collada",
        });
        widgets::Floater::new("b8_export", title, egui::pos2(260.0, 140.0), egui::vec2(460.0, 380.0)).show(ctx, p, &mut open, |ui| {
            ui.label(aurora_i18n::tr!("b8-export-rule"));
            widgets::divider(ui, p);
            let Some(plan) = &job.plan else {
                ui.label(if job.asked {
                    aurora_i18n::tr!("b8-export-waiting")
                } else {
                    aurora_i18n::tr!("b8-export-no-object")
                });
                return;
            };
            ui.label(aurora_i18n::tr!("b8-export-object", name = job.name.clone()));
            if let Some(why) = plan.refused() {
                ui.colored_label(p.danger, aurora_i18n::tr!("b8-export-refused", reason = reason(why)));
                return;
            }
            ui.label(aurora_i18n::tr!("b8-export-prims", count = plan.prims.len() as i64));
            if job.format == ExportFormat::Backup {
                ui.label(aurora_i18n::tr!("b8-export-textures-in", count = plan.textures.len() as i64));
            } else {
                ui.label(aurora_i18n::tr!("b8-export-collada-no-textures"));
            }
            if plan.replaced_faces > 0 {
                ui.colored_label(
                    p.warn,
                    aurora_i18n::tr!("b8-export-textures-replaced", count = plan.replaced_faces as i64),
                );
            }
            if plan.dropped > 0 {
                ui.colored_label(p.warn, aurora_i18n::tr!("b8-export-dropped", count = plan.dropped as i64));
            }
            if !plan.left_out.is_empty() {
                ui.colored_label(p.warn, aurora_i18n::tr!("b8-export-left-out", count = plan.left_out.len() as i64));
                egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                    for (name, why) in &plan.left_out {
                        let name = if name.is_empty() {
                            aurora_i18n::tr!("b8-export-unnamed")
                        } else {
                            name.as_str()
                        };
                        ui.label(format!("{name}: {}", reason(*why)));
                    }
                });
            }
            widgets::divider(ui, p);
            ui.horizontal(|ui| {
                save = widgets::action_button_if(
                    ui,
                    p,
                    "device-floppy-filled",
                    aurora_i18n::tr!("b8-export-save"),
                    widgets::ActionKind::Primary,
                    egui::Vec2::ZERO,
                    job.saving.is_none(),
                )
                .clicked();
                if job.saving.is_some() {
                    ui.spinner();
                }
            });
            if !job.message.is_empty() {
                ui.label(job.message.as_str());
            }
        });
        if save && let Some(plan) = job.plan.clone() {
            job.saving = Some(start_save(job.format, &job.name, plan, &src));
        }
        if !open {
            self.job = None;
        }
        commands
    }
}

/// Ask for the file and write it on a thread; the message for the window
/// comes back on the channel (empty when the dialog was cancelled).
fn start_save(format: ExportFormat, name: &str, plan: Plan, src: &Sources<'_>) -> crossbeam_channel::Receiver<String> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let file = file_name(name, format);
    let author = src.world.own_name();
    let client = format!("Aurora Viewer {}", aurora_version::version());
    let filter = match format {
        ExportFormat::Backup => ("OXP", "oxp"),
        ExportFormat::Collada => ("Collada", "dae"),
    };
    let job: Box<dyn FnOnce(&Path) -> std::io::Result<String> + Send> = match format {
        ExportFormat::Backup => {
            // the texture data: the rule's item names, the cached JPEG 2000 data
            let inv = &src.world.inventory;
            let wanted: Vec<(Uuid, String, String, PathBuf)> = plan
                .textures
                .iter()
                .map(|id| {
                    let item = actions::export_asset_item(inv, *id);
                    (
                        *id,
                        item.map_or_else(String::new, |it| it.name.clone()),
                        item.map_or_else(String::new, |it| it.desc.clone()),
                        crate::cache::path(src.texture_cache, "tex", id, "j2c"),
                    )
                })
                .collect();
            let meta = oxp::Meta {
                client,
                date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
                author,
                grid: src.grid.clone(),
            };
            Box::new(move |path: &Path| {
                let mut assets = Vec::new();
                let mut missing = 0usize;
                for (id, name, description, file) in wanted {
                    match crate::cache::read(&file) {
                        Ok(j2c) if !j2c.is_empty() => assets.push(oxp::Asset {
                            id,
                            name,
                            description,
                            j2c,
                        }),
                        _ => missing += 1,
                    }
                }
                let bytes = oxp::encode(&oxp::manifest(&plan, &assets, &meta))?;
                std::fs::write(path, bytes)?;
                let mut message = aurora_i18n::tr!("b8-export-saved", path = path.display().to_string());
                if missing > 0 {
                    message.push(' ');
                    message.push_str(&aurora_i18n::tr!("b8-export-not-cached", count = missing as i64));
                }
                Ok(message)
            })
        }
        ExportFormat::Collada => {
            let shapes: Vec<(String, Mat4, Shape, Vec<[f32; 4]>)> = transforms(&plan.prims)
                .into_iter()
                .zip(&plan.prims)
                .map(|(transform, prim)| {
                    let shape = match mesh_of(prim).and_then(|m| src.meshes.view(&m)) {
                        Some(view) => Shape::Mesh(view),
                        None => Shape::Prim(prim.volume),
                    };
                    let colors = prim.te.faces.iter().map(|f| f.color).collect();
                    (prim.name().to_owned(), transform, shape, colors)
                })
                .collect();
            Box::new(move |path: &Path| {
                let nodes: Vec<dae::Node> = shapes
                    .iter()
                    .map(|(name, transform, shape, colors)| dae::Node {
                        name: name.clone(),
                        transform: *transform,
                        faces: faces(shape, colors),
                    })
                    .collect();
                std::fs::write(path, dae::write(&nodes, &author, &client))?;
                Ok(aurora_i18n::tr!("b8-export-saved", path = path.display().to_string()))
            })
        }
    };
    std::thread::spawn(move || {
        let message = match rfd::FileDialog::new()
            .set_title(aurora_i18n::tr!("common-save-as"))
            .set_file_name(file)
            .add_filter(filter.0, &[filter.1])
            .save_file()
        {
            Some(path) => job(&path).unwrap_or_else(|e| aurora_i18n::tr!("b8-export-failed", reason = e.to_string())),
            None => String::new(),
        };
        let _ = tx.send(message);
    });
    rx
}
