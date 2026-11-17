# Upstream commits deliberately not taken

The fork is synced with the upstream `main` by a merge commit that keeps the
fork's tree (`git merge -s ours`): every upstream commit up to that point is
either merged, ported by hand under another commit, or listed here. One line
each: PR, title, reason. Where the fork has the behaviour already, the reason
names where (TASKS.md "Upstream sync batch" lines hold the same decisions).

## Tooling, CI and test-only commits (not part of the viewer)

- #3905 the release menu test follows its four steps, and `-BodyFile` takes a relative path: tooling.
- #3893 build slots hold every compile of the viewer crate: tooling.
- #3916 a Windows release build no longer fails on a long command line: tooling (the fork builds on its own PC runners).
- #3896 one Rust version named in `rust-toolchain.toml`: tooling; the fork's toolchain is the owner's.
- #3914 the shell hook test finds bash on Linux and macOS: tooling.
- #3908 remove nine unused dependencies: tooling; a `Cargo.lock` change needs its own licence regeneration, left for a dedicated change.
- #3902 unit tests pass when the system language is Japanese: tests only.
- #3913 Japanese dates, names, search and layouts the unit tests found: tests only.
- #3900 search tools and a hook for the upstream agents: tooling.
- #3777 French offer layout test in its own process: the fork already isolates that test (batch 1).

## Documentation, archive and news

- #3850 archive the tasks finished before 0.9.10: upstream's TASKS archive.
- #3847 0.9.10 highlights in the README and login news: upstream's release text.
- #3837 mark the Recent tab date filter task as validated: upstream's TASKS text.

## Owner decision

- #3860 "a small extra" (decorative, an 800 KB model): skipped, not wanted.
- #3653 local MCP: never taken (owner decision; not in the fork).

## Already in the fork under another commit

The fork merged these PRs' branch tips ("Merge upstream PR #N") or ported them
by hand; the squashed upstream commit only adds review changes or conflicts
with fork-only code.

- #3819 a new account wears its registration outfit at the first login: the fork's own, more complete `world/outfit.rs` (`step_initial`, #3411: also copies a Library outfit and creates the standard wearables).
- #3711 teleport history time zone: fork's `HistoryZone`, with UTC as the default (owner); upstream's default is SLT.
- #3714 floating text fade, background and hover highlight: fork's `people.text_fade_distance`, `text_background`, `text_hover_highlight`.
- #3696 close ((OOC)) automatically: fork's `chat_display.auto_close_ooc` (richer; a clean apply would duplicate the setting).
- #3810 dim nearby chat beyond chat range: fork's `chat_display.beyond_range_dim`; upstream's per-line flag stored at arrival and the card above the chat bar are not taken.
- #3709 Look at Last Chatter: fork's `Action::LookAtLastChatter`.
- #3713 hide the object-returned messages: fork's `notification_options.hide_object_returned`.
- #3712 mark unnamed objects as (no name): fork's `chat_display.mark_unnamed_objects`.
- #3721 mouselook combat aids and target markers: fork's `people.mouselook_markers`.
- #3718 JavaScript switch for the built-in browsers: fork's "Enable JavaScript" setting.
- #3745 tip toast lifetime: fork's `notification_options.tip_toast_seconds`.
- #3715 script information and display choices in the nearby list's menu: the fork's customizable menus already have the display choices (batch 4).
- #3741, #3807, #3786, #3820, #3717, #3674 render and weather commits: hand-ported onto the fork's own renderer (batches 9 and 10; the contact shadows' two cheaper steps re-applied on the fork's own pass).
- #2691 RLVa folder locks hold items worn through a link: in the fork through the RLVa port; upstream's final form and its tests are taken in batch 11.

## Partly taken, with a known difference

- #3705 distance in name tags: the fork has `people.tag_distance` and the colours by band on its classic tags; upstream's distance on the pill and far name plates (`relation_tag`, `apply_distance_preferences`) is not ported. A real, small gap to port on request.
- #3780 Recent folders sorted by date: taken; its settings migration runs at the fork's settings version 42 (upstream's is 40, which the fork uses for another change).

## Batch 12 (upstream commits up to 45c64fb2f)

Taken: see the "Upstream sync batch 12" line of TASKS.md. Not taken:

### Dependencies, tooling, security policy

- #3909 bump of the minor crates: a `Cargo.lock` change needs `./scripts/third-party-licenses.ps1`, left for a dedicated change.
- #3921 optimisation-free test executable, #3941 optimised dependencies in the CI test jobs: Cargo profile and CI tooling of upstream.
- #3938 the agents' shell hook stays fast on a long script, #3942 other tasks' Git commands no longer restart a build: upstream agent tooling.
- #3949 a security policy for vulnerability reports: it names upstream's reporting channel.

### Already in the fork (the same PR branch was merged); the squash adds only review changes

- #2467 poser follows a new or edited shape: identical, `rebase_pose` and its review fix are present.
- #2427 flying in mouselook follows the camera's pitch, #2818 an object worn from the ground under an RLVa lock, #2407 Alpha mode for any texture with alpha, #3500 @camtextures grey, #3368 RLVa camera distance limits: identical.
- #2615 refuse payments under @pay: identical but one line in `ui/context.rs` (`rlva_can_pay`).
- #3580 secondlife:// links clicked in a page on a prim, #3171 Ctrl+W closes script-opened windows (the fork's own, only a doc comment and tests differ): present.
- #2797 pose stand, #2990 panorama pictures: present (a few review lines differ).

### Present in the fork in another form; the squash's review changes conflict with fork-only code

- #3282 Enter opens the selected inventory items: taken in batch 13 as a delta (Marketplace wear check, links deduplicated, linked folders revealed); only upstream's focus tests are not taken.
- #2819 @interact and @fartouch hold beyond the touch: the fork's own implementation (`rlva_selection_too_far`, `rlva_can_designate` in `policy.rs`); upstream's regrouping into `rlva_selectable` is not taken, so a HUD already selected under @interact is not kept selected.
- #2778 @temprun and @alwaysrun each stop their own run, #2436 Enter toggles typing, #2701 Chat bar setting, #3024 a conversation the user opens takes the keyboard, #3277 minimap name formats, #3284 the SLT clock, #3785 rebindable chat keys: all taken in batch 13 (see below); #2436 was already in the fork.

### Persistence stack, partly taken

- #3931 teleport history across reconnects and shutdown: taken in batch 13 (see below); #3945 and #3950 are now complete with their `tphistory.rs` part.

## Batch 13 (upstream commits up to 2062d136d)

Taken: see the "Upstream sync batch 13" line of TASKS.md. Everything the batch-12 section above lists as "present in another form" is now taken, except #2819. Not taken:

- #2819 @interact and @fartouch hold beyond the touch: the fork has its own implementation (`rlva_selection_too_far`, `rlva_can_designate` in `policy.rs`, with #3951's rule that a worn HUD answers no click under @interact). Upstream regroups the same functions into `rlva_selectable`; taking it would replace the fork's designate rule. Kept as it is; the one visible difference is that a HUD already selected under @interact is not kept selected.
- #3966 the Rust cache save drops the workspace's outputs: taken (it is a CI action of the fork's own `.github`); its TASKS.md line is not copied.
- #3909 bump of the minor crates (batch 12), and any other `Cargo.lock` change: not taken, the licence script cannot be run here.
- Upstream TASKS.md text of every taken change is not copied: the fork's TASKS.md keeps its own lines and the batch-13 line.
- Conflicts resolved with the fork's structure kept (the upstream behaviour is taken, not the upstream code layout):
  - #3277 minimap name formats: the fork's `world::radar::Marks` and `ui/context.rs::mark_submenu` are kept (no `AvatarList::MiniMap`, no `CtxAction::MarkCustom { source }`); the four formats, the display-name options, the RLVa anonymous names and the "resident must be visible" rule are ported.
  - #3546 face-select pattern: the fork already had the earlier branch tip; the final version (HDR scene blend before tone mapping, the HUD pass over the finished HUD image, transparent faces getting their record) replaces it. The fork's `FEATURE_OPAQUE_WATER`, the `overlays` closure and `multi_mesh_materials_events` stay.
  - #3789 stochastic transparency: the fork moved the `ST_BEGIN` to `ST_END` block of `object.wgsl`; the new block replaces it there.
  - #3992 LiveMesh preview key: plug-in version 0.15.0 (the fork's 0.14.11 is superseded).
  - #3976 settings migration: upstream's version 42 is version 43 here, the fork's recent-sort migration already uses 42.
- #3105 square expand icon on minimised floaters: already in the fork (`minimize_title_icon` and its test are identical).

## Batch 14 (upstream commits up to a55a09001)

Taken: see the "Upstream sync batch 14" line of TASKS.md. Not taken:

- #4033 proxy settings (SOCKS 5 and HTTP proxy): the fork has its own complete implementation (`aurora-net/src/socks5.rs`, `NetOptions` / `HttpProxy` in `aurora-net/src/lib.rs`, Preferences › Network). Upstream's is a different one (new `aurora-proxy` crate, `circuit_socket.rs`, `session/proxy.rs`, credentials storage, its own Preferences page) with a `Cargo.lock` change that needs the licence script; taking it would replace the fork's feature, so it needs the owner's decision.
- #4034 target frame rate (Autotune): the fork has its own (`app/autotune.rs`, `settings/render_photo.rs` `AutoTune`, Preferences › Graphics, `scene/render_time.rs`). Upstream's reworks it (`settings/autotune.rs`, `ui/monitor/autotune.rs`, the user's own values kept apart, the Performance window's section) over 2300 lines; replacing the fork's tuner and its settings path is left for the owner's decision.
- #3600 0 s one-shot animation and #2545 flycam notice with the interface: already in the fork (same tests present).
- #4008 and #4025 (TASKS.md and docs only): nothing to take.
- Upstream TASKS.md text of every taken change is not copied: the fork's TASKS.md keeps its own lines and the batch-14 line.
- #3267 volumetric clouds: the fork already carries the cloud renderer (`clouds.rs` is identical); only the final change is taken: the shader's sheet cover for overcast skies, the noise file's loader test, the example's link fix, the demo scenario that only runs in demo mode, and the Hillaire notice in `NOTICE.md`. The fork's other renderer code is untouched.
- Conflicts resolved with the fork's structure kept:
  - #4009 / #4015 photo gaze: the fork's own head and eye limits (`look_limits`, `debug_eyes`) are kept beside the look-over-animations priority; `look_limits` still comes first in `scene/mod.rs`.
  - #3993 / #4016 RLVa camera: the fork's `run-` demo scenarios live in `app/movement.rs`, so only upstream's `offset-zoom` and `no-mouselook` demo steps are added to `app/rlva.rs`; the fork's `third_person_joint_delta(s, f)` and the Director's `track` field are kept.
  - #3975 Director: the fork's rule that the track's view makes a Camera Path or the Camera Lock give the camera up is kept over upstream's per-keyframe targets; the world guides were drawn twice in `app.rs` and are drawn once now.
  - #3989: `widgets::search_box` stays out of the re-export (the fork's own list), `chip_row` is added; the Director keys the lock rework replaced are dropped from the de, pt-BR and zh catalogues.
  - #3123 Environment labels: upstream's version of the fix replaces the fork's earlier one (#3109).

## Batch 15 (upstream commits up to 3704a8182)

Taken: see the "Upstream sync batch 15" line of TASKS.md. Not taken:

- #4035 WebSocket sync with outside script editors: the fork has its own (`script_ws.rs`, TASKS.md "script editor link": JSON-RPC on 127.0.0.1, off by default, the Legacy protocol). Upstream's is a new crate (`aurora-editor-sync`, with tokio-tungstenite 0.30 and sha1 0.11, so a `Cargo.lock` and `THIRD-PARTY-LICENSES.md` change) and rewrites five files of the scripts window that the fork changed for Import scripts. Left for the owner: port it as a dedicated batch, or keep the fork's.
- #3805 shadow quality and ambient occlusion in the viewfinder: the fork keeps its own photo light (`snapshot::light` softness and intensity of the sun's shadows, `render_settings_with`, `render_settings_now`, the photo effects list); upstream's `photo_render_settings`, `drawn_levels` and the new View fields conflict in about fifteen places across `app.rs`, `app/snapshot.rs`, `snapshot/light.rs`, `ui/snapshot.rs` and `gpu_recovery.rs`. Left for a dedicated port, if the owner wants those two controls.
- Conflicts resolved with the fork's structure kept:
  - #3920 dual-panel inventory: `InvAction::Notice`, `ImportScripts`, `EnvironmentSaveParcel` and the fork's script template stay beside `InvAction::Panel` and `DualPanel`; the fork's `open_script` and `open_document` (no `owner` argument) are kept, now called on the pane the action came from; the toolbar catalogue count is 99.
  - #4036 poser import: the fork's `Joint` has an `enabled` flag, so `Joint::new` sets it and `from_joints` sets `channels: Aurora`; a `.xml` file is tried as a Black Dragon pose first and read as a Legacy Viewer pose when it is not one; the fork's removed `Prepared` / `Import` job is not brought back; the fork's `wornposer-*` keys stay.
  - #4037 photo tilt: the fork's follow-joint rows stay above the tilt row.
  - #4055 Recent filters: applies to the dual pane and the extra windows as upstream does.
- Upstream TASKS.md text of every taken change is not copied except its own line.

