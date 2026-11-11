# FireDragon features for Aurora: gap table

Status: **gap table researched and independently verified on 2026-11-10; implemented on the port branch on 2026-11-10 and 11 (see "Status" at the end).** Sections 1 to 6 are the gap table as written before the work started; their `file:line` references point at `40f67c5`.

- Aurora commit read: `40f67c5` (main, 2026-11-10). All `file:line` references are to that commit, paths relative to `crates/`.
- FireDragon reference: the candidate list of the task, plus the FireDragon Master Plan (rev 85) feature list. The handoff zip and the FD source on the PC could not be read yet (folder access was refused at the time), so FD behaviour below comes from the task text and the Master Plan only.
- Effort: S = under a day of work, M = a few days, L = a week or more, each including tests and a demo scenario.

## 1. Gap table (the eleven candidates)

| # | Feature | Aurora today | Evidence | Gap to build | Effort | Main risks |
|---|---|---|---|---|---|---|
| 1 | People in region everywhere | **Partly.** The count exists in the Region Tracker window and on the world map, nowhere else a place is shown. | Rule (clusters summed, +1 in our own region, 0 when down): `aurora-viewer/src/world/region_tracker.rs:101-134`, tests `:246-256`. Requests limited to once a minute per region: `world/worldmap.rs:22`, `:611`, `:621`, `:631`. TASKS.md:214. Shown today in the tracker row (`ui/region_tracker.rs:194`) and on the world map (`ui/worldmap/mod.rs:369`, already hidden under RLVa `@showloc` at `:368`). | Reuse `region_tracker::status` and the map's throttled requests; show the number in place details (`ui/place_details.rs:9`), About Land (`ui/land/general.rs`), search details (`ui/search/details.rs:652-656`, `ui/search/aurora_details.rs:361`), Places history, the favorites bar tooltip (`ui/bars/favorites.rs:420`), the mini-map tooltip (`ui/minimap.rs`). Visible rows only. | S–M | Request volume on long lists (Aurora's limit is 1 per region per minute, FD's is 1 per 0.5 s / 30 per minute: keep Aurora's). RLVa `@showloc` must hide it. Both interface styles (Aurora and Legacy) need it. English, French and Japanese text for the label (AGENTS.md:42-47). |
| 2 | Camera Lock + camera undo/redo | **Not there.** | No lock, history or undo in `aurora-viewer/src/camera/` (`focus.rs`, `follow.rs`, `free.rs`, `presets.rs`, `settings.rs`) or `app/`. Rebindable actions exist: `keybinds.rs:319` (`Action`), `settings.rs:1511`. | A lock flag checked where the camera is moved; a ring of recent views with back / forward; three new `Action`s with free default keys; buttons in the Controls window. | M | Many code paths move the camera (follow, focus, free camera, presets, RLVa camera rules, sitting): each must respect the lock. RLVa restrictions must win over it. |
| 3 | Camera Path | **Not there.** Closest: saved camera presets (one position each, optional lens / filters / light). | TASKS.md:353 (camera presets, `ui/viewfinder/presets.rs`, `camera/presets.rs`). No waypoint, path or playback code. | Waypoint list (region, position, rotation, FOV, transition), recorder, player with smooth / linear / ease / cut, loop, hold last shot, keys, FD-compatible XML import / export, stop on teleport or region loss. Builds on feature 2's "something else owns the camera" hook. | L | Cross-region positions (global coordinates, regions not connected). XML compatibility needs the FD format files (`CAMPATH_P1_*.md`) which I have not been able to read yet. |
| 4 | Snapshots beyond 7680, supersampling, sharp detail, Full save, photo profiles | **Mostly there up to 7680.** | Limit: `snapshot/mod.rs:30` (`MAX_SIDE = 7680`). Seamless tiles with a 128 px guard band: `snapshot/mod.rs:438`, planner `:603`. One exposure for the whole photo: `aurora-render/src/exposure.rs:113`, `renderer.rs:9070`. "Photo quality" raises detail, loads textures in view at full size and waits for them: `snapshot/quality.rs:1-16`, `app/snapshot.rs:1562`, `:1747`. Supersampling is 2x only, and only up to half the limit: `snapshot/quality.rs:208-209`. Camera presets can keep lens, filters and light (TASKS.md:353). | (a) raise the limit to 16384; (b) supersampling 4 / 8 / 16 at any size; (c) Full save: photo + cutout + background-only from one moment (no background-only capture exists); (d) photo profiles for the settings the camera presets do not hold, built on the saved-presets library that already exists for graphics (`graphics_presets/saved.rs:1-4`). | M (a) + **L (b)** + M (c) + S–M (d) | Memory: a 16384 x 16384 photo is 1 GB of pixels before encoding. Supersampling today draws the whole enlarged photo and then shrinks it in one go (`app/snapshot.rs:1106`, `:762-768`): 4x of 7680 would need about 3.8 GB, so 4 / 8 / 16 needs the tiles shrunk one by one, a rework of the tile pipeline. Aurora has open issues about photos near the memory budget (TASKS.md:396, #1529). Must be tested on NVIDIA, AMD and Intel. Upstream may not want sizes the Legacy Viewer does not have. |
| 5 | Cutout rules | **Almost all there.** Aurora's photo isolation already removes everything not chosen, including its shadow. | Rule: `snapshot/isolate.rs:1-12`; a click is followed up to its avatar or linkset root, so attachments (worn animesh included) stay with their wearer: `:187-205`; visibility `:210-213`. Hidden objects are drawn in no pass at all, shadows included: `aurora-render/src/shaders/cull.wgsl:348`, `gpu_cull.rs:474`. No impostor either: `scene/mod.rs:4614-4626`. | One gap found: the list of local lights does not look at the isolation (`scene/lights.rs:222-244`; nothing else filters it: `scene/mod.rs:2646-2663`, `:3274-3285`), so a light worn by a hidden avatar or carried by a hidden animesh can still light the subjects. Add that check. | S | Deciding which hidden lights to drop: FD drops those of hidden avatars and animesh; dropping every hidden lamp of the scene would leave subjects unlit. Needs a demo capture to confirm before and after, including reflections of hidden objects in cached reflection probes (not checked yet). |
| 6 | Poser additions | **Partly.** | Guide: `docs/POSER.md`. In-world move / rotate / scale gizmos already exist (POSER.md:44-45, `ik_edit/gizmo.rs:79` `Tool::Move`, `ik_edit/poser/overlay.rs:18`). "Upload as animation…" exists and opens the normal import preview: `ik_edit/poser/ui.rs:427-428`, POSER.md:118-120. Loads Aurora pose files, `.anim`, `.bvh`: `ik_edit/poser/storage.rs:207-222`. Library has an open-folder button (POSER.md:88). No Black Dragon or XML pose reader (only hand presets, `ik_edit/poser/controls.rs:1`). Posing others needs their consent through the bridge (POSER.md:122-150, `world/poser_consent.rs`). | "Show to others" (upload own pose and play it, with the paid-upload warning, consent, Cancel default, session spend counter); uploads placed in Animations / "! Created Poses"; buttons to open those inventory folders; reading and writing Black Dragon pose files. Move arrows: already there, nothing to add. | L | Spends L$ on grids where uploads cost: the consent flow is the feature. Aurora's guide says the poser is local and "saving never uploads": a deliberate upstream design choice. Black Dragon format needs the FD reports (`POSER_*.md`). In-world test must be done by the user (see section 3). |
| 7 | Worn tab on profiles | **Partly.** The list exists as the avatar menu's Inspect, not on the profile. | Inspect of an avatar lists every object it wears, one row per linkset: `app/inspect.rs:46-56`, opened from the avatar menu (`app/context_menu.rs:668`), README.md:474; the selected row is outlined in world (`app/inspect.rs:105-107`). Columns are faces, vertices, triangles, texture memory, VRAM, complexity (`ui/inspect.rs:16-27`); no texture-id column. Profile UI has no worn list: `ui/profile/aurora/`. Derender and blacklist exist: `world/derender.rs`, `ui/blacklist.rs`. | A way in from the profile, grouping by type, and per-row derender / blacklist on the existing Inspect rows. | M | **Conflict with Aurora's rules:** asset ids are never shown or copied unless full permission or the user's own (AGENTS.md:148-152). "Real texture IDs, click to copy" for other people's items breaks that rule, so upstream will refuse it. Options: fork-only, or ids only for your own avatar. Your decision. |
| 8 | Hide a friend | **Not there.** | Only the opposite exists ("Show Friends Only": `world/derender.rs:129`). No hidden-friends list in `ui/people/` or `world/`. | Per-account hidden list (viewer side only), filtered out of the friends lists and online / offline notices, IMs untouched, a Hidden list to undo. | M | Many places list friends (People, contact sets, radar, pickers, notices). FD decision "hidden friends are non-friends everywhere including rendering" touches Show Friends Only. |
| 9 | Staying online | **Partly.** | Log out to the login screen without restarting: README.md:551. Network blips are ridden out: README.md:174, TASKS.md:1067. After a lost link, one click on "Reconnect" returns to the same place (kicks excluded): `app/link_health.rs:1-5`, `:55-83`. After a region restart the login button reads "Reconnect": `ui/login.rs:67-68`, `ui/login/credentials_form.rs:197-199`. The restart alert offers a teleport to a landmark or home: `app/region_restart.rs:511`. | Only the automation is missing. Automatic relogin after a disconnect or restart with return to the same place (countdown, Cancel, give-up rules, never for bans or duplicate logins); automatic evacuation to a backup landmark and return. | M–L | Uses saved credentials automatically: needs care and the user's explicit opt-in. Can only be tested in-world. |
| 10 | Own avatar stays sharp, VRAM reserve | **Mostly there.** | Nothing loaded is lowered without memory pressure, and avatar bakes are never lowered: `scene/texture_budget.rs:9-18`; only own bakes and interface images are favoured (`scene/textures.rs:461-464`). Budget read live from the driver, so other GPU programs (OBS) are counted: `aurora-render/src/vram.rs:1-15`; target = budget less 512 MB, at most 80 %: `scene/texture_budget.rs:11`. A window in the background for 60 s (1 s minimised) gives its textures back, as the Legacy Viewer does: `scene/texture_budget.rs:19-27`, `:243-247`. | Two small things: exempt the textures of your own attachments from pressure lowering (bakes already are), and an option not to give textures back when in the background (or to keep your own). The VRAM reserve needs nothing: measure first. | S–M | The background rule protects two viewers on one card (#1808); an option must not bring that crash back. |
| 11 | Graphics parity (photography) | **Measure first; most items look already covered.** | Shadows: 3 cascades (`aurora-render/src/renderer.rs:41`), filter with adaptive taps (`shaders/common.wgsl:1261-1293`), contact shadows (`contact_shadows.rs`). Specular occlusion from GTAO's bent normal: `shaders/object.wgsl:1054-1057`, `shaders/gtao.wgsl:66-67`. Distant cutout foliage keeps its coverage across mip levels: `aurora-render/src/textures.rs:1853`. Tone curves: Khronos PBR Neutral and ACES only: `shaders/post.wgsl:1,9`, `settings.rs:1355`. 66 looks and .cube tables (`aurora-render/src/looks/`). | Likely only extra tone curves (Reinhard, Uncharted 2, AgX), if side-by-side shots show a need. Everything else: compare first, add nothing Aurora already does as well. | S (tone curves); the rest unknown until measured | Shader changes must also build on Metal (macOS). Side-by-side shots need the user in-world. |

## 2. Master Plan features not in the candidate list

Checked more lightly (search only). Each row was re-checked by the independent verifier.

| Master Plan feature | Aurora | Evidence / note |
|---|---|---|
| Pose other avatars locally | Has it, with consent | `docs/POSER.md:122`, `world/poser_consent.rs` |
| Background-only snapshot (no avatars) | Not found | Part of Full save, feature 4c |
| Subjects' particles in the cutout | Has it | `scene/particles.rs:414` follows the isolation |
| Sharpening after anti-aliasing (CAS) | Has it | `aurora-render/src/renderer.rs:186`, `:7029` |
| Debanding | Does not have it | only name-label dithering, `aurora-render/src/shaders/labels.wgsl:40` |
| Own / others' worn lights switches | Has it | `scene/lights.rs:195-203` |
| Borderless window | Has it | `settings.rs:768` |
| Name-tag fade time | Has it | `settings.rs:1028`, duration `:1032` |
| UI scale / font size | Has it | `settings.rs:627`, `:1554` |
| Camera presets | Has it | TASKS.md:353 |
| Joint-follow camera, cinematic head tracking | Does not have it | nearest: object-anchored presets, `camera/presets.rs:32-34` |
| 360 snapshot | Does not have it | listed as future work, README.md:747 |
| Animation timeline window | Partly | keyframes with a frame slider, per-bone keys, trim, loop, ease (`docs/ANIMATION_CREATION.md:10-57`, `ik_edit/clip.rs`); no multi-track timeline window |
| Motion predictor (smooth motion between updates) | Has it | `world/objects.rs:516-520`, `world/mod.rs:1745-1756` |
| Filtered water reflections | Unclear, probably not | reflection read unfiltered at `aurora-render/src/shaders/common.wgsl:1637`; part of feature 11's comparison |
| Selection outline styles | One style only | `aurora-render/src/selection.rs:1-5` |
| Mouselook options, camera offsets, turning thresholds | Has most | `camera/settings.rs:199-251`; turning thresholds only `settings.rs:1258-1261` |
| ReShade cooperation | Open question | Aurora renders with Vulkan, not OpenGL. Whether your ReShade preset works on Aurora at all (and sees depth) has to be tried on your PC before any photo work is judged. |
| Inventory clothing organizer, Kakadu, texture compression | Not wanted | per the task |

## 3. Where Aurora's own rules differ from the task's rules

The task says to follow Aurora's contributor rules exactly. Four of them change how this job can run:

1. **In-world testing.** Aurora's rule 1 (AGENTS.md:16-20): development tools never connect to a grid and never touch credentials; the owner logs in and tests. The task asked for in-world tests through saved logins. Following Aurora means tests in demo mode, and the in-world checks are done by the owner.
2. **Commit and PR wording** (AGENTS.md:594-595): no attribution trailers in commits or PRs.
3. **Notes do not go in the tracked tree** (AGENTS.md:219-221), everything committed is in English (AGENTS.md:36-38) and says "Legacy Viewer" (AGENTS.md:596), and work is tracked in TASKS.md. A `FIREDRAGON_PORT.md` at the repository root fits a personal fork, not an upstream PR.
4. **Upstream scope.** Aurora's reference is the Legacy Viewer; user feature requests go to its feedback board (AGENTS.md:823-825); each PR needs human validation and the three-language strings. Features 6 (paid uploads) and 7 (texture ids) go against stated upstream choices.

## 4. Build and test status

- Cloned and read: AGENTS.md (all), HUMANS.md, ARCHITECTURE.md (outline, crate table), TASKS.md (searched), README.md (searched), docs/POSER.md, docs/LINUX.md.
- At the time of the gap table: **not built, tests not run.** In my Linux workspace `cargo check --workspace` stops at the sound library (`alsa-sys`: the ALSA development package is missing). Installing it is a download outside Aurora's documented steps, so I did not. Aurora's supported developer build is on Windows (`scripts/setup.ps1`, `aurora-tools.cmd`, `scripts/check.ps1`).

## 5. Proposed order

1. Feature 5, the cutout light check (smallest, extends what exists).
2. Feature 1, people in region (reuses the Region Tracker).
3. Feature 2, Camera Lock and view history.
4. Feature 10, own-avatar texture exemption and background option.
5. Feature 8, hide a friend.
6. Feature 4, snapshots (limit and supersampling first, then Full save, then profiles).
7. Feature 3, Camera Path.
8. Feature 9, staying online.
9. Feature 6, poser additions.
10. Feature 7, Worn tab (after the texture-id decision).
11. Feature 11, graphics, only after side-by-side shots.

## 6. Verification

A second, independent pass re-read every `file:line` reference and searched the repository on its own to try to disprove each "not there" claim (2026-11-10, same commit).

- No "Aurora today" verdict was overturned.
- Three findings changed the table and are applied above: feature 7's list already exists as avatar Inspect (effort lowered to M); feature 9 already has one-click return after a lost link; feature 4b's supersampling is L, not M, because of how the enlarged photo is assembled.
- Eight imprecise references were corrected (world map counts, throttle lines, move gizmo, sharpening, name-tag fade, AGENTS.md lines), and the "not verified" rows of section 2 were resolved.
- Still unchecked: reflections of isolation-hidden objects in cached probes (feature 5); FD's file formats, pending the handoff bundle.

## 7. Status

Updated 2026-11-11. Every feature group was built on its own branch from `40f67c5` or from the port branch, then merged into the port branch. The details of each group (settings, files, what to test in-world) are in TASKS.md (the block at the top) and the new modules in ARCHITECTURE.md.

### Where every group stands

The same for all groups unless a row says otherwise:

- **Verification:** the gap table and the Legacy parity list were verified independently before any code was written (section 6 for the eleven candidates; the parity list was re-searched item by item).
- **Code review:** pending.
- **Tests:** the whole workspace and its tests type-check with no error and no warning (`cargo check --workspace --profile ci --tests`, after the last merge). Unit tests were written for the pure logic of each group but have **not been run yet** (full `scripts/check.ps1` pending). Nothing has been tried in demo mode or in-world yet.

| Group | Branch | Merge | What was built |
|---|---|---|---|
| 11 Tone curves | `feat/tone-curves` | `3b10421` | Reinhard, Uncharted 2 and AgX tone curves. |
| 5 + 10 Cutout lights, own textures | `feat/cutout-own-textures` | `dab99fd` | Lights carried by what the photo isolation hides are put out; our own attachments' textures kept under memory pressure; option to keep textures in the background. |
| 1 People in region | `feat/people-in-region` | `e37fd5d` | The count beside place details, About Land, search, Places, the favorites bar, the back / forward list and the mini-map tooltip; Aurora's throttle kept; hidden under `@showloc`. |
| 2 + 3 Camera Lock, view history, Camera Path | `feat/camera-lock-path` | `fb8bc88` | Camera Lock (Alt+Shift+L), Previous / Next view (Alt+Shift+Z / Y), the Camera Path window with record, playback, loop, named paths and LLSD path file import / export. |
| 8 Hide a friend | `feat/hide-friend` | `52059ca` | Per-account hidden friends, viewer side only, with a Hidden tab in People. |
| 6 + 7 Worn tab, poser files | `feat/worn-tab-poser` | `894b2a6` | Worn tab on profiles (names only, no texture ids, following Aurora's rule); Black Dragon pose files read and written; "! Created Poses" upload folder. "Show to others" (paid pose upload) not built. |
| 9 Staying online | `feat/stay-online` | `ef1c895` | Automatic reconnection (off by default, never after kicks or refused logins) and restart evacuation with the way back. |
| 4 Snapshots | `feat/snapshots-16k` | `56deb2c` | Disk photos up to 16,384 a side, supersampling 4x / 9x / 16x at any size tile by tile, Full save (photo, cut-out, background), photo profiles. |
| B2 Render | `feat/b2-render` | `8626283` | Rendering types, colour grade, debanding, Filmic curve, scene lights, mirror and shadow resolution, terrain scale, DoF extras, probe floor 64 m. |
| B3 Camera | `feat/b3-camera` | `b3f9ae5` | Follow a body part, head roll, third-person and mouselook options, Cameratools window, Look at last chatter; sharp textures during Camera Path playback; lock kept through a local teleport. |
| B1 Photo | `feat/b1-photo` | `bc96052` | 360° photos, depth photos, cut-out of every avatar with a matte, settings reset, guide style, after-save options. |
| B6 People | `feat/b6-people` | `227b73e` | Radar Track, Marks, estate kick / teleport home / ban from resident menus, Share, contact-set notices, name tag and floating text options, chat range spheres, Worn tab grouping and multi-select, hidden friends greyed. |
| B5a Chat | `feat/b5a-chat` | `5a8a4bd` | The Legacy chat display options, the Conversation Log, group chat text moderation. Talk animations (#3322) and group notices in group chat (#3334): now upstream, the port's copies removed. |
| B4 Avatar | `feat/b4-avatar` | `249ca6a` | Pose stand, avatar alignment and Face nearest avatar, turn thresholds and look limits, reset skeleton on stand, Character Test resets, Appearance to XML (now upstream's export, #2359, reached from the port's menu entry); poser animation playback; timeline interpolation, `.anim` opening and autosaved draft. |
| B10 Map | `feat/b10-map` | `b85c095` | Teleport options (map, beacon, back SLURL, screens, unmute), teleport history position / date / time zone, world map region info and landmark filter. |
| B5b Translation | `feat/b5b-translate` | `8ed1c36` | Incoming chat translation (Azure, Google, DeepL; the user's own key in the credential store) and the Omnifilter's message rules. |
| B8 Export | `feat/b8-export` | `fb35b52` | Creator rule for exports (port of FSExportPermsCheck), Backup (.oxp) and Collada export, default texture save format. |
| B5c Notifications | `feat/b5c-notify` | `cfbfdb1` | Script dialog, toast and notice options; grid status news at login. Script dialogs per object: now upstream (#3332), the port's copy removed. |
| B9 Preferences | `feat/b9-prefs` | `5862dcb` | Small Legacy preferences (tooltip delay, HUD scale, proxy, UDP port, music fade…), RLVa texts, web guides, Set Window Size, Clear Group Cache. Destination Guide (#2798, #3342) and Release Keys (#3326): now upstream, the port's copies removed. |
| B7 Build | `feat/b7-build` | `6167a79` | Script editor link for VS Code (localhost only, off by default), recompile scripts, unlink confirmation, pivot point, selection filters, light radius, hide selected. |
| R1 Render leftovers | `feat/r1-render-photo` | `9940f71` | View-only transparency / fullbright, opaque water, DoF on blended faces, animesh impostor exclusion, pre-caching delay, auto-tune to a target frame rate, PBR skies listed first. The Phototools slider window was dropped in favour of the viewfinder design. |
| L2 Social leftovers | `feat/l2-social` | `d8cf9eb` | Omnifilter over notices / offers / dialogs and Legacy notecard import / export, objects' IMs translated, Worn tab creators and "out of view", ban / unban from all my groups, chat range colours and crosshair name, conversation order, moderator styles everywhere, black and white emoji, IM and username colours, IM copies in the transcript, far chat dimming and OOC closing as the Legacy Viewer. |
| L3 Inventory leftovers | `feat/l3-inventory` | `81ec56f` | Thumbnails helper, protected folders manager, hide empty system folders, Windlight bulk import, teleport cancel (now upstream, #3325; the port's copy removed), logout screen, Collada sculpts and backup particles. |
| Next-A Teleport and photo camera polish | `feat/next-a` | merged after the upstream merge | A cancelled teleport the region finishes anyway lands on its back / forward place (on top of upstream's cancel, #3325); the arrow keys' row lit in the viewfinder's sheets, the Follow labels fitted for wide fonts, `AURORA_DEMO_PHOTO_CAMERA`. Its TeleportCancel packet module was dropped for upstream's. |
| 12 Shadow Realm, derender that reverses | `feat/shadow-realm` | (not merged yet) | Per-account Shadow Realm list (local file): listed residents appear nowhere beyond a distance (10 m), red names within it, never removed from the world; World menu, window and every resident menu. Temporary derender confirmed to always reverse (test). Owner's updates of 2026-11-11, items 1 and 2. |
| L3 Inventory leftovers | `feat/l3-inventory` | `81ec56f` | Thumbnails helper, protected folders manager, hide empty system folders, Windlight bulk import, teleport cancel, logout screen, Collada sculpts and backup particles. |

| Follow-ups 3 + 6 (owner's updates of 2026-11-11) | `feat/fd-fixes` | not merged | Poser: a subject switch keeps the open tab, marker clicks with no joint selected, one Rotation / Position / Scale switch, animations keep loading with the window closed (stop on close off); tests for the rest, which Aurora already did. People in region: block and item request lanes (4 at once, then 1 a second each) instead of one shared request every 2 s, rows drawn now first, no count for an unnamed region; the world map's labels ask on their own, as the Legacy Viewer's map. |

Housekeeping after the merges (formatting, one unused constant removed) and the documentation (TASKS.md, ARCHITECTURE.md, README.md, `docs/legacy-settings.csv`) were done on `chore/maintenance`.

### Open follow-ups

Decisions for the owner:

- Teleport defaults: the map now closes on teleport and the teleport history shows SLT, as in the Legacy Viewer; Aurora kept the map open and used UTC. Keep the Legacy defaults or go back to Aurora's?
- Discord rich presence: needs a Discord application id owned by the project and a new dependency.
- The render extras are not part of the graphics presets (the Legacy presets hold several of them).
- Depth of field on blended faces is off by default (always on in the Legacy Viewer): turn it on after a visual check?
- PBR skies first asks one sky settings download per sky item (cached, lowest priority), which the Legacy Viewer does not: acceptable?
- Folder "Change Type" stays out (the Legacy Viewer never shows it, and it risks system folders).

Not built yet:

- Render: selection outline styles; ReShade cooperation (feasibility unknown on Vulkan); a 24-bit depth photo.
- Camera: upper-body aim in mouselook; mouse smoothing outside mouselook.
- Poser: "Show to others" (upload of one's own pose with the paid-upload consent flow). The pose stand uses built poses, not the Legacy animation assets, and has no lock.
- Chat: Legacy automatic dialog replies, web page request filtering in the Omnifilter.
- Notifications: animated script dialogs, toast startup and fade times, overflow height, bottom margin, nearby-chat toasts; the corner position covers llDialog and llTextBox only.
- Build: new-prim default texture, embedded item and new-script template; drag texture offset; selection beam and particle effects; save object back to contents; pathfinding editing; selection level-of-detail override; the rest of the script editor link (object, command and syntax files, Luau, runtime forwarding).
- Export: backup import (rez-and-apply with the upload cost shown and confirmed); object contents in backups.
- Inventory: favourite wearables window; inbox display options and ShowNewInventory.
- Preferences: SOCKS5 proxy (UDP wrapping, password in the credential store), line spacing, notecard and script fonts, inspector tooltip delays. The proxy and the UDP port apply at start, not at each login; a fixed UDP port already taken falls back to a system port (the Legacy Viewer fails the login).
- Worn tab: a separate Hair / Body grouping (needs name rules).

To check against the Legacy Viewer's source or settings:

- Camera defaults chosen without the reference: roll damping 0.5 / 0.75, head scale 0.1, mouselook offsets ±2 m.
- The 10 m dimming radius of far chat, AutoCloseOOC spacing, the talk animation ids.
- Avatar alignment's mouselook threshold "0 = legacy".
- DeepL Chinese: Traditional Chinese and Brazilian Portuguese codes not confirmed (Chinese falls back to ZH).
- Auto-tune ignores the frame rate cap and vsync and does not drop avatars from shadows; tested on synthetic input only.
- The chat range spheres' per-segment depth test: cost not measured.
- Hide selected: look at the outline on a hidden object.

Still to write: the POSER.md and ANIMATION_CREATION.md paragraphs for the poser and timeline additions.
