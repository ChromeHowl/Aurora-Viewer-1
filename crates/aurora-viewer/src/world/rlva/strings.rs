//! The RLVa texts the user may change (Preferences › RLVa › Texts), after
//! RlvFloaterStrings (indra/newview/rlvfloaters.cpp) and the entries of
//! rlva_strings.xml marked `customizable` (RlvStrings::setCustomString,
//! indra/newview/rlvcommon.cpp, originally LGPL 2.1). A changed text
//! replaces the translated one everywhere RLVa uses it: shown to us, or
//! sent to the other resident. An empty text gives the translated one back
//! (as the floater's "Revert to default").

use std::borrow::Cow;
use std::collections::BTreeMap;

/// The names of rlva_strings.xml the user may change, in the floater's
/// order. `imquery_list_suffix` is not customizable there either;
/// `blocked_tplurerequest_remote` is never sent by Aurora.
pub const CUSTOMIZABLE: &[&str] = &[
    "blocked_recvim",
    "blocked_recvim_remote",
    "blocked_sendim",
    "imquery_list_deny",
    "stopim_nosession",
    "stopim_endsession_remote",
];

/// The longest text kept: an instant message's (MAX_MSG_BUF_SIZE less its
/// terminator, as the IM field of the Legacy Viewer).
pub const MAX_LEN: usize = 1023;

/// The translated text of a name (rlva_strings.xml's `value`).
pub fn default_text(name: &str) -> &'static str {
    use aurora_i18n::tr;
    match name {
        "blocked_recvim" => tr!("rlva-im-blocked-receive"),
        "blocked_recvim_remote" => tr!("rlva-im-blocked-receive-remote"),
        "blocked_sendim" => tr!("rlva-im-blocked-send"),
        "imquery_list_deny" => tr!("rlva-im-denied"),
        "stopim_nosession" => tr!("rlva-im-no-restriction"),
        "stopim_endsession_remote" => tr!("rlva-im-ended-remote"),
        _ => "",
    }
}

/// The label and the description shown for a name (rlva_strings.xml's
/// `label` and `description`).
pub fn label(name: &str) -> (&'static str, &'static str) {
    use aurora_i18n::tr;
    match name {
        "blocked_recvim" => (tr!("b9-rlva-text-recvim"), tr!("b9-rlva-text-recvim-hint")),
        "blocked_recvim_remote" => (tr!("b9-rlva-text-recvim-remote"), tr!("b9-rlva-text-recvim-remote-hint")),
        "blocked_sendim" => (tr!("b9-rlva-text-sendim"), tr!("b9-rlva-text-sendim-hint")),
        "imquery_list_deny" => (tr!("b9-rlva-text-list-deny"), tr!("b9-rlva-text-list-deny-hint")),
        "stopim_nosession" => (tr!("b9-rlva-text-stopim-none"), tr!("b9-rlva-text-stopim-none-hint")),
        "stopim_endsession_remote" => (tr!("b9-rlva-text-stopim-ended"), tr!("b9-rlva-text-stopim-ended-hint")),
        _ => ("", ""),
    }
}

/// The text of `name`: the user's, else the translated one.
pub fn text(custom: &BTreeMap<String, String>, name: &str) -> Cow<'static, str> {
    match custom.get(name).filter(|t| !t.trim().is_empty()) {
        Some(t) => Cow::Owned(t.clone()),
        None => Cow::Borrowed(default_text(name)),
    }
}

/// Keeps the customizable names only, without empty texts, each at most
/// [`MAX_LEN`] characters.
pub fn sanitize(custom: &mut BTreeMap<String, String>) {
    custom.retain(|name, text| CUSTOMIZABLE.contains(&name.as_str()) && !text.trim().is_empty());
    for text in custom.values_mut() {
        if text.chars().count() > MAX_LEN {
            *text = text.chars().take(MAX_LEN).collect();
        }
    }
}

impl super::Rlva {
    /// The text RLVa uses for `name` (RlvStrings::getString).
    pub fn text(&self, name: &str) -> Cow<'static, str> {
        text(&self.preferences.strings, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_customizable_name_has_a_text_and_a_label() {
        for name in CUSTOMIZABLE {
            assert!(!default_text(name).is_empty(), "{name}");
            let (label, hint) = label(name);
            assert!(!label.is_empty() && !hint.is_empty(), "{name}");
        }
    }

    #[test]
    fn a_changed_text_replaces_the_translated_one_and_an_empty_one_reverts() {
        let mut custom = BTreeMap::new();
        assert_eq!(text(&custom, "blocked_recvim"), default_text("blocked_recvim"));
        custom.insert("blocked_recvim".to_owned(), "Busy, sorry".to_owned());
        assert_eq!(text(&custom, "blocked_recvim"), "Busy, sorry");
        custom.insert("blocked_recvim".to_owned(), "  ".to_owned());
        assert_eq!(text(&custom, "blocked_recvim"), default_text("blocked_recvim"));
    }

    #[test]
    fn sanitize_keeps_only_customizable_non_empty_bounded_texts() {
        let mut custom = BTreeMap::new();
        custom.insert("blocked_sendim".to_owned(), "x".repeat(MAX_LEN + 10));
        custom.insert("imquery_list_suffix".to_owned(), "not customizable".to_owned());
        custom.insert("stopim_nosession".to_owned(), " ".to_owned());
        sanitize(&mut custom);
        assert_eq!(custom.len(), 1);
        assert_eq!(custom["blocked_sendim"].chars().count(), MAX_LEN);
    }

    #[test]
    fn rlva_uses_the_users_text() {
        let mut rlva = super::super::Rlva::default();
        rlva.preferences
            .strings
            .insert("imquery_list_deny".to_owned(), "Not today".to_owned());
        assert_eq!(rlva.text("imquery_list_deny"), "Not today");
        assert_eq!(rlva.text("stopim_nosession"), default_text("stopim_nosession"));
    }
}
