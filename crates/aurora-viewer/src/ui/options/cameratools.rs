//! The Cameratools window (World › Photo and video › Cameratools, the
//! toolbar's `phototools_view`): the camera's settings gathered in one
//! window, as the Legacy Viewer's floater_phototools_camera.xml. It draws
//! the rows of Preferences › Camera themselves (their tabs: view,
//! behaviour, mouselook), and the mouse's sensitivity, so a setting has one
//! row, one help text and one reset wherever it shows. The joysticks stay
//! in the Controls window and the 3D mouse in Preferences › Joystick.

use super::cx::{Cx, Mode};
use super::{OptionsResult, OptionsUi, Page};
use crate::settings::Settings;
use crate::theme::Palette;
use crate::ui::widgets::{self, Floater};
use egui::{Pos2, Vec2};

pub const WINDOW: &str = "cameratools";

#[derive(Default)]
pub struct CameratoolsUi {
    tab: usize,
    /// The settings' defaults, for the rows' modified marks.
    defaults: Option<Box<Settings>>,
}

impl CameratoolsUi {
    /// The window, when `open`; true when a setting changed (the app saves
    /// and applies them as for Preferences).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        open: &mut bool,
        s: &mut Settings,
        st: &mut OptionsUi,
        r: &mut OptionsResult,
    ) -> bool {
        if !*open {
            return false;
        }
        let d: &Settings = self.defaults.get_or_insert_with(Default::default);
        let screen = ctx.content_rect();
        let pos = Pos2::new((screen.width() - 620.0).max(0.0) * 0.5, 90.0);
        let mut floater = Floater::new(WINDOW, aurora_i18n::tr!("camx-cameratools-title"), pos, Vec2::new(620.0, 560.0));
        floater.min_size = Vec2::new(480.0, 320.0);
        let tab = &mut self.tab;
        let mut changed = false;
        floater.show(ctx, p, open, |ui| {
            let labels: Vec<(&str, bool)> = super::pages::tabs(Page::Camera).into_iter().map(|t| (t, true)).collect();
            widgets::tabs(ui, p, tab, &labels);
            *tab = (*tab).min(labels.len().saturating_sub(1));
            egui::ScrollArea::vertical()
                .id_salt(("cameratools", *tab))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut cx = Cx::new(ui, *p, s, d, st, r, Mode::Show, "");
                    cx.enter(Page::Camera, *tab);
                    super::pages::draw(&mut cx, Page::Camera, *tab);
                    if *tab == 2 {
                        // "Mouse sensitivity" of the Legacy window's second tab
                        cx.borrow(Page::Controls, 0, &["mouse_sensitivity", "invert_mouse"]);
                    }
                    changed = cx.changed;
                });
        });
        changed
    }
}
