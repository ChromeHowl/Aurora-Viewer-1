//! The head of a conversation in Aurora's workspace: a 48 px band with its
//! picture, its name and a line on it (the resident's user name and status,
//! a group's participants and call, the nearby chat's region and who hears
//! us), then its actions: call (green while in the call), the resident's
//! profile and, while they are not our friend, a friendship offer, the
//! members' panel, find, and the menu of the others (those of
//! Firestorm's IM toolbar, FSFloaterIM, kept in `header::more_menu`).

use super::{ChatUi, ConvAction, find, header, presence, sidebar};
use crate::theme::Palette;
use crate::world::World;
use egui::Color32;
use std::collections::HashMap;
use uuid::Uuid;

pub(super) const HEIGHT: f32 = 48.0;
/// From the middle of the title to the middle of the line under it.
const LINE_GAP: f32 = 17.0;
const PIC: f32 = 32.0;
const TOOL: f32 = 30.0;
/// Adding people to a conversation, wherever it is offered: the
/// conversation's bubble with a plus, apart from the friendship offer's
/// `user-plus`.
pub(super) const ADD_PEOPLE_ICON: &str = "message-circle-plus";
/// Requesting a teleport, wherever it is offered: a car comes to fetch you,
/// apart from the paper plane of the offer beside it.
pub(super) const REQUEST_TELEPORT_ICON: &str = "car-filled";

/// What the head says of the conversation shown.
struct Identity {
    title: String,
    subtitle: String,
    avatar: Option<Uuid>,
    kind: Option<sidebar::Kind>,
}

fn identity(world: &World, st: &ChatUi) -> Identity {
    let Some(id) = st.selected else {
        // where we stand: the parcel (the location bar's name), else the region
        let region = Some(world.rlva_parcel_label())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| world.rlva_region_label());
        let near = aurora_i18n::tr!("ui-chat-head-in-earshot", n = st.nearby_count);
        return Identity {
            title: aurora_i18n::tr!("common-nearby-chat").to_owned(),
            // the count first: a narrow head cuts the end of the line, and a long
            // parcel name must not take the number with it
            subtitle: if region.is_empty() { near } else { format!("{near} · {region}") },
            avatar: None,
            kind: None,
        };
    };
    let kind = sidebar::kind(world, &id);
    let mut parts: Vec<String> = Vec::new();
    let title = match kind {
        sidebar::Kind::Private => {
            let names = &world.social.avatar_names;
            let cached = names.get(&id);
            if let Some(user) = cached.and_then(|n| n.chat_username(&names.options)) {
                parts.push(user);
            }
            let status = presence::of(world, &id);
            if status.pill() {
                parts.push(status.label().to_lowercase());
            }
            if status == presence::Status::Offline {
                parts.push(aurora_i18n::tr!("ui-chat-head-offline-delivery").to_owned());
            }
            cached
                .map(|n| n.display(&names.options))
                .unwrap_or_else(|| world.session_title(&id))
        }
        sidebar::Kind::Group | sidebar::Kind::Conference => {
            parts.push(
                if kind == sidebar::Kind::Group {
                    aurora_i18n::tr!("ui-chat-head-group")
                } else {
                    aurora_i18n::tr!("ui-chat-head-conference")
                }
                .to_owned(),
            );
            let n = world.groups.sessions.get(&id).map(|s| s.participants.len()).unwrap_or(0);
            if n > 0 {
                parts.push(aurora_i18n::tr!("ui-chat-head-participants", n = n));
            }
            world.session_title(&id)
        }
    };
    if world.call_of(&id).is_some() || world.call_launched(&id).is_some() {
        parts.push(aurora_i18n::tr!("ui-chat-head-call-on").to_owned());
    }
    Identity {
        title,
        subtitle: parts.join(" · "),
        avatar: (kind == sidebar::Kind::Private).then_some(id),
        kind: Some(kind),
    }
}

/// The head; it may open the find bar (the search button).
#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ui: &mut egui::Ui,
    p: &Palette,
    pics: &HashMap<Uuid, egui::TextureHandle>,
    // textures by id (a group's insignia)
    images: &HashMap<Uuid, egui::TextureHandle>,
    world: &World,
    st: &mut ChatUi,
    actions: &mut Vec<ConvAction>,
    members_room: bool,
) {
    let who = identity(world, st);
    let width = ui.available_width();
    let gap = crate::ui::widgets::two_lines_gap(ui, &egui::FontId::proportional(14.0), &egui::FontId::proportional(12.0), LINE_GAP);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, HEIGHT + gap - LINE_GAP), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        egui::Stroke::new(1.0, p.ink.gamma_multiply(0.05)),
    );
    let pic = egui::Rect::from_min_size(egui::pos2(rect.left() + 12.0, rect.center().y - PIC * 0.5), egui::Vec2::splat(PIC));
    let radius = ui.visuals().window_corner_radius;
    // while finding, the field takes the picture's and the names' place
    let finding = st.searches.get(&st.selected).is_some_and(|s| s.find.is_some());
    match (who.avatar, who.kind) {
        _ if finding => {}
        (Some(a), _) => {
            st.wanted_pics.insert(a);
            crate::ui::widgets::avatar_disc(ui, p, pic, pics.get(&a), &who.title, false);
            let status = presence::of(world, &a);
            if status != presence::Status::Unknown {
                presence::paint_dot(ui.painter(), p, pic, status, p.panel);
            }
            let r = ui.interact(pic, sidebar::control_id("head-pic"), egui::Sense::click());
            super::name_menu(&r, p, world, a);
            if r.on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(status.label())
                .clicked()
            {
                actions.push(ConvAction::Profile(a));
            }
        }
        (None, Some(sidebar::Kind::Group)) => {
            let insignia = st.selected.and_then(|id| sidebar::group_insignia(world, &id));
            if let Some(i) = insignia {
                st.wanted_images.insert(i);
            }
            sidebar::paint_group_picture(ui, p, pic, insignia.and_then(|i| images.get(&i)), PIC - 16.0);
            if let Some(g) = st.selected {
                let r = ui.interact(pic, sidebar::control_id("head-pic"), egui::Sense::click());
                if r.on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(aurora_i18n::tr!("ui-group-profile-view"))
                    .clicked()
                {
                    crate::ui::group_profile::request_open(ui.ctx(), g);
                }
            }
        }
        (None, Some(_)) => {
            ui.painter().rect_filled(pic, radius, p.raised);
            super::thread::paint_icon(ui.painter(), "users", pic.shrink(8.0), p.muted);
        }
        (None, None) => {
            ui.painter().rect_filled(pic, radius, p.teal.gamma_multiply(0.16));
            super::thread::paint_icon(ui.painter(), "map-pin-filled", pic.shrink(8.0), p.teal);
        }
    }
    // the actions, from the right edge: those the head has no room for go
    // to its "…" menu (`head_layout`)
    let selected = st.selected;
    let group = who.kind.is_some_and(|k| k != sidebar::Kind::Private);
    let friend = selected.is_some_and(|other| world.social.is_friend(&other));
    let poke = selected
        .filter(|_| !group)
        .and_then(|other| crate::ui::poke::button(ui.ctx(), world, &other));
    let title_font = egui::FontId::new(14.0, crate::ui::fonts::semibold(ui.ctx()));
    let title_room = text_min(ui.fonts_mut(|f| f.row_height(&title_font)));
    let present = present_actions(world, selected, who.kind, friend, poke.is_some());
    let layout = head_layout(width, title_room, &present);
    // what the menu repeats of the buttons it replaced (the members' panel
    // only when it can open)
    let dropped: Vec<HeadAction> = layout
        .overflow
        .iter()
        .copied()
        .filter(|a| *a != HeadAction::Members || members_room)
        .collect();
    let mut x = rect.right() - 8.0;
    let mut slot = || {
        x -= TOOL + 2.0;
        egui::Rect::from_center_size(egui::pos2(x + TOOL * 0.5, rect.center().y), egui::Vec2::splat(TOOL))
    };
    // at the right edge, the close button of the conversation (the X of
    // its tab); the nearby chat cannot be closed
    if let Some(other) = selected
        && layout.shows(HeadAction::Close)
        && tool(ui, p, slot(), "x", super::close_tip(world, &other), Tone::Plain).clicked()
    {
        actions.push(ConvAction::Close(other));
    }
    // a group chat's snooze beside its close (FSFloaterIM's title bar)
    if let Some(other) = selected
        && layout.shows(HeadAction::Snooze)
        && tool(
            ui,
            p,
            slot(),
            super::SNOOZE_ICON,
            aurora_i18n::tr!("ui-chat-snooze-tip"),
            Tone::Plain,
        )
        .clicked()
    {
        actions.push(ConvAction::Snooze(other));
    }
    // the menu of the others
    let more = tool(ui, p, slot(), "dots-filled", aurora_i18n::tr!("ui-chat-more"), Tone::Plain);
    crate::ui::menu::dropdown(&more, p, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        header::more_items(ui, p, world, st, actions, group, true, &dropped);
    });
    // find
    if layout.shows(HeadAction::Find) {
        let find_tip = aurora_i18n::tr!("ui-chat-find-in-conversation");
        if tool(ui, p, slot(), "search-filled", find_tip, Tone::active(finding)).clicked() {
            toggle_find(ui.ctx(), st, finding);
        }
    }
    match (selected, who.kind) {
        // the nearby chat: the people around (the People window)
        (None, _) => {
            if layout.shows(HeadAction::People)
                && tool(ui, p, slot(), "users", aurora_i18n::tr!("common-nearby-people"), Tone::Plain).clicked()
            {
                actions.push(ConvAction::OpenPeople(0));
            }
        }
        // a group or a conference: its members' panel
        (Some(_), Some(k)) if k != sidebar::Kind::Private => {
            if layout.shows(HeadAction::Members) {
                let open = st.members_open && members_room;
                let tip = if open {
                    aurora_i18n::tr!("ui-chat-hide-members")
                } else {
                    aurora_i18n::tr!("ui-chat-show-members")
                };
                let r = tool(
                    ui,
                    p,
                    slot(),
                    "users",
                    tip,
                    if members_room { Tone::active(open) } else { Tone::Off },
                );
                if r.clicked() && members_room {
                    st.members_open = !st.members_open;
                }
            }
        }
        // a resident: their profile, and a teleport offered or requested (one
        // menu button in FSFloaterIM's toolbar, two buttons here)
        (Some(other), _) => {
            if layout.shows(HeadAction::Profile)
                && tool(ui, p, slot(), "user-filled", aurora_i18n::tr!("common-profile"), Tone::Plain).clicked()
            {
                actions.push(ConvAction::Profile(other));
            }
            // the friendship offer of FSFloaterIM's add_friend_btn, hidden
            // (not greyed, as there) once they are our friend
            // (the friendship itself: no offer to a hidden friend)
            if layout.shows(HeadAction::AddFriend)
                && tool(ui, p, slot(), "user-plus", aurora_i18n::tr!("ui-chat-add-friend"), Tone::Plain).clicked()
            {
                crate::ui::context::request(ui.ctx(), crate::ui::context::CtxAction::AddFriend(other));
            }
            if layout.shows(HeadAction::Offer)
                && tool(ui, p, slot(), "send-filled", aurora_i18n::tr!("common-offer-teleport"), Tone::Plain).clicked()
            {
                actions.push(ConvAction::OfferTeleport(other));
            }
            // the other direction, greyed for an offline friend as
            // FSFloaterIM::changed greys teleport_btn; a head too narrow
            // for it leaves it to the row of the "…" menu
            if layout.shows(HeadAction::Request) {
                let request = if world.social.can_request_teleport(&other) {
                    Tone::Plain
                } else {
                    Tone::Off
                };
                if tool(
                    ui,
                    p,
                    slot(),
                    REQUEST_TELEPORT_ICON,
                    aurora_i18n::tr!("common-request-teleport"),
                    request,
                )
                .clicked()
                {
                    crate::ui::context::request(ui.ctx(), crate::ui::context::CtxAction::RequestTeleport(other));
                }
            }
            // a poke (`ui/poke.rs`), greyed out while they are offline or
            // just poked
            if layout.shows(HeadAction::Poke)
                && let Some(b) = poke
                && tool(
                    ui,
                    p,
                    slot(),
                    crate::ui::poke::ICON,
                    b.tip,
                    if b.enabled { Tone::Plain } else { Tone::Off },
                )
                .clicked()
                && b.enabled
            {
                crate::ui::context::request(ui.ctx(), crate::ui::context::CtxAction::Poke(other));
            }
        }
    }
    // add people: to a resident's conversation (a new conference, once
    // confirmed) or a conference; a role group's chat cannot
    // (FSFloaterIM's add_participant_panel, isInviteAllowed)
    if let Some(other) = selected
        && layout.shows(HeadAction::AddPeople)
        && tool(ui, p, slot(), ADD_PEOPLE_ICON, aurora_i18n::tr!("ui-call-add-people"), Tone::Plain).clicked()
    {
        actions.push(ConvAction::PickPeople(other));
    }
    if let Some(other) = selected
        && layout.shows(HeadAction::Call)
    {
        call_tool(ui, p, slot(), world, st, actions, other);
    }
    if finding {
        // the field, from the picture's left to the actions, as tall as them
        let field = egui::Rect::from_x_y_ranges(
            pic.left()..=x - 6.0,
            (rect.center().y - TOOL * 0.5)..=(rect.center().y + TOOL * 0.5),
        );
        let ChatUi { searches, views, .. } = st;
        let view = views.entry(selected).or_default();
        if let Some(search) = searches.get_mut(&selected)
            && let Some(f) = search.find.as_mut()
            && find::field(ui, p, field, p.bar, f, view, false, selected)
        {
            // the head comes back, the keyboard goes to the message field
            search.find = None;
            ui.ctx().memory_mut(|m| m.request_focus(egui::Id::unique(("conv_input", selected))));
        }
        return;
    }
    // the names, up to the actions
    let text_left = pic.right() + 10.0;
    let text_right = (x - 6.0).max(text_left);
    let title_y = if who.subtitle.is_empty() {
        rect.center().y
    } else {
        rect.center().y - gap * 8.0 / LINE_GAP
    };
    // a member of the Aurora team's tag after the name, cut before it
    let room = text_right - text_left;
    let mark = who.avatar.and_then(|id| crate::world::staff::mark(world, &id));
    let staff = crate::ui::staff::width(ui, p, mark, 14.0);
    let staff = if staff > 0.0 && room - staff - 6.0 >= 60.0 {
        staff + 6.0
    } else {
        0.0
    };
    // the full text of what was cut, on hover over the names
    let mut tips = crate::ui::widgets::CutTips::default();
    crate::ui::widgets::paint_elided_tip(
        ui,
        &mut tips,
        &who.title,
        egui::pos2(text_left, title_y),
        room - staff,
        title_font.clone(),
        who.avatar
            .map(|id| crate::ui::colors::resident_name(world, &id, p.ink))
            .unwrap_or(p.ink),
    );
    if staff > 0.0 {
        let shown = ui
            .painter()
            .layout_no_wrap(who.title.clone(), title_font, p.ink)
            .size()
            .x
            .min(room - staff);
        crate::ui::staff::paint_at(ui, ui.painter(), p, mark, egui::pos2(text_left + shown + 6.0, title_y), 14.0);
    }
    if !who.subtitle.is_empty() {
        crate::ui::widgets::paint_elided_tip(
            ui,
            &mut tips,
            &who.subtitle,
            egui::pos2(text_left, rect.center().y + gap * 9.0 / LINE_GAP),
            text_right - text_left,
            egui::FontId::proportional(12.0),
            p.muted_dim,
        );
    }
    let names = egui::Rect::from_x_y_ranges(text_left..=text_right, rect.y_range());
    let r = ui.interact(names, sidebar::control_id("head-names"), egui::Sense::click());
    if let Some(a) = who.avatar {
        super::name_menu(&r, p, world, a);
        let r = tips.show(r.on_hover_cursor(egui::CursorIcon::PointingHand));
        if r.clicked() {
            actions.push(ConvAction::Profile(a));
        }
    } else if who.kind == Some(sidebar::Kind::Group)
        && let Some(g) = st.selected
    {
        let r = tips.show(r.on_hover_cursor(egui::CursorIcon::PointingHand));
        if r.on_hover_text(aurora_i18n::tr!("ui-group-profile-view")).clicked() {
            crate::ui::group_profile::request_open(ui.ctx(), g);
        }
    } else {
        tips.show(r);
    }
}

/// The "…" menu's rows for the buttons the head had no room for, followed by
/// a separator. The offer, the request, the friendship, adding people and
/// closing are rows of that menu already.
pub(super) fn dropped_rows(
    ui: &mut egui::Ui,
    p: &Palette,
    world: &World,
    st: &mut ChatUi,
    actions: &mut Vec<ConvAction>,
    dropped: &[HeadAction],
) {
    use crate::ui::menu;
    let selected = st.selected;
    let mut any = false;
    for action in dropped {
        match (*action, selected) {
            (HeadAction::Call, Some(other)) => {
                any = true;
                if world.call_of(&other).is_some() {
                    if menu::item(ui, p, "phone-filled", aurora_i18n::tr!("ui-call-hang-up")) {
                        actions.push(ConvAction::HangUp);
                    }
                } else if world.calls.invite_for(&other).is_some() {
                    menu::item_if(ui, p, "phone-filled", aurora_i18n::tr!("ui-chat-call-ringing-banner"), false);
                } else if !st.voice_on {
                    menu::item_if(ui, p, "phone-filled", aurora_i18n::tr!("ui-call-voice-off"), false);
                } else {
                    let launched = world.call_launched(&other).is_some();
                    let label = if launched {
                        aurora_i18n::tr!("ui-chat-join-call")
                    } else {
                        aurora_i18n::tr!("common-voice-call")
                    };
                    if menu::item(ui, p, "phone-filled", label) {
                        actions.push(ConvAction::Call(other));
                    }
                }
            }
            (HeadAction::Snooze, Some(other)) => {
                any = true;
                if menu::item(ui, p, super::SNOOZE_ICON, aurora_i18n::tr!("ui-chat-snooze-tip")) {
                    actions.push(ConvAction::Snooze(other));
                }
            }
            (HeadAction::Find, _) => {
                any = true;
                if menu::item(ui, p, "search-filled", aurora_i18n::tr!("ui-chat-find-in-conversation")) {
                    open_find(ui.ctx(), st);
                }
            }
            (HeadAction::People, None) => {
                any = true;
                if menu::item(ui, p, "users", aurora_i18n::tr!("common-nearby-people")) {
                    actions.push(ConvAction::OpenPeople(0));
                }
            }
            (HeadAction::Members, Some(_)) => {
                any = true;
                let label = if st.members_open {
                    aurora_i18n::tr!("ui-chat-hide-members")
                } else {
                    aurora_i18n::tr!("ui-chat-show-members")
                };
                if menu::item(ui, p, "users", label) {
                    st.members_open = !st.members_open;
                }
            }
            (HeadAction::Profile, Some(other)) => {
                any = true;
                if menu::item(ui, p, "user-filled", aurora_i18n::tr!("common-profile")) {
                    actions.push(ConvAction::Profile(other));
                }
            }
            (HeadAction::Poke, Some(other)) => {
                if let Some(b) = crate::ui::poke::button(ui.ctx(), world, &other) {
                    any = true;
                    if menu::item_if(ui, p, crate::ui::poke::ICON, b.tip, b.enabled) {
                        crate::ui::context::request(ui.ctx(), crate::ui::context::CtxAction::Poke(other));
                    }
                }
            }
            _ => {}
        }
    }
    if any {
        menu::separator(ui, p);
    }
}

/// The find bar of the head: opened with its field focused, or closed.
fn toggle_find(ctx: &egui::Context, st: &mut ChatUi, finding: bool) {
    let selected = st.selected;
    let search = st.searches.entry(selected).or_default();
    if finding {
        search.find = None;
    } else {
        search.open_find();
        ctx.memory_mut(|m| m.request_focus(find::field_id(selected)));
    }
}

/// The find bar's menu row (a head without room for its button).
pub(super) fn open_find(ctx: &egui::Context, st: &mut ChatUi) {
    let finding = st.searches.get(&st.selected).is_some_and(|s| s.find.is_some());
    toggle_find(ctx, st, finding);
}

/// A button of the head. The head lists the ones it has, from the right
/// edge (the order they are drawn in), and `head_layout` says which of them
/// it has room for: the others move to its "…" menu (`header::more_items`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HeadAction {
    Close,
    Snooze,
    /// The "…" menu itself: always kept, it holds what the others left.
    More,
    Find,
    /// The nearby chat's People window.
    People,
    /// A group's members' panel.
    Members,
    Profile,
    AddFriend,
    Offer,
    Request,
    Poke,
    AddPeople,
    Call,
}

impl HeadAction {
    /// How long the button stays while the head narrows, the lowest first.
    /// The profile is a click on the picture and the names as well, and the
    /// friendship, the requests and the others are rows of the "…" menu
    /// anyway: they go before the call, the close and the offer.
    fn keep(self) -> u8 {
        match self {
            HeadAction::More => 0,
            HeadAction::Close => 1,
            HeadAction::Call => 2,
            HeadAction::Snooze => 3,
            HeadAction::Offer => 4,
            HeadAction::People | HeadAction::Members => 5,
            HeadAction::Find => 6,
            HeadAction::AddPeople => 7,
            HeadAction::AddFriend => 8,
            HeadAction::Poke => 9,
            HeadAction::Profile => 10,
            HeadAction::Request => 11,
        }
    }
}

/// What the head of `width` shows of its actions, and what goes to its menu.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct HeadLayout {
    /// In the order they were given (the drawing order).
    pub shown: Vec<HeadAction>,
    /// The ones left out, in the same order.
    pub overflow: Vec<HeadAction>,
}

impl HeadLayout {
    pub(super) fn shows(&self, action: HeadAction) -> bool {
        self.shown.contains(&action)
    }
}

/// From a button's left edge to the next one's.
const STEP: f32 = TOOL + 2.0;
/// The right margin, the picture's margin and the picture.
const EDGES: f32 = 8.0 + 12.0 + PIC;
/// Between the picture and the names, and the names and the buttons.
const TEXT_GAP: f32 = 16.0;
/// The characters of the title the head always keeps room for.
const TITLE_CHARS: f32 = 5.0;

/// The width the names keep at the least, for a title font whose line is
/// `row_height` tall (it follows the text size): `TITLE_CHARS` characters
/// of the title, and the gaps around them.
pub(super) fn text_min(row_height: f32) -> f32 {
    TEXT_GAP + TITLE_CHARS * row_height * 0.6
}

/// Whether a head of `width` keeps `text_min` for its names beside the
/// picture and `slots` buttons.
pub(super) fn fits(width: f32, slots: usize, text_min: f32) -> bool {
    EDGES + slots as f32 * STEP + text_min <= width
}

/// The buttons the head of a conversation has, from the right edge.
fn present_actions(world: &World, selected: Option<Uuid>, kind: Option<sidebar::Kind>, friend: bool, poke: bool) -> Vec<HeadAction> {
    let mut list = Vec::new();
    if let Some(other) = selected {
        list.push(HeadAction::Close);
        if super::can_snooze(world, &other) {
            list.push(HeadAction::Snooze);
        }
    }
    list.push(HeadAction::More);
    list.push(HeadAction::Find);
    match (selected, kind) {
        (None, _) => list.push(HeadAction::People),
        (Some(_), Some(k)) if k != sidebar::Kind::Private => list.push(HeadAction::Members),
        (Some(_), _) => {
            list.push(HeadAction::Profile);
            if !friend {
                list.push(HeadAction::AddFriend);
            }
            list.push(HeadAction::Offer);
            list.push(HeadAction::Request);
            if poke {
                list.push(HeadAction::Poke);
            }
        }
    }
    if let Some(other) = selected {
        if can_add_people(world, &other) {
            list.push(HeadAction::AddPeople);
        }
        list.push(HeadAction::Call);
    }
    list
}

/// Which of `actions` (in drawing order) a head of `width` shows, keeping
/// `text_min` for its names: as many as fit, the ones that `keep` longest
/// first, the "…" menu always. The others are `overflow`, for the menu.
/// (It generalises the rule that dropped the teleport request alone.)
pub(super) fn head_layout(width: f32, text_min: f32, actions: &[HeadAction]) -> HeadLayout {
    let mut room = 0;
    while room < actions.len() && fits(width, room + 1, text_min) {
        room += 1;
    }
    let mut order: Vec<usize> = (0..actions.len()).collect();
    order.sort_by_key(|&i| actions[i].keep());
    // the menu is rank 0 and takes one slot even when none is left
    let allowed = room.max(1);
    let mut kept = vec![false; actions.len()];
    for &i in order.iter().take(allowed) {
        kept[i] = true;
    }
    let (mut shown, mut overflow) = (Vec::new(), Vec::new());
    for (i, a) in actions.iter().enumerate() {
        if kept[i] {
            shown.push(*a);
        } else {
            overflow.push(*a);
        }
    }
    HeadLayout { shown, overflow }
}

/// People can be added to a conversation with a resident or a
/// conference, not to a role group's chat (FSFloaterIM::isInviteAllowed:
/// IM_NOTHING_SPECIAL, IM_SESSION_CONFERENCE_START, an invitation that is
/// not a group's).
pub(super) fn can_add_people(world: &World, conversation: &Uuid) -> bool {
    !world.groups.is_member(conversation) && !world.groups.sessions.get(conversation).is_some_and(|s| s.group)
}

/// The look of a head button.
#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Plain,
    /// Lit: its panel or bar is open.
    Active,
    /// Green: the call is in progress.
    Live,
    /// Greyed: it cannot be used now.
    Off,
}

impl Tone {
    fn active(on: bool) -> Tone {
        if on { Tone::Active } else { Tone::Plain }
    }
}

fn tool(ui: &mut egui::Ui, p: &Palette, rect: egui::Rect, icon: &str, tip: &str, tone: Tone) -> egui::Response {
    let id = sidebar::control_id(&format!("head-{icon}"));
    let resp = ui.interact(
        rect,
        id,
        if tone == Tone::Off {
            egui::Sense::hover()
        } else {
            egui::Sense::click()
        },
    );
    let radius = ui.visuals().widgets.inactive.corner_radius;
    if tone == Tone::Active || (resp.hovered() && tone != Tone::Off) {
        ui.painter().rect_filled(rect, radius, p.raised.gamma_multiply(0.7));
    }
    let color: Color32 = match tone {
        Tone::Live => p.success,
        Tone::Off => p.muted_dim.gamma_multiply(0.6),
        Tone::Active => p.ink,
        Tone::Plain if resp.hovered() => p.ink,
        Tone::Plain => p.muted,
    };
    super::thread::paint_icon(
        ui.painter(),
        icon,
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(18.0)),
        color,
    );
    resp.on_hover_text(tip)
}

/// The call button of FSFloaterIM: calls the conversation, green while its
/// call is in progress (a click then hangs up, like "Leave call"), greyed
/// while voice is off.
fn call_tool(ui: &mut egui::Ui, p: &Palette, rect: egui::Rect, world: &World, st: &ChatUi, actions: &mut Vec<ConvAction>, other: Uuid) {
    if world.call_of(&other).is_some() {
        if tool(ui, p, rect, "phone-filled", aurora_i18n::tr!("ui-call-hang-up"), Tone::Live).clicked() {
            actions.push(ConvAction::HangUp);
        }
    } else if world.calls.invite_for(&other).is_some() {
        tool(
            ui,
            p,
            rect,
            "phone-filled",
            aurora_i18n::tr!("ui-chat-call-ringing-banner"),
            Tone::Live,
        );
    } else if !st.voice_on {
        tool(ui, p, rect, "phone-filled", aurora_i18n::tr!("ui-call-voice-off"), Tone::Off);
    } else {
        let tip = if world.call_launched(&other).is_some() {
            aurora_i18n::tr!("ui-chat-join-call")
        } else {
            aurora_i18n::tr!("common-voice-call")
        };
        let tone = if world.call_launched(&other).is_some() {
            Tone::Live
        } else {
            Tone::Plain
        };
        if tool(ui, p, rect, "phone-filled", tip, tone).clicked() {
            actions.push(ConvAction::Call(other));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HeadAction::*;
    use super::*;

    /// The buttons of the heads, in drawing order.
    fn sets() -> Vec<(&'static str, Vec<HeadAction>)> {
        vec![
            ("nearby", vec![More, Find, People]),
            ("group", vec![Close, Snooze, More, Find, Members, AddPeople, Call]),
            ("conference", vec![Close, More, Find, Members, AddPeople, Call]),
            ("friend", vec![Close, More, Find, Profile, Offer, Request, Poke, AddPeople, Call]),
            (
                "stranger",
                vec![Close, More, Find, Profile, AddFriend, Offer, Request, Poke, AddPeople, Call],
            ),
        ]
    }

    /// A title line at the text sizes of the options.
    fn texts() -> [f32; 3] {
        [1.0, 1.5, 2.0].map(|scale| text_min(17.0 * scale))
    }

    #[test]
    fn the_names_keep_their_room_and_the_menu_stays() {
        for (name, set) in sets() {
            for text in texts() {
                for width in (240..=900).step_by(7) {
                    let width = width as f32;
                    let layout = head_layout(width, text, &set);
                    let case = format!("{name} at {width}, names {text}");
                    assert!(layout.shows(More), "{case}: the menu is always there");
                    // the buttons leave the names their room, unless only the menu is left
                    assert!(
                        layout.shown.len() == 1 || fits(width, layout.shown.len(), text),
                        "{case}: {layout:?}"
                    );
                    // nothing is lost: shown and overflow are the set, in order
                    let mut all: Vec<HeadAction> = layout.shown.iter().chain(&layout.overflow).copied().collect();
                    all.sort_by_key(|a| set.iter().position(|s| s == a));
                    assert_eq!(all, set, "{case}");
                    // as many as fit: one more would not
                    assert!(
                        layout.overflow.is_empty() || !fits(width, layout.shown.len() + 1, text),
                        "{case}: room left unused"
                    );
                }
            }
        }
    }

    #[test]
    fn the_least_important_buttons_go_first() {
        for (name, set) in sets() {
            for text in texts() {
                for width in (240..=900).step_by(11) {
                    let layout = head_layout(width as f32, text, &set);
                    let kept = layout.shown.iter().map(|a| a.keep()).max().unwrap_or(0);
                    let dropped = layout.overflow.iter().map(|a| a.keep()).min().unwrap_or(u8::MAX);
                    assert!(kept <= dropped, "{name} at {width}: {layout:?}");
                }
            }
        }
    }

    #[test]
    fn a_wider_head_never_shows_fewer_buttons() {
        for (name, set) in sets() {
            for text in texts() {
                let mut before = 0;
                for width in (240..=900).step_by(3) {
                    let shown = head_layout(width as f32, text, &set).shown.len();
                    assert!(shown >= before, "{name} at {width}");
                    before = shown;
                }
            }
        }
    }

    #[test]
    fn a_roomy_head_shows_everything_and_the_request_goes_first() {
        for (name, set) in sets() {
            let layout = head_layout(900.0, text_min(17.0), &set);
            assert!(layout.overflow.is_empty(), "{name}");
        }
        let (_, stranger) = sets().pop().expect("stranger");
        // one button too many for the width: it is the request's
        let full = stranger.len();
        let width = EDGES + full as f32 * STEP + 90.0 - 1.0;
        let layout = head_layout(width, 90.0, &stranger);
        assert_eq!(layout.overflow, vec![Request]);
    }

    #[test]
    fn the_smallest_window_keeps_the_essentials_of_each_head() {
        // the nearby pop-out at its least holds its three buttons; a little
        // narrower (or at a larger text) the find goes to the menu
        assert!(head_layout(234.0, text_min(17.0), &sets()[0].1).overflow.is_empty());
        let nearby = head_layout(214.0, text_min(17.0), &sets()[0].1);
        assert_eq!(nearby.shown, vec![More, People]);
        assert_eq!(nearby.overflow, vec![Find]);
        // the workspace at its least (300 wide, the rail beside the pane)
        let stranger = head_layout(242.0, text_min(17.0), &sets()[4].1);
        assert_eq!(stranger.shown, vec![Close, More, Call]);
        assert!(stranger.overflow.contains(&Offer), "the offer is a row of the menu");
        // a head narrower than its picture: the menu alone
        assert_eq!(head_layout(60.0, 90.0, &sets()[4].1).shown, vec![More]);
    }

    #[test]
    fn the_names_room_follows_the_text_size() {
        assert!(text_min(17.0 * 2.0) > text_min(17.0 * 1.5));
        assert!(text_min(17.0 * 1.5) > text_min(17.0));
        assert!(text_min(17.0) > TEXT_GAP);
    }

    #[test]
    fn a_window_minimum_follows_the_text_size_and_never_shrinks() {
        let base = egui::vec2(300.0, 240.0);
        let at = |s: f32| crate::ui::widgets::text_min_size(base, s);
        assert_eq!(at(1.0), base);
        assert_eq!(at(0.8), base, "a smaller text keeps the base minimum");
        assert!(at(1.5).x > at(1.0).x && at(2.0).x > at(1.5).x);
        assert!(at(2.0).y > at(1.0).y);
        assert_eq!(at(2.0).x, 600.0);
    }

    #[test]
    fn only_what_was_cut_is_in_the_hover() {
        let mut tips = crate::ui::widgets::CutTips::default();
        assert_eq!(tips.text(), None);
        tips.0.push("29 people in earshot · A long parcel name".to_owned());
        assert_eq!(tips.text().as_deref(), Some("29 people in earshot · A long parcel name"));
        tips.0.push("Name".to_owned());
        assert_eq!(tips.text().as_deref(), Some("29 people in earshot · A long parcel name\nName"));
    }
}
