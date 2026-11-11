//! Teleport and logout screen options of Preferences › Mouse and keyboard ›
//! Teleports and map, from the Legacy Viewer's settings.xml:
//! FSAlwaysShowTPCancel ("Always show the TP cancel button even if the sim
//! says it can't be canceled") and FSDisableLogoutScreens ("Disable logout
//! screen progress bar"); same defaults, both off.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TeleportExtras {
    /// The teleport's Cancel button shows even when the simulator says the
    /// teleport cannot be cancelled; RLVa still hides it.
    pub always_cancel: bool,
    /// Logging out keeps the world in view, with a small progress bar
    /// instead of the full logout screen.
    pub no_logout_screen: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_keep_the_legacy_defaults() {
        let t: TeleportExtras = serde_json::from_str("{}").expect("empty block");
        assert_eq!(t, TeleportExtras::default());
        assert!(!t.always_cancel && !t.no_logout_screen);
        let t: TeleportExtras = serde_json::from_str(r#"{"always_cancel":true}"#).expect("partial");
        assert!(t.always_cancel && !t.no_logout_screen);
    }
}
