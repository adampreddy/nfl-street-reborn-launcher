# NFL Street Reborn Dolphin Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the fork into NFL Street Reborn Dolphin: renamed exe and window, uploader and marker files gone, every game captured into `User\GameStats\Online` or `Offline` with the identity block, the dev folder cleaned and renamed, the runtime published as 1.1.0, and the registry-to-teams mapping measured and baked as 1.1.1.

**Architecture:** Source changes are confined to the Reborn modules (`NflsStatTracker`, `NflsPlayHide`), branding points (titles, version, resource, project name) and the shipped per-game INIs. The build path is the existing `Tools\verify-reborn-windows-build.ps1 -Build`. The dev folder becomes the source for the runtime component through the launcher's `build.ps1 -RuntimeOnly` allowlist and `rebornctl publish`. Nothing is deleted from the dev folder; everything retired moves to `_retired_2026-09-08\`.

**Tech Stack:** Visual Studio 2022 MSBuild (x64 Release), C++20, Qt 6, PowerShell 5.1, Python 3 for the file-move script, `rebornctl` and `rclone` for publishing.

**Spec:** `docs/superpowers/specs/2026-09-08-reborn-dolphin-and-stats-pipeline-design.md` sections 5, 10, 11 steps 5 and 6, 12 (Dolphin), 15.

## Global Constraints

- Fork: `C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\dolphin-exgl` (git). Dev folder: `C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN`, renamed to `NFL Street Reborn Dolphin` in Task 7.
- The exe name is `NFL Street Reborn Dolphin.exe`. The window title and product name are `NFL Street Reborn Dolphin`. Runtime version constant `1.1.0`, then `1.1.1` after the measurement.
- Read-only discipline: the tracker and PlayHide read guest RAM and write nothing to emulated memory. Every guest read uses `Memory::Read_*`.
- No marker files. Debug and test behavior exists only behind `NFLS_DEBUG_OSD` and `NFLS_PLAYHIDE_TEST` preprocessor defines, off in shipping builds.
- Nothing in the dev folder is deleted. Moves only, to `DOLPHIN ONLINE TEST\_retired_2026-09-08\`, keeping relative paths. Never restore a `.bak` over a live file.
- Zero error popups. A build that shows any popup on boot is not shippable.
- Dolphin rewrites `Dolphin.ini`, `GFX.ini` and `Logger.ini` from memory on a clean exit: edit those files only while Dolphin is closed.
- Multi-line scripts are written with the Write tool as `.ps1` or `.py` files, never heredocs.
- The build sync law is untouched by this plan: no DOL or ISO changes here (phase 2 owns those).
- US English. No em dashes.

---

## File structure

```
dolphin-exgl/
  Source/Core/Common/NflsVersion.h                 NEW: NFLS_RUNTIME_VERSION, NFLS_PRODUCT_NAME
  Source/Core/Common/Version.cpp                   scm_rev_str from NflsVersion.h
  Source/Core/DolphinQt/MainWindow.cpp             two titles
  Source/Core/DolphinQt/RenderWidget.cpp           one title
  Source/Core/DolphinQt/DolphinQt.vcxproj          TargetName = NFL Street Reborn Dolphin
  Source/Core/DolphinQt/CMakeLists.txt             OUTPUT_NAME twice
  Source/Core/DolphinQt/DolphinQt.rc               icon path, VERSIONINFO
  Installer/NFLStreetReborn.ico                    NEW: the Reborn icon (from the dev folder's QuickIcon.ico)
  Source/Core/Core/NflsStatUpload.{h,cpp}          DELETED
  Source/Core/Core/CMakeLists.txt                  drop the two NflsStatUpload lines
  Source/Core/DolphinLib.props                     drop the two NflsStatUpload lines
  Source/Core/Core/NflsPlayHide.{h,cpp}            LocalPad, LocalPads(), ClearLocalPads(); marker files to defines
  Source/Core/Core/NflsStatTracker.{h,cpp}         GameStats folders, no gate, reborn_match block, debug define
  Data/Sys/GameSettings/GN7.ini                    optimization and compatibility block
  Data/Sys/GameSettings/FR4N31.ini, CR4E12.ini     NEW, same block
  Tools/verify-reborn-windows-build.ps1            exe path
  Tools/retire_dev_folder.py                       NEW: the move list, then the rename
```

---

### Task 1: Commit what shipped, retire the source-tree backups

**Files:**
- Modify (commit as-is): `Source/Core/VideoCommon/NflsUiSplit.cpp`
- Move: `Source/Core/VideoCommon/NflsUiSplit.cpp.bak_*` (6 files)

- [ ] **Step 1: Confirm the working tree state**

Run: `cd "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\dolphin-exgl"; git status --short`
Expected: ` M Source/Core/VideoCommon/NflsUiSplit.cpp` and six `??` `.bak_*` files, nothing else.

- [ ] **Step 2: Commit the shipped source**

```bash
git add Source/Core/VideoCommon/NflsUiSplit.cpp
git commit -m "NflsUiSplit: the source of the 2026-09-04 shipped build, committed as found"
```

- [ ] **Step 3: Move the six backups out of the tree**

```powershell
$dst = "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\_retired_2026-09-08\dolphin-exgl\Source\Core\VideoCommon"
New-Item -ItemType Directory $dst -Force | Out-Null
Get-ChildItem "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\dolphin-exgl\Source\Core\VideoCommon\NflsUiSplit.cpp.bak_*" | Move-Item -Destination $dst
git -C "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\dolphin-exgl" status --short
```
Expected: `git status` prints nothing.

---

### Task 2: Version, titles, exe name, icon

**Files:**
- Create: `Source/Core/Common/NflsVersion.h`, `Installer/NFLStreetReborn.ico`
- Modify: `Source/Core/Common/Version.cpp:28`, `Source/Core/DolphinQt/MainWindow.cpp:213,1135`, `Source/Core/DolphinQt/RenderWidget.cpp:44`, `Source/Core/DolphinQt/DolphinQt.vcxproj`, `Source/Core/DolphinQt/CMakeLists.txt:417,543`, `Source/Core/DolphinQt/DolphinQt.rc`, `Tools/verify-reborn-windows-build.ps1`

- [ ] **Step 1: NflsVersion.h**

```cpp
// Copyright 2026 NFL Street Online
// SPDX-License-Identifier: GPL-2.0-or-later
//
// The Reborn runtime's own identity. Bumped with every runtime component publish. The version is
// emitted into every capture's reborn_match block so the backend knows which build wrote it, and
// it feeds the window title and the NetPlay version string, so two different runtime versions
// refuse to connect to each other, which is what a forced update wants.

#pragma once

#define NFLS_PRODUCT_NAME "NFL Street Reborn Dolphin"
#define NFLS_RUNTIME_VERSION "1.1.0"
```

- [ ] **Step 2: Version.cpp**

Add `#include "Common/NflsVersion.h"` with the other includes. Replace
`static const std::string scm_rev_str = "NFL Street Reborn Dolphin (BETA)"` with
`static const std::string scm_rev_str = NFLS_PRODUCT_NAME " " NFLS_RUNTIME_VERSION` (the `#ifdef __INTEL_COMPILER` / `BUILD_TYPE_STR` continuation stays).

- [ ] **Step 3: Titles**

In `MainWindow.cpp` (both sites) and `RenderWidget.cpp`, add `#include "Common/NflsVersion.h"` and replace `QStringLiteral("Franchise League Dolphin (BETA)")` with `QString::fromUtf8(NFLS_PRODUCT_NAME)`.

Run: `Select-String -Path Source\Core -Pattern "Franchise League" -Recurse`
Expected: no matches.

- [ ] **Step 4: Exe name for MSBuild and CMake**

In `DolphinQt.vcxproj`, insert immediately after the line `<Import Project="$(VCTargetsPath)\Microsoft.Cpp.props" />` (line 11):
```xml
  <PropertyGroup Label="Reborn">
    <TargetName>NFL Street Reborn Dolphin</TargetName>
  </PropertyGroup>
```
The solution's project name stays `Dolphin`, so `-t:Dolphin` in the build script keeps working. In `CMakeLists.txt` change both `OUTPUT_NAME      Dolphin` (line 417) and `OUTPUT_NAME Dolphin` (line 543) to `"NFL Street Reborn Dolphin"`.

- [ ] **Step 5: Icon and version resource**

```powershell
Copy-Item "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN\QuickIcon.ico" "Installer\NFLStreetReborn.ico"
```
Replace the whole of `DolphinQt.rc` with:
```
// Microsoft Visual C++ generated resource script.
//
#include "resource.h"
IDI_ICON1               ICON                    "..\\..\\..\\Installer\\NFLStreetReborn.ico"
"dolphin"               ICON                    "..\\..\\..\\Installer\\NFLStreetReborn.ico"

1 VERSIONINFO
FILEVERSION 1,1,0,0
PRODUCTVERSION 1,1,0,0
FILEOS 0x40004L
FILETYPE 0x1L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "NFL Street Reborn"
      VALUE "FileDescription", "NFL Street Reborn Dolphin"
      VALUE "FileVersion", "1.1.0.0"
      VALUE "InternalName", "NFL Street Reborn Dolphin"
      VALUE "LegalCopyright", "GPL-2.0-or-later. Based on Dolphin Emulator."
      VALUE "OriginalFilename", "NFL Street Reborn Dolphin.exe"
      VALUE "ProductName", "NFL Street Reborn Dolphin"
      VALUE "ProductVersion", "1.1.0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
```
If `QuickIcon.ico` turns out not to be the Reborn mark when viewed, use the launcher's icon source instead (`Desktop\Reborn Installer\NFL Street Reborn Launcher\crates\reborn-ui\icons\icon.ico` or the nearest `.ico` there) and note which file was used in the commit message.

- [ ] **Step 6: Build script**

In `Tools\verify-reborn-windows-build.ps1` change `$exe = Join-Path $repo "Binary\$Platform\Dolphin.exe"` to `"Binary\$Platform\NFL Street Reborn Dolphin.exe"` and the `no Dolphin.exe` message to match.

- [ ] **Step 7: Commit**

```bash
git add Source/Core/Common/NflsVersion.h Source/Core/Common/Version.cpp Source/Core/DolphinQt/MainWindow.cpp Source/Core/DolphinQt/RenderWidget.cpp Source/Core/DolphinQt/DolphinQt.vcxproj Source/Core/DolphinQt/CMakeLists.txt Source/Core/DolphinQt/DolphinQt.rc Installer/NFLStreetReborn.ico Tools/verify-reborn-windows-build.ps1
git commit -m "NFL Street Reborn Dolphin: exe name, titles, version resource, icon, runtime version 1.1.0"
```

---

### Task 3: Remove the uploader

**Files:**
- Delete: `Source/Core/Core/NflsStatUpload.h`, `Source/Core/Core/NflsStatUpload.cpp`
- Modify: `Source/Core/Core/CMakeLists.txt:446-447`, `Source/Core/DolphinLib.props:403,1032`, `Source/Core/Core/NflsStatTracker.cpp`

- [ ] **Step 1: Delete the module and its build lines**

```bash
git rm -q Source/Core/Core/NflsStatUpload.h Source/Core/Core/NflsStatUpload.cpp
```
Remove the lines `  NflsStatUpload.cpp` and `  NflsStatUpload.h` from `Source/Core/Core/CMakeLists.txt`, and `<ClInclude Include="Core\NflsStatUpload.h" />` and `<ClCompile Include="Core\NflsStatUpload.cpp" />` from `DolphinLib.props`.

- [ ] **Step 2: Cut the calls out of the tracker**

In `NflsStatTracker.cpp`:
- delete `#include "Core/NflsStatUpload.h"`;
- in `WriteJson`, delete the comment block and the line `NflsStatUpload::Nudge();` at the end;
- in `Reset()`, delete the comment and the two lines `if (s_wrote && !s_cap.path.empty())` / `NflsStatUpload::MarkFinal(s_cap.path);`;
- in `Update()`, delete the comment and the line `NflsStatUpload::Start();`.

Run: `Select-String -Path Source -Pattern "NflsStatUpload" -Recurse`
Expected: no matches.

- [ ] **Step 3: Commit**

```bash
git add Source/Core/Core/CMakeLists.txt Source/Core/DolphinLib.props Source/Core/Core/NflsStatTracker.cpp
git commit -m "Remove the in-Dolphin uploader and the token compiled into it; the launcher sends results"
```

---

### Task 4: PlayHide publishes the local pads; marker files become defines

**Files:**
- Modify: `Source/Core/Core/NflsPlayHide.h`, `Source/Core/Core/NflsPlayHide.cpp`

**Interfaces:**
- Produces:
```cpp
namespace NflsPlayHide {
struct LocalPad { int pad; int registry_team; };
// The pads this machine controlled in the current match, latched at the play-call state. CPU thread only.
const std::vector<LocalPad>& LocalPads();
// Forget the latch (the tracker calls this when a match unloads or a new game starts).
void ClearLocalPads();
}
```

- [ ] **Step 1: Header**

Add `#include <vector>` and the declarations above to `NflsPlayHide.h` inside the namespace.

- [ ] **Step 2: Latch the pads in Update()**

At the top of the anonymous namespace add:
```cpp
// Which ports this machine holds, and each one's registry team, latched while the play-call
// card is up. Written and read on the CPU thread only (both callers run from
// PatchEngine::ApplyFramePatches), so no lock. A pad appears once; a re-read that disagrees
// replaces the earlier value, so a mid-match team read that was garbage does not stick.
std::vector<LocalPad> s_local_pads;

void LatchPad(int pad, int team)
{
  for (auto& lp : s_local_pads)
  {
    if (lp.pad == pad)
    {
      lp.registry_team = team;
      return;
    }
  }
  s_local_pads.push_back(LocalPad{pad, team});
}
```
Add the two public functions after `MaskSide()`:
```cpp
const std::vector<LocalPad>& LocalPads()
{
  return s_local_pads;
}

void ClearLocalPads()
{
  s_local_pads.clear();
}
```
Replace the marker-file statics with defines. Delete the two `static const bool test_mode = File::Exists(...)` and `static const bool debug_mode = File::Exists(...)` declarations and write instead, near the top of `Update()`:
```cpp
#ifdef NFLS_PLAYHIDE_TEST
  constexpr bool test_mode = true;
#else
  constexpr bool test_mode = false;
#endif
#ifdef NFLS_PLAYHIDE_DEBUG
  constexpr bool debug_mode = true;
#else
  constexpr bool debug_mode = false;
#endif
```
Then, inside `Update()`, replace the block `if (gated && substate == 5u && paused == 0u && (test_mode || online)) { ... }` with two blocks: the latch, which runs at every play call, and the mask, which keeps its original condition:
```cpp
  if (gated && substate == 5u && paused == 0u)
  {
    // LATCH the pads this machine controls and each one's registry team. Under NetPlay a pad is
    // local when the pad map says so; offline, when a GameCube controller is configured on that
    // port. The tracker copies this list into every capture, and it is how the backend knows
    // which human held which team without ever reading a team name.
    const u32 reghdr = Memory::Read_U32(0x803EAB60u);
    const u32 rec = InMem1(reghdr) ? Memory::Read_U32(reghdr) : 0;
    if (InMem1(rec))
    {
      for (int p = 0; p < 4; ++p)
      {
        const bool local =
            np ? (SerialInterface::CSIDevice_GCController::NetPlay_InGamePadToLocalPad(p) < 4) :
                 (Config::Get(Config::GetInfoForSIDevice(p)) == SerialInterface::SIDEVICE_GC_CONTROLLER);
        if (!local)
          continue;
        const u8 team = Memory::Read_U8(rec + static_cast<u32>(p) * 0xCu);
        if (team == 0 || team == 1)
          LatchPad(p, team);
      }
    }
  }

  if (gated && substate == 5u && paused == 0u && (test_mode || online))
  {
    // ...the existing mask body, unchanged...
  }
```
Add `#include "Core/Config/MainSettings.h"` and `#include "Core/HW/SI/SI_Device.h"` beside the existing includes. Before building, confirm the two names exist in this tree: `Select-String -Path Source\Core\Core\Config\MainSettings.h -Pattern "GetInfoForSIDevice"` and `Select-String -Path Source\Core\Core\HW\SI\SI_Device.h -Pattern "SIDEVICE_GC_CONTROLLER"`. If `GetInfoForSIDevice` is absent, use `SConfig::GetInstance().m_SIDevice[p]` instead (the older accessor) and say so in the commit message.

Wrap the `if (debug_mode) { ... }` OSD block so it compiles away: `if constexpr (debug_mode) { ... }`.

- [ ] **Step 3: Commit**

```bash
git add Source/Core/Core/NflsPlayHide.h Source/Core/Core/NflsPlayHide.cpp
git commit -m "PlayHide: latch this machine's pads and their registry teams for the tracker; marker files become build defines"
```

---

### Task 5: Tracker: GameStats folders, capture always on, the identity block

**Files:**
- Modify: `Source/Core/Core/NflsStatTracker.cpp`, `Source/Core/Core/NflsStatTracker.h`

- [ ] **Step 1: Includes and the capture struct**

Add `#include "Common/NflsVersion.h"` and `#include "Core/NflsPlayHide.h"`. In `struct Capture` add:
```cpp
  // Latched at the FIRST write so a rewrite never changes folders mid-game.
  bool netplay_latched = false;
  bool netplay = false;
```

- [ ] **Step 2: Folder choice in WriteJson**

Replace `const std::string dir = File::GetUserPath(D_USER_IDX) + "LeagueGameStats" DIR_SEP;` with:
```cpp
  // Two folders, one per mode. Dolphin decides from its own NetPlay state, and the launcher
  // records which folder a capture came from; the backend holds a game where that folder and
  // the netplay_running flag below disagree. Latched at the first write, because a rewrite must
  // land in the same folder as the first one.
  if (!cap.netplay_latched)
  {
    cap.netplay = NetPlay::IsNetPlayRunning();
    cap.netplay_latched = true;
  }
  const std::string dir = File::GetUserPath(D_USER_IDX) + "GameStats" DIR_SEP +
                          (cap.netplay ? "Online" : "Offline") + DIR_SEP;
```
(`File::CreateFullPath(dir)` already follows and creates both levels.)

- [ ] **Step 3: The identity block**

Immediately after `body += fmt::format("  \"per_player_matched\": {},\n", cap.matched);` add:
```cpp
  // IDENTITY BLOCK, outside the hashed set (teams/style/team_totals/players), so both NetPlay
  // clients of one game still hash identically while each says which pads IT held. Who a pad
  // belongs to is the launcher's business (Discord sign-in); this only says pad and team.
  body += "  \"reborn_match\": {\n";
  body += "    \"schema\": \"nsr.reborn_match.v1\",\n";
  body += fmt::format("    \"netplay_running\": {},\n", cap.netplay ? "true" : "false");
  body += fmt::format("    \"game_id\": \"{}\",\n", JsonEscape(SConfig::GetInstance().GetGameID()));
  body += fmt::format("    \"runtime_version\": \"{}\",\n", NFLS_RUNTIME_VERSION);
  body += "    \"local_pads\": [";
  {
    const auto& pads = NflsPlayHide::LocalPads();
    for (size_t i = 0; i < pads.size(); ++i)
    {
      body += fmt::format("{}{{\"pad\": {}, \"registry_team\": {}}}", i ? ", " : "", pads[i].pad,
                          pads[i].registry_team);
    }
  }
  body += "]\n";
  // local_team_index is deliberately ABSENT until the registry-team to teams[] index mapping has
  // been measured (Task 9). A consumer treats absence as unknown, never as 0.
  body += "  },\n";
```

- [ ] **Step 4: Capture always on; debug OSD behind a define**

Delete the `NetplayGated()` function and its comment. In `Update()` change `if (!GameGated() || !NetplayGated())` to `if (!GameGated())`. Replace
```cpp
    static const bool debug_on =
        File::Exists(File::GetUserPath(D_USER_IDX) + "nsr_debug.txt") || File::Exists("nsr_debug.txt");
```
with
```cpp
#ifdef NFLS_DEBUG_OSD
    constexpr bool debug_on = true;
#else
    constexpr bool debug_on = false;
#endif
```
and make the following `if (debug_on && ...)` an `if constexpr (debug_on)` wrapping the existing `if ((s_diag++ % 4) == 0)` body. In `Reset()`, after `s_cap = Capture();`, add `NflsPlayHide::ClearLocalPads();`.

Update the header comment in `NflsStatTracker.h`: replace the two-screens paragraph's ending with one more line: `Output: User\GameStats\Online or User\GameStats\Offline, one file per game, rewritten as the decode improves; the launcher collects it after Dolphin exits.` Remove any mention of NetPlay-only capture.

- [ ] **Step 5: Confirm no marker file is read anywhere**

Run: `Select-String -Path Source\Core -Pattern "nsr_track_offline|nsr_debug|playhide_test|playhide_debug|nsr_ingest" -Recurse`
Expected: no matches.

- [ ] **Step 6: Commit**

```bash
git add Source/Core/Core/NflsStatTracker.cpp Source/Core/Core/NflsStatTracker.h
git commit -m "Tracker: every game captured into GameStats/Online or Offline with the reborn_match identity block; no marker files"
```

---

### Task 6: Shipped per-game defaults

**Files:**
- Modify: `Data/Sys/GameSettings/GN7.ini`
- Create: `Data/Sys/GameSettings/FR4N31.ini`, `Data/Sys/GameSettings/CR4E12.ini`

- [ ] **Step 1: GN7.ini**

Replace the file with:
```ini
# GN7E69, GN7P69 - NFL STREET 2
#
# The NFL Street Reborn defaults. The launcher writes the same keys into the player's
# User\GameSettings\GN7E69.ini at every launch; this file is what a hand-run Dolphin gets, so
# the two never differ. Kept on purpose: SyncGPU (the 2026-09 0 FPS freeze fix),
# CacheHiresTextures (the HD pack prefetch, 1.5 GB after the BC7 bake), IR 3 and MSAA 1 (the
# ReShade depth filter is tuned to exactly 1920x1584), EFBAccessEnable (portraits bake black
# without it), ImmediateXFBEnable (the reference install's choice).

[Core]
SyncGPU = True

[Video_Settings]
InternalResolution = 3
MSAA = 1
SSAA = False
HiresTextures = True
CacheHiresTextures = True
DumpTextures = False
DumpBaseTextures = False
DumpMipTextures = False

[Video_Enhancements]
ArbitraryMipmapDetection = False

[Video_Hacks]
EFBAccessEnable = True
DeferEFBCopies = False
EFBToTextureEnable = True
ImmediateXFBEnable = True
XFBToTextureEnable = True

[OnLoad]
[OnFrame]
[ActionReplay]
```

- [ ] **Step 2: The two twins**

Copy `GN7.ini` to `FR4N31.ini` and `CR4E12.ini`, changing only the first comment line to `# FR4N31 - NFL STREET 2 (Reborn online build). Mirrors GN7.ini.` and `# CR4E12 - NFL STREET 2 (Reborn Creation League build). Mirrors GN7.ini.` These ids do not match `GN7*`, so without their own files they would run with no per-game overrides at all.

- [ ] **Step 3: Commit**

```bash
git add Data/Sys/GameSettings/GN7.ini Data/Sys/GameSettings/FR4N31.ini Data/Sys/GameSettings/CR4E12.ini
git commit -m "Shipped per-game defaults: the Reborn compatibility and optimization block for all three disc ids"
```

---

### Task 7: Build, then the dev folder: rename, retire, fix

**Files:**
- Create: `Tools/retire_dev_folder.py` (in the fork's `Tools\`)
- Modify (dev folder, while Dolphin is closed): `User\Config\Dolphin.ini` (`DefaultISO`), `User\Config\Logger.ini`

- [ ] **Step 1: Build**

Close every Dolphin. Run: `pwsh -File Tools\verify-reborn-windows-build.ps1 -Build` (or `powershell -File ...` if `pwsh` is absent).
Expected: MSBuild finishes (its exit code may be 1 from the pre-existing mGBA `threading.h` failure; judge by the artifact), the script reports `Binary\x64\NFL Street Reborn Dolphin.exe` built within the last minutes, and its drift checks pass. If the mGBA failure stops the Dolphin target itself, run MSBuild with `-p:BuildProjectReferences=true -t:Dolphin` as the script does and read the error list for anything in `Nfls*` or `DolphinQt`.

- [ ] **Step 2: Write Tools/retire_dev_folder.py**

```python
"""Retire dev leftovers from the Reborn Dolphin dev folder, then rename the folder.

Moves only. Every path listed goes to _retired_2026-09-08\CREATION LEAGUE AND NEW DOLPHIN\<same relative path>.
Refuses to run while any Dolphin process is alive. Prints every move. Run:
    py -3 Tools\\retire_dev_folder.py            (dry run)
    py -3 Tools\\retire_dev_folder.py --go       (move + rename)
"""
import glob, os, shutil, subprocess, sys

ROOT = r"C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST"
SRC = os.path.join(ROOT, "CREATION LEAGUE AND NEW DOLPHIN")
DST = os.path.join(ROOT, "_retired_2026-09-08", "CREATION LEAGUE AND NEW DOLPHIN")
NEW_NAME = os.path.join(ROOT, "NFL Street Reborn Dolphin")

PATTERNS = [
    "Dolphin.exe", "Dolphin.exe.bak_*",
    "NSRShaderlessMenu.addon64.bak_*", "NSRShaderlessMenu.ini.bak_*", "NSRShaderlessMenu.log",
    "ReShade.ini.bak_*", "ReShade.log", "ReShade.log1", "ReShade.log2",
    "ReshadeEffectShaderToggler.addon64.disabled_*",
    r"reshade-presets\stages.bak_*",
    "Game Summary.html", "None", "play.txt",
    r"Games\*.iso", r"Games\*.png",
    r"User\Config\*.bak_*", r"User\GameSettings\*.bak_*",
    r"User\Logs\dolphin.log", r"User\NflsUiSplit.log", r"User\Dump\*",
    r"User\LeagueGameStats",
    r"User\Load\Textures\CR4E12", r"User\Load\Textures\CR4E12 - PS",
    r"User\Load\Textures\FR4N31", r"User\Load\Textures\FR4N31 - PS",
    r"User\Load\Textures\GN7E69.before-linking.*",
    r"User\Load\Textures\TO PICK YOUR CONTROLS*.txt",
]

def dolphin_running():
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq Dolphin.exe"], capture_output=True, text=True).stdout
    out2 = subprocess.run(["tasklist", "/FI", "IMAGENAME eq NFL Street Reborn Dolphin.exe"], capture_output=True, text=True).stdout
    return "Dolphin.exe" in out or "NFL Street Reborn Dolphin.exe" in out2

def main(go):
    if dolphin_running():
        sys.exit("Dolphin is running. Close it first.")
    if not os.path.isdir(SRC):
        sys.exit(f"missing {SRC} (already renamed?)")
    moves = []
    for pat in PATTERNS:
        for p in glob.glob(os.path.join(SRC, pat)):
            rel = os.path.relpath(p, SRC)
            # The texture junction must be moved as a link, never followed.
            moves.append((p, os.path.join(DST, rel)))
    for src, dst in moves:
        print(("MOVE " if go else "would move ") + os.path.relpath(src, SRC))
        if go:
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            shutil.move(src, dst)
    print(f"{len(moves)} item(s)")
    if go:
        os.rename(SRC, NEW_NAME)
        print(f"renamed -> {NEW_NAME}")

if __name__ == "__main__":
    main("--go" in sys.argv)
```
`Games\*.iso` covers the four images (5.7 GB); `Dolphin.exe` is the old build, replaced in step 4. `User\Load\Textures\GN7E69` is a junction and is NOT in the list: it stays.

- [ ] **Step 3: Dry run, then go**

Run: `py -3 Tools\retire_dev_folder.py`
Expected: the list of relative paths, with nothing outside the patterns above. Then `py -3 Tools\retire_dev_folder.py --go` and confirm `renamed -> ...NFL Street Reborn Dolphin`.

- [ ] **Step 4: Deploy the build into the renamed folder**

```powershell
$dev = "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\NFL Street Reborn Dolphin"
Copy-Item "Binary\x64\NFL Street Reborn Dolphin.exe" $dev
Copy-Item "Data\Sys\GameSettings\GN7.ini","Data\Sys\GameSettings\FR4N31.ini","Data\Sys\GameSettings\CR4E12.ini" "$dev\Sys\GameSettings\" -Force
Get-ChildItem $dev -Filter "*.exe"
```
Expected: `DSPTool.exe`, `DolphinTool.exe`, `NFL Street Reborn Dolphin.exe`, and no `Dolphin.exe`.

- [ ] **Step 5: Fix the default image and quiet the logger** (Dolphin closed)

Write `Tools\fix_dev_config.py`:
```python
import re
dev = r"C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\NFL Street Reborn Dolphin\User\Config"
ini = open(dev + r"\Dolphin.ini", encoding="utf-8").read()
new = re.sub(r"^DefaultISO = .*$", r"DefaultISO = C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn\\Games\\NFL Street 2 FIELDS 12-19.iso", ini, flags=re.M)
assert new != ini or "FIELDS 12-19" in ini
open(dev + r"\Dolphin.ini", "w", encoding="utf-8", newline="\n").write(new)
log = open(dev + r"\Logger.ini", encoding="utf-8").read()
log = re.sub(r"^WriteToFile = .*$", "WriteToFile = False", log, flags=re.M)
log = re.sub(r"^Verbosity = .*$", "Verbosity = 1", log, flags=re.M)
open(dev + r"\Logger.ini", "w", encoding="utf-8", newline="\n").write(log)
print(re.search(r"^DefaultISO = .*$", new, flags=re.M).group(0))
print([l for l in log.splitlines() if l.startswith(("WriteToFile", "Verbosity"))])
```
Run it. Expected: the `DefaultISO` line names `NFL Street 2 FIELDS 12-19.iso` and the two logger lines read `False` and `1`.

- [ ] **Step 6: The launcher's settings**

Run: `Select-String -Path "$env:APPDATA\NFL Street Reborn Launcher\config\settings.json" -Pattern "CREATION LEAGUE"`
Expected: no match. If there is one, edit that path to the renamed folder in the same file.

- [ ] **Step 7: Commit the tools**

```bash
git add Tools/retire_dev_folder.py Tools/fix_dev_config.py
git commit -m "Tools: retire the dev folder's leftovers and fix its default image and logger"
```

---

### Task 8: Boot tests, observed

- [ ] **Step 1: Title, icon, no popups**

Start `NFL Street Reborn Dolphin.exe` from the renamed folder by double-click (portable, own User). Observe: window title `NFL Street Reborn Dolphin`, the Reborn icon in the title bar and taskbar, no message box of any kind. `Get-ItemProperty ".\NFL Street Reborn Dolphin.exe" | Select -Expand VersionInfo` shows `ProductName NFL Street Reborn Dolphin`, `FileVersion 1.1.0.0`.

- [ ] **Step 2: One offline game**

Play a quick game against the CPU to the Game Stats and Player of the Game screens. Observe `User\GameStats\Offline\*.json` appears during the game. Open it: `reborn_match.netplay_running` is `false`, `runtime_version` is `1.1.0`, `local_pads` has one entry with `pad` 0 and `registry_team` 0 or 1, `players` has 14 entries, `team_totals` present. Nothing was written under `LeagueGameStats`.

- [ ] **Step 3: Two instances, one NetPlay game (this is also the measurement)**

Make a second user folder: `Copy-Item "<dev>\User" "<dev>\User-B" -Recurse -Exclude Load,Dump,Cache` then start two instances: `& ".\NFL Street Reborn Dolphin.exe" -u ".\User"` and `& ".\NFL Street Reborn Dolphin.exe" -u ".\User-B"`. In instance A: Tools, Start NetPlay, Host, direct connection, port 2626, game `NFL Street 2 FIELDS 12-19.iso`. In instance B: Tools, Start NetPlay, Connect, `127.0.0.1:2626`. Assign pad 1 to A and pad 2 to B, start. Pick clearly different NFL teams and WRITE DOWN which instance picked which, and which name appears on the left of the in-game score bug. Play to the Game Stats screen.

Observe: `User\GameStats\Online\*.json` in A and `User-B\GameStats\Online\*.json` in B, `netplay_running` true in both, both files hash to the same wire hash (run the backend repo's `node -e` with `wireHash` on both, or the launcher's `wire_hash_of` test binary), and the two `local_pads[0].registry_team` values differ. Record, for each instance: the team it picked, that team's index in `teams[]` of its own capture, and its `registry_team`. Repeat once with the picks swapped. Both runs must give the same mapping from `registry_team` to `teams[]` index; if they disagree, the measurement is not done and Task 9 does not run.

- [ ] **Step 4: Record**

Append the two runs' table (instance, picked team, teams index, registry_team) and the derived mapping to `docs/NEXT_STEPS.md` in the launcher repo under `Registry team to teams[] index, measured <date>`.

---

### Task 9: Bake the measurement, 1.1.1

**Files:**
- Modify: `Source/Core/Core/NflsStatTracker.cpp`, `Source/Core/Common/NflsVersion.h`, `Source/Core/DolphinQt/DolphinQt.rc`

- [ ] **Step 1: The constant**

In `NflsStatTracker.cpp`'s constants block add, with the measured values in place of the placeholders `A` and `B` (each 0 or 1, different):
```cpp
// MEASURED <date> on two instances, twice with the picks swapped (docs/NEXT_STEPS.md in the
// launcher repo): the play-call registry's team value -> index into teams[] as this tracker
// renders them. Index 0 of this array answers for registry team 0.
constexpr int kRegistryTeamToTeamsIndex[2] = {A, B};
```
In `WriteJson`, replace the comment about `local_team_index` being absent with:
```cpp
  {
    const auto& pads = NflsPlayHide::LocalPads();
    if (!pads.empty() && (pads[0].registry_team == 0 || pads[0].registry_team == 1))
      body += fmt::format(",\n    \"local_team_index\": {}\n", kRegistryTeamToTeamsIndex[pads[0].registry_team]);
    else
      body += "\n";
  }
```
and change the preceding `body += "]\n";` to `body += "]";` so the JSON stays valid in both branches. (`pads` is sorted by insertion order; `LatchPad` runs `p` from 0 to 3, so `pads[0]` is the lowest pad.)

- [ ] **Step 2: Version**

`NFLS_RUNTIME_VERSION` to `"1.1.1"`; `DolphinQt.rc` `FILEVERSION`/`PRODUCTVERSION` to `1,1,1,0` and the two string versions to `1.1.1.0` and `1.1.1`.

- [ ] **Step 3: Build, redeploy, one more offline game**

Repeat Task 7 step 1 and step 4 (copy the exe). Play one offline game. Observe `local_team_index` present and equal to the index of the team you picked in `teams[]`.

- [ ] **Step 4: Commit**

```bash
git add Source/Core/Core/NflsStatTracker.cpp Source/Core/Common/NflsVersion.h Source/Core/DolphinQt/DolphinQt.rc
git commit -m "Emit local_team_index from the measured registry mapping; runtime 1.1.1"
```

- [ ] **Step 5: Backfill on the backend**

`curl -s -X POST https://api.nflstreetreborn.com/v1/admin/backfill -H "x-admin-token: <ADMIN_TOKEN>" -H "Content-Type: application/json" -d "{\"registry_team_to_index\":{\"0\":A,\"1\":B}}"` with the measured values. Expected: `{"ok":true,"updated":N}`.

---

### Task 10: Publish the runtime

- [ ] **Step 1: Gate**

The launcher update 3.1.0 must already be on the bucket (launcher plan Task 11 step 5), because only it understands the renamed exe and the new folders.

- [ ] **Step 2: Stage by allowlist**

From the launcher repo: `powershell -File installer\windows\build.ps1 -RuntimeOnly -Version 3.1.0 -Runtime "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\NFL Street Reborn Dolphin"`
Expected: `Runtime staged alone at ...\target\installer-payload-runtime\runtime`, and the `Left out of the runtime:` line lists only `User`, `portable.txt`, `Dolphin.exe.bak`-style names if any remain, and `Games` is absent. Then `cargo run -p reborn-cli --release -- runtime audit "target\installer-payload-runtime\runtime"` if that subcommand exists, else rely on `RuntimePackager::audit` through the publish step's own check.

- [ ] **Step 3: Publish component and release**

From the launcher repo root, with the release exe built (`cargo build --workspace --release`):
```
target\release\rebornctl.exe publish component --root "target\installer-payload-runtime\runtime" --component runtime --version 1.1.1 --sequence 5 --origin "C:\Reborn Origin" --apply
```
Then read the current pins: `Get-Content "C:\Reborn Origin\release.json"` (as of 2026-09-08 evening the live release is sequence 4 with runtime 1.0.2, published by another session; the next sequence is 5, and if release.json shows a higher sequence use the next one after that) and publish the release with every other component kept at its current version and `runtime=1.1.1`, following the exact `publish release` invocation recorded in `docs/RUNNING_UPDATES.md` (section "Publish a release"). Upload blobs first, then `release.json` and `release.json.sig` last, with the rclone command recorded there (`rclone.exe` under `%LOCALAPPDATA%\Microsoft\WinGet\Packages\Rclone.Rclone_*\rclone-*\`, conf `C:\Users\Adam\Reborn Keys\rclone.conf`, remote `r2:reborn-content`).

- [ ] **Step 4: Prove it from a scratch root**

`target\release\rebornctl.exe update content --data-root C:\Temp\reborn-scratch --apply` against the live origin. Expected: runtime 1.1.1 installs, `<scratch>\runtime\NFL Street Reborn Dolphin.exe` exists, and no `Dolphin.exe` beside it (retired into `prior\`).

- [ ] **Step 5: Record**

Add to `docs/NEXT_STEPS.md`: runtime 1.1.1 published, the release sequence it landed in, the date, and the scratch-root proof.

---

## Self-review

**Spec coverage.** 5.1 folders: Task 5. 5.2 identity block: Tasks 4, 5, 9. 5.3 gates and removals: Tasks 3, 4, 5. 10.1 rename and brand: Task 2 (and the shipped INIs in Task 6). 10.2: Tasks 3 to 5. 10.3 optimization keys in `Sys\GameSettings`: Task 6. 10.4 retirement list, default image, logger, settings check: Task 7. 10.5 ship path: Task 10. 11 step 5 and 6: Tasks 10 and 9. 12 Dolphin verification: Task 8. 15 constants: used verbatim in Task 4.

**Placeholders.** `A` and `B` in Task 9 are the measured values from Task 8 by design, and the task says so. Everything else is written out.

**Type consistency.** `NflsPlayHide::LocalPad{pad, registry_team}` and `LocalPads()` (Task 4) are what `WriteJson` reads (Tasks 5, 9). `ClearLocalPads()` is called from `Reset()` (Task 5). `NFLS_RUNTIME_VERSION` and `NFLS_PRODUCT_NAME` (Task 2) are used in Tasks 2 and 5. The exe name `NFL Street Reborn Dolphin.exe` is identical in Tasks 2, 7, 8 and 10 and in the launcher plan's allowlist.
