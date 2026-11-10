//! Who shows the view when it is not the normal camera: the one hook the
//! Camera Lock and the Camera Path share (Aurora's own; the Legacy Viewer
//! has neither).
//!
//! `Camera::update` works the normal camera out as always (behind the
//! avatar, on a focus point, a seat, a script, the free camera…), then
//! hands its view to `Takeover::apply`, which returns the view to show: the
//! normal one, the view an owner holds, or the short glide back from a held
//! view to the normal one. The normal camera keeps running underneath, so
//! giving the view back never needs to rebuild it.
//!
//! RLVa camera restrictions come after this in `Camera::update` and release
//! any owner (`Camera::update`): they always win.

use glam::{Quat, Vec3};

/// The glide from a released view back to the normal camera, seconds.
pub const RELEASE_TIME: f32 = 0.5;

/// One view of the world, in render space: what a lock freezes, what the
/// view history keeps and what a path plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub position: Vec3,
    /// The point looked at.
    pub target: Vec3,
    /// Vertical field of view, radians.
    pub fov: f32,
    /// Roll around the view's direction, radians (`Camera::roll`).
    pub roll: f32,
}

impl View {
    pub fn forward(&self) -> Vec3 {
        (self.target - self.position).normalize_or(Vec3::X)
    }

    fn distance(&self) -> f32 {
        self.position.distance(self.target)
    }

    pub fn is_finite(&self) -> bool {
        self.position.is_finite() && self.target.is_finite() && self.fov.is_finite() && self.roll.is_finite()
    }

    pub fn shifted(mut self, delta: Vec3) -> View {
        self.position += delta;
        self.target += delta;
        self
    }

    /// The same view for practical purposes: a step of the history that
    /// would show nothing.
    pub fn close_to(&self, other: &View) -> bool {
        self.position.distance(other.position) < 0.02
            && self.forward().dot(other.forward()) > 0.99999
            && (self.fov - other.fov).abs() < 0.002
            && (self.roll - other.roll).abs() < 0.002
    }

    /// From `self` to `to` at `k` (0 to 1): the eye in a straight line, the
    /// direction turned on the shortest arc, the lens and the roll evenly.
    pub fn blend(&self, to: &View, k: f32) -> View {
        let k = k.clamp(0.0, 1.0);
        let position = self.position.lerp(to.position, k);
        let turn = Quat::from_rotation_arc(self.forward(), to.forward());
        let direction = Quat::IDENTITY.slerp(turn, k) * self.forward();
        let distance = self.distance() + (to.distance() - self.distance()) * k;
        View {
            position,
            target: position + direction * distance.max(0.01),
            fov: self.fov + (to.fov - self.fov) * k,
            roll: self.roll + shortest_angle(to.roll - self.roll) * k,
        }
    }
}

fn shortest_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a + PI).rem_euclid(TAU) - PI
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// What holds the view instead of the normal camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The Camera Lock: the view frozen where it was.
    Lock,
    /// A Camera Path being played, or holding its last shot.
    Path,
}

#[derive(Debug, Clone, Copy)]
struct Release {
    from: View,
    elapsed: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Takeover {
    held: Option<(Owner, View)>,
    release: Option<Release>,
    /// `Camera::roll` when the view was taken: the roll the normal camera
    /// goes back to.
    base_roll: f32,
}

impl Takeover {
    pub fn owner(&self) -> Option<Owner> {
        self.held.map(|(owner, _)| owner)
    }

    pub fn locked(&self) -> bool {
        self.owner() == Some(Owner::Lock)
    }

    /// Nothing held and no glide running: the normal camera shows.
    pub fn idle(&self) -> bool {
        self.held.is_none() && self.release.is_none()
    }

    /// `owner` holds `view` from now on. The lock is never taken from by a
    /// path (false: refused); the lock takes the view from a path.
    /// `normal_roll`: the normal camera's roll now.
    pub fn take(&mut self, owner: Owner, view: View, normal_roll: f32) -> bool {
        if !view.is_finite() || (self.locked() && owner != Owner::Lock) {
            return false;
        }
        if self.idle() {
            self.base_roll = normal_roll;
        }
        self.held = Some((owner, view));
        self.release = None;
        true
    }

    /// The view `owner` holds moves (a path's next frame). False: `owner`
    /// no longer holds the view.
    pub fn hold(&mut self, owner: Owner, view: View) -> bool {
        match &mut self.held {
            Some((o, held)) if *o == owner && view.is_finite() => {
                *held = view;
                true
            }
            _ => false,
        }
    }

    /// The normal camera's roll, kept while a view is held or glides back.
    pub fn base_roll(&self) -> f32 {
        self.base_roll
    }

    pub fn held_view(&self) -> Option<View> {
        self.held.map(|(_, view)| view)
    }

    /// Give the view back: gliding from the held view to the normal camera
    /// over `RELEASE_TIME` when `blend`, at once otherwise.
    pub fn release(&mut self, blend: bool) {
        if let Some((_, view)) = self.held.take() {
            self.release = blend.then_some(Release { from: view, elapsed: 0.0 });
        } else if !blend {
            self.release = None;
        }
    }

    /// The normal camera just jumped (a step of the view history, a
    /// waypoint asked for): show the way there from `from` as a glide.
    /// `normal_roll`: the roll it jumped to.
    pub fn ease_from(&mut self, from: View, normal_roll: f32) {
        if self.held.is_none() && from.is_finite() {
            self.base_roll = normal_roll;
            self.release = Some(Release { from, elapsed: 0.0 });
        }
    }

    /// The region positions are relative to changed (`Camera::shift`).
    pub fn shift(&mut self, delta: Vec3) {
        if let Some((_, view)) = &mut self.held {
            *view = view.shifted(delta);
        }
        if let Some(release) = &mut self.release {
            release.from = release.from.shifted(delta);
        }
    }

    /// The view to show this frame, given the normal camera's. The roll of
    /// the returned view is the one to draw with; `normal.roll` is ignored
    /// while a view is held or glides back (the roll kept at `take`).
    pub fn apply(&mut self, normal: View, dt: f32) -> View {
        if let Some((_, view)) = self.held {
            return view;
        }
        let Some(mut release) = self.release else {
            return normal;
        };
        let normal = View {
            roll: self.base_roll,
            ..normal
        };
        release.elapsed += dt.max(0.0);
        if release.elapsed >= RELEASE_TIME || !normal.is_finite() {
            self.release = None;
            return normal;
        }
        self.release = Some(release);
        release.from.blend(&normal, smoothstep(release.elapsed / RELEASE_TIME))
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

    #[test]
    fn a_held_view_shows_whatever_the_normal_camera_does() {
        let mut t = Takeover::default();
        assert!(t.idle());
        assert_eq!(t.apply(view(3.0), 0.016), view(3.0));
        assert!(t.take(Owner::Lock, view(1.0), 0.0));
        assert!(t.locked() && !t.idle());
        for x in [2.0, 50.0, -400.0] {
            assert_eq!(t.apply(view(x), 0.016), view(1.0));
        }
    }

    #[test]
    fn a_release_glides_back_in_half_a_second_without_a_jump() {
        let mut t = Takeover::default();
        t.take(Owner::Lock, view(0.0), 0.0);
        t.release(true);
        assert_eq!(t.owner(), None);
        assert!(!t.idle());
        let mut last = 0.0;
        let mut longest = 0.0f32;
        for _ in 0..40 {
            let shown = t.apply(view(10.0), 1.0 / 60.0);
            assert!(shown.position.x >= last, "never backwards");
            longest = longest.max(shown.position.x - last);
            last = shown.position.x;
        }
        assert!(t.idle(), "over after {RELEASE_TIME} s");
        assert_eq!(last, 10.0);
        assert!(longest < 0.6, "eased, no jump: {longest}");
    }

    #[test]
    fn a_release_without_blend_is_immediate() {
        let mut t = Takeover::default();
        t.take(Owner::Path, view(0.0), 0.0);
        t.release(false);
        assert!(t.idle());
        assert_eq!(t.apply(view(10.0), 0.016), view(10.0));
    }

    #[test]
    fn the_lock_takes_the_view_from_a_path_and_not_the_reverse() {
        let mut t = Takeover::default();
        assert!(t.take(Owner::Path, view(1.0), 0.0));
        assert!(t.hold(Owner::Path, view(2.0)));
        assert!(t.take(Owner::Lock, view(2.0), 0.0));
        assert!(!t.hold(Owner::Path, view(3.0)), "the path lost the view");
        assert!(!t.take(Owner::Path, view(3.0), 0.0), "a locked camera refuses a path");
        assert_eq!(t.held_view(), Some(view(2.0)));
    }

    #[test]
    fn the_normal_roll_comes_back_after_a_held_roll() {
        let mut t = Takeover::default();
        let rolled = View { roll: 0.5, ..view(0.0) };
        t.take(Owner::Path, rolled, 0.1);
        t.release(true);
        let mut shown = rolled;
        for _ in 0..40 {
            // the camera draws with the shown roll: it must not feed back
            shown = t.apply(
                View {
                    roll: shown.roll,
                    ..view(0.0)
                },
                1.0 / 60.0,
            );
        }
        assert!((shown.roll - 0.1).abs() < 1e-6, "{}", shown.roll);
    }

    #[test]
    fn a_border_crossing_moves_the_held_view_and_the_glide() {
        let mut t = Takeover::default();
        t.take(Owner::Lock, view(1.0), 0.0);
        t.shift(Vec3::new(-256.0, 0.0, 0.0));
        assert_eq!(t.held_view(), Some(view(-255.0)));
        t.release(true);
        t.shift(Vec3::new(256.0, 0.0, 0.0));
        let shown = t.apply(view(1.0), 0.0);
        assert!((shown.position.x - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_broken_view_is_refused() {
        let mut t = Takeover::default();
        let broken = View {
            position: Vec3::splat(f32::NAN),
            ..view(0.0)
        };
        assert!(!t.take(Owner::Lock, broken, 0.0));
        assert!(t.idle());
    }

    #[test]
    fn a_blend_turns_on_the_shortest_arc_and_keeps_a_unit_direction() {
        let a = view(0.0);
        let b = View {
            target: a.position + Vec3::Y * 5.0,
            ..a
        };
        let mid = a.blend(&b, 0.5);
        let f = mid.forward();
        assert!((f.x - f.y).abs() < 1e-4 && f.z.abs() < 1e-4, "{f:?}");
        assert!(a.blend(&b, 0.0).close_to(&a) && a.blend(&b, 1.0).close_to(&b));
    }
}
