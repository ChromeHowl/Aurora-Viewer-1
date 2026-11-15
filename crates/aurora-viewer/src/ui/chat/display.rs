//! The chat display options (`settings/chat_display.rs`) as the chat's
//! renderers read them: published once per UI frame by the app, as the
//! clock's format is (`bars::clock::set_chat_format`), so the many places
//! that draw a line or a time need no new argument. The defaults until the
//! first frame (and in the tests).

use crate::settings::chat_display::ChatDisplay;
use std::sync::{Arc, RwLock};

static GLOBAL: RwLock<Option<Arc<ChatDisplay>>> = RwLock::new(None);

/// Publishes the options (once per UI frame; copied only when they changed).
pub(crate) fn set(options: &ChatDisplay) {
    let same = GLOBAL
        .read()
        .ok()
        .is_some_and(|g| g.as_deref().is_some_and(|current| current == options));
    if !same && let Ok(mut g) = GLOBAL.write() {
        *g = Some(Arc::new(options.clone()));
    }
}

/// The options in use.
pub(crate) fn get() -> Arc<ChatDisplay> {
    GLOBAL
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(|| Arc::new(ChatDisplay::default()))
}

/// Where the links drawn now stand: the nearby chat (Some(true)), a
/// conversation (Some(false)), or elsewhere (None: profiles, notices…),
/// set around the drawing of a conversation's lines.
pub(crate) fn links_in(ctx: &egui::Context, nearby: Option<bool>) {
    ctx.data_mut(|d| match nearby {
        Some(n) => {
            d.insert_temp(links_id(), n);
        }
        None => d.remove::<bool>(links_id()),
    });
}

fn links_id() -> egui::Id {
    egui::Id::unique("chat_links_in_nearby")
}

/// A labelled web link shows its real address at once on hover: the
/// nearby chat's option there (FSDisableLabeledChatLinksNearbyChat), the
/// IMs' everywhere else, group notices included (FSDisableLabeledChatLinks).
pub(crate) fn labelled_link_hint(ctx: &egui::Context) -> bool {
    let nearby = ctx.data(|d| d.get_temp::<bool>(links_id())).unwrap_or(false);
    let d = get();
    if nearby { d.link_hint_nearby } else { d.link_hint_im }
}
