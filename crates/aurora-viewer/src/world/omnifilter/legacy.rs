//! The Legacy Viewer's Omnifilter notecards, read and written as files: an
//! LLSD XML map with the header "omnifilter_rule_set", a version, the rule
//! set's name and its rules by name, each with its order. Port of the format
//! of OmnifilterEngine::exportNotecardCreatedCallback, validateImportNotecard,
//! exportToLLSD and importRuleSetFromLLSD (indra/newview/omnifilterengine.cpp,
//! originally LGPL 2.1, Copyright (C) 2025, Zi Ree @ Second Life).
//!
//! What the Legacy rules have and Aurora's do not (a region, an owner, an
//! automatic dialog reply) is left out on import; what Aurora's have and the
//! Legacy rules do not (highlighting, setting aside, the volumes of nearby
//! chat other than script errors) is written as the nearest Legacy rule: a
//! rule that hides.

use super::{Action, Channel, MatchMode, Pattern, Rule, Source};
use aurora_llsd::Llsd;

/// The notecard's header (OmnifilterEngine::HEADER).
pub const HEADER: &str = "omnifilter_rule_set";
/// The newest version read and the one written (OmnifilterEngine::VERSION).
pub const VERSION: i32 = 4;
/// Largest notecard read (a notecard holds at most 64 KiB of text; a file
/// a little more).
pub const MAX_BYTES: usize = 1024 * 1024;

/// OmnifilterEngine::eType, by its value in the notecards.
fn legacy_type(source: Source) -> i32 {
    match source {
        Source::NearbyChat => 0,
        Source::GroupChat => 1,
        Source::GroupNotice => 2,
        Source::GroupInvite => 3,
        Source::Im => 4,
        Source::ObjectChat => 5,
        Source::ObjectIm => 6,
        Source::ScriptDialog => 8,
        Source::FriendshipOffer => 9,
        Source::InventoryOffer => 10,
        Source::Lure => 11,
        Source::TeleportRequest => 12,
    }
}

/// ScriptError (7) is object chat on the debug channel; URLRequest (13) has
/// no counterpart.
fn source_of(value: i32) -> Option<Source> {
    Some(match value {
        0 => Source::NearbyChat,
        1 => Source::GroupChat,
        2 => Source::GroupNotice,
        3 => Source::GroupInvite,
        4 => Source::Im,
        5 | 7 => Source::ObjectChat,
        6 => Source::ObjectIm,
        8 => Source::ScriptDialog,
        9 => Source::FriendshipOffer,
        10 => Source::InventoryOffer,
        11 => Source::Lure,
        12 => Source::TeleportRequest,
        _ => return None,
    })
}

fn mode_value(mode: MatchMode) -> i32 {
    match mode {
        MatchMode::Exact => 0,
        MatchMode::Substring => 1,
        MatchMode::Regex => 2,
    }
}

fn mode_of(value: &Llsd) -> MatchMode {
    match value.as_i32() {
        0 => MatchMode::Exact,
        2 => MatchMode::Regex,
        _ => MatchMode::Substring,
    }
}

/// The rules as a Legacy notecard named `name`.
pub fn export(rules: &[Rule], name: &str) -> String {
    let mut needles = aurora_llsd::Map::new();
    for (order, rule) in rules.iter().enumerate() {
        let mut types: Vec<Llsd> = Vec::new();
        let errors_only = rule.channels == [Channel::ScriptError];
        let sources: Vec<Source> = if rule.sources.is_empty() {
            Source::ALL.to_vec()
        } else {
            rule.sources.clone()
        };
        for source in sources {
            let value = if source == Source::ObjectChat && errors_only {
                7
            } else {
                legacy_type(source)
            };
            types.push(Llsd::Integer(value));
        }
        let replace = if rule.action == Action::Replace {
            rule.replacement.clone()
        } else {
            String::new()
        };
        // a name is the rule's key: an empty or repeated one gets its order
        let mut key = if rule.name.trim().is_empty() {
            format!("{}", order + 1)
        } else {
            rule.name.clone()
        };
        while needles.contains_key(&key) {
            key = format!("{key} ({})", order + 1);
        }
        let needle = aurora_llsd::llsd_map! {
            "order" => order as i32,
            "sender_name" => rule.sender.text.clone(),
            "content" => rule.content.text.clone(),
            "region_name" => String::new(),
            "chat_replace" => replace,
            "button_reply" => String::new(),
            "textbox_reply" => String::new(),
            "sender_name_match_type" => mode_value(rule.sender.mode),
            "content_match_type" => mode_value(rule.content.mode),
            "owner_id" => uuid::Uuid::nil(),
            "types" => Llsd::Array(types),
            "enabled" => rule.enabled,
            "sender_name_case_insensitive" => !rule.sender.case_sensitive,
            "content_case_insensitive" => !rule.content.case_sensitive,
            "reply_delay" => 0.0,
        };
        needles.insert(key, needle);
    }
    let all = aurora_llsd::llsd_map! {
        "header" => HEADER,
        "version" => VERSION,
        "rule_set" => Llsd::Map(needles),
        "name" => name.to_owned(),
    };
    aurora_llsd::to_pretty_xml_string(&all)
}

/// The rules of a Legacy notecard, in their order, with the rule set's
/// name; None when it is not one (header, version, fields).
pub fn import(bytes: &[u8]) -> Option<(String, Vec<Rule>)> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let all = aurora_llsd::from_xml(bytes).ok()?;
    if all.get("header").as_str() != HEADER || !all.has("version") || all.get("version").as_i32() > VERSION {
        return None;
    }
    let needles = all.get("rule_set").as_map()?;
    let mut ordered: Vec<(i32, Rule)> = needles
        .iter()
        .map(|(name, n)| {
            let types = n.get("types").as_array();
            let mut sources: Vec<Source> = Vec::new();
            for source in types.iter().filter_map(|t| source_of(t.as_i32())) {
                if !sources.contains(&source) {
                    sources.push(source);
                }
            }
            // script errors alone: object chat on the debug channel
            let errors_only = types.iter().any(|t| t.as_i32() == 7) && !types.iter().any(|t| t.as_i32() == 5);
            let replacement = n.get("chat_replace").as_str().to_owned();
            let rule = Rule {
                name: name.clone(),
                // no kind at all never matches there: kept, disabled
                enabled: n.get("enabled").as_bool() && !sources.is_empty(),
                channels: if errors_only { vec![Channel::ScriptError] } else { Vec::new() },
                sources,
                sender: Pattern {
                    text: n.get("sender_name").as_str().to_owned(),
                    mode: mode_of(n.get("sender_name_match_type")),
                    case_sensitive: n.has("sender_name_case_insensitive") && !n.get("sender_name_case_insensitive").as_bool(),
                },
                content: Pattern {
                    text: n.get("content").as_str().to_owned(),
                    mode: mode_of(n.get("content_match_type")),
                    case_sensitive: n.has("content_case_insensitive") && !n.get("content_case_insensitive").as_bool(),
                },
                action: if replacement.trim().is_empty() {
                    Action::Hide
                } else {
                    Action::Replace
                },
                replacement,
                ..Rule::default()
            };
            (n.get("order").as_i32(), rule)
        })
        .collect();
    ordered.sort_by_key(|(order, _)| *order);
    let rules = ordered.into_iter().map(|(_, r)| r).take(super::MAX_RULES).collect();
    Some((all.get("name").as_str().to_owned(), rules))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(name: &str) -> Rule {
        Rule {
            name: name.into(),
            enabled: true,
            sources: vec![Source::GroupNotice, Source::InventoryOffer],
            sender: Pattern {
                text: "Spam.*".into(),
                mode: MatchMode::Regex,
                case_sensitive: true,
            },
            content: Pattern {
                text: "free".into(),
                mode: MatchMode::Substring,
                case_sensitive: false,
            },
            action: Action::Replace,
            replacement: "(ad)".into(),
            ..Rule::default()
        }
    }

    #[test]
    fn a_notecard_written_reads_back_the_same_rules_in_order() {
        let rules = vec![rule("Zeta"), rule("Alpha")];
        let text = export(&rules, "My set");
        assert!(text.contains(HEADER));
        let (name, back) = import(text.as_bytes()).expect("a notecard");
        assert_eq!(name, "My set");
        assert_eq!(back, rules, "the order of the rules, not of their names");
    }

    #[test]
    fn script_errors_and_hiding_rules_cross_both_ways() {
        let errors = Rule {
            name: "errors".into(),
            enabled: true,
            sources: vec![Source::ObjectChat],
            channels: vec![Channel::ScriptError],
            ..Rule::default()
        };
        let highlight = Rule {
            name: "mark".into(),
            action: Action::Highlight,
            ..Rule::default()
        };
        let (_, back) = import(export(&[errors.clone(), highlight], "s").as_bytes()).expect("a notecard");
        assert_eq!(back[0].sources, [Source::ObjectChat]);
        assert_eq!(back[0].channels, [Channel::ScriptError]);
        // highlighting has no Legacy counterpart: a rule that hides, every kind
        assert_eq!(back[1].action, Action::Hide);
        assert_eq!(back[1].sources.len(), Source::ALL.len());
    }

    #[test]
    fn what_is_not_a_notecard_of_rules_is_refused() {
        assert!(import(b"hello").is_none());
        let wrong = aurora_llsd::to_xml_string(&aurora_llsd::llsd_map! { "header" => "other", "version" => 1 });
        assert!(import(wrong.as_bytes()).is_none());
        let newer = aurora_llsd::to_xml_string(&aurora_llsd::llsd_map! {
            "header" => HEADER, "version" => VERSION + 1, "rule_set" => Llsd::new_map(), "name" => "x",
        });
        assert!(import(newer.as_bytes()).is_none());
        // a Legacy rule with an unknown kind only (a web page request)
        let url = aurora_llsd::to_xml_string(&aurora_llsd::llsd_map! {
            "header" => HEADER, "version" => 3, "name" => "x",
            "rule_set" => aurora_llsd::llsd_map! { "r" => aurora_llsd::llsd_map! {
                "order" => 0, "enabled" => true, "types" => Llsd::Array(vec![Llsd::Integer(13)]),
            } },
        });
        let (_, rules) = import(url.as_bytes()).expect("read");
        assert!(!rules[0].enabled && rules[0].action == Action::Hide);
        // missing case fields: the Legacy default (insensitive)
        assert!(!rules[0].sender.case_sensitive);
    }
}
