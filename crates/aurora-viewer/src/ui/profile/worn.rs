//! The "Worn" tab of a resident's profile, in both window styles: what the
//! avatar wears, by part of the body, with each object's attachment point,
//! complexity and triangles. The figures are those of "Inspect"
//! (app/inspect.rs gathers both); nothing here shows a texture or any other
//! asset id. On another resident's profile a row can be derendered, put
//! back and outlined in the world; our own attachments only list.
//!
//! The list needs the avatar in a region we are connected to: the simulator
//! sends what an avatar wears with the avatar itself.

use crate::theme::Palette;
use crate::ui::widgets::{self, ActionKind};
use egui::{Rect, RichText, Sense, Vec2};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// The lists are gathered again this often while a Worn tab shows.
const EVERY: Duration = Duration::from_millis(500);
const ROW_H: f32 = 22.0;
const FIGURE_W: f32 = 84.0;

/// Where on the body an attachment point is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Head,
    UpperBody,
    Arms,
    LowerBody,
    Legs,
    Other,
    /// Our own avatar only: nobody else's HUDs reach us.
    Hud,
}

impl Category {
    /// The category of attachment point `point` (the ids of
    /// avatar_lad.xml); `hud` is the point's own flag there.
    pub fn of(point: u8, hud: bool) -> Category {
        if hud {
            return Category::Hud;
        }
        match point {
            // skull, mouth, chin, ears, eyes, nose, jaw, alt ears and eyes, tongue
            2 | 11..=17 | 47..=52 => Category::Head,
            // chest, shoulders, spine, pecs, neck, wings
            1 | 3 | 4 | 9 | 29 | 30 | 39 | 45 | 46 => Category::UpperBody,
            // hands, upper arms, forearms, ring fingers
            5 | 6 | 18..=21 | 41 | 42 => Category::Arms,
            // pelvis, hips, stomach, tail, groin
            10 | 22 | 25 | 28 | 43 | 44 | 53 => Category::LowerBody,
            // feet, upper and lower legs, hind feet
            7 | 8 | 23 | 24 | 26 | 27 | 54 | 55 => Category::Legs,
            // the HUD points, should the flag be missing
            31..=38 => Category::Hud,
            // avatar center, and the points a grid adds
            _ => Category::Other,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Category::Head => aurora_i18n::tr!("wornposer-cat-head"),
            Category::UpperBody => aurora_i18n::tr!("wornposer-cat-upper"),
            Category::Arms => aurora_i18n::tr!("wornposer-cat-arms"),
            Category::LowerBody => aurora_i18n::tr!("wornposer-cat-lower"),
            Category::Legs => aurora_i18n::tr!("wornposer-cat-legs"),
            Category::Other => aurora_i18n::tr!("wornposer-cat-other"),
            Category::Hud => aurora_i18n::tr!("wornposer-cat-hud"),
        }
    }
}

/// One worn object (a whole linkset).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// Its root prim.
    pub id: Uuid,
    /// None until the simulator gave it.
    pub name: Option<String>,
    pub point: u8,
    pub point_name: String,
    pub hud: bool,
    pub complexity: u32,
    pub triangles: u32,
    /// Derendered: kept out of the world, listed to be put back.
    pub derendered: bool,
}

/// What a tab asks of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// "Derender" / "Derender + blacklist" of one worn object.
    Derender { object: Uuid, permanent: bool },
    /// "Re-render": its derender entry goes, the object comes back.
    Rerender { object: Uuid },
}

/// The worn lists of the avatars whose tab shows, by avatar. An avatar
/// that is not in a connected region has none.
#[derive(Default)]
pub struct Feed {
    lists: HashMap<Uuid, Vec<Row>>,
    at: Option<Instant>,
}

impl Feed {
    pub fn due(&self, now: Instant) -> bool {
        self.at.is_none_or(|at| now.duration_since(at) >= EVERY)
    }

    pub fn set(&mut self, lists: HashMap<Uuid, Vec<Row>>, now: Instant) {
        self.lists = lists;
        self.at = Some(now);
    }

    pub fn get(&self, avatar: &Uuid) -> Option<&[Row]> {
        self.lists.get(avatar).map(Vec::as_slice)
    }

    /// Nothing shows a Worn tab any more.
    pub fn clear(&mut self) {
        self.lists.clear();
        self.at = None;
    }
}

/// The tab's own state in a profile window.
#[derive(Default)]
pub struct State {
    /// The selected row.
    pub selected: Option<Uuid>,
    /// The row outlined in the world ("Highlight in world").
    pub lit: Option<Uuid>,
}

/// The rows by category, in the order of `Category`, each by attachment
/// point then name. The HUDs are left out of another avatar's list.
pub fn grouped(rows: &[Row], own: bool) -> Vec<(Category, Vec<&Row>)> {
    let mut groups: Vec<(Category, Vec<&Row>)> = Vec::new();
    let mut sorted: Vec<(Category, &Row)> = rows
        .iter()
        .map(|row| (Category::of(row.point, row.hud), row))
        .filter(|(category, _)| own || *category != Category::Hud)
        .collect();
    sorted.sort_by(|(ca, a), (cb, b)| {
        ca.cmp(cb)
            .then(a.point.cmp(&b.point))
            .then_with(|| crate::natural_order::cmp(a.name.as_deref().unwrap_or(""), b.name.as_deref().unwrap_or("")))
            .then(a.id.cmp(&b.id))
    });
    for (category, row) in sorted {
        match groups.last_mut() {
            Some((last, list)) if *last == category => list.push(row),
            _ => groups.push((category, vec![row])),
        }
    }
    groups
}

/// Which row actions the tab offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Available {
    pub derender: bool,
    pub rerender: bool,
    pub highlight: bool,
}

/// The actions for the selected row: none at all on our own profile, and
/// a derendered object can only be put back (it has nothing to outline).
pub fn available(own: bool, row: Option<&Row>) -> Available {
    match row {
        Some(row) if !own => Available {
            derender: !row.derendered,
            rerender: row.derendered,
            highlight: !row.derendered && !row.hud,
        },
        _ => Available::default(),
    }
}

fn figure(ui: &egui::Ui, text: String, cell: Rect, color: egui::Color32) {
    let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(12.0), color);
    let left = egui::pos2(cell.right() - 8.0 - galley.size().x, cell.center().y);
    widgets::paint_ink_left(ui.painter(), galley, left, color);
}

/// The cells of a row: name, attachment point, complexity, triangles.
fn cells(row: Rect) -> [Rect; 4] {
    let figures = if row.width() < 420.0 { FIGURE_W * 0.8 } else { FIGURE_W };
    let text = (row.width() - 2.0 * figures).max(0.0);
    let name = text * 0.6;
    let mut x = row.left();
    [name, text - name, figures, figures].map(|w| {
        let cell = Rect::from_min_max(egui::pos2(x, row.top()), egui::pos2(x + w, row.bottom()));
        x += w;
        cell
    })
}

fn header(ui: &mut egui::Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::hover());
    ui.painter().rect_filled(rect, ui.visuals().window_corner_radius, p.field);
    let font = egui::FontId::proportional(12.0);
    let [name, point, complexity, triangles] = cells(rect);
    for (cell, title) in [
        (name, aurora_i18n::tr!("wornposer-col-name")),
        (point, aurora_i18n::tr!("wornposer-col-point")),
    ] {
        let left = egui::pos2(cell.left() + 8.0, cell.center().y);
        widgets::paint_elided(ui, title, left, cell.width() - 12.0, font.clone(), p.muted);
    }
    figure(ui, aurora_i18n::tr!("inspect-totals-complexity").to_owned(), complexity, p.muted);
    figure(ui, aurora_i18n::tr!("inspect-col-triangles").to_owned(), triangles, p.muted);
}

fn row_line(ui: &mut egui::Ui, p: &Palette, row: &Row, state: &mut State) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let radius = ui.visuals().window_corner_radius;
    if state.selected == Some(row.id) {
        ui.painter().rect_filled(rect, radius, p.violet.gamma_multiply(0.30));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, radius, p.raised.gamma_multiply(0.6));
    }
    let [name, point, complexity, triangles] = cells(rect);
    let font = egui::FontId::proportional(12.5);
    let known = row.name.as_deref().filter(|n| !n.trim().is_empty());
    let text = match (known, row.derendered) {
        (Some(name), false) => name.to_owned(),
        (Some(name), true) => aurora_i18n::tr!("wornposer-derendered-name", name = name),
        (None, true) => aurora_i18n::tr!("wornposer-derendered").to_owned(),
        (None, false) => aurora_i18n::tr!("common-loading-2").to_owned(),
    };
    let ink = if known.is_some() && !row.derendered { p.ink } else { p.muted };
    let mut left = egui::pos2(name.left() + 8.0, name.center().y);
    let mut room = name.width() - 12.0;
    if state.lit == Some(row.id) {
        // the mark of the row outlined in the world
        if let Some(texture) = crate::ui::icons::global("focus-2") {
            let at = Rect::from_center_size(egui::pos2(left.x + 6.0, left.y), Vec2::splat(12.0));
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(texture.id(), at, uv, p.violet_light);
        }
        left.x += 16.0;
        room -= 16.0;
    }
    let cut = widgets::paint_elided(ui, &text, left, room, font.clone(), ink);
    let at = egui::pos2(point.left() + 8.0, point.center().y);
    widgets::paint_elided(ui, &row.point_name, at, point.width() - 12.0, font, p.muted);
    if row.derendered {
        // out of the world: its geometry is not there to be counted
        figure(ui, "–".to_owned(), complexity, p.muted);
        figure(ui, "–".to_owned(), triangles, p.muted);
    } else {
        figure(ui, aurora_i18n::integer(i64::from(row.complexity)), complexity, p.ink);
        figure(ui, aurora_i18n::integer(i64::from(row.triangles)), triangles, p.ink);
    }
    if resp.clicked() {
        state.selected = Some(row.id);
    }
    if cut {
        resp.on_hover_text(text);
    }
}

fn buttons(ui: &mut egui::Ui, p: &Palette, row: Option<&Row>, state: &mut State, actions: &mut Vec<Action>) {
    let can = available(false, row);
    let size = Vec2::new(0.0, 26.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for (icon, label, tip, permanent) in [
            (
                "eye-off",
                aurora_i18n::tr!("ui-context-derender-temporarily"),
                aurora_i18n::tr!("wornposer-derender-tip"),
                false,
            ),
            (
                "eye-closed",
                aurora_i18n::tr!("ui-context-derender-blacklist"),
                aurora_i18n::tr!("wornposer-blacklist-tip"),
                true,
            ),
        ] {
            let button = widgets::action_button_if(ui, p, icon, label, ActionKind::Flat, size, can.derender);
            if button.on_hover_text(tip).clicked()
                && let Some(row) = row
            {
                actions.push(Action::Derender { object: row.id, permanent });
                if state.lit == Some(row.id) {
                    state.lit = None;
                }
            }
        }
        let rerender = widgets::action_button_if(
            ui,
            p,
            "eye-filled",
            aurora_i18n::tr!("wornposer-rerender"),
            ActionKind::Flat,
            size,
            can.rerender,
        );
        if rerender.on_hover_text(aurora_i18n::tr!("wornposer-rerender-tip")).clicked()
            && let Some(row) = row
        {
            actions.push(Action::Rerender { object: row.id });
        }
        let lit = row.is_some_and(|row| state.lit == Some(row.id));
        let kind = if lit { ActionKind::Primary } else { ActionKind::Flat };
        let highlight = widgets::action_button_if(ui, p, "focus-2", aurora_i18n::tr!("wornposer-highlight"), kind, size, can.highlight);
        if highlight.on_hover_text(aurora_i18n::tr!("wornposer-highlight-tip")).clicked()
            && let Some(row) = row
        {
            state.lit = if lit { None } else { Some(row.id) };
        }
    });
}

/// Draw the tab. `rows` is None while the avatar is not in a connected
/// region; `name` is the resident's, for that message.
pub fn show(ui: &mut egui::Ui, p: &Palette, state: &mut State, rows: Option<&[Row]>, own: bool, name: &str, actions: &mut Vec<Action>) {
    let Some(rows) = rows else {
        state.lit = None;
        ui.add_space(18.0);
        ui.vertical_centered(|ui| {
            if let Some(texture) = crate::ui::icons::global("shirt-filled") {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(texture.id(), rect, uv, p.muted_dim);
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(aurora_i18n::tr!("wornposer-not-nearby", name = name))
                    .size(13.0)
                    .color(p.muted),
            );
            ui.label(
                RichText::new(aurora_i18n::tr!("wornposer-not-nearby-hint"))
                    .size(12.0)
                    .color(p.muted_dim),
            );
        });
        return;
    };
    let groups = grouped(rows, own);
    let listed: Vec<&Row> = groups.iter().flat_map(|(_, list)| list.iter().copied()).collect();
    // what is selected or outlined left the list (detached, gone)
    if !listed.iter().any(|row| Some(row.id) == state.selected) {
        state.selected = None;
    }
    if own || !listed.iter().any(|row| Some(row.id) == state.lit && !row.derendered) {
        state.lit = None;
    }
    ui.spacing_mut().item_spacing.y = 4.0;
    let shown = listed.iter().filter(|row| !row.derendered);
    let complexity: u64 = shown.clone().map(|row| u64::from(row.complexity)).sum();
    let triangles: u64 = shown.map(|row| u64::from(row.triangles)).sum();
    ui.label(
        RichText::new(aurora_i18n::tr!(
            "wornposer-summary",
            count = listed.len() as i64,
            complexity = aurora_i18n::integer(complexity as i64),
            triangles = aurora_i18n::integer(triangles as i64)
        ))
        .size(12.5)
        .color(p.ink),
    );
    if !own {
        let current = listed.iter().copied().find(|row| Some(row.id) == state.selected);
        buttons(ui, p, current, state, actions);
    }
    ui.add_space(2.0);
    if listed.is_empty() {
        ui.label(RichText::new(aurora_i18n::tr!("wornposer-nothing")).size(12.0).color(p.muted));
        return;
    }
    header(ui, p);
    ui.spacing_mut().item_spacing.y = 1.0;
    for (category, list) in groups {
        ui.add_space(6.0);
        ui.label(
            RichText::new(aurora_i18n::tr!(
                "wornposer-category",
                category = category.title(),
                count = list.len() as i64
            ))
            .size(11.0)
            .strong()
            .color(p.muted_dim),
        );
        for row in list {
            row_line(ui, p, row, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(n: u128, name: &str, point: u8) -> Row {
        Row {
            id: Uuid::from_u128(n),
            name: (!name.is_empty()).then(|| name.to_owned()),
            point,
            hud: (31..=38).contains(&point),
            ..Row::default()
        }
    }

    #[test]
    fn every_attachment_point_of_the_skeleton_has_its_category() {
        let head = [2, 11, 12, 13, 14, 15, 16, 17, 47, 48, 49, 50, 51, 52];
        let upper = [1, 3, 4, 9, 29, 30, 39, 45, 46];
        let arms = [5, 6, 18, 19, 20, 21, 41, 42];
        let lower = [10, 22, 25, 28, 43, 44, 53];
        let legs = [7, 8, 23, 24, 26, 27, 54, 55];
        for point in 1..=55u8 {
            let expected = if head.contains(&point) {
                Category::Head
            } else if upper.contains(&point) {
                Category::UpperBody
            } else if arms.contains(&point) {
                Category::Arms
            } else if lower.contains(&point) {
                Category::LowerBody
            } else if legs.contains(&point) {
                Category::Legs
            } else if (31..=38).contains(&point) {
                Category::Hud
            } else {
                Category::Other
            };
            assert_eq!(Category::of(point, (31..=38).contains(&point)), expected, "point {point}");
        }
        // avatar center, a point a grid adds, and the flag of the skeleton file
        assert_eq!(Category::of(40, false), Category::Other);
        assert_eq!(Category::of(120, false), Category::Other);
        assert_eq!(Category::of(120, true), Category::Hud);
        // the skeleton's own flags agree with the table
        let lib = crate::scene::avatar::AvatarLibrary::load();
        for (point, ap) in &lib.attach_points {
            assert_eq!(Category::of(*point, ap.hud) == Category::Hud, ap.hud, "point {point}");
        }
    }

    #[test]
    fn rows_are_grouped_by_body_part_then_point_then_name() {
        let rows = [
            row(1, "Boots", 7),
            row(2, "hair", 2),
            row(3, "Ring", 5),
            row(4, "Glasses", 17),
            row(5, "AO", 35),
            row(6, "", 2),
            row(7, "Belt", 10),
            row(8, "Collar", 39),
            row(9, "Aura", 40),
        ];
        let names = |own: bool| {
            grouped(&rows, own)
                .into_iter()
                .map(|(category, list)| {
                    (
                        category,
                        list.into_iter().map(|r| r.name.as_deref().unwrap_or("?")).collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let others = vec![
            // the same point: the unnamed one first, then by name
            (Category::Head, vec!["?", "hair", "Glasses"]),
            (Category::UpperBody, vec!["Collar"]),
            (Category::Arms, vec!["Ring"]),
            (Category::LowerBody, vec!["Belt"]),
            (Category::Legs, vec!["Boots"]),
            (Category::Other, vec!["Aura"]),
        ];
        assert_eq!(names(false), others);
        // our own HUDs are listed, last; nobody else's ever
        let mut mine = others;
        mine.push((Category::Hud, vec!["AO"]));
        assert_eq!(names(true), mine);
        assert!(grouped(&[], false).is_empty());
    }

    #[test]
    fn row_actions_are_for_other_avatars_only() {
        let worn = row(1, "Hair", 2);
        let hidden = Row {
            derendered: true,
            ..row(2, "Tail", 43)
        };
        let none = Available::default();
        // our own attachments: nothing, whatever the row
        assert_eq!(available(true, Some(&worn)), none);
        assert_eq!(available(true, Some(&hidden)), none);
        assert_eq!(available(true, None), none);
        // another avatar's: nothing without a row
        assert_eq!(available(false, None), none);
        assert_eq!(
            available(false, Some(&worn)),
            Available {
                derender: true,
                rerender: false,
                highlight: true
            }
        );
        // derendered: only to be put back
        assert_eq!(
            available(false, Some(&hidden)),
            Available {
                derender: false,
                rerender: true,
                highlight: false
            }
        );
    }

    #[test]
    fn the_lists_are_gathered_again_twice_a_second() {
        let mut feed = Feed::default();
        let start = Instant::now();
        assert!(feed.due(start));
        let avatar = Uuid::from_u128(9);
        feed.set(HashMap::from([(avatar, vec![row(1, "Hair", 2)])]), start);
        assert!(!feed.due(start + Duration::from_millis(100)));
        assert!(feed.due(start + EVERY));
        assert_eq!(feed.get(&avatar).map(<[Row]>::len), Some(1));
        // not in a connected region: no list
        assert_eq!(feed.get(&Uuid::from_u128(10)), None);
        feed.clear();
        assert!(feed.due(start));
        assert_eq!(feed.get(&avatar), None);
    }
}
