//! Nearby chat said from afar: the lines whose speaker stood beyond the
//! region's say range when they spoke (LFSimFeatureHandler::sayRange, 20 m
//! unless an OpenSim region says otherwise; the older
//! LLAgent::getNearChatRadius of 10 m is no longer what the Legacy Viewer
//! reads there), drawn with their colour diminished
//! (FSBeyondNearbyChatColorDiminishFactor in LLViewerChat::getChatColor,
//! indra/newview/llviewerchat.cpp, originally LGPL 2.1). The line keeps no
//! position: the far ones are remembered here by their moment and speaker.

use super::{ChatKind, ChatLine, World};
use std::collections::VecDeque;
use std::time::SystemTime;
use uuid::Uuid;

/// The far lines remembered (more than the nearby chat keeps is useless).
const MAX_FAR: usize = 2000;

/// The far lines of the nearby chat, oldest first.
#[derive(Default)]
pub struct FarChat(VecDeque<(SystemTime, Uuid)>);

impl World {
    /// A line said now by `source` at `position` (region coordinates; zero
    /// when unknown, never far, as `isExactlyZero` decides).
    pub(super) fn note_chat_range(&mut self, time: SystemTime, source: Uuid, position: glam::Vec3) {
        if far(self.agent.position, position, super::radar::chat_ranges().say) {
            let list = &mut self.far_chat.0;
            list.push_back((time, source));
            while list.len() > MAX_FAR {
                list.pop_front();
            }
        }
    }

    /// The line was said from beyond the say range.
    pub fn said_far(&self, line: &ChatLine) -> bool {
        if !matches!(line.kind, ChatKind::Local(_) | ChatKind::Object(_)) {
            return false;
        }
        // newest first, down to the line's moment
        self.far_chat
            .0
            .iter()
            .rev()
            .take_while(|(t, _)| *t >= line.time)
            .any(|(t, s)| *t == line.time && *s == line.source)
    }
}

/// Beyond the say range `say` of `agent`.
fn far(agent: glam::Vec3, speaker: glam::Vec3, say: f32) -> bool {
    speaker != glam::Vec3::ZERO && agent.distance_squared(speaker) > say * say
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn only_a_known_position_beyond_the_radius_is_far() {
        let me = Vec3::new(128.0, 128.0, 20.0);
        let say = crate::world::radar::CHAT_RANGE;
        assert!(!far(me, Vec3::new(130.0, 128.0, 20.0), say));
        // 12 m: within the say range of 20 m (not the old 10 m radius)
        assert!(!far(me, Vec3::new(128.0, 140.0, 20.0), say));
        assert!(far(me, Vec3::new(128.0, 150.0, 20.0), say));
        assert!(far(me, Vec3::new(128.0, 140.0, 20.0), 10.0), "a region's own range");
        assert!(!far(me, Vec3::ZERO, say), "unknown");
    }

    #[test]
    fn far_lines_are_found_by_their_moment_and_speaker() {
        let mut w = World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
        w.agent.position = Vec3::new(128.0, 128.0, 20.0);
        let (near, away) = (Uuid::from_u128(2), Uuid::from_u128(3));
        let now = SystemTime::now();
        w.note_chat_range(now, near, Vec3::new(129.0, 128.0, 20.0));
        w.note_chat_range(now, away, Vec3::new(100.0, 128.0, 20.0));
        let line = |source| ChatLine {
            time: now,
            from: String::new(),
            text: "hi".into(),
            kind: ChatKind::Local(aurora_net::ChatType::Shout),
            source,
            origin: super::super::Origin::Live,
        };
        assert!(!w.said_far(&line(near)));
        assert!(w.said_far(&line(away)));
    }
}
