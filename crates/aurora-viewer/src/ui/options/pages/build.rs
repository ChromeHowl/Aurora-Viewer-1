//! Preferences › Build: what a prim made with the Create tool starts as,
//! after the Legacy Viewer's Build preferences (panel_preferences_firestorm.xml,
//! the "Build 1" tab: size, phantom, physical, temporary, material, color,
//! alpha, glow, full bright, shininess). The look and flags are given when
//! the prim arrives (`build::new_defaults`). Editing holds how the build
//! tools behave on a selection (the Texture tab's automatic choice).

use super::super::cx::{Cx, field};

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
}
