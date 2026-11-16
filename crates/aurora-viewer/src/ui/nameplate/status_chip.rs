//! The status chip beside a full plate: what the resident listens to, uses
//! or wrote (`crate::status_share`). Collapsed it is a round chip the size
//! of a badge, with the icon of its category; while the plate is under the
//! pointer it slides out to a pill that reads the line. It is not part of
//! the plate's own size (the plate's layout and the tests of its size stay
//! as they are), and only a full plate has one: a compact or far plate, at
//! the crowd's limits, never adds work for it.
//!
//! This is geometry only; `paint::status` draws it.

use super::Metrics;
use egui::{Pos2, Rect, Vec2};

/// Mockup pixels between the plate's right edge and the chip: clear of the
/// voice ring's room.
pub const GAP: f32 = 10.0;
/// Mockup pixels after the text, inside the pill.
pub const PAD: f32 = 10.0;

/// The chip of a full plate `plate`, `hover` (0..1) of the way out, for a
/// line `text_w` points wide: a circle at 0, the pill at 1. Vertically
/// centred on the plate.
pub fn rect(plate: Rect, hover: f32, text_w: f32, m: &Metrics) -> Rect {
    let h = m.badge;
    let open = h + text_w + PAD * m.scale;
    let width = h + (open - h) * hover.clamp(0.0, 1.0);
    Rect::from_min_size(
        Pos2::new(plate.right() + GAP * m.scale, plate.center().y - h * 0.5),
        Vec2::new(width, h),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::nameplate::{Level, SCALE};

    #[test]
    fn the_chip_is_a_round_badge_that_opens_to_the_right() {
        let m = Metrics::of(Level::Full, SCALE);
        let plate = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(180.0, 66.0 * SCALE));
        let closed = rect(plate, 0.0, 120.0, &m);
        assert_eq!(closed.size(), Vec2::splat(m.badge));
        assert!(closed.left() > plate.right(), "clear of the plate");
        assert!((closed.center().y - plate.center().y).abs() < 1e-3);
        let open = rect(plate, 1.0, 120.0, &m);
        assert_eq!(
            (open.left(), open.height()),
            (closed.left(), closed.height()),
            "it grows from its left edge"
        );
        assert!(open.width() > m.badge + 120.0);
        let half = rect(plate, 0.5, 120.0, &m);
        assert!(half.width() > closed.width() && half.width() < open.width());
        assert_eq!(rect(plate, 7.0, 120.0, &m), open, "hover is bounded");
    }
}
