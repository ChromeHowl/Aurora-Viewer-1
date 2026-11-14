//! Advanced › Set Window Size › Custom size…: the width and height window
//! (`ui/window_size.rs`); Apply asks the size like a menu entry does
//! (LLFloaterWindowSize::onClickSet, indra/newview/llfloaterwindowsize.cpp,
//! originally LGPL 2.1).
use super::*;

/// The size `AURORA_DEMO_WINDOW_SIZE=<width>x<height>` asks for.
fn demo_size() -> Option<(u32, u32)> {
    static SIZE: std::sync::OnceLock<Option<(u32, u32)>> = std::sync::OnceLock::new();
    *SIZE.get_or_init(|| {
        let v = std::env::var("AURORA_DEMO_WINDOW_SIZE").ok()?;
        let (w, h) = v.split_once('x')?;
        Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
    })
}

impl App {
    /// The Custom size window: Apply asks the size like a menu entry does.
    pub(super) fn window_size_window(&mut self, ctx: &egui::Context, p: &Palette) {
        let monitor = self
            .gfx
            .as_ref()
            .and_then(|g| g.window.current_monitor())
            .map(|m| (m.size().width, m.size().height));
        if let Some(size) = self.window_size_ui.show(ctx, p, monitor) {
            self.pending_window_size = Some(size);
        }
    }

    /// `AURORA_DEMO_WINDOW_SIZE=<width>x<height>`: asks the size at frame
    /// 400, as the menu entry does.
    pub(super) fn window_size_demo_steps(&mut self) {
        if !self.demo || self.frame_count != 400 {
            return;
        }
        if let Some(size) = demo_size() {
            log::info!("demo window size: {} x {}", size.0, size.1);
            self.pending_window_size = Some(size);
        }
    }
}
