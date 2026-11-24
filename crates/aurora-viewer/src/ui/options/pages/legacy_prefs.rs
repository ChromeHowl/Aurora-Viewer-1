//! The rows of `settings/legacy_prefs.rs`, each on the page where the
//! Legacy Viewer has it: the tooltip delay, the HUDs' size and the
//! simulator version (Layout; panel_preferences_UI.xml) and scripts opening
//! the map (Privacy).

use super::super::cx::{Cx, field};
use crate::settings::legacy_prefs::HUD_SCALE;

/// Layout › Bars and windows.
pub fn interface(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("b9-interface-group"), |cx| {
        cx.slider(
            tr!("b9-tooltip-delay"),
            tr!("b9-tooltip-delay-hint"),
            &field!(legacy_prefs.tooltip_delay),
            0.0..=5.0,
            " s",
        );
        cx.slider(
            tr!("b9-hud-scale"),
            tr!("b9-hud-scale-hint"),
            &field!(legacy_prefs.hud_scale),
            HUD_SCALE,
            " ×",
        );
        cx.toggle(
            tr!("b9-sim-version"),
            tr!("b9-sim-version-hint"),
            &field!(legacy_prefs.sim_version_in_bar),
        );
    });
}

/// Privacy › Scripts and content: llMapDestination may open the map.
pub fn scripts(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.toggle(
        tr!("b9-scripts-show-ui"),
        tr!("b9-scripts-show-ui-hint"),
        &field!(legacy_prefs.scripts_can_show_ui),
    );
}
