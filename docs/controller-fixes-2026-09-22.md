# Controller fixes, September 22, 2026

The [verification follow-up](controller-validation-2026-09-22.md) supersedes the initial test/build results below and records additional launcher and runtime fixes.

Implementation is on `fix/controller-navigation` in this worktree. It includes release/3.0.23 plus the existing crash-detail commit, then the controller changes below. Existing worktrees and their uncommitted edits were not overwritten.

## Confirmed defects and fixes

| Defect | Change |
| --- | --- |
| Every DirectInput device was classified as keyboard/mouse. USB controllers available only there disappeared. | Filter actual keyboard/mouse names, retain DirectInput controllers for the existing runtime capture/remap workflow. |
| Any named backend caused all SDL controllers to be removed, including a separate USB controller. | Prefer XInput and WGInput, deduplicate known aliases per device/model, retain unique SDL devices and additional same-model controllers. |
| Raw hat-switch D-pads were treated as unknown even though their direction names are explicit. | Recognize `Hat 0 N/S/W/E` for automatic game D-pad mapping. Numeric button and stick indices still require measurement. |
| Every OR binding was treated as an automatic placeholder. A working custom stick-plus-D-pad binding could be replaced on launch; OR expressions ending in a positive axis were also misclassified as a single missing input. | Resolve OR control names individually and narrow only the launcher's own default vocabulary. Preserve working custom OR expressions and inverted/raw-axis assignments. Repair trigger/rumble extras independently. |
| The launcher always polled the first standard controller. An idle first controller blocked a second controller. Nonstandard browser layouts were silently ignored. | One window input reader accepts activity from any supported pad. Raw layouts have a local, saved calibration flow for directions, select, back, and optional page buttons. |
| A first direction could both establish focus and move past the primary control. Diagonal stick noise could repeatedly switch direction and restart the initial repeat. | First input reveals focus; direction hysteresis and a single repeat timer prevent rapid alternating steps. D-pad wins over stick drift. |
| Dialog navigation searched only inside the main page, although dialogs can be attached to the document body. | Focus recovery and movement are scoped to the open dialog. |
| The launcher input loop continued accepting pad input while a game session was active. | Pause launcher navigation during gameplay; require neutral controls after returning, losing focus, testing, reconnecting, or a sampling stall. |

The raw launcher calibration is distinct from game remapping because browser indices are not Dolphin input names. Standard browser mappings work without this extra setup. This follows the [Gamepad specification's distinction between standard and unknown layouts](https://w3c.github.io/gamepad/#remapping). Calibration measures polarity and hat neutral values rather than assuming Xbox indices for every USB device. The existing game remapper continues to capture inputs through Dolphin itself.

## Verification

- `cargo test --workspace --target-dir ../controller-build --quiet`: 1,207 passed, 4 existing ignored.
- `node Tools/test-menu-pad.mjs`: nine deterministic input tests, including repeat timing, diagonal jitter, simultaneous virtual copies, a second controller, reconnects/stalls, unused axes, and raw hats.
- `node Tools/check-controller-navigation.mjs`: Edge tests for first focus, adjacent D-pad movement, focus recovery in an external dialog, raw hat/stick calibration, reversed-axis normalization, and persistence across reload.
  Also checks the real Controllers dialog at 800px and 1280px and verifies standard controllers do not receive a setup prompt on Home.
- `node Tools/check-controller-press-to-map.mjs`: existing runtime capture/remap tests passed.
- `node Tools/run-browser-tests.mjs`: full launcher flow, four-player controls, responsive layouts, settings, NetPlay forms, and session-state suites passed. The new input suites are now also included by this entry point.
- `git diff --check`: passed.

Two old browser fixtures needed updates: the error path already opens the typed-binding disclosure, so clicking it again closed it; the four-player test expected browser presses to perform game remapping although release/3.0.23 already uses Dolphin capture. The first failure was reproduced on the unchanged release worktree. The four-player fixture now returns a runtime binding for the explicitly selected device and still verifies other controllers cannot save it.

## In-game investigation and remaining release checks

Read-only inspection of the current `GameCube Version/root` files found:

- All six main-menu label positions match the reference main menu.
- D-pad control descriptors 1 through 4 are `8003`, `8002`, `8000`, `8001` (up/down/left/right).
- Left-stick descriptors 23/24 remain `c000`/`c201`.

This does not reproduce the reported reversal in a running game. No speculative game executable or menu-data patch was applied, and no NetPlay-affecting disc changes were introduced.

The local runtime probe did not produce an input dump, so it is not hardware verification. The browser tests use simulated controller samples. Before a public release, verify Quick Game, Game Modes, submenu movement, team selection, and an actual match on affected users' hardware with their installed runtime:

- Xbox, DualShock 4, and DualSense over USB and Bluetooth where available.
- The reported USB GameCube controller, plus native GameCube adapters in their actual PC/Wii U modes.
- A controller connected before launch, after launch, and after disconnect/reconnect.
- Mixed controller types and two identical devices; confirm one pad does not operate two players.
- Steam Input/DS4Windows enabled and disabled, where those tools are part of the affected setup.

Backend deduplication still has only device names and backend indices to work with, not an OS physical-device identifier. Unknown raw game controllers can require the existing one-time press-to-map flow. A driver that does not expose a controller, or an older runtime without capture support, cannot be made universally plug-and-play by the launcher alone.

This is a local test candidate. It has not been published or installed, and the affected-PC in-game reports are not yet certified resolved.

## Local Windows test build

`cargo build -p reborn-ui --release --target-dir ../controller-build --quiet` completed successfully. The compiler reported one existing unused import (`UNVERIFIED_GAME` in `main.rs`).

The initial candidate was 54,160,384 bytes, SHA-256 `1941210e0f371d0a0d1f0999b34218114ded3e59ec21c4fa3d24a4c074f43721`. It has been replaced at `dist/controller-test/reborn-launcher.exe` by the corrected build documented in the verification follow-up.

This is the launcher executable, not an installer or a new Dolphin runtime. It retains the branch's existing 3.1.0 version metadata. Use it with a test installation; launching it can use and update the current user's launcher state. The build has not been run against the production installation.
