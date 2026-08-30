# Reborn Dolphin Integration

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers:** `CLAUDE_START_HERE.md` step 6, and items 1 through 8 of its runtime checklist
**Status:** proposal. `CLAUDE_START_HERE.md` item 8 requires Adam's review of a written migration plan and risk assessment before the fork is rebased or replaced. **This is that document.**

---

## 1. Build lineage: proven

**START_HERE checklist item 1.** SHA-256 of the selected runtime's `Dolphin.exe`:

```
617321bf37ac7b2f9bdfcd3af803a019e862591259c770e9e688d53834df1338
```

Byte-identical to `dolphin-exgl\Build\x64\Release\Dolphin\bin\Dolphin.exe`. Both 17,818,624 bytes, August 29, 2026. The runtime folder is a copy of the working tree's Release output. Lineage is not inferred; it is a hash match.

**For contrast**, the currently shipped `NFL Street Reborn Dolphin.exe` hashes to `8705a96b...`, is 14 MB, and links Qt5. It is a different, older binary from a different lineage. Replacing it is a runtime migration, not a file swap.

| Property | Shipped today | Selected foundation |
|---|---|---|
| SHA-256 | `8705a96b...` | `617321bf...` |
| Size | 14 MB | 17.8 MB |
| Qt | 5 | 6 |
| Upstream | unknown snapshot | `BlueSwordM/dolphin` at `7f4875e` |

---

## 2. Source inventory

**START_HERE checklist item 2.** Every NFL Street specific file and every modified upstream file, as of the preservation commit `eca379f`.

### NFL Street sources

| File | Size | Purpose |
|---|---|---|
| `Source/Core/Core/NflsPlayHide.cpp/.h` | 5 KB | Hides the opponent's play call. Resolves NetPlay port to in-game team |
| `Source/Core/Core/NflsStatTracker.cpp/.h` | 49 KB | Captures teams, score, team totals, and 12 categories for 14 players |
| `Source/Core/Core/NflsStatUpload.cpp/.h` | 17 KB | Posts JSON to the league ingest endpoint |
| `Source/Core/VideoCommon/NflsUiSplit.cpp/.h` | 28 KB | Experimental field and HUD render separation |

### Modified upstream files

```
Externals/mGBA/exports.props            Source/Core/DolphinQt/MainWindow.cpp
Installer/Dolphin.ico                   Source/Core/DolphinQt/RenderWidget.cpp
Source/Core/Common/Version.cpp          Source/Core/VideoCommon/RenderBase.cpp
Source/Core/Core/CMakeLists.txt         Source/Core/VideoCommon/VertexManagerBase.cpp
Source/Core/Core/PatchEngine.cpp        Source/Core/DolphinLib.props
Source/Core/DolphinQt/DolphinQt.vcxproj Source/VSProps/Base.Dolphin.props
```

**All of it was uncommitted before this project.** Roughly 98 KB of original C++ plus twelve integration points existed only as working-tree state on one machine. Now preserved on branch `nfls-reborn/preserve-2026-08-29`, commit `eca379f`, pushed to `adampreddy/reborn-dolphin` (private).

---

## 3. What the cross-platform build actually proved

**START_HERE checklist items 4 and 5**, answered with evidence rather than inspection. Ten CI rounds on real GitHub runners.

### Failure ledger

| # | Blocker | Cause | Whose |
|---|---|---|---|
| 1 | `libspng-dev`, `qt6-svg-dev` not found | wrong package names for the runner image | **my workflow bug** |
| 2 | `Externals/pugixml` `cmake_minimum_required` | CMake 4 dropped pre-3.5 compatibility | fork age |
| 3 | `IOFile.h:109` `-Werror=nonnull` | GCC 13+ stricter than the snapshot's compiler | fork age, upstream code |
| 4 | bundled mbedtls `-Werror=unused-but-set-variable` | Clang stricter than the snapshot's compiler | fork age, bundled external |
| 5 | `BuiltIn.h:23` `IP_PROTOCOL` | modern glibc defines it as a macro | fork age, upstream code |
| 6 | `MsgHandler.h` `fmt::detail::is_compile_string` | Homebrew fmt 11.x vs the bundled fmt API | fork age, dependency |
| 7 | **`NflsUiSplit::OnFrameEnd` undefined at link** | **never added to `VideoCommon/CMakeLists.txt`** | **Reborn** |
| 8 | `NetPlayIndex` undefined in nogui and tool | upstream link-order in two auxiliary targets | fork age, upstream |
| 9 | **`NflsUiSplit.h:30` `u32` not declared** | **header relies on the MSVC precompiled header** | **Reborn** |
| 10 | MoltenVK configure | builds from source, fragile in CI | fork age, external |

### The two findings that matter

**Finding A: `NflsUiSplit.cpp` was never in the CMake build.**

`RenderBase.cpp` and `VertexManagerBase.cpp` both **call** it, unguarded. The file was added to the Visual Studio project only. So Windows links fine and every CMake build, meaning Linux and macOS, fails with undefined references.

**Finding B: `NflsUiSplit.h` uses `u32` while including nothing.**

The MSVC precompiled header supplies it. CMake builds do not.

**Neither is a logic bug.** Both are Windows-only build assumptions, invisible on the platform they were written on and fatal everywhere else. Neither would have been found by reading the code. They were found by trying to build it, which is the entire justification for the probe.

### Where the build reached

- **Linux: 1204 of 1205 objects compiled**, including all four Reborn sources, before the link stage. Every line of Reborn C++ compiles clean under GCC 12.
- **macOS: past 1000 of 3533 objects** and climbing at time of writing.

**The honest verdict:** this fork is portable, and its age is the tax. Nine of ten blockers were the 2022-era snapshot meeting 2026 toolchains. Two were Reborn build wiring, both now fixed.

---

## 4. Reproducibility gap

**START_HERE checklist item 4.**

**Before:** no CI, no committed build script, no pinned toolchain, eight untracked `build*.log` files in the tree root. A second machine could not reproduce `617321bf...`.

**Now:** a committed workflow that builds Linux and macOS from a clean checkout with pinned compilers, and that greps the build tree to confirm the Reborn translation units are present.

**Still missing:**

1. **No Windows CI.** The shipping platform is the only one without a reproducible build. This is the largest remaining gap and it is embarrassing to write down.
2. **Compiler is pinned in CI only**, not in the repository. A local build still uses whatever is installed.
3. **The MSBuild and CMake file lists can drift**, which is precisely how finding A happened. Nothing prevents a recurrence.

**Proposed:** add a Windows CI job, and add a check that fails if any `Nfls*` source is present in one build system's file list and absent from the other. That check would have caught finding A the day it was introduced.

---

## 5. Windows-only surface

**START_HERE checklist item 5.**

| Component | Portable | Evidence |
|---|---|---|
| `NflsPlayHide` | **Yes** | Zero Windows-specific code |
| `NflsStatTracker` | **Yes** | One `_WIN32` guard |
| `NflsStatUpload` | **Yes** | Uses `Common/HttpRequest.h`, Dolphin's own cross-platform wrapper. **Not WinHTTP**, correcting an earlier claim in the inventory |
| `NflsUiSplit` | **No** | Includes `windows.h`, three `_WIN32` guards |
| ReShade via `dxgi.dll` | **No** | Windows injection only |
| `NSRShaderlessMenu.addon64` | **No** | A ReShade add-on |
| Per-field ReShade presets | **No** | Same |

**`NflsUiSplit` is now in the CMake build** so the symbols resolve, with its Windows-specific internals behind existing guards. It is experimental and modified the same day the project began, so **it should not gate any release**.

Linux and macOS ship without the per-field ReShade look. Adam accepted this.

---

## 6. Runtime packaging

**START_HERE checklist item 3.**

### What ships

```
runtime/<platform>/
  Dolphin binary and platform libraries
  Sys/                 required data
  Languages/
  LICENSES/            GPLv2+ obligation
  config-templates/    generated per launch, never edited in place
  version.json         semver, build hash, expected Sys hash
  Windows only: dxgi.dll, reshade-shaders/, reshade-presets/,
                NSRShaderlessMenu.addon64 + .ini
```

### What must never ship, and this is enforced by packaging

| Item | Why |
|---|---|
| `User\nsr_track_offline.txt` | **Disables the netplay-only stat gate.** The runtime folder's own README states this causes every offline CPU game to upload, and that league-team games can land on a real fixture |
| `User\nsr_debug.txt` | Prints gate state on screen |
| `Games\*.iso` | 5.7 GB of disc images sit in the runtime folder today |
| `*.bak*` | Dated backups of the exe, the addon, the addon ini, and `GFX.ini` |
| `Dolphin.exe.bak_2026-08-29_pre_uisplit` | Same |

**Adam approved making this structural.** Packaging strips both text files, and the netplay gate becomes a compiled-in setting so that dropping a file into `User\` cannot defeat it. A build flag, not a file, enables offline capture for testing.

### Configuration model

The launcher **generates** `GCPadNew.ini`, `GFX.ini`, and `Dolphin.ini` from its own settings model on every launch. It never edits them in place.

Reason: the user can still open Reborn Dolphin directly and change things there. Without generation, the two would fight over the same files and the launcher's displayed settings would silently drift from reality.

Dolphin's own changes are therefore **not preserved** across a launcher launch. The Settings page is the source of truth, and Reborn Dolphin is for inspection and one-off experiments. That needs saying in the UI, once, on the Open Reborn Dolphin action.

### Launch

Direct boot by path, no game list, no NetPlay browser. Online launches receive the match context by controlled local file. **The Discord token is never passed to Dolphin.**

---

## 7. Migration plan

**START_HERE checklist item 8. This is the part requiring Adam's approval.**

### Phase 0, done

Preservation commit `eca379f`, private remote, cross-platform CI, two Reborn build defects found and fixed.

### Phase 1, before any public build

1. **Rotate the compiled-in bearer token** in `NflsStatUpload.cpp`. Currently deferred by Adam. It blocks making the repository public, which GPLv2+ requires once binaries are distributed.
2. **Add Windows CI**, so the shipping platform is reproducible.
3. **Add the build-system drift check** described in section 4.
4. **Make the netplay gate compiled-in.**

### Phase 2, integration

5. Move upload out of Dolphin. `NflsStatTracker` keeps writing JSON; the launcher and supervisor own authentication, retry, and queue.
6. Extend `NflsPlayHide`'s port-to-team mapping into a stable interface for the results protocol. It already solves the hard half.
7. Add a match-context input path: a controlled local file, read once at boot.

### Phase 3, deferred and explicitly not now

8. **Rebasing onto modern upstream Dolphin.** This would fix nine of the ten build blockers at a stroke, and it is genuinely tempting after watching them all be fork-age problems.

**Recommendation: do not do it before September 12.** The fork carries a custom fixed-delay NetPlay path, `NflsUiSplit`'s renderer hooks into `RenderBase` and `VertexManagerBase`, and ExGL's own renderer changes. A rebase means re-deriving all of that against several years of upstream churn with no test hardware for two of three platforms. The failure mode is a subtly broken netcode discovered by players.

**Recommendation: schedule it immediately after.** Every month of delay makes it harder, and the CI now exists to tell you when it works.

### Risk assessment

| Risk | Severity | Mitigation |
|---|---|---|
| Token ships in a public binary | **High** | Rotate. Blocks publication until done |
| Windows has no CI | **High** | Add it in phase 1 |
| MSBuild and CMake lists drift again | Medium | Automated check. This already happened once |
| `NflsUiSplit` destabilizes a platform | Medium | Not in the CMake build originally; keep it out of the release gate |
| Rebase breaks netcode | **High if attempted now** | Defer past launch |
| macOS and Linux runtimes unverifiable | **High** | Ship flagged. No hardware exists |
| Qt5 to Qt6 config migration | Medium | Config is generated, so old files are read once for migration then discarded |

---

## 8. Licensing

**Dolphin is GPLv2+.** Distributing a modified binary obliges:

1. Attribution, already present in `README.txt` for BlueSwordM and `nfl_numbers`.
2. **Corresponding source availability.** `adampreddy/reborn-dolphin` is currently private, which is fine while nothing is distributed and **not fine on release day**.
3. A separate repository from the launcher, which is already the case.

The launcher itself, being a separate program that runs Dolphin as a child process, is not a derivative work. Bundling a GPL binary alongside a differently licensed application is standard, provided the GPL component's source is available.

**Blocking chain:** token rotation gates making the repository public, which gates lawful distribution. It is one task and it is on the critical path whether or not it feels urgent.

---

## 9. Preserved behaviors

**START_HERE checklist item 6.** Nothing in this plan removes any of these.

| Behavior | Status |
|---|---|
| Stat capture | Preserved unchanged |
| Stat upload history, `uploaded.txt` ledger | Preserved, moves to the launcher queue |
| Play-call hiding | Preserved and extended |
| Fair Input Delay NetPlay | Preserved. Rollback stays out of scope |
| ReShade add-ons | Preserved, Windows only |
| Per-field stage presets | Preserved, Windows only |
| UI split experiments | Preserved, now in the CMake build, not release-gating |

---

## 10. Web upload separation

**START_HERE checklist item 7.**

```
  Dolphin                         Launcher / supervisor
  ───────                         ─────────────────────
  reads game memory        →      reads the JSON file
  writes atomic JSON              authenticates
  writes uploaded.txt             retries with backoff
  knows nothing about HTTP        owns the credential
                                  survives a game crash
```

**Why this is strictly better than today:** the credential becomes rotatable without a rebuild, the retry queue survives the game exiting mid-upload, and the netplay gate cannot be defeated by placing a file in `User\`. Capture stays where the game-memory knowledge is.

---

## Open items

1. **Token rotation**, deferred, on the critical path for lawful distribution.
2. **Windows CI**, the shipping platform is the unreproducible one.
3. **Drift check** between MSBuild and CMake file lists.
4. **Neither Linux nor macOS has been run**, only built. Nobody has hardware.
5. **This plan is unapproved.** START_HERE item 8 requires Adam's review before further fork work.
