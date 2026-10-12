# macOS port — handoff for the Claude Code session on Adam's MacBook

Written 2026-10-08 by the Windows-side Claude session. Read all of this before
touching anything. Adam asked for a macOS build of NFL Street Reborn with
**all the same features** as Windows and the Steam Deck build.

## Decisions already made by Adam (do not re-ask)

- **Apple Silicon only** (arm64, M1+). No Intel / universal build.
- **Unsigned.** No Apple Developer account. Ad-hoc sign (`codesign --force --deep --sign -`).
  Players do a one-time "Open Anyway" (System Settings > Privacy & Security; on
  macOS 15+ right-click > Open no longer bypasses Gatekeeper).
- **Cross-platform netplay wanted**: Windows, Deck and Mac players in the same game.
  So the Mac emulator MUST be the same fork revision as the shipping Windows
  runtime (currently **1.2.12**). Netplay refuses mismatched versions.
- **Build on this MacBook**, Command Line Tools only (NOT full Xcode). The disk had
  48 GB free on 10-08; the build plus a texture test needs about 21 GB. Delete build
  dirs you no longer need and check `df -h /` before big steps.
- **Releases are published from the Windows PC, never from the Mac.** Signing keys
  and upload config live only on the PC. Your job ends at a tested `.dmg` and a staged
  `runtime-macos` folder that Adam copies to the PC.

## Repos and branches

| What | Where | Branch to start from |
|---|---|---|
| Launcher (Rust + Tauri 2) | `github.com/adampreddy/nfl-street-reborn-launcher` (private) | **`port/macos`** (this branch; = Deck port `port/linux-deck-on-3.0.35` @6b48d3c + this file) |
| Dolphin fork | `github.com/adampreddy/reborn-dolphin` (private) | **`port/linux-1.2.12`** @7797cfc498 (= shipped 1.2.12 + Linux build fixes) |

Make your own branches: launcher `port/macos` (commit onto it), fork `port/macos-1.2.12`
off `port/linux-1.2.12`. Push both often so the PC can see them.

Setup: `xcode-select --install`, Homebrew, `brew install gh rustup cmake ninja pkg-config qt@6 ffmpeg libpng zstd lzo node xdelta`,
`gh auth login`, `rustup-init` (stable, aarch64-apple-darwin), `cargo install tauri-cli --version 2.12.1 --locked`.
Clone the fork with `--recurse-submodules`.

## What already exists (from the Deck port) — reuse, don't rewrite

The Deck port did the hard cross-platform work; most of it is `cfg(unix)` or has a
`target_os = "macos"` arm already. Read these first:

- `docs/superpowers/specs/2026-09-22-linux-steam-deck-port-design.md` — the port design.
- `installer/linux/*.sh` — how the Linux release is built, bundled, e2e-tested and shipped
  (`build-release.sh`, `build-appimage.sh`, `release-deck.sh`, `e2e_first_run.py`). Mirror them as `installer/macos/*.sh`.
- `crates/reborn-core/src/platform/{mod.rs,imp.rs,runtime_process_unix.rs}` — paths, Dolphin-running check, open_url.
- `crates/reborn-core/src/update.rs` — `latest.json` already has a `macos` asset slot; `apply` has
  Windows (Inno) and Linux (AppImage swap) paths; **macOS apply is missing**.
- `crates/reborn-core/src/content/pipeline.rs` — `ComponentId::RuntimeMacos` ("runtime-macos")
  exists; `ComponentId::runtime_here()` and `pipeline::updatable()` are per OS.
- `crates/reborn-core/src/dolphin_preferences.rs` — per-OS `VIDEO_BACKENDS`/`AUDIO_BACKENDS`
  + `unusable_here` (template says D3D; Linux swaps to Vulkan). **Mac must use Metal.**
- `crates/reborn-ui/src/content_updates.rs` — `ensure_import_bundle`, `ensure_patch_chain`,
  `ensure_runtime`, `runtime_now` (pages must never wait on a download).
- `crates/reborn-core/src/controller*` + `tests/linux_pads.rs` — Linux pad name maps.
- Fork `.github/workflows/reborn-crossplatform.yml` — a working **macOS arm64 CMake recipe**
  (Metal, MoltenVK/SDL2 off, `-DCMAKE_POLICY_VERSION_MINIMUM=3.5`, do NOT brew fmt/pugixml,
  ad-hoc codesign of DolphinQt.app). It compiled on CI but was never run.

## Work list (in this order)

1. **Emulator.** Build fork `port/macos-1.2.12` for arm64 with Metal. Fix whatever breaks
   (Linux needed: `<optional>` in State.h, dolphin-tool link group, Languages `.mo` beside exe).
   Ship the **Sys / Languages / User from the shipped Windows runtime, not the fork's Data/Sys**
   (they differ in 1,650 files; `Sys/GameSettings/GN7.ini` holds settings that override Dolphin.ini).
   They are in `installer/macos/runtime-data-1.2.12.tar.gz` (2,238 files, identical to the
   Windows and Linux 1.2.12 runtimes). In a `.app`, the Sys folder goes in `Contents/Resources/Sys`.
   Check: the window title says "NFL Street Reborn Dolphin" + Metal, and the version/netplay
   string matches Windows 1.2.12.
2. **Boot proof.** Get the game: run the launcher's first run and choose "Download the Reborn game"
   (the `game-full` component), or import a retail USA disc. `dolphin -u <User> -e NSR.iso` must
   reach Press START. Then with the HD texture pack (textures component, ~8 GB) it must show the
   Reborn title art, with no MISSING TEXTURES overlay.
3. **Launcher on macOS.** `cargo test --workspace` green (gate any Windows-only tests with cfg
   as the Deck port did). Then:
   - Data root `~/Library/Application Support/NFL Street Reborn`; install root = the `.app`'s
     `Contents/Resources` (read-only once installed; treat it like the AppImage's `$APPDIR`).
   - **Bundle everything** in the `.app` like the Deck AppImage does: the launcher, `rebornctl`,
     the emulator (`Contents/Resources/runtime`), DolphinTool (an **upstream** dolphin-tool with
     `header -j` and `extract`; our fork's dolphin-tool lacks them — the Deck built upstream master
     dolphin-tool only, ENABLE_QT=OFF), `xdelta3`, and a macOS `reborn-builder-cli` (roster). The Builder is pure
     Python frozen with PyInstaller; its source is in `installer/macos/reborn-builder-src.tar.gz`
     (= `nflstreet2_clean/tools/reborn_builder` on the PC, 10-08). See step 2b of
     `installer/linux/build-release.sh` for how Linux freezes it.
     Bundled Homebrew dylibs must be copied in and re-pointed (`dylibbundler` or
     `install_name_tool`) — test on a path with no Homebrew.
   - The first content check adopts the bundled emulator (see 7e91a3e for Linux).
   - **Self-update** (`update::apply` for macOS): download the new `.app` (zip or dmg), verify sha256,
     swap the bundle beside the running one, relaunch. Files our own app downloads are NOT
     quarantined, but strip `com.apple.quarantine` anyway and re-run ad-hoc codesign if needed.
   - Settings: Metal only, no ReShade, no D3D/Vulkan choices; reuse the `choicesFor`/`platformNote`
     per-OS settings code.
   - Controllers: macOS Dolphin uses GCController/IOKit/SDL names — add a macOS name map like
     `LinuxPad`, with tests. Xbox, PS5 and Switch Pro pads should work. Untested hardware is fine,
     but say so.
   - Close-Dolphin guard, open_url (`open`), reveal-in-Finder (`open -R`), `.exe` assumptions.
4. **Package.** `installer/macos/build-release.sh`: build Dolphin + launcher, assemble the `.app`
   (`cargo tauri build --bundles app`, then add runtime/tools), ad-hoc sign the whole bundle,
   make the `.dmg` (`hdiutil create`), write BUILD-INFO.txt (launcher commit, fork commit, sha256,
   bytes). **REBORN_BUILD_VERSION must equal the live launcher version** (now 3.0.36; check
   `curl -s https://files.nflstreetreborn.com/launcher/latest.json`). An older stamp loops on updates.
   Also stage `runtime-macos-1.2.12/` (the emulator files exactly as the content component installs them).
5. **End-to-end test on a clean data root.** Make it a script like `installer/linux/e2e_first_run.py`:
   welcome -> game -> download screen (8 GB) -> all Installed -> Play -> game boots. Then every
   launcher page opens. Online: a Mac-vs-Windows netplay game with Adam (cross-play).
6. **Hand back to the PC:** push both branches, then give Adam the `.dmg`, `runtime-macos-1.2.12/`,
   BUILD-INFO.txt, and a short test report. The PC session publishes `runtime-macos` (a content
   release seq), adds the `macos` entry to `latest.json`, and merges `port/macos` into the release line.
   The live 3.0.36 is newer than this branch (3.0.35 + Deck work); merging is the PC's job, not yours.

## Laws learned the hard way (follow them)

- Anything the Windows installer ships beside the launcher (game-patch chain, game-import bundle,
  runtime, textures seed) is ABSENT on a one-file install. Every "seed from install root" path needs
  a bundled copy or a content-download fallback. The Deck hit this four times
  ("not a Reborn build", "No Reborn emulator was found", "Loading…" stuck, stale staged file).
- Pages must never block on a download (`runtime_now`, not `ensure_runtime`, for page lookups).
- A staged file is re-hashed before it is trusted (fixed 2b89318; keep it that way).
- Never SIGKILL Dolphin in its first seconds: it leaves a 0-byte memory card that the game then
  rejects ("Wrong device in Slot A"). If that happens, delete `User/GC/MemoryCardA.USA.raw`.
- Mac must stay on **Metal**: OpenGL skips every BC7 DDS in the HD pack.
- Dolphin settings have THREE layers; `Sys/GameSettings/GN7.ini` (GlobalGame) beats Dolphin.ini/GFX.ini.
- Keep `.rs` and `.sh` files LF (see `.gitattributes`); CRLF breaks source-scanning tests.
- When probing for a running process, match on `-e`/`.iso` and check it's still alive after 45 s
  (the controller scan runs `dolphin-emu --dump-device-inputs`, which caused a false "it works").
- Chain build/check/publish steps with `&&`. A failed build once printed a stale "pass".
- Don't claim something works until you've seen it run. Say what's untested.
- Commit messages end with: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
