//! The Shadow Realm's switch and distance (World › Shadow Realm…). The list
//! of residents is per account and kept apart (`world/shadow_realm.rs`).

use serde::{Deserialize, Serialize};

/// The distance a new settings file starts with (m).
pub const DEFAULT_DISTANCE: f32 = 10.0;
/// The distance's bounds (m): a resident nearer than the smallest still
/// shows; past the largest the whole region would.
pub const MIN_DISTANCE: f32 = 1.0;
pub const MAX_DISTANCE: f32 = 1000.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShadowRealm {
    /// The residents of the list are hidden while farther than `distance_m`.
    pub enabled: bool,
    /// Within this distance a listed resident shows (in red).
    pub distance_m: f32,
}

impl Default for ShadowRealm {
    fn default() -> Self {
        Self {
            enabled: true,
            distance_m: DEFAULT_DISTANCE,
        }
    }
}

impl ShadowRealm {
    /// A distance edited in the file by hand keeps within its bounds.
    pub fn sanitize(&mut self) {
        self.distance_m = if self.distance_m.is_finite() {
            self.distance_m.clamp(MIN_DISTANCE, MAX_DISTANCE)
        } else {
            DEFAULT_DISTANCE
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_file_without_the_section_takes_the_defaults() {
        let s: ShadowRealm = serde_json::from_str("{}").expect("defaults");
        assert_eq!(s, ShadowRealm::default());
        assert_eq!(s.distance_m, 10.0);
        assert!(s.enabled);
    }

    #[test]
    fn the_distance_keeps_within_its_bounds() {
        for (given, kept) in [
            (0.0, MIN_DISTANCE),
            (-5.0, MIN_DISTANCE),
            (5000.0, MAX_DISTANCE),
            (f32::NAN, DEFAULT_DISTANCE),
            (25.0, 25.0),
        ] {
            let mut s = ShadowRealm {
                enabled: true,
                distance_m: given,
            };
            s.sanitize();
            assert_eq!(s.distance_m, kept, "{given}");
        }
    }
}
