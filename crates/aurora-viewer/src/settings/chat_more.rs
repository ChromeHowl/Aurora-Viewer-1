//! More of the Legacy Viewer's chat and conversation options, kept inside
//! `chat_display` (its `more` field) so that the chat's renderers and the
//! world read them through the same copy. Each field names its Legacy
//! setting (indra/newview/app_settings/settings.xml) and keeps its default,
//! unless the comment says why not.

use serde::{Deserialize, Serialize};

/// The options (see the module).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatMore {
    /// FSAutoOrderIMTabsPriorities: the order of the conversation list's
    /// kinds. Aurora's list was always ordered by kind (private messages,
    /// groups, conferences): that order stays the default, where the
    /// Legacy Viewer orders by creation unless FSAutoOrderIMTabs is on
    /// (then groups, conferences, private messages).
    pub tab_order: TabOrder,
    /// FSAutoOrderIMTabsAtTop (off): a new conversation goes first in its
    /// kind instead of last.
    pub new_tabs_first: bool,
    /// FSColorUsername (off): the username of the name tags in its own
    /// colour (NameTagUsername, `ColorSettings::tag_username`).
    pub color_username: bool,
    /// FSColorIMsDistinctly (off): the IMs and group chat shown in the
    /// nearby chat and its lines over the world in their own colour
    /// (AgentIMColor, `ColorSettings::chat_im`).
    pub color_ims: bool,
    /// FSUseBWEmojis (off): emoji in black and white.
    pub bw_emojis: bool,
    /// FSUnfocusChatHistoryOnReturn (on): with the chat bar letting go of
    /// the keyboard after a message (CloseChatOnReturn, Aurora's
    /// `chat_keep_focus` off), the nearby chat's field of the
    /// Conversations window lets go of it too.
    pub unfocus_nearby_on_send: bool,
    /// FSLogIMInChatHistory (off): the IMs copied into the nearby chat
    /// are written to its transcript too.
    pub log_im_copies: bool,
}

impl Default for ChatMore {
    fn default() -> Self {
        Self {
            tab_order: TabOrder::default(),
            new_tabs_first: false,
            color_username: false,
            color_ims: false,
            bw_emojis: false,
            unfocus_nearby_on_send: true,
            log_im_copies: false,
        }
    }
}

/// A kind of conversation, for their order in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationKind {
    Private,
    Group,
    Conference,
}

/// FSAutoOrderIMTabsPriorities: the six orders of the three kinds, named
/// as the Legacy combo box lists them ("210": groups, conferences, IMs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TabOrder {
    /// "102", Aurora's order.
    #[default]
    ImsGroupsConferences,
    /// "012".
    ImsConferencesGroups,
    /// "210", the Legacy default.
    GroupsConferencesIms,
    /// "201".
    GroupsImsConferences,
    /// "120".
    ConferencesGroupsIms,
    /// "021".
    ConferencesImsGroups,
}

impl TabOrder {
    pub const ALL: [TabOrder; 6] = [
        TabOrder::ImsGroupsConferences,
        TabOrder::ImsConferencesGroups,
        TabOrder::GroupsConferencesIms,
        TabOrder::GroupsImsConferences,
        TabOrder::ConferencesGroupsIms,
        TabOrder::ConferencesImsGroups,
    ];

    /// The kinds from the top of the list.
    pub fn kinds(self) -> [ConversationKind; 3] {
        use ConversationKind::{Conference as C, Group as G, Private as I};
        match self {
            TabOrder::ImsGroupsConferences => [I, G, C],
            TabOrder::ImsConferencesGroups => [I, C, G],
            TabOrder::GroupsConferencesIms => [G, C, I],
            TabOrder::GroupsImsConferences => [G, I, C],
            TabOrder::ConferencesGroupsIms => [C, G, I],
            TabOrder::ConferencesImsGroups => [C, I, G],
        }
    }

    /// The Legacy setting's value: the priority of groups, conferences and
    /// IMs, highest first in the list. Aurora stores the variant itself;
    /// the table checks `kinds` against the Legacy values.
    #[cfg(test)]
    pub fn legacy_value(self) -> &'static str {
        match self {
            TabOrder::ImsGroupsConferences => "102",
            TabOrder::ImsConferencesGroups => "012",
            TabOrder::GroupsConferencesIms => "210",
            TabOrder::GroupsImsConferences => "201",
            TabOrder::ConferencesGroupsIms => "120",
            TabOrder::ConferencesImsGroups => "021",
        }
    }
}

/// Where a conversation that just opened goes among those of its kind
/// already listed (`order`, top first, `same_kind` telling them):
/// last, or before the first of its kind (FSAutoOrderIMTabsAtTop).
pub fn insert_at<T>(order: &[T], first: bool, same_kind: impl Fn(&T) -> bool) -> usize {
    if first {
        order.iter().position(same_kind).unwrap_or(order.len())
    } else {
        order.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ConversationKind::*;

    #[test]
    fn the_orders_follow_the_legacy_priorities() {
        for order in TabOrder::ALL {
            // each kind once, ordered by its digit in the Legacy value,
            // highest first (groups[0], conferences[1], IMs[2])
            let digits: Vec<u32> = order.legacy_value().chars().filter_map(|c| c.to_digit(10)).collect();
            let priority = |k: ConversationKind| match k {
                Group => digits[0],
                Conference => digits[1],
                Private => digits[2],
            };
            let kinds = order.kinds();
            assert!(kinds.windows(2).all(|w| priority(w[0]) > priority(w[1])), "{order:?}");
        }
        assert_eq!(TabOrder::default().kinds(), [Private, Group, Conference]);
        assert_eq!(TabOrder::GroupsConferencesIms.kinds(), [Group, Conference, Private]);
    }

    #[test]
    fn a_new_conversation_goes_last_or_first_of_its_kind() {
        let order = [(1, Private), (2, Group), (3, Private)];
        assert_eq!(insert_at(&order, false, |(_, k)| *k == Private), 3);
        assert_eq!(insert_at(&order, true, |(_, k)| *k == Private), 0);
        assert_eq!(insert_at(&order, true, |(_, k)| *k == Group), 1);
        assert_eq!(insert_at(&order, true, |(_, k)| *k == Conference), 3, "the first of its kind");
    }

    #[test]
    fn an_old_settings_file_takes_the_defaults() {
        let more: ChatMore = serde_json::from_value(serde_json::json!({ "color_ims": true })).expect("partial");
        assert!(more.color_ims && more.unfocus_nearby_on_send && !more.log_im_copies);
        assert_eq!(more.tab_order, TabOrder::ImsGroupsConferences);
    }
}
