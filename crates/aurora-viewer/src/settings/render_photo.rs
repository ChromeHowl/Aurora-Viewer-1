//! Rendering and photo extras of Preferences › Graphics: the view-only
//! transparency and fullbright switches (session only, the user's own
//! objects untouched), transparent water (RenderTransparentWater), depth
//! of field on blended faces (LLDrawPoolAlpha's depth pass for the depth
//! of field), the animesh impostor exclusion (FSImpostorAvatarExclude),
//! the pre-caching delay at login (PrecachingDelay) and the auto-tune to a
//! target frame rate (TargetFPS, AutoTuneFPS and the AutoTune* settings,
//! LLPerfStats::StatsRecorder::updateAvatarParams in llperfstats.cpp,
//! originally LGPL 2.1). Defaults leave the picture and the behaviour as
//! they were.

use serde::{Deserialize, Serialize};

/// PrecachingDelay's slider (panel_preferences_graphics1.xml): 1 to 15 s;
/// Aurora adds 0, its own quicker screen.
pub const PRECACHING_RANGE: std::ops::RangeInclusive<f32> = 0.0..=15.0;
/// TargetFPS's choices in the Legacy Viewer's performance window.
pub const TARGET_FPS_RANGE: std::ops::RangeInclusive<u32> = 1..=200;
/// LLPerfStats::DD_STEP: metres per step of the draw distance.
pub const DRAW_DISTANCE_STEP: f32 = 10.0;
/// LLPerfStats::ART_MINIMUM_NANOS, ART_MIN_ADJUST_DOWN_NANOS and
/// ART_MIN_ADJUST_UP_NANOS, in microseconds.
pub const ART_MINIMUM_US: f32 = 100.0;
pub const ART_ADJUST_DOWN_US: f32 = 10.0;
pub const ART_ADJUST_UP_US: f32 = 5.0;

/// View-only switches of the session (the Develop › Rendering switches of
/// render object transparency and fullbright): off, the faces of what the
/// user does not own are drawn opaque, or lit instead of fullbright.
/// Never saved: every session starts with both on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewFaces {
    pub transparency: bool,
    pub fullbright: bool,
}

impl Default for ViewFaces {
    fn default() -> Self {
        ViewFaces {
            transparency: true,
            fullbright: true,
        }
    }
}

impl ViewFaces {
    /// Whether the switches change anything for an object: `own` the user
    /// owns it (LLViewerObject::permYouOwner) or wears it.
    pub fn applies(&self, own: bool) -> bool {
        !own && (!self.transparency || !self.fullbright)
    }

    /// A face's colour alpha seen with the switches: opaque when
    /// transparency is off, except a fully transparent face (alpha under
    /// 1/255 of the shader's cut), which stays hidden, as it is not meant to
    /// be seen (invisible prims, unused layers of mesh bodies).
    pub fn face_alpha(&self, own: bool, alpha: f32) -> f32 {
        if own || self.transparency || alpha < 0.004 { alpha } else { 1.0 }
    }
}

/// TuningFPSStrategy: what the auto-tune may lower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Strategy {
    /// 0: the avatars only.
    Avatars,
    /// 1: the avatars and the scene (the default).
    Both,
    /// 2: the scene only.
    Scene,
}

/// The auto-tune's settings (the Legacy Viewer's performance window).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoTune {
    /// AutoTuneFPS (off).
    pub enabled: bool,
    /// TargetFPS (15).
    pub target_fps: u32,
    /// AutoTuneLock: keep tuning once the target is reached (off).
    pub lock: bool,
    /// KeepAutoTuneLock: the lock stays for the next sessions (on).
    pub keep_lock: bool,
    /// TuningFPSStrategy (both).
    pub strategy: Strategy,
    /// AutoTuneRenderFarClipMin (32 m): the draw distance never goes below.
    pub draw_distance_min: f32,
    /// AutoTuneRenderFarClipTarget (256 m): the draw distance it climbs
    /// back to.
    pub draw_distance_target: f32,
    /// AutoTuneImpostorByDistEnabled (off): the fully drawn avatars are
    /// those within `impostor_distance`.
    pub impostor_by_distance: bool,
    /// AutoTuneImpostorFarAwayDistance (64 m).
    pub impostor_distance: f32,
}

impl Default for AutoTune {
    fn default() -> Self {
        AutoTune {
            enabled: false,
            target_fps: 15,
            lock: false,
            keep_lock: true,
            strategy: Strategy::Both,
            draw_distance_min: 32.0,
            draw_distance_target: 256.0,
            impostor_by_distance: false,
            impostor_distance: 64.0,
        }
    }
}

impl AutoTune {
    /// Tunables::initialiseFromSettings: the lock only lasts past the
    /// session with KeepAutoTuneLock, and a kept lock turns the tuning on.
    pub fn at_login(&mut self) {
        if self.lock && !self.keep_lock {
            self.lock = false;
        }
        if self.lock {
            self.enabled = true;
        }
    }
}

/// The settings of this module (`Settings::render_photo`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderPhoto {
    /// RenderTransparentWater (on): off, the water is opaque, its deep
    /// fog in place of what lies under it (waterF.glsl without
    /// TRANSPARENT_WATER).
    pub transparent_water: bool,
    /// The depth of field focuses on blended faces opaque enough, as the
    /// Legacy Viewer's alpha pass writes their depth for it. Off by default:
    /// Aurora's picture until now, blended faces blurred like what lies
    /// behind them.
    pub dof_alphas: bool,
    /// FSImpostorAvatarExclude "User": avatars wearing animesh are never
    /// impostors, silhouettes nor tuned (off). Rezzed animesh, "Control",
    /// are never impostors in Aurora.
    pub impostor_exclude_animesh: bool,
    /// PrecachingDelay: seconds the login's loading screen stays at least
    /// after the arrival; 0 (Aurora's default) leaves as soon as the ground
    /// is there.
    pub precaching_delay: f32,
    pub autotune: AutoTune,
    /// Session only (`ViewFaces`).
    #[serde(skip)]
    pub view: ViewFaces,
}

impl Default for RenderPhoto {
    fn default() -> Self {
        RenderPhoto {
            transparent_water: true,
            dof_alphas: false,
            impostor_exclude_animesh: false,
            precaching_delay: 0.0,
            autotune: AutoTune::default(),
            view: ViewFaces::default(),
        }
    }
}

fn finite_in(v: f32, range: std::ops::RangeInclusive<f32>, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(*range.start(), *range.end())
    } else {
        default
    }
}

impl RenderPhoto {
    /// The bounds of a settings file.
    pub fn sanitize(&mut self) {
        self.precaching_delay = finite_in(self.precaching_delay, PRECACHING_RANGE, 0.0);
        let a = &mut self.autotune;
        a.target_fps = a.target_fps.clamp(*TARGET_FPS_RANGE.start(), *TARGET_FPS_RANGE.end());
        a.draw_distance_min = finite_in(a.draw_distance_min, 32.0..=512.0, 32.0);
        a.draw_distance_target = finite_in(a.draw_distance_target, a.draw_distance_min..=1024.0, 256.0);
        a.impostor_distance = finite_in(a.impostor_distance, 16.0..=256.0, 64.0);
    }
}

/// The login's loading screen gives way to the world (with
/// `teleport_comfort::loading_screen_ends`'s other rules): not before
/// `delay` seconds after the arrival (LLStartUp STATE_PRECACHE).
pub fn precached(delay: f32, since_arrival: f32) -> bool {
    since_arrival >= delay
}

/// What the auto-tune reads each frame.
#[derive(Debug, Clone, Copy)]
pub struct TuneInput {
    /// Mean frame time (ms).
    pub frame_ms: f32,
    /// The window has the focus (no tuning in the background).
    pub focused: bool,
    /// The avatars' estimated render time this frame, in total and the
    /// largest one (µs).
    pub avatars_us: f32,
    pub max_avatar_us: f32,
    /// Avatars over the render time limit.
    pub tuned_avatars: usize,
    /// Current draw distance (m).
    pub draw_distance: f32,
}

/// What the auto-tune changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TuneAction {
    DrawDistance(f32),
    /// RenderAvatarMaxART (µs; 0 none).
    AvatarLimit(f32),
    /// The target is reached and the tuning is not locked: it turns off.
    Done,
}

/// The auto-tune's state of the session (LLPerfStats statics).
#[derive(Debug, Clone, Default)]
pub struct Tuner {
    /// RenderAvatarMaxART (µs; 0 no limit).
    pub max_art_us: f32,
    below_target: bool,
    frame: u64,
    last_change: u64,
    /// Frame the window last lost the focus (`lastSleepedFrame`).
    slept: Option<u64>,
}

impl Tuner {
    /// StatsRecorder::updateAvatarParams, once per frame while the tuning
    /// is on. Avatars are tuned by their estimated render time (Aurora has
    /// no per-avatar GPU timer: a share of the GPU frame time by their
    /// triangles, `scene::render_time`).
    pub fn step(&mut self, cfg: &AutoTune, i: &TuneInput) -> Option<TuneAction> {
        self.frame += 1;
        if !i.focused {
            self.slept = Some(self.frame);
            return None;
        }
        let target_fps = cfg.target_fps.max(1);
        if let Some(s) = self.slept {
            // wait a short time after the window has the focus again
            if self.frame - s > u64::from(target_fps) * 5 {
                self.slept = None;
            } else {
                return None;
            }
        }
        let target_ms = 1000.0 / target_fps as f32;
        let frame_ms = i.frame_ms.max(0.001);
        let inferred_fps = (1000.0 / frame_ms.max(1.0)) as u64;
        let change_every = inferred_fps.max(50);
        let buffer = target_ms * 0.1;
        let since_change = self.frame - self.last_change;
        if target_ms + buffer <= frame_ms {
            if !self.below_target {
                // the first frame under: hold fire, a little hysteresis
                self.below_target = true;
                self.last_change = self.frame;
                return None;
            }
            let non_avatar_ms = (frame_ms - i.avatars_us / 1000.0).max(0.0);
            let target_avatar_ms = if target_ms < non_avatar_ms {
                // the avatars alone cannot do it
                if since_change <= change_every {
                    return None;
                }
                if cfg.strategy != Strategy::Avatars {
                    let dd = (i.draw_distance - DRAW_DISTANCE_STEP).max(cfg.draw_distance_min);
                    if dd < i.draw_distance {
                        self.last_change = self.frame;
                        return Some(TuneAction::DrawDistance(dd));
                    }
                }
                0.0
            } else {
                target_ms - non_avatar_ms
            };
            if target_avatar_ms * 1000.0 < i.avatars_us && cfg.strategy != Strategy::Scene {
                let mut limit = i.max_avatar_us;
                if self.max_art_us > 0.0 && limit > self.max_art_us {
                    limit = self.max_art_us;
                }
                if limit > ART_ADJUST_DOWN_US {
                    limit -= ART_ADJUST_DOWN_US;
                }
                limit = limit.max(ART_MINIMUM_US);
                if limit != self.max_art_us {
                    self.max_art_us = limit;
                    return Some(TuneAction::AvatarLimit(limit));
                }
            }
            None
        } else if target_ms > frame_ms + self.max_art_us / 1000.0 {
            if self.below_target {
                // the target is reached: a pause
                self.last_change = self.frame;
                self.below_target = false;
            }
            if since_change <= change_every * 3 {
                return None;
            }
            if !cfg.lock {
                return Some(TuneAction::Done);
            }
            if self.max_art_us > 0.0 && i.tuned_avatars > 0 && cfg.strategy != Strategy::Scene {
                let up = if i.tuned_avatars > 2 {
                    ART_ADJUST_UP_US
                } else {
                    ART_ADJUST_UP_US * 2.0
                };
                self.max_art_us += up;
                return Some(TuneAction::AvatarLimit(self.max_art_us));
            }
            if cfg.strategy != Strategy::Avatars && i.draw_distance < cfg.draw_distance_target {
                self.last_change = self.frame;
                return Some(TuneAction::DrawDistance(
                    (i.draw_distance + DRAW_DISTANCE_STEP).min(cfg.draw_distance_target),
                ));
            }
            None
        } else {
            None
        }
    }
}

/// An avatar over the render time limit is drawn as an impostor
/// (LLVOAvatar::updateTooSlow / isTooSlowWithoutShadows): never ours, one
/// always drawn in full by the user's exception or as a friend, nor one
/// the animesh exclusion keeps.
pub fn too_slow(limit_us: f32, avatar_us: f32, own: bool, always_full: bool, excluded: bool) -> bool {
    limit_us > 0.0 && avatar_us >= limit_us && !own && !always_full && !excluded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_picture_and_old_files_load() {
        let d: RenderPhoto = serde_json::from_str("{}").expect("an empty object");
        assert_eq!(d, RenderPhoto::default());
        assert!(d.transparent_water && !d.dof_alphas && !d.impostor_exclude_animesh);
        assert_eq!(d.precaching_delay, 0.0);
        assert_eq!(d.autotune.target_fps, 15);
        assert!(!d.autotune.enabled && d.autotune.keep_lock && d.autotune.strategy == Strategy::Both);
        let mut s = d.clone();
        s.view.transparency = false;
        assert!(!serde_json::to_string(&s).expect("saved").contains("view"));
    }

    #[test]
    fn a_settings_file_is_bounded() {
        let mut s = RenderPhoto {
            precaching_delay: f32::NAN,
            ..Default::default()
        };
        s.autotune.target_fps = 0;
        s.autotune.draw_distance_min = 1.0;
        s.autotune.draw_distance_target = 8.0;
        s.autotune.impostor_distance = f32::INFINITY;
        s.sanitize();
        assert_eq!(s.precaching_delay, 0.0);
        assert_eq!(s.autotune.target_fps, 1);
        assert_eq!((s.autotune.draw_distance_min, s.autotune.draw_distance_target), (32.0, 32.0));
        assert_eq!(s.autotune.impostor_distance, 64.0);
        s.precaching_delay = 40.0;
        s.sanitize();
        assert_eq!(s.precaching_delay, 15.0);
    }

    #[test]
    fn the_view_switches_leave_own_objects_and_hidden_faces() {
        let on = ViewFaces::default();
        assert!(!on.applies(false));
        let off = ViewFaces {
            transparency: false,
            fullbright: true,
        };
        assert!(off.applies(false) && !off.applies(true));
        assert_eq!(off.face_alpha(false, 0.5), 1.0);
        assert_eq!(off.face_alpha(true, 0.5), 0.5, "own objects untouched");
        assert_eq!(off.face_alpha(false, 0.0), 0.0, "fully transparent stays hidden");
        assert_eq!(on.face_alpha(false, 0.5), 0.5);
        let no_fb = ViewFaces {
            transparency: true,
            fullbright: false,
        };
        assert!(no_fb.applies(false));
        assert_eq!(no_fb.face_alpha(false, 0.5), 0.5);
    }

    #[test]
    fn the_lock_lasts_only_when_kept() {
        let mut a = AutoTune {
            lock: true,
            keep_lock: false,
            ..Default::default()
        };
        a.at_login();
        assert!(!a.lock && !a.enabled);
        let mut a = AutoTune {
            lock: true,
            ..Default::default()
        };
        a.at_login();
        assert!(a.lock && a.enabled);
    }

    #[test]
    fn the_precaching_delay_holds_the_screen() {
        assert!(precached(0.0, 0.0));
        assert!(!precached(6.0, 5.9));
        assert!(precached(6.0, 6.0));
    }

    fn input(frame_ms: f32, avatars_us: f32, max_us: f32, dd: f32) -> TuneInput {
        TuneInput {
            frame_ms,
            focused: true,
            avatars_us,
            max_avatar_us: max_us,
            tuned_avatars: 0,
            draw_distance: dd,
        }
    }

    #[test]
    fn slow_avatars_lower_the_avatar_limit_first() {
        let cfg = AutoTune {
            enabled: true,
            target_fps: 50,
            ..Default::default()
        };
        let mut t = Tuner::default();
        // 40 ms a frame, 30 of them avatars: the avatars can do it
        let i = input(40.0, 30_000.0, 8_000.0, 128.0);
        assert_eq!(t.step(&cfg, &i), None, "hysteresis on the first frame under");
        assert_eq!(t.step(&cfg, &i), Some(TuneAction::AvatarLimit(7_990.0)));
        assert_eq!(t.step(&cfg, &i), Some(TuneAction::AvatarLimit(7_980.0)), "keeps heading down");
        assert!(too_slow(7_980.0, 8_000.0, false, false, false));
        assert!(!too_slow(7_980.0, 8_000.0, true, false, false), "never ours");
        assert!(!too_slow(7_980.0, 8_000.0, false, true, false));
        assert!(!too_slow(7_980.0, 8_000.0, false, false, true));
        assert!(!too_slow(0.0, 8_000.0, false, false, false), "no limit");
    }

    #[test]
    fn a_heavy_scene_steps_the_draw_distance_down_to_its_minimum() {
        let cfg = AutoTune {
            enabled: true,
            target_fps: 60,
            ..Default::default()
        };
        let mut t = Tuner::default();
        // 40 ms, no avatar: only the scene
        let i = input(40.0, 0.0, 0.0, 40.0);
        let mut changes = Vec::new();
        for _ in 0..400 {
            if let Some(a) = t.step(&cfg, &i) {
                changes.push(a);
            }
        }
        assert_eq!(changes.first(), Some(&TuneAction::DrawDistance(32.0)));
        let avatars_only = AutoTune {
            strategy: Strategy::Avatars,
            ..cfg
        };
        let mut t = Tuner::default();
        assert!((0..400).all(|_| !matches!(t.step(&avatars_only, &i), Some(TuneAction::DrawDistance(_)))));
    }

    #[test]
    fn reaching_the_target_turns_off_unless_locked_then_climbs_back() {
        let cfg = AutoTune {
            enabled: true,
            target_fps: 20,
            ..Default::default()
        };
        let mut t = Tuner::default();
        let fast = input(10.0, 0.0, 0.0, 64.0);
        let done = (0..400).find_map(|_| t.step(&cfg, &fast));
        assert_eq!(done, Some(TuneAction::Done));
        let locked = AutoTune { lock: true, ..cfg };
        let mut t = Tuner::default();
        let up = (0..400).find_map(|_| t.step(&locked, &fast));
        assert_eq!(up, Some(TuneAction::DrawDistance(74.0)));
        // nothing in the background
        let mut t = Tuner::default();
        let away = TuneInput { focused: false, ..fast };
        assert!((0..400).all(|_| t.step(&locked, &away).is_none()));
    }
}
