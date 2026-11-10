//! The photo window's further settings, kept between sessions beside its
//! others (`Settings::extras`): the look of the composition guide, what
//! follows a save or an upload, the matte under a cut-out photo saved
//! without alpha, and the 360° photo's quality.
//!
//! Legacy Viewer settings: FSSnapshotFrameGuideColor, FSSnapshotFrameGuide-
//! Width, FSSnapshotGuideVisibility and FSSnapshotGuideStyle (the corner of
//! the golden spiral, floater_snapshot_guide_settings.xml); FSLogSnapshots-
//! ToLocal (off) and FSOpenInventoryAfterSnapshot (on); 360QualitySelection
//! and 360CaptureHideAvatars (floater_360capture.xml).

use serde::{Deserialize, Serialize};

/// The thinnest and thickest guide line, in points (the Legacy Viewer's
/// spinner: 0 to 20).
pub const GUIDE_WIDTH: std::ops::RangeInclusive<f32> = 0.5..=20.0;

/// The corner the golden spiral winds into (FSSnapshotGuideStyle's
/// golden_ratio_top_left and the others).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Corner {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight];

    /// The spiral is mirrored left to right, top to bottom.
    pub fn flips(self) -> (bool, bool) {
        match self {
            Corner::TopLeft => (true, false),
            Corner::TopRight => (false, false),
            Corner::BottomLeft => (true, true),
            Corner::BottomRight => (false, true),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Extras {
    /// The guide's colour and opacity (sRGB, straight alpha); None: the
    /// theme's.
    pub guide_color: Option<[u8; 4]>,
    /// The guide's lines, in points.
    pub guide_width: f32,
    /// The golden spiral's corner.
    pub guide_corner: Corner,
    /// "Log filename of saved snapshots in to chat history".
    pub log_to_chat: bool,
    /// "Automatically show snapshots in inventory after upload".
    pub open_inventory: bool,
    /// The colour a cut-out photo is laid on in a file or an upload with
    /// no alpha (sRGB).
    pub matte: [u8; 3],
    /// The 360° photo's quality.
    pub pano_quality: super::pano::Quality,
    /// The avatars, what they wear and their particles and lights left out
    /// of the 360° photo.
    pub pano_hide_avatars: bool,
}

impl Default for Extras {
    fn default() -> Self {
        Extras {
            guide_color: None,
            guide_width: 1.0,
            guide_corner: Corner::TopLeft,
            log_to_chat: false,
            open_inventory: true,
            matte: [255, 255, 255],
            pano_quality: Default::default(),
            pano_hide_avatars: false,
        }
    }
}

impl Extras {
    /// Bring loaded values back into their ranges.
    pub fn sanitize(&mut self) {
        self.guide_width = if self.guide_width.is_finite() {
            self.guide_width.clamp(*GUIDE_WIDTH.start(), *GUIDE_WIDTH.end())
        } else {
            1.0
        };
    }
}

/// A cut-out photo laid on `matte` (sRGB) for a file or an upload with no
/// alpha: each colour mixed with the matte by its alpha, the alpha made
/// opaque. The colours are straight (not premultiplied), as the photo
/// reduces them (`quality::reduce_into`).
pub fn lay_on_matte(rgba: &mut [u8], matte: [u8; 3]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        for c in 0..3 {
            px[c] = ((u32::from(px[c]) * a + u32::from(matte[c]) * (255 - a) + 127) / 255) as u8;
        }
        px[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_out_photo_is_laid_on_its_matte() {
        let mut px = vec![200, 100, 50, 255, 9, 9, 9, 0, 0, 0, 0, 128, 255, 255, 255, 64];
        lay_on_matte(&mut px, [10, 20, 30]);
        assert_eq!(&px[0..4], &[200, 100, 50, 255], "opaque: kept");
        assert_eq!(&px[4..8], &[10, 20, 30, 255], "empty: the matte");
        assert_eq!(&px[8..12], &[5, 10, 15, 255], "half: between the two");
        assert_eq!(&px[12..16], &[71, 79, 86, 255]);
    }

    #[test]
    fn old_settings_load_and_values_come_back_in_range() {
        let s: super::super::Settings = serde_json::from_str(r#"{"jpeg_quality": 80}"#).expect("load");
        assert_eq!(s.extras, Extras::default());
        assert!(s.extras.open_inventory && !s.extras.log_to_chat, "the Legacy Viewer's defaults");
        let mut e: Extras = serde_json::from_str(r#"{"guide_width": 99.0, "guide_corner": "BottomRight"}"#).expect("load");
        e.sanitize();
        assert_eq!(e.guide_corner, Corner::BottomRight);
        assert_eq!(e.guide_width, *GUIDE_WIDTH.end());
        let mut e = Extras {
            guide_width: f32::NAN,
            ..Extras::default()
        };
        e.sanitize();
        assert_eq!(e.guide_width, 1.0);
    }
}
