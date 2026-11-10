//! Inventory extras of Preferences › Inventory: the default format of a
//! texture's "Save as" (FSTextureDefaultSaveAsFormat, used by
//! LLPreviewTexture::saveAs in llpreviewtexture.cpp). Aurora keeps its own
//! default, asking each time; the Legacy Viewer's default is TGA.

use serde::{Deserialize, Serialize};

/// The format a texture preview saves to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureSaveFormat {
    /// Offer PNG and TGA at each save.
    #[default]
    Ask,
    Png,
    Tga,
}

impl TextureSaveFormat {
    /// The fixed choice: Some(true) for PNG, Some(false) for TGA.
    pub fn png(self) -> Option<bool> {
        match self {
            TextureSaveFormat::Ask => None,
            TextureSaveFormat::Png => Some(true),
            TextureSaveFormat::Tga => Some(false),
        }
    }
}

/// The settings of this module (`Settings::inventory_extras`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InventoryExtras {
    pub texture_save_format: TextureSaveFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_save_format_reads_back_and_old_files_ask() {
        assert_eq!(TextureSaveFormat::default().png(), None);
        assert_eq!(TextureSaveFormat::Png.png(), Some(true));
        assert_eq!(TextureSaveFormat::Tga.png(), Some(false));
        let saved = InventoryExtras {
            texture_save_format: TextureSaveFormat::Tga,
        };
        let text = serde_json::to_string(&saved).expect("save");
        assert_eq!(serde_json::from_str::<InventoryExtras>(&text).expect("load"), saved);
        assert_eq!(
            serde_json::from_str::<InventoryExtras>("{}").expect("old"),
            InventoryExtras::default()
        );
    }
}
