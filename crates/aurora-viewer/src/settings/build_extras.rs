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
    /// FSTurnAvatarToSelectedObject (on): the avatar turns toward an object
    /// it selects, while the camera is not on the avatar.
    pub turn_to_selection: bool,
}

impl Default for BuildExtras {
    fn default() -> Self {
        BuildExtras {
            drag_texture: false,
            commit_sale_on_change: false,
            parcel_walls_to_max: false,
            inventory_scripts_mono: true,
            turn_to_selection: true,
        }
    }
}

/// LLToolSelect::handleObjectSelection's turn: the yaw that faces the
/// selection's centre `target` from `me`, when the avatar does not face it
/// already (its look direction `yaw` within about 53 degrees of it).
pub fn turn_toward(me: glam::Vec3, yaw: f32, target: glam::Vec3) -> Option<f32> {
    let dir = (target - me).truncate().try_normalize()?;
    let at = glam::Vec2::new(yaw.cos(), yaw.sin());
    (at.dot(dir) < 0.6).then(|| dir.y.atan2(dir.x))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_legacy_viewers_and_old_files_load() {
        let e: BuildExtras = serde_json::from_str("{}").unwrap();
        assert_eq!(e, BuildExtras::default());
        assert!(!e.drag_texture && !e.commit_sale_on_change && !e.parcel_walls_to_max);
        assert!(e.inventory_scripts_mono && e.turn_to_selection);
    }

    #[test]
    fn the_avatar_turns_only_when_it_does_not_face_the_selection() {
        use glam::Vec3;
        let me = Vec3::new(10.0, 10.0, 20.0);
        // facing +X, the object straight ahead: no turn
        assert_eq!(turn_toward(me, 0.0, Vec3::new(15.0, 10.5, 25.0)), None);
        // the object behind: turn to face it (-X)
        let yaw = turn_toward(me, 0.0, Vec3::new(5.0, 10.0, 20.0)).unwrap();
        assert!((yaw.abs() - std::f32::consts::PI).abs() < 1e-5);
        // to the left (+Y)
        let yaw = turn_toward(me, 0.0, Vec3::new(10.0, 14.0, 0.0)).unwrap();
        assert!((yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        // straight above: no direction
        assert_eq!(turn_toward(me, 0.0, Vec3::new(10.0, 10.0, 40.0)), None);
    }
}
