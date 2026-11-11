//! How the chat is shown and sent (Preferences › Chat): the Legacy
//! Viewer's chat display options that Aurora had no setting for. Each field
//! names its Legacy setting (indra/newview/app_settings/settings.xml) and
//! keeps its default, unless the comment says why not.

use serde::{Deserialize, Serialize};

/// The chat display options (see the module).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatDisplay {
    /// PlayChatAnim (on): our avatar plays the talk, shout or whisper
    /// animation when we say something on channel 0 (`chat_animation`).
    pub play_chat_anim: bool,
    /// FSShowIMInChatHistory (off): residents' IMs and group chat copied
    /// into the nearby chat (`world/chat_copies.rs`).
    pub im_in_nearby_chat: bool,
    /// FSIMChatHistoryFade (0.5, 0.25 to 1): how visible those copies are.
    pub im_nearby_fade: f32,
    /// FSLogImToChatConsole (off): IMs shown in the nearby chat lines over
    /// the world in place of their toast.
    pub im_in_console: bool,
    /// FSLogGroupImToChatConsole (off): the same for group chat.
    pub group_im_in_console: bool,
    /// FSShowGroupNameLength (0): how much of a group's name is printed
    /// before a group line copied into the nearby chat: -1 all of it, 0
    /// none, else that many characters (at most 63).
    pub group_name_length: i32,
    /// FSGroupNoticesToIMLog (on): a group notice also enters its group's
    /// conversation and transcript.
    pub group_notices_in_chat: bool,
    /// FSDisableLabeledChatLinks (on): a labelled web link of an IM, a group
    /// chat or a conference shows its real address at once on hover, in a
    /// large hint.
    pub link_hint_im: bool,
    /// FSDisableLabeledChatLinksNearbyChat (on): the same in nearby chat.
    pub link_hint_nearby: bool,
    /// FSHighlightGroupMods (on): the moderators' messages of a group chat
    /// stand out with the styles below.
    pub highlight_moderators: bool,
    /// FSModNameStyle (bold): the style of a moderator's name.
    pub moderator_name_style: TextStyle,
    /// FSModTextStyle (bold): the style of a moderator's text.
    pub moderator_text_style: TextStyle,
    /// FSIMTabNameFormat (display name): how a conversation with one
    /// resident is named in the list.
    pub tab_name_format: TabNameFormat,
    /// ConsoleBackgroundOpacity: the nearby chat lines' background. Aurora's
    /// look before the option (0.75) is kept as the default, where the
    /// Legacy Viewer has 0.5.
    pub console_opacity: f32,
    /// ChatFullWidth: the nearby chat lines as wide as the window. Off, as
    /// Aurora's lines were (the Legacy Viewer has it on).
    pub console_full_width: bool,
    /// ChatConsoleFontSize: the text size of the nearby chat lines. Medium
    /// is the size Aurora always had.
    pub console_font: ConsoleFont,
    /// FSIMSystemMessageBrackets (off): the system's lines in [brackets] in
    /// the Legacy style of the conversations.
    pub system_brackets: bool,
    /// FSChatHistoryShowYou (off): "You" in place of our own name.
    pub show_you: bool,
    /// FSMarkObjects (off): an object without a name reads "(No name)".
    pub mark_unnamed_objects: bool,
    /// FSBeyondNearbyChatColorDiminishFactor (0.8): the colour of nearby
    /// chat said from beyond the region's say range (20 m,
    /// `world/chat_range.rs`) is diminished by this factor; 1 changes
    /// nothing.
    pub beyond_range_dim: f32,
    /// AutoCloseOOC (off): a message that opens "((" without closing it
    /// gets its "))" added.
    pub auto_close_ooc: bool,
    /// FSOOCPrefix ("(("): what Alt+Enter puts before the message.
    pub ooc_prefix: String,
    /// FSOOCPostfix ("))"): what Alt+Enter puts after it.
    pub ooc_postfix: String,
    /// FSShowChatType (on): the volume button (whisper, say, shout) of the
    /// nearby chat field.
    pub show_chat_type: bool,
    /// FSEnableEmojiWindowPopupWhileTyping (on): typing ":" and two letters
    /// opens the emoji suggestions.
    pub emoji_popup_while_typing: bool,
    /// FSShowEmojiButton (on): the emoji button of the chat fields.
    pub show_emoji_button: bool,
    /// FSShowIMSendButton (on): the send button of the conversations.
    pub show_im_send_button: bool,
    /// FSShowMutedChatHistory (off): what blocked residents and objects say
    /// stays in the nearby chat, greyed (never over the world nor in a
    /// bubble).
    pub show_muted_chat: bool,
    /// FSSecondsinChatTimestamps (off): seconds in the times of the chat
    /// and of the transcripts.
    pub seconds_in_timestamps: bool,
    /// More options (`settings/chat_more.rs`).
    pub more: super::chat_more::ChatMore,
}

impl Default for ChatDisplay {
    fn default() -> Self {
        Self {
            play_chat_anim: true,
            im_in_nearby_chat: false,
            im_nearby_fade: 0.5,
            im_in_console: false,
            group_im_in_console: false,
            group_name_length: 0,
            group_notices_in_chat: true,
            link_hint_im: true,
            link_hint_nearby: true,
            highlight_moderators: true,
            moderator_name_style: TextStyle::BOLD,
            moderator_text_style: TextStyle::BOLD,
            tab_name_format: TabNameFormat::DisplayName,
            console_opacity: 0.75,
            console_full_width: false,
            console_font: ConsoleFont::Medium,
            system_brackets: false,
            show_you: false,
            mark_unnamed_objects: false,
            beyond_range_dim: 0.8,
            auto_close_ooc: false,
            ooc_prefix: "((".into(),
            ooc_postfix: "))".into(),
            show_chat_type: true,
            emoji_popup_while_typing: true,
            show_emoji_button: true,
            show_im_send_button: true,
            show_muted_chat: false,
            seconds_in_timestamps: false,
            more: Default::default(),
        }
    }
}

impl ChatDisplay {
    /// The values a settings file may hold brought within their bounds.
    pub fn sanitize(&mut self) {
        self.im_nearby_fade = finite_or(self.im_nearby_fade, 0.5).clamp(0.25, 1.0);
        self.group_name_length = self.group_name_length.clamp(-1, 63);
        self.console_opacity = finite_or(self.console_opacity, 0.75).clamp(0.0, 1.0);
        self.beyond_range_dim = finite_or(self.beyond_range_dim, 0.8).clamp(0.0, 1.0);
        self.moderator_name_style.0 &= TextStyle::ALL;
        self.moderator_text_style.0 &= TextStyle::ALL;
        // a marker of a few characters, never a line of its own
        for marker in [&mut self.ooc_prefix, &mut self.ooc_postfix] {
            let clean: String = marker.chars().filter(|c| !c.is_control()).take(MARKER_CHARS).collect();
            *marker = clean;
        }
    }
}

/// The longest OOC marker kept.
pub const MARKER_CHARS: usize = 8;

fn finite_or(v: f32, default: f32) -> f32 {
    if v.is_finite() { v } else { default }
}

/// FSModNameStyle / FSModTextStyle: bold 1, italic 2, underline 4, as the
/// Legacy settings store them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TextStyle(pub u8);

impl TextStyle {
    pub const BOLD: TextStyle = TextStyle(1);
    pub const ITALIC: TextStyle = TextStyle(2);
    pub const UNDERLINE: TextStyle = TextStyle(4);
    const ALL: u8 = 7;

    pub fn bold(self) -> bool {
        self.0 & Self::BOLD.0 != 0
    }

    pub fn italic(self) -> bool {
        self.0 & Self::ITALIC.0 != 0
    }

    pub fn underline(self) -> bool {
        self.0 & Self::UNDERLINE.0 != 0
    }

    /// The eight choices, in the Legacy Viewer's order.
    pub fn all() -> [TextStyle; 8] {
        std::array::from_fn(|i| TextStyle(i as u8))
    }
}

/// FSIMTabNameFormat: 0 display name, 1 user name, 2 display name (user
/// name), 3 user name (display name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TabNameFormat {
    #[default]
    DisplayName,
    UserName,
    DisplayThenUser,
    UserThenDisplay,
}

impl TabNameFormat {
    /// The name of a conversation with one resident, from their display
    /// name and user name (either may be unknown yet: empty).
    pub fn name(self, display: &str, user: &str) -> String {
        let both = |a: &str, b: &str| {
            if a.is_empty() || b.is_empty() || a.eq_ignore_ascii_case(b) {
                if a.is_empty() { b.to_owned() } else { a.to_owned() }
            } else {
                format!("{a} ({b})")
            }
        };
        match self {
            TabNameFormat::DisplayName if !display.is_empty() => display.to_owned(),
            TabNameFormat::DisplayName => user.to_owned(),
            TabNameFormat::UserName if !user.is_empty() => user.to_owned(),
            TabNameFormat::UserName => display.to_owned(),
            TabNameFormat::DisplayThenUser => both(display, user),
            TabNameFormat::UserThenDisplay => both(user, display),
        }
    }
}

/// ChatConsoleFontSize: Small, Medium, Large, Huge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ConsoleFont {
    Small,
    #[default]
    Medium,
    Large,
    Huge,
}

impl ConsoleFont {
    /// The factor on the lines' text size.
    pub fn scale(self) -> f32 {
        match self {
            ConsoleFont::Small => 0.85,
            ConsoleFont::Medium => 1.0,
            ConsoleFont::Large => 1.15,
            ConsoleFont::Huge => 1.35,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_settings_file_takes_the_legacy_defaults() {
        let s: crate::settings::Settings = serde_json::from_value(serde_json::json!({})).expect("old file");
        assert_eq!(s.chat_display, ChatDisplay::default());
        assert!(s.chat_display.play_chat_anim && s.chat_display.group_notices_in_chat);
        assert_eq!(s.chat_display.moderator_name_style, TextStyle::BOLD);
        let partial: ChatDisplay = serde_json::from_value(serde_json::json!({ "show_you": true })).expect("partial");
        assert!(partial.show_you);
        assert_eq!(partial.ooc_prefix, "((");
    }

    #[test]
    fn sanitize_bounds_every_value() {
        let mut d = ChatDisplay {
            im_nearby_fade: 0.0,
            group_name_length: 500,
            console_opacity: f32::NAN,
            beyond_range_dim: 3.0,
            moderator_name_style: TextStyle(255),
            ooc_prefix: "((\n((((((((((".into(),
            ..ChatDisplay::default()
        };
        d.sanitize();
        assert_eq!(d.im_nearby_fade, 0.25);
        assert_eq!(d.group_name_length, 63);
        assert_eq!(d.console_opacity, 0.75);
        assert_eq!(d.beyond_range_dim, 1.0);
        assert_eq!(d.moderator_name_style, TextStyle(7));
        assert_eq!(d.ooc_prefix.chars().count(), MARKER_CHARS);
        assert!(!d.ooc_prefix.contains('\n'));
    }

    #[test]
    fn styles_are_the_legacy_bits() {
        let s = TextStyle(5);
        assert!(s.bold() && !s.italic() && s.underline());
        assert_eq!(TextStyle::all().len(), 8);
        assert_eq!(TextStyle::all()[3], TextStyle(3));
    }

    #[test]
    fn tab_names_follow_the_format() {
        let (d, u) = ("Jane", "jane.doe");
        assert_eq!(TabNameFormat::DisplayName.name(d, u), "Jane");
        assert_eq!(TabNameFormat::UserName.name(d, u), "jane.doe");
        assert_eq!(TabNameFormat::DisplayThenUser.name(d, u), "Jane (jane.doe)");
        assert_eq!(TabNameFormat::UserThenDisplay.name(d, u), "jane.doe (Jane)");
        // a name not known yet, or the same twice
        assert_eq!(TabNameFormat::DisplayThenUser.name("", u), "jane.doe");
        // (a display name that only differs by case counts as the default
        // one: FSFloaterIM::onAvatarNameCache shows the user name alone)
        assert_eq!(TabNameFormat::UserThenDisplay.name("JaneDoe", "janedoe"), "janedoe");
        assert_eq!(TabNameFormat::UserName.name(d, ""), "Jane");
    }
}
