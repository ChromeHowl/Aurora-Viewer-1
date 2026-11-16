//! Poser › Load › Inventory animation: an animation of the agent's inventory
//! read into the poser like a `.anim` file, its first frame as the pose.
//!
//! The Legacy Viewer's poser loads its own pose files only
//! (`FSFloaterPoser::loadPoseFromXml`, indra/newview/fsfloaterposer.cpp), and
//! its animation preview only plays an animation
//! (`LLPreviewAnim`, indra/newview/llpreviewanim.cpp), so it has no rule for
//! reading an animation's motion into an editor. The motion read here can be
//! saved and uploaded again as a new asset, so the rule is the one that lets
//! an asset UUID be shown (`LLViewerInventoryItem::getIsFullPerm` and
//! `getProtectedAssetUUID`, indra/newview/llviewerinventory.cpp): copy,
//! modify and transfer, through `actions::content_missing`. Stricter than the
//! Legacy Viewer, which plays any animation it holds. An item the rule
//! refuses is listed greyed out, with the rights it lacks; its asset id is
//! never shown anywhere.
//!
//! The asset comes through the animation streamer (`AnimStreamer`: the disk
//! cache, then the simulator's animation capability), as the Animations
//! panel's animations do, and loads through the same path as a file
//! (`Editor::apply_loaded`): the item's name in the name field, one undo step,
//! the four load modes.
use super::*;
use crate::{
    scene::anim::{AnimStreamer, BoundAnim},
    world::inventory::{Inventory, actions},
};
use document::LoadMode;

/// How long the asset may take before the load gives up.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(60);

/// Why an animation item is not loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    /// The rights of `actions::FULL` the agent lacks.
    Rights(u32),
    /// Not an animation, no asset yet, or in the Trash.
    Unavailable,
}

/// An animation item the rule allows.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Source {
    pub item: Uuid,
    pub asset: Uuid,
    pub name: String,
}

/// The animation item an id names (a link resolves to its original), None
/// when it is none, has no asset yet or is in the Trash.
fn animation_item(inv: &Inventory, id: Uuid) -> Option<&aurora_net::inventory::InvItem> {
    let original = actions::original(inv, id)?;
    let it = inv.items.get(&original)?;
    (it.asset_type == 20 && !it.asset_id.is_nil() && !inv.in_folder_type(original, 14)).then_some(it)
}

/// The rule: the animation of the item `id`, or why it is refused.
pub(super) fn source(inv: &Inventory, id: Uuid) -> Result<Source, Refusal> {
    let it = animation_item(inv, id).ok_or(Refusal::Unavailable)?;
    match actions::content_missing(inv, it) {
        0 => Ok(Source {
            item: it.id,
            asset: it.asset_id,
            name: it.name.clone(),
        }),
        missing => Err(Refusal::Rights(missing)),
    }
}

/// The text of the rights `missing` (bits of `actions::FULL`), "copy, modify".
pub(super) fn rights_text(missing: u32) -> String {
    [
        (actions::COPY, "poser-inv-right-copy"),
        (actions::MODIFY, "poser-inv-right-modify"),
        (actions::TRANSFER, "poser-inv-right-transfer"),
    ]
    .into_iter()
    .filter(|(bit, _)| missing & bit != 0)
    .map(|(_, key)| aurora_i18n::text(key))
    .collect::<Vec<_>>()
    .join(", ")
}

/// A row of the picker.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Candidate {
    pub item: Uuid,
    pub name: String,
    pub refusal: Option<Refusal>,
}

/// The animation items of the inventory (a link counted once, as its
/// original) whose name holds `filter`, the allowed ones first, then by name.
pub(super) fn candidates(inv: &Inventory, filter: &str) -> Vec<Candidate> {
    let filter = filter.to_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Candidate> = inv
        .items
        .keys()
        .filter_map(|id| {
            let it = animation_item(inv, *id)?;
            if !seen.insert(it.id) || !it.name.to_lowercase().contains(&filter) {
                return None;
            }
            Some(Candidate {
                item: it.id,
                name: it.name.clone(),
                refusal: match actions::content_missing(inv, it) {
                    0 => None,
                    missing => Some(Refusal::Rights(missing)),
                },
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.refusal
            .is_some()
            .cmp(&b.refusal.is_some())
            .then_with(|| crate::natural_order::cmp(&a.name, &b.name))
            .then(a.item.cmp(&b.item))
    });
    out
}

/// What the animation holds that a pose cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Ignored {
    /// Joints of the animation that are not on this skeleton.
    pub unknown_joints: usize,
    /// It moves after its first frame.
    pub more_frames: bool,
}

impl Ignored {
    pub fn of(animation: &BoundAnim) -> Self {
        Self {
            unknown_joints: animation.joints.iter().filter(|j| j.is_none()).count(),
            more_frames: animation.anim.joints.iter().any(|j| j.rot_keys.len() > 1 || j.pos_keys.len() > 1),
        }
    }
    /// The notes shown under the window, after the loaded line.
    pub fn text(self) -> String {
        let mut notes = Vec::new();
        if self.more_frames {
            notes.push(aurora_i18n::text("poser-inv-note-frame").to_owned());
        }
        if self.unknown_joints > 0 {
            notes.push(aurora_i18n::tr!("poser-inv-note-joints", count = self.unknown_joints).to_string());
        }
        notes.push(aurora_i18n::text("poser-inv-note-extras").to_owned());
        notes.join(" ")
    }
}

/// The name a file load gives the field (storage.rs): no control character
/// and no `|`, at most 100 characters.
pub(super) fn field_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_control() && *c != '|').take(100).collect()
}

struct Request {
    source: Source,
    mode: LoadMode,
    subject: Uuid,
    since: std::time::Instant,
}

#[derive(Default)]
pub(super) struct InventoryLoad {
    /// The picker is shown.
    pub open: bool,
    filter: String,
    request: Option<Request>,
    cache: Option<(u64, String)>,
    candidates: Vec<Candidate>,
}

impl InventoryLoad {
    pub fn pending(&self) -> bool {
        self.request.is_some()
    }
}

impl Editor {
    /// Asks for the animation of the item `id` for the selected subject, in
    /// the compact window's load mode. The rule is asked again each frame
    /// until the asset arrives.
    pub(super) fn start_inventory_load(&mut self, inv: &Inventory, id: Uuid) -> Result<(), Refusal> {
        let source = source(inv, id)?;
        self.poser.storage.message = Some(aurora_i18n::tr!("poser-inv-loading", name = field_name(&source.name)).to_string());
        self.poser.inventory_load.request = Some(Request {
            source,
            mode: self.poser.load_mode,
            subject: self.poser.subject,
            since: std::time::Instant::now(),
        });
        self.poser.inventory_load.open = false;
        Ok(())
    }

    /// Fetches the animation of a pending load and hands it to the storage's
    /// loads when it arrives (`tick_animations`, which has the streamer).
    pub(super) fn tick_inventory_load(&mut self, streamer: &mut AnimStreamer, inv: &mut Inventory) {
        if self.poser.inventory_load.open
            && let Some(root) = actions::system(inv, 20)
        {
            actions::request_tree(inv, root);
        }
        let Some(request) = self.poser.inventory_load.request.take() else {
            return;
        };
        let source = match source(inv, request.source.item) {
            Ok(source) => source,
            Err(refusal) => {
                self.poser.storage.message = Some(refused_text(refusal));
                return;
            }
        };
        match streamer.get(&source.asset) {
            Some(animation) => {
                let note = Ignored::of(&animation).text();
                self.poser.storage.message = Some(format!(
                    "{} {note}",
                    aurora_i18n::tr!("poser-inv-loaded", name = field_name(&source.name))
                ));
                self.poser.storage.inject(
                    request.subject,
                    storage::Loaded::Animation(animation.anim.clone(), request.mode, field_name(&source.name)),
                );
            }
            None if streamer.asset_settled(&source.asset) || request.since.elapsed() > PATIENCE => {
                self.poser.storage.message = Some(aurora_i18n::tr!("poser-inv-failed", name = field_name(&source.name)).to_string());
            }
            None => self.poser.inventory_load.request = Some(Request { source, ..request }),
        }
    }

    /// The picker: a dialog over the poser, the same for the full and the
    /// compact window.
    pub(super) fn inventory_picker_ui(&mut self, ctx: &egui::Context, p: &Palette, inv: &Inventory) {
        if !self.poser.inventory_load.open {
            return;
        }
        let mut chosen = None;
        let mut cancel = false;
        let response = widgets::Modal::new(egui::Id::new("poser-inventory-load")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(egui::RichText::new(aurora_i18n::tr!("poser-inv-title")).size(17.0).color(p.ink));
            ui.add_space(6.0);
            let load = &mut self.poser.inventory_load;
            widgets::search_text_edit(ui, p, &mut load.filter, |edit| {
                edit.hint_text(aurora_i18n::tr!("ik-edit-animation-search"))
                    .desired_width(f32::INFINITY)
            });
            if load.cache.as_ref() != Some(&(inv.generation, load.filter.clone())) {
                load.candidates = candidates(inv, &load.filter);
                load.cache = Some((inv.generation, load.filter.clone()));
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (mode, key) in [
                    (LoadMode::All, "poser-load-all"),
                    (LoadMode::Rotations, "poser-load-rotations"),
                    (LoadMode::Selective, "poser-load-selective"),
                    (LoadMode::SelectiveRotations, "poser-load-selective-rotations"),
                ] {
                    widgets::radio_value(ui, p, &mut self.poser.load_mode, mode, aurora_i18n::text(key));
                }
            });
            ui.add_space(4.0);
            let load = &self.poser.inventory_load;
            if load.candidates.is_empty() {
                ui.label(egui::RichText::new(aurora_i18n::text("poser-inv-empty")).size(12.0).color(p.muted));
            }
            egui::ScrollArea::vertical()
                .id_salt("poser-inventory-load-list")
                .max_height(240.0)
                .auto_shrink([false, true])
                .show_rows(
                    ui,
                    ui.text_style_height(&egui::TextStyle::Body) + 6.0,
                    load.candidates.len(),
                    |ui, range| {
                        for candidate in &load.candidates[range] {
                            let allowed = candidate.refusal.is_none();
                            let row = ui.add_enabled_ui(allowed, |ui| ui.selectable_label(false, &candidate.name)).inner;
                            match candidate.refusal {
                                None if row.clicked() => chosen = Some(candidate.item),
                                Some(refusal) => {
                                    row.on_disabled_hover_text(refused_text(refusal));
                                }
                                None => {}
                            }
                        }
                    },
                );
            ui.add_space(4.0);
            ui.label(egui::RichText::new(aurora_i18n::text("poser-inv-hint")).size(11.5).color(p.muted));
            ui.add_space(6.0);
            if ui.button(aurora_i18n::tr!("common-cancel")).clicked() {
                cancel = true;
            }
        });
        if let Some(id) = chosen {
            if let Err(refusal) = self.start_inventory_load(inv, id) {
                self.poser.storage.message = Some(refused_text(refusal));
                self.poser.inventory_load.open = false;
            }
        } else if cancel || response.should_close() {
            self.poser.inventory_load.open = false;
        }
    }
}

/// The tooltip and message for a refused item. Never the asset id.
fn refused_text(refusal: Refusal) -> String {
    match refusal {
        Refusal::Rights(missing) => aurora_i18n::tr!("poser-inv-refused", rights = rights_text(missing)).to_string(),
        Refusal::Unavailable => aurora_i18n::text("poser-inv-unavailable").to_owned(),
    }
}
