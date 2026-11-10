//! "People in region" beside a place shown outside the world map (place
//! details, About Land, search, teleport history, landmarks, favorites, the
//! mini-map's tooltip).
//!
//! The figure is the Region Tracker's (`region_tracker::status_at`) and the
//! requests are the world map's (`WorldMap::want_named_region`,
//! `want_agents`, `want_regions`, each already limited to once a minute per
//! region). This module only decides which of the regions on screen is asked
//! next: one request every `REQUEST_EVERY` for all of them together, so a
//! long list never floods the map service, and nothing for a row that is no
//! longer drawn.

use super::region_tracker::status_at;
use super::worldmap::WorldMap;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One map request at most this often, all regions together (30 a minute).
pub const REQUEST_EVERY: Duration = Duration::from_secs(2);
/// A region counts as on screen for this long after it was last drawn.
const VISIBLE_FOR: Duration = Duration::from_secs(1);
/// Asked and still no figure after this long: shown as unknown.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
/// A region not drawn for this long is forgotten.
const FORGET_AFTER: Duration = Duration::from_secs(120);
/// A name the map does not know yet is searched again in its regions at
/// most this often (the search reads every known region).
const RESOLVE_EVERY: Duration = Duration::from_millis(500);

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
    /// The last request sent, for the turn of each region.
    served: Option<Instant>,
}

#[derive(Default)]
pub struct RegionPeople {
    /// Behind a lock as `Names::ask`: the windows read the world without
    /// changing it, and drawing a row is what asks for its region.
    entries: Mutex<HashMap<Target, Entry>>,
    last_sent: Option<Instant>,
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
            served: None,
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

    /// Queue the next request of the regions on screen, when one is due:
    /// the one served longest ago first. A region the map knows is asked
    /// for its avatars; one it does not know, or that is down, by name (or
    /// by its block of the grid) for its state.
    pub fn pump(&mut self, map: &mut WorldMap, now: Instant) {
        let entries = self.entries.get_mut().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, e| now.duration_since(e.seen) < FORGET_AFTER);
        if self.last_sent.is_some_and(|t| now.duration_since(t) < REQUEST_EVERY) {
            return;
        }
        let mut visible: Vec<_> = entries
            .iter()
            .filter(|(_, e)| now.duration_since(e.seen) < VISIBLE_FOR)
            .map(|(t, e)| (e.served, t.clone()))
            .collect();
        visible.sort();
        for (_, target) in visible {
            let Some(entry) = entries.get_mut(&target) else { continue };
            let key = resolve(map, &target, entry, now);
            let sim = key.and_then(|k| map.sims.get(&k).map(|s| (k, s.is_down(), s.name.clone())));
            let sent = match (sim, &target) {
                (Some((k, false, _)), _) => map.want_agents_at(k, now),
                (Some((_, true, name)), _) => map.want_named_region_at(&name, now),
                (None, Target::Name(name)) => map.want_named_region_at(name, now),
                (None, Target::Grid(x, y)) => {
                    let (x, y) = (i64::from(*x), i64::from(*y));
                    map.want_regions_at(x, y, x, y, now)
                }
            };
            if sent {
                entry.served = Some(now);
                entry.asked.get_or_insert(now);
                self.last_sent = Some(now);
                return;
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

    #[test]
    fn one_request_every_two_seconds_whatever_the_number_of_rows() {
        let mut map = WorldMap::default();
        let mut people = RegionPeople::default();
        let t0 = Instant::now();
        let names: Vec<String> = (0..100).map(|i| format!("Region {i}")).collect();
        let mut sent = 0;
        // a minute of frames, every row drawn each frame
        for frame in 0..=600 {
            let now = secs(t0, frame as f32 * 0.1);
            for name in &names {
                people.by_name(&map, name, None, now);
            }
            people.pump(&mut map, now);
            sent += map.take_commands().len();
        }
        assert!((29..=31).contains(&sent), "{sent} requests in a minute");
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
        assert!(w.take_map_commands().is_empty());
    }
}
