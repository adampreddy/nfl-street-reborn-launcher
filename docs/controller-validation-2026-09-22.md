# Controller verification follow-up — September 22, 2026

## Result

The expanded software checks pass. The review found and fixed additional defects that the first regression suite did not cover. Both the launcher and Dolphin test binaries were rebuilt successfully. Physical controller directions, the in-game Quick Game/Game Modes menus, and a complete match remain unverified: the live runtime reported only `DInput/0/Keyboard Mouse`, with no active game controller. Windows listed an Xbox wireless adapter, which is not evidence of a connected pad.

This is a local test candidate, not a published or installed release. It is not yet sufficient evidence to certify every reported in-game failure resolved.

## Additional defects reproduced and fixed

1. **Calibration could lose the Select button.** Starting setup while a button was held recorded it as neutral, so it could never be captured later. The new browser test failed at completion before the fix and passes after setup waits for released buttons before taking its baseline.
2. **A generic USB pad named Gamepad disappeared beside XInput.** Two new Rust regressions reproduced this and the removal of an extra Xbox-named SDL controller. The alias logic now distinguishes the generic USB name from XInput's fixed label and limits the special duplicate-WGInput rule to WGInput.
3. **The runtime still hid every SDL device.** Launcher filtering alone could not fix this. The Dolphin fork now honors a child-process `REBORN_RAW_CONTROLLERS=1` opt-in for list, dump, and capture. The launcher sends it on all three paths. Old launchers retain the original filter; older runtimes ignore the environment variable without rejecting their existing command-line flags. The new runtime is required to remove this runtime-side restriction.

## Checks and evidence

| Check | Result |
| --- | --- |
| Rust workspace regression suite | 1,210 passed; four pre-existing ignored tests were not run |
| Real launcher-to-runtime integration | Passed separately using the new Dolphin executable: list/dump/capture agree, a missing device returns no input, and the opt-in does not change the parent environment |
| Deterministic browser input tests | 12 passed, including noisy diagonal holds and individual taps at 30/60/120/144 Hz, duplicate controllers, staggered Select releases, and reconnects |
| Browser integration | Passed: focus entry, adjacent D-pad movement, dialog isolation, held-button calibration, inverted raw axes, hats, saved calibration, focus loss, busy state, and game-session return requiring neutral |
| Full browser regression suite | Passed: launcher flow, four-player configuration, settings, NetPlay forms, responsive pages, and session recovery |
| Existing press-to-map suite | Passed: numbered USB inputs and PlayStation runtime capture |
| Native filter predicate | Compiled and passed against the production C++ function: legacy behavior, raw opt-in, generic USB/Sony/Xbox names, and invalid opt-in values |
| Existing and rebuilt Dolphin process probes | Both passed: legacy list, opted-in list/dump agreement, capture startup, missing-device timeout, and clean shutdown |
| Windows release builds | Launcher and Dolphin succeeded; Dolphin's repository build-verification script also passed |
| Diff checks | Passed in both worktrees |

The real-runtime integration test is opt-in because it needs a local executable. It is normally ignored by the workspace command but was explicitly executed successfully in this verification.

The live probes validate executable startup and the controller-query protocol. They do not simulate physical hardware and did not boot the game. The launcher browser checks use Edge with a simulated native bridge; they are not a manual test of the packaged launcher window.

## Reproduce

From the launcher worktree:

```powershell
cargo test --workspace --target-dir ../controller-build --quiet
node Tools/run-browser-tests.mjs
node Tools/check-controller-press-to-map.mjs
./Tools/check-controller-runtime.ps1 -Runtime '../dolphin-controller-navigation/Binary/x64/NFL Street Reborn Dolphin.exe'
$env:REBORN_TEST_RUNTIME = (Resolve-Path '../dolphin-controller-navigation/Binary/x64/NFL Street Reborn Dolphin.exe').Path
cargo test -p reborn-core --test controller_navigation real_runtime_list_dump_and_capture_agree --target-dir ../controller-build -- --ignored --nocapture
```

From the Dolphin worktree:

```powershell
./Tools/test-controller-filter.ps1
./Tools/verify-reborn-windows-build.ps1
```

Live probe evidence is under `target/runtime-controller-probe/622b04fd294848d8a6ad5ca6457bb942` in the launcher worktree. The existing-runtime comparison is under `e688c3ec1826400ea18fcff3a163d6cd` beside it. The native build log is `Build/controller-build.log` in the Dolphin worktree.

## Updated local candidates

- Launcher: `dist/controller-test/reborn-launcher.exe` in this worktree, 54,156,800 bytes. SHA-256: `4f060083075c15976788c36ae7afa42ec053c30d3cb9c52381ec77b2feaf8bc9`.
- Dolphin: `../dolphin-controller-navigation/Binary/x64/NFL Street Reborn Dolphin.exe`, 18,513,920 bytes. SHA-256: `8c31530f6a17bd035332863578766e53e803be603e3154b55a036d3720cba385`. Keep it with the DLLs and resources in that directory.
- Runtime source: branch `fix/controller-enumeration`, commit `a61abf9594`, based on native repository commit `8b515a14e5`. Existing uncommitted runtime changes were not included or overwritten.
- Launcher source: branch `fix/controller-navigation`; the original fix was `d72fc82`, followed by this verification's corrections.

The launcher candidate keeps the existing 3.1.0 version metadata. Neither candidate was installed over production. Game files were not changed. The older runtime's SDL filter remains in effect until the runtime update is distributed too.

## Remaining acceptance test

Connect the affected controller and check launcher navigation, in-game Quick Game and Game Modes, team selection, and an actual match. Repeat after reconnecting, with mixed controllers, and with the user's Steam Input/DS4Windows arrangement. Specifically test the reported PlayStation and USB GameCube devices. Name-based backend deduplication still cannot prove physical identity; this requires hardware validation.
