//! A list of resident ids kept by this viewer for one account, in a file of
//! the account's folder (a JSON array of ids): the hidden friends
//! (`hidden_friends.rs`) and the Shadow Realm (`shadow_realm.rs`).
//!
//! Local only: nothing of it is sent to the server, unlike the block list.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct AccountIds {
    ids: BTreeSet<Uuid>,
    /// Where the list is saved; `None` before the login (and in tests).
    path: Option<PathBuf>,
}

/// The ids of a saved list. An entry that is not an id costs that entry
/// only; anything that is not a list gives none.
pub(super) fn parse(bytes: &[u8]) -> Option<BTreeSet<Uuid>> {
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

impl AccountIds {
    /// The saved list; empty when there is none yet or it cannot be read.
    pub fn load(path: PathBuf) -> Self {
        let ids = match std::fs::read(&path) {
            Ok(bytes) => parse(&bytes).unwrap_or_else(|| {
                log::warn!("{} unreadable, starting empty", path.display());
                BTreeSet::new()
            }),
            Err(_) => BTreeSet::new(),
        };
        Self { ids, path: Some(path) }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Err(error) = self.write(path) {
            log::warn!("could not save {}: {error}", path.display());
        }
    }

    fn write(&self, path: &Path) -> std::io::Result<()> {
        let ids: Vec<String> = self.ids.iter().map(Uuid::to_string).collect();
        let bytes = serde_json::to_vec_pretty(&ids).map_err(std::io::Error::other)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::atomic_file::write(path, &bytes)
    }

    pub fn contains(&self, id: &Uuid) -> bool {
        self.ids.contains(id)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aurora-account-ids-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        dir
    }

    #[test]
    fn the_list_survives_a_save_and_a_load() {
        let dir = temp_dir();
        let path = dir.join("list.json");
        let mut list = AccountIds::load(path.clone());
        assert!(list.is_empty(), "no file yet: nobody listed");
        assert!(list.insert(id(1)));
        assert!(list.insert(id(2)));
        assert!(!list.insert(id(2)), "listed once only");
        assert!(list.remove(&id(1)));
        assert!(!list.remove(&id(1)));
        let loaded = AccountIds::load(path);
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
    fn a_file_that_is_not_a_list_lists_nobody() {
        let dir = temp_dir();
        for text in ["", "{", r#"{"a": 1}"#, "42", "\u{0}\u{1}"] {
            assert!(parse(text.as_bytes()).is_none(), "{text:?}");
            let path = dir.join("list.json");
            std::fs::write(&path, text).expect("the test file is written");
            assert!(AccountIds::load(path).is_empty(), "{text:?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn nothing_is_written_before_the_account_is_known() {
        let mut list = AccountIds::default();
        assert!(list.insert(id(3)));
        assert!(list.contains(&id(3)));
        assert!(!list.insert(Uuid::nil()));
    }

    #[test]
    fn a_missing_account_folder_is_made_on_the_first_save() {
        let dir = temp_dir();
        let path = dir.join("account").join("list.json");
        let mut list = AccountIds::load(path.clone());
        assert!(list.insert(id(4)));
        assert!(AccountIds::load(path).contains(&id(4)));
        let _ = std::fs::remove_dir_all(dir);
    }
}
