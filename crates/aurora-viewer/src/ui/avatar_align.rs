//! "Avatar alignment": a compass that turns our avatar to a cardinal or
//! intercardinal direction, by steps of 1 to 90°, or to face the nearest
//! avatar (Me › Movement, the toolbar's avatar_align and face_nearest_avatar).
//!
//! Port of FSAvatarAlignBase and FSFloaterAvatarAlign
//! (indra/newview/fsfloateravataralign.cpp, floater_avatar_align.xml,
//! originally LGPL 2.1): a click in the ring turns to the nearest 45°
//! octant, a click in its centre (a quarter of the radius) faces the nearest
//! avatar within 20 m, the needle shows where we face and the bearing is
//! written under it, north up and clockwise.
//!
//! Differences: one window that keeps its size instead of the Legacy
//! Viewer's full and mini floaters; nothing turns while we sit (the seat
//! holds the avatar) and the nearest avatar is not looked for while RLVa
//! hides the nearby residents (@shownearby).
use super::widgets::{self, Floater};
use crate::theme::Palette;
use egui::{Pos2, Sense, Stroke, Vec2};
use glam::Vec3;
use uuid::Uuid;

/// The window's id (Avatar.AlignToggle "avatar_align").
pub const WINDOW: &str = "avatar_align";
/// MAX_FACE_DISTANCE (fsfloateravataralign.h), in any direction (m).
pub const FACE_DISTANCE: f32 = 20.0;
/// The ring's steps (floater_avatar_align.xml), negative to the left.
const FINE: [f32; 4] = [-10.0, -1.0, 1.0, 10.0];
const COARSE: [f32; 4] = [-90.0, -45.0, 45.0, 90.0];
const COMPASS: f32 = 200.0;

/// What the window asks of the app.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Face this compass bearing (degrees, north 0, east 90).
    Bearing(f32),
    /// Turn by this many degrees, positive to the right.
    Rotate(f32),
    /// Face the nearest 45° direction (onClickNearest).
    NearestCardinal,
    /// Face the nearest avatar (onClickFaceNearestAvatar).
    FaceNearest,
}

/// Why the avatar does not turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    Sitting,
    /// RLVa hides the nearby residents: the nearest one is not looked for.
    Rlva,
}

impl Blocked {
    pub fn message(self) -> &'static str {
        match self {
            Blocked::Sitting => aurora_i18n::tr!("b4-align-sitting"),
            Blocked::Rlva => aurora_i18n::tr!("b4-align-rlva"),
        }
    }
}

/// Whether `action` may turn us: never seated (on an object or the ground),
/// and the nearest avatar not under @shownearby.
pub fn blocked(action: Action, sitting: bool, shownearby: bool) -> Option<Blocked> {
    if sitting {
        Some(Blocked::Sitting)
    } else if action == Action::FaceNearest && shownearby {
        Some(Blocked::Rlva)
    } else {
        None
    }
}

/// The compass bearing (degrees in [0, 360), north 0, east 90) of the
/// agent's yaw (radians from east, counterclockwise).
pub fn bearing(yaw: f32) -> f32 {
    (90.0 - yaw.to_degrees()).rem_euclid(360.0)
}

/// The yaw (radians) that faces a compass bearing.
pub fn yaw(bearing: f32) -> f32 {
    (90.0 - bearing).to_radians()
}

/// The bearing after a turn of `delta` degrees, positive to the right
/// (onClickRotate).
pub fn rotated(bearing: f32, delta: f32) -> f32 {
    (bearing + delta).rem_euclid(360.0)
}

/// The nearest multiple of 45° (onClickNearest, ll_round).
pub fn nearest_cardinal(bearing: f32) -> f32 {
    ((bearing / 45.0 + 0.5).floor() * 45.0).rem_euclid(360.0)
}

/// The bearing toward `target` on the ground plane, None straight above.
pub fn bearing_to(from: Vec3, target: Vec3) -> Option<f32> {
    let d = (target - from).truncate();
    (d.length_squared() > 1e-6).then(|| d.x.atan2(d.y).to_degrees().rem_euclid(360.0))
}

/// The nearest avatar within FACE_DISTANCE of `me`, in 3D like the Legacy
/// Viewer (our own avatar is not in `others`).
pub fn nearest(me: Vec3, others: impl IntoIterator<Item = (Uuid, Vec3)>) -> Option<(Uuid, Vec3)> {
    others
        .into_iter()
        .filter(|(_, p)| p.is_finite())
        .map(|(id, p)| (id, p, p.distance_squared(me)))
        .filter(|(_, _, d)| *d <= FACE_DISTANCE * FACE_DISTANCE)
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(id, p, _)| (id, p))
}

/// Where a click lands on the compass, as the offset from its centre with
/// y up: the centre (a quarter of the radius), an octant's bearing, or out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zone {
    Center,
    Octant(f32),
}

pub fn zone(offset: Vec2, radius: f32) -> Option<Zone> {
    let dist = offset.length();
    if dist > radius {
        None
    } else if dist < radius * 0.25 {
        Some(Zone::Center)
    } else {
        Some(Zone::Octant(nearest_cardinal(offset.x.atan2(offset.y).to_degrees())))
    }
}

#[derive(Default)]
pub struct AvatarAlignUi;

impl AvatarAlignUi {
    /// `facing`: our bearing, None before our avatar is known.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        facing: Option<f32>,
        blocked: Option<Blocked>,
        open: &mut bool,
    ) -> Option<Action> {
        let mut action = None;
        Floater::new(
            WINDOW,
            aurora_i18n::tr!("toolbar-command-avatar-align"),
            egui::pos2(80.0, 140.0),
            egui::vec2(250.0, 390.0),
        )
        .fixed()
        .show(ctx, p, open, |ui| {
            let enabled = facing.is_some() && blocked != Some(Blocked::Sitting);
            ui.add_enabled_ui(enabled, |ui| {
                action = compass(ui, p, facing.unwrap_or(0.0));
                for row in [FINE, COARSE] {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 3.0 * ui.spacing().item_spacing.x) / 4.0;
                        for delta in row {
                            let label = format!("{delta:+}°");
                            let tip = if delta < 0.0 {
                                aurora_i18n::tr!("b4-align-left", degrees = delta.abs())
                            } else {
                                aurora_i18n::tr!("b4-align-right", degrees = delta)
                            };
                            if widgets::flat_button_sized(ui, p, &label, egui::vec2(width, 26.0))
                                .on_hover_text(tip)
                                .clicked()
                            {
                                action = Some(Action::Rotate(delta));
                            }
                        }
                    });
                }
                let size = egui::vec2(ui.available_width(), 28.0);
                if widgets::action_button(
                    ui,
                    p,
                    "compass-filled",
                    aurora_i18n::tr!("b4-align-nearest-cardinal"),
                    widgets::ActionKind::Flat,
                    size,
                )
                .on_hover_text(aurora_i18n::tr!("b4-align-nearest-cardinal-tip"))
                .clicked()
                {
                    action = Some(Action::NearestCardinal);
                }
                if widgets::action_button(
                    ui,
                    p,
                    "user-search",
                    aurora_i18n::tr!("toolbar-command-face-nearest"),
                    widgets::ActionKind::Flat,
                    size,
                )
                .on_hover_text(aurora_i18n::tr!("b4-align-face-tip"))
                .clicked()
                {
                    action = Some(Action::FaceNearest);
                }
            });
            if let Some(blocked) = blocked {
                ui.label(egui::RichText::new(blocked.message()).color(p.muted));
            }
        });
        action
    }
}

/// The compass (FSAvatarAlignBase::drawCompass): ring, hovered zone, the
/// four cardinal arms (north in the danger colour) and the intercardinal
/// ones, the needle of our facing and the bearing under it.
fn compass(ui: &mut egui::Ui, p: &Palette, facing: f32) -> Option<Action> {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), COMPASS + 24.0), Sense::click());
    let painter = ui.painter_at(rect);
    let r = (COMPASS * 0.5 - 18.0).max(30.0);
    let c = Pos2::new(rect.center().x, rect.top() + COMPASS * 0.5);
    // north up, clockwise: a bearing to a point on screen
    let at = |bearing: f32, radius: f32| {
        let a = bearing.to_radians();
        c + Vec2::new(a.sin(), -a.cos()) * radius
    };
    let to_zone = |pos: Pos2| zone(Vec2::new(pos.x - c.x, c.y - pos.y), r);
    painter.circle_filled(c, r, p.field);
    painter.circle_stroke(c, r, Stroke::new(1.0, p.muted));
    painter.circle_stroke(c, r * 0.5, Stroke::new(1.0, p.muted_dim));
    let hovered = response.hover_pos().and_then(to_zone);
    match hovered {
        Some(Zone::Center) => {
            painter.circle_filled(c, r * 0.25, p.violet.gamma_multiply(0.3));
        }
        Some(Zone::Octant(b)) => {
            // the slice between the centre zone and the rim, in eight
            // convex strips (the triangle fan of the Legacy Viewer)
            for i in 0..8 {
                let a0 = b - 22.5 + i as f32 * 45.0 / 8.0;
                let a1 = a0 + 45.0 / 8.0;
                let strip = vec![at(a0, r), at(a1, r), at(a1, r * 0.25), at(a0, r * 0.25)];
                painter.add(egui::Shape::convex_polygon(strip, p.violet.gamma_multiply(0.25), Stroke::NONE));
            }
        }
        None => {}
    }
    let kite = |bearing: f32, tip: f32, side: f32, color: egui::Color32| {
        let left = at(bearing - 90.0, side);
        let right = at(bearing + 90.0, side);
        painter.add(egui::Shape::convex_polygon(
            vec![at(bearing, tip), right, c, left],
            color,
            Stroke::NONE,
        ));
    };
    for b in [45.0, 135.0, 225.0, 315.0] {
        kite(b, r * 0.62, r * 0.10, p.muted_dim);
    }
    for (b, color) in [(0.0, p.danger), (180.0, p.ink), (90.0, p.muted), (270.0, p.muted)] {
        kite(b, r - 4.0, r * 0.245 * 0.5, color);
    }
    // the needle of our facing
    painter.line_segment([c, at(facing, (r - 4.0) * 0.88)], Stroke::new(3.0, p.amber));
    painter.circle_filled(c, 4.0, p.raised);
    let font = egui::FontId::proportional(13.0);
    for (b, key, color) in [
        (0.0, "b4-align-n", p.danger),
        (90.0, "b4-align-e", p.ink),
        (180.0, "b4-align-s", p.ink),
        (270.0, "b4-align-w", p.ink),
    ] {
        painter.text(
            at(b, r + 10.0),
            egui::Align2::CENTER_CENTER,
            aurora_i18n::text(key),
            font.clone(),
            color,
        );
    }
    painter.text(
        Pos2::new(c.x, rect.bottom() - 8.0),
        egui::Align2::CENTER_CENTER,
        format!("{:03.0}°", facing.round().rem_euclid(360.0)),
        font,
        p.ink,
    );
    let zone = response
        .clicked()
        .then(|| response.interact_pointer_pos().and_then(to_zone))
        .flatten();
    match zone {
        Some(Zone::Center) => Some(Action::FaceNearest),
        Some(Zone::Octant(b)) => Some(Action::Bearing(b)),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearings_are_north_up_and_clockwise() {
        // yaw 0 faces east (+X), pi/2 north (+Y)
        assert!((bearing(0.0) - 90.0).abs() < 1e-4);
        assert!(bearing(std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert!((bearing(std::f32::consts::PI) - 270.0).abs() < 1e-3);
        for b in [0.0, 45.0, 90.0, 200.0, 359.0] {
            assert!((bearing(yaw(b)) - b).abs() < 1e-3, "{b}");
        }
        // a turn to the left lowers the bearing
        assert_eq!(rotated(5.0, -10.0), 355.0);
        assert_eq!(rotated(350.0, 45.0), 35.0);
    }

    #[test]
    fn the_nearest_cardinal_rounds_to_45_degrees() {
        assert_eq!(nearest_cardinal(22.0), 0.0);
        assert_eq!(nearest_cardinal(23.0), 45.0);
        assert_eq!(nearest_cardinal(350.0), 0.0);
        assert_eq!(nearest_cardinal(181.0), 180.0);
        assert_eq!(nearest_cardinal(-10.0), 0.0);
    }

    #[test]
    fn compass_clicks_choose_the_centre_or_an_octant() {
        let r = 80.0;
        assert_eq!(zone(Vec2::new(5.0, 5.0), r), Some(Zone::Center));
        assert_eq!(zone(Vec2::new(0.0, 60.0), r), Some(Zone::Octant(0.0)));
        assert_eq!(zone(Vec2::new(60.0, 0.0), r), Some(Zone::Octant(90.0)));
        assert_eq!(zone(Vec2::new(40.0, -40.0), r), Some(Zone::Octant(135.0)));
        assert_eq!(zone(Vec2::new(-60.0, 0.0), r), Some(Zone::Octant(270.0)));
        assert_eq!(zone(Vec2::new(90.0, 0.0), r), None);
    }

    #[test]
    fn the_nearest_avatar_is_the_closest_within_20_metres() {
        let me = Vec3::new(100.0, 100.0, 20.0);
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let far = Uuid::from_u128(3);
        let others = [
            (a, me + Vec3::new(10.0, 0.0, 0.0)),
            (b, me + Vec3::new(0.0, -6.0, 3.0)),
            (far, me + Vec3::new(0.0, 0.0, 25.0)),
        ];
        assert_eq!(nearest(me, others).map(|n| n.0), Some(b));
        assert_eq!(nearest(me, [(far, others[2].1)]), None);
        assert_eq!(nearest(me, [(a, Vec3::NAN)]), None);
        // facing it: south
        let (_, at) = nearest(me, others).expect("nearest");
        assert!((bearing_to(me, at).expect("bearing") - 180.0).abs() < 1e-3);
        assert_eq!(bearing_to(me, me + Vec3::Z), None);
    }

    #[test]
    fn nothing_turns_seated_and_rlva_hides_the_nearest_avatar() {
        assert_eq!(blocked(Action::Bearing(90.0), true, false), Some(Blocked::Sitting));
        assert_eq!(blocked(Action::FaceNearest, false, true), Some(Blocked::Rlva));
        assert_eq!(blocked(Action::Rotate(10.0), false, true), None);
        assert_eq!(blocked(Action::NearestCardinal, false, false), None);
    }
}
