# Component Inventory

## NFL Street Reborn Launcher, discovery phase

**Date:** August 29, 2026
**Mode:** Read-only. Nothing outside this repository was created, modified, or deleted.
**Method:** Directory listings, manifests, source files, build metadata, git state, and SHA-256 comparison. Large binary and vendor trees were sized, not scanned.

### How to read this document

Each component records what was verified from evidence and what remains an inference. Two labels are used throughout:

- **Observed:** confirmed directly from a file, listing, hash, or git command in this session.
- **Inferred:** a reasonable reading of the evidence that has not been proven.

State values: **working**, **experimental**, **obsolete**, **unknown**, **distributable**.

---

## Executive summary of what was found

The Reborn system is not one product. It is five loosely coupled bodies of work that currently share nothing but the user's hard drive:

1. A **shipped Windows consumer package** (Inno Setup plus a portable Qt5 Dolphin plus a prebuilt 1.46 GB game image plus JSGME).
2. A **live development emulator fork** (`dolphin-exgl`, Qt6, ExGL lineage) that contains every NFL Street specific runtime feature.
3. A **game modification workspace** (`nflstreet2_clean`, 984 tools) that is the real source of every game build.
4. An **online matchmaking prototype** (Electron plus Node plus SQLite) that exists in three divergent copies outside both of the above.
5. An **Xbox Dev Mode package** distributed as a two-folder manual ZIP.

Four findings materially affect the launcher architecture and are expanded in "Contradictions and risks" at the end.

---

## 1. Launcher repository (this repo)

| Field | Value |
|---|---|
| Name | NFL Street Reborn Launcher |
| Authoritative path | `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher` |
| Language/framework | None yet |
| State | **unknown** (planning only) |
| Contents | `CLAUDE_START_HERE.md`, `docs/design/CLAUDE_UX_RESEARCH_BRIEF.md`, `docs/design/Reborn Launcher Moodboard.png` |
| Inputs | Product blueprint and transcript in `C:\Users\Adam\Documents\Codex\2026-08-29\we-x20\outputs\` |
| Outputs | None |
| Dependencies | None |
| Platform assumptions | Windows, Linux, Steam Deck, macOS |
| Secrets | None |
| Licensing | Original team work |
| Reuse | N/A, this is the new home |

**Observed:** the directory is **not a git repository**. `git status` and `git log` both return "not a git repository". No `.gitignore`, no source control history, no CI. Every planning artifact currently exists as a single uncommitted copy.

**Questions for Adam:** canonical Git host and remote; whether this Desktop path stays the working tree.

---

## 2. Windows installer pipeline (Inno Setup)

| Field | Value |
|---|---|
| Name | NFL Street Reborn Setup |
| Authoritative path | `C:\Users\Adam\Desktop\Reborn Installer\rebornsetupv10.iss` |
| Language/framework | Inno Setup 6, Pascal script |
| State | **distributable** (this is what users install today) |
| Inputs | `Files\Reborn Dolphin\**`, `Files\NFL Street Reborn Patcher.exe`, `Files\README.txt`, `Files\READMEAFTER.txt`, bitmaps in the parent folder |
| Outputs | `Output\NFL Street Reborn Setup.exe` (7.4 MB) plus `NFL Street Reborn Setup-1.bin` (1.63 GB), both dated January 11, 2026 |
| Dependencies | Inno Setup compiler; absolute `C:\Users\Adam\Desktop\...` source paths |
| Platform assumptions | Windows only |
| Reuse | **Wrap, then reduce.** Keep Inno Setup, strip it back to installing the launcher only |

**Observed behavior of v10:**

- App version 2.1, AppId `{DCD6B9B9-11DE-4AC0-A7F5-256E7AC55A7E}`, publisher URL `https://nflstreetreborn.com`.
- `DefaultDirName={userappdata}\NFL Street Reborn`, `PrivilegesRequired=lowest`. So the current product already installs to Roaming AppData, matching the blueprint's storage decision.
- `DiskSpanning=yes` because the payload exceeds 2 GB.
- It runs **two post-install wizard pages** that launch external programs: a "Patch Your File" page that executes the patcher, and a "Tweak Your Mods" page that shell-executes JSGME with `runas` (elevation prompt).
- Update path: reads `DisplayVersion` from the uninstall registry key, backs up `{app}\User`, deletes `User\Load\Textures\GN7E69` and the whole `MODS` folder, reinstalls, then restores.
- `CurUninstallStepChanged` performs `DelTree({app}, True, True, True)`, which **deletes the entire application folder including saves and memory cards** on uninstall. This directly contradicts blueprint section 14 ("uninstallation should preserve saves").
- Nine older `.iss` versions (v3 through v9, plus two unnumbered) sit alongside v10 and are **obsolete**.

**Verified defects to carry into the new design:**

- `BackupUserData` and `RestoreUserData` use `FileCopy` with wildcard paths (`{app}\User\*`). Inno Setup's `FileCopy` copies a single file and does not expand wildcards, so these calls fail and only log. **Inferred:** the backup and restore have never actually worked; user data survives only because of the `onlyifdoesntexist` flag on the `User\*` entry.
- The uninstall `DelTree` is unconditional and offers no preserve choice.

**Questions for Adam:** has anyone ever reported losing saves on uninstall or update?

---

## 3. Patcher and game import workflow

| Field | Value |
|---|---|
| Name | NFL Street Reborn Patcher |
| Authoritative path | `C:\Users\Adam\Desktop\Reborn Installer\Reborn Patcher\rebornpatcherv6.py` (August 30, 2025) |
| Language/framework | Python 3, Tkinter, packaged with PyInstaller |
| State | **obsolete by design**, see below |
| Reuse | **Retire.** Replace with a real import and verify pipeline |

**Observed, and this is the single most important product finding:**

`rebornpatcherv6.py` does not patch anything. Its core function is literally named `fake_patch`. It asks the user to select an ISO, then iterates a hardcoded list of twelve status strings with `time.sleep()` delays totaling about 14 seconds, advances a progress bar, prints "Game File Successfully Patched!", and exits. **The selected file is never opened, never read, never hashed, and never written.**

The playable game the user ends up with is `Files\Reborn Dolphin\Games\NSR.iso` (1,459,978,240 bytes), which the installer copies verbatim. The "select your legally owned ISO" step is presentation only.

The same pattern exists in the Xbox package, which ships `Step 1 - Patch Game File\resources\nsrpatch.iso` at the same 1,459,978,240 bytes.

**Consequences for the launcher:**

- The blueprint's entire game import model ("import once, hash and identify, require a supported USA revision, preserve a verified private base, generate managed builds") **does not exist yet in any form**. It is new engineering, not integration.
- The current distribution ships a complete commercial game image. The blueprint's legally owned source requirement is currently unenforced. This is a distribution and licensing decision Adam needs to make explicitly, not a bug to quietly fix.
- No diffing, patch, or delta format exists between a retail NFL Street 2 USA image and `NSR.iso`. Building one is a prerequisite for "import once, use forever" and for differential updates.

**Questions for Adam:** this is question 1 in the questionnaire and it blocks the content and update architecture.

---

## 4. Selected Reborn Dolphin runtime (production foundation)

| Field | Value |
|---|---|
| Name | CREATION LEAGUE AND NEW DOLPHIN |
| Authoritative path | `C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN` |
| Language/framework | C++, Qt 6, Dolphin |
| State | **experimental**, currently configured as a test rig |
| Reuse | **Port and harden.** Chosen foundation per `CLAUDE_START_HERE.md` |

**Build lineage: PROVEN.** SHA-256 of the runtime's `Dolphin.exe` is `617321bf37ac7b2f9bdfcd3af803a019e862591259c770e9e688d53834df1338`, byte-identical to `dolphin-exgl\Build\x64\Release\Dolphin\bin\Dolphin.exe`. Both are 17,818,624 bytes dated August 29, 2026. The runtime folder is a copy of the working tree's Release output. Item 1 of the START_HERE runtime checklist is satisfied.

For contrast, the shipped installer's `NFL Street Reborn Dolphin.exe` hashes to `8705a96b...`, is 14 MB, and links Qt5. It is a **different and older binary from a different lineage**. Replacing it is a real runtime migration, not a file swap.

**Contents observed:**

- Qt6 runtime DLLs, `Sys`, `Languages`, `LICENSES`, `QtPlugins`, `portable.txt` (portable mode is already in use).
- `dxgi.dll` 5.5 MB dated February 28, 2026, which is the ReShade injector.
- `NSRShaderlessMenu.addon64` (205 KB, August 29, 2026) plus `.ini`, plus two dated `.bak` copies. `ReshadeEffectShaderToggler.addon64` is third-party.
- `reshade-presets` and `reshade-shaders`.
- `User\` with `Config`, `GameSettings`, `GC`, `Load`, `LeagueGameStats`, `StateSaves`, and more.
- `Games\` holds **5.7 GB across four full disc images**: `CR4E12.iso` (Creation League), `FR4N31.iso` (online build), `NSR.iso`, and one named `TOTALLY NOT MALWARE.iso`.
- `Dolphin.exe.bak_2026-08-29_pre_uisplit`, plus dated `.bak` files for the ReShade addon, the addon ini, and `GFX.ini`.

**Debug and test toggles that must never ship (observed, and the folder's own README says so):**

- `User\nsr_track_offline.txt` is **present**. The README states this file disables the netplay-only gate, so every offline game against the CPU gets captured and uploaded to the live league site.
- `User\nsr_debug.txt` is **present**. It prints gate state on screen.
- The README's own "BEFORE YOU SEND THIS OUT" section instructs deleting both. Neither has been deleted.

**Windows-only dependencies blocking Linux and macOS (START_HERE item 5):**

- `dxgi.dll` ReShade injection is Windows only.
- `NSRShaderlessMenu.addon64` is a ReShade add-on, so the per-field preset work is Windows only.
- `NflsUiSplit.cpp` includes `windows.h` and carries three `_WIN32` guards. It is also **not listed in `Source/Core/VideoCommon/CMakeLists.txt`**, so the CMake build used on Linux and macOS does not compile it at all.
- **Correction to an earlier draft of this document:** `NflsStatUpload.cpp` does **not** use WinHTTP. It includes `Common/HttpRequest.h`, Dolphin's own cross-platform HTTP wrapper. Verified by reading its include block.
- **Verified portable:** `NflsPlayHide.cpp` has zero Windows-specific code. `NflsStatTracker.cpp` has one `_WIN32` guard. `NflsStatUpload.cpp` has none. All three are already listed in `Source/Core/Core/CMakeLists.txt` lines 442-447, so the CMake build path already includes them.

**Questions for Adam:** which of the four ISOs is authoritative for which purpose; whether `TOTALLY NOT MALWARE.iso` is meaningful or a joke copy.

---

## 5. Reborn Dolphin source tree

| Field | Value |
|---|---|
| Name | dolphin-exgl |
| Authoritative path | `C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\dolphin-exgl` |
| Language/framework | C++, CMake and MSBuild, Qt 6 |
| State | **experimental**, single-machine working tree |
| Upstream | `https://github.com/BlueSwordM/dolphin.git`, HEAD `7f4875e` |
| Licensing | GPLv2+ (Dolphin). Distribution requires attribution and corresponding source |
| Reuse | **This is the source of truth. Preserve it before anything else** |

### The most severe risk found in this discovery

**Observed via `git status`:** every NFL Street specific source file is **untracked**, and every upstream modification is **uncommitted**.

Untracked (`??`), meaning they exist in no commit anywhere:

```
Source/Core/Core/NflsPlayHide.cpp            4,277 bytes    2026-07-22
Source/Core/Core/NflsPlayHide.h                776 bytes    2026-07-22
Source/Core/Core/NflsStatTracker.cpp        47,000 bytes    2026-07-27
Source/Core/Core/NflsStatTracker.h           1,619 bytes    2026-07-26
Source/Core/Core/NflsStatUpload.cpp         14,359 bytes    2026-07-27
Source/Core/Core/NflsStatUpload.h            2,558 bytes    2026-07-27
Source/Core/VideoCommon/NflsUiSplit.cpp     26,232 bytes    2026-08-29
Source/Core/VideoCommon/NflsUiSplit.h        1,585 bytes    2026-08-29
Installer/Dolphin.ico.bak-stock
```

Modified but uncommitted (` M`):

```
Externals/mGBA/exports.props
Installer/Dolphin.ico
Source/Core/Common/Version.cpp
Source/Core/Core/CMakeLists.txt
Source/Core/Core/PatchEngine.cpp
Source/Core/DolphinLib.props
Source/Core/DolphinQt/DolphinQt.vcxproj
Source/Core/DolphinQt/MainWindow.cpp
Source/Core/DolphinQt/RenderWidget.cpp
Source/Core/VideoCommon/RenderBase.cpp
Source/Core/VideoCommon/VertexManagerBase.cpp
Source/VSProps/Base.Dolphin.props
```

That is roughly 98 KB of original C++ representing the entire competitive and statistical feature set, plus twelve upstream integration points, existing as **working-tree state on one machine with no commit, no branch, no tag, and no remote**. A disk failure, an accidental `git checkout .`, or a `git clean -fd` erases the product.

**Recommendation, and it is the first thing that should happen regardless of every other decision:** commit this tree to a branch and push it to the canonical remote before any further work. This is a preservation action, not a rebase or a replacement, so it does not conflict with START_HERE item 8. Adam's approval is requested in question 2.

### Verified feature inventory

| File | Purpose | Observed detail |
|---|---|---|
| `NflsPlayHide` | Hides the opponent's play-call area during online play | Identifies which GameCube port is local and which in-game team that port controls. This is the existing half of the winner identity chain |
| `NflsStatTracker` | Captures teams, score, team totals, per-player stats | 47 KB, the largest single file. Emits `sides_verified`, `team_totals`, `players` (14), `teams` to JSON |
| `NflsStatUpload` | Uploads captured records | Endpoint `https://nflstreetreborn.com/league/stats/ingest.php`. Sends `Authorization: Bearer <token>`. **A `DEFAULT_TOKEN` is compiled into the binary.** A local override file can supply URL on line 1 and bearer token on line 2. Maintains a ledger of already-sent hashes |
| `NflsUiSplit` | Experimental field and HUD render separation | 26 KB, modified today (August 29, 2026). The most volatile file in the tree |
| `Version.cpp` | Product naming | Changes the Dolphin revision string to `NFL Street Reborn Dolphin (BETA)`. The in-file comment notes this shifts payload offset inside `.sav` headers, which matters to external save-state parsers |

**Secrets:** the bearer token is a real embedded credential. Its value is deliberately not reproduced in this document. It must be rotated, and it must not survive into a public repository or a shipped binary. See the `.gitignore` plan below.

**Reproducibility gap (START_HERE item 4):** no CI, no build script committed, no pinned toolchain in source control. `build_info.txt` in the runtime records `VCToolsVersion=14.44.35211.0` and minimum OS Windows 10 build 18362, but that file is Dolphin's updater metadata, not a build record. Eight `build*.log` files sit untracked in the tree root. There is currently no way for a second machine to reproduce `617321bf...`.

---

## 6. Game modification workspace

| Field | Value |
|---|---|
| Name | nflstreet2_clean |
| Authoritative path | `C:\Users\Adam\nflstreet2_clean` |
| Language/framework | Python 3 (984 files in `tools\`), plus C and PowerPC assembly for DOL caves |
| State | **working**, and it is the real factory for every game build |
| Reuse | **Wrap as a build backend.** Do not port to Rust or TypeScript |

**Observed:** this directory is **not a git repository** either. `git status` and `git log` both fail. It holds roughly 200 loose log files (`nfls2_link_*.log`, `nfls2_soak_*.log`, `build_*.log`), scratch scripts, and pickled analysis state at the top level, mixed with the authoritative content.

Key areas confirmed present:

- `GameCube Version\root` is the live extracted game filesystem and the source of truth for the build tools.
- `tools\` with 984 entries, including `build_iso_fields.py` and `build_iso_with_game12.py` (ISO builders), `music_editor.py`, `music_pack.py`, `team_colors.py`, and the field, jersey, and portrait importers.
- `Stat Tracking\` with `gcmem.py`, `tracker.py`, `parse_gamestats.py`, `decode_sav.py`, `lzo1x.py`, `Start.dol.statread1`, and a documented DOL hook at instruction `0x8005a960` into a cave at `0x803fe4e0`.
- `GAME VERSIONS\` with four reference builds: GameCube release, PS2 release, Xbox release, and an Xbox prototype dated August 28, 2006.
- `decomp\` and `decomp_gc\` decompilation work.
- `Obsidian Vault\` documentation.
- `projects\audio_soundtrack`, `projects\online_netcode`, `tools\NSRShaderlessMenu`.

**Licensing:** contains extracted commercial game data and four full retail game versions. None of this can ship. It is a development input only.

---

## 7. Music editor and `.nsmp` packs

| Field | Value |
|---|---|
| Name | Music Editor |
| Authoritative path | `C:\Users\Adam\nflstreet2_clean\tools\music_editor.py` (21.9 KB, July 2, 2026) |
| Language/framework | Python 3, Tkinter, FFmpeg |
| State | **working** |
| Reuse | **Wrap the tool chain, replace the UI.** This is harder than the blueprint assumes |

**Observed, and this changes the plan:** `music_editor.py` is an **orchestrator, not an engine**. Its own docstring states it drives four separate tools:

```
extend_slot_40big.py       40 bigger segments, EA-exact sample accounting
fix_pfdx_toc.py            refresh the per-stream position table in pathfind.dat
edit_music_meta.py         title/artist/album in template.dat
build_iso_with_game12.py   self-heals the 5 DOL guards, packs the disc
```

Its hardcoded paths are the problem:

- `ROOT = <repo>/GameCube Version/root`, so it operates on the **extracted game filesystem**, not on an ISO.
- `PATHFIND = root/pathfind.dat`, `TEMPLATE = root/template.dat`.
- `GOOD_BASE = root/pathfind.dat.bak_good_2026-07-02`, a dated reset point.
- `ISO_OUT = %APPDATA%/NFL Street Reborn/Games/NFL Street 2 BEACH FIELD13.iso`, a developer path.
- It imports `edit_music_meta` as a Python module at runtime.

**Consequence:** shipping music editing to end users requires shipping an extracted, writable copy of the game filesystem plus a Python runtime plus FFmpeg plus at least five tools, or reimplementing the PFDx and template.dat writers natively. Neither is a small task, and the choice affects both the mods and the music architecture. This is question 5.

`music_pack.py` (8.2 KB) handles `.nsmp` import and export. **Inferred:** it is the pack format authority, but its schema was not read in this pass.

---

## 8. Statistics capture and upload

| Field | Value |
|---|---|
| Name | Reborn stat pipeline |
| Authoritative paths | `dolphin-exgl\Source\Core\Core\NflsStatTracker.*` and `NflsStatUpload.*`; `nflstreet2_clean\Stat Tracking\` |
| Language/framework | C++ inside Dolphin; Python for offline decode; PHP endpoint on the site |
| State | **working**, with a known trust gap |
| Reuse | **Split.** Keep capture in Dolphin, move upload into the launcher, exactly as the blueprint proposes |

**Observed:** two independent capture mechanisms exist and should not be confused.

1. **In-emulator (production):** `NflsStatTracker` reads game memory live and writes JSON to `User\LeagueGameStats\`, with `uploaded.txt` as the sent ledger. `NflsStatUpload` posts it. This is what ships.
2. **Offline DOL hook (research):** `Stat Tracking\Start.dol.statread1` patches the game itself and logs to reserved RAM, decoded later from a save state. This requires a modified `Start.dol` and an ISO rebuild. This is a research path, not a shipping one.

**Verified trust gap:** the runtime README states plainly that "the uploader runs whenever any game is open, it is not behind the netplay gate" when `nsr_track_offline.txt` is present, and that with league teams selected "a test game could resolve to a real fixture and write real stats." The gate exists but is currently disabled on the selected runtime.

**Security:** compiled-in bearer token, described in component 5. Rotation is required before any public build.

---

## 9. Online matchmaking

| Field | Value |
|---|---|
| Name | nfl-street-reborn-online |
| Candidate paths | See table below |
| Language/framework | Electron, Vite, React, Tailwind; Node.js server; SQLite |
| State | **experimental**, three divergent copies, **no canonical selection** |
| Reuse | **Rebuild the client, harvest the server** |

Per START_HERE, no copy was selected silently. Three distinct working copies exist. `C:\Users\Adam\Application Data\nfl-street-reborn-online` resolves to the same object as the AppData path and is Electron runtime data, not source.

| Copy | Newest source file | Distinguishing content |
|---|---|---|
| `Downloads\StreetLauncherProject\nfl-street-reborn-online` | June 5, 2026 | Has `server\liveState.js`, `app\src\main\settings.js`, and a `renderer\components\settings\` folder. **Newest overall** |
| `Downloads\nfl-street-reborn-test\nfl-street-reborn-online` | June 2, 2026 | Has `app\src\main\netplay_uia.ps1`, a Windows UI Automation script. **Not present in the other copies** |
| `Downloads\StreetLauncherProject\nfl-street-reborn-test\nfl-street-reborn-online` | June 4, 2026 | Intermediate; partially deleted in git status |
| `AppData\Roaming\nfl-street-reborn-online` | n/a | Electron user data, not source |

**Neither leading copy is a superset of the other.** `dolphin.js`, `index.js`, `preload.js`, `TitleBar.jsx`, `Queue.jsx`, `api.js`, `server\index.js`, `routes\admin.js`, and all three `socket\*.js` files differ between them. `netplay_uia.ps1` exists only in the test copy and is likely the mechanism that drives Dolphin's NetPlay UI automatically, which is directly relevant to hiding NetPlay from users.

**Git state of `StreetLauncherProject`:** two commits only (`d1b19c9 Initial Project`, `0ffad6e initial commit`), with uncommitted modifications and deletions. Two remotes are configured:

- `origin` -> `https://github.com/MiltyTheGreat/RebornOnline.git`
- `launcher` -> `https://github.com/michaelpenixfan2390/nflstreetrebornlauncher.git`

**These are third-party GitHub accounts, not Adam's.** Ownership, licensing, and contributor permission are unresolved. This is question 4.

**Server components worth preserving (observed):** `elo.js`, `liveState.js`, `db\schema.sql` plus a live `nfl_reborn.db`, `middleware\auth.js`, `routes\{admin,auth,matches,users}.js`, `socket\{queue,match,custom}.js`. This is a working matchmaking backend shape and maps closely to the blueprint's backend entity list.

**Secrets:** `server\.env` contains `PORT`, `JWT_SECRET`, `DISCORD_CLIENT_ID`, `DISCORD_CLIENT_SECRET`, `DISCORD_REDIRECT_URI`, `CLIENT_URL`. Values were not read into this document. A Discord application therefore **already exists**, which answers part of blueprint open decision "Discord application credentials". All of these must be rotated because they have lived in a Downloads folder alongside git remotes pointing at third-party accounts.

**macOS reality check:** `dolphin\mac\` contains exactly one file, `PUT_DOLPHIN_APP_HERE.txt`, zero bytes. **No macOS Dolphin build exists anywhere in the inspected material.** `dolphin\win\` contains a Qt6 Dolphin support set but, notably, no `Dolphin.exe` in the tracked copy.

---

## 10. Mods, JSGME, and the future marketplace

| Field | Value |
|---|---|
| Name | Reborn Mod Manager (JSGME 2.6) |
| Authoritative path | `Files\Reborn Dolphin\Reborn Mod Manager (JSGME).exe` (1.2 MB) plus `MODS\` (20 MB) |
| State | **distributable**, to be replaced |
| Licensing | JSGME is third-party freeware. Redistribution terms need confirmation |
| Reuse | **Retire.** Migrate its content into the new package format |

**Observed current catalog** (13 entries in `MODS\`): Aqua, Blue, Camo, Green, Panda, Pink, and Purple Football; PS Controls; Xbox Controls; Patriots Throwback; Saints Alternate; plus `!BACKUP` and `!INSTLOGS` control folders and `JSGME.ini`.

Every mod is a texture overlay into `User\Load\Textures\`. Confirmed for `PS Controls`, whose entire tree is `User\Load\Textures\`.

**Migration note:** `!BACKUP` and `!INSTLOGS` are JSGME's restore mechanism. The v10 installer deletes the whole `MODS` folder on update, which destroys them. Any user with mods enabled across an update is left with overlaid files and no restore path. This is exactly the "JSGME-style destructive ambiguity" the blueprint wants gone, and it is a real observed failure mode, not a theoretical one.

**Texture identity:** `User\Load\Textures\` contains a single game ID folder, `GN7E69`, roughly 2.0 GB. `User\GameSettings\` holds `GN7E69.ini` and `GNNE69.ini`.

---

## 11. Controller configuration

| Field | Value |
|---|---|
| Name | Controller profiles |
| Authoritative paths | `Files\Reborn Dolphin\User\Config\GCPadNew.ini`; `User\Config\Profiles\GCPad\{PS,Xbox}.ini` |
| State | **working but misleading** |
| Reuse | **Replace entirely** with an SDL3 controller service |

**Observed and confirmed:** the active `GCPadNew.ini` binds `[GCPad1]` to `Device = XInput/0/Gamepad`. Both named profiles, `PS.ini` and `Xbox.ini`, exist under `Profiles\GCPad\`. The blueprint's claim that the PlayStation profile "still targets XInput/0/Gamepad" is consistent with what is in the active config.

**The important detail:** the `PS Controls` entry in `MODS\` is **a texture pack only**. Its complete tree is `User\Load\Textures\`. It changes the on-screen button icons and nothing else.

So today, "PlayStation support" consists of Xbox-style XInput bindings plus PlayStation-shaped button art. There is no DualShock 4 or DualSense device backend, no Bluetooth handling, no hot-plug logic, no per-device profile storage, and no Steam Input or DS4Windows conflict detection. The blueprint's PlayStation acceptance matrix (twelve scenarios) is **entirely unbuilt**. Adam flagged this as a long-standing pain point in the transcript, so it should be treated as new first-class engineering with a real budget.

Two Wiimote profiles (`Sluggers.ini`, `Sluggers no nun.ini`) are leftovers from an unrelated game and are **obsolete**.

---

## 12. Update checker

| Field | Value |
|---|---|
| Name | NSR_UpdateChecker.exe |
| Authoritative path | `Files\Reborn Dolphin\NSR_UpdateChecker.exe` (128 KB) |
| State | **distributable**, legacy |
| Reuse | **Retire** |

**Observed:** the installer registers this as the **primary Start Menu and desktop shortcut**, ahead of Dolphin itself. So the user's entry point to NFL Street Reborn today is already a small launcher shim. No source was located for it in the inspected paths. **Inferred:** it checks a version endpoint and then starts Dolphin.

**Question for Adam:** where is its source, and what URL does it contact? Recorded as a Research item.

---

## 13. Xbox Dev Mode package

| Field | Value |
|---|---|
| Name | NFL Street Reborn Xbox Dev Mode 2.1 |
| Authoritative path | `C:\Users\Adam\Downloads\NFL Street Reborn Xbox Dev Mode 2.1` |
| State | **distributable**, manual |
| Reuse | **Absorb into a PC-side wizard** |

**Observed structure, and the duplication is confirmed:** the folder contains `Step 1 - Patch Game File` and `Step 2 - Transfer to Xbox` **at its root**, and then an identical nested `NFL Street Reborn Xbox Dev Mode 2.1\` containing the same two folders again. The blueprint's "duplicate nested package structure" concern is real.

Contents:

- `Step 1\NFL Street Reborn Patcher.exe` plus `resources\nsrpatch.iso` (1,459,978,240 bytes). Same fake-patcher pattern as the Windows installer.
- `Step 2\Config\` with `Dolphin.ini`, `GFX.ini`, `GCPadNew.ini`, `GCKeyNew.ini`, `DSUClient.ini`, `FreeLook.ini`, `Logger.ini`, `RetroAchievements.ini`, `TimePlayed.ini`, `WiimoteNew.ini`, `GraphicMods\`.
- `Step 2\GameSettings\GN7E69.ini`.
- `Step 2\GC\MemoryCardA.USA.raw` and `SRAM.raw`. **Real save data ships inside the package**, so any wizard that writes this folder must not overwrite a returning user's card. The blueprint already calls for this.
- `Step 2\Load\Textures\` and `NSR.png` cover art.
- No app package, certificate, or Device Portal tooling. There is **no existing deployment automation** to reuse. The entire Device Portal path in the blueprint is new work.

**Not verified in this pass:** the `E:\Games` path assumption and the WGInput mapping were not read out of `Dolphin.ini` and `GCPadNew.ini`. Recorded as a Research item.

---

## 14. ReShade and the shaderless menu add-on

| Field | Value |
|---|---|
| Name | NSRShaderlessMenu, ReShade integration |
| Authoritative paths | `nflstreet2_clean\tools\NSRShaderlessMenu`; runtime `NSRShaderlessMenu.addon64` and `.ini`; `reshade-presets\`; `reshade-shaders\` (28 MB in the installer copy) |
| State | **working**, Windows only |
| Licensing | ReShade and `ReshadeEffectShaderToggler.addon64` are third-party. `dxgi.dll` injection has redistribution implications |
| Reuse | **Keep on Windows, do not port** |

Per the blueprint and Adam's explicit direction, Linux and macOS do not require ReShade parity. The per-field preset work is Windows only by construction because it is a ReShade add-on.

---

## 15. Existing website and backend endpoints

| Field | Value |
|---|---|
| Name | nflstreetreborn.com league stack |
| Known endpoint | `https://nflstreetreborn.com/league/stats/ingest.php` (observed in `NflsStatUpload.cpp`) |
| Related | `nflstreet2_clean\docs\league_site_deploy` |
| State | **working**, live |
| Reuse | **Out of launcher scope, but the stat contract matters** |

League functionality is explicitly excluded from the launcher. The ingest contract is not, because the launcher takes over uploading. Whether the launcher posts to the existing PHP endpoint or to a new backend is question 7.

---

## Contradictions and dangerous ambiguities

Ranked by how much damage they do if not addressed.

**1. The Reborn Dolphin feature set exists only as uncommitted working-tree state.** Eight untracked files and twelve uncommitted modifications, on one machine, with no branch or remote. Losing this loses `NflsStatTracker` (47 KB), `NflsUiSplit` (26 KB), `NflsStatUpload`, and `NflsPlayHide`. Neither `nflstreet2_clean` nor the launcher repository is under version control either. Preservation should precede planning.

**2. The patcher is a simulation, so game import does not exist.** `fake_patch()` sleeps and lies. The shipped `NSR.iso` is a complete prebuilt game image. The blueprint's import, verify, retain-base, and generate-builds model, and the differential update model that depends on it, are entirely new engineering. This also means the "legally owned copy" requirement is currently unenforced, which is a distribution decision for Adam.

**3. The selected runtime is armed for live uploads with the safety gate off.** `nsr_track_offline.txt` and `nsr_debug.txt` are both present in `CREATION LEAGUE AND NEW DOLPHIN\User\`. Its own README says shipping in this state causes every player's offline CPU game to be captured and uploaded, and that league-team games can land on real fixtures. Anyone copying this folder forward inherits that.

**4. Three divergent online copies, neither leading one a superset, on third-party remotes.** The newest copy has settings and live-state work; a different copy has the `netplay_uia.ps1` NetPlay automation. Remotes point to `MiltyTheGreat/RebornOnline` and `michaelpenixfan2390/nflstreetrebornlauncher`. Deleting the wrong copy loses unique work, and the ownership question is unanswered.

**5. Live credentials in three places.** A bearer token compiled into `NflsStatUpload.cpp`; `JWT_SECRET`, `DISCORD_CLIENT_ID`, and `DISCORD_CLIENT_SECRET` in `server\.env` inside a Downloads folder wired to third-party remotes. All should be rotated. None of their values appear in this document.

**6. Uninstall deletes everything, and update backup silently fails.** `DelTree({app})` on uninstall removes saves and memory cards. `BackupUserData` and `RestoreUserData` call `FileCopy` with wildcards, which Inno Setup does not support, so they log a failure and continue. Update also deletes the whole `MODS` folder including JSGME's `!BACKUP` restore data.

**7. macOS has nothing at all.** The only macOS artifact found is a zero-byte `PUT_DOLPHIN_APP_HERE.txt`. No Dolphin build, no toolchain, no signing account, and per the transcript, no Mac hardware. Every macOS claim is currently unbacked.

**8. Two Dolphin lineages are in play.** Shipped Qt5 `8705a96b...` versus selected Qt6 `617321bf...`. Config schemas, profile formats, and texture behavior can differ between them, so migrating existing users is a real compatibility exercise.

---

## Appendix: `.gitignore` plan

Required by `CLAUDE_START_HERE.md` before any build artifacts or secrets are created. Proposed for the launcher repository at first commit:

```gitignore
# Secrets, never commit
.env
.env.*
!.env.example
*.pem
*.p12
*.pfx
credentials.json

# Game data and user-provided content, never commit
*.iso
*.rvz
*.gcm
*.wbfs
*.dol
*.raw
*.nsmp

# Large binary trees
/runtime/
/vendor/dolphin/
**/reshade-shaders/
**/Load/Textures/

# Build output
/target/
/dist/
/build/
/release/
node_modules/
*.exe
*.dll
*.dylib
*.so

# Local state
*.db
*.db-shm
*.db-wal
*.log
.DS_Store
Thumbs.db

# Allowed by exception
!docs/design/*.png
```

Two rules that matter more than the list: the `dolphin-exgl` fork must be a **separate repository** (GPLv2+ corresponding-source obligations), and the design moodboard is the one binary this repository should carry.

---

## Component state summary

| Component | State | Reuse recommendation |
|---|---|---|
| Launcher repository | unknown | Build here |
| Inno Setup pipeline | distributable | Wrap, reduce to install-only |
| Patcher | obsolete by design | Retire, replace with real import |
| Selected Reborn Dolphin runtime | experimental | Port and harden |
| dolphin-exgl source | experimental | **Preserve first**, then harden |
| nflstreet2_clean workspace | working | Wrap as build backend |
| Music editor | working | Wrap tool chain, replace UI |
| Stat capture | working | Keep in Dolphin |
| Stat upload | working | Move into launcher, rotate token |
| Online client | experimental | Rebuild |
| Online server | experimental | Harvest and rebuild |
| JSGME and MODS | distributable | Retire, migrate content |
| Controller profiles | working but misleading | Replace with SDL3 service |
| NSR_UpdateChecker | distributable | Retire |
| Xbox Dev Mode package | distributable | Absorb into PC-side wizard |
| ReShade and NSRShaderlessMenu | working | Keep, Windows only |
| League site and ingest endpoint | working | Out of scope, keep contract |
