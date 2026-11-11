//! Bulk Windlight import: several Windlight XML files of one kind (skies,
//! water or day cycles), each converted and saved as a new environment
//! settings item in the Settings folder. Port of import_windlight_bulk
//! (indra/newview/llviewermenufile.cpp, originally LGPL 2.1): the item takes
//! the file's unescaped name; a file that does not convert is reported with
//! WLImportFail's reasons and the others go on; a single notice says when
//! all are done. Settings items carry no upload fee.

use super::App;
use crate::world::env_edit::{self, SettingsDoc};
use crate::world::env_select::SettingsKind;
use crate::world::notifications::{Data, Kind};
use aurora_net::NetCommand;
use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc::Receiver;
use uuid::Uuid;

/// One converted file: its item name, LLSettingsType subtype and asset, or
/// why it was refused.
pub(super) type Converted = Result<(String, u8, Vec<u8>), String>;

#[derive(Default)]
pub(super) struct EnvBulk {
    /// The files being read and converted, off the frame.
    reading: Option<Receiver<Vec<Converted>>>,
    /// The items being created.
    pending: HashSet<Uuid>,
    created: usize,
    failed: Vec<String>,
}

impl EnvBulk {
    /// File.EnableImportWindlightBulk: one import at a time.
    pub(super) fn active(&self) -> bool {
        self.reading.is_some() || !self.pending.is_empty()
    }
}

/// LLSettingsType of a kind: 0 sky, 1 water, 2 day cycle.
pub(super) fn subtype(kind: SettingsKind) -> u8 {
    match kind {
        SettingsKind::Sky => 0,
        SettingsKind::Water => 1,
        SettingsKind::Day => 2,
    }
}

/// createSkyFromLegacyPreset / createWaterFromLegacyPreset /
/// createDayCycleFromLegacyPreset on one file.
pub(super) fn convert(kind: SettingsKind, path: &Path) -> Converted {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = env_edit::legacy_preset_name(&stem);
    let shown = path.display().to_string();
    let refused = || aurora_i18n::tr!("l3-env-bulk-failed-file", file = shown.as_str());
    let data = match kind {
        SettingsKind::Day => super::env_apply::import_day_file(path)?.to_asset(),
        SettingsKind::Sky | SettingsKind::Water => {
            let xml = std::fs::read(path).map_err(|e| {
                log::warn!("windlight import {shown}: {e}");
                refused()
            })?;
            convert_preset(kind, &xml, &name).ok_or_else(refused)?
        }
    };
    Ok((name, subtype(kind), data))
}

/// A sky or water Windlight preset as a settings asset.
pub(super) fn convert_preset(kind: SettingsKind, xml: &[u8], name: &str) -> Option<Vec<u8>> {
    let doc = if kind == SettingsKind::Water {
        SettingsDoc::import_legacy_water(xml, name)
    } else {
        SettingsDoc::import_legacy_sky(xml, name)
    }?;
    Some(doc.to_asset())
}

impl App {
    /// World › Environment › Import Windlight: pick the files, read and
    /// convert them off the frame.
    pub(super) fn env_bulk_import(&mut self, kind: SettingsKind) {
        if self.env_bulk.active() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.env_bulk.reading = Some(rx);
        std::thread::spawn(move || {
            let files = rfd::FileDialog::new()
                .set_title(aurora_i18n::tr!("l3-env-bulk-pick"))
                .add_filter("XML", &["xml"])
                .pick_files()
                .unwrap_or_default();
            let converted = files.iter().map(|path| convert(kind, path)).collect();
            let _ = tx.send(converted);
        });
    }

    /// Every frame: the converted files become new items in the Settings
    /// folder.
    pub(super) fn poll_env_bulk(&mut self) {
        let Some(rx) = &self.env_bulk.reading else { return };
        let Ok(converted) = rx.try_recv() else { return };
        self.env_bulk.reading = None;
        self.env_bulk.created = 0;
        self.env_bulk.failed.clear();
        let inv = &self.world.inventory;
        let parent = crate::world::inventory::actions::system(inv, 56).unwrap_or(inv.root);
        let mut refused = Vec::new();
        for file in converted {
            match file {
                Ok((name, subtype, data)) => {
                    let request = Uuid::new_v4();
                    self.env_bulk.pending.insert(request);
                    self.send(NetCommand::CreateSettingsItem {
                        request,
                        parent,
                        name,
                        subtype,
                        next_owner_mask: None,
                        data,
                    });
                }
                Err(reason) => refused.push(reason),
            }
        }
        if !refused.is_empty() {
            self.world
                .notifications
                .push(Kind::Alert, aurora_i18n::tr!("l3-env-bulk-title"), refused.join("\n"), Data::None);
        }
        if self.env_bulk.pending.is_empty() && !refused.is_empty() {
            self.env_bulk_finished();
        }
    }

    /// The answer to one of the import's items; false for any other request.
    pub(super) fn env_bulk_saved(&mut self, request: Uuid, result: &Result<aurora_net::inventory::InvItem, String>) -> bool {
        if !self.env_bulk.pending.remove(&request) {
            return false;
        }
        match result {
            Ok(it) => {
                self.world.inventory.add_items(vec![it.clone()]);
                self.env_bulk.created += 1;
            }
            Err(e) => {
                log::warn!("windlight import: settings item not created: {e}");
                self.env_bulk.failed.push(e.clone());
            }
        }
        if self.env_bulk.pending.is_empty() {
            self.env_bulk_finished();
        }
        true
    }

    /// WindlightBulkImportFinished.
    fn env_bulk_finished(&mut self) {
        let created = self.env_bulk.created.to_string();
        let failed = self.env_bulk.failed.len().to_string();
        let text = aurora_i18n::tr!("l3-env-bulk-finished", created = created.as_str(), failed = failed.as_str());
        self.world
            .notifications
            .push(Kind::Info, aurora_i18n::tr!("l3-env-bulk-title"), text, Data::None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtypes_follow_the_settings_types() {
        assert_eq!(subtype(SettingsKind::Sky), 0);
        assert_eq!(subtype(SettingsKind::Water), 1);
        assert_eq!(subtype(SettingsKind::Day), 2);
    }

    #[test]
    fn a_file_that_is_not_windlight_is_refused_with_its_name() {
        assert!(convert_preset(SettingsKind::Sky, b"not xml", "x").is_none());
        let missing = Path::new("/nonexistent/folder/A%20Sky.xml");
        let reason = convert(SettingsKind::Sky, missing).expect_err("missing file");
        assert!(reason.contains("A%20Sky.xml"));
    }

    #[test]
    fn a_built_in_preset_converts() {
        use crate::world::env_day_edit::LegacyFolder;
        let names = crate::world::env_day_edit::legacy_file_names("Default");
        let Some(xml) = names
            .iter()
            .find_map(|n| crate::world::windlight_presets::file(LegacyFolder::Skies, n))
        else {
            return;
        };
        assert!(convert_preset(SettingsKind::Sky, &xml, "Default").is_some_and(|a| !a.is_empty()));
    }
}
