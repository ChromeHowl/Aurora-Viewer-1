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
