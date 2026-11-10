//! The camera's view history (Aurora's own): the views the user chose, to
//! step back and forth through like an undo list.
//!
//! A view is kept when a deliberate camera move ends: the user's own input
//! (a drag, the wheel, the camera keys, a focus click), once it has stopped
//! for `DEBOUNCE`. The camera following the avatar, a script or a seat
//! records nothing. The view the move started from is kept too, so the
//! first step back has somewhere to go. Session only, never saved.

use super::takeover::View;
use glam::Vec3;
use std::collections::VecDeque;

/// Views kept; the oldest goes first.
pub const CAPACITY: usize = 32;
/// The camera input must have stopped this long for the move to count as
/// finished, seconds.
pub const DEBOUNCE: f32 = 0.6;

/// The end of a camera move: the input started, then stopped for
/// `DEBOUNCE`. Shared with the Camera Path's recording.
#[derive(Debug, Clone, Copy, Default)]
pub struct Debounce {
    /// Seconds since the last camera input of a move in progress.
    quiet: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Started,
    Finished,
}

impl Debounce {
    /// Once a frame; `touched`: the user moved the camera this frame.
    pub fn observe(&mut self, touched: bool, dt: f32) -> Option<Move> {
        if touched {
            let started = self.quiet.is_none();
            self.quiet = Some(0.0);
            return started.then_some(Move::Started);
        }
        let quiet = self.quiet.as_mut()?;
        *quiet += dt.max(0.0);
        if *quiet < DEBOUNCE {
            return None;
        }
        self.quiet = None;
        Some(Move::Finished)
    }

    pub fn cancel(&mut self) {
        self.quiet = None;
    }
}

#[derive(Debug, Clone, Default)]
pub struct History {
    views: VecDeque<View>,
    /// The view shown last through the history, or the latest recorded.
    cursor: usize,
    moving: Debounce,
}

impl History {
    pub fn len(&self) -> usize {
        self.views.len()
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    pub fn can_go_back(&self, current: &View) -> bool {
        self.cursor > 0 || self.off_cursor(current)
    }

    pub fn can_go_forward(&self) -> bool {
        self.cursor + 1 < self.views.len()
    }

    fn off_cursor(&self, current: &View) -> bool {
        self.views.get(self.cursor).is_some_and(|v| !v.close_to(current))
    }

    pub fn clear(&mut self) {
        *self = History::default();
    }

    /// The region positions are relative to changed (`Camera::shift`).
    pub fn shift(&mut self, delta: Vec3) {
        for v in &mut self.views {
            *v = v.shifted(delta);
        }
    }

    /// Keep `view` after the cursor: what was ahead of it (views stepped
    /// back from) is dropped, as an undo list does. A view the same as the
    /// cursor's is not kept twice. True when it was kept.
    pub fn record(&mut self, view: View) -> bool {
        if !view.is_finite() || self.views.get(self.cursor).is_some_and(|v| v.close_to(&view)) {
            return false;
        }
        self.views.truncate(self.cursor + 1);
        self.views.push_back(view);
        while self.views.len() > CAPACITY {
            self.views.pop_front();
        }
        self.cursor = self.views.len() - 1;
        true
    }

    /// Once a frame, before the camera's update. `touched`: the user moved
    /// the camera since the last frame; `view`: the view shown last (the
    /// input's effect is not in it yet). True when a view was recorded.
    pub fn observe(&mut self, touched: bool, view: View, dt: f32) -> bool {
        match self.moving.observe(touched, dt) {
            Some(Move::Started) => {
                // where the move starts from
                self.record(view);
                false
            }
            Some(Move::Finished) => self.record(view),
            None => false,
        }
    }

    /// A move in progress is dropped (the camera was taken over, or a step
    /// of the history itself moved it).
    pub fn cancel_pending(&mut self) {
        self.moving.cancel();
    }

    /// The view before the cursor's. `current`: the view shown now; when it
    /// is not the cursor's (a move not recorded yet), it is kept first so
    /// that `next` comes back to it.
    pub fn previous(&mut self, current: View) -> Option<View> {
        self.moving.cancel();
        if self.off_cursor(&current) {
            self.record(current);
        }
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.views.get(self.cursor).copied()
    }

    pub fn next(&mut self) -> Option<View> {
        self.moving.cancel();
        if !self.can_go_forward() {
            return None;
        }
        self.cursor += 1;
        self.views.get(self.cursor).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(x: f32) -> View {
        View {
            position: Vec3::new(x, 0.0, 20.0),
            target: Vec3::new(x + 5.0, 0.0, 20.0),
            fov: 1.0,
            roll: 0.0,
        }
    }

    const FRAME: f32 = 1.0 / 60.0;

    /// A drag from `from` to `to` over ten frames, then stillness for
    /// `rest` seconds.
    fn drag(h: &mut History, from: f32, to: f32, rest: f32) -> usize {
        let mut recorded = 0;
        let mut last = from;
        for n in 1..=10 {
            let x = from + (to - from) * n as f32 / 10.0;
            recorded += usize::from(h.observe(true, view(last), FRAME));
            last = x;
        }
        let mut waited = 0.0;
        while waited < rest {
            recorded += usize::from(h.observe(false, view(to), FRAME));
            waited += FRAME;
        }
        recorded
    }

    #[test]
    fn a_move_is_recorded_once_when_it_stops_with_where_it_started() {
        let mut h = History::default();
        assert_eq!(drag(&mut h, 0.0, 10.0, 1.0), 1, "one view per move, not per frame");
        assert_eq!(h.len(), 2, "the start and the end");
        assert_eq!(h.previous(view(10.0)), Some(view(0.0)));
        assert_eq!(h.next(), Some(view(10.0)));
        assert_eq!(h.next(), None);
    }

    #[test]
    fn nothing_is_recorded_before_the_debounce_or_without_input() {
        let mut h = History::default();
        // the camera follows the avatar: it moves, the user did not move it
        for n in 0..600 {
            assert!(!h.observe(false, view(n as f32), FRAME));
        }
        assert!(h.is_empty());
        assert_eq!(drag(&mut h, 0.0, 10.0, DEBOUNCE * 0.5), 0, "still inside the debounce");
        // moving again restarts the wait, and stays one move
        assert_eq!(drag(&mut h, 10.0, 20.0, 1.0), 1);
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn the_ring_keeps_the_last_thirty_two() {
        let mut h = History::default();
        for n in 0..50 {
            h.record(view(n as f32));
        }
        assert_eq!(h.len(), CAPACITY);
        let mut back = 0;
        let mut current = view(49.0);
        while let Some(v) = h.previous(current) {
            current = v;
            back += 1;
        }
        assert_eq!(back, CAPACITY - 1);
        assert_eq!(current, view(18.0));
    }

    #[test]
    fn a_new_view_after_stepping_back_drops_what_was_ahead() {
        let mut h = History::default();
        for n in 0..4 {
            h.record(view(n as f32));
        }
        assert_eq!(h.previous(view(3.0)), Some(view(2.0)));
        assert_eq!(h.previous(view(2.0)), Some(view(1.0)));
        assert!(h.can_go_forward());
        h.record(view(9.0));
        assert!(!h.can_go_forward());
        assert_eq!(h.len(), 3);
        assert_eq!(h.previous(view(9.0)), Some(view(1.0)));
    }

    #[test]
    fn stepping_back_from_an_unrecorded_view_keeps_it_for_next() {
        let mut h = History::default();
        h.record(view(0.0));
        h.record(view(1.0));
        assert!(h.can_go_back(&view(5.0)));
        assert_eq!(h.previous(view(5.0)), Some(view(1.0)));
        assert_eq!(h.next(), Some(view(5.0)));
    }

    #[test]
    fn the_same_view_is_not_kept_twice_and_clear_empties() {
        let mut h = History::default();
        h.record(view(1.0));
        h.record(view(1.0));
        assert_eq!(h.len(), 1);
        assert!(!h.can_go_back(&view(1.0)));
        assert_eq!(h.previous(view(1.0)), None);
        h.clear();
        assert!(h.is_empty() && !h.can_go_forward());
    }

    #[test]
    fn a_border_crossing_moves_the_kept_views() {
        let mut h = History::default();
        h.record(view(1.0));
        h.record(view(300.0));
        h.shift(Vec3::new(-256.0, 0.0, 0.0));
        assert_eq!(h.previous(view(44.0)), Some(view(-255.0)));
    }
}
