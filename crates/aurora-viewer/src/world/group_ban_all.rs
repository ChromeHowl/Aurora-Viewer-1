//! "Ban from all my groups" and "Unban from all my groups" of a resident's
//! menus: one GroupAPIv1 request per group where we hold the power to
//! manage bans, one group at a time with a pause between them, then a
//! report of where it worked and where it failed. A resident banned from a
//! group they are (or may be) a member of is ejected too, where we may eject.
//!
//! Port of LLGroupActions::canBanFromAnyGroup, confirmBanFromAllGroups and
//! banFromAllGroupsCoro (indra/newview/llgroupactions.cpp, originally LGPL
//! 2.1), with their FSBanFromAllGroups, FSUnbanFromAllGroups,
//! FSBanFromAllGroupsStarted, FSBanFromAllGroupsReport and
//! FSUnbanFromAllGroupsReport messages (notifications.xml). The requests
//! are the group window's own (`group_data`: `NetCommand::GroupBans`, the
//! ejection's EjectGroupMemberRequest).

use super::World;
use aurora_net::NetCommand;
use aurora_net::groups::{ban, powers};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// The pause between two groups (suspendUntilTimeout(0.5)), so that the
/// requests never look like a flood.
const PAUSE: Duration = Duration::from_millis(500);
/// A request the server never answered counts as failed after this.
const GIVE_UP: Duration = Duration::from_secs(60);

/// A run over the groups, one at a time.
#[derive(Debug, Clone, PartialEq)]
pub struct BanAll {
    pub resident: Uuid,
    pub name: String,
    pub ban: bool,
    /// The groups to do, in the order of the question (by name).
    groups: Vec<(Uuid, String)>,
    next: usize,
    /// The group asked and since when.
    waiting: Option<(Uuid, Instant)>,
    /// When the last answer came: the next request waits `PAUSE` after it.
    answered: Option<Instant>,
    pub succeeded: Vec<String>,
    pub failed: Vec<String>,
}

/// What a step of the run asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing now.
    Wait,
    /// The request for this group.
    Send(Uuid),
    /// Every group was done: the report.
    Done,
}

impl BanAll {
    pub fn new(resident: Uuid, name: String, ban: bool, groups: Vec<(Uuid, String)>) -> BanAll {
        BanAll {
            resident,
            name,
            ban,
            groups,
            next: 0,
            waiting: None,
            answered: None,
            succeeded: Vec::new(),
            failed: Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.groups.len()
    }

    /// The next step at `now`: a request once the previous one was answered
    /// and the pause is over; a request left unanswered fails after
    /// `GIVE_UP`.
    pub fn step(&mut self, now: Instant) -> Step {
        if let Some((group, since)) = self.waiting {
            if now.duration_since(since) < GIVE_UP {
                return Step::Wait;
            }
            log::warn!("group bans: no answer for group {group}");
            self.answer(group, false, now);
        }
        if self.answered.is_some_and(|at| now.duration_since(at) < PAUSE) {
            return Step::Wait;
        }
        match self.groups.get(self.next) {
            Some((group, _)) => {
                let group = *group;
                self.next += 1;
                self.waiting = Some((group, now));
                Step::Send(group)
            }
            None => Step::Done,
        }
    }

    /// The server's answer for `group`; false when the run was not waiting
    /// for it (another window's request).
    pub fn answer(&mut self, group: Uuid, ok: bool, now: Instant) -> bool {
        if self.waiting.map(|(g, _)| g) != Some(group) {
            return false;
        }
        self.waiting = None;
        self.answered = Some(now);
        let name = self
            .groups
            .iter()
            .find(|(g, _)| *g == group)
            .map(|(_, n)| n.clone())
            .unwrap_or_default();
        if ok {
            self.succeeded.push(name);
        } else {
            self.failed.push(name);
        }
        true
    }
}

/// The groups where we may manage bans (GP_GROUP_BAN_ACCESS), sorted by
/// name as the question lists them; none for ourselves or nobody
/// (canBanFromAnyGroup).
pub fn ban_groups(groups: &[aurora_net::GroupMembership], me: Uuid, resident: Uuid) -> Vec<(Uuid, String)> {
    if resident.is_nil() || resident == me {
        return Vec::new();
    }
    let mut list: Vec<(Uuid, String)> = groups
        .iter()
        .filter(|g| g.powers & powers::GROUP_BAN_ACCESS != 0)
        .map(|g| (g.id, g.name.clone()))
        .collect();
    list.sort_by(|a, b| crate::natural_order::cmp(&a.1, &b.1));
    list
}

/// The names in a report: "-" for none, as the Legacy report.
pub fn names(list: &[String]) -> String {
    if list.is_empty() { "-".to_owned() } else { list.join(", ") }
}

impl World {
    /// The groups where "Ban from all my groups" would act on `resident`.
    pub fn group_ban_targets(&self, resident: Uuid) -> Vec<(Uuid, String)> {
        ban_groups(&self.groups.groups, self.agent_id, resident)
    }

    /// The question was answered: the run starts, and says so.
    pub fn start_group_ban_all(&mut self, resident: Uuid, name: String, ban: bool) {
        let groups = self.group_ban_targets(resident);
        if groups.is_empty() || self.group_ban_all.is_some() {
            return;
        }
        let count = groups.len().to_string();
        self.notifications.push(
            super::notifications::Kind::Info,
            aurora_i18n::tr!("l2s-ban-all-title"),
            aurora_i18n::tr!("l2s-ban-all-started", name = name.as_str(), count = count.as_str()),
            super::notifications::Data::None,
        );
        self.group_ban_all = Some(BanAll::new(resident, name, ban, groups));
    }

    /// Each frame: the next request, or the report at the end.
    pub fn tick_group_ban_all(&mut self, now: Instant) {
        let Some(run) = self.group_ban_all.as_mut() else {
            return;
        };
        match run.step(now) {
            Step::Wait => {}
            Step::Send(group) => {
                let (resident, ban) = (run.resident, run.ban);
                self.groups.out.push(NetCommand::GroupBans {
                    group,
                    action: Some(if ban { ban::CREATE } else { ban::DELETE }),
                    ids: vec![resident],
                    refresh: false,
                });
            }
            Step::Done => {
                let Some(run) = self.group_ban_all.take() else {
                    return;
                };
                let (count, done) = (run.count().to_string(), run.succeeded.len().to_string());
                let (succeeded, failed) = (names(&run.succeeded), names(&run.failed));
                let body = if run.ban {
                    aurora_i18n::tr!(
                        "l2s-ban-all-report",
                        name = run.name.as_str(),
                        done = done.as_str(),
                        count = count.as_str(),
                        succeeded = succeeded.as_str(),
                        failed = failed.as_str()
                    )
                } else {
                    aurora_i18n::tr!(
                        "l2s-unban-all-report",
                        name = run.name.as_str(),
                        done = done.as_str(),
                        count = count.as_str(),
                        succeeded = succeeded.as_str(),
                        failed = failed.as_str()
                    )
                };
                self.notifications.push(
                    super::notifications::Kind::Info,
                    aurora_i18n::tr!("l2s-ban-all-title"),
                    body,
                    super::notifications::Data::None,
                );
            }
        }
    }

    /// GroupBanDone: the run's answer, then, after a ban, the ejection where
    /// we may eject and the resident is not known to be outside the group
    /// (a ban alone does not remove a member); our copy of the ban list
    /// follows.
    pub(super) fn on_group_ban_done(&mut self, group: Uuid, action: i32, ids: &[Uuid], ok: bool) {
        let Some(run) = self.group_ban_all.as_mut() else {
            return;
        };
        if ids != [run.resident] || action != if run.ban { ban::CREATE } else { ban::DELETE } {
            return;
        }
        if !run.answer(group, ok, Instant::now()) || !ok {
            return;
        }
        let (resident, banned) = (run.resident, run.ban);
        if !banned {
            self.group_data.forget_ban(group, resident);
            return;
        }
        self.group_data.note_ban(group, resident);
        let may_eject = self.groups.group(&group).is_some_and(|g| g.powers & powers::MEMBER_EJECT != 0);
        let known_outside = self
            .group_data
            .get(&group)
            .is_some_and(|g| g.members_complete && !g.members.contains_key(&resident));
        if may_eject && !known_outside {
            let me = self.agent_id;
            self.group_data.eject(group, vec![resident], me);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: u128, name: &str, powers: u64) -> aurora_net::GroupMembership {
        aurora_net::GroupMembership {
            id: Uuid::from_u128(id),
            name: name.into(),
            insignia: Uuid::nil(),
            powers,
            accept_notices: true,
            list_in_profile: true,
            contribution: 0,
        }
    }

    #[test]
    fn only_the_groups_where_we_manage_bans_by_name() {
        let me = Uuid::from_u128(1);
        let jane = Uuid::from_u128(2);
        let groups = [
            group(10, "Zebra Club", powers::GROUP_BAN_ACCESS),
            group(11, "No power", powers::MEMBER_EJECT),
            group(12, "art gallery", powers::GROUP_BAN_ACCESS | powers::MEMBER_EJECT),
        ];
        let list = ban_groups(&groups, me, jane);
        assert_eq!(
            list.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(),
            ["art gallery", "Zebra Club"]
        );
        assert!(ban_groups(&groups, me, me).is_empty(), "never ourselves");
        assert!(ban_groups(&groups, me, Uuid::nil()).is_empty());
    }

    #[test]
    fn one_group_at_a_time_with_a_pause_then_the_report() {
        let (a, b) = (Uuid::from_u128(10), Uuid::from_u128(11));
        let mut run = BanAll::new(Uuid::from_u128(2), "Jane".into(), true, vec![(a, "A".into()), (b, "B".into())]);
        let t0 = Instant::now();
        assert_eq!(run.step(t0), Step::Send(a));
        assert_eq!(run.step(t0 + Duration::from_secs(1)), Step::Wait, "until it is answered");
        assert!(!run.answer(b, true, t0), "not the group asked");
        assert!(run.answer(a, true, t0 + Duration::from_secs(1)));
        assert_eq!(run.step(t0 + Duration::from_millis(1200)), Step::Wait, "the pause");
        assert_eq!(run.step(t0 + Duration::from_millis(1500)), Step::Send(b));
        assert!(run.answer(b, false, t0 + Duration::from_secs(2)));
        assert_eq!(run.step(t0 + Duration::from_secs(3)), Step::Done);
        assert_eq!((names(&run.succeeded), names(&run.failed)), ("A".to_owned(), "B".to_owned()));
    }

    #[test]
    fn a_request_never_answered_fails() {
        let a = Uuid::from_u128(10);
        let mut run = BanAll::new(Uuid::from_u128(2), "Jane".into(), false, vec![(a, "A".into())]);
        let t0 = Instant::now();
        assert_eq!(run.step(t0), Step::Send(a));
        assert_eq!(run.step(t0 + GIVE_UP), Step::Wait, "failed, then the pause");
        assert_eq!(run.step(t0 + GIVE_UP + PAUSE), Step::Done);
        assert_eq!(run.failed, ["A"]);
        assert_eq!(names(&run.succeeded), "-");
    }
}
