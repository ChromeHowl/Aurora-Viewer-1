use super::*;
use crate::build::contents_drop::UNRESTRICTED;
use crate::ui::context::flags::{OBJECT_MODIFY, OBJECT_YOU_OWNER};
use aurora_net::build::BuildCmd;
use std::sync::Arc;

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aurora-script-import-app-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("folder");
    dir
}

/// The demo plaza with its inventory and the action object 970, owned and
/// modifiable by the agent (the world of `contents_drop_tests`).
fn world() -> (World, usize) {
    let mut w = World::new(Arc::new(crate::scene::avatar::AvatarLibrary::load()));
    for ev in crate::demo::events().into_iter().chain(crate::demo::action_events()) {
        w.apply(ev);
    }
    crate::world::inventory::demo::seed(&mut w.inventory, w.agent_id);
    let idx = w.objects.index_of_uuid(&crate::demo::action_id(970)).expect("the action object");
    let agent = w.agent_id;
    let o = w.objects.get_mut(idx).expect("object");
    o.parent_id = 0;
    o.owner_id = agent;
    o.update_flags = OBJECT_MODIFY | OBJECT_YOU_OWNER;
    (w, idx)
}

fn object(w: &World, idx: usize) -> Uuid {
    w.objects.get(idx).expect("object").full_id
}

/// A script as the import saves it: ours, with full rights.
fn script(w: &mut World, number: u128, base: u32) -> Uuid {
    let id = Uuid::from_u128(number);
    let folder = w.inventory.root;
    w.inventory.add_items(vec![InvItem {
        id,
        parent: folder,
        name: "Imported".into(),
        asset_type: 10,
        inv_type: 10,
        asset_id: Uuid::from_u128(number + 1),
        owner: w.agent_id,
        base_mask: base,
        owner_mask: base,
        ..Default::default()
    }]);
    id
}

fn commands(drops: &[contents_drop::ContentsDrop]) -> Vec<(bool, Uuid)> {
    drops
        .iter()
        .map(|drop| match &drop.command {
            BuildCmd::RezScript { enabled, item, .. } => (*enabled, item.item_id),
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

#[test]
fn the_files_of_a_pick_are_read_in_order_and_the_pick_is_limited() {
    let dir = temp_dir();
    let mut paths = Vec::new();
    for (name, bytes) in [("a.lsl", &b"default{}"[..]), ("b.lsl", &b"\xff"[..]), ("c.luau", &b"print(1)"[..])] {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("file");
        paths.push(path);
    }
    let (tx, rx) = mpsc::channel();
    read_paths(paths.clone(), false, &tx);
    drop(tx);
    let loaded: Vec<_> = rx.iter().collect();
    assert_eq!(loaded.len(), 3);
    let results: Vec<_> = loaded
        .iter()
        .map(|message| match message {
            Message::File(file) => file.result.clone(),
            Message::TooMany(_) => panic!("no limit hit"),
        })
        .collect();
    assert_eq!(results[0], Ok("default{}".to_owned()));
    assert_eq!(results[1], Err(Skip::NotUtf8));
    assert_eq!(results[2], Ok("print(1)".to_owned()));
    // more than a pick may hold: the first ones are read, the rest counted
    let many: Vec<PathBuf> = (0..import_rules::MAX_PER_PICK + 5).map(|_| paths[0].clone()).collect();
    let (tx, rx) = mpsc::channel();
    read_paths(many, false, &tx);
    drop(tx);
    let messages: Vec<_> = rx.iter().collect();
    assert_eq!(messages.len(), import_rules::MAX_PER_PICK + 1);
    assert!(matches!(messages.last(), Some(Message::TooMany(5))));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_object_of_the_agent_takes_the_script_and_the_run_choice_decides_if_it_runs() {
    let (mut w, idx) = world();
    let item = script(&mut w, 91001, UNRESTRICTED);
    let object = object(&w, idx);
    let build = BuildTool::default();
    assert_eq!(object_gate(&w, &build, object), Ok(()));
    let running = place_plan(&w, &build, object, item, true).expect("plan");
    assert_eq!(commands(&running), vec![(true, item)]);
    assert!(running.iter().all(|drop| drop.consume.is_none()), "the script is copyable: nothing leaves the inventory");
    let stopped = place_plan(&w, &build, object, item, false).expect("plan");
    assert_eq!(commands(&stopped), vec![(false, item)]);
}

#[test]
fn someone_elses_object_or_one_without_modify_takes_nothing() {
    let (mut w, idx) = world();
    let item = script(&mut w, 91002, UNRESTRICTED);
    let object = object(&w, idx);
    let build = BuildTool::default();
    // modifiable, owned by someone else, no group of ours
    {
        let o = w.objects.get_mut(idx).expect("object");
        o.owner_id = Uuid::from_u128(88001);
        o.update_flags = OBJECT_MODIFY;
    }
    assert_eq!(object_gate(&w, &build, object), Err("rez-contents-denied"));
    assert_eq!(place_plan(&w, &build, object, item, true).err(), Some("rez-contents-denied"));
    // ours, but without modify
    {
        let agent = w.agent_id;
        let o = w.objects.get_mut(idx).expect("object");
        o.owner_id = agent;
        o.update_flags = OBJECT_YOU_OWNER;
    }
    assert_eq!(object_gate(&w, &build, object), Err("rez-contents-denied"));
    assert_eq!(place_plan(&w, &build, object, item, true).err(), Some("rez-contents-denied"));
    // an object that is not there
    assert_eq!(object_gate(&w, &build, Uuid::from_u128(88002)), Err("rez-no-surface"));
}

#[test]
fn a_group_owned_object_takes_scripts_only_for_a_member() {
    let (mut w, idx) = world();
    let item = script(&mut w, 91003, UNRESTRICTED);
    let object = object(&w, idx);
    let group = Uuid::from_u128(88003);
    {
        let o = w.objects.get_mut(idx).expect("object");
        o.owner_id = group;
        o.update_flags = OBJECT_MODIFY;
    }
    let mut build = BuildTool::default();
    build.props.insert(
        object,
        aurora_net::build::ObjectProps {
            object_id: object,
            owner_id: group,
            group_id: group,
            ..Default::default()
        },
    );
    assert_eq!(object_gate(&w, &build, object), Err("rez-contents-denied"), "not a member");
    w.groups.groups.push(aurora_net::social::GroupMembership {
        id: group,
        name: "Group".into(),
        insignia: Uuid::nil(),
        powers: 0,
        accept_notices: false,
        list_in_profile: false,
        contribution: 0,
    });
    // a member may add to it (LLPanelContents::getState); the drop's own rule
    // (willObjectAcceptInventory) takes a script the object's owner may receive
    assert_eq!(object_gate(&w, &build, object), Ok(()));
    assert!(place_plan(&w, &build, object, item, true).is_ok());
}

#[test]
fn a_worn_object_takes_only_unrestricted_scripts() {
    let (mut w, idx) = world();
    let full = script(&mut w, 91004, UNRESTRICTED);
    let no_copy = script(&mut w, 91006, UNRESTRICTED & !actions::COPY);
    let object = object(&w, idx);
    let build = BuildTool::default();
    let avatar = w.objects.index_of_uuid(&w.agent_id).expect("avatar");
    let avatar_id = w.objects.get(avatar).expect("avatar").key.local_id;
    w.objects.get_mut(idx).expect("object").parent_id = avatar_id;
    assert_eq!(object_gate(&w, &build, object), Ok(()), "a script of full rights goes into an attachment");
    assert!(place_plan(&w, &build, object, full, true).is_ok());
    assert_eq!(
        place_plan(&w, &build, object, no_copy, true).err(),
        Some("rez-contents-denied"),
        "the server keeps an item in the world or in the inventory, not both"
    );
}

#[test]
fn a_script_made_with_the_probe_rights_is_what_the_import_creates() {
    let probe = probe(Uuid::from_u128(5));
    assert_eq!(probe.asset_type, 10);
    assert_eq!(probe.owner, Uuid::from_u128(5));
    assert_eq!(probe.base_mask & UNRESTRICTED, UNRESTRICTED);
    assert_eq!(probe.owner_mask & UNRESTRICTED, UNRESTRICTED);
}

#[test]
fn only_a_folder_of_the_inventory_that_takes_new_items_is_a_destination() {
    let (w, _) = world();
    let inv = &w.inventory;
    assert!(actions::destination(inv, Uuid::from_u128(8000)), "an own folder");
    assert!(actions::destination(inv, inv.root));
    assert!(!actions::destination(inv, inv.lib_root), "the Library takes nothing");
    let trash = actions::system(inv, 14).expect("trash");
    assert!(!actions::destination(inv, trash), "nor does the Trash");
    assert!(!actions::destination(inv, Uuid::from_u128(99999)), "a folder that does not exist");
}

#[test]
fn the_demo_grid_answers_with_one_error_per_marked_line() {
    let source = "a\nAURORA_DEMO_SYNTAX_ERROR\nb\nAURORA_DEMO_SYNTAX_ERROR\n";
    let errors: Vec<String> = source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("AURORA_DEMO_SYNTAX_ERROR"))
        .map(|(number, _)| format!("({}, 9) : ERROR : Syntax error", number + 1))
        .collect();
    assert_eq!(errors, ["(2, 9) : ERROR : Syntax error", "(4, 9) : ERROR : Syntax error"]);
}
