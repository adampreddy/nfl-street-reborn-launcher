# Launcher polish handoff

Worktree: `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\target\merge-tree` (`integration`).
These changes are local and uncommitted. Do not copy the older root worktree over them.

## Implemented

- Controllers retain controller navigation on entry. Testing and input capture pause navigation, with Escape or View + Menu to finish testing. All four player tabs are reachable with the D-pad, including during play.
- A permanent Controllers message states **B maps to B; X maps to X**. The correction button appears only when both bindings are exactly reversed, and restores the canonical mapping.
- Game volume and mute apply immediately to the running Reborn Dolphin. Controller bindings and assignments apply live and save for later launches. Other emulator settings remain protected while Dolphin owns its configuration.
- Offline controller assignment also hot-plugs the corresponding GameCube port. NetPlay changes only local bindings; host port ownership, memory cards, and synchronized settings stay with NetPlay.
- Fullscreen on Play is a checkbox.
- Stats uses a single game-history panel with compact counts, aligned score rows, team/player search, Online/Offline filters, and six games per page. Each game opens a separate box-score dialog with team comparisons and player Offense/Defense views. Closing it preserves the history and focus. Profile and result sync are folded below history.
- Game mode comes from each queue entry's `capture_folder`, never the capture's `source` (which is always `dolphin`). Legacy records without mode are marked Unknown and excluded from Online/Offline filters. Missing side verification is treated as unverified.
- Music reads the selected playing disc's song metadata, rather than assuming the pristine import is still current. Metadata caching uses path, size, and modification time. Custom music defaults off; applying staged custom songs opts in.
- Both update channels check throughout the launcher every two minutes, on reconnect, and after returning to a stale window. Downloads survive page navigation. Release notes, a 15-second automatic countdown, and a 15-minute reminder are provided. Automatic installation waits for gameplay and other writes to finish. A policy change cancels an unaccepted automatic countdown.
- Closing during a launcher/content download warns before exiting. Closing during content installation is blocked until it completes.
- Runtime-only packaging now stages directly into its own payload, preserves the full installer payload, and rejects sources inside the folder it is about to restage.

## Runtime pairing is required

Live settings require the accompanying Dolphin changes in:

`C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\dolphin-exgl`

Changed files: `Source/Core/DolphinQt/MainWindow.cpp` and `MainWindow.h`.

Built executable:

`Binary\x64\NFL Street Reborn Dolphin.exe`

The bridge uses `User\Config\RebornLive.json` and `.command`. Commands are atomic JSON writes, scoped to the current PID and launch instance, and acknowledged after applying on Dolphin's Qt thread. Controller updates take the input state lock. The launcher never races live Dolphin.ini writes. An old runtime is not falsely reported as supporting live edits.

**The launcher-only installer does not carry this runtime.** The matching runtime component is staged, with ReShade, dependencies, licensing and the sanitized tuned User template, at:

`target\installer-payload-runtime\runtime`

The native executable's SHA-256 is `d6067bda3751847f6cbb41d2669d6b797c9910a2ff17f5f6f7dbf8f039c35a5d`. It matches the tested build. Publish this runtime component alongside the launcher before distributing live settings. Nothing has been uploaded or installed into Adam's running game.

Local launcher artifact:

`dist\NFL Street Reborn Beta Launcher Update 3.0.1-beta.28.exe`

Unpacked launcher:

`target\installer-payload-launcher\reborn-launcher.exe`

Installer SHA-256: `351a494fd5ddae5a96a5a49d6d426c653bf55fdf6c04d49c2803e1a19ea88b02`. The local `dist\latest-beta.json` matches this installer and has release notes. Beta.28 includes the final Stats redesign and game-mode correction. It is built locally, not published.

## Verified

- `cargo test --workspace`: 984 passed, 4 existing ignored tests.
- All checks from the original handoff passed: shell pages, onboarding, four players, Online, music, update notices, Publisher, and the full browser suite (including `review-finish.mjs`).
- Added `Tools/check-launch-polish.mjs`: stats search/filter/paging, stable search focus, dialog close/focus restoration (Escape and controller B), missing data, full seven-player teams and long names at 800x600, controller scrolling inside the dialog without moving history, sticky Close, profile visibility and sync, conditional B/X correction, controller navigation while playing, test exit, live Audio controls, and fullscreen checkbox.
- Rebuilt Dolphin with `Tools/verify-reborn-windows-build.ps1 -Build`; Windows build and source-list checks passed.
- `Tools/test-live-settings.ps1` verified volume, mute, and four-player bindings with the game actually running, offline and in a local NetPlay host session. NetPlay reported `memory_cards_off: true`. These were isolated test User folders, no personal saves, and an existing read-only ISO. No two-person network match was claimed or tested.
- Current Stats screenshots: `target\browser-review\stats-*.png`. Other polish screenshots: `target\browser-review\polish-*.png`.
- Stats follow-up verification: `target\stats-browser-check.log`, `target\stats-shell-check.log`, and `target\stats-full-browser-check.log` all pass. `review-finish.mjs` now postpones automatic update notices in its fixture, keeping unrelated settings focus checks isolated; update notice behavior has its own dedicated suite.
- Shared update-notice checks also pass (`target\stats-update-notice-check.log`). Final build log: `target\stats-launcher-build-final.log`; earlier `stats-launcher-build.log` has a superseded installer file-lock failure. The final build successfully includes every Stats source change.
- Logs: `target\polish-*.log` and `target\dolphin-polish-build.log`. An earlier standalone `polish-review-finish.log` contains a superseded failure; the final full browser suite runs that same review and passes.
- Runtime staging log: `target\polish-runtime-stage.log`. The full installer payload was preserved. The extra source snapshot at `target\polish-runtime-source` remains because automatic approval review rejected its cleanup command without a specific reason. Temporary full-size soundtrack images were removed successfully by the verification tool.

## Default soundtrack patch is staged; publication is pending

The beta's actual `library\offline\NSR.iso` was inspected read-only. Its first songs are still **Scarred**, **Breathe, Stretch, Shake**, **Ground Zero**, **Put it in the Air**, and **I am Rock**. There is no Music-page setting that can truthfully label that disc as already containing the new soundtrack.

The new local chain is at `target\polish-content\game-patch`. It preserves the existing 2.1-to-NSRV3 step and appends NSRV3 to a build with all 21 songs from Adam's `NFLStreetReborn-fixed.nsrmusic` baked in. Once that game patch lands, those are the disc's own songs with custom music off. No soundtrack mod toggle is required.

- Source: `98bfe6bfce1a31377d53fa361bb93c917791167ebdc21a0e1d086cc1f8f6398b` (the current public NSRV3 chain head).
- Result: `d82b206a796b9308dbd3dfb1a86e70f5f1c5912a0ec747c14198bac462b3c9e9`.
- New patch: 105,076,599 bytes, SHA-256 `eae16f8c27e70a99394d5bcdde460446533d4fb7a1ffa3174e53fea0003a19c1`.
- Every audio blob was exported back out of the built disc and compared byte for byte with the pack. All 21 titles, artists and albums match. Only `pathfind.dat` and the song banners in `template.dat` changed; all other FST files, the main executable and non-music template data matched the source. The delta decoded to the exact target hash. Temporary full-size ISOs were removed.
- Reproducible local builder: `Tools\prepare-default-soundtrack.py`; report: `target\polish-content\game-patch\verification.json`; log: `target\polish-soundtrack-content.log`.

This patch deliberately preserves the public game executable. It does not absorb the separate, still-changing FIELDS development disc or the unshipped crash fix mentioned in the original handoff. Reconcile that separate game-code release before selecting the final public chain head. The soundtrack work here is ready to review and publish; it has not been uploaded, signed or pinned into the public release.

The launcher, runtime and soundtrack content must be released together. The local launcher installer alone cannot update the public runtime pin or put the new songs into an old game image.

## Build command

```powershell
./installer/windows/build.ps1 -LauncherOnly -Channel beta -Version 3.0.1-beta.28 -Builder 'C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\target\merge-tree\target\installer-payload\tools\reborn-builder-cli.exe'
```

Keep launcher and runtime publishes coordinated. The public runtime pin otherwise restores the older runtime; this local launcher installer alone cannot provide live settings in that older process.
