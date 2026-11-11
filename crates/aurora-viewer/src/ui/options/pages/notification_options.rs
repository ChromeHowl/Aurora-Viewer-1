//! Preferences › Chat › Notifications: the script dialogs, the notices and
//! the Legacy toasts' timing (`settings/notification_options.rs`), after the
//! Legacy Viewer's User Interface › Script dialogs and Toasts, Chat ›
//! Notifications and Notifications › Alerts panels.

use super::super::cx::{Cx, field};
use crate::settings::notification_options::{
    BOTTOM_MARGIN, DIALOG_ROWS, DialogPosition, FADE_SECONDS, GAP, INFO_SECONDS, OFFER_SECONDS, OVERFLOW_HEIGHT,
    THROTTLE_COUNT, TIP_SECONDS,
};

/// The script dialogs' rows after "Script dialogs per object", in the
/// Script dialogs group of Preferences › Chat › Notifications.
pub fn script_dialog_rows(cx: &mut Cx) {
    use aurora_i18n::tr;
    let places: Vec<(DialogPosition, &str)> = DialogPosition::ALL.iter().map(|at| (*at, at.label())).collect();
    cx.combo(
        tr!("b5c-dialog-position"),
        tr!("b5c-dialog-position-hint"),
        &field!(notification_options.script_dialog_position),
        &places,
    );
    cx.slider(
        tr!("b5c-dialog-rows"),
        tr!("b5c-dialog-rows-hint"),
        &field!(notification_options.script_dialog_rows),
        DIALOG_ROWS,
        "",
    );
    // FSAnimatedScriptDialogs: only in a top corner (LLFloaterPreference
    // enables it for those positions alone)
    let top =
        cx.s.notification_options
            .script_dialog_position
            .corner()
            .is_some_and(|c| c.y() == egui::Align::Min);
    cx.greyed(!top);
    cx.toggle(
        tr!("l4-animated-dialogs"),
        tr!("l4-animated-dialogs-hint"),
        &field!(notification_options.animated_script_dialogs),
    );
    let bottom =
        cx.s.notification_options
            .script_dialog_position
            .corner()
            .is_some_and(|c| c.y() == egui::Align::Max);
    cx.greyed(!bottom);
    cx.slider(
        tr!("l4-bottom-margin"),
        tr!("l4-bottom-margin-hint"),
        &field!(notification_options.toast_bottom_margin),
        BOTTOM_MARGIN,
        " px",
    );
    cx.greyed(false);
    cx.toggle(
        tr!("b5c-hide-block"),
        tr!("b5c-hide-block-hint"),
        &field!(notification_options.hide_script_block_button),
    );
    cx.toggle(
        tr!("b5c-debit-deny"),
        tr!("b5c-debit-deny-hint"),
        &field!(notification_options.debit_default_deny),
    );
}

pub fn groups(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("b5c-notices"), |cx| {
        cx.toggle(
            tr!("b5c-flash-friend-status"),
            tr!("b5c-flash-friend-status-hint"),
            &field!(notification_options.flash_on_friend_status),
        );
        cx.toggle(
            tr!("b5c-report-muted-group"),
            tr!("b5c-report-muted-group-hint"),
            &field!(notification_options.report_muted_group_chat),
        );
        cx.toggle(
            tr!("b5c-open-on-offline"),
            tr!("b5c-open-on-offline-hint"),
            &field!(notification_options.open_conversations_on_offline_message),
        );
        cx.toggle(
            tr!("b5c-payments-in-chat"),
            tr!("b5c-payments-in-chat-hint"),
            &field!(notification_options.payments_in_chat),
        );
        cx.toggle(
            tr!("b5c-grid-status-login"),
            tr!("b5c-grid-status-login-hint"),
            &field!(notification_options.grid_status_at_login),
        );
        cx.toggle(
            tr!("b5c-landmark-notice"),
            tr!("b5c-landmark-notice-hint"),
            &field!(notification_options.landmark_created_notice),
        );
        cx.toggle(
            tr!("b5c-hide-returned"),
            tr!("b5c-hide-returned-hint"),
            &field!(notification_options.hide_object_returned),
        );
        cx.slider(
            tr!("b5c-throttle-count"),
            tr!("b5c-throttle-count-hint"),
            &field!(notification_options.offer_throttle_count),
            THROTTLE_COUNT,
            "",
        );
        cx.subs(false, |cx| {
            cx.toggle(tr!("b5c-spam-from"), "", &field!(notification_options.notify_offer_spam_from));
            cx.toggle(tr!("b5c-spam"), "", &field!(notification_options.notify_offer_spam));
        });
    });
    // the Legacy toasts only: Aurora's center has its own times and place
    let center = cx.s.notifications_style == crate::settings::NotificationsStyle::Aurora;
    cx.group(tr!("b5c-legacy-toasts"), |cx| {
        if center {
            cx.note(tr!("b5c-legacy-toasts-note"));
        }
        cx.greyed(center);
        cx.slider(
            tr!("b5c-info-seconds"),
            tr!("b5c-info-seconds-hint"),
            &field!(notification_options.info_toast_seconds),
            INFO_SECONDS,
            " s",
        );
        cx.slider(
            tr!("b5c-offer-seconds"),
            tr!("b5c-offer-seconds-hint"),
            &field!(notification_options.offer_toast_seconds),
            OFFER_SECONDS,
            " s",
        );
        cx.slider(
            tr!("b5c-tip-seconds"),
            tr!("b5c-tip-seconds-hint"),
            &field!(notification_options.tip_toast_seconds),
            TIP_SECONDS,
            " s",
        );
        cx.slider(tr!("b5c-toast-gap"), "", &field!(notification_options.toast_gap), GAP, " px");
        cx.slider(
            tr!("l4-toast-fade"),
            tr!("l4-toast-fade-hint"),
            &field!(notification_options.toast_fade_seconds),
            FADE_SECONDS,
            " s",
        );
        cx.slider(
            tr!("l4-overflow-height"),
            tr!("l4-overflow-height-hint"),
            &field!(notification_options.overflow_toast_height),
            OVERFLOW_HEIGHT,
            " px",
        );
        cx.toggle(
            tr!("b5c-toasts-in-front"),
            tr!("b5c-toasts-in-front-hint"),
            &field!(notification_options.toasts_in_front),
        );
    });
}
