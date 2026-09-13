# Stable 3.0.2: startup process inspection

## Report and limits of the diagnosis

Adam reports that **other users' PCs**, running published stable **3.0.1**, display
"Windows could not check whether Reborn Dolphin is running. Try again." immediately
after opening the launcher. This was not reproduced on Adam's PC. No affected-PC
diagnostic log is available, so the precise Windows error on those PCs is unconfirmed.

The 3.0.1 source emits that message for an empty sysinfo snapshot or a matching
process whose executable could not be read. Its sysinfo 0.38.4 Windows implementation
opens every process with QUERY_INFORMATION + VM_READ first, falls back to limited
rights, and uses GetModuleFileNameExW to obtain executable paths. Those permissions
and an unavailable snapshot must not be assumed to behave identically on every PC.

## Change

`crates/reborn-core/src/platform/runtime_process.rs` now owns Windows detection:

- Enumerate process IDs/names with Toolhelp without opening every process.
- Request only PROCESS_QUERY_LIMITED_INFORMATION, and only for matching executable
  names. Read the full image path with QueryFullProcessImageNameW. This API explicitly
  supports limited-query handles, per [Microsoft's documentation](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew).
- Compare the actual full path with the configured/canonical runtime path. Same-name
  processes from another install do not block this one.
- An exited PID between enumeration and query is no longer a running candidate.
- If native enumeration fails, use Windows tasklist as an independent fallback.
  It runs hidden, has a five-second deadline, drains its output while waiting, and
  reads CSV image names/PIDs without relying on localized headings or no-match text.
- A valid process list with no matching candidate establishes idle. Empty, malformed,
  failed or timed-out fallback output does not. An actual matching process that remains
  unreadable is still reported as an inspection failure, not silently treated as idle.
- OS details are logged locally on failures. No new uploads or telemetry were added.

The core runtime entry point uses this detector. Existing UI/session guards still
protect actual launcher-owned sessions and serialize writes. The earlier stale UI,
background result-upload and Settings refresh fixes are retained. sysinfo is again
disk-only; the new Win32 calls have scoped unsafe blocks with buffer/handle safety
comments and RAII handle cleanup.

## Verification

- `cargo test --workspace`: **992 passed, 0 failed, 4 existing ignored**.
- Injected snapshot error and empty-list tests prove startup can recover via an
  independent valid listing without querying unrelated protected processes.
- Tests cover an active target through the fallback, an exited target, another
  installation, canonical path spelling, unreadable matching candidates, and malformed
  or unavailable fallback results.
- Real Windows tests use tasklist and limited-query handles to find the test process,
  and launch/reap a child to prove active protection and immediate exit recovery.
- `node Tools/check-session-state.mjs`: passed, covering Play/Online/Settings recovery.
- `git diff --check`: passed.
- Logs: `target/startup-process-check-workspace-tests.log`,
  `target/process-check-tests.log`, `target/startup-process-check-browser-tests.log`.

These are local and injected-failure checks. Verification on an affected user's PC
is still required before claiming the reported cross-machine issue is confirmed fixed.

## Build

Source: `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\target\merge-tree`,
integration worktree based on `92804e4`. Claude's 3.0.1 texture-prefetch correction is retained.

```powershell
./installer/windows/build.ps1 -LauncherOnly -Channel stable -Version 3.0.2 -Builder 'C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\target\merge-tree\target\installer-payload\tools\reborn-builder-cli.exe'
```

Build completed successfully. Log: `target/startup-process-check-stable-build.log`.

- Installer: `dist/NFL Street Reborn Launcher Update 3.0.2.exe`
- Size: **64,667,277 bytes**
- SHA-256: `238a4522ab999076061d784b34e76f2c3f00d76ebfaa38e4a4f59c8f50cea7e0`
- `dist/latest.json` verified against the installer: stable, version 3.0.2, matching
  size/hash, release notes included.
- Staged launcher hash matches the release executable; no beta marker is present.

No publication, installation, runtime replacement, or save migration has been performed.
The next validation is to install this launcher-only update on an affected PC and
confirm that startup, Play, and Settings work with Dolphin closed, then verify that
actual-running protection and exit recovery still work there.
