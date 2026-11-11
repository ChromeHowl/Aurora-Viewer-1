//! Preferences › Layout › Text: the rows of `settings/text_tuning.rs`
//! (the Legacy Viewer's panel_preferences_UI.xml: font_line_spacing_slider,
//! chat_line_spacing_slider, FSNotecardFontName, FSNotecardFontSize and
//! FSInventoryThumbnailTooltipsDelay).

use super::super::cx::{Cx, field};
use crate::settings::text_tuning::{CHAT_LINE_SPACING, MAX_THUMBNAIL_TOOLTIP_DELAY, NotecardFont, NotecardSize, UI_LINE_SPACING};

pub fn rows(cx: &mut Cx) {
    use aurora_i18n::tr;
    cx.group(tr!("l4-text-group"), |cx| {
        cx.slider_whole(
            tr!("l4-ui-line-spacing"),
            tr!("l4-ui-line-spacing-hint"),
            &field!(text_tuning.ui_line_spacing),
            UI_LINE_SPACING,
            " px",
        );
        cx.slider_whole(
            tr!("l4-chat-line-spacing"),
            tr!("l4-chat-line-spacing-hint"),
            &field!(text_tuning.chat_line_spacing),
            CHAT_LINE_SPACING,
            " px",
        );
        cx.choice(
            tr!("l4-notecard-font"),
            tr!("l4-notecard-font-hint"),
            &field!(text_tuning.notecard_font),
            &[
                (NotecardFont::SansSerif, tr!("l4-notecard-font-sans"), true),
                (NotecardFont::Monospace, tr!("l4-notecard-font-mono"), true),
            ],
        );
        cx.choice(
            tr!("l4-notecard-size"),
            "",
            &field!(text_tuning.notecard_size),
            &[
                (NotecardSize::Small, tr!("l4-size-small"), true),
                (NotecardSize::Medium, tr!("l4-size-medium"), true),
                (NotecardSize::Large, tr!("l4-size-large"), true),
            ],
        );
        cx.slider(
            tr!("l4-thumbnail-delay"),
            tr!("l4-thumbnail-delay-hint"),
            &field!(text_tuning.thumbnail_tooltip_delay),
            0.0..=MAX_THUMBNAIL_TOOLTIP_DELAY,
            " s",
        );
    });
}
