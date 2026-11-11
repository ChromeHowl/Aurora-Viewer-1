//! The Shadow Realm: residents of a per-account list (anyone, friends or
//! not) who appear nowhere while farther than a distance (World › Shadow
//! Realm…, 10 m by default). An Aurora addition; the Legacy Viewer has none.
//!
//! Out of range, a listed resident is not drawn with what they wear, nor
//! their rezzed objects whose owner is known; they are left out of the
//! minimap, the radar, the nearby lists, the nearby voice list and the
//! picker's "near me"; their local chat and typing are hidden and their
//! nearby (spatial) voice is muted; their objects' chat, sounds, lights,
//! particles, look-at beams, beacons, hover text, media audio, dialogs, URL
//! and permission questions are dropped, as are the instant messages of
//! their objects (an offer of an object is declined without a word). Their
//! own instant messages and private and group calls always come through.
//! In range everything shows again at once and their name is red wherever
//! it is written, unless RLVa hides names.
//!
//! Nothing is ever removed from the world: what is hidden stays loaded and
//! is only left out of what is drawn and listed (the scene flags it like
//! the photo's isolation, `ObjGpu::hidden`), so the resident always comes
//! back; on the way back the scene rebuilds their objects' records and
//! takes a new impostor picture (`take_revealed`). The list lives in the
//! account's folder (`account_ids.rs`), never in the block list, which the
//! server keeps and which stops instant messages.
//!
//! Distance: the avatar's own position when it is in the world, else its
//! coarse location; a coarse location without its height (over 1020 m)
//! counts as out of range. A resident shows within the distance and hides
//! again only past it plus `HYSTERESIS_M`, checked `CHECK_EVERY`. One not
//! checked yet (a new arrival) is hidden. With an empty list or the
//! feature off, every question here is a single test.

use super::World;
use super::account_ids::AccountIds;
use aurora_net::{ChatMessage, ChatSourceType, InstantMessage};
use glam::Vec3;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// The account's file (a JSON array of ids).
pub const FILE: &str = "shadow_realm.json";
/// About four checks a second.
pub const CHECK_EVERY: Duration = Duration::from_millis(250);
/// A resident shown hides again only this far past the distance (m).
pub const HYSTERESIS_M: f32 = 1.0;
/// IM_FROM_TASK (llinstantmessage.h): llInstantMessage of an object, its
/// owner in the sender, the object in the session id.
const IM_FROM_TASK: u8 = 19;

/// Where a listed resident is known to be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seen {
    /// The avatar is in the world: its own position.
    Exact(Vec3),
    /// A coarse location only; `altitude_known` false for a height of 0 or
    /// 1020 m (the byte 0 or 255: the server does not know it).
    Coarse { position: Vec3, altitude_known: bool },
    /// Not around.
    Nowhere,
}

/// The distance to `me`, None when it cannot be known (out of range).
pub fn distance(seen: Seen, me: Vec3) -> Option<f32> {
    match seen {
        Seen::Exact(at) => at.is_finite().then(|| at.distance(me)),
        Seen::Coarse {
            position,
            altitude_known: true,
        } => position.is_finite().then(|| position.distance(me)),
        Seen::Coarse { .. } | Seen::Nowhere => None,
    }
}

/// Whether a listed resident shows after a check: within `range`, or still
/// within `range` + `HYSTERESIS_M` when they showed already.
pub fn next_shown(was_shown: bool, distance: Option<f32>, range: f32) -> bool {
    let Some(d) = distance else {
        return false;
    };
    let limit = if was_shown { range + HYSTERESIS_M } else { range };
    d <= limit
}

/// Whether a voice participant is muted for us: the block list's voice
/// mute anywhere, the Shadow Realm only on the nearby (spatial) channel.
pub fn voice_muted(blocked: bool, spatial: bool, shadow_hidden: bool) -> bool {
    blocked || (spatial && shadow_hidden)
}

#[derive(Debug)]
pub struct ShadowRealm {
    list: AccountIds,
    enabled: bool,
    range: f32,
    /// The listed residents in range at the last check.
    shown: HashSet<Uuid>,
    /// Bumped whenever who is hidden changes: the scene flags its objects
    /// again.
    version: u64,
    /// The next check; None: at the next tick.
    next_check: Option<Instant>,
    /// Residents hidden until now and shown again: the scene refreshes them.
    revealed: Vec<Uuid>,
}

impl Default for ShadowRealm {
    fn default() -> Self {
        let defaults = crate::settings::shadow_realm::ShadowRealm::default();
        Self {
            list: AccountIds::default(),
            enabled: defaults.enabled,
            range: defaults.distance_m,
            shown: HashSet::new(),
            version: 0,
            next_check: None,
            revealed: Vec::new(),
        }
    }
}

impl ShadowRealm {
    /// The account's list, at the login (before anything of the world).
    pub fn load(path: PathBuf) -> Self {
        let list = AccountIds::load(path);
        log::info!("shadow realm: {} resident(s) listed", list.len());
        Self { list, ..Self::default() }
    }

    /// The feature is on and someone is listed.
    #[inline]
    pub fn active(&self) -> bool {
        self.enabled && !self.list.is_empty()
    }

    pub fn is_listed(&self, id: &Uuid) -> bool {
        !self.list.is_empty() && self.list.contains(id)
    }

    /// The resident appears nowhere now.
    #[inline]
    pub fn hides(&self, id: &Uuid) -> bool {
        self.active() && self.list.contains(id) && !self.shown.contains(id)
    }

    /// The resident is listed and in range: their name is red.
    pub fn shows_listed(&self, id: &Uuid) -> bool {
        self.active() && self.shown.contains(id) && self.list.contains(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &Uuid> {
        self.list.ids()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    fn bump(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// The listed residents hidden now (before a change shows them all).
    fn hidden_now(&self) -> Vec<Uuid> {
        if !self.active() {
            return Vec::new();
        }
        self.list.ids().filter(|id| !self.shown.contains(id)).copied().collect()
    }

    /// The settings of this frame; a change applies at once.
    pub fn configure(&mut self, enabled: bool, range: f32) {
        if enabled != self.enabled {
            if !enabled {
                let back = self.hidden_now();
                self.revealed.extend(back);
            }
            self.enabled = enabled;
            self.shown.clear();
            self.next_check = None;
            self.bump();
        }
        if range != self.range {
            self.range = range;
            self.next_check = None;
        }
    }

    /// Adds a resident (saved at once): hidden from now on, until a check
    /// finds them in range. Never ourselves.
    pub fn add(&mut self, id: Uuid, me: Uuid) -> bool {
        if id == me || !self.list.insert(id) {
            return false;
        }
        self.shown.remove(&id);
        self.next_check = None;
        self.bump();
        true
    }

    /// Removes a resident (saved at once): they show again at once.
    pub fn remove(&mut self, id: &Uuid) -> bool {
        let hidden = self.hides(id);
        if !self.list.remove(id) {
            return false;
        }
        self.shown.remove(id);
        if hidden {
            self.revealed.push(*id);
        }
        self.bump();
        true
    }

    /// A check when one is due: who of the list is in range of `me`, with
    /// `locate` telling where each is. True when someone appeared or hid.
    pub fn tick(&mut self, now: Instant, me: Vec3, locate: impl Fn(&Uuid) -> Seen) -> bool {
        if !self.active() {
            return false;
        }
        if self.next_check.is_some_and(|at| now < at) {
            return false;
        }
        self.next_check = Some(now + CHECK_EVERY);
        let mut changed = false;
        let ids: Vec<Uuid> = self.list.ids().copied().collect();
        for id in ids {
            let was = self.shown.contains(&id);
            let shown = next_shown(was, distance(locate(&id), me), self.range);
            if shown == was {
                continue;
            }
            changed = true;
            if shown {
                self.shown.insert(id);
                self.revealed.push(id);
            } else {
                self.shown.remove(&id);
            }
        }
        if changed {
            self.bump();
        }
        changed
    }

    /// The residents shown again since the last call (the scene's refresh).
    pub fn take_revealed(&mut self) -> Vec<Uuid> {
        std::mem::take(&mut self.revealed)
    }
}

impl World {
    /// The account's list, read at the login before any object arrives.
    pub(super) fn load_shadow_realm(&mut self, account: &std::path::Path) {
        let (enabled, range) = (self.shadow_realm.enabled, self.shadow_realm.range);
        self.shadow_realm = ShadowRealm::load(account.join(FILE));
        self.shadow_realm.configure(enabled, range);
        self.shadow_realm.bump();
    }

    /// Per frame: the settings, then the check when due.
    pub fn tick_shadow_realm(&mut self, cfg: &crate::settings::shadow_realm::ShadowRealm, now: Instant) {
        self.shadow_realm.configure(cfg.enabled, cfg.distance_m);
        if !self.shadow_realm.active() {
            return;
        }
        let me = self.agent.position;
        let mut realm = std::mem::take(&mut self.shadow_realm);
        realm.tick(now, me, |id| self.shadow_realm_seen(id));
        self.shadow_realm = realm;
    }

    /// Where a resident is: their avatar, else their coarse location.
    fn shadow_realm_seen(&self, id: &Uuid) -> Seen {
        if let Some(o) = self
            .objects
            .index_of_uuid(id)
            .and_then(|i| self.objects.get(i))
            .filter(|o| o.is_avatar())
            && let Some(at) = super::radar::native_position(self, o)
        {
            return Seen::Exact(at);
        }
        for region in self.coarse.keys() {
            if let Some((_, position, altitude_known)) = super::radar::coarse_in(self, *region).find(|(other, ..)| other == id) {
                return Seen::Coarse { position, altitude_known };
            }
        }
        Seen::Nowhere
    }

    /// Adds a resident to the Shadow Realm (World menu, avatar menus).
    pub fn shadow_realm_add(&mut self, id: Uuid) -> bool {
        let added = self.shadow_realm.add(id, self.agent_id);
        if added {
            log::info!("shadow realm: {id} added");
        }
        added
    }

    /// Takes a resident out of the Shadow Realm: they show at once.
    pub fn shadow_realm_remove(&mut self, id: &Uuid) -> bool {
        let removed = self.shadow_realm.remove(id);
        if removed {
            log::info!("shadow realm: {id} removed");
        }
        removed
    }

    /// The resident appears nowhere now.
    #[inline]
    pub fn shadow_hides(&self, id: &Uuid) -> bool {
        self.shadow_realm.hides(id)
    }

    /// The resident's name is written in red: listed, in range, and named
    /// (never while RLVa hides their name).
    pub fn shadow_red(&self, id: &Uuid) -> bool {
        self.shadow_realm.shows_listed(id) && self.rlva_can_name(*id)
    }

    /// The object at `index` belongs to someone the Shadow Realm hides now:
    /// worn by them (up to the avatar), or a prim of theirs (an owner the
    /// object's update named).
    pub fn shadow_hides_index(&self, index: usize) -> bool {
        if !self.shadow_realm.active() {
            return false;
        }
        let mut index = index;
        for _ in 0..64 {
            let Some(o) = self.objects.get(index) else {
                return false;
            };
            if o.is_avatar() {
                return self.shadow_realm.hides(&o.full_id);
            }
            if !o.owner_id.is_nil() && self.shadow_realm.hides(&o.owner_id) {
                return true;
            }
            if o.parent_id == 0 {
                return false;
            }
            let Some(parent) = self.objects.parent_of(o) else {
                return false;
            };
            index = parent;
        }
        false
    }

    /// Something said, played or asked by `object` (owned by `owner` when
    /// the message names one) comes from someone hidden: the owner, or the
    /// object's owner or wearer as the world knows them.
    pub fn shadow_hides_source(&self, object: &Uuid, owner: &Uuid) -> bool {
        if !self.shadow_realm.active() {
            return false;
        }
        if (!owner.is_nil() && self.shadow_realm.hides(owner)) || self.shadow_realm.hides(object) {
            return true;
        }
        self.objects.index_of_uuid(object).is_some_and(|i| self.shadow_hides_index(i))
    }

    /// Nearby chat of a hidden resident or of their objects.
    pub(super) fn shadow_hides_chat(&self, c: &ChatMessage) -> bool {
        match c.source_type {
            ChatSourceType::System => false,
            ChatSourceType::Agent => self.shadow_hides(&c.source_id),
            ChatSourceType::Object => self.shadow_hides_source(&c.source_id, &c.owner_id),
        }
    }

    /// An instant message of a hidden resident's object is dropped; an
    /// object of theirs offering an item is declined, without a word. Their
    /// own messages and offers, and group messages, come through.
    pub(super) fn shadow_drops_im(&mut self, im: &InstantMessage) -> bool {
        use super::notifications::im as d;
        if !self.shadow_realm.active() || im.from_agent_id.is_nil() {
            return false;
        }
        let from_object = im.dialog == IM_FROM_TASK;
        let object_offer = im.dialog == d::TASK_INVENTORY_OFFERED;
        if !(from_object || object_offer) || im.from_group {
            return false;
        }
        let hidden = self.shadow_hides(&im.from_agent_id) || (from_object && self.shadow_hides_source(&im.session_id, &Uuid::nil()));
        if !hidden {
            return false;
        }
        if object_offer {
            // closing the offer (LLOfferInfo IOR_DECLINE), nothing shown
            self.groups.out.push(super::notifications::inventory_answer(
                im.from_agent_id,
                im.session_id,
                true,
                false,
                Uuid::nil(),
            ));
        }
        log::debug!(
            "shadow realm: object message (dialog {}) of {} dropped",
            im.dialog,
            im.from_agent_id
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aurora-shadow-realm-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        dir
    }

    const ME: Uuid = Uuid::from_u128(99);

    fn realm(listed: &[u128]) -> ShadowRealm {
        let mut r = ShadowRealm::default();
        for n in listed {
            assert!(r.add(id(*n), ME));
        }
        r
    }

    #[test]
    fn the_list_is_kept_per_account_in_a_local_file() {
        let dir = temp_dir();
        let (a, b) = (dir.join("account-a").join(FILE), dir.join("account-b").join(FILE));
        let mut first = ShadowRealm::load(a.clone());
        assert!(first.add(id(1), ME));
        assert!(!first.add(id(1), ME), "listed once");
        assert!(!first.add(ME, ME), "never ourselves");
        assert!(!first.add(Uuid::nil(), ME));
        let mut second = ShadowRealm::load(b.clone());
        assert!(second.add(id(2), ME));
        assert_eq!(ShadowRealm::load(a.clone()).ids().copied().collect::<Vec<_>>(), vec![id(1)]);
        assert_eq!(ShadowRealm::load(b).ids().copied().collect::<Vec<_>>(), vec![id(2)]);
        assert!(first.remove(&id(1)));
        assert_eq!(ShadowRealm::load(a).ids().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The list never reaches the block list nor the server.
    #[test]
    fn adding_and_removing_send_nothing_and_leave_the_block_list_alone() {
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        world.agent_id = ME;
        assert!(world.shadow_realm_add(id(1)));
        assert!(world.shadow_realm_remove(&id(1)));
        assert!(world.shadow_realm_add(id(1)));
        assert!(world.take_social_commands().is_empty());
        assert!(!world.mutes.is_muted_id(&id(1)));
        assert!(!world.is_avatar_blocked(&id(1)));
    }

    #[test]
    fn hysteresis_shows_within_the_distance_and_hides_past_one_more_metre() {
        assert!(next_shown(false, Some(10.0), 10.0));
        assert!(!next_shown(false, Some(10.5), 10.0), "not shown yet: the distance itself");
        assert!(next_shown(true, Some(10.5), 10.0), "shown: kept up to 11 m");
        assert!(next_shown(true, Some(11.0), 10.0));
        assert!(!next_shown(true, Some(11.01), 10.0));
        assert!(!next_shown(true, None, 10.0));
        let mut r = realm(&[1]);
        let t0 = Instant::now();
        let at = |d: f32| move |_: &Uuid| Seen::Exact(Vec3::new(d, 0.0, 0.0));
        r.tick(t0, Vec3::ZERO, at(10.5));
        assert!(r.hides(&id(1)));
        r.tick(t0 + CHECK_EVERY, Vec3::ZERO, at(9.0));
        assert!(!r.hides(&id(1)) && r.shows_listed(&id(1)));
        r.tick(t0 + CHECK_EVERY * 2, Vec3::ZERO, at(10.9));
        assert!(!r.hides(&id(1)), "inside the extra metre");
        r.tick(t0 + CHECK_EVERY * 3, Vec3::ZERO, at(11.5));
        assert!(r.hides(&id(1)));
    }

    #[test]
    fn checks_come_about_four_times_a_second() {
        let mut r = realm(&[1]);
        let t0 = Instant::now();
        let near = |_: &Uuid| Seen::Exact(Vec3::ONE);
        assert!(r.tick(t0, Vec3::ZERO, near));
        assert!(!r.hides(&id(1)));
        let far = |_: &Uuid| Seen::Nowhere;
        assert!(!r.tick(t0 + Duration::from_millis(100), Vec3::ZERO, far), "not due yet");
        assert!(!r.hides(&id(1)));
        assert!(r.tick(t0 + CHECK_EVERY, Vec3::ZERO, far));
        assert!(r.hides(&id(1)));
    }

    #[test]
    fn the_exact_position_wins_then_the_coarse_one_and_an_unknown_height_hides() {
        let me = Vec3::new(100.0, 100.0, 30.0);
        assert_eq!(distance(Seen::Exact(Vec3::new(103.0, 104.0, 30.0)), me), Some(5.0));
        let coarse = |z, known| Seen::Coarse {
            position: Vec3::new(103.0, 104.0, z),
            altitude_known: known,
        };
        assert_eq!(distance(coarse(30.0, true), me), Some(5.0));
        assert_eq!(distance(coarse(1020.0, false), me), None, "over 1020 m: the height is not known");
        assert_eq!(distance(coarse(0.0, false), me), None);
        assert_eq!(distance(Seen::Nowhere, me), None);
        assert_eq!(distance(Seen::Exact(Vec3::NAN), me), None);
        // the world: the avatar's own place first, else the coarse location
        let (mut world, handle) = crate::world::derender::tests_support::demo_world();
        let wolf = crate::demo::DEMO_LOUP;
        let exact = match world.shadow_realm_seen(&wolf) {
            Seen::Exact(at) => at,
            other => panic!("the wolf is in the world: {other:?}"),
        };
        world.coarse.insert(handle, vec![(wolf, Vec3::new(1.0, 1.0, 1020.0))]);
        assert_eq!(
            world.shadow_realm_seen(&wolf),
            Seen::Exact(exact),
            "the exact place beats the coarse one"
        );
        let stranger = id(77);
        world.coarse.insert(handle, vec![(stranger, Vec3::new(1.0, 1.0, 1020.0))]);
        assert!(matches!(
            world.shadow_realm_seen(&stranger),
            Seen::Coarse { altitude_known: false, .. }
        ));
        assert_eq!(world.shadow_realm_seen(&id(78)), Seen::Nowhere);
    }

    #[test]
    fn a_new_arrival_starts_hidden_and_shows_at_its_first_check() {
        let mut r = realm(&[1]);
        assert!(r.hides(&id(1)), "before any check");
        r.tick(Instant::now(), Vec3::ZERO, |_| Seen::Exact(Vec3::ONE));
        assert!(!r.hides(&id(1)));
        assert_eq!(r.take_revealed(), vec![id(1)]);
        // a resident added while near: hidden until the next tick, which is due at once
        assert!(r.add(id(2), ME));
        assert!(r.hides(&id(2)));
        r.tick(Instant::now(), Vec3::ZERO, |_| Seen::Exact(Vec3::ONE));
        assert!(!r.hides(&id(2)));
    }

    #[test]
    fn an_empty_list_costs_nothing() {
        let mut r = ShadowRealm::default();
        let version = r.version();
        assert!(!r.active());
        let checked = std::cell::Cell::new(0);
        assert!(!r.tick(Instant::now(), Vec3::ZERO, |_| {
            checked.set(checked.get() + 1);
            Seen::Nowhere
        }));
        assert_eq!(checked.get(), 0, "nothing located");
        assert!(!r.hides(&id(1)) && !r.shows_listed(&id(1)));
        assert_eq!(r.version(), version, "the scene has nothing to flag again");
        assert!(r.next_check.is_none());
        // and in the world
        let (world, _) = crate::world::derender::tests_support::demo_world();
        let wolf = world.objects.index_of_uuid(&crate::demo::DEMO_LOUP).expect("wolf");
        assert!(!world.shadow_hides_index(wolf));
        assert!(!world.shadow_hides_source(&crate::demo::wolf_hat(), &Uuid::nil()));
    }

    #[test]
    fn switching_off_or_removing_shows_at_once_and_reveals() {
        let mut r = realm(&[1, 2]);
        r.tick(Instant::now(), Vec3::ZERO, |_| Seen::Nowhere);
        assert!(r.hides(&id(1)) && r.hides(&id(2)));
        let version = r.version();
        r.configure(false, 10.0);
        assert!(!r.hides(&id(1)) && !r.hides(&id(2)));
        assert!(r.version() != version);
        let mut back = r.take_revealed();
        back.sort();
        assert_eq!(back, vec![id(1), id(2)]);
        r.configure(true, 10.0);
        assert!(r.hides(&id(1)), "on again: hidden until checked");
        let version = r.version();
        assert!(r.remove(&id(1)));
        assert!(!r.hides(&id(1)) && r.version() != version);
        assert_eq!(r.take_revealed(), vec![id(1)]);
    }

    #[test]
    fn a_new_distance_is_checked_at_once() {
        let mut r = realm(&[1]);
        let t0 = Instant::now();
        let at = |_: &Uuid| Seen::Exact(Vec3::new(30.0, 0.0, 0.0));
        r.tick(t0, Vec3::ZERO, at);
        assert!(r.hides(&id(1)));
        r.configure(true, 40.0);
        r.tick(t0 + Duration::from_millis(1), Vec3::ZERO, at);
        assert!(!r.hides(&id(1)));
    }

    #[test]
    fn what_they_wear_and_own_hides_with_them_and_comes_back() {
        let (mut world, _) = crate::world::derender::tests_support::demo_world();
        world.agent_id = ME;
        let wolf = crate::demo::DEMO_LOUP;
        let hat = crate::demo::wolf_hat();
        let wolf_idx = world.objects.index_of_uuid(&wolf).expect("wolf");
        let hat_idx = world.objects.index_of_uuid(&hat).expect("hat");
        let objects = world.objects.slots.iter().flatten().count();
        world.shadow_realm_add(wolf);
        assert!(world.shadow_hides_index(wolf_idx) && world.shadow_hides_index(hat_idx));
        assert!(world.shadow_hides_source(&hat, &Uuid::nil()), "a worn object's sound or dialog");
        // a prim of theirs, its owner named by its update
        let prim = world
            .objects
            .slots
            .iter()
            .flatten()
            .find(|o| !o.is_avatar() && o.parent_id == 0)
            .map(|o| o.key)
            .expect("a rezzed prim");
        let prim_idx = world.objects.index_of(&prim).expect("prim");
        assert!(!world.shadow_hides_index(prim_idx), "owner unknown: shown");
        world.objects.get_mut_untracked(prim_idx).expect("prim").owner_id = wolf;
        assert!(world.shadow_hides_index(prim_idx));
        // never removed: everything is still in the world
        assert_eq!(world.objects.slots.iter().flatten().count(), objects);
        world.shadow_realm_remove(&wolf);
        assert!(!world.shadow_hides_index(wolf_idx) && !world.shadow_hides_index(hat_idx) && !world.shadow_hides_index(prim_idx));
        assert_eq!(world.shadow_realm.take_revealed(), vec![wolf]);
    }

    fn chat(source_type: ChatSourceType, source: Uuid, owner: Uuid) -> ChatMessage {
        ChatMessage {
            from_name: "Resident".to_owned(),
            source_id: source,
            owner_id: owner,
            source_type,
            chat_type: aurora_net::ChatType::Normal,
            position: Vec3::ZERO,
            message: "hello".to_owned(),
        }
    }

    #[test]
    fn their_chat_and_their_objects_chat_are_hidden_out_of_range() {
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        world.agent_id = ME;
        world.shadow_realm_add(id(1));
        assert!(world.shadow_hides_chat(&chat(ChatSourceType::Agent, id(1), id(1))));
        assert!(
            world.shadow_hides_chat(&chat(ChatSourceType::Object, id(50), id(1))),
            "by the owner the chat names"
        );
        assert!(!world.shadow_hides_chat(&chat(ChatSourceType::Agent, id(2), id(2))));
        assert!(!world.shadow_hides_chat(&chat(ChatSourceType::System, id(1), id(1))));
        assert!(world.nearby_typing_hidden(&id(1)));
        world.nearby_typing.insert(id(1));
        assert!(!world.is_typing_nearby(&id(1)));
        assert!(world.sound_blocked(&id(50), &id(1)));
        assert!(world.particles_blocked(&id(1)));
        // in range: everything back
        world.shadow_realm.tick(Instant::now(), Vec3::ZERO, |_| Seen::Exact(Vec3::ONE));
        assert!(!world.shadow_hides_chat(&chat(ChatSourceType::Agent, id(1), id(1))));
        assert!(world.is_typing_nearby(&id(1)));
        assert!(!world.sound_blocked(&id(50), &id(1)));
    }

    fn im(dialog: u8, from: Uuid, session: Uuid) -> InstantMessage {
        InstantMessage {
            from_agent_id: from,
            from_name: "Resident".into(),
            to_agent_id: ME,
            dialog,
            session_id: session,
            message: "hello".into(),
            offline: false,
            from_group: false,
            binary_bucket: Vec::new(),
            timestamp: 0,
        }
    }

    #[test]
    fn their_messages_and_calls_always_come_through_and_their_objects_messages_do_not() {
        use super::super::notifications::im as d;
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        world.agent_id = ME;
        world.shadow_realm_add(id(1));
        assert!(world.shadow_hides(&id(1)));
        // their own messages, offers, teleport offers and group messages
        for dialog in [0, d::INVENTORY_OFFERED, d::LURE_USER, d::FRIENDSHIP_OFFERED] {
            assert!(!world.shadow_drops_im(&im(dialog, id(1), id(5))), "dialog {dialog}");
        }
        let mut group = im(IM_FROM_TASK, id(1), id(5));
        group.from_group = true;
        assert!(!world.shadow_drops_im(&group));
        world.on_im(im(0, id(1), id(5)));
        assert!(
            world.social.ims.iter().any(|s| s.other == id(1)),
            "the message is in its conversation"
        );
        // their object's message: dropped, nothing sent
        assert!(world.shadow_drops_im(&im(IM_FROM_TASK, id(1), id(50))));
        assert!(world.take_social_commands().is_empty());
        // their object's offer: declined quietly
        assert!(world.shadow_drops_im(&im(d::TASK_INVENTORY_OFFERED, id(1), id(60))));
        let sent = world.take_social_commands();
        assert_eq!(sent.len(), 1);
        assert!(
            matches!(&sent[0], aurora_net::NetCommand::SendImDialog { to, dialog, .. } if *to == id(1) && *dialog == d::TASK_INVENTORY_DECLINED)
        );
        // a stranger's object is untouched
        assert!(!world.shadow_drops_im(&im(IM_FROM_TASK, id(2), id(51))));
        // in range: their objects speak again
        world.shadow_realm.tick(Instant::now(), Vec3::ZERO, |_| Seen::Exact(Vec3::ONE));
        assert!(!world.shadow_drops_im(&im(IM_FROM_TASK, id(1), id(50))));
    }

    #[test]
    fn voice_is_muted_on_the_nearby_channel_only() {
        assert!(voice_muted(false, true, true), "nearby voice of someone hidden");
        assert!(!voice_muted(false, false, true), "a private or group call");
        assert!(!voice_muted(false, true, false));
        assert!(voice_muted(true, false, false), "the block list still mutes everywhere");
    }

    #[test]
    fn the_red_name_waits_while_rlva_hides_names() {
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        world.agent_id = ME;
        world.shadow_realm_add(id(1));
        assert!(!world.shadow_red(&id(1)), "hidden: nothing to colour");
        world.shadow_realm.tick(Instant::now(), Vec3::ZERO, |_| Seen::Exact(Vec3::ONE));
        assert!(world.shadow_red(&id(1)));
        assert!(!world.shadow_red(&id(2)), "not listed");
        world.rlva.configure(true);
        let restriction = ChatMessage {
            chat_type: aurora_net::ChatType::OwnerSay,
            message: "@shownames=n".to_owned(),
            ..chat(ChatSourceType::Object, id(40), ME)
        };
        assert!(world.rlva.receive(&restriction));
        assert!(!world.rlva_can_name(id(1)));
        assert!(!world.shadow_red(&id(1)), "RLVa wins");
    }
}
