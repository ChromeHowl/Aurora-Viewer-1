//! Advanced › Set Window Size › Custom size…: the window.
//! floater_window_size.xml, in pixels. Port of
//! LLFloaterWindowSize::onClickSet and LLViewerWindow::movieSize
//! (indra/newview/llfloaterwindowsize.cpp, llviewerwindow.cpp, originally
//! LGPL 2.1). The window keeps the size at the next start
//! (`record_window_geometry`, as WindowWidth and WindowHeight).
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

