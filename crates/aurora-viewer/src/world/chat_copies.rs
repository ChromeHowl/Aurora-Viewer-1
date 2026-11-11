//! Lines shown in another place than their own (Preferences › Chat):
//!
//! - a resident's IM or group line copied into the nearby chat
//!   (FSShowIMInChatHistory) and the nearby chat lines over the world
//!   (FSLogImToChatConsole, FSLogGroupImToChatConsole, which then replace
//!   the message's toast), behind "IM:" or the group's name cut to
//!   FSShowGroupNameLength (LLIMModel::addMessage and the v1-style console
//!   of indra/newview/llimview.cpp);
//! - what a blocked resident or object says, kept greyed in the nearby chat
//!   (FSShowMutedChatHistory, process_chat_from_simulator in
//!   indra/newview/llviewermessage.cpp), all originally LGPL 2.1.
//!
//! A copy is written to the nearby chat's transcript only with
//! FSLogIMInChatHistory (`World::log_nearby`), is never counted as unread
//! and never alerts: its own conversation did.

use super::{ChatKind, ChatLine, Origin, World};
use crate::ui_sound::{ImKind, ImMessage};
use aurora_net::{ChatMessage, ChatSourceType, ChatType};
use std::time::SystemTime;

/// The copies kept for the lines over the world (they show 50 rows at most).
const MAX_CONSOLE: usize = 50;

impl World {
    /// A message just added to its conversation (`im_messages`, drained by
    /// the app every frame): copied where the options ask. A resident's
    /// line only, as its toast (`resident_line`: no system line, no object,
    /// nothing of ours or of the blocked).
    pub fn copy_message(&mut self, m: &ImMessage) {
        let console = self.message_in_console(m.kind);
        let nearby = self.chat_display.im_in_nearby_chat;
        if !console && !nearby {
            return;
        }
        let Some(line) = self.resident_line(m).filter(|l| l.origin == Origin::Live) else {
            return;
        };
        let group = (m.kind != ImKind::Private).then(|| self.session_title(&m.session));
        let copy = ChatLine {
            time: line.time,
            from: copy_sender(
                aurora_i18n::tr!("chat-extras-im-prefix"),
                &line.from,
                group.as_deref(),
                self.chat_display.group_name_length,
            ),
            text: line.text.clone(),
            kind: ChatKind::Im,
            source: line.source,
            origin: Origin::Copy,
        };
        if console {
            self.console_copies.push_back(copy.clone());
            while self.console_copies.len() > MAX_CONSOLE {
                self.console_copies.pop_front();
            }
        }
        if nearby {
            self.nearby_push(copy);
        }
    }

    /// A message of this kind goes to the nearby chat lines over the world,
    /// in place of its toast.
    pub(super) fn message_in_console(&self, kind: ImKind) -> bool {
        match kind {
            ImKind::Private => self.chat_display.im_in_console,
            ImKind::Group | ImKind::Conference => self.chat_display.group_im_in_console,
        }
    }

    /// A blocked resident's or object's nearby line (`chat_blocked`): kept
    /// greyed in the nearby chat when asked, else dropped as before. Nothing
    /// else follows it: no bubble, no sound, no alert, no unread count.
    pub(super) fn keep_muted_chat(&mut self, c: ChatMessage) {
        if !self.chat_display.show_muted_chat || c.message.is_empty() {
            return;
        }
        let kind = match (c.source_type, c.chat_type) {
            (ChatSourceType::Agent, t @ (ChatType::Whisper | ChatType::Normal | ChatType::Shout)) => ChatKind::Local(t),
            (ChatSourceType::Object, t @ (ChatType::Whisper | ChatType::Normal | ChatType::Shout | ChatType::OwnerSay)) => {
                ChatKind::Object(t)
            }
            _ => return,
        };
        self.nearby_push(ChatLine {
            time: SystemTime::now(),
            from: c.from_name,
            text: c.message,
            kind,
            source: c.source_id,
            origin: Origin::Muted,
        });
    }
}

/// The sender's name of a copy: "IM: Jane Doe", or "[Group] Jane Doe" for
/// a group line while its name is printed (`length`: -1 the whole name, 0
/// none, else that many characters).
fn copy_sender(prefix: &str, name: &str, group: Option<&str>, length: i32) -> String {
    match group {
        Some(g) if length != 0 && !g.is_empty() => {
            let cut: String = match usize::try_from(length) {
                Ok(n) => g.chars().take(n).collect(),
                Err(_) => g.to_owned(),
            };
            format!("[{}] {name}", cut.trim_end())
        }
        _ => format!("{prefix} {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::mutes::{Mute, MuteType};
    use super::*;
    use crate::scene::avatar::AvatarLibrary;
    use std::sync::Arc;
    use uuid::Uuid;

    const ME: Uuid = Uuid::from_u128(1);
    const JANE: Uuid = Uuid::from_u128(2);
    const GROUP: Uuid = Uuid::from_u128(0x6200);

    fn world() -> World {
        let mut w = World::new(Arc::new(AvatarLibrary::load()));
        w.agent_id = ME;
        w.social.names.insert(JANE, "Jane Doe".into());
        w
    }

    fn receive(w: &mut World, session: Uuid, kind: ImKind, text: &str) -> ImMessage {
        w.session_lines(session);
        w.conversation_push(
            session,
            ChatLine {
                time: SystemTime::now(),
                from: "Jane Doe".into(),
                text: text.into(),
                kind: ChatKind::Im,
                source: JANE,
                origin: Origin::Live,
            },
            true,
        );
        ImMessage {
            kind,
            session,
            new_session: false,
        }
    }

    #[test]
    fn the_sender_of_a_copy_says_where_it_comes_from() {
        assert_eq!(copy_sender("IM:", "Jane", None, -1), "IM: Jane");
        assert_eq!(copy_sender("IM:", "Jane", Some("Builders Guild"), 0), "IM: Jane");
        assert_eq!(copy_sender("IM:", "Jane", Some("Builders Guild"), -1), "[Builders Guild] Jane");
        assert_eq!(copy_sender("IM:", "Jane", Some("Builders Guild"), 9), "[Builders] Jane");
        assert_eq!(copy_sender("IM:", "Jane", Some("Été"), 1), "[É] Jane", "characters, not bytes");
    }

    #[test]
    fn ims_are_copied_only_where_asked() {
        let mut w = world();
        let m = receive(&mut w, JANE, ImKind::Private, "hello");
        w.copy_message(&m);
        assert!(w.chat.is_empty() && w.console_copies.is_empty(), "off by default");
        w.chat_display.im_in_nearby_chat = true;
        w.copy_message(&m);
        let copy = w.chat.back().expect("the copy");
        assert_eq!((copy.origin, copy.text.as_str(), copy.source), (Origin::Copy, "hello", JANE));
        assert!(copy.from.ends_with("Jane Doe"));
        assert!(w.console_copies.is_empty());
        assert_eq!(w.chat_unread, 0, "never unread");
        // the lines over the world take it in place of its toast
        assert!(!w.message_in_console(ImKind::Private));
        w.chat_display.im_in_console = true;
        assert!(w.message_in_console(ImKind::Private) && !w.message_in_console(ImKind::Group));
        w.copy_message(&m);
        assert_eq!(w.console_copies.len(), 1);
    }

    #[test]
    fn a_group_line_is_copied_by_its_own_switch() {
        let mut w = world();
        w.chat_display.group_im_in_console = true;
        let m = receive(&mut w, GROUP, ImKind::Group, "meeting at 5");
        w.copy_message(&m);
        assert_eq!(w.console_copies.len(), 1);
        let p = receive(&mut w, JANE, ImKind::Private, "psst");
        w.copy_message(&p);
        assert_eq!(w.console_copies.len(), 1, "the IMs' switch is off");
    }

    #[test]
    fn a_blocked_residents_line_is_not_copied() {
        let mut w = world();
        w.chat_display.im_in_nearby_chat = true;
        let mute = Mute {
            id: JANE,
            name: "Jane Doe".into(),
            kind: MuteType::Agent,
            flags: 0,
        };
        assert!(w.mutes.add(mute, 0).is_ok());
        let m = receive(&mut w, JANE, ImKind::Private, "hello");
        w.copy_message(&m);
        assert!(w.chat.is_empty());
    }

    #[test]
    fn muted_chat_is_kept_greyed_only_when_asked() {
        let mut w = world();
        let said = |text: &str| ChatMessage {
            from_name: "Jane Doe".into(),
            source_id: JANE,
            owner_id: JANE,
            source_type: ChatSourceType::Agent,
            chat_type: ChatType::Normal,
            position: glam::Vec3::ZERO,
            message: text.into(),
        };
        w.keep_muted_chat(said("hidden"));
        assert!(w.chat.is_empty(), "dropped by default");
        w.chat_display.show_muted_chat = true;
        w.keep_muted_chat(said("kept"));
        let line = w.chat.back().expect("kept");
        assert_eq!((line.origin, line.kind), (Origin::Muted, ChatKind::Local(ChatType::Normal)));
        assert_eq!(w.chat_unread, 0);
        // typing notices and the debug channel never show
        w.keep_muted_chat(ChatMessage {
            chat_type: ChatType::StartTyping,
            ..said("x")
        });
        w.keep_muted_chat(ChatMessage {
            source_type: ChatSourceType::Object,
            chat_type: ChatType::Debug,
            ..said("error")
        });
        assert_eq!(w.chat.len(), 1);
    }
}
