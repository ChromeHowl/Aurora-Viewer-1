//! Smaller build options of the Legacy Viewer that Aurora lacked (Preferences
//! › Build). Held in `BuildSettings::extras`; every default is the Legacy
//! Viewer's (settings.xml names in comments).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BuildExtras {
    /// FSExperimentalDragTexture: in the Select Face mode, a press on the
    /// face already selected drags its texture (build/texture_drag.rs).
    pub drag_texture: bool,
    /// FSCommitForSaleOnChange (off): the General tab's "For sale" box and
    /// sale type apply at once, without the Apply button (old behaviour).
    pub commit_sale_on_change: bool,
    /// FSRenderParcelSelectionToMaxBuildHeight (off): the selected parcel's
    /// walls rise to the highest build level (build/land.rs).
    pub parcel_walls_to_max: bool,
    /// FSSaveInventoryScriptsAsMono (on): a script edited from the
    /// inventory is saved as Mono, else as LSL2.
    pub inventory_scripts_mono: bool,
}

impl Default for BuildExtras {
    fn default() -> Self {
        BuildExtras {
            drag_texture: false,
            commit_sale_on_change: false,
            parcel_walls_to_max: false,
            inventory_scripts_mono: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_old_files_load() {
        let e: BuildExtras = serde_json::from_str("{}").unwrap();
        assert_eq!(e, BuildExtras::default());
        assert!(!e.drag_texture && !e.commit_sale_on_change && !e.parcel_walls_to_max);
        assert!(e.inventory_scripts_mono);
    }
}
