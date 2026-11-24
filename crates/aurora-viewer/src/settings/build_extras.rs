//! Smaller build options of the Legacy Viewer that Aurora lacked (Preferences
//! › Build). Held in `BuildSettings::extras`; every default is the Legacy
//! Viewer's (settings.xml names in comments).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BuildExtras {
    /// FSExperimentalDragTexture (off): in the Select Face mode, a press on
    /// the face already selected drags its texture (build/texture_drag.rs).
    pub drag_texture: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_old_files_load() {
        let e: BuildExtras = serde_json::from_str("{}").unwrap();
        assert_eq!(e, BuildExtras::default());
        assert!(!e.drag_texture);
    }
}
