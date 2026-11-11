//! Text preferences of the Legacy Viewer's User Interface page
//! (panel_preferences_UI.xml): the line spacing of the interface's text
//! and of the chat (FSFontLineSpacingAdjustment, FSFontChatLineSpacingPixels),
//! the notecard editor's font (FSNotecardFontName, FSNotecardFontSize) and
//! the delay of the inventory's picture tooltips
//! (FSInventoryThumbnailTooltipsDelay). Applied by `ui/text_tuning.rs`.
//! The in-world hover tips' delay (AvatarInspectorTooltipDelay and
//! ObjectInspectorTooltipDelay) is `HoverTips::delay`.

use serde::{Deserialize, Serialize};

/// The interface's extra line spacing offered, pixels. The Legacy Viewer
/// goes down to -10, which would overlap the rows of egui's text: from 0.
pub const UI_LINE_SPACING: std::ops::RangeInclusive<f32> = 0.0..=10.0;
/// The chat's extra line spacing offered (chat_line_spacing_slider).
pub const CHAT_LINE_SPACING: std::ops::RangeInclusive<f32> = 0.0..=36.0;
/// FSInventoryThumbnailTooltipsDelay's default.
pub const THUMBNAIL_TOOLTIP_DELAY: f32 = 0.7;
/// The longest picture tooltip delay offered.
pub const MAX_THUMBNAIL_TOOLTIP_DELAY: f32 = 5.0;

/// FSNotecardFontName. The Legacy Viewer's "Scripting" and "Cascadia
/// Code" are monospaced faces Aurora does not ship: its monospace serves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotecardFont {
    #[default]
    SansSerif,
    Monospace,
}

impl NotecardFont {
    pub fn family(self) -> egui::FontFamily {
        match self {
            NotecardFont::SansSerif => egui::FontFamily::Proportional,
            NotecardFont::Monospace => egui::FontFamily::Monospace,
        }
    }
}

/// FSNotecardFontSize.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotecardSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl NotecardSize {
    /// The size against the editor's usual one: fonts.xml's Small, Medium
    /// and Large are 9, 10 and 11 points.
    pub fn scale(self) -> f32 {
        match self {
            NotecardSize::Small => 0.9,
            NotecardSize::Medium => 1.0,
            NotecardSize::Large => 1.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextTuning {
    /// FSFontLineSpacingAdjustment (0): pixels added between the lines of
    /// the interface's text.
    pub ui_line_spacing: f32,
    /// FSFontChatLineSpacingPixels (0): pixels added between the lines of
    /// the chat.
    pub chat_line_spacing: f32,
    /// FSNotecardFontName (Sans Serif).
    pub notecard_font: NotecardFont,
    /// FSNotecardFontSize (Medium).
    pub notecard_size: NotecardSize,
    /// FSInventoryThumbnailTooltipsDelay (0.7 s).
    pub thumbnail_tooltip_delay: f32,
}

impl Default for TextTuning {
    fn default() -> Self {
        TextTuning {
            ui_line_spacing: 0.0,
            chat_line_spacing: 0.0,
            notecard_font: NotecardFont::default(),
            notecard_size: NotecardSize::default(),
            thumbnail_tooltip_delay: THUMBNAIL_TOOLTIP_DELAY,
        }
    }
}

fn bounded(value: f32, range: std::ops::RangeInclusive<f32>, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(*range.start(), *range.end())
    } else {
        default
    }
}

impl TextTuning {
    /// Bounds a file's values.
    pub fn sanitize(&mut self) {
        self.ui_line_spacing = bounded(self.ui_line_spacing, UI_LINE_SPACING, 0.0).round();
        self.chat_line_spacing = bounded(self.chat_line_spacing, CHAT_LINE_SPACING, 0.0).round();
        self.thumbnail_tooltip_delay = bounded(
            self.thumbnail_tooltip_delay,
            0.0..=MAX_THUMBNAIL_TOOLTIP_DELAY,
            THUMBNAIL_TOOLTIP_DELAY,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_legacy_viewers() {
        let t = TextTuning::default();
        assert_eq!((t.ui_line_spacing, t.chat_line_spacing), (0.0, 0.0));
        assert_eq!((t.notecard_font, t.notecard_size), (NotecardFont::SansSerif, NotecardSize::Medium));
        assert_eq!(t.thumbnail_tooltip_delay, 0.7);
        let old: TextTuning = serde_json::from_str(r#"{"chat_line_spacing": 4.0}"#).expect("partial file");
        assert_eq!(old.chat_line_spacing, 4.0);
        assert_eq!(old.thumbnail_tooltip_delay, THUMBNAIL_TOOLTIP_DELAY);
    }

    #[test]
    fn sanitize_bounds_every_value() {
        let mut t = TextTuning {
            ui_line_spacing: -10.0,
            chat_line_spacing: 99.0,
            thumbnail_tooltip_delay: f32::NAN,
            ..TextTuning::default()
        };
        t.sanitize();
        assert_eq!((t.ui_line_spacing, t.chat_line_spacing), (0.0, 36.0));
        assert_eq!(t.thumbnail_tooltip_delay, THUMBNAIL_TOOLTIP_DELAY);
    }

    #[test]
    fn the_notecard_sizes_follow_fonts_xml() {
        assert_eq!(NotecardSize::Small.scale() * 10.0, 9.0);
        assert_eq!(NotecardSize::Medium.scale(), 1.0);
        assert_eq!(NotecardSize::Large.scale() * 10.0, 11.0);
        assert_eq!(NotecardFont::Monospace.family(), egui::FontFamily::Monospace);
    }
}
