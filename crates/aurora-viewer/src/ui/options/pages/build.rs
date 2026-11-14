//! Preferences › Build: what a prim made with the Create tool starts as,
//! after the Legacy Viewer's Build preferences (panel_preferences_firestorm.xml,
//! the "Build 1" tab: size, phantom, physical, temporary, material, color,
//! alpha, glow, full bright, shininess). The look and flags are given when
//! the prim arrives (`build::new_defaults`). Editing holds how the build
//! tools behave on a selection (the Texture tab's automatic choice).

use super::super::cx::{Cx, field};
use crate::settings::new_objects::Label;
use crate::theme::Palette;
use crate::ui::widgets;

pub fn page(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("ui-options-build-new-objects"), |cx| {
        cx.note(tr!("ui-options-build-new-objects-note"));
        // FSBuildPrefs_X/Y/Zsize: 0.01 to 10 m, as the Legacy spinners
        cx.keywords("FSBuildPrefs_Xsize FSBuildPrefs_Ysize FSBuildPrefs_Zsize").row(
            tr!("common-size"),
            tr!("ui-options-build-size-hint"),
            &field!(build.new_prim_size),
            |ui, s| {
                let mut changed = false;
                for v in &mut s.build.new_prim_size {
                    changed |= ui
                        .add(
                            egui::DragValue::new(v)
                                .range(0.01..=10.0)
                                .speed(0.05)
                                .max_decimals(3)
                                .custom_parser(aurora_i18n::parse_number),
                        )
                        .changed();
                }
                changed
            },
        );
        let materials = [
            (0u8, tr!("build-ui-features-stone")),
            (1, tr!("build-ui-features-metal")),
            (2, tr!("build-ui-features-glass")),
            (3, tr!("build-ui-features-wood")),
            (4, tr!("ui-options-build-flesh")),
            (5, tr!("build-ui-features-plastic")),
            (6, tr!("build-ui-features-rubber")),
        ];
        cx.keywords("FSBuildPrefs_Material")
            .combo(tr!("ui-options-build-material"), "", &field!(build.new_prim_material), &materials);
        cx.keywords("FSBuildPrefs_Physical")
            .toggle(tr!("ui-options-build-physical"), "", &field!(build.new_prim_physical));
        cx.keywords("FSBuildPrefs_Temporary")
            .toggle(tr!("ui-options-build-temporary"), "", &field!(build.new_prim_temporary));
        cx.keywords("FSBuildPrefs_Phantom")
            .toggle(tr!("build-ui-object-phantom"), "", &field!(build.new_prim_phantom));
    });
    cx.group(tr!("ui-options-build-new-faces"), |cx| {
        cx.keywords("FSBuildPrefs_Color")
            .color(tr!("common-color"), "", &field!(build.new_prim_color));
        cx.keywords("FSBuildPrefs_Alpha").slider_whole(
            tr!("build-ui-texture-transparency"),
            "",
            &field!(build.new_prim_transparency),
            0.0..=100.0,
            " %",
        );
        cx.keywords("FSBuildPrefs_Glow")
            .slider(tr!("build-ui-texture-glow"), "", &field!(build.new_prim_glow), 0.0..=1.0, "");
        cx.keywords("FSBuildPrefs_FullBright")
            .toggle(tr!("build-ui-texture-fullbright"), "", &field!(build.new_prim_fullbright));
        let shines = [
            (0u8, tr!("common-none-2")),
            (1, tr!("build-ui-texture-low")),
            (2, tr!("common-medium")),
            (3, tr!("build-ui-texture-high")),
        ];
        cx.keywords("FSBuildPrefs_Shiny")
            .combo(tr!("build-ui-texture-shininess"), "", &field!(build.new_prim_shiny), &shines);
    });
    cx.group(tr!("ui-options-build-editing"), |cx| {
        // LLPanelFace::updateUI's tab choice (build/material_preview.rs)
        cx.keywords("PBR Blinn-Phong texture material").toggle(
            tr!("ui-options-build-auto-material-tab"),
            tr!("ui-options-build-auto-material-tab-hint"),
            &field!(build.auto_material_tab),
        );
        // ShowSelectionBeam (on); EffectColor of Preferences › Colors
        cx.keywords("ShowSelectionBeam beam").toggle(
            tr!("ui-options-build-show-beams"),
            tr!("ui-options-build-show-beams-hint"),
            &field!(beams.show),
        );
        cx.keywords("EffectColor beam").color(
            tr!("ui-options-build-beam-color"),
            tr!("ui-options-build-beam-color-hint"),
            &field!(beams.color),
        );
    });
    // the Legacy Viewer's Build 1 and Build 2 tabs and Build menu options
    // (settings/build_tools.rs)
    cx.group(tr!("b7-options-group"), |cx| {
        cx.keywords("FSUnlinkConfirmEnabled")
            .toggle(tr!("b7-options-unlink-confirm"), "", &field!(build.tools.unlink_confirm));
        cx.keywords("MinObjectsForUnlinkConfirm").slider(
            tr!("b7-options-unlink-min"),
            tr!("b7-options-unlink-min-hint"),
            &field!(build.tools.unlink_confirm_min),
            0..=256,
            "",
        );
        cx.keywords("FSBuildPrefs_PivotX FSBuildPrefs_PivotY FSBuildPrefs_PivotZ pivot")
            .row(
                tr!("b7-options-pivot"),
                tr!("b7-options-pivot-hint"),
                &field!(build.tools.pivot),
                |ui, s| {
                    let mut changed = false;
                    let range = crate::settings::build_tools::PIVOT_RANGE;
                    for v in &mut s.build.tools.pivot {
                        changed |= ui
                            .add(
                                egui::DragValue::new(v)
                                    .range(range.clone())
                                    .speed(0.1)
                                    .max_decimals(3)
                                    .custom_parser(aurora_i18n::parse_number),
                            )
                            .changed();
                    }
                    changed
                },
            );
        cx.keywords("FSBuildPrefs_PivotIsPercent")
            .toggle(tr!("b7-options-pivot-percent"), "", &field!(build.tools.pivot_percent));
        cx.keywords("FSSelectLockedOnly")
            .toggle(tr!("b7-select-locked"), "", &field!(build.tools.select_locked));
        cx.keywords("FSSelectCopyableOnly")
            .toggle(tr!("b7-select-copyable"), "", &field!(build.tools.select_copyable));
        cx.keywords("FSSelectIncludeGroupOwned").toggle(
            tr!("b7-select-group-owned"),
            tr!("b7-select-group-owned-hint"),
            &field!(build.tools.select_group_owned),
        );
        cx.keywords("RenderLightRadius")
            .toggle(tr!("b7-light-radius"), "", &field!(build.tools.light_radius));
        cx.keywords("HideSelectedObjects").toggle(
            tr!("b7-hide-selected"),
            tr!("b7-hide-selected-hint"),
            &field!(build.tools.hide_selected),
        );
    });
    // FSDefaultObjectTexture, FSBuildPrefs_EmbedItem / _Item and
    // FSBuildPrefs_UseCustomScript / _CustomScriptItem: dropped from the
    // inventory, as on the Legacy Viewer's drop targets (settings/new_objects.rs)
    let p = cx.p;
    cx.group(tr!("l5-new-group"), |cx| {
        cx.note(tr!("l5-new-note"));
        cx.keywords("FSDefaultObjectTexture texture").row_with(
            tr!("l5-new-texture"),
            tr!("l5-new-texture-hint"),
            &field!(build.new_objects.texture),
            |ui, s, st, _| {
                let Some(view) = st.new_objects.as_ref() else {
                    ui.label(tr!("l5-new-logged-out"));
                    return false;
                };
                let carried = view.carried.filter(|c| c.texture).map(|c| c.asset);
                match drop_slot(ui, &p, &view.texture, view.carried.map(|c| c.texture), true) {
                    Slot::Dropped => carried.is_some_and(|asset| s.build.new_objects.texture.replace(asset) != Some(asset)),
                    Slot::Cleared => s.build.new_objects.texture.take().is_some(),
                    Slot::Idle => false,
                }
            },
        );
        cx.keywords("FSBuildPrefs_EmbedItem FSBuildPrefs_Item embed contents").row_with(
            tr!("l5-new-embed"),
            tr!("l5-new-embed-hint"),
            &field!(build.new_objects.accounts),
            |ui, s, st, _| {
                let Some(view) = st.new_objects.as_ref() else {
                    ui.label(tr!("l5-new-logged-out"));
                    return false;
                };
                let mut a = s.build.new_objects.account(view.agent);
                let before = a;
                widgets::switch(ui, &p, &mut a.embed);
                match drop_slot(ui, &p, &view.embed, view.carried.map(|c| c.embed), a.embed_item.is_some()) {
                    Slot::Dropped => a.embed_item = view.carried.filter(|c| c.embed).map(|c| c.item).or(a.embed_item),
                    Slot::Cleared => a.embed_item = None,
                    Slot::Idle => {}
                }
                s.build.new_objects.set_account(view.agent, a);
                a != before
            },
        );
        cx.keywords("FSBuildPrefs_UseCustomScript FSBuildPrefs_CustomScriptItem script template")
            .row_with(
                tr!("l5-new-script"),
                tr!("l5-new-script-hint"),
                &field!(build.new_objects.accounts),
                |ui, s, st, _| {
                    let Some(view) = st.new_objects.as_ref() else {
                        ui.label(tr!("l5-new-logged-out"));
                        return false;
                    };
                    let mut a = s.build.new_objects.account(view.agent);
                    let before = a;
                    widgets::switch(ui, &p, &mut a.custom_script);
                    match drop_slot(ui, &p, &view.script, view.carried.map(|c| c.script), a.script_item.is_some()) {
                        Slot::Dropped => a.script_item = view.carried.filter(|c| c.script).map(|c| c.item).or(a.script_item),
                        Slot::Cleared => a.script_item = None,
                        Slot::Idle => {}
                    }
                    s.build.new_objects.set_account(view.agent, a);
                    a != before
                },
            );
    });
    // the smaller Legacy build options (settings/build_extras.rs)
    cx.group(tr!("l5-extras-group"), |cx| {
        cx.keywords("FSExperimentalDragTexture drag texture offset").toggle(
            tr!("l5-drag-texture"),
            tr!("l5-drag-texture-hint"),
            &field!(build.extras.drag_texture),
        );
        cx.keywords("FSCommitForSaleOnChange sale price").toggle(
            tr!("l5-sale-on-change"),
            tr!("l5-sale-on-change-hint"),
            &field!(build.extras.commit_sale_on_change),
        );
        cx.keywords("FSRenderParcelSelectionToMaxBuildHeight parcel boundary").toggle(
            tr!("l5-walls-to-max"),
            tr!("l5-walls-to-max-hint"),
            &field!(build.extras.parcel_walls_to_max),
        );
        cx.keywords("FSSaveInventoryScriptsAsMono mono lsl").toggle(
            tr!("l5-inventory-mono"),
            tr!("l5-inventory-mono-hint"),
            &field!(build.extras.inventory_scripts_mono),
        );
    });
}

enum Slot {
    Idle,
    Dropped,
    Cleared,
}

/// A drop target naming the choice, with a button that clears it. While an
/// inventory entry is dragged over it, its outline says whether the entry
/// is taken (`accepted`); released there, an accepted entry is `Dropped`.
/// The drop never reaches the world behind the window.
fn drop_slot(ui: &mut egui::Ui, p: &Palette, label: &Label, accepted: Option<bool>, clearable: bool) -> Slot {
    use aurora_i18n::tr;
    let text = match label {
        Label::NotSet => tr!("l5-new-drop-here").to_owned(),
        Label::Missing => tr!("l5-new-missing").to_owned(),
        Label::Name(name) => name.clone(),
    };
    let mut out = Slot::Idle;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - ui.spacing().interact_size.y - ui.spacing().item_spacing.x).max(80.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ui.spacing().interact_size.y), egui::Sense::hover());
        let visuals = ui.visuals().widgets.inactive;
        ui.painter().rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        let colour = if matches!(label, Label::Name(_)) { p.ink } else { p.muted };
        ui.painter().with_clip_rect(rect).text(
            rect.left_center() + egui::vec2(ui.spacing().button_padding.x, 0.0),
            egui::Align2::LEFT_CENTER,
            text,
            egui::TextStyle::Body.resolve(ui.style()),
            colour,
        );
        if let Some(accepted) = accepted
            && ui.rect_contains_pointer(rect)
        {
            crate::ui::inventory::drag::feedback(
                ui.ctx(),
                if accepted {
                    egui::CursorIcon::Copy
                } else {
                    egui::CursorIcon::NotAllowed
                },
            );
            ui.painter().rect_stroke(
                rect,
                visuals.corner_radius,
                egui::Stroke::new(2.0, if accepted { p.violet_light } else { p.muted_dim }),
                egui::StrokeKind::Inside,
            );
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                egui::DragAndDrop::clear_payload(ui.ctx());
                if accepted {
                    out = Slot::Dropped;
                }
            }
        }
        if widgets::icon_button(ui, p, "x", tr!("l5-new-clear"), clearable) {
            out = Slot::Cleared;
        }
    });
    out
}
