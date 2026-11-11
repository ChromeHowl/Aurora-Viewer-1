//! "People in region" beside a place shown outside the world map (place
//! details, About Land, search, teleport history, landmarks, favorites, the
//! mini-map's tooltip).
//!
//! The figure is the Region Tracker's (`region_tracker::status_at`) and the
//! requests are the world map's (`WorldMap::want_named_region`,
//! `want_agents`, `want_regions`, each already limited to once a minute per
//! region). This module only decides which of the regions on screen is asked
//! next, in two lanes that never wait for each other:
//!
//! - the block lane finds a region: its name (MapNameRequest) or its 16 x 16
//!   block of the grid (MapBlockRequest), for a region the map does not know
//!   yet or that is down;
//! - the item lane counts the avatars of a region the map knows
//!   (MapItemRequest).
//!
//! Each lane sends a few requests at once, then about one a second (`Lane`).
//! A single shared budget made a long list fill slowly: every unknown row
//! needed its block before its count, and both waited in the same queue.
//! The block lane is served first in each pass, and its answers move a row
//! to the item lane. Only the rows drawn in the last second are asked,
//! those drawn now first; nothing is asked for a row scrolled away.
//! An avatar count is never asked for a region the map has not named, so
//! the answers never make a nameless region (`WorldMap::apply_blocks` keeps
//! named regions only, `apply_items` adds no region).
//!
//! The world map's own labels do not go through the lanes: the map asks the
//! regions it shows itself, as the Legacy Viewer's (`World::people_on_map`).

use super::region_tracker::status_at;
use super::worldmap::WorldMap;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The block lane: this many requests at once, then one a second.
pub const BLOCK_BURST: u32 = 4;
pub const BLOCK_EVERY: Duration = Duration::from_secs(1);
/// The item lane: the same shape, on its own budget.
pub const ITEM_BURST: u32 = 4;
pub const ITEM_EVERY: Duration = Duration::from_secs(1);
/// A region counts as on screen for this long after it was last drawn.
const VISIBLE_FOR: Duration = Duration::from_secs(1);
/// Drawn this recently: on screen now, asked before the rows still counted
/// as visible but scrolled past.
const SHOWN_NOW: Duration = Duration::from_millis(250);
/// Asked and still no figure after this long: shown as unknown.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
/// A region not drawn for this long is forgotten.
const FORGET_AFTER: Duration = Duration::from_secs(120);
/// A name the map does not know yet is searched again in its regions at
/// most this often (the search reads every known region).
const RESOLVE_EVERY: Duration = Duration::from_millis(500);

/// A request lane: `burst` requests at once, then one every `every` (a
/// token bucket, full at the start).
#[derive(Debug, Clone, Copy)]
pub struct Lane {
    burst: u32,
    every: Duration,
    tokens: f64,
    at: Option<Instant>,
}

impl Lane {
    pub const fn new(burst: u32, every: Duration) -> Self {
        Self {
            burst,
            every,
            tokens: burst as f64,
            at: None,
        }
    }

    fn refill(&mut self, now: Instant) {
        if let Some(at) = self.at {
            let earned = now.saturating_duration_since(at).as_secs_f64() / self.every.as_secs_f64().max(1e-3);
            self.tokens = (self.tokens + earned).min(f64::from(self.burst));
        }
        self.at = Some(now);
    }

    /// Whether a request may go now.
    pub fn ready(&mut self, now: Instant) -> bool {
        self.refill(now);
        self.tokens >= 1.0
    }

    /// A request went.
    pub fn spend(&mut self) {
        self.tokens = (self.tokens - 1.0).max(0.0);
    }
}

/// What a surface shows for a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum People {
    /// Not answered yet: an ellipsis.
    Waiting,
    /// No answer in `ANSWER_TIMEOUT`: a question mark.
    Unknown,
    Count(u32),
}

impl People {
    /// The figure as the dense lists write it.
    pub fn figure(self) -> String {
        match self {
            People::Waiting => "…".to_owned(),
            People::Unknown => "?".to_owned(),
            People::Count(n) => aurora_i18n::integer(i64::from(n)),
        }
    }

    /// "People in region: 12".
    pub fn label(self) -> String {
        aurora_i18n::tr!("people-region-label", count = self.figure())
    }
}

/// A region as a surface knows it: by name (lowercase) or by its place on
/// the grid (region units).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Target {
    Name(String),
    Grid(u32, u32),
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    /// Last drawn.
    seen: Instant,
    /// The region's corner on the map once found, and when it was last
    /// searched.
    key: Option<(u32, u32)>,
    resolved: Option<Instant>,
    /// The first request sent since the last figure.
    asked: Option<Instant>,
    /// The last request of each lane, for the turn of each region.
    block_served: Option<Instant>,
    item_served: Option<Instant>,
}

pub struct RegionPeople {
    /// Behind a lock as `Names::ask`: the windows read the world without
    /// changing it, and drawing a row is what asks for its region.
    entries: Mutex<HashMap<Target, Entry>>,
    blocks: Lane,
    items: Lane,
}

impl Default for RegionPeople {
    fn default() -> Self {
        Self {
            entries: Mutex::default(),
            blocks: Lane::new(BLOCK_BURST, BLOCK_EVERY),
            items: Lane::new(ITEM_BURST, ITEM_EVERY),
        }
    }
}

fn resolve(map: &WorldMap, target: &Target, entry: &mut Entry, now: Instant) -> Option<(u32, u32)> {
    let still = entry.key.is_some_and(|k| match target {
        Target::Name(name) => map.sims.get(&k).is_some_and(|s| s.name.eq_ignore_ascii_case(name)),
        Target::Grid(..) => map.sims.contains_key(&k),
    });
    if still {
        return entry.key;
    }
    if entry.resolved.is_some_and(|t| now.duration_since(t) < RESOLVE_EVERY) {
        return None;
    }
    entry.resolved = Some(now);
    entry.key = match target {
        Target::Name(name) => map.region_by_name(name),
        Target::Grid(x, y) => map.sim_at(*x, *y).map(|(k, _)| k),
    };
    entry.key
}

impl RegionPeople {
    /// The region `name` is on screen: its figure, asked for if needed.
    pub fn by_name(&self, map: &WorldMap, name: &str, own_region: Option<(u32, u32)>, now: Instant) -> Option<People> {
        let lower = name.trim().to_lowercase();
        (!lower.is_empty()).then(|| self.read(map, Target::Name(lower), own_region, now))
    }

    /// The region holding the grid slot (region units) is on screen.
    pub fn by_grid(&self, map: &WorldMap, grid: (u32, u32), own_region: Option<(u32, u32)>, now: Instant) -> People {
        self.read(map, Target::Grid(grid.0, grid.1), own_region, now)
    }

    fn read(&self, map: &WorldMap, target: Target, own_region: Option<(u32, u32)>, now: Instant) -> People {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut entry = entries.get(&target).copied().unwrap_or(Entry {
            seen: now,
            key: None,
            resolved: None,
            asked: None,
            block_served: None,
            item_served: None,
        });
        entry.seen = now;
        let key = resolve(map, &target, &mut entry, now);
        let people = match status_at(map, key, own_region).agents {
            Some(n) => {
                entry.asked = None;
                People::Count(n)
            }
            None if entry.asked.is_some_and(|t| now.duration_since(t) >= ANSWER_TIMEOUT) => People::Unknown,
            None => People::Waiting,
        };
        entries.insert(target, entry);
        people
    }

    /// Queue the requests due for the regions on screen, the block lane
    /// first: a region the map does not know, or that is down, by name (or
    /// by its block of the grid) for its state; then the item lane: a region
    /// the map knows, for its avatars. In each lane the rows drawn now come
    /// first, then the one served longest ago.
    pub fn pump(&mut self, map: &mut WorldMap, now: Instant) {
        let entries = self.entries.get_mut().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, e| now.duration_since(e.seen) < FORGET_AFTER);
        let mut visible: Vec<Target> = entries
            .iter()
            .filter(|(_, e)| now.duration_since(e.seen) < VISIBLE_FOR)
            .map(|(t, _)| t.clone())
            .collect();
        if visible.is_empty() {
            return;
        }
        // a stable order between equals (a HashMap's is not)
        visible.sort();
        // what each row needs now: its block, or its count
        let mut blocks = Vec::new();
        let mut items = Vec::new();
        for target in visible {
            let Some(entry) = entries.get_mut(&target) else { continue };
            let key = resolve(map, &target, entry, now);
            let up = key.filter(|k| map.sims.get(k).is_some_and(|s| !s.is_down()));
            let late = now.duration_since(entry.seen) >= SHOWN_NOW;
            match up {
                Some(k) => items.push((late, entry.item_served, target, k)),
                None => blocks.push((late, entry.block_served, target, key)),
            }
        }
        blocks.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        items.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        for (_, _, target, key) in blocks {
            if !self.blocks.ready(now) {
                break;
            }
            let down = key.and_then(|k| map.sims.get(&k)).map(|s| s.name.clone());
            let sent = match (down, &target) {
                (Some(name), _) => map.want_named_region_at(&name, now),
                (None, Target::Name(name)) => map.want_named_region_at(name, now),
                (None, Target::Grid(x, y)) => {
                    let (x, y) = (i64::from(*x), i64::from(*y));
                    map.want_regions_at(x, y, x, y, now)
                }
            };
            if sent {
                self.blocks.spend();
                if let Some(entry) = entries.get_mut(&target) {
                    entry.block_served = Some(now);
                    entry.asked.get_or_insert(now);
                }
            }
        }
        for (_, _, target, key) in items {
            if !self.items.ready(now) {
                break;
            }
            // only a region the map named: an answer never makes a nameless one
            if map.sims.get(&key).is_none_or(|s| s.name.is_empty()) {
                continue;
            }
            if map.want_agents_at(key, now) {
                self.items.spend();
                if let Some(entry) = entries.get_mut(&target) {
                    // the wait for the figure starts with its own request,
                    // not with the block request that found the region
                    let from_items = entry.asked.filter(|a| entry.item_served.is_some_and(|i| *a <= i));
                    entry.asked = Some(from_items.unwrap_or(now));
                    entry.item_served = Some(now);
                }
            }
        }
    }
}

impl super::World {
    fn own_grid(&self) -> Option<(u32, u32)> {
        self.main_origin().map(|(x, y)| (x / 256, y / 256))
    }

    /// "People in region" for a place shown by its region's name; None when
    /// there is nothing to show: no name, or RLVa @showloc, which hides the
    /// figure wherever a place is shown (and asks nothing).
    pub fn people_in_region(&self, name: &str) -> Option<People> {
        if self.rlva.has("showloc") {
            return None;
        }
        self.region_people.by_name(&self.map, name, self.own_grid(), Instant::now())
    }

    /// The same for a place known by its global position (metres).
    pub fn people_at_global(&self, x: f64, y: f64) -> Option<People> {
        if self.rlva.has("showloc") || x < 0.0 || y < 0.0 {
            return None;
        }
        let grid = ((x / 256.0) as u32, (y / 256.0) as u32);
        Some(self.region_people.by_grid(&self.map, grid, self.own_grid(), Instant::now()))
    }

    /// The figure of a region the world map shows, read without joining
    /// the lanes: the map asks the regions it shows itself, each once a
    /// minute, as the Legacy Viewer's (LLWorldMapView::draw calling
    /// LLSimInfo::updateAgentCount), so its many regions never take the
    /// turn of a list's rows. The same rule as everywhere (the Region
    /// Tracker's), and nothing under RLVa @showloc.
    pub fn people_on_map(&self, key: (u32, u32)) -> Option<People> {
        if self.rlva.has("showloc") {
            return None;
        }
        Some(
            status_at(&self.map, Some(key), self.own_grid())
                .agents
                .map_or(People::Waiting, People::Count),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::region_tracker::status;
    use crate::world::worldmap::{SIM_ACCESS_DOWN, SIM_ACCESS_PG, SimInfo};
    use aurora_net::NetCommand;
    use uuid::Uuid;

    fn sim(name: &str) -> SimInfo {
        SimInfo {
            name: name.into(),
            access: SIM_ACCESS_PG,
            flags: 0,
            image: Uuid::nil(),
            size_x: 256,
            size_y: 256,
        }
    }

    fn secs(t0: Instant, s: f32) -> Instant {
        t0 + Duration::from_secs_f32(s)
    }

    #[test]
    fn the_figure_is_the_region_trackers() {
        let mut map = WorldMap::default();
        let people = RegionPeople::default();
        let t0 = Instant::now();
        map.sims.insert((10, 20), sim("Ahern"));
        map.sims.insert((11, 20), sim("Morris"));
        map.sims.insert((12, 20), sim("Empty"));
        map.agents.insert((10, 20), vec![(2560, 5120, 4), (2570, 5130, 3)]);
        map.agents.insert((12, 20), Vec::new());
        let own = Some((10, 20));
        for own in [None, own, Some((11, 20))] {
            for name in ["Ahern", " ahern ", "Morris", "Empty", "Nowhere"] {
                let expected = status(&map, name, own).agents;
                let got = people.by_name(&map, name, own, t0);
                assert_eq!(got, Some(expected.map_or(People::Waiting, People::Count)), "{name} {own:?}");
            }
            for (grid, name) in [((10, 20), "Ahern"), ((11, 20), "Morris"), ((12, 20), "Empty")] {
                let expected = status(&map, name, own).agents;
                assert_eq!(people.by_grid(&map, grid, own, t0), expected.map_or(People::Waiting, People::Count));
            }
        }
        assert_eq!(people.by_name(&map, "Ahern", own, t0), Some(People::Count(8)));
        // a region that is down has nobody, as in the tracker
        map.sims.get_mut(&(10, 20)).expect("inserted").access = SIM_ACCESS_DOWN;
        assert_eq!(status(&map, "Ahern", own).agents, Some(0));
        assert_eq!(people.by_name(&map, "Ahern", own, t0), Some(People::Count(0)));
        assert_eq!(people.by_name(&map, "  ", own, t0), None);
    }

    fn block(x: u16, y: u16, name: &str) -> aurora_net::MapBlock {
        aurora_net::MapBlock {
            x,
            y,
            name: name.into(),
            access: SIM_ACCESS_PG,
            region_flags: 0,
            water_height: 20,
            agents: 0,
            map_image_id: Uuid::nil(),
            size_x: 256,
            size_y: 256,
        }
    }

    fn kinds(sent: &[NetCommand]) -> (usize, usize) {
        let blocks = sent
            .iter()
            .filter(|c| matches!(c, NetCommand::MapNameRequest { .. } | NetCommand::MapBlockRequest { .. }))
            .count();
        let items = sent.iter().filter(|c| matches!(c, NetCommand::MapItemRequest { .. })).count();
        (blocks, items)
    }

    #[test]
    fn a_lane_bursts_then_sends_one_a_second() {
        let t0 = Instant::now();
        let mut lane = Lane::new(4, Duration::from_secs(1));
        let mut sent = 0;
        while lane.ready(t0) {
            lane.spend();
            sent += 1;
        }
        assert_eq!(sent, 4, "the burst");
        assert!(!lane.ready(secs(t0, 0.5)), "half a second earns nothing yet");
        assert!(lane.ready(secs(t0, 1.0)));
        lane.spend();
        assert!(!lane.ready(secs(t0, 1.5)));
        // a long pause refills the burst, never more
        let later = secs(t0, 100.0);
        let mut again = 0;
        while lane.ready(later) {
            lane.spend();
            again += 1;
        }
        assert_eq!(again, 4);
    }

    #[test]
    fn each_lane_has_its_own_budget_and_blocks_go_first() {
        let mut map = WorldMap::default();
        let mut people = RegionPeople::default();
        let t0 = Instant::now();
        // ten rows the map knows and ten it does not
        for i in 0..10 {
            map.sims.insert((10 + i, 20), sim(&format!("Known {i}")));
        }
        let draw = |people: &RegionPeople, map: &WorldMap, now| {
            for i in 0..10 {
                people.by_name(map, &format!("Known {i}"), None, now);
                people.by_name(map, &format!("Unknown {i}"), None, now);
            }
        };
        draw(&people, &map, t0);
        people.pump(&mut map, t0);
        let sent = map.take_commands();
        // both lanes burst at once: the unknown rows do not hold the counts back
        assert_eq!(kinds(&sent), (BLOCK_BURST as usize, ITEM_BURST as usize));
        // the block lane's requests are queued first
        let first_item = sent
            .iter()
            .position(|c| matches!(c, NetCommand::MapItemRequest { .. }))
            .expect("an item request");
        assert!(sent[..first_item].iter().all(|c| matches!(c, NetCommand::MapNameRequest { .. })));
        // then about one a second in each lane
        let mut total = kinds(&sent);
        for frame in 1..=35 {
            let now = secs(t0, frame as f32 * 0.1);
            draw(&people, &map, now);
            people.pump(&mut map, now);
            let (b, i) = kinds(&map.take_commands());
            total = (total.0 + b, total.1 + i);
        }
        // 3.5 s: the burst and three more in each lane
        assert_eq!(total, (7, 7));
    }

    #[test]
    fn a_long_list_fills_in_seconds_and_matches_the_tracker() {
        let mut map = WorldMap::default();
        let mut people = RegionPeople::default();
        let t0 = Instant::now();
        let names: Vec<String> = (0..20).map(|i| format!("Region {i}")).collect();
        let place = |name: &str| {
            let i: u16 = name.trim_start_matches("region ").parse().expect("a number");
            (100 + i, 200)
        };
        let mut filled_at = None;
        for frame in 0..=600 {
            let now = secs(t0, frame as f32 * 0.1);
            let all = names
                .iter()
                .map(|n| people.by_name(&map, n, None, now))
                .all(|p| matches!(p, Some(People::Count(_))));
            if all {
                filled_at = Some(frame as f32 * 0.1);
                break;
            }
            people.pump(&mut map, now);
            // the grid answers by the next frame
            for command in map.take_commands() {
                match command {
                    NetCommand::MapNameRequest { name } => {
                        let (x, y) = place(&name);
                        map.apply_blocks(vec![block(x, y, &format!("Region {}", x - 100))]);
                    }
                    NetCommand::MapItemRequest { handle, .. } => {
                        let (x, y) = aurora_net::handle_to_origin(handle);
                        let (gx, gy) = (x / 256, y / 256);
                        map.apply_items(
                            aurora_net::map_item::AGENT_LOCATIONS,
                            vec![aurora_net::MapItem {
                                x: x + 128,
                                y: y + 128,
                                id: Uuid::nil(),
                                extra: (gx - 99) as i32,
                                extra2: 0,
                                name: String::new(),
                            }],
                        );
                        assert!(map.sims.contains_key(&(gx, gy)));
                    }
                    other => panic!("unexpected {other:?}"),
                }
            }
        }
        // one shared request every 2 s took 80 s for these 40 requests
        let filled_at = filled_at.expect("every row filled within a minute");
        assert!(filled_at <= 20.0, "filled after {filled_at} s");
        for name in &names {
            let expected = status(&map, name, None).agents.map(People::Count);
            assert_eq!(people.by_name(&map, name, None, secs(t0, 60.0)), expected, "{name}");
        }
    }

    #[test]
    fn rows_drawn_now_go_before_rows_scrolled_past() {
        let mut map = WorldMap::default();
        let mut people = RegionPeople::default();
        let t0 = Instant::now();
        // a fast scroll drew eight rows half a second ago, then these four
        for i in 0..8 {
            people.by_name(&map, &format!("Passed {i}"), None, t0);
        }
        let now = secs(t0, 0.5);
        for i in 0..4 {
            people.by_name(&map, &format!("Shown {i}"), None, now);
        }
        people.pump(&mut map, now);
        let asked: Vec<String> = map
            .take_commands()
            .into_iter()
            .filter_map(|c| match c {
                NetCommand::MapNameRequest { name } => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(asked, ["shown 0", "shown 1", "shown 2", "shown 3"]);
        // past a second off screen they are not asked at all
        let later = secs(t0, 1.2);
        for i in 0..4 {
            people.by_name(&map, &format!("Shown {i}"), None, later);
        }
        let mut sent = Vec::new();
        for frame in 0..50 {
            let now = later + Duration::from_millis(100 * frame);
            for i in 0..4 {
                people.by_name(&map, &format!("Shown {i}"), None, now);
            }
            people.pump(&mut map, now);
            sent.extend(map.take_commands());
        }
        assert!(sent.is_empty(), "nothing for the rows scrolled past: {sent:?}");
    }

    #[test]
    fn counts_are_asked_only_for_named_regions_and_answers_make_no_nameless_region() {
        let mut map = WorldMap::default();
        let mut people = RegionPeople::default();
        let t0 = Instant::now();
        for frame in 0..100 {
            let now = secs(t0, frame as f32 * 0.1);
            people.by_name(&map, "Nowhere", None, now);
            people.by_grid(&map, (300, 400), None, now);
            people.pump(&mut map, now);
            let sent = map.take_commands();
            assert!(
                !sent.iter().any(|c| matches!(c, NetCommand::MapItemRequest { .. })),
                "no count before the region has its name: {sent:?}"
            );
        }
        // an avatar answer for a region never named, and a block with no
        // name, add no region
        map.apply_items(
            aurora_net::map_item::AGENT_LOCATIONS,
            vec![aurora_net::MapItem {
                x: 300 * 256 + 5,
                y: 400 * 256 + 5,
                id: Uuid::nil(),
                extra: 3,
                extra2: 0,
                name: String::new(),
            }],
        );
        map.apply_blocks(vec![block(300, 400, "")]);
        assert!(map.sims.is_empty());
        assert!(map.sims.values().all(|s| !s.name.is_empty()));
        assert_eq!(people.by_grid(&map, (300, 400), None, secs(t0, 11.0)), People::Unknown);
        // named, it is counted
        map.apply_blocks(vec![block(300, 400, "Far")]);
        people.by_grid(&map, (300, 400), None, secs(t0, 12.0));
        people.pump(&mut map, secs(t0, 12.0));
        assert!(matches!(&map.take_commands()[..], [NetCommand::MapItemRequest { .. }]));
    }

    #[test]
    fn the_world_map_reads_the_figure_without_taking_a_lane() {
        let mut w = crate::world::World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        w.map.sims.insert((10, 20), sim("Ahern"));
        w.map.sims.insert((11, 20), sim("Morris"));
        w.map.agents.insert((10, 20), vec![(2560, 5120, 4)]);
        assert_eq!(w.people_on_map((10, 20)), Some(People::Count(4)));
        assert_eq!(w.people_on_map((11, 20)), Some(People::Waiting));
        assert_eq!(
            w.people_on_map((10, 20)).map(People::figure),
            w.people_in_region("Ahern").map(People::figure)
        );
        w.take_map_commands();
        // the map's reads queue nothing of the lanes
        for _ in 0..10 {
            w.people_on_map((11, 20));
        }
        assert!(w.take_map_commands().is_empty());
    }

    #[test]
    fn each_region_is_asked_once_a_minute_at_most() {
        let mut map = WorldMap::default();
        let mut people = RegionPeople::default();
        let t0 = Instant::now();
        map.sims.insert((10, 20), sim("Ahern"));
        let mut sent = Vec::new();
        for frame in 0..=1250 {
            let now = secs(t0, frame as f32 * 0.1);
            people.by_name(&map, "Ahern", None, now);
            people.by_name(&map, "Nowhere", None, now);
            people.pump(&mut map, now);
            sent.extend(map.take_commands());
        }
        // 125 s: three rounds of the avatars of the known one and of the
        // name of the unknown one
        let agents = sent.iter().filter(|c| matches!(c, NetCommand::MapItemRequest { .. })).count();
        let named = sent.iter().filter(|c| matches!(c, NetCommand::MapNameRequest { .. })).count();
        assert_eq!((agents, named), (3, 3));
    }

    #[test]
    fn only_the_rows_still_drawn_are_asked() {
        let mut map = WorldMap::default();
        let mut people = RegionPeople::default();
        let t0 = Instant::now();
        // never drawn: nothing asked
        people.pump(&mut map, t0);
        assert!(map.take_commands().is_empty());
        // drawn once then scrolled away: "Gone" is not asked later
        people.by_name(&map, "Gone", None, t0);
        people.by_name(&map, "Shown", None, t0);
        let mut asked = Vec::new();
        for frame in 15..100 {
            let now = secs(t0, frame as f32 * 0.1);
            people.by_name(&map, "Shown", None, now);
            people.pump(&mut map, now);
            for c in map.take_commands() {
                if let NetCommand::MapNameRequest { name } = c {
                    asked.push(name);
                }
            }
        }
        assert_eq!(asked, ["shown"]);
        // and forgotten after a while
        people.pump(&mut map, secs(t0, 200.0));
        assert!(people.entries.lock().expect("lock").is_empty());
    }

    #[test]
    fn rows_take_turns_and_a_grid_slot_asks_its_block_then_its_avatars() {
        let mut map = WorldMap::default();
        let mut people = RegionPeople::default();
        let t0 = Instant::now();
        map.sims.insert((10, 20), sim("Ahern"));
        let draw = |people: &RegionPeople, map: &WorldMap, now| {
            people.by_name(map, "Ahern", None, now);
            people.by_grid(map, (40, 50), None, now)
        };
        draw(&people, &map, t0);
        people.pump(&mut map, t0);
        let now = secs(t0, 2.0);
        draw(&people, &map, now);
        people.pump(&mut map, now);
        let sent = map.take_commands();
        assert_eq!(sent.len(), 2);
        assert!(sent.iter().any(|c| matches!(c, NetCommand::MapItemRequest { handle, .. }
            if *handle == aurora_net::origin_to_handle(10 * 256, 20 * 256))));
        assert!(
            sent.iter()
                .any(|c| matches!(c, NetCommand::MapBlockRequest { min_x: 32, min_y: 48, .. }))
        );
        // the block answers: the slot's region is asked for its avatars
        map.sims.insert((40, 50), sim("Far"));
        let now = secs(t0, 4.0);
        draw(&people, &map, now);
        people.pump(&mut map, now);
        assert!(matches!(&map.take_commands()[..], [NetCommand::MapItemRequest { handle, .. }]
            if *handle == aurora_net::origin_to_handle(40 * 256, 50 * 256)));
        map.agents.insert((40, 50), vec![(10240, 12800, 2)]);
        assert_eq!(draw(&people, &map, now), People::Count(2));
        assert_eq!(status(&map, "Far", None).agents, Some(2));
    }

    #[test]
    fn waiting_then_unknown_after_ten_seconds_then_the_figure() {
        let mut map = WorldMap::default();
        let mut people = RegionPeople::default();
        let t0 = Instant::now();
        map.sims.insert((10, 20), sim("Ahern"));
        // not asked yet: waiting, however long the queue makes it wait
        assert_eq!(people.by_name(&map, "Ahern", None, t0), Some(People::Waiting));
        assert_eq!(people.by_name(&map, "Ahern", None, secs(t0, 30.0)), Some(People::Waiting));
        let asked = secs(t0, 30.5);
        people.by_name(&map, "Ahern", None, asked);
        people.pump(&mut map, asked);
        assert_eq!(map.take_commands().len(), 1);
        assert_eq!(
            people.by_name(&map, "Ahern", None, asked + Duration::from_secs(9)),
            Some(People::Waiting)
        );
        assert_eq!(people.by_name(&map, "Ahern", None, asked + ANSWER_TIMEOUT), Some(People::Unknown));
        // a late answer still shows, and the next wait starts again
        map.agents.insert((10, 20), vec![(2560, 5120, 6)]);
        assert_eq!(
            people.by_name(&map, "Ahern", None, asked + Duration::from_secs(12)),
            Some(People::Count(6))
        );
        map.agents.remove(&(10, 20));
        assert_eq!(
            people.by_name(&map, "Ahern", None, asked + Duration::from_secs(13)),
            Some(People::Waiting)
        );
        // our own region is never unknown: we are in it
        assert_eq!(
            people.by_name(&map, "Ahern", Some((10, 20)), asked + Duration::from_secs(13)),
            Some(People::Count(1))
        );
        assert_eq!(People::Waiting.figure(), "…");
        assert_eq!(People::Unknown.figure(), "?");
        assert_eq!(People::Count(12).figure(), "12");
    }

    #[test]
    fn showloc_hides_the_figure_and_asks_nothing() {
        let mut w = crate::world::World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        w.map.sims.insert((10, 20), sim("Ahern"));
        w.map.agents.insert((10, 20), vec![(2560, 5120, 4)]);
        assert_eq!(w.people_in_region("Ahern"), Some(People::Count(4)));
        assert_eq!(w.people_at_global(2570.0, 5130.0), Some(People::Count(4)));
        assert_eq!(w.people_at_global(-1.0, 5.0), None);
        w.take_map_commands();
        w.rlva.configure(true);
        w.apply(aurora_net::NetEvent::Chat(aurora_net::ChatMessage {
            from_name: "Fixture".into(),
            source_id: Uuid::from_u128(1),
            owner_id: w.agent_id,
            source_type: aurora_net::ChatSourceType::Object,
            chat_type: aurora_net::ChatType::OwnerSay,
            position: glam::Vec3::ZERO,
            message: "@showloc=n".into(),
        }));
        assert!(w.rlva.has("showloc"));
        w.take_map_commands();
        assert_eq!(w.people_in_region("Ahern"), None);
        assert_eq!(w.people_in_region("Nowhere"), None);
        assert_eq!(w.people_at_global(2570.0, 5130.0), None);
        assert_eq!(w.people_on_map((10, 20)), None);
        assert!(w.take_map_commands().is_empty());
    }
}
