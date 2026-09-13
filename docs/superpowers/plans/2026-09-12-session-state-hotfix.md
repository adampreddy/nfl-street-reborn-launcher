# Stable session-state hotfix

The launcher is now on the stable channel. Adam's installed launcher is at
`C:\Users\Adam\AppData\Roaming\NFL Street Reborn\app\reborn-launcher.exe`.
The earlier beta handoffs are historical. Build this correction as stable 3.0.1
from the integration worktree (`target\merge-tree`). Do not publish a beta build.

## What was wrong

- The exclusive launch guard also covers update adoption, soundtrack rebuilds,
  imports, and settings writes. `session_active` returned that same flag, so an
  update could be reported as a running Dolphin session.
- Play sampled session state alongside slow initial page reads. The shell only
  dispatched changes in its own sampled value. A transient `true` read by Play,
  missed by the shell, could therefore remain on screen indefinitely. The
  regression test reproduces this against commit `0cda5c3`.
- Play's launch command released its guard after Dolphin exited, but still waited
  for result uploads before resolving. The pending frontend action kept showing
  Session active during that potentially long network wait.
- Runtime and settings checks used two different shell probes, with inconsistent
  filename/path matching. A failed PowerShell check was reported as proof Dolphin
  was running. No Dolphin process was running on this PC during initial diagnosis.

## Correction

- Game-session state is separate from exclusive write ownership. Play, opening
  Dolphin settings, and NetPlay acquire a session guard. Updates and settings
  retain exclusion without claiming a game is running. Conflicting writes still
  cannot run together and get an accurate applying-changes message.
- The shell sends authoritative session snapshots. Play no longer lets a late
  grouped read overwrite the current state. Repeated idle snapshots do not
  rebuild the page or rescan devices.
- Settings refreshes availability while open, including for Dolphin launched
  externally. Controls unlock automatically after exit without replacing focused
  controls on unchanged polls or requiring Refresh settings.
- Play returns after local cleanup and queues uploads on a background thread.
  Captures remain durable; collection failures are preserved in the return note.
  The note does not claim delivery before the upload answers. Sync status remains
  available on Stats.
- All Windows runtime checks now use a fresh `sysinfo` process snapshot with
  executable paths only. Exact runtime paths are checked, including canonical
  paths; another installation with the same filename is not this runtime.
  An unreadable matching process produces an inspection error, not a false
  Close Dolphin message. No PowerShell or tasklist process is spawned.

## Verification

- `cargo test --workspace`: 986 passed, 4 existing ignored tests.
- Native Windows test starts a small test child, verifies the exact executable
  is protected, confirms a different install with the same name remains idle,
  then terminates/reaps the child and verifies the runtime is immediately idle.
- Session guard test covers real-session exclusion, operations that are not games,
  conflicting writers, and release after completion/failure.
- `Tools/check-session-state.mjs --before` fails on the stuck Play state in
  `0cda5c3`; the current version passes. It covers late snapshots, slow page load,
  actual active/idle transitions, inexpensive idle polling, Online unlocking,
  and Settings unlocking after external Dolphin exits while keeping focus.
  The regression is also included in `Tools/run-browser-tests.mjs`.
- Full browser suite, shell-page checks, and update-notice suite passed.
- Logs: `target\session-state-before.log`, `session-state-after.log`,
  `session-fix-rust-tests.log`, `session-fix-browser-tests.log`,
  `session-fix-shell-tests.log`, and `session-fix-update-tests.log`.

## Build

```powershell
./installer/windows/build.ps1 -LauncherOnly -Channel stable -Version 3.0.1 -Builder 'C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\target\merge-tree\target\installer-payload\tools\reborn-builder-cli.exe'
```

Build log: `target\session-fix-stable-build.log`. This is a launcher-only hotfix;
no new Dolphin build or content release is needed for the session-state fix.
Building locally does not publish the installer or modify an installed launcher.

Final build completed successfully, including the last Settings refresh fix.
The installer retried one temporary file lock and succeeded. Verified output:

- `dist\NFL Street Reborn Launcher Update 3.0.1.exe`
- 64,676,528 bytes
- SHA-256: `243a3e96b0bd701d4a91bf9260c8617bbe96188e6f6bb42e0dec3fae42549a1c`
- `dist\latest.json` matches version 3.0.1, stable channel, size and hash, and now contains hotfix release notes.
- Staged launcher matches `target\release\reborn-launcher.exe` by hash; no beta channel marker is present.

At Adam's request, the shared Claude handoff is at
`C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\CODEX_TO_CLAUDE_HANDOFF.md`.
Both checkouts' `CLAUDE_START_HERE.md` files link to it. Claude is in a terminal;
direct terminal messaging was not performed and receipt has not been confirmed.
