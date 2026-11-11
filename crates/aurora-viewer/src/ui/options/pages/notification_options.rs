//! Preferences › Chat › Notifications: the script dialogs, the notices and
//! the Legacy toasts' timing (`settings/notification_options.rs`), after the
//! Legacy Viewer's User Interface › Script dialogs and Toasts, Chat ›
//! Notifications and Notifications › Alerts panels.

use super::super::cx::{Cx, field};
use crate::settings::notification_options::{
    DIALOG_ROWS, DialogLimit, DialogPosition, GAP, INFO_SECONDS, OFFER_SECONDS, THROTTLE_COUNT, TIP_SECONDS,
};

pub fn groups(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("b5c-script-dialogs"), |cx| {
        let limits: Vec<(DialogLimit, &str)> = DialogLimit::ALL.iter().map(|l| (*l, l.label())).collect();
        cx.combo(
            tr!("b5c-dialog-limit"),
            tr!("b5c-dialog-limit-hint"),
            &field!(notification_options.script_dialog_limit),
            &limits,
        );
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
    });
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
        cx.toggle(
            tr!("b5c-toasts-in-front"),
            tr!("b5c-toasts-in-front-hint"),
            &field!(notification_options.toasts_in_front),
        );
    });
}
