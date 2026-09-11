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

## Corrections found while doing Track A

- A retail disc IS patched already, by the shipped game-import bundle
  (`game-import\reborn.xdelta`, 80 MB): DolphinTool extracts the disc, the
  63 retail files are verified by sha256 (so junk regions do not matter and
  Redump and scrubbed images both pass), concatenated, and patched straight
  to NSRV3. The chain only moves older Reborn discs forward. Adam's
  `GC NFL Street 2 (USA).nkit.iso` is a scrubbed full-size image (sha1
  CFE7AF47…, not Redump's D8603281…), which is exactly what the bundle
  handles. No retail chain step is needed; the wizard wording was.
- Retail -> 2.1 whole-window patch is 4.7 MB; kept in the scratchpad, unused.
- Seeding already hardlinks; beta.4 had copied because the exe ran from
  `%LOCALAPPDATA%` (different tree, links attempted across the same volume
  should have worked; unverified why). With `app` inside the data root the
  walk measures link counts directly.

## Status (23:55)

- Version fix committed (ed84ea0), tests green (21 in update/game_patch/seed).
- game-patch 2026.09.11 + release 3.0.1 seq 10 LIVE (58.5 MB blob, read back).
- Layout, wizard wording, play column committed (d2a0812); beta.8 full build
  running (schtasks RebornBeta8Full); then walk from the retail disc with
  `REBORN_WALK_DISC` + `REBORN_WALK_EXPECT_OUTCOME=bundle`, then upload,
  then `publish-beta-launcher.ps1 -Version 3.0.1-beta.8`.

## Status (00:25)

- beta.8 built, walked from the retail disc (ALL PASS, bundle outcome,
  17 s to NSRV3), version belief 3.0.1-beta.8, play column aligned at
  1280 and 1920, set + self-update LIVE.
- BUT the walk's fresh root planned a 6.4 GB download: `Paths::install_root`
  on Windows was the fixed `%LOCALAPPDATA%\Programs\...` path, so seeding
  found nothing beside the moved exe. Fixed: the folder holding
  `reborn-launcher.exe` (falls back to the channel path for test binaries).
  beta.9 replaces beta.8 (same set otherwise); beta.8 installs self-update.
- The restore of Adam's beta data into the new root: robocopy exit 11
  (some files failed to move; the leftovers were removed). Their game,
  library, mods, users and config are present; the beta root is test data.

## Status (01:05): DONE

- beta.9 (7d256ee) full set + self-update LIVE; beta.6/7/8 removed from the bucket.
- Self-update PROVEN on Adam's PC: beta.8 saw beta.9, downloaded 64,471,089 B
  during startup, exited at 4 s to apply, came back as 3.0.1-beta.9 up to
  date, and fetched game-patch 2026.09.11 in the same start. Wart: it landed
  in the retired Programs folder because beta.8's `update::apply` restated
  its (old) install root as /DIR; fixed (e030ce3): a root under Programs is
  not restated. The beta.9 full Setup run here moved this PC into `app` and
  removed the Programs copy.
- Seeding PROVEN on the installed beta.9 against a fresh scratch root: first
  check adopts runtime 1.1.2, textures 1.2.0 (8,317 files, hardlinked),
  mods 1.0.0, game-patch 2026.09.11 with 0 bytes to download; only the
  roster (32 MB, not shipped) is fetched.
- Publisher redesign merged (b05e4d9) and installed at
  `%APPDATA%\NFL Street Reborn Launcher\publisher\app`.
- Not yet shipped in any beta: e030ce3 (the /DIR rule) and 6803747 is in
  beta.9. Saturday's stable build carries everything on `integration`.

## After the ship-blocking work (02:00 onwards)

- Drop folders `C:\Reborn Publisher\` (README, one subfolder per thing,
  hardlinked to what release 10 pins); Publisher `drop_root` setting and
  per-thing remembered folders (be08eca, 218d08e); proven on the installed
  Publisher (Textures/Runtime/Mods propose their folders; settings field).
- B and X for returning players (4b301ea): migrated swap undone + one-time
  notice; Swap X and B button; press-first assign; first run folds the
  grid. beta.10 launcher-only publish carries it.

## Morning batch (05:30 to 06:00)

- beta.12: migrated Dolphin.ini absolute paths rebased onto the user folder
  (textures from the old 2.1 folder); content re-check every 5 min.
- beta.13: installer icon restored; CREATE_NO_WINDOW everywhere a console
  tool runs; Display item on Play matches its neighbours; Uninstall row;
  Controllers on shared margins; setup logo lower; controller step fits
  1280x800 folded and opened (review asserts it).
- beta.14: netplay runs the texture sync before launch ("no textures on
  launch"); invite links (`nflstreetreborn://join/<code>`, installer
  registry, pending-join file, Online prefill, Copy invite).

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
