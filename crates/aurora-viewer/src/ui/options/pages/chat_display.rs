//! Preferences › Chat: the Legacy Viewer's chat display options
//! (`settings/chat_display.rs`; panel_preferences_chat.xml, the labelled
//! links of panel_preferences_firestorm.xml and the beyond-range dimming of
//! panel_preferences_colors.xml).

use super::super::COMBO_W;
use super::super::cx::{Cx, field};
use crate::settings::chat_display::{ConsoleFont, MARKER_CHARS, TabNameFormat, TextStyle};

/// The nearby chat's group and the conversations' group.
pub fn groups(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("chat-extras-group-nearby"), |cx| {
        // PlayChatAnim
        cx.toggle(
            tr!("chat-extras-play-chat-anim"),
            tr!("chat-extras-play-chat-anim-hint"),
            &field!(chat_display.play_chat_anim),
        );
        // FSSecondsinChatTimestamps
        cx.toggle(
            tr!("chat-extras-seconds"),
            tr!("chat-extras-seconds-hint"),
            &field!(chat_display.seconds_in_timestamps),
        );
        // FSShowMutedChatHistory
        cx.toggle(
            tr!("chat-extras-show-muted"),
            tr!("chat-extras-show-muted-hint"),
            &field!(chat_display.show_muted_chat),
        );
        // FSShowChatType
        cx.toggle(
            tr!("chat-extras-show-chat-type"),
            tr!("chat-extras-show-chat-type-hint"),
            &field!(chat_display.show_chat_type),
        );
        // FSBeyondNearbyChatColorDiminishFactor (Colors)
        cx.slider(
            tr!("chat-extras-beyond-range"),
            tr!("chat-extras-beyond-range-hint"),
            &field!(chat_display.beyond_range_dim),
            0.0..=1.0,
            "",
        );
        // ChatConsoleFontSize, ChatFullWidth, ConsoleBackgroundOpacity
        cx.combo(
            tr!("chat-extras-console-font"),
            "",
            &field!(chat_display.console_font),
            &[
                (ConsoleFont::Small, tr!("chat-extras-console-font-small")),
                (ConsoleFont::Medium, tr!("chat-extras-console-font-medium")),
                (ConsoleFont::Large, tr!("chat-extras-console-font-large")),
                (ConsoleFont::Huge, tr!("chat-extras-console-font-huge")),
            ],
        );
        cx.toggle(
            tr!("chat-extras-console-full-width"),
            tr!("chat-extras-console-full-width-hint"),
            &field!(chat_display.console_full_width),
        );
        cx.slider(
            tr!("chat-extras-console-opacity"),
            "",
            &field!(chat_display.console_opacity),
            0.0..=1.0,
            "",
        );
        // AutoCloseOOC, FSOOCPrefix, FSOOCPostfix
        cx.toggle(
            tr!("chat-extras-auto-close-ooc"),
            tr!("chat-extras-auto-close-ooc-hint"),
            &field!(chat_display.auto_close_ooc),
        );
        cx.row(
            tr!("chat-extras-ooc-prefix"),
            tr!("chat-extras-ooc-markers-hint"),
            &field!(chat_display.ooc_prefix),
            |ui, s| marker(ui, &mut s.chat_display.ooc_prefix),
        );
        cx.row(
            tr!("chat-extras-ooc-postfix"),
            tr!("chat-extras-ooc-markers-hint"),
            &field!(chat_display.ooc_postfix),
            |ui, s| marker(ui, &mut s.chat_display.ooc_postfix),
        );
        // FSEnableEmojiWindowPopupWhileTyping, FSShowEmojiButton
        cx.toggle(
            tr!("chat-extras-emoji-popup"),
            tr!("chat-extras-emoji-popup-hint"),
            &field!(chat_display.emoji_popup_while_typing),
        );
        cx.toggle(tr!("chat-extras-emoji-button"), "", &field!(chat_display.show_emoji_button));
    });
    cx.group(tr!("chat-extras-group-conversations"), |cx| {
        // FSShowIMInChatHistory and FSIMChatHistoryFade
        cx.toggle(
            tr!("chat-extras-im-in-nearby"),
            tr!("chat-extras-im-in-nearby-hint"),
            &field!(chat_display.im_in_nearby_chat),
        );
        let off = !cx.s.chat_display.im_in_nearby_chat;
        cx.subs(off, |cx| {
            cx.slider(tr!("chat-extras-im-fade"), "", &field!(chat_display.im_nearby_fade), 0.25..=1.0, "");
        });
        // FSLogImToChatConsole, FSLogGroupImToChatConsole
        cx.toggle(
            tr!("chat-extras-im-in-console"),
            tr!("chat-extras-im-in-console-hint"),
            &field!(chat_display.im_in_console),
        );
        cx.toggle(
            tr!("chat-extras-group-im-in-console"),
            tr!("chat-extras-im-in-console-hint"),
            &field!(chat_display.group_im_in_console),
        );
        // FSShowGroupNameLength (-1 to 63)
        cx.row(
            tr!("chat-extras-group-name-length"),
            tr!("chat-extras-group-name-length-hint"),
            &field!(chat_display.group_name_length),
            |ui, s| {
                ui.add(
                    egui::DragValue::new(&mut s.chat_display.group_name_length)
                        .custom_parser(aurora_i18n::parse_number)
                        .range(-1..=63)
                        .speed(0.2),
                )
                .changed()
            },
        );
        // FSGroupNoticesToIMLog
        cx.toggle(
            tr!("chat-extras-notices-in-chat"),
            tr!("chat-extras-notices-in-chat-hint"),
            &field!(chat_display.group_notices_in_chat),
        );
        // FSDisableLabeledChatLinks, FSDisableLabeledChatLinksNearbyChat
        cx.toggle(
            tr!("chat-extras-link-hint-im"),
            tr!("chat-extras-link-hint-hint"),
            &field!(chat_display.link_hint_im),
        );
        cx.toggle(
            tr!("chat-extras-link-hint-nearby"),
            tr!("chat-extras-link-hint-hint"),
            &field!(chat_display.link_hint_nearby),
        );
        // FSHighlightGroupMods, FSModNameStyle, FSModTextStyle
        cx.toggle(
            tr!("chat-extras-highlight-moderators"),
            tr!("chat-extras-highlight-moderators-hint"),
            &field!(chat_display.highlight_moderators),
        );
        let off = !cx.s.chat_display.highlight_moderators;
        let styles = style_choices();
        cx.subs(off, |cx| {
            cx.combo(
                tr!("chat-extras-moderator-name-style"),
                "",
                &field!(chat_display.moderator_name_style),
                &styles,
            );
            cx.combo(
                tr!("chat-extras-moderator-text-style"),
                "",
                &field!(chat_display.moderator_text_style),
                &styles,
            );
        });
        // FSIMTabNameFormat
        cx.combo(
            tr!("chat-extras-tab-name-format"),
            "",
            &field!(chat_display.tab_name_format),
            &[
                (TabNameFormat::DisplayName, tr!("chat-extras-name-display")),
                (TabNameFormat::UserName, tr!("chat-extras-name-user")),
                (TabNameFormat::DisplayThenUser, tr!("chat-extras-name-display-user")),
                (TabNameFormat::UserThenDisplay, tr!("chat-extras-name-user-display")),
            ],
        );
        // FSIMSystemMessageBrackets, FSChatHistoryShowYou, FSMarkObjects
        cx.toggle(
            tr!("chat-extras-system-brackets"),
            tr!("chat-extras-system-brackets-hint"),
            &field!(chat_display.system_brackets),
        );
        cx.toggle(tr!("chat-extras-show-you"), "", &field!(chat_display.show_you));
        cx.toggle(tr!("chat-extras-mark-objects"), "", &field!(chat_display.mark_unnamed_objects));
        // FSShowIMSendButton
        cx.toggle(tr!("chat-extras-im-send-button"), "", &field!(chat_display.show_im_send_button));
    });
}

/// The eight styles of FSModNameStyle / FSModTextStyle.
fn style_choices() -> [(TextStyle, &'static str); 8] {
    use aurora_i18n::tr;
    let labels = [
        tr!("chat-extras-style-normal"),
        tr!("chat-extras-style-bold"),
        tr!("chat-extras-style-italic"),
        tr!("chat-extras-style-bold-italic"),
        tr!("chat-extras-style-underline"),
        tr!("chat-extras-style-bold-underline"),
        tr!("chat-extras-style-italic-underline"),
        tr!("chat-extras-style-all"),
    ];
    let styles = TextStyle::all();
    std::array::from_fn(|i| (styles[i], labels[i]))
}

/// A short text field for an OOC marker.
fn marker(ui: &mut egui::Ui, text: &mut String) -> bool {
    ui.add(
        egui::TextEdit::singleline(text)
            .char_limit(MARKER_CHARS)
            .desired_width(COMBO_W * 0.4),
    )
    .changed()
}
