//! The Shadow Realm's window and its demo (`world/shadow_realm.rs`,
//! `ui/shadow_realm.rs`).
use super::*;
use crate::ui::shadow_realm::{Action, Row};

impl App {
    /// World › Shadow Realm…, while open.
    pub(super) fn shadow_realm_window(&mut self, ctx: &egui::Context, p: &crate::theme::Palette) {
        if !self.panels.shadow_realm {
            return;
        }
        let ids: Vec<uuid::Uuid> = self.world.shadow_realm.ids().copied().collect();
        let rows: Vec<Row> = ids
            .into_iter()
            .map(|id| {
                let can_name = self.world.rlva_can_name(id);
                let name = if can_name {
                    self.world.social.want_name(id);
                    self.world.social.name_of(&id)
                } else {
                    crate::world::rlva::privacy::anonymous(id).display_name.clone()
                };
                let shown = !self.world.shadow_hides(&id);
                Row { id, name, can_name, shown }
            })
            .collect();
        let mut settings = self.settings.shadow_realm;
        let (action, changed) = self
            .shadow_realm_ui
            .show(ctx, p, &rows, &mut settings, &mut self.world, &mut self.panels.shadow_realm);
        if changed || settings != self.settings.shadow_realm {
            self.settings.shadow_realm = settings;
            self.settings.save();
            self.world.tick_shadow_realm(&self.settings.shadow_realm, Instant::now());
        }
        match action {
            Some(Action::Add(ids)) => {
                for id in ids {
                    self.world.shadow_realm_add(id);
                }
            }
            Some(Action::Remove(ids)) => {
                for id in ids {
                    self.world.shadow_realm_remove(&id);
                }
            }
            Some(Action::Profile(id)) => self.on_ctx_action(crate::ui::context::CtxAction::Profile(id)),
            None => {}
        }
    }

    /// AURORA_DEMO_SHADOW_REALM=1|remove|off (docs/TESTING.md): Violet Wolf
    /// listed at frame 650 with a 1 m distance (gone with what he wears),
    /// the distance at 40 m at frame 800 (back, his name red), then at 950
    /// removed from the list (`remove`) or the feature switched off (`off`
    /// and `1`): back in his usual colours. Never saved.
    pub(super) fn demo_shadow_realm_steps(&mut self) {
        if !self.demo {
            return;
        }
        let Ok(mode) = std::env::var("AURORA_DEMO_SHADOW_REALM") else {
            return;
        };
        let wolf = crate::demo::DEMO_LOUP;
        let now = Instant::now();
        match self.frame_count {
            650 => {
                self.settings.shadow_realm = crate::settings::shadow_realm::ShadowRealm {
                    enabled: true,
                    distance_m: 1.0,
                };
                // a list of this run only: nothing written to the account
                self.world.shadow_realm = Default::default();
                self.world.shadow_realm_add(wolf);
                ui::widgets::open_floater(&self.egui_ctx, ui::shadow_realm::WINDOW, &mut self.panels.shadow_realm);
            }
            800 => self.settings.shadow_realm.distance_m = 40.0,
            950 if mode.trim() == "remove" => {
                self.world.shadow_realm_remove(&wolf);
            }
            950 => self.settings.shadow_realm.enabled = false,
            _ => return,
        }
        self.world.tick_shadow_realm(&self.settings.shadow_realm, now);
        log::info!(
            "demo shadow realm: frame {} wolf hidden {} red {}",
            self.frame_count,
            self.world.shadow_hides(&wolf),
            self.world.shadow_red(&wolf)
        );
    }
}
