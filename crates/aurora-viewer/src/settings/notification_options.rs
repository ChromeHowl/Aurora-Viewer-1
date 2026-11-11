//! Notification and dialog options of Preferences › Chat › Notifications:
//! the script dialogs (ScriptDialogLimitations, ScriptDialogsPosition,
//! FSRowsPerScriptDialog, FSRemoveScriptBlockButton), the conversations
//! (FSIMChatFlashOnFriendStatusChange, FSReportMutedGroupChat,
//! FSOpenIMContainerOnOfflineMessage), the Legacy toasts' timing, gap and
//! order (NotificationToastLifeTime, NotificationTipToastLifeTime,
//! ToastGap, FSShowToastsInFront), the small notices
//! (FSLandmarkCreatedNotification, FSDisableReturnObjectNotification), the
//! offer throttle (FSOfferThrottleMaxCount, FSNotifyIncomingObjectSpam,
//! FSNotifyIncomingObjectSpamFrom), FSPaymentInfoInChat,
//! AutoQueryGridStatus and FSPermissionDebitDefaultDeny.
//!
//! Defaults are the Legacy Viewer's, except where Aurora already behaved
//! otherwise before these options existed (the toasts' times and gap, the
//! toasts in front of the windows, script dialogs with the other
//! notifications): those keep Aurora's look.

use serde::{Deserialize, Serialize};

/// How many script dialogs an object may keep open at once
/// (ScriptDialogLimitations, LLScriptFloaterManager::onAddNotification):
/// a new dialog closes the older one it matches.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum DialogLimit {
    /// One per object (SCRIPT_PER_OBJECT, 0).
    PerObject,
    /// One per object and channel (SCRIPT_PER_CHANNEL, 1, the default).
    #[default]
    PerChannel,
    /// One per channel for attachments, one per object for the rest (2).
    PerChannelAttachments,
    /// One per channel for HUDs, one per object for the rest (3).
    PerChannelHuds,
    /// No limit for HUDs, one per object for the rest (4).
    UnconstrainedHuds,
    /// No limit at all (5).
    Unconstrained,
}

impl DialogLimit {
    pub const ALL: [DialogLimit; 6] = [
        DialogLimit::PerObject,
        DialogLimit::PerChannel,
        DialogLimit::PerChannelAttachments,
        DialogLimit::PerChannelHuds,
        DialogLimit::UnconstrainedHuds,
        DialogLimit::Unconstrained,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::PerObject => aurora_i18n::tr!("b5c-limit-per-object"),
            Self::PerChannel => aurora_i18n::tr!("b5c-limit-per-channel"),
            Self::PerChannelAttachments => aurora_i18n::tr!("b5c-limit-per-channel-attachments"),
            Self::PerChannelHuds => aurora_i18n::tr!("b5c-limit-per-channel-huds"),
            Self::UnconstrainedHuds => aurora_i18n::tr!("b5c-limit-unconstrained-huds"),
            Self::Unconstrained => aurora_i18n::tr!("b5c-limit-unconstrained"),
        }
    }
}

/// Where script dialogs stand (ScriptDialogsPosition). Aurora's default
/// keeps them with the other notifications (the Legacy toasts' stack at
/// the top right, or under Aurora's bell), the place of the Legacy
/// Viewer's default (top right); the corners take them apart.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum DialogPosition {
    #[default]
    WithNotifications,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl DialogPosition {
    pub const ALL: [DialogPosition; 5] = [
        DialogPosition::WithNotifications,
        DialogPosition::TopLeft,
        DialogPosition::TopRight,
        DialogPosition::BottomLeft,
        DialogPosition::BottomRight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::WithNotifications => aurora_i18n::tr!("b5c-position-with-notifications"),
            Self::TopLeft => aurora_i18n::tr!("ui-options-position-top-left"),
            Self::TopRight => aurora_i18n::tr!("ui-options-position-top-right"),
            Self::BottomLeft => aurora_i18n::tr!("ui-options-position-bottom-left"),
            Self::BottomRight => aurora_i18n::tr!("ui-options-position-bottom-right"),
        }
    }

    /// The corner of the screen's free room the dialogs stand in (`None`:
    /// with the notifications).
    pub fn corner(self) -> Option<egui::Align2> {
        match self {
            Self::WithNotifications => None,
            Self::TopLeft => Some(egui::Align2::LEFT_TOP),
            Self::TopRight => Some(egui::Align2::RIGHT_TOP),
            Self::BottomLeft => Some(egui::Align2::LEFT_BOTTOM),
            Self::BottomRight => Some(egui::Align2::RIGHT_BOTTOM),
        }
    }
}

/// FSRowsPerScriptDialog's bounds (the slider of panel_preferences_UI.xml).
pub const DIALOG_ROWS: std::ops::RangeInclusive<u32> = 2..=50;
/// NotificationToastLifeTime's slider: seconds.
pub const INFO_SECONDS: std::ops::RangeInclusive<f32> = 1.0..=300.0;
/// Aurora's own time for an offer's toast.
pub const OFFER_SECONDS: std::ops::RangeInclusive<f32> = 5.0..=300.0;
/// NotificationTipToastLifeTime's slider.
pub const TIP_SECONDS: std::ops::RangeInclusive<f32> = 1.0..=30.0;
/// ToastGap's slider: pixels.
pub const GAP: std::ops::RangeInclusive<f32> = 0.0..=30.0;
/// ToastFadingTime's slider: seconds.
pub const FADE_SECONDS: std::ops::RangeInclusive<f32> = 0.0..=10.0;
/// OverflowToastHeight's slider: pixels.
pub const OVERFLOW_HEIGHT: std::ops::RangeInclusive<f32> = 0.0..=250.0;
/// ChannelBottomPanelMargin offered: pixels.
pub const BOTTOM_MARGIN: std::ops::RangeInclusive<f32> = 0.0..=100.0;
/// FSAnimatedScriptDialogs: a dialog unrolls at 2.5 of its height a second
/// (LLScriptFloater::draw).
pub const UNROLL_PER_SECOND: f64 = 2.5;
/// FSOfferThrottleMaxCount: offers within the throttle's 10 seconds.
pub const THROTTLE_COUNT: std::ops::RangeInclusive<u32> = 1..=50;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationOptions {
    /// ScriptDialogLimitations (one per channel).
    pub script_dialog_limit: DialogLimit,
    /// ScriptDialogsPosition.
    pub script_dialog_position: DialogPosition,
    /// Lines of a script dialog's text shown before it scrolls
    /// (FSRowsPerScriptDialog, 20).
    pub script_dialog_rows: u32,
    /// No Block button on script dialogs (FSRemoveScriptBlockButton, off).
    pub hide_script_block_button: bool,
    /// A friend's online / offline line in an open conversation counts as
    /// unread, so its tab flashes (FSIMChatFlashOnFriendStatusChange, off).
    pub flash_on_friend_status: bool,
    /// A line of nearby chat when a blocked group chat is left
    /// (FSReportMutedGroupChat, off).
    pub report_muted_group_chat: bool,
    /// Open the Conversations window when a message kept while we were
    /// offline arrives (FSOpenIMContainerOnOfflineMessage, off).
    pub open_conversations_on_offline_message: bool,
    /// Seconds an information toast stays (NotificationToastLifeTime; 7,
    /// Aurora's time before this option).
    pub info_toast_seconds: f32,
    /// Seconds an offer's toast stays before it waits in the list (25,
    /// Aurora's own).
    pub offer_toast_seconds: f32,
    /// Seconds a friend's online / offline toast stays
    /// (NotificationTipToastLifeTime, 10).
    pub tip_toast_seconds: f32,
    /// Pixels between two toasts (ToastGap; 6, Aurora's).
    pub toast_gap: f32,
    /// The toasts stand in front of the windows (FSShowToastsInFront; on,
    /// Aurora's place before this option, where the Legacy Viewer puts them
    /// behind).
    pub toasts_in_front: bool,
    /// A notice when a landmark we made reaches the inventory
    /// (FSLandmarkCreatedNotification, off).
    pub landmark_created_notice: bool,
    /// No message when our object is returned to Lost and Found
    /// (FSDisableReturnObjectNotification, off).
    pub hide_object_returned: bool,
    /// Say when offers come too fast, without the giver's name
    /// (FSNotifyIncomingObjectSpam, on).
    pub notify_offer_spam: bool,
    /// Say it with the giver's name (FSNotifyIncomingObjectSpamFrom, on).
    pub notify_offer_spam_from: bool,
    /// Kept offers within 10 seconds before they stop showing in the
    /// inventory (FSOfferThrottleMaxCount, 5).
    pub offer_throttle_count: u32,
    /// L$ received said in nearby chat instead of a toast
    /// (FSPaymentInfoInChat, off).
    pub payments_in_chat: bool,
    /// The grid's latest status news in nearby chat at login
    /// (AutoQueryGridStatus, off).
    pub grid_status_at_login: bool,
    /// The question to take L$ answers Deny by default
    /// (FSPermissionDebitDefaultDeny, on).
    pub debit_default_deny: bool,
    /// Script dialogs unroll from the top when they come, in a top corner
    /// (FSAnimatedScriptDialogs, off).
    pub animated_script_dialogs: bool,
    /// Seconds a toast takes to fade out after its time (ToastFadingTime;
    /// 0, Aurora's toasts leaving at once, where the Legacy Viewer fades
    /// for 2).
    pub toast_fade_seconds: f32,
    /// Height of the "+ N more" row under the toasts (OverflowToastHeight;
    /// 0, Aurora's single line, where the Legacy Viewer's overflow toast
    /// is 72 pixels high).
    pub overflow_toast_height: f32,
    /// Room left under the script dialogs of a bottom corner
    /// (ChannelBottomPanelMargin; 0, Aurora's place, the Legacy Viewer
    /// leaving 35 pixels above its bottom tray).
    pub toast_bottom_margin: f32,
}

impl Default for NotificationOptions {
    fn default() -> Self {
        NotificationOptions {
            script_dialog_limit: DialogLimit::PerChannel,
            script_dialog_position: DialogPosition::WithNotifications,
            script_dialog_rows: 20,
            hide_script_block_button: false,
            flash_on_friend_status: false,
            report_muted_group_chat: false,
            open_conversations_on_offline_message: false,
            info_toast_seconds: 7.0,
            offer_toast_seconds: 25.0,
            tip_toast_seconds: 10.0,
            toast_gap: 6.0,
            toasts_in_front: true,
            landmark_created_notice: false,
            hide_object_returned: false,
            notify_offer_spam: true,
            notify_offer_spam_from: true,
            offer_throttle_count: 5,
            payments_in_chat: false,
            grid_status_at_login: false,
            debit_default_deny: true,
            animated_script_dialogs: false,
            toast_fade_seconds: 0.0,
            overflow_toast_height: 0.0,
            toast_bottom_margin: 0.0,
        }
    }
}

fn finite_in(v: f32, range: std::ops::RangeInclusive<f32>, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(*range.start(), *range.end())
    } else {
        default
    }
}

impl NotificationOptions {
    /// The bounds of a settings file.
    pub fn sanitize(&mut self) {
        let d = Self::default();
        self.script_dialog_rows = self.script_dialog_rows.clamp(*DIALOG_ROWS.start(), *DIALOG_ROWS.end());
        self.info_toast_seconds = finite_in(self.info_toast_seconds, INFO_SECONDS, d.info_toast_seconds);
        self.offer_toast_seconds = finite_in(self.offer_toast_seconds, OFFER_SECONDS, d.offer_toast_seconds);
        self.tip_toast_seconds = finite_in(self.tip_toast_seconds, TIP_SECONDS, d.tip_toast_seconds);
        self.toast_gap = finite_in(self.toast_gap, GAP, d.toast_gap);
        self.offer_throttle_count = self.offer_throttle_count.clamp(*THROTTLE_COUNT.start(), *THROTTLE_COUNT.end());
        self.toast_fade_seconds = finite_in(self.toast_fade_seconds, FADE_SECONDS, d.toast_fade_seconds);
        self.overflow_toast_height = finite_in(self.overflow_toast_height, OVERFLOW_HEIGHT, d.overflow_toast_height);
        self.toast_bottom_margin = finite_in(self.toast_bottom_margin, BOTTOM_MARGIN, d.toast_bottom_margin);
    }

    /// How much of an arriving script dialog shows, `elapsed` seconds after
    /// it came (FSAnimatedScriptDialogs, only in a top corner as
    /// LLScriptFloater::initFromNotification): 0 to 1.
    pub fn dialog_unrolled(&self, elapsed: f64) -> f32 {
        let top = self.script_dialog_position.corner().is_some_and(|c| c.y() == egui::Align::Min);
        if !self.animated_script_dialogs || !top {
            return 1.0;
        }
        (elapsed.max(0.0) * UNROLL_PER_SECOND).min(1.0) as f32
    }

    /// The opacity of a toast `left` seconds before it leaves
    /// (ToastFadingTime): opaque until its last `toast_fade_seconds`.
    pub fn toast_opacity(&self, left: f64) -> f32 {
        if self.toast_fade_seconds <= 0.0 {
            return 1.0;
        }
        (left / f64::from(self.toast_fade_seconds)).clamp(0.0, 1.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_settings_file_gets_the_defaults() {
        let s: crate::settings::Settings = serde_json::from_str("{}").expect("empty settings");
        assert_eq!(s.notification_options, NotificationOptions::default());
        let o: NotificationOptions = serde_json::from_str(r#"{"hide_script_block_button":true}"#).expect("partial");
        assert!(o.hide_script_block_button);
        assert_eq!(o.script_dialog_limit, DialogLimit::PerChannel);
        assert!(o.debit_default_deny);
    }

    #[test]
    fn sanitize_keeps_values_in_their_ranges() {
        let mut o = NotificationOptions {
            script_dialog_rows: 0,
            info_toast_seconds: f32::NAN,
            offer_toast_seconds: 1000.0,
            tip_toast_seconds: 0.0,
            toast_gap: -4.0,
            offer_throttle_count: 0,
            ..NotificationOptions::default()
        };
        o.sanitize();
        assert_eq!(o.script_dialog_rows, 2);
        assert_eq!(o.info_toast_seconds, 7.0);
        assert_eq!(o.offer_toast_seconds, 300.0);
        assert_eq!(o.tip_toast_seconds, 1.0);
        assert_eq!(o.toast_gap, 0.0);
        assert_eq!(o.offer_throttle_count, 1);
        let mut o = NotificationOptions {
            toast_fade_seconds: 99.0,
            overflow_toast_height: -1.0,
            toast_bottom_margin: f32::NAN,
            ..NotificationOptions::default()
        };
        o.sanitize();
        assert_eq!(
            (o.toast_fade_seconds, o.overflow_toast_height, o.toast_bottom_margin),
            (10.0, 0.0, 0.0)
        );
    }

    #[test]
    fn dialogs_unroll_only_when_asked_in_a_top_corner() {
        let mut o = NotificationOptions {
            animated_script_dialogs: true,
            ..NotificationOptions::default()
        };
        assert_eq!(o.dialog_unrolled(0.0), 1.0, "with the notifications");
        o.script_dialog_position = DialogPosition::BottomRight;
        assert_eq!(o.dialog_unrolled(0.0), 1.0, "a bottom corner");
        o.script_dialog_position = DialogPosition::TopLeft;
        assert_eq!(o.dialog_unrolled(0.0), 0.0);
        assert_eq!(o.dialog_unrolled(0.2), 0.5);
        assert_eq!(o.dialog_unrolled(1.0), 1.0);
        o.animated_script_dialogs = false;
        assert_eq!(o.dialog_unrolled(0.0), 1.0);
    }

    #[test]
    fn toasts_fade_in_their_last_seconds_only() {
        let mut o = NotificationOptions::default();
        assert_eq!(o.toast_opacity(0.1), 1.0, "Aurora's: no fade");
        o.toast_fade_seconds = 2.0;
        assert_eq!(o.toast_opacity(5.0), 1.0);
        assert_eq!(o.toast_opacity(1.0), 0.5);
        assert_eq!(o.toast_opacity(-1.0), 0.0);
    }

    #[test]
    fn only_the_corners_take_dialogs_apart() {
        assert_eq!(DialogPosition::WithNotifications.corner(), None);
        assert_eq!(DialogPosition::BottomLeft.corner(), Some(egui::Align2::LEFT_BOTTOM));
        assert_eq!(DialogPosition::ALL.len(), 5);
        assert_eq!(DialogLimit::ALL.len(), 6);
    }
}
