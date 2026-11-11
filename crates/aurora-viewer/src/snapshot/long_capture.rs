//! A long photo and the link with the regions. A large or supersampled
//! photo is drawn in parts, one a frame, and can take minutes; the window
//! keeps answering between its parts and the network runs on its own
//! threads (aurora-net: pings answered and acks sent there), so the circuit
//! never waits for the photo. The regions are still told, as the Legacy
//! Viewer tells them around its long stalls (send_agent_pause /
//! send_agent_resume, indra/newview/llworld.cpp; LLViewerWindow::rawSnapshot
//! for its high resolution photos): AgentPause once the photo is expected
//! or seen to take [`LONG`], AgentResume when it ends however it ends, and
//! the paused time is left out of the circuit timeout without hiding a real
//! loss (aurora-net agent_pause.rs). A photo running past [`WARN_AFTER`] is
//! logged, with the longest the main thread went without a frame.

use std::time::{Duration, Instant};

/// A photo expected or seen to take this long pauses the agent.
pub const LONG: Duration = Duration::from_secs(5);
/// A photo still drawing after this long is logged as a warning.
pub const WARN_AFTER: Duration = Duration::from_secs(60);

/// What the app does this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Send AgentPause.
    Pause,
    /// Send AgentResume.
    Resume,
    /// The photo has been drawing for this long; the longest frame so far.
    Warn { elapsed: Duration, longest: Duration },
    /// The photo ended after this long; its longest frame.
    Ended { elapsed: Duration, longest: Duration },
}

#[derive(Debug, Clone, Copy)]
struct Run {
    started: Instant,
    last: Instant,
    longest: Duration,
    estimate: Option<Duration>,
    warned: bool,
}

/// The photo under way and whether the regions were told the agent paused.
#[derive(Debug, Default)]
pub struct LongCapture {
    run: Option<Run>,
    paused: bool,
}

impl LongCapture {
    /// Each frame: `capturing` while a photo (or a 360° photo, a Full save)
    /// is under way, `estimate` its expected drawing time when known,
    /// `connected` while a session is open (none: nothing is sent, and a
    /// pause ends with the session that held it).
    pub fn update(&mut self, now: Instant, capturing: bool, estimate: Option<Duration>, connected: bool) -> Vec<Step> {
        let mut steps = Vec::new();
        if !connected {
            self.paused = false;
        }
        if !capturing {
            if let Some(run) = self.run.take() {
                if std::mem::take(&mut self.paused) && connected {
                    steps.push(Step::Resume);
                }
                steps.push(Step::Ended {
                    elapsed: now.saturating_duration_since(run.started),
                    longest: run.longest.max(now.saturating_duration_since(run.last)),
                });
            }
            return steps;
        }
        let run = self.run.get_or_insert(Run {
            started: now,
            last: now,
            longest: Duration::ZERO,
            estimate,
            warned: false,
        });
        // (a later, measured estimate replaces the first one)
        run.estimate = estimate.or(run.estimate);
        run.longest = run.longest.max(now.saturating_duration_since(run.last));
        run.last = now;
        let elapsed = now.saturating_duration_since(run.started);
        let long = elapsed >= LONG || run.estimate.is_some_and(|e| e >= LONG);
        if long && connected && !self.paused {
            self.paused = true;
            steps.push(Step::Pause);
        }
        if elapsed >= WARN_AFTER && !run.warned {
            run.warned = true;
            steps.push(Step::Warn {
                elapsed,
                longest: run.longest,
            });
        }
        steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn a_short_photo_never_pauses_the_agent() {
        let t = Instant::now();
        let mut c = LongCapture::default();
        assert!(c.update(t, true, Some(Duration::from_millis(800)), true).is_empty());
        assert!(c.update(t + S(1), true, None, true).is_empty());
        let end = c.update(t + S(2), false, None, true);
        assert_eq!(end.len(), 1);
        assert!(matches!(end[0], Step::Ended { elapsed, .. } if elapsed == S(2)));
        assert!(!c.paused);
    }

    #[test]
    fn a_photo_expected_to_be_long_pauses_before_its_first_part() {
        let t = Instant::now();
        let mut c = LongCapture::default();
        assert_eq!(c.update(t, true, Some(S(40)), true), vec![Step::Pause]);
        assert!(c.paused);
        assert!(c.update(t + S(1), true, Some(S(40)), true).is_empty(), "paused once");
        let end = c.update(t + S(41), false, None, true);
        assert_eq!(end[0], Step::Resume);
        assert!(matches!(end[1], Step::Ended { .. }));
        assert!(!c.paused);
    }

    #[test]
    fn an_unmeasured_photo_pauses_once_it_has_taken_long() {
        let t = Instant::now();
        let mut c = LongCapture::default();
        assert!(c.update(t, true, None, true).is_empty());
        assert!(c.update(t + S(4), true, None, true).is_empty());
        assert_eq!(c.update(t + S(5), true, None, true), vec![Step::Pause]);
    }

    #[test]
    fn a_photo_past_a_minute_is_warned_once_with_its_longest_frame() {
        let t = Instant::now();
        let mut c = LongCapture::default();
        c.update(t, true, None, true);
        c.update(t + S(5), true, None, true);
        // a part that held the main thread 12 s
        c.update(t + S(17), true, None, true);
        let steps = c.update(t + S(61), true, None, true);
        assert_eq!(
            steps,
            vec![Step::Warn {
                elapsed: S(61),
                longest: S(44)
            }]
        );
        assert!(c.update(t + S(70), true, None, true).is_empty(), "warned once");
    }

    #[test]
    fn a_session_lost_during_the_photo_sends_no_resume() {
        let t = Instant::now();
        let mut c = LongCapture::default();
        assert_eq!(c.update(t, true, Some(S(30)), true), vec![Step::Pause]);
        // the session ended under the photo: the pause ended with it
        assert!(c.update(t + S(10), true, None, false).is_empty());
        assert!(!c.paused);
        let end = c.update(t + S(20), false, None, false);
        assert!(!end.contains(&Step::Resume));
        // a new session during the same photo is paused again
        let mut c = LongCapture::default();
        c.update(t, true, Some(S(30)), false);
        assert_eq!(c.update(t + S(1), true, None, true), vec![Step::Pause]);
    }

    #[test]
    fn every_end_resumes_the_agent() {
        // given up, failed or saved: the photo is no longer under way
        let t = Instant::now();
        for end in [S(6), S(90), S(400)] {
            let mut c = LongCapture::default();
            c.update(t, true, None, true);
            c.update(t + S(5), true, None, true);
            assert!(c.paused);
            let steps = c.update(t + end, false, None, true);
            assert_eq!(steps[0], Step::Resume, "{end:?}");
            assert!(!c.paused);
        }
    }
}
