use super::*;
use crate::ui::context::flags::{OBJECT_ANY_OWNER, OBJECT_COPY, OBJECT_MODIFY};

fn parsed(line: &str) -> Option<TakeCommand> {
    let mut i = Scanner::new(line);
    let command = i.word()?;
    parse(command, &mut i)
}

fn key(n: u32) -> ObjKey {
    ObjKey {
        region: Default::default(),
        local_id: n,
    }
}

#[test]
fn arguments() {
    assert_eq!(parsed("zdrop"), Some(TakeCommand::Help(Verb::Zdrop)));
    assert_eq!(parsed("zdrop off"), Some(TakeCommand::Off(Verb::Zdrop)));
    assert_eq!(parsed("zdrop maybe"), Some(TakeCommand::Invalid(Verb::Zdrop, "maybe".into())));
    assert_eq!(
        parsed("zdrop on abc  My   Stuff"),
        Some(TakeCommand::ZdropOn {
            object: Some("abc".into()),
            folder: "My Stuff".into()
        })
    );
    assert_eq!(
        parsed("zdrop on"),
        Some(TakeCommand::ZdropOn {
            object: None,
            folder: String::new()
        })
    );
    assert_eq!(
        parsed("ztake on Take Box"),
        Some(TakeCommand::On {
            verb: Verb::Ztake,
            folder: "Take Box".into()
        })
    );
    assert_eq!(parsed("mtake"), Some(TakeCommand::Help(Verb::Mtake)));
    assert_eq!(parsed("mtake off"), Some(TakeCommand::Off(Verb::Mtake)));
    assert_eq!(parsed("ktake"), Some(TakeCommand::Silent));
    assert_eq!(
        parsed("kcopy Box a b"),
        Some(TakeCommand::Packager {
            copy: true,
            folder: "Box".into(),
            objects: vec!["a".into(), "b".into()]
        })
    );
    assert_eq!(
        parsed("ktake Box"),
        Some(TakeCommand::Packager {
            copy: false,
            folder: "Box".into(),
            objects: vec![]
        })
    );
    assert_eq!(parsed("gtp 1 2"), None);
}

#[test]
fn root_folder_by_exact_name() {
    let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let list = [(a, "Objects"), (b, "Take Box")];
    assert_eq!(find_root_folder(list, "Take Box"), Some(b));
    assert_eq!(find_root_folder(list, "take box"), None);
    assert_eq!(find_root_folder(list, ""), None);
}

#[test]
fn dimension_names() {
    assert_eq!(dimension_name(glam::Vec3::new(1.0, 2.5, 0.25)), "1x2.5x0.25");
    assert_eq!(dimension_name(glam::Vec3::new(10.0, 0.5, 0.01)), "10x0.5x0.01");
    assert_eq!(dimension_name(glam::Vec3::ZERO), "0x0x0");
}

const OWN: u32 = OBJECT_YOU_OWNER | OBJECT_ANY_OWNER;
const FULL: u32 = OWN | OBJECT_COPY | OBJECT_MODIFY | OBJECT_TRANSFER;

fn cand(flags: u32) -> Candidate {
    Candidate {
        flags,
        ..Default::default()
    }
}

#[test]
fn take_rights() {
    // full permissions
    assert_eq!(check_take(&cand(FULL), false), Ok(()));
    assert_eq!(check_take(&cand(FULL), true), Ok(()));
    // our own object with nothing else: taken, not copied (no copy)
    assert_eq!(check_take(&cand(OWN), false), Ok(()));
    assert_eq!(check_take(&cand(OWN), true), Err(Refusal::Copy));
    // our own, no transfer and no modify: still ours
    assert_eq!(check_take(&cand(OWN | OBJECT_COPY), false), Ok(()));
    assert_eq!(check_take(&cand(OWN | OBJECT_COPY), true), Ok(()));
    // someone else's, nothing granted
    assert_eq!(check_take(&cand(OBJECT_ANY_OWNER), false), Err(Refusal::Take));
    assert_eq!(check_take(&cand(OBJECT_ANY_OWNER), true), Err(Refusal::Copy));
    // group-owned through a role: modify and transfer, no copy
    let group = OBJECT_ANY_OWNER | OBJECT_MODIFY | OBJECT_TRANSFER;
    assert_eq!(check_take(&cand(group), false), Ok(()));
    assert_eq!(check_take(&cand(group), true), Err(Refusal::Copy));
    // no transfer
    assert_eq!(check_take(&cand(OBJECT_ANY_OWNER | OBJECT_MODIFY), false), Err(Refusal::Take));
    assert_eq!(
        check_take(&cand(OBJECT_ANY_OWNER | OBJECT_MODIFY | OBJECT_COPY), true),
        Err(Refusal::Copy)
    );
    // no modify
    assert_eq!(check_take(&cand(OBJECT_ANY_OWNER | OBJECT_TRANSFER), false), Err(Refusal::Take));
    // copyable and transferable but not ours
    assert_eq!(check_take(&cand(OBJECT_ANY_OWNER | OBJECT_COPY | OBJECT_TRANSFER), true), Ok(()));
    // copyable but not transferable: stricter than the menu
    assert_eq!(check_take(&cand(OBJECT_ANY_OWNER | OBJECT_COPY), true), Err(Refusal::Copy));
}

#[test]
fn take_never_applies_to_avatars_attachments_seats_or_permanent_objects() {
    let c = |f: &dyn Fn(&mut Candidate)| {
        let mut c = cand(FULL);
        f(&mut c);
        c
    };
    assert_eq!(check_take(&c(&|c| c.avatar = true), false), Err(Refusal::Avatar));
    assert_eq!(check_take(&c(&|c| c.attachment = true), true), Err(Refusal::Attachment));
    assert_eq!(check_take(&c(&|c| c.sitting_on = true), false), Err(Refusal::Take));
    let permanent = Candidate {
        flags: FULL | AFFECTS_NAVMESH,
        in_main_region: true,
        ..Default::default()
    };
    assert_eq!(check_take(&permanent, false), Err(Refusal::Take));
    assert_eq!(check_take(&permanent, true), Ok(()));
}

fn texts(out: &[Out]) -> usize {
    out.iter().filter(|o| matches!(o, Out::Say(_))).count()
}

#[test]
fn ztake_counts_down_then_takes_the_selection_one_per_tick() {
    let t0 = Instant::now();
    let (mut z, first) = Taker::selection(t0, Uuid::nil());
    assert_eq!(texts(&first), 1);
    let sel = [key(1), key(2)];
    let mut now = t0;
    // five countdown ticks
    for _ in 0..5 {
        now += ZTAKE_PERIOD;
        let (out, done) = z.tick(now, &sel);
        assert_eq!(texts(&out), 1);
        assert!(!done);
    }
    // too early: nothing
    assert!(z.tick(now, &sel).0.is_empty());
    // selection read
    now += ZTAKE_PERIOD;
    assert!(z.tick(now, &sel).0.is_empty());
    now += ZTAKE_PERIOD;
    let (out, _) = z.tick(now, &sel);
    assert!(matches!(out[0], Out::Take { key: k, copy: false, .. } if k == key(1)));
    now += ZTAKE_PERIOD;
    let (out, _) = z.tick(now, &sel);
    assert!(matches!(out[0], Out::Take { key: k, .. } if k == key(2)));
    // all taken: told once, then watching the selection again
    assert_eq!(texts(&out), 1);
    // the same objects are not taken twice, a new one is
    now += ZTAKE_PERIOD;
    assert!(z.tick(now, &sel).0.is_empty());
    now += ZTAKE_PERIOD;
    let (out, _) = z.tick(now, &[key(1), key(3)]);
    assert!(out.is_empty());
    now += ZTAKE_PERIOD;
    let (out, _) = z.tick(now, &[key(1), key(3)]);
    assert!(matches!(out[0], Out::Take { key: k, .. } if k == key(3)));
}

#[test]
fn ktake_and_kcopy_take_the_list_and_finish() {
    let t0 = Instant::now();
    let folder = Uuid::from_u128(9);
    let (mut k, first) = Taker::keys(t0, folder, true, vec![key(5), key(6)]);
    assert_eq!(texts(&first), 1);
    assert!(k.deactivated().is_none());
    let (out, done) = k.tick(t0 + ZTAKE_PERIOD, &[]);
    assert_eq!(
        out[0],
        Out::Take {
            key: key(5),
            folder,
            copy: true
        }
    );
    assert!(!done);
    assert_eq!(texts(&out), 0);
    let (out, done) = k.tick(t0 + ZTAKE_PERIOD * 2, &[]);
    assert_eq!(
        out[0],
        Out::Take {
            key: key(6),
            folder,
            copy: true
        }
    );
    assert_eq!(texts(&out), 1);
    assert!(done);
}

#[test]
fn a_long_take_reports_every_ten_objects() {
    let t0 = Instant::now();
    let keys: Vec<ObjKey> = (1..=21).map(key).collect();
    let (mut k, _) = Taker::keys(t0, Uuid::nil(), false, keys);
    let mut said = Vec::new();
    for n in 1..=21u32 {
        let (out, _) = k.tick(t0 + ZTAKE_PERIOD * n, &[]);
        said.push(texts(&out));
    }
    // 20 left after the first, 10 after the eleventh, none after the last
    assert_eq!(said[0], 1);
    assert_eq!(said[10], 1);
    assert_eq!(said[20], 1);
    assert_eq!(said.iter().sum::<usize>(), 3);
}

#[test]
fn mtake_renames_then_takes_after_the_countdown() {
    let t0 = Instant::now();
    let (mut m, _) = Mtake::new(t0, Uuid::nil());
    let sel = [(key(1), glam::Vec3::new(1.0, 2.0, 0.5))];
    let (mut renames, mut takes) = (0, 0);
    for n in 1..=7u32 {
        for o in m.tick(t0 + MTAKE_PERIOD * n, &sel) {
            match o {
                Out::Rename { name, .. } => {
                    assert_eq!(name, "1x2x0.5");
                    renames += 1;
                }
                Out::Take { copy, .. } => {
                    assert!(!copy);
                    assert!(n > 5, "taken during the countdown");
                    takes += 1;
                }
                _ => {}
            }
        }
    }
    assert_eq!((renames, takes), (1, 1));
}

#[test]
fn zdrop_sends_one_item_per_second_from_the_last_and_ends() {
    let t0 = Instant::now();
    let object = Uuid::from_u128(7);
    let items = vec![(Uuid::from_u128(1), "a".to_owned()), (Uuid::from_u128(2), "b".to_owned())];
    let mut z = Zdrop::new(t0, items, object, "obj".into(), "Folder".into());
    assert!(z.tick(t0, true).0.is_empty());
    let (out, done) = z.tick(t0 + ZDROP_PERIOD, true);
    assert!(out.contains(&Out::Drop {
        item: Uuid::from_u128(2),
        object
    }));
    assert!(!done);
    let (out, done) = z.tick(t0 + ZDROP_PERIOD * 2, true);
    assert!(out.contains(&Out::Drop {
        item: Uuid::from_u128(1),
        object
    }));
    assert!(done);
    assert_eq!(texts(&out), 2);
}

#[test]
fn zdrop_stops_when_the_object_is_gone() {
    let t0 = Instant::now();
    let mut z = Zdrop::new(t0, vec![(Uuid::from_u128(1), "a".into())], Uuid::nil(), "o".into(), "F".into());
    let (out, done) = z.tick(t0 + ZDROP_PERIOD, false);
    assert!(done);
    assert_eq!(texts(&out), 3);
    assert!(!out.iter().any(|o| matches!(o, Out::Drop { .. })));
}
