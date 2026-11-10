//! The pose stand's window (World › Pose stand, the toolbar's pose_stand):
//! the poses of `ik_edit::poser::stand`, one chosen at a time, held on our
//! avatar while the window is open (floater_fs_posestand.xml). The pose is
//! local, like the poser's: the window says so.
use super::widgets::{self, Floater};
use crate::ik_edit::poser::stand::StandPose;
use crate::theme::Palette;

/// The window's id (Floater.Show "fs_posestand").
pub const WINDOW: &str = "pose_stand";

#[derive(Default)]
pub struct PoseStandUi;

impl PoseStandUi {
    /// `held`: the pose shows (the poser or IK Edit does not hold our
    /// avatar). True when the choice changed.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, pose: &mut StandPose, held: bool, open: &mut bool) -> bool {
        let mut changed = false;
        Floater::new(
            WINDOW,
            aurora_i18n::tr!("toolbar-command-pose-stand"),
            egui::pos2(90.0, 150.0),
            egui::vec2(290.0, 420.0),
        )
        .fixed()
        .title_note(aurora_i18n::tr!("poser-visible-you"))
        .title_note_tooltip(aurora_i18n::tr!("b4-stand-local-tip"))
        .show(ctx, p, open, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            for choice in StandPose::ALL {
                if widgets::radio(ui, p, *pose == choice, choice.label()).clicked() && *pose != choice {
                    *pose = choice;
                    changed = true;
                }
            }
            if !held {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(aurora_i18n::tr!("b4-stand-waiting")).color(p.muted));
            }
        });
        changed
    }
}
