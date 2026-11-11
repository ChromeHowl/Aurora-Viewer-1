//! The rules behind Preferences › Chat › Notifications' options
//! (`settings/notification_options.rs`), each where its Legacy Viewer
//! source decides it:
//!
//! - how many script dialogs an object keeps (ScriptDialogLimitations,
//!   LLScriptFloaterManager::onAddNotification, indra/newview/llscriptfloater.cpp);
//! - a friend's online / offline line in an open conversation that makes
//!   its tab flash (FSIMChatFlashOnFriendStatusChange,
//!   FSFloaterIMContainer::onNewMessageReceived);
//! - the line of nearby chat for a group chat left because it is blocked
//!   (FSReportMutedGroupChat, LLIMMgr::addMessage / processNewMessage in
//!   indra/newview/llimview.cpp, GroupChatMuteNotice);
//! - the "object returned" message dropped (FSDisableReturnObjectNotification,
//!   IM_FROM_TASK in indra/newview/llimprocessing.cpp);
//! - the notice of a landmark we made (FSLandmarkCreatedNotification,
//!   open_inventory_offer, LandmarkCreated);
//! - the throttle of kept offers shown in the inventory
//!   (check_offer_throttle, FSOfferThrottleMaxCount,
//!   FSNotifyIncomingObjectSpam / From, indra/newview/llviewermessage.cpp);
//!
//! all originally LGPL 2.1.

use super::notice_topics::Topic;
use super::notifications::{Data, Kind};
use super::{ChatKind, ChatLine, Origin, World};
use crate::settings::notification_options::DialogLimit;
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

/// Where a script dialog's object stands, for ScriptDialogLimitations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// In the world, or unknown (the Legacy Viewer's object not found).
    World,
    /// Worn on the body (getAttachmentItemID not null).
    Attachment,
    /// Worn on a HUD point (isHUDAttachment).
    Hud,
}

/// Whether a new dialog of an object (from `source`, on `channel`) closes
/// an older dialog of the same object on `old_channel`.
pub fn replaces(limit: DialogLimit, source: Source, channel: i32, old_channel: i32) -> bool {
    let per_channel = match limit {
        DialogLimit::Unconstrained => return false,
        DialogLimit::UnconstrainedHuds if source == Source::Hud => return false,
        DialogLimit::PerChannel => true,
        // a HUD is an attachment too
        DialogLimit::PerChannelAttachments => source != Source::World,
        DialogLimit::PerChannelHuds => source == Source::Hud,
        DialogLimit::PerObject | DialogLimit::UnconstrainedHuds => false,
    };
    !per_channel || channel == old_channel
}

/// OFFER_THROTTLE_TIME: the throttle's window.
pub const OFFER_THROTTLE_TIME: Duration = Duration::from_secs(10);

/// check_offer_throttle's timer and count.
#[derive(Debug, Default)]
pub struct OfferThrottle {
    since: Option<Instant>,
    count: u32,
    told: bool,
}

/// What the throttle says of one kept offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Throttle {
    Pass,
    /// Held back; `first`: the first held of its window, which says so.
    Held {
        first: bool,
    },
}

impl OfferThrottle {
    /// One kept offer at `now`, `max` allowed per window: the window
    /// starts with the first offer after the last one ended.
    pub fn hit(&mut self, now: Instant, max: u32) -> Throttle {
        if self.since.is_none_or(|t| now.duration_since(t) >= OFFER_THROTTLE_TIME) {
            self.since = Some(now);
            self.count = 1;
            self.told = false;
            return Throttle::Pass;
        }
        if self.count >= max {
            let first = !self.told;
            self.told = true;
            return Throttle::Held { first };
        }
        self.count += 1;
        Throttle::Pass
    }
}

/// The server's text of an object returned to Lost and Found (the test of
/// FSDisableReturnObjectNotification).
const RETURNED: &str = "been returned to your inventory";
/// IM_FROM_TASK.
const FROM_TASK: u8 = 19;

/// The message of an object returned to our Lost and Found folder.
pub fn is_object_returned(dialog: u8, message: &str) -> bool {
    dialog == FROM_TASK && message.contains(RETURNED)
}

/// The landmark asset type (AT_LANDMARK).
const AT_LANDMARK: i32 = 3;

impl World {
    /// Where a script dialog's object stands: the attachment point of its
    /// linkset, if worn.
    fn dialog_source(&self, object: &Uuid) -> Source {
        let point = self.objects.index_of_uuid(object).and_then(|idx| self.objects.worn_point(idx));
        match point {
            Some(31..=38) => Source::Hud,
            Some(_) => Source::Attachment,
            None => Source::World,
        }
    }

    /// Before a new script dialog of `object` on `channel` is listed: the
    /// older ones it replaces leave, unanswered, as the Legacy Viewer
    /// closes their floaters (removeNotification, no reply sent).
    pub(super) fn limit_script_dialogs(&mut self, object: Uuid, channel: i32) {
        let limit = self.notifications.options.script_dialog_limit;
        if limit == DialogLimit::Unconstrained {
            return;
        }
        let source = self.dialog_source(&object);
        self.notifications.list.retain(|n| match n.data {
            Data::Dialog { object: o, channel: c, .. } | Data::TextBox { object: o, channel: c, .. } => {
                o != object || !replaces(limit, source, channel, c)
            }
            _ => true,
        });
    }

    /// A friend's online / offline line in their open conversation: unread
    /// (its tab flashes) only with the option, and never for a friend the
    /// user hides.
    pub(super) fn friend_status_line(&mut self, friend: Uuid, text: String) {
        let unread = self.notifications.options.flash_on_friend_status && self.social.is_friend_shown(&friend);
        let line = ChatLine {
            time: SystemTime::now(),
            from: String::new(),
            text,
            kind: ChatKind::System,
            source: Uuid::nil(),
            origin: Origin::Live,
        };
        self.conversation_push(friend, line, unread);
    }

    /// A blocked group chat just left: said in nearby chat with the option.
    pub(super) fn report_muted_group_chat(&mut self, group: Uuid) {
        if self.notifications.options.report_muted_group_chat {
            let name = format!("secondlife:///app/group/{group}/about");
            self.system_notice(aurora_i18n::tr!("b5c-group-chat-muted", name = name), Topic::People);
        }
    }

    /// The "object returned" message of an object's IM, dropped with the
    /// option.
    pub(super) fn object_return_hidden(&self, im: &aurora_net::InstantMessage) -> bool {
        self.notifications.options.hide_object_returned && is_object_returned(im.dialog, &im.message)
    }

    /// A landmark we made reached the inventory (`item`, from
    /// UpdateCreateInventoryItem): LandmarkCreated with the option.
    pub fn landmark_created_notice(&mut self, item: &aurora_net::inventory::InvItem) {
        if !self.notifications.options.landmark_created_notice || item.asset_type != AT_LANDMARK {
            return;
        }
        let folder = self
            .inventory
            .folders
            .get(&item.parent)
            .map(|f| f.info.name.clone())
            .unwrap_or_default();
        let text = aurora_i18n::tr!("b5c-landmark-created", name = item.name.clone(), folder = folder);
        self.notifications
            .push(Kind::Info, aurora_i18n::tr!("b5c-landmark-created-title"), text, Data::None);
    }

    /// check_offer_throttle for a kept offer about to be shown in the
    /// inventory (ShowInInventory): past the count within 10 seconds (once
    /// logged in), it is not shown and the first one held says so, with the
    /// giver's name when known. Without ShowInInventory nothing counts.
    pub(super) fn offer_throttle_passes(&mut self, giver: &str) -> bool {
        if !self.inventory_offers.show_accepted || !self.notifications.started {
            return true;
        }
        let o = self.notifications.options;
        match self.notifications.offer_throttle.hit(Instant::now(), o.offer_throttle_count) {
            Throttle::Pass => true,
            Throttle::Held { first } => {
                let seconds = OFFER_THROTTLE_TIME.as_secs().to_string();
                let text = if giver.is_empty() {
                    o.notify_offer_spam
                        .then(|| aurora_i18n::tr!("b5c-offers-too-fast", seconds = seconds).to_owned())
                } else {
                    o.notify_offer_spam_from
                        .then(|| aurora_i18n::tr!("b5c-offers-too-fast-from", name = giver.to_owned(), seconds = seconds))
                };
                if first && let Some(text) = text {
                    self.notifications
                        .push(Kind::Info, aurora_i18n::tr!("b5c-offers-too-fast-title"), text, Data::None);
                }
                false
            }
        }
    }

    /// The name of the giver of the inventory offer `id` (the object's
    /// name for an object's offer), empty when unknown.
    pub(super) fn offer_giver(&self, id: u64) -> String {
        match self.notifications.list.iter().find(|n| n.id == id).map(|n| &n.data) {
            Some(Data::Inventory {
                task_offer: Some(offer), ..
            }) => super::dialog_names::shown(&offer.object_name).into_owned(),
            Some(Data::Inventory { from, .. }) if !from.is_nil() => self.social.name_of(from),
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_limit_replaces_what_the_legacy_viewer_closes() {
        use DialogLimit::*;
        use Source::*;
        let cases = [
            // (limit, source, same channel, other channel)
            (PerObject, World, true, true),
            (PerObject, Hud, true, true),
            (PerChannel, World, true, false),
            (PerChannel, Hud, true, false),
            (PerChannelAttachments, World, true, true),
            (PerChannelAttachments, Attachment, true, false),
            (PerChannelAttachments, Hud, true, false),
            (PerChannelHuds, Attachment, true, true),
            (PerChannelHuds, Hud, true, false),
            (UnconstrainedHuds, Hud, false, false),
            (UnconstrainedHuds, Attachment, true, true),
            (Unconstrained, World, false, false),
        ];
        for (limit, source, same, other) in cases {
            assert_eq!(replaces(limit, source, 5, 5), same, "{limit:?} {source:?} same channel");
            assert_eq!(replaces(limit, source, 5, 7), other, "{limit:?} {source:?} other channel");
        }
    }

    #[test]
    fn the_throttle_holds_past_the_count_and_says_it_once_per_window() {
        let mut t = OfferThrottle::default();
        let t0 = Instant::now();
        for _ in 0..3 {
            assert_eq!(t.hit(t0, 3), Throttle::Pass);
        }
        assert_eq!(t.hit(t0, 3), Throttle::Held { first: true });
        assert_eq!(t.hit(t0 + Duration::from_secs(5), 3), Throttle::Held { first: false });
        // a new window
        assert_eq!(t.hit(t0 + OFFER_THROTTLE_TIME, 3), Throttle::Pass);
        assert_eq!(t.hit(t0 + OFFER_THROTTLE_TIME, 3), Throttle::Pass);
    }

    #[test]
    fn only_an_objects_return_message_is_recognised() {
        let text = "Your object 'Box' has been returned to your inventory Lost and Found folder from parcel 'Home'.";
        assert!(is_object_returned(19, text));
        assert!(!is_object_returned(0, text));
        assert!(!is_object_returned(19, "Hello"));
    }

    fn world() -> World {
        World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()))
    }

    fn dialog(object: u128, channel: i32) -> Data {
        Data::Dialog {
            sender: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            object: Uuid::from_u128(object),
            object_name: "Box".into(),
            own_object: false,
            channel,
            buttons: vec!["OK".into()],
        }
    }

    #[test]
    fn a_new_dialog_closes_the_older_one_of_its_object_and_channel() {
        let mut w = world();
        let a = w.notifications.push(Kind::Script, "Box", "a", dialog(7, 1));
        let b = w.notifications.push(Kind::Script, "Box", "b", dialog(7, 2));
        let c = w.notifications.push(Kind::Script, "Other", "c", dialog(8, 1));
        w.limit_script_dialogs(Uuid::from_u128(7), 1);
        let ids: Vec<u64> = w.notifications.list.iter().map(|n| n.id).collect();
        assert_eq!(ids, [b, c], "{a} closed");
        w.notifications.options.script_dialog_limit = DialogLimit::PerObject;
        w.limit_script_dialogs(Uuid::from_u128(7), 9);
        let ids: Vec<u64> = w.notifications.list.iter().map(|n| n.id).collect();
        assert_eq!(ids, [c]);
        w.notifications.options.script_dialog_limit = DialogLimit::Unconstrained;
        w.limit_script_dialogs(Uuid::from_u128(8), 1);
        assert_eq!(w.notifications.list.len(), 1);
    }

    #[test]
    fn a_friends_status_line_is_unread_only_with_the_option_and_never_hidden() {
        let friend = Uuid::from_u128(5);
        let unread = |w: &World| w.social.ims.iter().find(|s| s.other == friend).map_or(0, |s| s.unread);
        let mut w = world();
        w.social.session_mut(friend);
        w.friend_status_line(friend, "online".into());
        assert_eq!(unread(&w), 0, "off by default");
        w.notifications.options.flash_on_friend_status = true;
        w.friend_status_line(friend, "online".into());
        // not a friend at all: no flash
        assert_eq!(unread(&w), 0);
        w.social.friends.push(crate::world::social::Friend {
            id: friend,
            online: true,
            rights_given: 1,
            rights_has: 1,
        });
        w.friend_status_line(friend, "offline".into());
        assert_eq!(unread(&w), 1, "a shown friend flashes");
        assert!(w.social.hide_friend(friend));
        w.friend_status_line(friend, "online".into());
        assert_eq!(unread(&w), 1, "a hidden friend never flashes");
    }

    #[test]
    fn an_offline_message_opens_the_conversations_once_with_the_option() {
        let mut n = super::super::notifications::Notifications::default();
        n.offline_message(true);
        assert!(!n.take_open_conversations(), "off by default");
        n.options.open_conversations_on_offline_message = true;
        n.offline_message(false);
        assert!(!n.take_open_conversations(), "a live message");
        n.offline_message(true);
        n.offline_message(true);
        assert!(n.take_open_conversations());
        assert!(!n.take_open_conversations(), "asked once");
    }

    #[test]
    fn the_report_of_a_muted_group_chat_waits_for_its_option() {
        let mut w = world();
        let before = w.chat.len();
        w.report_muted_group_chat(Uuid::from_u128(9));
        assert_eq!(w.chat.len(), before);
        w.notifications.options.report_muted_group_chat = true;
        w.report_muted_group_chat(Uuid::from_u128(9));
        assert_eq!(w.chat.len(), before + 1);
    }
}
