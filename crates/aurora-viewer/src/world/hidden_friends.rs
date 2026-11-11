//! Friends hidden in this viewer: a per-account list of friend ids kept in
//! `hidden_friends.json` in the account's folder (a JSON array of ids).
//!
//! Hiding is local only: nothing is sent to the server, the friendship and
//! its rights stay as they are. A hidden friend counts as a non-friend for
//! everything the viewer shows (`Social::is_friend_shown`,
//! `Social::shown_friends`): the friends lists and pickers, the online and
//! offline notices, the friend colours and "Show Friends Only". What the
//! friend sends (instant messages, calls, inventory and teleport offers)
//! goes through the real friendship (`Social::is_friend`) and arrives as
//! before.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const FILE: &str = "hidden_friends.json";

#[derive(Debug, Default)]
pub struct HiddenFriends {
    ids: BTreeSet<Uuid>,
    /// Where the list is saved; `None` before the login (and in tests).
    path: Option<PathBuf>,
}

/// The ids of a saved list. An entry that is not an id costs that entry
/// only; anything that is not a list gives none.
fn parse(bytes: &[u8]) -> Option<BTreeSet<Uuid>> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let entries = value.as_array()?;
    Some(
        entries
            .iter()
            .filter_map(|entry| Uuid::parse_str(entry.as_str()?).ok())
            .filter(|id| !id.is_nil())
            .collect(),
    )
}

impl HiddenFriends {
    /// The saved list; empty when there is none yet or it cannot be read.
    pub fn load(path: PathBuf) -> Self {
        let ids = match std::fs::read(&path) {
            Ok(bytes) => parse(&bytes).unwrap_or_else(|| {
                log::warn!("hidden friends: {} unreadable, starting empty", path.display());
                BTreeSet::new()
            }),
            Err(_) => BTreeSet::new(),
        };
        Self { ids, path: Some(path) }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Err(error) = self.write(path) {
            log::warn!("hidden friends: could not save {}: {error}", path.display());
        }
    }

    fn write(&self, path: &Path) -> std::io::Result<()> {
        let ids: Vec<String> = self.ids.iter().map(Uuid::to_string).collect();
        let bytes = serde_json::to_vec_pretty(&ids).map_err(std::io::Error::other)?;
        crate::atomic_file::write(path, &bytes)
    }

    pub fn contains(&self, id: &Uuid) -> bool {
        self.ids.contains(id)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[cfg(test)]
    pub fn ids(&self) -> impl Iterator<Item = &Uuid> {
        self.ids.iter()
    }

    /// Saved at once; false when the id was already there.
    pub(super) fn insert(&mut self, id: Uuid) -> bool {
        let added = !id.is_nil() && self.ids.insert(id);
        if added {
            self.save();
        }
        added
    }

    /// Saved at once; false when the id was not there.
    pub(super) fn remove(&mut self, id: &Uuid) -> bool {
        let removed = self.ids.remove(id);
        if removed {
            self.save();
        }
        removed
    }
}

impl super::World {
    /// Hides a friend in this viewer: nothing is sent and the friendship
    /// stays. "Show Friends Only" follows at once.
    pub fn hide_friend(&mut self, id: Uuid) -> bool {
        let hidden = self.social.hide_friend(id);
        if hidden {
            self.follow_friends_for_friends_only();
        }
        hidden
    }

    /// Shows a hidden friend again.
    pub fn unhide_friend(&mut self, id: Uuid) -> bool {
        let shown = self.social.unhide_friend(&id);
        if shown {
            self.follow_friends_for_friends_only();
        }
        shown
    }

    /// The contact set look of a resident drawn in the world and in chat
    /// (markers, minimap, name tags, name and chat colours): none for a
    /// hidden friend, whose sets would single them out.
    pub fn contact_appearance(&self, id: &Uuid) -> &std::sync::Arc<super::contact_sets::appearance::Appearance> {
        if self.social.is_friend_hidden(id) {
            super::contact_sets::ContactSets::no_appearance()
        } else {
            self.contact_sets.appearance(id)
        }
    }

    /// The members a contact set view lists (`ContactSets::members`),
    /// without the hidden friends.
    pub fn contact_set_members(&self, choice: &str) -> Vec<Uuid> {
        let social = &self.social;
        let friends: Vec<Uuid> = social.shown_friends().map(|f| f.id).collect();
        let mut members = self.contact_sets.members(choice, &friends, |id| social.is_friend(id));
        members.retain(|id| !social.is_friend_hidden(id));
        members
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aurora-hidden-friends-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        dir
    }

    #[test]
    fn the_list_survives_a_save_and_a_load() {
        let dir = temp_dir();
        let path = dir.join(FILE);
        let mut list = HiddenFriends::load(path.clone());
        assert!(list.is_empty(), "no file yet: nobody is hidden");
        assert!(list.insert(id(1)));
        assert!(list.insert(id(2)));
        assert!(!list.insert(id(2)), "hidden once only");
        assert!(list.remove(&id(1)));
        assert!(!list.remove(&id(1)));
        let loaded = HiddenFriends::load(path);
        assert_eq!(loaded.ids().copied().collect::<Vec<_>>(), vec![id(2)]);
        assert_eq!(loaded.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_file_keeps_the_entries_that_can_be_read() {
        let good = id(7);
        let text = format!(r#"["{good}", "not an id", 12, null, {{"id": "x"}}, "{}"]"#, Uuid::nil());
        let ids = parse(text.as_bytes()).expect("a list");
        assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec![good]);
    }

    #[test]
    fn a_file_that_is_not_a_list_hides_nobody() {
        let dir = temp_dir();
        for text in ["", "{", r#"{"a": 1}"#, "42", "\u{0}\u{1}"] {
            assert!(parse(text.as_bytes()).is_none(), "{text:?}");
            let path = dir.join(FILE);
            std::fs::write(&path, text).expect("the test file is written");
            assert!(HiddenFriends::load(path).is_empty(), "{text:?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn nothing_is_written_before_the_account_is_known() {
        let mut list = HiddenFriends::default();
        assert!(list.insert(id(3)));
        assert!(list.contains(&id(3)));
        assert!(!list.insert(Uuid::nil()));
    }

    use crate::world::World;
    use crate::world::social::{Friend, Social, rights};

    fn friend(n: u128) -> Friend {
        Friend {
            id: id(n),
            online: false,
            rights_given: rights::ONLINE_STATUS,
            rights_has: rights::ONLINE_STATUS | rights::MAP_LOCATION,
        }
    }

    fn social() -> Social {
        let mut social = Social::default();
        social.friends = vec![friend(1), friend(2)];
        for n in [1, 2, 3] {
            social.names.insert(id(n), format!("Resident {n}"));
        }
        social
    }

    #[test]
    fn a_hidden_friend_is_a_friend_that_is_not_shown() {
        let mut social = social();
        assert!(social.is_friend_shown(&id(1)) && !social.is_friend_hidden(&id(1)));
        assert!(social.hide_friend(id(1)));
        assert!(!social.hide_friend(id(1)), "already hidden");
        assert!(social.is_friend(&id(1)), "the friendship itself is untouched");
        assert!(!social.is_friend_shown(&id(1)) && social.is_friend_hidden(&id(1)));
        assert!(social.shown_friend(&id(1)).is_none());
        assert_eq!(social.shown_friends().map(|f| f.id).collect::<Vec<_>>(), vec![id(2)]);
        assert_eq!(social.hidden_friends().map(|f| f.id).collect::<Vec<_>>(), vec![id(1)]);
        assert_eq!(social.friends.len(), 2);
        // a stranger cannot be hidden, and is neither shown nor hidden
        assert!(!social.hide_friend(id(3)));
        assert!(!social.is_friend_shown(&id(3)) && !social.is_friend_hidden(&id(3)));
    }

    #[test]
    fn un_hiding_restores_the_friend_everywhere() {
        let mut social = social();
        social.hide_friend(id(1));
        social.set_online(&[id(1)], true);
        assert!(!social.can_map_friend(id(1)), "not found on the map while hidden");
        assert!(social.unhide_friend(&id(1)));
        assert!(!social.unhide_friend(&id(1)));
        assert!(social.is_friend_shown(&id(1)));
        assert_eq!(social.shown_friends().count(), 2);
        assert!(social.can_map_friend(id(1)));
        assert_eq!(social.hidden_friends().count(), 0);
    }

    #[test]
    fn a_hidden_friend_comes_and_goes_without_a_notice() {
        let mut social = social();
        social.hide_friend(id(1));
        social.set_online(&[id(1), id(2)], true);
        assert!(social.friends.iter().all(|f| f.online), "the state is still followed");
        let notices: Vec<Uuid> = social.take_online_notices().into_iter().map(|(who, ..)| who).collect();
        assert_eq!(notices, vec![id(2)]);
        social.set_online(&[id(1)], false);
        assert!(social.take_online_notices().is_empty());
        // hidden while the notice waited for its turn: dropped too
        social.set_online(&[id(2)], false);
        social.hide_friend(id(2));
        assert!(social.take_online_notices().is_empty());
        // shown again: the next change is announced
        social.unhide_friend(&id(1));
        social.set_online(&[id(1)], true);
        assert_eq!(social.take_online_notices().len(), 1);
    }

    #[test]
    fn a_friendship_that_ends_leaves_the_list() {
        let mut social = social();
        social.hide_friend(id(1));
        assert!(social.end_friendship(&id(1)));
        assert!(!social.hidden.contains(&id(1)));
        // friends again later: shown
        social.form_friendship(id(1));
        assert!(social.is_friend_shown(&id(1)));
    }

    fn message(from: Uuid) -> aurora_net::types::InstantMessage {
        aurora_net::types::InstantMessage {
            from_agent_id: from,
            from_name: "Resident".into(),
            to_agent_id: id(99),
            dialog: 0,
            session_id: Uuid::nil(),
            message: "hello".into(),
            offline: false,
            from_group: false,
            binary_bucket: Vec::new(),
            timestamp: 0,
        }
    }

    fn lines(world: &World, from: &Uuid) -> usize {
        world.social.ims.iter().find(|s| s.other == *from).map_or(0, |s| s.lines.len())
    }

    /// Even with "only friends can message me": the rule reads the
    /// friendship itself, so a hidden friend's message arrives as before.
    #[test]
    fn the_messages_of_a_hidden_friend_still_arrive() {
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        world.social.friends = vec![friend(1), friend(2)];
        world.status.cfg.friends_only = true;
        assert!(world.hide_friend(id(1)));
        assert!(!world.friends_only_drops(&id(1), "Resident"));
        assert!(world.friends_only_drops(&id(3), "Resident"), "a real stranger is still dropped");
        world.on_im(message(id(1)));
        world.on_im(message(id(2)));
        assert!(lines(&world, &id(1)) > 0, "the hidden friend's message is in its conversation");
        assert_eq!(lines(&world, &id(1)), lines(&world, &id(2)), "as a shown friend's");
    }

    #[test]
    fn contact_set_views_leave_the_hidden_friends_out() {
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        world.social.friends = vec![friend(1), friend(2)];
        world.contact_sets.add_set("Family");
        world.contact_sets.add_to_set(&[id(1), id(2)], "Family", |_| true);
        assert_eq!(world.contact_set_members("Family"), vec![id(1), id(2)]);
        world.hide_friend(id(1));
        assert_eq!(world.contact_set_members("Family"), vec![id(2)]);
        assert_eq!(world.contact_set_members(crate::world::contact_sets::ALL_SETS), vec![id(2)]);
        world.unhide_friend(id(1));
        assert_eq!(world.contact_set_members("Family"), vec![id(1), id(2)]);
    }

    /// A hidden friend's sets colour nothing in the world or in chat.
    #[test]
    fn a_hidden_friend_loses_the_look_of_its_sets() {
        use crate::world::contact_sets::appearance::Visuals;
        let mut world = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        world.social.friends = vec![friend(1)];
        world.contact_sets.add_set("Family");
        world.contact_sets.add_to_set(&[id(1)], "Family", |_| true);
        world.contact_sets.set_color("Family", [1.0, 0.0, 0.0, 1.0]);
        let visuals = Visuals {
            minimap: true,
            nameplate_name: true,
            ..Default::default()
        };
        world.contact_sets.set_visuals("Family", visuals);
        assert!(world.contact_appearance(&id(1)).minimap.is_some());
        world.hide_friend(id(1));
        let look = world.contact_appearance(&id(1));
        assert!(look.minimap.is_none() && look.nameplate_name.is_none());
        world.unhide_friend(id(1));
        assert!(world.contact_appearance(&id(1)).minimap.is_some());
    }
}
