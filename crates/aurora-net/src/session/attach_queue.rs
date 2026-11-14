//! Port of LLAttachmentsMgr::requestAttachments
//! (indra/newview/llattachmentsmgr.cpp, originally LGPL 2.1): attachment
//! requests leave one object per RezMultipleAttachmentsFromInv, never as a
//! burst. "Requesting many attachments at once causes frequent server-side
//! failures" (one per idle pass since FIRE-6070).
//!
//! One per pass is not enough on a busy region: on 2026-10-27, 18 requests
//! 20 ms apart, after the region had put 14 objects back by itself, got the
//! first 14 worn and the last 4 refused ("Attempt to rez an object failed");
//! the refused items then stayed off for the session, whatever asked for
//! them again. So only `limit` requests wait for their object at a time
//! (until an object worn from the item arrives, or `IN_FLIGHT_TIMEOUT`):
//! it starts at `START_LIMIT`, grows by one every `GROW_EVERY` objects
//! that arrive, up to `MAX_LIMIT`, and halves when the simulator refuses
//! one, which also pauses the queue for `PAUSE`. Four at a time made an
//! outfit change on a busy region take two minutes. The Legacy Viewer has
//! no such limit and loses objects the same way now and then.

use super::*;
use std::collections::VecDeque;

/// Shortest gap between two requests.
const SPACING: Duration = Duration::from_millis(50);
/// Requests waiting for their object at once: at first, at least, at most.
const START_LIMIT: usize = 8;
const MIN_LIMIT: usize = 2;
const MAX_LIMIT: usize = 12;
/// Objects arrived between two raises of the limit.
const GROW_EVERY: usize = 4;
/// A request whose object has not arrived by then frees its place.
const IN_FLIGHT_TIMEOUT: Duration = Duration::from_secs(10);
/// No request leaves for this long after the simulator refused one.
const PAUSE: Duration = Duration::from_secs(5);
/// The simulator's refusal (AlertMessage).
const REZ_FAILED: &str = "Attempt to rez an object failed";

/// ATTACHMENT_ADD (indra/llcommon/indra_constants.h): keep
/// what the point already holds. Point 0 asks for the item's own default point.
const ATTACHMENT_ADD: u8 = 0x80;

/// The AttachmentPt byte of RezMultipleAttachmentsFromInv, as
/// LLAttachmentsMgr::requestAttachments builds it: the point (0 = the
/// item's default one, chosen by the simulator), with ATTACHMENT_ADD set
/// when the request adds instead of replacing.
pub(super) fn wire_point(point: u8, add: bool) -> u8 {
    point | if add { ATTACHMENT_ADD } else { 0 }
}

/// First eight characters of an id, for the log.
fn short(id: Uuid) -> String {
    id.simple().to_string().chars().take(8).collect()
}

/// A request sent and waiting for its object.
struct InFlight {
    at: Instant,
    point: u8,
    add: bool,
}

pub(super) struct AttachQueue {
    pending: VecDeque<AttachRequest>,
    /// Items requested whose object has not arrived yet, and when.
    in_flight: HashMap<Uuid, InFlight>,
    /// Explicit requests (a point was chosen) that came while an earlier,
    /// different request for the same item was on its way: sent once that
    /// one is settled, never dropped.
    deferred: HashMap<Uuid, AttachRequest>,
    limit: usize,
    /// Objects arrived since the limit last grew.
    arrived_since_raise: usize,
    last_sent: Option<Instant>,
    paused_until: Option<Instant>,
    /// The run of requests under way, for its log line.
    run: Option<Run>,
}

/// A run of requests, from the first queued to the queue empty again.
struct Run {
    started: Instant,
    worn: Vec<Duration>,
    timed_out: usize,
    refused: usize,
}

impl Default for AttachQueue {
    fn default() -> Self {
        AttachQueue {
            pending: VecDeque::new(),
            in_flight: HashMap::new(),
            deferred: HashMap::new(),
            limit: START_LIMIT,
            arrived_since_raise: 0,
            last_sent: None,
            paused_until: None,
            run: None,
        }
    }
}

impl AttachQueue {
    /// Queues `list`. A request for an item still waiting takes its place
    /// (the latest point and add mode win), except that a request without a
    /// point (the default one, as login restore, outfit sync and refresh
    /// ask) never replaces a chosen point. One whose object is on its way
    /// is not sent twice; if it names another point or mode than the one
    /// sent, it waits for that one to settle and is then sent.
    pub(super) fn push(&mut self, list: Vec<AttachRequest>, now: Instant) {
        for request in list {
            let id = short(request.item_id);
            if let Some(sent) = self.in_flight.get(&request.item_id) {
                if request.point != 0 && (request.point != sent.point || request.add != sent.add) {
                    log::info!(
                        "attach request {id} point {} add {}: deferred, the one on its way names point {} add {}",
                        request.point,
                        request.add,
                        sent.point,
                        sent.add
                    );
                    self.deferred.insert(request.item_id, request);
                } else {
                    log::info!(
                        "attach request {id} point {} add {}: dropped, same item already on its way (point {} add {})",
                        request.point,
                        request.add,
                        sent.point,
                        sent.add
                    );
                }
                continue;
            }
            match self.pending.iter_mut().find(|p| p.item_id == request.item_id) {
                Some(slot) if request.point == 0 && slot.point != 0 => {
                    log::info!(
                        "attach request {id} point 0 add {}: dropped, the queued one names point {}",
                        request.add,
                        slot.point
                    );
                }
                Some(slot) => {
                    log::info!(
                        "attach request {id} point {} add {}: merged into the queued one (was point {} add {})",
                        request.point,
                        request.add,
                        slot.point,
                        slot.add
                    );
                    *slot = request;
                }
                None => self.pending.push_back(request),
            }
        }
        self.start_run(now);
    }

    fn start_run(&mut self, now: Instant) {
        if !self.pending.is_empty() && self.run.is_none() {
            self.run = Some(Run {
                started: now,
                worn: Vec::new(),
                timed_out: 0,
                refused: 0,
            });
        }
    }

    /// A settled request (object arrived, or timed out) lets the explicit
    /// one that waited behind it go first.
    fn release(&mut self, item: Uuid, now: Instant) {
        if let Some(request) = self.deferred.remove(&item) {
            log::info!(
                "attach request {} point {} add {}: sent now, the earlier one is settled",
                short(item),
                request.point,
                request.add
            );
            self.pending.push_front(request);
            self.start_run(now);
        }
    }

    /// The next request to send now, if its turn has come.
    pub(super) fn next(&mut self, now: Instant) -> Option<AttachRequest> {
        let before = self.in_flight.len();
        let expired: Vec<Uuid> = self
            .in_flight
            .iter()
            .filter(|(_, sent)| now.saturating_duration_since(sent.at) >= IN_FLIGHT_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.in_flight.remove(&id);
            self.release(id, now);
        }
        if let Some(run) = &mut self.run {
            run.timed_out += before - self.in_flight.len();
        }
        self.end_run(now);
        if self.pending.is_empty()
            || self.in_flight.len() >= self.limit
            || self.paused_until.is_some_and(|until| now < until)
            || self.last_sent.is_some_and(|at| now.saturating_duration_since(at) < SPACING)
        {
            return None;
        }
        let request = self.pending.pop_front()?;
        self.last_sent = Some(now);
        self.in_flight.insert(
            request.item_id,
            InFlight {
                at: now,
                point: request.point,
                add: request.add,
            },
        );
        Some(request)
    }

    /// Objects arrived, each with its name-values and the attachment point
    /// it sits on: one worn from a requested item frees its place, and its
    /// point is logged next to the one asked for.
    pub(super) fn observe<'a>(&mut self, objects: impl Iterator<Item = (&'a str, u8)>, now: Instant) {
        if self.in_flight.is_empty() {
            return;
        }
        for (nv, landed) in objects {
            let Some(item) = attach_item_id(nv) else {
                continue;
            };
            let Some(sent) = self.in_flight.remove(&item) else {
                continue;
            };
            if sent.point != 0 && sent.point != landed {
                log::warn!(
                    "attach arrived {}: on point {landed}, point {} was asked for (add {})",
                    short(item),
                    sent.point,
                    sent.add
                );
            } else {
                log::info!(
                    "attach arrived {}: on point {landed}, asked for point {} add {}",
                    short(item),
                    sent.point,
                    sent.add
                );
            }
            if let Some(run) = &mut self.run {
                run.worn.push(now.saturating_duration_since(sent.at));
            }
            self.arrived_since_raise += 1;
            if self.arrived_since_raise >= GROW_EVERY {
                self.arrived_since_raise = 0;
                self.limit = (self.limit + 1).min(MAX_LIMIT);
            }
            self.release(item, now);
        }
    }

    /// An alert from the simulator: a refusal halves the limit and pauses
    /// the queue.
    pub(super) fn on_alert(&mut self, message: &str, now: Instant) {
        if message.contains(REZ_FAILED) && !(self.pending.is_empty() && self.in_flight.is_empty()) {
            self.limit = (self.limit / 2).max(MIN_LIMIT);
            self.arrived_since_raise = 0;
            if let Some(run) = &mut self.run {
                run.refused += 1;
            }
            log::info!(
                "attachments: the simulator refused one, {} waiting resume in {} s, {} at a time",
                self.pending.len(),
                PAUSE.as_secs(),
                self.limit
            );
            self.paused_until = Some(now + PAUSE);
        }
    }

    /// Logs the run once nothing waits any more.
    fn end_run(&mut self, now: Instant) {
        if !self.pending.is_empty() || !self.in_flight.is_empty() || !self.deferred.is_empty() {
            return;
        }
        let Some(mut run) = self.run.take() else {
            return;
        };
        run.worn.sort();
        let median = run.worn.get(run.worn.len() / 2).copied().unwrap_or_default();
        let slowest = run.worn.last().copied().unwrap_or_default();
        log::info!(
            "attachments: {} worn in {:.1} s (each {:.1} s median, {:.1} s at most), {} without an object after {} s, {} refused, {} at a time",
            run.worn.len(),
            now.saturating_duration_since(run.started).as_secs_f32(),
            median.as_secs_f32(),
            slowest.as_secs_f32(),
            run.timed_out,
            IN_FLIGHT_TIMEOUT.as_secs(),
            run.refused,
            self.limit
        );
    }

    pub(super) fn len(&self) -> usize {
        self.pending.len() + self.deferred.len()
    }
}

/// The item an object is worn from (its `AttachItemID` name-value).
fn attach_item_id(name_values: &str) -> Option<Uuid> {
    name_values
        .lines()
        .find(|l| l.starts_with("AttachItemID"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|id| id.parse().ok())
}

impl Session<'_> {
    /// Sends the next queued attachment request when its turn has come.
    pub(super) fn send_queued_attachment(&mut self, now: Instant) {
        let Some(a) = self.attach_queue.next(now) else {
            return;
        };
        let wire = wire_point(a.point, a.add);
        log::info!(
            "attach request sent: '{}' item {} point {} add {} byte 0x{wire:02x} item_flags 0x{:x}",
            a.name,
            short(a.item_id),
            a.point,
            a.add,
            a.flags
        );
        let mut m = RezMultipleAttachmentsFromInv::default();
        m.agent_data.agent_id = self.agent_id();
        m.agent_data.session_id = self.session_id();
        m.header_data.compound_msg_id = uuid::Uuid::new_v4();
        m.header_data.total_objects = 1;
        m.header_data.first_detach_all = false;
        m.object_data = vec![rez_multiple_attachments_from_inv::ObjectData {
            item_id: a.item_id,
            owner_id: a.owner_id,
            attachment_pt: wire,
            item_flags: a.flags,
            group_mask: a.group_mask,
            everyone_mask: a.everyone_mask,
            next_owner_mask: a.next_owner_mask,
            name: str_field(&a.name),
            description: str_field(&a.desc),
        }];
        self.send_main(&m, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: u128, point: u8) -> AttachRequest {
        AttachRequest {
            item_id: Uuid::from_u128(id),
            owner_id: Uuid::nil(),
            point,
            add: true,
            flags: 0,
            group_mask: 0,
            everyone_mask: 0,
            next_owner_mask: 0,
            name: String::new(),
            desc: String::new(),
        }
    }

    fn worn(id: u128) -> String {
        format!("AttachItemID STRING RW SV {}", Uuid::from_u128(id))
    }

    /// Sends what the queue lets leave over `ticks` gaps from `at`.
    fn drain(q: &mut AttachQueue, mut at: Instant, ticks: u32) -> (Vec<u128>, Instant) {
        let mut sent = Vec::new();
        for _ in 0..ticks {
            if let Some(a) = q.next(at) {
                sent.push(a.item_id.as_u128());
            }
            at += SPACING;
        }
        (sent, at)
    }

    #[test]
    fn requests_leave_spaced_and_a_few_at_a_time() {
        let mut q = AttachQueue::default();
        let t0 = Instant::now();
        q.push((1..=20).map(|id| request(id, 0)).collect(), t0);
        assert_eq!(q.next(t0).map(|a| a.item_id.as_u128()), Some(1));
        assert!(q.next(t0 + SPACING / 2).is_none(), "not before the gap");
        let (sent, at) = drain(&mut q, t0 + SPACING, 30);
        assert_eq!(sent.len() + 1, START_LIMIT, "the others wait for their object");
        // an object worn from item 2 arrives: its place is free
        q.observe([worn(2)].iter().map(|w| (w.as_str(), 0)), at);
        assert_eq!(q.next(at).map(|a| a.item_id.as_u128()), Some(START_LIMIT as u128 + 1));
        // the others free theirs after the timeout
        let (sent, _) = drain(&mut q, t0 + IN_FLIGHT_TIMEOUT + SPACING, 30);
        assert!(!sent.is_empty());
    }

    #[test]
    fn the_limit_grows_with_arrivals_and_halves_on_a_refusal() {
        let mut q = AttachQueue::default();
        let t0 = Instant::now();
        q.push((1..=40).map(|id| request(id, 0)).collect(), t0);
        let (sent, at) = drain(&mut q, t0, 20);
        q.observe(
            sent.iter().map(|id| worn(*id)).collect::<Vec<_>>().iter().map(|w| (w.as_str(), 0)),
            at,
        );
        assert_eq!(q.limit, START_LIMIT + START_LIMIT / GROW_EVERY);
        let (_, at) = drain(&mut q, at, 20);
        q.on_alert("Attempt to rez an object failed.", at);
        assert_eq!(q.limit, (START_LIMIT + START_LIMIT / GROW_EVERY) / 2);
        assert!(q.next(at + PAUSE - Duration::from_millis(1)).is_none(), "paused");
        // never under MIN_LIMIT nor over MAX_LIMIT
        for _ in 0..5 {
            q.on_alert("Attempt to rez an object failed.", at);
        }
        assert_eq!(q.limit, MIN_LIMIT);
        let many: Vec<String> = (1..=200).map(worn).collect();
        q.in_flight = (1..=200)
            .map(|id| (Uuid::from_u128(id), InFlight { at, point: 0, add: true }))
            .collect();
        q.observe(many.iter().map(|w| (w.as_str(), 0)), at);
        assert_eq!(q.limit, MAX_LIMIT);
    }

    #[test]
    fn other_alerts_change_nothing() {
        let mut q = AttachQueue::default();
        let t0 = Instant::now();
        q.push(vec![request(1, 0), request(2, 0)], t0);
        assert!(q.next(t0).is_some());
        q.on_alert("You cannot do that here.", t0);
        assert_eq!(q.limit, START_LIMIT);
        assert!(q.next(t0 + SPACING).is_some());
    }

    #[test]
    fn a_waiting_item_asked_again_keeps_its_place_with_the_new_point() {
        let mut q = AttachQueue::default();
        let t0 = Instant::now();
        q.push(vec![request(1, 0), request(2, 0)], t0);
        q.push(vec![request(1, 35), request(3, 0)], t0);
        assert_eq!(q.len(), 3);
        let first = q.next(t0).expect("first");
        assert_eq!((first.item_id, first.point), (Uuid::from_u128(1), 35));
        // asked again while its object is on its way: not sent twice
        q.push(vec![request(1, 0)], t0);
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn the_wire_byte_is_the_point_with_the_add_bit() {
        assert_eq!(wire_point(0, true), 0x80, "Add without a point: the item's own point, stacked");
        assert_eq!(wire_point(0, false), 0, "Wear without a point");
        assert_eq!(wire_point(35, true), 0x80 | 35);
        assert_eq!(wire_point(35, false), 35);
        assert_eq!(wire_point(3, true) & !0x80, 3);
    }

    #[test]
    fn a_default_request_never_replaces_a_chosen_point_in_the_queue() {
        let mut q = AttachQueue::default();
        let t0 = Instant::now();
        q.push(vec![request(1, 35)], t0);
        q.push(vec![request(1, 0)], t0);
        assert_eq!(q.len(), 1);
        assert_eq!(q.next(t0).map(|a| a.point), Some(35));
        // a later explicit point still replaces an earlier one
        let mut q = AttachQueue::default();
        q.push(vec![request(1, 35)], t0);
        q.push(vec![request(1, 7)], t0);
        assert_eq!(q.next(t0).map(|a| a.point), Some(7));
    }

    #[test]
    fn a_chosen_point_asked_while_a_default_request_is_on_its_way_is_sent_after_it() {
        let mut q = AttachQueue::default();
        let t0 = Instant::now();
        q.push(vec![request(1, 0)], t0);
        assert_eq!(q.next(t0).map(|a| a.point), Some(0));
        q.push(vec![request(1, 35)], t0);
        assert_eq!(q.len(), 1, "kept, not dropped");
        assert!(q.next(t0 + SPACING).is_none(), "not while the first is on its way");
        q.observe([worn(1)].iter().map(|w| (w.as_str(), 6)), t0 + SPACING);
        let again = q.next(t0 + SPACING * 2).expect("sent once the first object arrived");
        assert_eq!((again.item_id, again.point, again.add), (Uuid::from_u128(1), 35, true));
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn a_deferred_request_is_sent_when_the_first_one_times_out() {
        let mut q = AttachQueue::default();
        let t0 = Instant::now();
        q.push(vec![request(1, 0)], t0);
        assert!(q.next(t0).is_some());
        q.push(vec![request(1, 35)], t0);
        assert!(q.next(t0 + IN_FLIGHT_TIMEOUT - Duration::from_millis(1)).is_none());
        let again = q.next(t0 + IN_FLIGHT_TIMEOUT).expect("sent after the timeout");
        assert_eq!(again.point, 35);
    }

    #[test]
    fn a_default_or_identical_request_while_one_is_on_its_way_is_dropped() {
        let mut q = AttachQueue::default();
        let t0 = Instant::now();
        q.push(vec![request(1, 35)], t0);
        assert!(q.next(t0).is_some());
        q.push(vec![request(1, 0), request(1, 35)], t0);
        assert_eq!(q.len(), 0);
        // a different add mode on the same chosen point is a different request
        let mut replace = request(1, 35);
        replace.add = false;
        q.push(vec![replace], t0);
        assert_eq!(q.len(), 1);
        // the latest chosen request is the one kept
        q.push(vec![request(1, 7)], t0);
        assert_eq!(q.len(), 1);
        q.observe([worn(1)].iter().map(|w| (w.as_str(), 35)), t0);
        assert_eq!(q.next(t0 + SPACING).map(|a| a.point), Some(7));
    }

    #[test]
    fn the_item_comes_from_the_attach_item_id_name_value() {
        let id = Uuid::from_u128(7);
        assert_eq!(
            attach_item_id(&format!("Title STRING RW SV hi\nAttachItemID STRING RW SV {id}")),
            Some(id)
        );
        assert_eq!(attach_item_id("Title STRING RW SV hi"), None);
    }
}
