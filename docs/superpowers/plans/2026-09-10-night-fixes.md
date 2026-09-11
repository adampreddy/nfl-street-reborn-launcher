# 2026-09-10 night: fix everything before the 09-12 ship

Adam: "Everything has been tested on other PCs, my friends. Can we fix
everything." Two tracks, one worktree each. Track A (launcher, this file's
owner) on `integration` in `target\merge-tree`. Track B (Publisher redesign)
on branch `publisher-redesign` in its own worktree, merged into
`integration` when both are done.

## Findings this rests on (all measured tonight)

- The launcher reports `env!("CARGO_PKG_VERSION")` = 3.1.0 (workspace bump
  d4391dd, 09-09 15:56) as its own version, whatever `build.ps1 -Version`
  stamped on the installer. `update_check` therefore called 3.0.1-beta.7
  "up to date" on Adam's 16:56 beta. No launcher built since 09-09 can
  self-update to any version below 3.1.0.
- The patch chain has one step, 2.1's NSR.iso (78a987c7) -> NSRV3
  (98bfe6bf). A retail GN7E69 disc is accepted by the importer (game id
  only) and stored unpatched. Adam's own import was already NSRV3.
- `publish/game_patch.rs` encodes with `-B 268435456`; the disc is
  1,459,978,240 bytes. Re-encoded with `-B <disc size>`: 2.1 -> NSRV3 is
  58,536,978 bytes (was 757,972,750); retail (d9063865, the nkit.iso that
  is a full-size image) -> NSRV3 is 62,980,454 bytes. Decode of the retail
  patch reproduced 98bfe6bf byte for byte in 6 s.
- Layout: program under `%LOCALAPPDATA%\Programs\<name>` (7.8 GB, of which
  7.6 GB shipped content that first run COPIES into the data root), data
  under `%APPDATA%\NFL Street Reborn Launcher[ Beta]`. Adam wants one
  folder in Roaming like 2.1.
- Play page at 1920x1009 fills the window; every other page is a 1280 px
  column at x=407, y=126.
- Publisher: one global operation panel shown on all 7 pages, Plan/Dry
  run/Run/Verify/Cancel/Clear on every page, paragraphs of prose, jargon.
  Adam chose "one button per job".

## Track A: launcher (this worktree)

1. Version: `build.ps1 -Version` reaches the exe (`REBORN_BUILD_VERSION`
   at compile time, `option_env!`), one `launcher_version()` used by
   `update_check`/`update_view` and anywhere the version is shown. Proof:
   real-window `update_check` on the beta.8 build reports
   `current: 3.0.1-beta.8`.
2. Patch chain: encoder window = source size; second entry step from the
   retail disc; both patches ~60 MB; the wizard says what will happen to
   the chosen disc (recognised + will be patched / already newest / not
   recognised, used as is). Publish `game-patch` (new version label) and a
   release pinning it. Installer ships the new chain.
3. One folder: `{app}` = `{userappdata}\NFL Street Reborn Launcher[ Beta]\app`;
   the data root is unchanged so nobody's data moves. Seeding hardlinks
   (same volume now) instead of copying: no 7 GB duplicate, instant first
   run. Beta installer removes a previous `%LOCALAPPDATA%\Programs` beta
   program folder (never the 2.1 folder).
4. Play page: controls and the bottom strip sit in the same 1280 column as
   the other pages; the artwork stays full-bleed.
5. Beta.8 full set + launcher-only, first-run walk in the real window from
   a RETAIL disc (must patch to NSRV3), upload, tell Adam. Friends must
   install beta.8 by hand: their copies cannot self-update.

## Track B: Publisher redesign (worktree `target\publisher-tree`, branch `publisher-redesign`)

Owns `crates/reborn-publisher/**`, `crates/reborn-core/src/publish/{mod.rs,upload.rs}`,
`Tools/*publisher*.mjs`, `installer/windows/reborn-publisher.iss`. Touches
nothing else.

- `ui/job.js` replaces `panel.js`: one primary button per job; check ->
  plain-words card + Cancel / Yes, publish -> progress bar (bytes) ->
  "Done. Players now get X. Verified at HH:MM." Errors: one red sentence +
  Copy. Raw log behind a closed "Details". No Dry run box on screen.
- Each page renders only its own operation (`op_status.operation`); another
  page's job shows one line naming it.
- Core: `Event::Progress { done, total }` emitted per object/part by
  `upload::upload`; `OpState.progress`.
- Sidebar order and names: Home, Textures runtime mods, Launcher update,
  Roster, Roll back, then Advanced: Build an ISO, Full installer set, Keys
  and settings. Words: server not bucket, this computer not origin, files
  not objects; sequence/manifest/pins in Details only. Home shows one
  version per thing players get.
- Publisher installs to `{userappdata}\NFL Street Reborn Launcher\publisher\app`.
- Checkers and the two real-window drivers rewritten; Content and Home
  jobs proven in the real window on the rebuilt installer.
