//! The "Import scripts" window: the list of files with the state of each
//! one, the destination (an inventory folder, or an object's contents), and
//! the buttons. An Aurora addition (the Legacy Viewer has no bulk script
//! import); the rules are in `import_rules.rs`, the work in
//! `app/script_import.rs`.

use super::import_rules::{Batch, Kind, Row, State};
use crate::theme::Palette;
use crate::ui::widgets;
use crate::world::inventory::Inventory;
use egui::{Color32, RichText, Vec2};
use uuid::Uuid;

pub const WINDOW: &str = "script_import";

/// The compile errors shown under a row; the editor holds all of them.
const ERRORS_SHOWN: usize = 3;

/// Where the scripts go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// A folder of the inventory.
    Folder(Uuid),
    /// The contents of an object (its full id and its name).
    Object { object: Uuid, name: String },
}

/// What the window asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    AddFiles,
    Import,
    Stop,
    Clear,
    /// Open the script of this inventory item in the editor.
    Open(Uuid),
}

pub struct Form {
    pub open: bool,
    pub batch: Batch,
    pub destination: Destination,
    /// What `.lsl` and `.txt` files compile for: Mono or LSL.
    pub lsl: Kind,
    /// Object destination: the scripts run once they are in.
    pub run: bool,
    /// The region runs SLua.
    pub lua_region: bool,
    /// Files being read.
    pub loading: bool,
    /// A line under the list: what was left out, what is missing.
    pub note: String,
    pub picker: crate::imports::ui::destination::Picker,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            open: false,
            batch: Batch::default(),
            destination: Destination::Folder(Uuid::nil()),
            lsl: Kind::Mono,
            run: true,
            lua_region: false,
            loading: false,
            note: String::new(),
            picker: Default::default(),
        }
    }
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Mono => aurora_i18n::tr!("script-import-kind-mono"),
        Kind::Lsl => aurora_i18n::tr!("script-import-kind-lsl"),
        Kind::Slua => aurora_i18n::tr!("script-import-kind-slua"),
    }
}

fn size_label(bytes: usize) -> String {
    format!("{:.1} KB", bytes as f64 / 1024.0)
}

/// The icon, the color and the words of a row's state.
fn status(p: &Palette, row: &Row, object: bool) -> (&'static str, Color32, String) {
    match &row.state {
        State::Ready => ("clock-filled", p.muted, aurora_i18n::tr!("script-import-state-ready").into()),
        State::Queued => ("hourglass-filled", p.muted, aurora_i18n::tr!("script-import-state-queued").into()),
        State::Working => ("hourglass-filled", p.violet_light, aurora_i18n::tr!("script-import-state-working").into()),
        State::Saved | State::Unchecked => {
            let text = match (&row.state, object && row.placed) {
                (State::Unchecked, _) => aurora_i18n::tr!("script-import-state-unchecked"),
                (_, true) => aurora_i18n::tr!("script-import-state-saved-object"),
                _ => aurora_i18n::tr!("script-import-state-saved"),
            };
            ("circle-check-filled", p.success, text.into())
        }
        State::CompileFailed(errors) => (
            "alert-triangle-filled",
            p.warn,
            aurora_i18n::tr!("script-import-state-errors", count = errors.len().max(1)),
        ),
        State::Failed { reason, kept } => (
            "circle-x-filled",
            p.danger,
            if *kept {
                aurora_i18n::tr!("script-import-state-kept", reason = reason.as_str())
            } else {
                aurora_i18n::tr!("script-import-state-failed", reason = reason.as_str())
            },
        ),
        State::Skipped(skip) => ("circle-x-filled", p.muted, skip.text()),
        State::Cancelled => ("circle-x-filled", p.muted, aurora_i18n::tr!("script-import-state-cancelled").into()),
    }
}

/// The window, while `form.open`.
pub fn window(ctx: &egui::Context, p: &Palette, form: &mut Form, inventory: &Inventory) -> Option<Action> {
    if !form.open {
        return None;
    }
    let mut open = true;
    let mut action = None;
    let size = egui::vec2(520.0, 460.0);
    widgets::Floater::new(
        WINDOW,
        aurora_i18n::tr!("script-import-title"),
        ctx.content_rect().center() - size / 2.0,
        size,
    )
    .steady_size()
    .show(ctx, p, &mut open, |ui| {
        let (mut above, footer) = widgets::above_footer(ui);
        top(&mut above, p, form, inventory, &mut action);
        footer.show(ui, |ui| bottom(ui, p, form, &mut action));
    });
    if !open {
        form.open = false;
    }
    action
}

fn top(ui: &mut egui::Ui, p: &Palette, form: &mut Form, inventory: &Inventory, action: &mut Option<Action>) {
    ui.spacing_mut().item_spacing.y = 8.0;
    let running = form.batch.running();
    ui.horizontal(|ui| {
        ui.label(RichText::new(aurora_i18n::tr!("script-import-destination")).color(p.muted));
        match &mut form.destination {
            Destination::Folder(folder) => {
                ui.add_enabled_ui(!running, |ui| form.picker.show(ui, p, inventory, folder));
            }
            Destination::Object { name, .. } => {
                ui.label(RichText::new(aurora_i18n::tr!("script-import-object", name = name.as_str())).color(p.ink));
            }
        }
    });
    ui.horizontal(|ui| {
        if widgets::action_button_if(
            ui,
            p,
            "plus",
            aurora_i18n::tr!("script-import-add-files"),
            widgets::ActionKind::Flat,
            Vec2::new(0.0, 28.0),
            !form.loading,
        )
        .clicked()
        {
            *action = Some(Action::AddFiles);
        }
        if widgets::action_button_if(
            ui,
            p,
            "trash-filled",
            aurora_i18n::tr!("script-import-clear"),
            widgets::ActionKind::Flat,
            Vec2::new(0.0, 28.0),
            !form.batch.rows.is_empty(),
        )
        .clicked()
        {
            *action = Some(Action::Clear);
        }
        if form.loading {
            ui.spinner();
        }
    });
    widgets::divider(ui, p);
    let object = matches!(form.destination, Destination::Object { .. });
    egui::ScrollArea::vertical()
        .id_salt("script_import_rows")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if form.batch.rows.is_empty() {
                ui.add(egui::Label::new(RichText::new(aurora_i18n::tr!("script-import-empty")).color(p.muted)).wrap());
            }
            let lsl = form.lsl;
            for row in &form.batch.rows {
                if let Some(open) = row_ui(ui, p, row, lsl, object) {
                    *action = Some(Action::Open(open));
                }
            }
        });
}

fn row_ui(ui: &mut egui::Ui, p: &Palette, row: &Row, lsl: Kind, object: bool) -> Option<Uuid> {
    let mut open = None;
    let (icon, color, text) = status(p, row, object);
    ui.push_id(&row.path, |ui| {
        ui.horizontal(|ui| {
            widgets::glyph(ui, icon, color, 16.0);
            let name = row.made_as.as_deref().unwrap_or(&row.name);
            ui.add(egui::Label::new(RichText::new(name).color(p.ink)).truncate())
                .on_hover_text(&row.file);
            if !matches!(row.state, State::Skipped(_)) {
                ui.label(RichText::new(kind_label(row.kind(lsl))).color(p.muted).small());
                ui.label(RichText::new(size_label(row.bytes)).color(p.muted).small());
            }
            if let Some(item) = row.item.filter(|_| row.state.saved() || matches!(row.state, State::Failed { kept: true, .. })) {
                let response = ui.small_button(aurora_i18n::tr!("script-import-open"));
                if response.on_hover_text(aurora_i18n::tr!("script-import-open-tip")).clicked() {
                    open = Some(item);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            ui.add(egui::Label::new(RichText::new(text).color(color).small()).wrap());
        });
        if let (State::CompileFailed(errors), Some(item)) = (&row.state, row.item) {
            for error in errors.iter().take(ERRORS_SHOWN) {
                let label = egui::Label::new(RichText::new(error).color(p.ink).small())
                    .wrap()
                    .sense(egui::Sense::click());
                let response = ui
                    .horizontal(|ui| {
                        ui.add_space(22.0);
                        ui.add(label)
                    })
                    .inner;
                if response.on_hover_text(aurora_i18n::tr!("script-import-open-tip")).clicked() {
                    open = Some(item);
                }
            }
        }
    });
    open
}

fn bottom(ui: &mut egui::Ui, p: &Palette, form: &mut Form, action: &mut Option<Action>) {
    ui.spacing_mut().item_spacing.y = 6.0;
    widgets::divider(ui, p);
    let running = form.batch.running();
    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(aurora_i18n::tr!("script-import-lsl-runs-on")).color(p.muted));
            widgets::radio_value(ui, p, &mut form.lsl, Kind::Mono, kind_label(Kind::Mono));
            widgets::radio_value(ui, p, &mut form.lsl, Kind::Lsl, kind_label(Kind::Lsl));
        });
        if matches!(form.destination, Destination::Object { .. }) {
            widgets::checkbox(ui, p, &mut form.run, aurora_i18n::tr!("script-import-run"));
        }
    });
    if !form.note.is_empty() {
        ui.add(egui::Label::new(RichText::new(form.note.as_str()).color(p.warn)).wrap());
    }
    let s = form.batch.summary();
    ui.add(
        egui::Label::new(
            RichText::new(aurora_i18n::tr!(
                "script-import-summary",
                saved = s.saved,
                errors = s.compile_errors,
                failed = s.failed,
                skipped = s.skipped
            ))
            .color(p.muted),
        )
        .wrap(),
    );
    let first = if running {
        widgets::DialogButton {
            icon: "x",
            label: aurora_i18n::tr!("script-import-stop"),
            kind: widgets::ActionKind::Flat,
            enabled: true,
        }
    } else {
        widgets::DialogButton {
            icon: "x",
            label: aurora_i18n::tr!("script-import-close"),
            kind: widgets::ActionKind::Flat,
            enabled: true,
        }
    };
    let buttons = [
        first,
        widgets::DialogButton {
            icon: "file-import",
            label: aurora_i18n::tr!("script-import-import"),
            kind: widgets::ActionKind::Primary,
            enabled: !running && form.batch.ready() > 0,
        },
    ];
    match widgets::dialog_buttons(ui, p, &buttons, 32.0) {
        Some(0) if running => *action = Some(Action::Stop),
        Some(0) => form.open = false,
        Some(_) => *action = Some(Action::Import),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scripts::import_rules::{Loaded, Skip};
    use std::path::PathBuf;

    fn form(object: bool) -> Form {
        let mut form = Form {
            open: true,
            ..Form::default()
        };
        for (name, result) in [
            ("a.lsl", Ok("x".to_owned())),
            ("b.luau", Ok("y".to_owned())),
            ("c.lsl", Err(Skip::NotUtf8)),
            ("d.lsl", Ok("z".to_owned())),
        ] {
            form.batch.add(Loaded {
                path: PathBuf::from(name),
                result,
            });
        }
        form.batch.start();
        form.batch.next();
        form.batch.rows[1].state = State::CompileFailed(vec!["(5, 9) : ERROR : Syntax error".into()]);
        form.batch.rows[1].item = Some(Uuid::from_u128(1));
        form.note = "note".into();
        if object {
            form.destination = Destination::Object {
                object: Uuid::from_u128(2),
                name: "Door".into(),
            };
        }
        form
    }

    #[test]
    fn the_window_keeps_its_size_for_both_destinations() {
        for object in [false, true] {
            widgets::steady_tests::assert_window_steady(
                WINDOW,
                &[egui::vec2(520.0, 460.0), egui::vec2(420.0, 360.0)],
                aurora_i18n::tr!("script-import-import"),
                || form(object),
                |form, ctx, p| {
                    window(ctx, p, form, &Inventory::default());
                },
            );
        }
    }
}
