//! The photo's own quality values, chosen in the viewfinder (Photo-only
//! quality sheet): how much of the "Photo quality" boost (`quality`) a photo
//! takes. The boost raises the anti-aliasing to MSAA 8x, the shadows to the
//! top level and the draw distance to 512 m; here each of the three can be
//! held lower (or left at the user's own value), for a lighter photo.
//!
//! They only ever apply inside the boost: its values are written while the
//! photo window is open and the user's own come back with
//! `quality::Held::restore`, so nothing here reaches the normal view. A
//! choice never goes below the user's own setting (the boost never lowers a
//! setting) and the default, `Max`, changes nothing. Aurora's own; the
//! Legacy Viewer has no such boost.

use crate::graphics_presets::Values;
use serde::{Deserialize, Serialize};

use super::quality::{MSAA_2X, MSAA_4X, samples};

/// The anti-aliasing a photo is drawn with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Antialiasing {
    /// MSAA 8x (the boost's own).
    #[default]
    Max,
    X4,
    X2,
    /// The user's setting.
    Own,
}

/// The shadow level a photo is drawn with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shadows {
    /// The top level (the boost's own).
    #[default]
    Max,
    /// Level 3.
    High,
    /// The user's setting.
    Own,
}

/// The draw distance a photo is drawn with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reach {
    /// 512 m (the boost's own).
    #[default]
    Far,
    /// 256 m.
    Mid,
    /// The user's setting.
    Own,
}

impl Antialiasing {
    pub const ALL: [Antialiasing; 4] = [Antialiasing::Max, Antialiasing::X4, Antialiasing::X2, Antialiasing::Own];
}

impl Shadows {
    pub const ALL: [Shadows; 3] = [Shadows::Max, Shadows::High, Shadows::Own];
}

impl Reach {
    pub const ALL: [Reach; 3] = [Reach::Far, Reach::Mid, Reach::Own];
}

/// The draw distance `Reach::Mid` stops at (m).
pub const MID_REACH: f32 = 256.0;
/// The shadow level `Shadows::High` stops at.
pub const HIGH_SHADOWS: u8 = 3;

/// The three choices (`snapshot::Settings::quality_tune`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tune {
    pub antialiasing: Antialiasing,
    pub shadows: Shadows,
    pub reach: Reach,
}

impl Tune {
    /// The boost's `boosted` values held to this choice: never below the
    /// user's own, and nothing else touched.
    pub fn apply(self, boosted: Values, user: Values) -> Values {
        let mut v = boosted;
        let aa = match self.antialiasing {
            Antialiasing::Max => v.antialiasing,
            Antialiasing::X4 => capped(v.antialiasing, MSAA_4X),
            Antialiasing::X2 => capped(v.antialiasing, MSAA_2X),
            Antialiasing::Own => user.antialiasing,
        };
        v.antialiasing = if samples(aa) < samples(user.antialiasing) {
            user.antialiasing
        } else {
            aa
        };
        v.shadow_quality = match self.shadows {
            Shadows::Max => v.shadow_quality,
            Shadows::High => v.shadow_quality.min(HIGH_SHADOWS).max(user.shadow_quality),
            Shadows::Own => user.shadow_quality,
        };
        v.draw_distance = match self.reach {
            Reach::Far => v.draw_distance,
            Reach::Mid => v.draw_distance.min(MID_REACH).max(user.draw_distance),
            Reach::Own => user.draw_distance,
        };
        v
    }
}

/// An anti-aliasing setting held to the samples of `cap`.
fn capped(aa: u8, cap: u8) -> u8 {
    if samples(aa) > samples(cap) { cap } else { aa }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::snapshot::quality::{MSAA_8X, boosted};

    fn user() -> Values {
        Values::read(&Settings {
            draw_distance: 128.0,
            shadow_quality: 1,
            antialiasing: MSAA_2X,
            ..Settings::default()
        })
    }

    #[test]
    fn the_default_changes_nothing() {
        let u = user();
        assert_eq!(Tune::default().apply(boosted(u), u), boosted(u));
    }

    #[test]
    fn each_choice_holds_its_setting_lower() {
        let u = user();
        let b = boosted(u);
        assert_eq!(b.antialiasing, MSAA_8X);
        let t = Tune {
            antialiasing: Antialiasing::X4,
            shadows: Shadows::High,
            reach: Reach::Mid,
        };
        let v = t.apply(b, u);
        assert_eq!((v.antialiasing, v.shadow_quality, v.draw_distance), (MSAA_4X, 3, 256.0));
        let v = Tune {
            antialiasing: Antialiasing::X2,
            ..t
        }
        .apply(b, u);
        assert_eq!(v.antialiasing, MSAA_2X);
    }

    #[test]
    fn own_gives_the_users_values_back_for_that_setting_only() {
        let u = user();
        let b = boosted(u);
        let t = Tune {
            antialiasing: Antialiasing::Own,
            shadows: Shadows::Own,
            reach: Reach::Own,
        };
        let v = t.apply(b, u);
        assert_eq!((v.antialiasing, v.shadow_quality, v.draw_distance), (u.antialiasing, 1, 128.0));
        assert_eq!(v.lod_factor, b.lod_factor, "the rest of the boost stays");
        assert_eq!(v.max_avatars, b.max_avatars);
    }

    #[test]
    fn a_choice_never_goes_below_the_users_own_setting() {
        let mut u = user();
        u.antialiasing = MSAA_4X;
        u.shadow_quality = 4;
        u.draw_distance = 400.0;
        let t = Tune {
            antialiasing: Antialiasing::X2,
            shadows: Shadows::High,
            reach: Reach::Mid,
        };
        let v = t.apply(boosted(u), u);
        assert_eq!((v.antialiasing, v.shadow_quality, v.draw_distance), (MSAA_4X, 4, 400.0));
        // (a user's 1,024 m is the boost's too, and a cap never lowers it past theirs)
        u.draw_distance = 1024.0;
        assert_eq!(t.apply(boosted(u), u).draw_distance, 1024.0);
    }

    #[test]
    fn old_settings_files_load_with_the_default() {
        let t: Tune = serde_json::from_str("{}").expect("load");
        assert_eq!(t, Tune::default());
        let t: Tune = serde_json::from_str(r#"{"reach": "Own"}"#).expect("load");
        assert_eq!((t.reach, t.shadows), (Reach::Own, Shadows::Max));
    }
}
