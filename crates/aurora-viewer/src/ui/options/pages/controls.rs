//! Preferences › Mouse and keyboard: the Legacy Viewer's "Move & View ›
//! Movement" (panel_preferences_move.xml) and what the keys move.

use super::super::Page;
use super::super::cx::{Cx, Nothing, field};
use super::super::toggle;

pub fn page(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("ui-options-mouse"), |cx| {
        cx.slider(tr!("ui-options-sensitivity"), "", &field!(mouse_sensitivity), 0.1..=5.0, "×");
        cx.toggle(tr!("ui-options-invert-vertical-axis"), "", &field!(invert_mouse));
        cx.toggle(
            tr!("ui-options-world-mouse-walk"),
            tr!("ui-options-world-mouse-walk-help"),
            &field!(world_mouse_walk),
        );
        // "Double-click on land" (double_click_action_combo)
        let choices: Vec<(crate::autopilot::DoubleClick, &str)> =
            crate::autopilot::DoubleClick::ALL.iter().map(|a| (*a, a.label())).collect();
        cx.combo(
            tr!("ui-options-double-click-ground"),
            tr!("ui-options-double-click-ground-help"),
            &field!(double_click),
            &choices,
        );
        cx.toggle(
            tr!("ui-options-double-click-scripted"),
            tr!("ui-options-double-click-scripted-help"),
            &field!(double_click_scripted),
        );
        // Legacy Viewer › Protection (FSBlockClickSit)
        cx.toggle(
            tr!("ui-options-block-click-sit"),
            tr!("ui-options-block-click-sit-help"),
            &field!(block_click_sit),
        );
    });
    cx.group(tr!("ui-options-keyboard"), |cx| {
        cx.toggle(
            tr!("ui-options-arrows-strafe"),
            tr!("ui-options-left-right-strafe-instead-of-turning-shift-reverses-this"),
            &field!(arrows_strafe),
        );
        cx.toggle(
            tr!("ui-options-arrow-keys-always-move"),
            tr!("ui-options-arrow-keys-always-move-help"),
            &field!(arrow_keys_always_move),
        );
        cx.toggle(
            tr!("ui-options-letter-keys-move"),
            tr!("ui-options-letter-keys-move-help"),
            &field!(letter_keys_move),
        );
        // FSDisableTurningAroundWhenWalkingBackwards
        cx.toggle(
            tr!("ui-options-walk-backwards-without-turning"),
            tr!("ui-options-walk-backwards-without-turning-help"),
            &field!(walk_backwards_without_turning),
        );
        // AutomaticFly (agent_jump / LLAgent::propagate), split into two
        // independent choices at the human's request.
        cx.toggle(
            tr!("ui-options-automatic-fly"),
            tr!("ui-options-automatic-fly-help"),
            &field!(automatic_fly),
        );
        cx.toggle(
            tr!("ui-options-automatic-land"),
            tr!("ui-options-automatic-land-help"),
            &field!(automatic_land),
        );
        // FSCrouchToggle, FSIgnoreFinishAnimation, FSFlyAfterTeleport
        cx.toggle(
            tr!("ui-options-crouch-toggle"),
            tr!("ui-options-crouch-toggle-help"),
            &field!(crouch_toggle),
        );
        cx.toggle(tr!("ui-options-quickjump"), tr!("ui-options-quickjump-help"), &field!(quickjump));
        cx.toggle(
            tr!("ui-options-fly-after-teleport"),
            tr!("ui-options-fly-after-teleport-help"),
            &field!(fly_after_teleport),
        );
        // FSAlwaysShowTPCancel (panel_preferences_move.xml)
        cx.toggle(
            tr!("ui-options-always-show-teleport-cancel"),
            tr!("ui-options-always-show-teleport-cancel-help"),
            &field!(always_show_tp_cancel),
        );
        friends_only_row(cx);
        friends_only_keep_row(cx);
        cx.link(
            tr!("ui-options-shortcuts"),
            tr!("ui-options-keys-and-mouse-buttons-for-each-action"),
            tr!("ui-options-edit-shortcuts"),
            Page::Shortcuts,
            0,
        );
    });
    teleport_group(cx);
}

/// Teleports, the teleport history and the world map
/// (settings/teleport.rs): the Legacy Viewer's rows of Move & View ›
/// Teleports and Map, Legacy Viewer › Login / Teleport, Sound and User
/// interface.
fn teleport_group(cx: &mut Cx) {
    use crate::settings::teleport::HistoryZone;
    use aurora_i18n::tr;
    cx.group(tr!("tpmap-group"), |cx| {
        cx.toggle(
            tr!("tpmap-opt-keep-map-open"),
            tr!("tpmap-opt-keep-map-open-help"),
            &field!(teleport.keep_map_open),
        );
        cx.toggle(
            tr!("tpmap-opt-clear-beacon"),
            tr!("tpmap-opt-clear-beacon-help"),
            &field!(teleport.clear_beacon),
        );
        cx.toggle(
            tr!("tpmap-opt-back-slurl"),
            tr!("tpmap-opt-back-slurl-help"),
            &field!(teleport.back_slurl),
        );
        cx.toggle(
            tr!("tpmap-opt-no-teleport-screen"),
            tr!("tpmap-opt-no-teleport-screen-help"),
            &field!(teleport.no_teleport_screen),
        );
        cx.toggle(
            tr!("tpmap-opt-no-login-screen"),
            tr!("tpmap-opt-no-login-screen-help"),
            &field!(teleport.no_login_screen),
        );
        // FSDisableLogoutScreens (panel_preferences_firestorm.xml)
        cx.toggle(
            tr!("l3-opt-no-logout-screen"),
            tr!("l3-opt-no-logout-screen-help"),
            &field!(teleport_extras.no_logout_screen),
        );
        cx.toggle(
            tr!("tpmap-opt-unmute-sounds"),
            tr!("tpmap-opt-unmute-sounds-help"),
            &field!(teleport.unmute_sounds),
        );
        cx.toggle(
            tr!("tpmap-opt-unmute-ambient"),
            tr!("tpmap-opt-unmute-ambient-help"),
            &field!(teleport.unmute_ambient),
        );
        cx.toggle(
            tr!("tpmap-opt-history-position"),
            tr!("tpmap-opt-history-position-help"),
            &field!(teleport.history.position),
        );
        cx.toggle(
            tr!("tpmap-opt-history-date"),
            tr!("tpmap-opt-history-date-help"),
            &field!(teleport.history.date),
        );
        let zones: Vec<(HistoryZone, &str)> = HistoryZone::ALL.iter().map(|z| (*z, z.label())).collect();
        cx.combo(
            tr!("tpmap-opt-history-zone"),
            tr!("tpmap-opt-history-zone-help"),
            &field!(teleport.history.zone),
            &zones,
        );
        cx.toggle(
            tr!("tpmap-opt-map-region-info"),
            tr!("tpmap-opt-map-region-info-help"),
            &field!(teleport.map_region_info),
        );
        cx.toggle(
            tr!("tpmap-opt-map-unique-landmarks"),
            tr!("tpmap-opt-map-unique-landmarks-help"),
            &field!(teleport.map_unique_landmarks),
        );
    });
}

/// "Show Friends Only" itself (FSRenderFriendsOnly), the switch of the
/// toolbar button and the World menu: an Aurora addition, the Legacy Viewer
/// has only the next row in its Preferences. Kept per account, so it waits
/// for a login.
fn friends_only_row(cx: &mut Cx) {
    use aurora_i18n::tr;
    let p = cx.p;
    let state = cx.st.friends_only;
    let mut on = state.unwrap_or(false);
    let hint = if state.is_some() {
        tr!("ui-options-friends-only-show-help")
    } else {
        tr!("ui-options-friends-only-keep-login")
    };
    cx.row_with(tr!("toolbar-command-friends-only"), hint, &Nothing, |ui, _, _, r| {
        if ui.add_enabled_ui(state.is_some(), |ui| toggle(ui, &p, &mut on)).inner {
            r.friends_only = Some(on);
        }
        false
    });
}

/// "Keep 'Show Friends Only' enabled after teleporting" (FSRenderFriendsOnlyPersistsTP of
/// panel_preferences_move.xml): kept per account, so it waits for a login,
/// as in the Legacy Viewer.
fn friends_only_keep_row(cx: &mut Cx) {
    use aurora_i18n::tr;
    let p = cx.p;
    let state = cx.st.friends_only_keep;
    let mut kept = state.unwrap_or(false);
    let hint = if state.is_some() {
        tr!("ui-options-friends-only-keep-help")
    } else {
        tr!("ui-options-friends-only-keep-login")
    };
    cx.row_with(tr!("ui-options-friends-only-keep"), hint, &Nothing, |ui, _, _, r| {
        if ui.add_enabled_ui(state.is_some(), |ui| toggle(ui, &p, &mut kept)).inner {
            r.friends_only_keep = Some(kept);
        }
        false
    });
}
