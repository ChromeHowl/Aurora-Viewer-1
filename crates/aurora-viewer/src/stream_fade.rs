//! The fade of the parcel music when it changes (Preferences › Sound:
//! FSFadeAudioStream, FSAudioMusicFadeIn, FSAudioMusicFadeOut).
//!
//! Port of LLViewerAudio::startInternetStreamWithAutoFade, onIdleUpdate,
//! startFading and getFadeVolume (indra/newview/llvieweraudio.cpp,
//! originally LGPL 2.1): a station playing fades out before the next one
//! starts, which then fades in; with no next station the music fades out
//! and stops. Stopping the music (the user's switch, the Music channel
//! turned off, leaving the world) is at once, as
//! stopInternetStreamWithAutoFade.
//!
//! Difference: a change during a fade-in fades out from the volume reached,
//! where the Legacy Viewer finishes the fade-in first; going back to the
//! station that is fading out fades it in again without restarting it.

use std::time::{Duration, Instant};

/// AUDIO_MUSIC_MINIMUM_FADE_TIME: no division by zero.
const MINIMUM: Duration = Duration::from_millis(10);

/// The fade times.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fade {
    pub fade_in: Duration,
    pub fade_out: Duration,
}

impl Fade {
    /// From the preferences' seconds.
    pub fn from_secs(fade_in: f32, fade_out: f32) -> Fade {
        let secs = |s: f32| Duration::from_secs_f32(if s.is_finite() { s.clamp(0.0, 60.0) } else { 0.0 });
        Fade {
            fade_in: secs(fade_in).max(MINIMUM),
            fade_out: secs(fade_out).max(MINIMUM),
        }
    }
}

/// What the engine is asked to do this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Play(String),
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Phase {
    #[default]
    Idle,
    /// Fading out since `since`, from the volume `from`.
    Out {
        since: Instant,
        from: f32,
    },
    In {
        since: Instant,
    },
}

/// The music played and the fade under way (see the module).
#[derive(Debug, Default)]
pub struct StreamFade {
    /// The station the engine plays (empty: none).
    playing: String,
    /// The station wanted next (mNextStreamURI).
    next: String,
    phase: Phase,
}

fn fraction(since: Instant, now: Instant, length: Duration) -> f32 {
    (now.saturating_duration_since(since).as_secs_f32() / length.max(MINIMUM).as_secs_f32()).min(1.0)
}

impl StreamFade {
    /// The station the engine plays.
    pub fn playing(&self) -> &str {
        &self.playing
    }

    /// Back to nothing played (the login screen).
    pub fn reset(&mut self) {
        *self = StreamFade::default();
    }

    /// The fade's volume now (getFadeVolume).
    fn gain(&self, now: Instant, fade: Fade) -> f32 {
        match self.phase {
            Phase::Idle => 1.0,
            Phase::Out { since, from } => from * (1.0 - fraction(since, now, fade.fade_out)),
            Phase::In { since } => fraction(since, now, fade.fade_in),
        }
    }

    /// One frame: `want` is the station to hear (empty: none); `fade` None
    /// changes at once. Returns what the engine must do and the volume of
    /// the music (0 to 1, multiplied with the user's).
    pub fn update(&mut self, want: &str, fade: Option<Fade>, now: Instant) -> (Option<Cmd>, f32) {
        let Some(fade) = fade else {
            self.phase = Phase::Idle;
            if self.next != want {
                self.next = want.to_owned();
            }
            if self.playing == want {
                return (None, 1.0);
            }
            self.playing = want.to_owned();
            return (Some(if want.is_empty() { Cmd::Stop } else { Cmd::Play(want.to_owned()) }), 1.0);
        };
        let mut cmd = None;
        if self.next != want {
            self.next = want.to_owned();
            let gain = self.gain(now, fade);
            match self.phase {
                Phase::Idle | Phase::In { .. } if self.playing != want => {
                    if self.playing.is_empty() {
                        // nothing to fade out: the new station fades in
                        self.playing = want.to_owned();
                        self.phase = Phase::In { since: now };
                        cmd = Some(Cmd::Play(want.to_owned()));
                    } else {
                        self.phase = Phase::Out { since: now, from: gain };
                    }
                }
                Phase::Out { .. } if self.playing == want => {
                    // back to the station fading out: in again from here
                    self.phase = Phase::In {
                        since: now.checked_sub(fade.fade_in.mul_f32(gain)).unwrap_or(now),
                    };
                }
                _ => {}
            }
        }
        match self.phase {
            Phase::Out { since, .. } if fraction(since, now, fade.fade_out) >= 1.0 => {
                if self.next.is_empty() {
                    self.playing.clear();
                    self.phase = Phase::Idle;
                    return (Some(Cmd::Stop), 1.0);
                }
                self.playing = self.next.clone();
                self.phase = Phase::In { since: now };
                return (Some(Cmd::Play(self.playing.clone())), 0.0);
            }
            Phase::In { since } if fraction(since, now, fade.fade_in) >= 1.0 => self.phase = Phase::Idle,
            _ => {}
        }
        (cmd, self.gain(now, fade))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FADE: Fade = Fade {
        fade_in: Duration::from_secs(3),
        fade_out: Duration::from_secs(2),
    };

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn the_first_station_fades_in() {
        let t0 = Instant::now();
        let mut f = StreamFade::default();
        assert_eq!(f.update("http://a", Some(FADE), t0), (Some(Cmd::Play("http://a".into())), 0.0));
        let (cmd, gain) = f.update("http://a", Some(FADE), at(t0, 1500));
        assert_eq!(cmd, None);
        assert!((gain - 0.5).abs() < 0.01);
        assert_eq!(f.update("http://a", Some(FADE), at(t0, 3000)), (None, 1.0));
        assert_eq!(f.update("http://a", Some(FADE), at(t0, 9000)), (None, 1.0));
    }

    #[test]
    fn a_new_station_waits_for_the_old_one_to_fade_out() {
        let t0 = Instant::now();
        let mut f = StreamFade::default();
        f.update("http://a", None, t0);
        let (cmd, gain) = f.update("http://b", Some(FADE), t0);
        assert_eq!((cmd, gain), (None, 1.0), "the old one goes on, fading");
        let (cmd, gain) = f.update("http://b", Some(FADE), at(t0, 1000));
        assert_eq!(cmd, None);
        assert!((gain - 0.5).abs() < 0.01);
        assert_eq!(f.playing(), "http://a");
        assert_eq!(
            f.update("http://b", Some(FADE), at(t0, 2000)),
            (Some(Cmd::Play("http://b".into())), 0.0)
        );
        assert_eq!(f.playing(), "http://b");
        let (_, gain) = f.update("http://b", Some(FADE), at(t0, 3500));
        assert!((gain - 0.5).abs() < 0.01);
    }

    #[test]
    fn a_parcel_without_music_fades_out_then_stops() {
        let t0 = Instant::now();
        let mut f = StreamFade::default();
        f.update("http://a", None, t0);
        assert_eq!(f.update("", Some(FADE), t0), (None, 1.0));
        assert_eq!(f.update("", Some(FADE), at(t0, 2000)), (Some(Cmd::Stop), 1.0));
        assert_eq!(f.playing(), "");
        assert_eq!(f.update("", Some(FADE), at(t0, 3000)), (None, 1.0));
    }

    #[test]
    fn without_the_fade_every_change_is_at_once() {
        let t0 = Instant::now();
        let mut f = StreamFade::default();
        assert_eq!(f.update("http://a", None, t0), (Some(Cmd::Play("http://a".into())), 1.0));
        assert_eq!(f.update("http://a", None, t0), (None, 1.0));
        // the user stops the music during a fade: at once
        f.update("http://b", Some(FADE), t0);
        assert_eq!(f.update("", None, at(t0, 500)), (Some(Cmd::Stop), 1.0));
        assert_eq!(f.playing(), "");
    }

    #[test]
    fn going_back_during_a_fade_out_fades_the_same_station_in_again() {
        let t0 = Instant::now();
        let mut f = StreamFade::default();
        f.update("http://a", None, t0);
        f.update("http://b", Some(FADE), t0);
        let (cmd, gain) = f.update("http://a", Some(FADE), at(t0, 1000));
        assert_eq!(cmd, None, "not restarted");
        assert!((gain - 0.5).abs() < 0.01, "{gain}");
        assert_eq!(f.update("http://a", Some(FADE), at(t0, 2500)), (None, 1.0));
    }

    #[test]
    fn a_change_during_a_fade_in_fades_out_from_the_volume_reached() {
        let t0 = Instant::now();
        let mut f = StreamFade::default();
        f.update("http://a", Some(FADE), t0);
        let (cmd, gain) = f.update("http://b", Some(FADE), at(t0, 1500));
        assert_eq!(cmd, None);
        assert!((gain - 0.5).abs() < 0.01);
        let (_, gain) = f.update("http://b", Some(FADE), at(t0, 2500));
        assert!((gain - 0.25).abs() < 0.01, "{gain}");
    }

    #[test]
    fn zero_seconds_still_fades_without_dividing_by_zero() {
        let fade = Fade::from_secs(0.0, f32::NAN);
        assert_eq!((fade.fade_in, fade.fade_out), (MINIMUM, MINIMUM));
        let t0 = Instant::now();
        let mut f = StreamFade::default();
        f.update("http://a", Some(fade), t0);
        assert_eq!(f.update("http://a", Some(fade), at(t0, 20)), (None, 1.0));
    }
}
