//! Settings of the radar, the contact sets' notices, the name tags and the
//! floating texts (Preferences › Chat, Name tags and World), from the Legacy
//! Viewer's settings.xml: FSDisableAvatarTrackerAtCloseIn,
//! FSContactSetsNotificationToast, FSContactSetsNotificationNearbyChat,
//! FSTagShowDistance, FSTagShowDistanceColors, FSNameTagZOffsetCorrection,
//! FSShowAutorespondInNametag, FSHudTextFadeDistance, FSHudTextFadeRange,
//! FSHudTextUseHoverHighlight, FSHudTextShowBackground,
//! FSHudTextBackgroundOpacity, FSShowChatRangeSpheres,
//! FSMouselookCombatFeatures with ExodusMouselookIFF and
//! ExodusMouselookIFFRange; same defaults.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// FSNameTagZOffsetCorrection's bounds, in tenths of a meter.
pub const TAG_OFFSET_MAX: i32 = 5;
/// The floating texts' fade, in meters (the Legacy Viewer's spinners).
pub const TEXT_FADE_MAX: f32 = 128.0;
/// ExodusMouselookIFFRange's bounds, in meters.
pub const MARKER_RANGE_MAX: f32 = 512.0;

/// Which floating texts have a plate behind them (FSHudTextShowBackground).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum TextBackground {
    #[default]
    Off,
    /// Only the text of the object under the pointer (with the hover
    /// highlight).
    Highlighted,
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct People {
    /// The radar's tracking beacon goes away once within 3 m of the
    /// resident (FSDisableAvatarTrackerAtCloseIn).
    pub track_clear_close_in: bool,
    /// The distance to the resident on their name tag (FSTagShowDistance).
    pub tag_distance: bool,
    /// Name tags coloured by the chat range the resident is in
    /// (FSTagShowDistanceColors).
    pub tag_distance_colors: bool,
    /// Name tags raised or lowered, in tenths of a meter
    /// (FSNameTagZOffsetCorrection).
    pub tag_z_offset: i32,
    /// "Auto-Response" on our own name tag while it is on
    /// (FSShowAutorespondInNametag).
    pub tag_autorespond: bool,
    /// Distance at which floating texts start to fade (FSHudTextFadeDistance).
    pub text_fade_distance: f32,
    /// Distance over which they fade out (FSHudTextFadeRange).
    pub text_fade_range: f32,
    /// The text of the object under the pointer does not fade and stands in
    /// front of the scene (FSHudTextUseHoverHighlight).
    pub text_hover_highlight: bool,
    pub text_background: TextBackground,
    /// Opacity of that plate (FSHudTextBackgroundOpacity).
    pub text_background_opacity: f32,
    /// Whisper, say and shout spheres around our avatar (FSShowChatRangeSpheres).
    pub chat_range_spheres: bool,
    /// Markers on the residents in range in mouselook
    /// (FSMouselookCombatFeatures with ExodusMouselookIFF).
    pub mouselook_markers: bool,
    pub mouselook_marker_range: f32,
    /// In mouselook, the name and distance of the resident under the
    /// crosshair (FSMouselookCombatFeatures, on), within the markers' range.
    pub mouselook_name: bool,
    /// The accounts that saw the hidden friends' tip once.
    pub hidden_tip_shown: Vec<Uuid>,
}

impl Default for People {
    fn default() -> Self {
        People {
            track_clear_close_in: true,
            tag_distance: false,
            tag_distance_colors: false,
            tag_z_offset: 0,
            tag_autorespond: false,
            text_fade_distance: 8.0,
            text_fade_range: 4.0,
            text_hover_highlight: false,
            text_background: TextBackground::Off,
            text_background_opacity: 0.75,
            chat_range_spheres: false,
            mouselook_markers: false,
            mouselook_marker_range: 380.0,
            mouselook_name: true,
            hidden_tip_shown: Vec::new(),
        }
    }
}

impl People {
    /// Values of a hand-edited file brought back into their ranges.
    pub fn sanitize(&mut self) {
        let d = People::default();
        self.tag_z_offset = self.tag_z_offset.clamp(-TAG_OFFSET_MAX, TAG_OFFSET_MAX);
        let meters = |v: f32, default: f32, max: f32| if v.is_finite() { v.clamp(0.0, max) } else { default };
        self.text_fade_distance = meters(self.text_fade_distance, d.text_fade_distance, TEXT_FADE_MAX);
        self.text_fade_range = meters(self.text_fade_range, d.text_fade_range, TEXT_FADE_MAX);
        self.text_background_opacity = meters(self.text_background_opacity, d.text_background_opacity, 1.0);
        self.mouselook_marker_range = meters(self.mouselook_marker_range, d.mouselook_marker_range, MARKER_RANGE_MAX);
    }

    /// The name tag's height correction in meters.
    pub fn tag_offset_m(&self) -> f32 {
        self.tag_z_offset.clamp(-TAG_OFFSET_MAX, TAG_OFFSET_MAX) as f32 / 10.0
    }

    /// LLHUDText::renderText's fade of a floating text `distance` meters
    /// away: 1 up to the fade distance, then down to 0 over the range (no
    /// fade with a range of 0, FIRE-17393).
    pub fn text_alpha(&self, distance: f32) -> f32 {
        if distance > self.text_fade_distance && self.text_fade_range > 0.0 {
            (1.0 - (distance - self.text_fade_distance) / self.text_fade_range).max(0.0)
        } else {
            1.0
        }
    }

    /// Whether the hidden friends' tip was shown to this account, and
    /// marks it shown.
    pub fn take_hidden_tip(&mut self, account: Uuid) -> bool {
        if account.is_nil() || self.hidden_tip_shown.contains(&account) {
            return false;
        }
        self.hidden_tip_shown.push(account);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_legacy_viewer_s() {
        let d = People::default();
        assert!(d.track_clear_close_in);
        assert!(!d.tag_distance && !d.tag_distance_colors && d.tag_z_offset == 0 && !d.tag_autorespond);
        assert_eq!((d.text_fade_distance, d.text_fade_range), (8.0, 4.0));
        assert_eq!(d.text_background, TextBackground::Off);
        assert_eq!(d.mouselook_marker_range, 380.0);
    }

    #[test]
    fn an_old_settings_file_loads_with_the_defaults() {
        let p: People = serde_json::from_str("{}").expect("empty object");
        assert_eq!(p, People::default());
        let p: People = serde_json::from_str(r#"{"tag_distance": true}"#).expect("one field");
        assert!(p.tag_distance);
    }

    #[test]
    fn floating_texts_fade_from_the_distance_over_the_range() {
        let mut p = People::default();
        assert_eq!(p.text_alpha(5.0), 1.0);
        assert_eq!(p.text_alpha(8.0), 1.0);
        assert!((p.text_alpha(10.0) - 0.5).abs() < 1e-6);
        assert_eq!(p.text_alpha(12.0), 0.0);
        assert_eq!(p.text_alpha(40.0), 0.0);
        p.text_fade_range = 0.0;
        assert_eq!(p.text_alpha(40.0), 1.0);
    }

    #[test]
    fn hand_edited_values_come_back_into_range() {
        let mut p = People {
            tag_z_offset: 40,
            text_fade_distance: f32::NAN,
            text_background_opacity: 3.0,
            mouselook_marker_range: -1.0,
            ..People::default()
        };
        p.sanitize();
        assert_eq!(p.tag_z_offset, TAG_OFFSET_MAX);
        assert_eq!(p.text_fade_distance, 8.0);
        assert_eq!(p.text_background_opacity, 1.0);
        assert_eq!(p.mouselook_marker_range, 0.0);
        assert!(
            (People {
                tag_z_offset: -3,
                ..People::default()
            }
            .tag_offset_m()
                + 0.3)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn the_hidden_friends_tip_is_shown_once_per_account() {
        let mut p = People::default();
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        assert!(p.take_hidden_tip(a));
        assert!(!p.take_hidden_tip(a));
        assert!(p.take_hidden_tip(b));
        assert!(!p.take_hidden_tip(Uuid::nil()));
    }
}
