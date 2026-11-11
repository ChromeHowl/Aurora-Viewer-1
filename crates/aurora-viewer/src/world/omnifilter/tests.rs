use super::*;

fn pattern(text: &str, mode: MatchMode) -> Pattern {
    Pattern {
        text: text.into(),
        mode,
        case_sensitive: false,
    }
}

fn rule(name: &str, action: Action) -> Rule {
    Rule {
        name: name.into(),
        enabled: true,
        action,
        ..Rule::default()
    }
}

fn filter(rules: Vec<Rule>) -> Omnifilter {
    let mut f = Omnifilter::default();
    f.install(RuleSet {
        version: VERSION,
        on: true,
        rules,
        ..RuleSet::default()
    });
    f
}

fn nearby<'a>(sender: &'a str, text: &'a str) -> Haystack<'a> {
    Haystack {
        source: Source::NearbyChat,
        channel: Some(Channel::Say),
        sender,
        text,
    }
}

fn result(f: &Omnifilter, h: Haystack) -> Option<Outcome> {
    f.check(&h).map(|(_, o)| o)
}

#[test]
fn off_or_disabled_rules_match_nothing() {
    let mut f = filter(vec![rule("all", Action::Hide)]);
    assert_eq!(result(&f, nearby("Jane", "hi")), Some(Outcome::Hide), "empty fields match anything");
    let mut set = f.rules().clone();
    set.on = false;
    f.install(set.clone());
    assert_eq!(result(&f, nearby("Jane", "hi")), None, "the filter is off");
    set.on = true;
    set.rules[0].enabled = false;
    f.install(set);
    assert_eq!(result(&f, nearby("Jane", "hi")), None, "the rule is disabled");
    assert!(!RuleSet::default().on, "off by default");
    assert!(!Rule::default().enabled, "a new rule starts disabled");
}

#[test]
fn the_sender_matches_as_a_whole_the_text_anywhere() {
    let mut r = rule("spam", Action::Hide);
    r.sender = pattern("jane", MatchMode::Substring);
    r.content = pattern("BUY", MatchMode::Substring);
    let f = filter(vec![r]);
    assert!(
        result(&f, nearby("Jane Doe", "please buy now")).is_some(),
        "case-insensitive substring"
    );
    assert!(result(&f, nearby("John Doe", "please buy now")).is_none());
    let mut r = rule("exact", Action::Hide);
    r.sender = pattern("Jane Doe", MatchMode::Exact);
    let f = filter(vec![r]);
    assert!(result(&f, nearby("jane doe", "x")).is_some());
    assert!(result(&f, nearby("Jane Doe2", "x")).is_none());
    let mut r = rule("regex", Action::Hide);
    r.sender = pattern("jane.*", MatchMode::Regex);
    r.content = pattern(r"\bsale\b", MatchMode::Regex);
    let f = filter(vec![r]);
    assert!(result(&f, nearby("Jane Doe", "big SALE today")).is_some());
    assert!(
        result(&f, nearby("Mary Jane", "big sale today")).is_none(),
        "the name regex covers the whole name"
    );
    assert!(result(&f, nearby("Jane", "wholesale")).is_none());
}

#[test]
fn case_sensitive_fields() {
    let mut r = rule("cs", Action::Hide);
    r.content = Pattern {
        text: "Hello".into(),
        mode: MatchMode::Substring,
        case_sensitive: true,
    };
    let f = filter(vec![r]);
    assert!(result(&f, nearby("a", "Hello there")).is_some());
    assert!(result(&f, nearby("a", "hello there")).is_none());
}

#[test]
fn the_first_matching_rule_decides() {
    let mut keep = rule("friend", Action::Highlight);
    keep.sender = pattern("Jane Doe", MatchMode::Exact);
    let hide = rule("everyone", Action::Hide);
    let f = filter(vec![keep, hide]);
    assert!(matches!(result(&f, nearby("Jane Doe", "hi")), Some(Outcome::Highlight(_))));
    assert_eq!(result(&f, nearby("John", "hi")), Some(Outcome::Hide));
}

#[test]
fn sources_and_channels_narrow_a_rule() {
    let mut r = rule("objects", Action::Hide);
    r.sources = vec![Source::ObjectChat];
    r.channels = vec![Channel::OwnerSay];
    let f = filter(vec![r]);
    let object = |channel| Haystack {
        source: Source::ObjectChat,
        channel,
        sender: "HUD",
        text: "ping",
    };
    assert!(result(&f, object(Some(Channel::OwnerSay))).is_some());
    assert!(result(&f, object(Some(Channel::Say))).is_none());
    assert!(result(&f, nearby("HUD", "ping")).is_none(), "another source");
    let im = Haystack {
        source: Source::ObjectChat,
        channel: None,
        sender: "HUD",
        text: "ping",
    };
    assert!(
        result(&f, im).is_none(),
        "a message without channel never matches a rule naming one"
    );
    assert_eq!(Channel::of(ChatType::Other(9)), Some(Channel::Direct));
    assert_eq!(Channel::of(ChatType::Debug), Some(Channel::ScriptError));
    assert_eq!(Channel::of(ChatType::StartTyping), None);
}

#[test]
fn actions_hide_replace_highlight_and_set_aside() {
    let mut replace = rule("rewrite", Action::Replace);
    replace.content = pattern("darn", MatchMode::Substring);
    replace.replacement = "[filtered]".into();
    let mut empty = rule("empty replacement", Action::Replace);
    empty.content = pattern("heck", MatchMode::Substring);
    let mut aside = rule("aside", Action::SetAside);
    aside.content = pattern("lag", MatchMode::Substring);
    let mut bright = rule("bright", Action::Highlight);
    bright.content = pattern("party", MatchMode::Substring);
    bright.color = [1, 2, 3, 255];
    let mut f = filter(vec![replace, empty, aside, bright]);

    let mut text = "darn it".to_owned();
    assert_eq!(f.apply(nearby("A", "darn it"), &mut text), Some(None));
    assert_ne!(text, "darn it");
    assert!(text.contains("[filtered]"), "{text}");

    let mut text = "oh heck".to_owned();
    assert_eq!(f.apply(nearby("A", "oh heck"), &mut text), None, "an empty replacement hides");

    let mut text = "so much lag".to_owned();
    assert_eq!(f.apply(nearby("Bob", "so much lag"), &mut text), None);
    let last = f.log.back().expect("set aside");
    assert_eq!(
        (last.from.as_str(), last.text.as_str(), last.rule.as_str()),
        ("Bob", "so much lag", "aside")
    );

    let mut text = "party time".to_owned();
    assert_eq!(f.apply(nearby("A", "party time"), &mut text), Some(Some([1, 2, 3, 255])));
    assert_eq!(text, "party time");

    let mut text = "nothing".to_owned();
    assert_eq!(f.apply(nearby("A", "nothing"), &mut text), Some(None));
    // hidden and replaced messages leave only the rule's name
    assert!(
        f.log
            .iter()
            .filter(|e| e.rule != "aside")
            .all(|e| e.text.is_empty() && e.from.is_empty())
    );
}

#[test]
fn the_log_is_bounded() {
    let mut f = filter(vec![rule("all", Action::Hide)]);
    for _ in 0..MAX_LOG + 20 {
        let mut text = "x".to_owned();
        f.apply(nearby("A", "x"), &mut text);
    }
    assert_eq!(f.log.len(), MAX_LOG);
    f.clear_log();
    assert!(f.log.is_empty());
}

#[test]
fn broken_or_oversized_patterns_disable_their_rule() {
    let mut bad = rule("bad", Action::Hide);
    bad.content = pattern("(unclosed", MatchMode::Regex);
    let mut huge = rule("huge", Action::Hide);
    // compiles to far more than the size limit
    huge.content = pattern(r"\w{1000}\w{1000}\w{1000}", MatchMode::Regex);
    let mut long = rule("long", Action::Hide);
    long.content = pattern(&"a".repeat(MAX_PATTERN + 1), MatchMode::Substring);
    let fine = rule("fine", Action::Highlight);
    let f = filter(vec![bad, huge, long, fine]);
    assert_eq!(f.broken(), &[0, 1, 2]);
    assert!(matches!(result(&f, nearby("A", "(unclosed")), Some(Outcome::Highlight(_))));
}

#[test]
fn backtracking_patterns_stay_linear() {
    let mut r = rule("evil", Action::Hide);
    r.content = pattern("(a+)+$", MatchMode::Regex);
    let f = filter(vec![r]);
    let text = format!("{}!", "a".repeat(5000));
    let started = std::time::Instant::now();
    assert!(result(&f, nearby("A", &text)).is_none());
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn a_file_loads_tolerantly() {
    let json = br#"{
        "on": true,
        "future_field": 1,
        "rules": [
            {"name": "ok", "enabled": true, "content": {"text": "hi"}},
            {"name": "unknown source", "sources": ["Teleports"]},
            "not a rule",
            {"name": "partial"}
        ]
    }"#;
    let (set, skipped) = RuleSet::parse(json);
    assert!(set.on);
    assert_eq!(skipped, 2);
    let names: Vec<&str> = set.rules.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["ok", "partial"]);
    assert_eq!(
        set.rules[1],
        Rule {
            name: "partial".into(),
            ..Rule::default()
        }
    );
    assert_eq!(RuleSet::parse(b"garbage"), (RuleSet::default(), 0));
    // what is written reads back the same
    let (again, skipped) = RuleSet::parse(&set.to_bytes());
    assert_eq!((again, skipped), (set, 0));
}

#[test]
fn rules_over_the_limit_are_cut() {
    let rules: Vec<serde_json::Value> = (0..MAX_RULES + 5).map(|i| serde_json::json!({"name": i.to_string()})).collect();
    let bytes = serde_json::to_vec(&serde_json::json!({"rules": rules})).expect("json");
    assert_eq!(RuleSet::parse(&bytes).0.rules.len(), MAX_RULES);
}

#[test]
fn highlights_are_remembered_by_line() {
    let time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_234_567);
    let source = Uuid::from_u128(0x0f11_7e57);
    assert_eq!(highlight(time, source), None);
    mark(time, source, [9, 9, 9, 255]);
    assert_eq!(highlight(time, source), Some([9, 9, 9, 255]));
    // another line a microsecond later (Windows keeps a SystemTime in steps
    // of 100 ns: a nanosecond later would be the same time there)
    assert_eq!(highlight(time + std::time::Duration::from_micros(1), source), None);
}

#[test]
fn notices_invitations_offers_and_dialogs_have_their_kind() {
    assert_eq!(Source::of_im(0, true), Some(Source::Im));
    assert_eq!(Source::of_im(0, false), None, "the system's lines");
    assert_eq!(Source::of_im(32, true), Some(Source::GroupNotice));
    assert_eq!(Source::of_im(3, true), Some(Source::GroupInvite));
    assert_eq!(Source::of_im(4, true), Some(Source::InventoryOffer));
    assert_eq!(Source::of_im(9, false), Some(Source::InventoryOffer), "an object's offer");
    assert_eq!(Source::of_im(38, true), Some(Source::FriendshipOffer));
    assert_eq!(Source::of_im(22, true), Some(Source::Lure));
    assert_eq!(Source::of_im(26, true), Some(Source::TeleportRequest));
    assert_eq!(Source::of_im(41, true), None, "typing");
}

#[test]
fn a_dialog_is_matched_on_its_buttons_too() {
    let mut world = crate::world::World::new(std::sync::Arc::new(crate::scene::avatar::AvatarLibrary::load()));
    let mut r = rule("vendor", Action::Hide);
    r.sources = vec![Source::ScriptDialog];
    r.content = pattern("button_name=Buy", MatchMode::Substring);
    world.omnifilter = filter(vec![r]);
    let mut message = "Choose".to_owned();
    assert!(world.omnifilter_dialog("Vendor", &mut message, &["Buy".into(), "Cancel".into()]));
    assert!(!world.omnifilter_dialog("Vendor", &mut message, &["Cancel".into()]));
    assert_eq!(message, "Choose", "left as it was");
    // a group notice: hidden by a rule on its kind
    let mut n = rule("notices", Action::Hide);
    n.sources = vec![Source::GroupNotice];
    let f = filter(vec![n]);
    let h = Haystack {
        source: Source::GroupNotice,
        channel: None,
        sender: "Jane Doe",
        text: "Party|Tonight",
    };
    assert_eq!(result(&f, h), Some(Outcome::Hide));
    assert_eq!(result(&f, nearby("Jane", "Party")), None);
}

#[test]
fn a_file_from_before_the_rule_sets_holds_the_default_set_alone() {
    let (set, skipped) = RuleSet::parse(br#"{"version":1,"on":true,"rules":[{"name":"spam","enabled":true}]}"#);
    assert_eq!(skipped, 0);
    assert_eq!(set.set_names(), [DEFAULT_SET]);
    assert_eq!((set.current, set.current_name()), (0, DEFAULT_SET));
    assert_eq!(set.rules.len(), 1);
    assert!(set.sets[0].rules.is_empty(), "the rules in use live in `rules` alone");
    let (none, _) = RuleSet::parse(b"not json");
    assert_eq!(none.set_names(), [DEFAULT_SET]);
}

#[test]
fn rule_sets_are_added_switched_and_removed_keeping_their_rules() {
    let mut set = RuleSet {
        on: true,
        rules: vec![rule("a", Action::Hide)],
        ..RuleSet::default()
    };
    set.normalize();
    assert_eq!(set.add(" ", Vec::new()), Err(SetError::NoName));
    assert_eq!(set.add(DEFAULT_SET, Vec::new()), Err(SetError::Taken));
    // a new set, empty, in use
    assert_eq!(set.add("Club", Vec::new()), Ok(()));
    assert_eq!((set.current, set.rules.len()), (1, 0));
    // a copy of the rules in use
    set.rules.push(rule("b", Action::Replace));
    let copy = set.rules.clone();
    assert_eq!(set.add("Club copy", copy), Ok(()));
    assert_eq!(set.set_names(), [DEFAULT_SET, "Club", "Club copy"]);
    assert_eq!(set.rules[0].name, "b");
    set.select(0);
    assert_eq!(set.rules[0].name, "a");
    set.select(1);
    assert_eq!(set.rules[0].name, "b");
    assert!(set.sets[1].rules.is_empty());
    // the Default set is never removed; another falls back to it
    assert!(set.remove_current());
    assert_eq!(set.set_names(), [DEFAULT_SET, "Club copy"]);
    assert_eq!((set.current, set.rules[0].name.as_str()), (0, "a"));
    assert!(!set.remove_current());
    assert_eq!(set.free_name("Club copy"), "Club copy (2)");
    assert_eq!(set.free_name("New"), "New");
    // the bound
    for n in set.sets.len()..MAX_SETS {
        assert_eq!(set.add(&format!("s{n}"), Vec::new()), Ok(()));
    }
    assert_eq!(set.add("one more", Vec::new()), Err(SetError::Full));
}

#[test]
fn rule_sets_survive_the_file_and_hand_edits_are_bounded() {
    let mut set = RuleSet {
        on: true,
        rules: vec![rule("a", Action::Hide)],
        ..RuleSet::default()
    };
    set.normalize();
    set.add("Second", vec![rule("b", Action::Hide)]).expect("added");
    let (read, skipped) = RuleSet::parse(&set.to_bytes());
    assert_eq!(skipped, 0);
    assert_eq!(read, set);
    // names repeated or missing, the set in use out of range
    let (odd, _) =
        RuleSet::parse(br#"{"on":false,"current":9,"sets":[{"name":"","rules":[{"name":"x"}]},{"name":"Same"},{"name":"Same"}]}"#);
    assert_eq!(odd.set_names(), [DEFAULT_SET, "Same", "Same (2)"]);
    assert_eq!(odd.current, 0);
    assert_eq!(odd.rules.len(), 1, "the set in use takes its rules");
}

#[test]
fn the_filter_matches_the_set_in_use_only() {
    let mut set = RuleSet {
        on: true,
        rules: vec![rule("all", Action::Hide)],
        ..RuleSet::default()
    };
    set.normalize();
    set.add("Quiet", Vec::new()).expect("added");
    let mut f = Omnifilter::default();
    f.replace(set.clone());
    assert_eq!(result(&f, nearby("Jane", "hello")), None);
    set.select(0);
    f.replace(set);
    assert_eq!(result(&f, nearby("Jane", "hello")), Some(Outcome::Hide));
}
