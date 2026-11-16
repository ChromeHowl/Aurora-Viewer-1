//! The interface of avatar marks that the radar and mini-map menus
//! (`ui/context.rs::mark_submenu`) do not draw: the ring of a marked
//! resident's picture in the People lists and the custom colour window.
//! The Legacy Viewer offers its five colours only and shows the mark on the
//! maps and the name tags, not in the radar list: the custom colour and the
//! ring are Aurora additions (ported from the upstream "radar marks"
//! change onto the fork's `world::radar::Marks`).

use super::widgets::{self, Floater};
use crate::theme::Palette;
use crate::world::World;
use crate::world::radar::MARK_COLORS;
use egui::{Color32, Rect, RichText, Stroke, Vec2};
use uuid::Uuid;

type Rgb = [u8; 3];

const WINDOW: &str = "avatar_mark_color";
/// Width of the ring around a marked picture.
const RING: f32 = 2.0;

/// A mark's colour as the lists draw it.
fn color(rgb: Rgb) -> Color32 {
    let [r, g, b] = rgb;
    // the colour the user marked a resident with: content, not theme
    Color32::from_rgb(r, g, b)
}

/// The ring of a marked resident around their picture in a People list:
/// `round` for Aurora's discs, the theme's corners for the Legacy table's
/// squares. Nothing for a resident without a mark, nor for one whose name
/// RLVa hides (`World::rlva_shown_mark`).
pub fn ring(ui: &egui::Ui, world: &World, id: &Uuid, picture: Rect, round: bool) {
    let Some(rgb) = world.rlva_shown_mark(id) else {
        return;
    };
    let stroke = Stroke::new(RING, color(rgb));
    if round {
        ui.painter()
            .circle_stroke(picture.center(), picture.width() / 2.0 + RING / 2.0, stroke);
    } else {
        ui.painter().rect_stroke(
            picture.expand(RING / 2.0),
            ui.visuals().window_corner_radius,
            stroke,
            egui::StrokeKind::Middle,
        );
    }
}

/// The custom colour window: the interface's compact picker
/// (`widgets::color::color_panel`), whose colour marks its residents as it
/// changes.
pub struct CustomMark {
    ids: Vec<Uuid>,
    color: widgets::color::Rgba,
    state: widgets::color::ColorPickState,
    /// Opened from a menu: comes to the front once shown.
    raise: bool,
}

impl CustomMark {
    /// Starts from the colour the residents share, else the last custom
    /// colour of the session, else the menu's first.
    pub fn new(world: &World, ids: Vec<Uuid>, last: Option<Rgb>) -> Self {
        let [r, g, b] = world.avatar_marks.shared(&ids).or(last).unwrap_or(MARK_COLORS[0]);
        let color = [r, g, b, 255];
        CustomMark {
            ids,
            color,
            state: widgets::color::ColorPickState::new(egui::Id::unique(WINDOW), color),
            raise: true,
        }
    }

    /// The colour shown now, kept for the next opening.
    pub fn rgb(&self) -> Rgb {
        [self.color[0], self.color[1], self.color[2]]
    }
}

/// What the window's line under the title names: who its colour marks.
fn target(world: &World, ids: &[Uuid]) -> String {
    match ids {
        [one] if world.rlva_can_name(*one) => world.person_name(one).unwrap_or_else(|| "…".into()),
        many => aurora_i18n::tr!("ui-avatar-mark-many", count = many.len()).to_owned(),
    }
}

/// Draws the custom colour window; `picker` becomes `None` once it closed.
pub fn show(ctx: &egui::Context, p: &Palette, world: &mut World, picker: &mut Option<CustomMark>) {
    let Some(mark) = picker else {
        return;
    };
    let size = Vec2::new(280.0, 220.0);
    let mut open = true;
    let who = target(world, &mark.ids);
    Floater::new(
        WINDOW,
        aurora_i18n::tr!("ui-avatar-mark-title"),
        ctx.content_rect().center() - size / 2.0,
        size,
    )
    .auto_height()
    .show(ctx, p, &mut open, |ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(6.0);
        ui.label(RichText::new(who).strong().color(p.ink));
        if widgets::color::color_panel(ui, p, &mut mark.state, &mut mark.color, false) {
            let rgb = mark.rgb();
            world.avatar_marks.set(&mark.ids, Some(rgb));
        }
        ui.label(RichText::new(aurora_i18n::tr!("ui-avatar-mark-private")).size(11.0).color(p.muted));
    });
    if std::mem::take(&mut mark.raise) {
        widgets::bring_floater_front(ctx, WINDOW);
    }
    if !open {
        *picker = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()))
    }

    #[test]
    fn the_custom_window_starts_from_the_shared_mark_then_the_last_colour() {
        let mut w = world();
        let (a, b) = (Uuid::from_u128(11), Uuid::from_u128(12));
        assert_eq!(CustomMark::new(&w, vec![a], None).rgb(), MARK_COLORS[0]);
        assert_eq!(CustomMark::new(&w, vec![a], Some([1, 2, 3])).rgb(), [1, 2, 3]);
        w.avatar_marks.set(&[a, b], Some([9, 8, 7]));
        assert_eq!(CustomMark::new(&w, vec![a, b], Some([1, 2, 3])).rgb(), [9, 8, 7]);
        w.avatar_marks.set(&[b], Some([1, 1, 1]));
        assert_eq!(CustomMark::new(&w, vec![a, b], Some([4, 5, 6])).rgb(), [4, 5, 6]);
    }

    #[test]
    fn the_window_names_one_resident_or_counts_several() {
        let w = world();
        let (a, b) = (Uuid::from_u128(11), Uuid::from_u128(12));
        assert!(target(&w, &[a, b]).contains('2'));
        assert!(!target(&w, &[a]).is_empty());
    }
}
