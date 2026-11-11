//! The text preferences of `settings/text_tuning.rs` where the interface
//! reads them: the interface's line spacing in egui's style, the rest
//! kept in the context for the chat (`chat.rs`), the notecard editor
//! (`inventory/embedded.rs`) and the inventory's picture tooltips
//! (`inventory/hover.rs`).

use crate::settings::text_tuning::TextTuning;

fn key() -> egui::Id {
    egui::Id::unique("text_tuning")
}

/// Every frame: cheap when nothing changed.
pub fn apply(ctx: &egui::Context, tuning: &TextTuning) {
    let spacing = tuning.ui_line_spacing.max(0.0);
    if ctx.global_style().spacing.extra_text_line_spacing != spacing {
        ctx.all_styles_mut(|s| s.spacing.extra_text_line_spacing = spacing);
    }
    if ctx.data(|d| d.get_temp::<TextTuning>(key())).as_ref() != Some(tuning) {
        ctx.data_mut(|d| d.insert_temp(key(), tuning.clone()));
    }
}

/// The preferences last applied (the defaults before the first frame).
pub fn current(ctx: &egui::Context) -> TextTuning {
    ctx.data(|d| d.get_temp::<TextTuning>(key())).unwrap_or_default()
}

/// The chat's line spacing on a laid-out line: each section's row grows by
/// `extra` pixels (LLFontGL's FSFontChatLineSpacingPixels).
pub fn space_lines(ctx: &egui::Context, job: &mut egui::text::LayoutJob, extra: f32) {
    if extra <= 0.0 {
        return;
    }
    for section in &mut job.sections {
        let row = section
            .format
            .line_height
            .unwrap_or_else(|| ctx.fonts_mut(|f| f.row_height(&section.format.font_id)));
        section.format.line_height = Some(row + extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_lines_grow_by_the_spacing_only() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        let job = || {
            egui::text::LayoutJob::simple(
                "one\ntwo".into(),
                egui::FontId::proportional(14.0),
                egui::Color32::PLACEHOLDER,
                400.0,
            )
        };
        let plain = ctx.fonts_mut(|f| f.layout_job(job()));
        let mut spaced = job();
        space_lines(&ctx, &mut spaced, 0.0);
        assert_eq!(ctx.fonts_mut(|f| f.layout_job(spaced.clone())).size().y, plain.size().y);
        space_lines(&ctx, &mut spaced, 6.0);
        let spaced = ctx.fonts_mut(|f| f.layout_job(spaced));
        assert!(
            (spaced.size().y - (plain.size().y + 12.0)).abs() < 0.5,
            "{} vs {}",
            spaced.size().y,
            plain.size().y
        );
    }

    #[test]
    fn the_context_keeps_the_last_applied() {
        let ctx = egui::Context::default();
        assert_eq!(current(&ctx), TextTuning::default());
        let tuning = TextTuning {
            ui_line_spacing: 3.0,
            chat_line_spacing: 5.0,
            ..TextTuning::default()
        };
        apply(&ctx, &tuning);
        assert_eq!(current(&ctx), tuning);
        assert_eq!(ctx.global_style().spacing.extra_text_line_spacing, 3.0);
    }
}
