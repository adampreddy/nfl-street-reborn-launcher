# Linux (Steam Deck) and macOS port: design

Adam, 2026-09-22: "Scope out porting the launcher and everything NSR related to Linux for Steam Decks and MacOS", then: Deck first, cross-platform play wanted, no ReShade on the Deck, a MacBook (Apple Silicon) is available, no Steam Deck, no dual boot.

Branch `port/linux-deck`, from `release/3.0.23` (the newest shipped launcher).

## Principles

1. **Windows players see no change.** Every change is either off-Windows only or a no-op on Windows. The Windows test suite stays green.
2. **One product, per-OS runtime.** Textures, mods, roster, game-full and game-patch are shared by every OS. Only the Dolphin runtime and its helper tools (`DolphinTool`, `xdelta3`, `reborn-builder-cli`) differ per OS.
3. **Cross-platform play means one fork revision everywhere.** A Windows, Deck and Mac player in the same match must run the same `reborn-main` commit. The release has to ship all three runtimes from one commit.
4. **No ReShade off Windows.** The launcher neither offers nor stages it there. The fork's UI split is already inert without it (`NSR-UISPLIT-INERT-WITHOUT-RESHADE`).

## Steps

| Step | What | Needs a Deck? |
|---|---|---|
| 1 | The launcher runs correctly on Linux: process check, settings, tool names, links, update refusal, UI text | No |
| 2 | Linux Dolphin runtime: the fork's CI build, plus Linux `DolphinTool`, `xdelta3`, `reborn-builder-cli`, and a Linux user template (Vulkan, Cubeb) | No |
| 3 | Per-OS release: a platform field on the runtime component; Windows clients unchanged | No |
| 4 | Flatpak packaging, updates through Flatpak, `nflstreetreborn://` through the `.desktop` file; runtime and content under the data root, since `/app` is read only | No |
| 5 | On a real Deck: frame rate, a Deck graphics preset, Steam Input pad names, Game Mode, SD card (exFAT: no hardlinks, copy fallback), netplay against Windows | **Yes** |
| 6 | macOS: Metal-only runtime, `.app` signing and notarization, deep links, self-update | MacBook |

## Step 1 detail

- **Dolphin running check.** `runtime::runtime_is_running` returned `Ok(false)` off Windows, so "close Dolphin before changing settings" protected nothing. Linux reads `/proc/<pid>/exe`; other Unix asks `ps`. The same rule applies as on Windows: only this exact executable counts, and an unreadable process is not proof of anything.
- **Settings edits.** `dolphin_settings::require_closed` refused every edit off Windows. It now uses the check above.
- **Graphics and audio choices per OS.** Windows: D3D, D3D12, Vulkan, OpenGL / Cubeb, WASAPI. Linux: Vulkan (default), OpenGL / Cubeb. macOS: Metal / Cubeb. The seeded default follows the OS. At launch, a value this OS cannot run (D3D on Linux, from a template built on Windows) is replaced with the OS default. That is a launcher-written value, so replacing it breaks no player choice. The page gets its choices from the command, not from a list in `settings.js`.
- **Helper tool names.** `DolphinTool` and `xdelta3` use the platform's executable suffix. Dolphin discovery accepts the Linux `dolphin-emu` and never mistakes `dolphin-tool` for the emulator.
- **Links and uninstall.** Websites open through `platform::imp::open_url` (`xdg-open`/`open`) instead of `rundll32.exe`. Uninstall off Windows says how to remove the package.
- **Self-update.** `update::apply` runs an Inno Setup installer. Off Windows it refuses with a sentence until step 4 gives it a package route.
- **UI text and paths.** No `"\\MODS"` path joins and no "Windows" in text shown off Windows.

## Verification for step 1

- Windows: `cargo test --workspace` and `cargo clippy --workspace --all-targets` are no worse than the base commit.
- Linux: `cargo zigbuild`/`cargo clippy --target x86_64-unknown-linux-gnu` for `reborn-core` and `reborn-cli`, clean apart from the base warnings. Unix-only tests are written but **cannot run on this PC**: there is no WSL and no Linux machine. `reborn-ui` for Linux needs a GTK/WebKit sysroot and is not checked here.
- A real Linux run of the tests is the open item. WSL2 on this PC (admin plus a restart) or a Linux CI job would close it.

### Observed 2026-09-22 (step 1 commit)

- Windows `cargo test --workspace`: exit 0, including 1,016 `reborn-core` tests.
- Windows `cargo clippy --workspace --all-targets`: 0 warnings on any changed line (55 warnings in total, all on untouched code).
- Linux `cargo clippy -p reborn-core -p reborn-cli --target x86_64-unknown-linux-gnu --all-targets` through zig: 0 warnings in changed code. That type-checks the Linux-only process test; it did not run it.
- `Tools/check-shell-pages.mjs` has 5 new checks: Linux choices, no Direct3D, ReShade or Visual effects on Linux, Windows lists unchanged. PASS. Against the old `settings.js`, the 3 Linux checks FAIL and the Windows one passes. `check-harness-args` and `check-page-call-args` PASS.
- Not done: running any test on Linux; building `reborn-ui` for Linux; controller wording ("reserved by Windows") and Linux pad names wait for step 5.
