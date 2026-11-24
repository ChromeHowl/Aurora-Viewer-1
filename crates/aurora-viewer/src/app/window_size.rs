//! Advanced › Set Window Size › Custom size…: the width and height window
//! (`ui/window_size.rs`); Apply asks the size like a menu entry does
//! (LLFloaterWindowSize::onClickSet, indra/newview/llfloaterwindowsize.cpp,
//! originally LGPL 2.1).
use super::*;

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
}
