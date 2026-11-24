//! Settings of the status shown beside the name plate (`crate::status_share`):
//! what we tell the other Aurora viewers nearby about what we listen to, use
//! or write, and whether we show theirs. Everything we share is off by
//! default; nothing leaves the viewer until the master switch is on.

use serde::{Deserialize, Serialize};

/// Longest text of our own, in bytes (`aurora_net::avatar_status::MAX_DETAIL`).
pub const CUSTOM_MAX: usize = aurora_net::avatar_status::MAX_DETAIL;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusShare {
    /// Master switch: share a status at all.
    pub share: bool,
    /// Share the track that plays (Windows media session).
    pub music: bool,
    /// With the music: "Artist - Title", else only "Listening to music".
    pub music_detail: bool,
    /// Share a program of the table that runs.
    pub programs: bool,
    /// The programs (ids of `crate::status_share::PROGRAMS`) we agree to show.
    pub programs_allowed: Vec<u16>,
    /// Share the text below.
    pub custom: bool,
    pub custom_text: String,
    /// Nothing is shared while we are busy or invisible.
    pub hide_when_busy: bool,
    /// Show the statuses of the others.
    pub show_others: bool,
}

impl Default for StatusShare {
    fn default() -> Self {
        StatusShare {
            share: false,
            music: false,
            music_detail: true,
            programs: false,
            programs_allowed: Vec::new(),
            custom: false,
            custom_text: String::new(),
            hide_when_busy: true,
            show_others: true,
        }
    }
}

impl StatusShare {
    /// Values of a hand-edited file brought back into their ranges.
    pub fn sanitize(&mut self) {
        self.custom_text = aurora_net::avatar_status::clean_detail(&self.custom_text);
        self.programs_allowed.retain(|id| crate::status_share::program(*id).is_some());
        self.programs_allowed.sort_unstable();
        self.programs_allowed.dedup();
    }

    /// Whether a program is ticked.
    pub fn allows(&self, id: u16) -> bool {
        self.programs_allowed.contains(&id)
    }

    /// Ticks or unticks a program.
    pub fn allow(&mut self, id: u16, on: bool) {
        self.programs_allowed.retain(|p| *p != id);
        if on {
            self.programs_allowed.push(id);
            self.programs_allowed.sort_unstable();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_shared_by_default() {
        let d = StatusShare::default();
        assert!(!d.share && !d.music && !d.programs && !d.custom);
        assert!(d.programs_allowed.is_empty() && d.hide_when_busy && d.show_others);
    }

    #[test]
    fn an_old_settings_file_loads_with_the_defaults() {
        let s: StatusShare = serde_json::from_str("{}").expect("empty object");
        assert_eq!(s, StatusShare::default());
        let s: StatusShare = serde_json::from_str(r#"{"share":true}"#).expect("one field");
        assert!(s.share && !s.music);
    }

    #[test]
    fn a_hand_edited_file_is_brought_back() {
        let mut s = StatusShare {
            custom_text: format!("a\u{202E}b{}", "x".repeat(300)),
            programs_allowed: vec![3, 60_000, 1, 3],
            ..StatusShare::default()
        };
        s.sanitize();
        assert!(s.custom_text.len() <= CUSTOM_MAX && s.custom_text.starts_with("abx"));
        assert_eq!(s.programs_allowed, vec![1, 3]);
    }

    #[test]
    fn programs_are_ticked_and_unticked() {
        let mut s = StatusShare::default();
        s.allow(3, true);
        s.allow(1, true);
        s.allow(3, true);
        assert_eq!(s.programs_allowed, vec![1, 3]);
        s.allow(1, false);
        assert!(s.allows(3) && !s.allows(1));
    }
}
