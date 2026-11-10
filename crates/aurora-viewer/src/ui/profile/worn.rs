//! The "Worn" tab of a resident's profile, in both window styles: what the
//! avatar wears, by part of the body, with each object's attachment point,
//! complexity and triangles. The figures are those of "Inspect"
//! (app/inspect.rs gathers both); nothing here shows a texture or any other
//! asset id. On another resident's profile a row can be derendered, put
//! back and outlined in the world; our own attachments only list.
//!
//! The list needs the avatar in a region we are connected to: the simulator
//! sends what an avatar wears with the avatar itself. When they leave, the
//! last list stays, greyed, with the time they left; while RLVa hides who
//! they are, nothing is listed. The rows can also be grouped by kind
//! (rigged mesh, attached objects, animesh), several chosen at once
//! (Ctrl, Shift), and the one chosen has its details under the list: its
//! name, point, kind, figures and creator (once the viewer knows it),
//! never an id.

use crate::theme::Palette;
use crate::ui::widgets::{self, ActionKind};
use egui::{Rect, RichText, Sense, Vec2};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime};
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

/// What kind of object it is, for the grouping by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Mesh worn on the skeleton (a prim of it is rigged).
    Rigged,
    /// An object simply attached to a point.
    #[default]
    Attached,
    /// An animated object (EXTENDED_MESH_ANIMATED on its root).
    Animesh,
}

/// The groups of the list by kind, in their order; the HUDs last, ours only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KindGroup {
    Rigged,
    Attached,
    Animesh,
    Hud,
}

impl KindGroup {
    fn of(row: &Row) -> KindGroup {
        match (row.hud, row.kind) {
            (true, _) => KindGroup::Hud,
            (false, Kind::Rigged) => KindGroup::Rigged,
            (false, Kind::Attached) => KindGroup::Attached,
            (false, Kind::Animesh) => KindGroup::Animesh,
        }
    }

    fn title(self) -> &'static str {
        match self {
            KindGroup::Rigged => aurora_i18n::tr!("b6-worn-kind-rigged"),
            KindGroup::Attached => aurora_i18n::tr!("b6-worn-kind-attached"),
            KindGroup::Animesh => aurora_i18n::tr!("b6-worn-kind-animesh"),
            KindGroup::Hud => aurora_i18n::tr!("wornposer-cat-hud"),
        }
    }
}

/// How the list is grouped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupBy {
    #[default]
    BodyPart,
    Kind,
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
    pub kind: Kind,
    /// Its creator's name, once the viewer has its full properties.
    pub creator: Option<String>,
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
/// that is not in a connected region has none; one that left keeps its
/// last list, with when it left.
#[derive(Default)]
pub struct Feed {
    lists: HashMap<Uuid, Vec<Row>>,
    left: HashMap<Uuid, (Vec<Row>, SystemTime)>,
    /// RLVa hides who they are (RlvActions::canShowName).
    hidden: HashSet<Uuid>,
    at: Option<Instant>,
}

/// Where the avatar of a tab is, for its list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seen<'a> {
    /// In a region we are connected to: what they wear now.
    Here(&'a [Row]),
    /// Gone since this time: what they wore then.
    Left(&'a [Row], SystemTime),
    /// RLVa hides who they are.
    Hidden,
    /// Never seen while the tab showed.
    Away,
}

impl Feed {
    pub fn due(&self, now: Instant) -> bool {
        self.at.is_none_or(|at| now.duration_since(at) >= EVERY)
    }

    /// The lists of this round; an avatar listed before and missing now
    /// left at `when` (unless RLVa hides it now).
    pub fn set(&mut self, lists: HashMap<Uuid, Vec<Row>>, hidden: HashSet<Uuid>, now: Instant, when: SystemTime) {
        for (avatar, rows) in self.lists.drain() {
            if !lists.contains_key(&avatar) && !hidden.contains(&avatar) {
                self.left.insert(avatar, (rows, when));
            }
        }
        self.left
            .retain(|avatar, _| !lists.contains_key(avatar) && !hidden.contains(avatar));
        self.lists = lists;
        self.hidden = hidden;
        self.at = Some(now);
    }

    pub fn get(&self, avatar: &Uuid) -> Option<&[Row]> {
        self.lists.get(avatar).map(Vec::as_slice)
    }

    pub fn seen(&self, avatar: &Uuid) -> Seen<'_> {
        if self.hidden.contains(avatar) {
            return Seen::Hidden;
        }
        match (self.lists.get(avatar), self.left.get(avatar)) {
            (Some(rows), _) => Seen::Here(rows),
            (None, Some((rows, when))) => Seen::Left(rows, *when),
            (None, None) => Seen::Away,
        }
    }

    /// Nothing shows a Worn tab any more.
    pub fn clear(&mut self) {
        self.lists.clear();
        self.left.clear();
        self.hidden.clear();
        self.at = None;
    }
}

/// The tab's own state in a profile window.
#[derive(Default)]
pub struct State {
    /// The selected rows, and the one Shift extends from.
    pub selected: Vec<Uuid>,
    anchor: Option<Uuid>,
    /// The rows outlined in the world ("Highlight in world").
    pub lit: Vec<Uuid>,
    pub group: GroupBy,
}

/// A click on row `id` among `order` (the list as shown): alone, Ctrl
/// adds or removes it, Shift takes the rows from the last one clicked.
pub fn pick(state: &mut State, order: &[Uuid], id: Uuid, ctrl: bool, shift: bool) {
    let position = |x: Uuid| order.iter().position(|o| *o == x);
    match (shift, state.anchor.and_then(position), position(id)) {
        (true, Some(a), Some(b)) => {
            let (from, to) = (a.min(b), a.max(b));
            state.selected = order[from..=to].to_vec();
            return;
        }
        _ if ctrl => {
            if let Some(i) = state.selected.iter().position(|s| *s == id) {
                state.selected.remove(i);
            } else {
                state.selected.push(id);
            }
        }
        _ => state.selected = vec![id],
    }
    state.anchor = Some(id);
}

/// The rows by kind, in the order of `KindGroup`, each by attachment point
/// then name. The HUDs are left out of another avatar's list.
pub fn grouped_by_kind(rows: &[Row], own: bool) -> Vec<(KindGroup, Vec<&Row>)> {
    let mut sorted: Vec<(KindGroup, &Row)> = rows
        .iter()
        .map(|row| (KindGroup::of(row), row))
        .filter(|(group, _)| own || *group != KindGroup::Hud)
        .collect();
    sorted.sort_by(|(ga, a), (gb, b)| {
        ga.cmp(gb)
            .then(a.point.cmp(&b.point))
            .then_with(|| crate::natural_order::cmp(a.name.as_deref().unwrap_or(""), b.name.as_deref().unwrap_or("")))
            .then(a.id.cmp(&b.id))
    });
    let mut groups: Vec<(KindGroup, Vec<&Row>)> = Vec::new();
    for (group, row) in sorted {
        match groups.last_mut() {
            Some((last, list)) if *last == group => list.push(row),
            _ => groups.push((group, vec![row])),
        }
    }
    groups
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

/// The actions for the selected rows: none at all on our own profile; a
/// derendered object can only be put back (it has nothing to outline).
/// With several rows, an action is offered when it applies to one of them.
pub fn available(own: bool, rows: &[&Row]) -> Available {
    if own {
        return Available::default();
    }
    Available {
        derender: rows.iter().any(|row| !row.derendered),
        rerender: rows.iter().any(|row| row.derendered),
        highlight: rows.iter().any(|row| !row.derendered && !row.hud),
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

fn row_line(ui: &mut egui::Ui, p: &Palette, row: &Row, state: &mut State, order: &[Uuid]) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let radius = ui.visuals().window_corner_radius;
    if state.selected.contains(&row.id) {
        ui.painter().rect_filled(rect, radius, p.violet.gamma_multiply(0.30));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, radius, p.raised.gamma_multiply(0.6));
    }
    let [name, point, complexity, triangles] = cells(rect);
    let font = egui::FontId::proportional(12.5);
    let text = row_name(row);
    let known = row.name.as_deref().is_some_and(|n| !n.trim().is_empty());
    let ink = if known && !row.derendered { p.ink } else { p.muted };
    let mut left = egui::pos2(name.left() + 8.0, name.center().y);
    let mut room = name.width() - 12.0;
    if state.lit.contains(&row.id) {
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
        let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
        pick(state, order, row.id, ctrl, shift);
    }
    if cut {
        resp.on_hover_text(text);
    }
}

/// The name a row shows: the object's, marked when derendered.
fn row_name(row: &Row) -> String {
    let known = row.name.as_deref().filter(|n| !n.trim().is_empty());
    match (known, row.derendered) {
        (Some(name), false) => name.to_owned(),
        (Some(name), true) => aurora_i18n::tr!("wornposer-derendered-name", name = name),
        (None, true) => aurora_i18n::tr!("wornposer-derendered").to_owned(),
        (None, false) => aurora_i18n::tr!("common-loading-2").to_owned(),
    }
}

fn kind_title(kind: Kind) -> &'static str {
    match kind {
        Kind::Rigged => aurora_i18n::tr!("b6-worn-kind-rigged"),
        Kind::Attached => aurora_i18n::tr!("b6-worn-kind-attached"),
        Kind::Animesh => aurora_i18n::tr!("b6-worn-kind-animesh"),
    }
}

/// The details of the one row chosen: names and figures, never an id.
fn details(ui: &mut egui::Ui, p: &Palette, row: &Row) {
    ui.add_space(8.0);
    widgets::divider(ui, p);
    ui.add_space(4.0);
    ui.label(RichText::new(row_name(row)).size(13.0).strong().color(p.ink));
    let creator = row
        .creator
        .clone()
        .unwrap_or_else(|| aurora_i18n::tr!("b6-worn-creator-unknown").to_owned());
    let figures = if row.derendered {
        "–".to_owned()
    } else {
        aurora_i18n::tr!(
            "b6-worn-figures",
            complexity = aurora_i18n::integer(i64::from(row.complexity)),
            triangles = aurora_i18n::integer(i64::from(row.triangles))
        )
    };
    for (label, value) in [
        (aurora_i18n::tr!("wornposer-col-point"), row.point_name.clone()),
        (aurora_i18n::tr!("b6-worn-kind"), kind_title(row.kind).to_owned()),
        (aurora_i18n::tr!("b6-worn-creator"), creator),
        (aurora_i18n::tr!("inspect-totals-complexity"), figures),
    ] {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.0).color(p.muted));
            ui.label(RichText::new(value).size(12.0).color(p.ink));
        });
    }
}

fn buttons(ui: &mut egui::Ui, p: &Palette, rows: &[&Row], state: &mut State, actions: &mut Vec<Action>) {
    let can = available(false, rows);
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
            if button.on_hover_text(tip).clicked() {
                for row in rows.iter().filter(|row| !row.derendered) {
                    actions.push(Action::Derender { object: row.id, permanent });
                    state.lit.retain(|lit| *lit != row.id);
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
        if rerender.on_hover_text(aurora_i18n::tr!("wornposer-rerender-tip")).clicked() {
            for row in rows.iter().filter(|row| row.derendered) {
                actions.push(Action::Rerender { object: row.id });
            }
        }
        let outlinable: Vec<Uuid> = rows.iter().filter(|row| !row.derendered && !row.hud).map(|row| row.id).collect();
        let lit = !outlinable.is_empty() && outlinable.iter().all(|id| state.lit.contains(id));
        let kind = if lit { ActionKind::Primary } else { ActionKind::Flat };
        let highlight = widgets::action_button_if(ui, p, "focus-2", aurora_i18n::tr!("wornposer-highlight"), kind, size, can.highlight);
        if highlight.on_hover_text(aurora_i18n::tr!("wornposer-highlight-tip")).clicked() {
            state.lit = if lit { Vec::new() } else { outlinable };
        }
    });
}

/// Draw the tab. `seen` says where the avatar is; `name` is the
/// resident's, for the messages.
pub fn show(ui: &mut egui::Ui, p: &Palette, state: &mut State, seen: Seen<'_>, own: bool, name: &str, actions: &mut Vec<Action>) {
    let (rows, left) = match seen {
        Seen::Here(rows) => (rows, None),
        Seen::Left(rows, when) => (rows, Some(when)),
        Seen::Hidden | Seen::Away => {
            state.lit.clear();
            let (line, hint) = if seen == Seen::Hidden {
                (aurora_i18n::tr!("b6-worn-rlva").to_owned(), "")
            } else {
                (
                    aurora_i18n::tr!("wornposer-not-nearby", name = name),
                    aurora_i18n::tr!("wornposer-not-nearby-hint"),
                )
            };
            ui.add_space(18.0);
            ui.vertical_centered(|ui| {
                if let Some(texture) = crate::ui::icons::global("shirt-filled") {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    ui.painter().image(texture.id(), rect, uv, p.muted_dim);
                }
                ui.add_space(6.0);
                ui.label(RichText::new(line).size(13.0).color(p.muted));
                if !hint.is_empty() {
                    ui.label(RichText::new(hint).size(12.0).color(p.muted_dim));
                }
            });
            return;
        }
    };
    let groups: Vec<(&str, Vec<&Row>)> = match state.group {
        GroupBy::BodyPart => grouped(rows, own).into_iter().map(|(c, list)| (c.title(), list)).collect(),
        GroupBy::Kind => grouped_by_kind(rows, own).into_iter().map(|(g, list)| (g.title(), list)).collect(),
    };
    let listed: Vec<&Row> = groups.iter().flat_map(|(_, list)| list.iter().copied()).collect();
    let order: Vec<Uuid> = listed.iter().map(|row| row.id).collect();
    // what is selected or outlined left the list (detached, gone)
    state.selected.retain(|id| order.contains(id));
    if own || left.is_some() {
        state.lit.clear();
    }
    state.lit.retain(|id| listed.iter().any(|row| row.id == *id && !row.derendered));
    ui.spacing_mut().item_spacing.y = 4.0;
    if let Some(when) = left {
        // gone: the last list, greyed (FD's "left at")
        ui.label(
            RichText::new(aurora_i18n::tr!("b6-worn-left", name = name, time = crate::ui::chat::hhmm_24(when)))
                .size(12.5)
                .color(p.amber),
        );
    }
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
    let mut by = usize::from(state.group == GroupBy::Kind);
    if widgets::segmented(
        ui,
        p,
        &mut by,
        &[
            (aurora_i18n::tr!("b6-worn-by-part"), false),
            (aurora_i18n::tr!("b6-worn-by-kind"), false),
        ],
    ) {
        state.group = if by == 1 { GroupBy::Kind } else { GroupBy::BodyPart };
    }
    let chosen: Vec<&Row> = listed.iter().copied().filter(|row| state.selected.contains(&row.id)).collect();
    if !own && left.is_none() {
        buttons(ui, p, &chosen, state, actions);
    }
    ui.add_space(2.0);
    if listed.is_empty() {
        ui.label(RichText::new(aurora_i18n::tr!("wornposer-nothing")).size(12.0).color(p.muted));
        return;
    }
    let opacity = ui.opacity();
    if left.is_some() {
        ui.set_opacity(opacity * 0.55);
    }
    header(ui, p);
    ui.spacing_mut().item_spacing.y = 1.0;
    for (title, list) in &groups {
        ui.add_space(6.0);
        ui.label(
            RichText::new(aurora_i18n::tr!("wornposer-category", category = *title, count = list.len() as i64))
                .size(11.0)
                .strong()
                .color(p.muted_dim),
        );
        for row in list {
            row_line(ui, p, row, state, &order);
        }
    }
    ui.set_opacity(opacity);
    if let [one] = chosen.as_slice() {
        details(ui, p, one);
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
        assert_eq!(available(true, &[&worn]), none);
        assert_eq!(available(true, &[&hidden]), none);
        assert_eq!(available(true, &[]), none);
        // another avatar's: nothing without a row
        assert_eq!(available(false, &[]), none);
        // several: what applies to one of them
        assert_eq!(
            available(false, &[&worn, &hidden]),
            Available {
                derender: true,
                rerender: true,
                highlight: true
            }
        );
        assert_eq!(
            available(false, &[&worn]),
            Available {
                derender: true,
                rerender: false,
                highlight: true
            }
        );
        // derendered: only to be put back
        assert_eq!(
            available(false, &[&hidden]),
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
        let when = SystemTime::UNIX_EPOCH;
        feed.set(HashMap::from([(avatar, vec![row(1, "Hair", 2)])]), HashSet::new(), start, when);
        assert!(!feed.due(start + Duration::from_millis(100)));
        assert!(feed.due(start + EVERY));
        assert_eq!(feed.get(&avatar).map(<[Row]>::len), Some(1));
        // not in a connected region: no list
        assert_eq!(feed.get(&Uuid::from_u128(10)), None);
        assert_eq!(feed.seen(&Uuid::from_u128(10)), Seen::Away);
        feed.clear();
        assert!(feed.due(start));
        assert_eq!(feed.get(&avatar), None);
    }

    #[test]
    fn an_avatar_that_leaves_keeps_its_last_list_until_it_comes_back() {
        let mut feed = Feed::default();
        let start = Instant::now();
        let (avatar, other) = (Uuid::from_u128(9), Uuid::from_u128(8));
        let at = |s: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let list = vec![row(1, "Hair", 2)];
        feed.set(HashMap::from([(avatar, list.clone())]), HashSet::new(), start, at(1));
        assert_eq!(feed.seen(&avatar), Seen::Here(&list));
        // gone at the next round
        feed.set(HashMap::new(), HashSet::new(), start, at(2));
        assert_eq!(feed.seen(&avatar), Seen::Left(&list, at(2)));
        // the time stays the one they left at
        feed.set(HashMap::new(), HashSet::new(), start, at(3));
        assert_eq!(feed.seen(&avatar), Seen::Left(&list, at(2)));
        // back again
        feed.set(HashMap::from([(avatar, list.clone())]), HashSet::new(), start, at(4));
        assert_eq!(feed.seen(&avatar), Seen::Here(&list));
        // RLVa hiding who they are: nothing, not even the last list
        feed.set(HashMap::new(), HashSet::from([avatar]), start, at(5));
        assert_eq!(feed.seen(&avatar), Seen::Hidden);
        feed.set(HashMap::new(), HashSet::new(), start, at(6));
        assert_eq!(feed.seen(&avatar), Seen::Away);
        assert_eq!(feed.seen(&other), Seen::Away);
    }

    #[test]
    fn rows_are_chosen_alone_with_ctrl_or_with_shift() {
        let order: Vec<Uuid> = (1..=5).map(Uuid::from_u128).collect();
        let mut state = State::default();
        pick(&mut state, &order, order[1], false, false);
        assert_eq!(state.selected, [order[1]]);
        pick(&mut state, &order, order[3], true, false);
        assert_eq!(state.selected, [order[1], order[3]]);
        pick(&mut state, &order, order[1], true, false);
        assert_eq!(state.selected, [order[3]]);
        // from the last clicked, in the list's order
        pick(&mut state, &order, order[0], false, true);
        assert_eq!(state.selected, order[0..=3].to_vec());
        pick(&mut state, &order, order[4], false, false);
        assert_eq!(state.selected, [order[4]]);
    }

    #[test]
    fn rows_grouped_by_kind_put_rigged_mesh_first_and_huds_last() {
        let rigged = Row {
            kind: Kind::Rigged,
            ..row(1, "Body", 10)
        };
        let animesh = Row {
            kind: Kind::Animesh,
            ..row(2, "Pet", 6)
        };
        let rows = [row(3, "Ring", 5), animesh, rigged, row(4, "AO", 35)];
        let groups = |own| {
            grouped_by_kind(&rows, own)
                .into_iter()
                .map(|(g, l)| (g, l.len()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            groups(false),
            [(KindGroup::Rigged, 1), (KindGroup::Attached, 1), (KindGroup::Animesh, 1)]
        );
        assert_eq!(groups(true).last(), Some(&(KindGroup::Hud, 1)));
    }
}
