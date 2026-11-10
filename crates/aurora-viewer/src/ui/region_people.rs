//! "People in region" for the surfaces drawn from a snapshot of the world
//! (the bars: favourites, the arrows' list of places), which cannot call
//! `World::people_in_region` themselves. They note the region under the
//! pointer here and read the answer to the names noted in the previous
//! frame; the app answers once a frame with the same call as every other
//! surface, so the figure, its limits and RLVa @showloc are the same.

use crate::world::World;
use crate::world::region_people::People;
use std::collections::HashMap;

#[derive(Clone, Default)]
struct Board {
    /// Regions shown this frame (lowercase).
    wanted: Vec<String>,
    /// The answers to those of the previous frame.
    known: HashMap<String, People>,
}

fn id() -> egui::Id {
    egui::Id::unique("region_people_board")
}

/// The region is shown: its figure once the app answered, None before that,
/// without a name, or when it is hidden.
pub fn of(ctx: &egui::Context, region: &str) -> Option<People> {
    let key = region.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    ctx.data_mut(|d| {
        let board = d.get_temp_mut_or_default::<Board>(id());
        let known = board.known.get(&key).copied();
        if !board.wanted.contains(&key) {
            board.wanted.push(key);
        }
        known
    })
}

/// `text` with "People in region: N" under it, for a tooltip.
pub fn with_line(ctx: &egui::Context, region: &str, text: &str) -> String {
    match of(ctx, region) {
        Some(people) if text.is_empty() => people.label(),
        Some(people) => format!("{text}\n{}", people.label()),
        None => text.to_owned(),
    }
}

/// The compact figure of a dense row, in a right-to-left trailing area: the
/// number in the muted ink of the neighbouring figures, the people icon
/// before it, the full sentence as its tooltip.
pub fn figure(ui: &mut egui::Ui, p: &crate::theme::Palette, people: People) {
    ui.label(egui::RichText::new(people.figure()).size(12.0).color(p.muted))
        .on_hover_text(people.label());
    ui.add_space(-4.0);
    super::widgets::glyph(ui, "users", p.muted, 13.0);
}

/// Once a frame, before the bars are drawn: answer the regions they showed.
pub fn serve(ctx: &egui::Context, world: &World) {
    let wanted = ctx.data_mut(|d| std::mem::take(&mut d.get_temp_mut_or_default::<Board>(id()).wanted));
    let known: HashMap<_, _> = wanted
        .into_iter()
        .filter_map(|name| world.people_in_region(&name).map(|people| (name, people)))
        .collect();
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Board>(id()).known = known);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::worldmap::{SIM_ACCESS_PG, SimInfo};

    #[test]
    fn a_region_noted_by_a_bar_is_answered_the_next_frame_and_dropped_when_no_longer_shown() {
        let ctx = egui::Context::default();
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        world.map.sims.insert(
            (10, 20),
            SimInfo {
                name: "Ahern".into(),
                access: SIM_ACCESS_PG,
                flags: 0,
                image: uuid::Uuid::nil(),
                size_x: 256,
                size_y: 256,
            },
        );
        world.map.agents.insert((10, 20), vec![(2560, 5120, 4)]);
        serve(&ctx, &world);
        assert_eq!(of(&ctx, " Ahern "), None);
        assert_eq!(of(&ctx, ""), None);
        serve(&ctx, &world);
        assert_eq!(of(&ctx, "ahern"), world.people_in_region("Ahern"));
        assert_eq!(of(&ctx, "Ahern"), Some(People::Count(4)));
        assert_eq!(with_line(&ctx, "Ahern", "Here"), format!("Here\n{}", People::Count(4).label()));
        // not shown for a frame: forgotten
        serve(&ctx, &world);
        serve(&ctx, &world);
        assert_eq!(of(&ctx, "Ahern"), None);
        assert_eq!(with_line(&ctx, "Ahern", "Here"), "Here");
    }
}
