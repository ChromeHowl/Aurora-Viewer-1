//! Logout screen option of Preferences › Mouse and keyboard › Teleports and
//! map, from the Legacy Viewer's settings.xml: FSDisableLogoutScreens
//! ("Disable logout screen progress bar"); same default, off.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TeleportExtras {
    /// Logging out keeps the world in view, with a small progress bar
    /// instead of the full logout screen.
    pub no_logout_screen: bool,
    /// The former "Always allow cancelling a teleport", read once: it became
    /// `Settings::always_show_tp_cancel` (FSAlwaysShowTPCancel).
    #[serde(rename = "always_cancel", skip_serializing)]
    pub(crate) former_always_cancel: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_keep_the_legacy_defaults() {
        let t: TeleportExtras = serde_json::from_str("{}").expect("empty block");
        assert_eq!(t, TeleportExtras::default());
        assert!(!t.no_logout_screen);
        let t: TeleportExtras = serde_json::from_str(r#"{"always_cancel":true}"#).expect("partial");
        assert_eq!(t.former_always_cancel, Some(true));
        assert!(!t.no_logout_screen);
        let saved = serde_json::to_value(t).expect("save");
        assert!(saved.get("always_cancel").is_none(), "only the new setting is saved");
    }
}
