# System Architecture

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers:** `CLAUDE_START_HERE.md` step 6, the umbrella architecture document
**Depends on:** [`COMPONENT_INVENTORY.md`](../discovery/COMPONENT_INVENTORY.md), [`COMPONENT_MAP.md`](../discovery/COMPONENT_MAP.md), [`PROJECT_QUESTIONNAIRE.md`](../discovery/PROJECT_QUESTIONNAIRE.md)

The protocol requires that every proposal say which existing components it **reuses, wraps, ports, replaces, or retires**. That verdict appears against every element below.

---

## 1. Decisions this architecture is built on

Locked with Adam during discovery. These are constraints, not options.

| Decision | Consequence for architecture |
|---|---|
| **Always ship the finished game image** | No patch engine, no base retention, no differential game delivery. Removes the largest single component from scope and removes differential updates for game content |
| **Music editing embedded, extract on demand** | Python and FFmpeg ship inside the launcher. This is the only reason a scripting runtime is bundled |
| **New backend, split hosting** | Website stays on SiteGround. Matchmaking and real time go to Railway |
| **September 12 is a finished public release, all platforms** | Everything below is scoped against that, with unverifiable platforms flagged |
| **Stock icon set, shear motif, red primary** | Design system settled. See `docs/design/` |
| **Fork preserved, token rotation deferred** | The Dolphin repository stays private until rotation |

---

## 2. Process model

Four processes. The boundaries are what make the product survivable.

```text
  ┌─ A ─────────────────┐  spawn   ┌─ B ──────────────────┐
  │ Launcher            │─────────>│ Supervisor           │
  │ Tauri 2 + webview   │          │ Rust, a few MB       │
  │ React + TypeScript  │          │ Outlives A           │
  │ EXITS during play   │<─────────│ Relaunches A on exit │
  └─────────────────────┘  restart └──────────┬───────────┘
                                              │ direct boot
                                              ▼
                                   ┌─ C ──────────────────┐
                                   │ Reborn Dolphin       │
                                   │ Never shows its      │
                                   │ game list or NetPlay │
                                   │ browser              │
                                   └──────────┬───────────┘
                                              │ writes JSON
                                              ▼
                                   User\LeagueGameStats\*.json
                                              │
  ┌─ D ─────────────────┐                     │ uploaded by A or B,
  │ Backend (Railway)   │<────────────────────┘ never by C
  │ Node + Postgres     │
  └─────────────────────┘
```

**Why A exits rather than minimizes.** Adam asked for minimize with no resource cost. A webview process cannot cost nothing. Serializing UI state, exiting A, and letting a small Rust supervisor watch the game gives the observable behavior at a real cost of a few megabytes.

**Why the supervisor exists separately.** It is the only thing that can honestly report "the game crashed." If A were the parent, a launcher crash would look like a game crash and vice versa. B also owns the stat queue, so a game exiting mid-upload does not lose a capture.

**Why C never uploads.** Today the emulator posts to the league endpoint using a token compiled into the binary, and its gate can be defeated by dropping a text file into `User\`. Moving upload to A and B makes the credential rotatable, the retry queue durable across game crashes, and the gate impossible to defeat by file placement.

| Existing | Verdict |
|---|---|
| `NSR_UpdateChecker.exe`, current entry point | **Retire.** A and B replace it |
| `NflsStatUpload` inside Dolphin | **Retire from C, port the logic to B** |
| `NflsStatTracker` inside Dolphin | **Reuse unchanged.** Capture stays next to game memory |

---

## 3. Technology, and where each choice is load-bearing

| Layer | Choice | Why it is not arbitrary |
|---|---|---|
| Shell | **Tauri 2** | System webview rather than a bundled Chromium. Three platforms from one UI codebase. The blueprint's low-resource requirement rules out Electron |
| UI | **React + TypeScript** | The existing online prototype is already React, so the rebuild has a reference implementation |
| Core | **Rust** | Hashing, atomic file operations, process supervision, downloads, USB enumeration. All of it is failure-prone and all of it benefits from a language that forces error handling |
| Controllers | **SDL3** | Normalized gamepad identity and hot-plug across Windows, Linux, macOS and SteamOS. This is the component that has to actually solve PlayStation |
| Local state | **SQLite** | Match history, mod state, download queue, settings. Survives crashes |
| Secrets | **OS credential vault** | Windows Credential Manager, macOS Keychain, libsecret. Tokens never touch disk in plaintext |
| Music engine | **Bundled Python + FFmpeg** | Forced by the decision to embed music editing. See section 5 |
| Backend | **Node + Postgres on Railway** | The prototype is Node with socket handlers. Railway runs persistent processes and WebSockets; SiteGround shared hosting does not |

### The one uncomfortable choice

Bundling a Python runtime inside a Tauri application is not elegant. It exists solely because `music_editor.py` orchestrates four other Python tools against an extracted game filesystem, and reimplementing the PFDx and `template.dat` writers in Rust risks regressing the any-length-song fix that is already solved.

**Verdict: wrap, do not port, for v1.** Native reimplementation is the follow-up, and it is the single largest quality improvement available after launch.

---

## 4. Filesystem

```text
INSTALL ROOT          installer owns it, replaceable wholesale
  Windows   %LOCALAPPDATA%\Programs\NFL Street Reborn Launcher
  macOS     /Applications/NFL Street Reborn Launcher.app
  Linux     AppImage or Flatpak

DATA ROOT             launcher owns it, survives uninstall by default
  Windows   %APPDATA%\NFL Street Reborn          <- already used today
  macOS     ~/Library/Application Support/NFL Street Reborn
  Linux     $XDG_DATA_HOME/NFL Street Reborn

  library/offline/     shipped offline build          managed
  library/online/      NSRONLINE.iso                  managed
  library/work/        extracted tree, music only      derived, purgeable
  runtime/             platform Reborn Dolphin        managed
  mods/{packages,staging,active}/                     mixed
  music/{sources,packs}/  sources are USER            never delete
  saves/               memory cards                   never delete
  stats/{queue,local.db}                              never delete
  cache/ downloads/ logs/                             purgeable
  backups/ config/                                    preserve
```

**`library/base/` is gone** from the map in discovery. Shipping the finished image means there is no user-provided base to retain.

**`library/work/` is new**, created by the music decision. It holds the extracted game tree, roughly 1.5 GB, created on first music edit and purgeable afterwards.

**The two rules that matter:**

1. Install root and data root are separate, so the uninstaller can never reach saves. The current installer runs `DelTree({app})` over a folder containing memory cards; this makes that impossible rather than merely discouraged.
2. Anything under `saves/`, `music/sources/`, and `stats/` requires an explicit separate confirmation to remove. They are the only things a user cannot regenerate.

| Existing | Verdict |
|---|---|
| `%APPDATA%\NFL Street Reborn` layout | **Reuse the root, restructure inside it.** Migration reads the old shape |
| `User\GC` memory cards | **Reuse, move to `saves/`** |
| `User\Load\Textures\GN7E69` | **Reuse, move under `library/`** |

---

## 5. Subsystems

### 5.1 Library and preparation

Replaces the fake patcher entirely. Since the game ships finished, this service copies, verifies by hash, and repairs. It never patches.

- Copy shipped payload into `library/offline/` and `library/online/`
- Verify every file against a signed manifest
- Repair by re-downloading only mismatched files
- Extract to `library/work/` on demand for music

| Existing | Verdict |
|---|---|
| `rebornpatcherv6.py` | **Retire.** `fake_patch()` never opened the user's file |
| `build_iso_fields.py`, `build_iso_with_game12.py` | **Wrap.** Used by music rebuild only, never by install |

### 5.2 Runtime supervision

Owns Reborn Dolphin: version, config generation, direct boot, exit classification.

Config is **generated, never edited in place**. The launcher writes `GCPadNew.ini`, `GFX.ini`, and `Dolphin.ini` from its own settings model each launch. This is what stops the launcher and Dolphin's own UI fighting over the same files.

| Existing | Verdict |
|---|---|
| `CREATION LEAGUE AND NEW DOLPHIN` runtime folder | **Port.** Strip the four game images, the `.bak` files, `nsr_track_offline.txt` and `nsr_debug.txt` |
| `dolphin-exgl` | **Port and harden.** See `DOLPHIN_INTEGRATION.md` |
| ReShade, `NSRShaderlessMenu.addon64` | **Reuse on Windows only** |

### 5.3 Controller service

The largest genuinely new component. Today "PlayStation support" is XInput bindings plus PlayStation button art, and no device backend exists.

SDL3 provides device identity, hot-plug, and standardized layouts. The service owns per-device profiles keyed on physical identity, prompt-pack selection, conflict detection for Steam Input and DS4Windows, and generation of Dolphin's input config.

| Existing | Verdict |
|---|---|
| `GCPadNew.ini`, `Profiles\GCPad\{PS,Xbox}.ini` | **Replace** |
| `MODS\PS Controls`, `MODS\Xbox Controls` | **Reuse as prompt packs**, since that is all they ever were |
| Wiimote `Sluggers` profiles | **Retire.** Leftovers from another game |

### 5.4 Content and updates

Seven independently versioned components, since the game-content diff is gone: launcher, runtime, offline content, online content, textures, mods, controller database.

Signed manifests, hash verification, atomic staging and swap, one prior version retained for rollback. Detail in `CONTENT_AND_UPDATE_MODEL.md`.

| Existing | Verdict |
|---|---|
| `NSR_UpdateChecker.exe` | **Retire.** No source located |
| Inno Setup `rebornsetupv10.iss` | **Wrap and reduce** to installing the launcher only |
| JSGME plus `!BACKUP`, `!INSTLOGS` | **Retire.** The 13 packages migrate to the new format |

### 5.5 Music

The Music page drives the existing Python chain against `library/work/`. Changes are queued and applied in one batched rebuild, because a four-minute rebuild per song edit is unusable.

| Existing | Verdict |
|---|---|
| `music_editor.py` | **Wrap the engine, replace the Tkinter UI** |
| `music_pack.py`, `.nsmp` | **Reuse** |
| `extend_slot_40big.py`, `fix_pfdx_toc.py`, `edit_music_meta.py` | **Reuse unchanged** |

### 5.6 Online and stats

Backend on Railway owns identity, matchmaking, Elo, lobbies, chat, and results. Gameplay stays peer to peer through Dolphin NetPlay. Detail in `ONLINE_AND_RESULTS_PROTOCOL.md`.

| Existing | Verdict |
|---|---|
| `nfl-street-reborn-online` client | **Rebuild**, all three copies as reference |
| `server/elo.js`, `socket/*.js`, `db/schema.sql` | **Harvest**, then rebuild against Postgres |
| `netplay_uia.ps1` | **Study, then replace.** UI automation is Windows only |
| `NflsPlayHide` | **Reuse and extend.** Already resolves port to in-game team |
| `NflsStatTracker` | **Reuse unchanged** |
| `ingest.php` on the league site | **Leave alone.** Out of launcher scope |

### 5.7 Xbox Dev Mode

A safe file mover, per Adam. Drive enumeration, explicit picker with nothing preselected, copy, verify, preserve `GC\`, safe eject. No Device Portal, no app deployment. Detail in `XBOX_DEV_MODE_INSTALLER.md`.

| Existing | Verdict |
|---|---|
| `Step 2 - Transfer to Xbox` payload | **Reuse as the payload** |
| `Step 1` fake patcher | **Retire** |
| Nested duplicate folder | **Normalize** |

---

## 6. What the architecture cannot do

Stated plainly, because these are consequences of decisions rather than gaps to fill later.

1. **No differential updates for game content.** Shipping finished images means a content change is a full re-download of that image, roughly 1.5 GB. Launcher, mods, and textures can still be differential.
2. **No enforcement of legal ownership.** The product ships the game. That was true before this project and remains true.
3. **Music editing needs about 1.5 GB of scratch** the first time it is used, plus a bundled Python and FFmpeg in every platform package.
4. **macOS and Linux ship unverified.** No hardware, no signing. CI can build; nobody can confirm behavior.
5. **Rollback for game content is a re-download**, not a delta, for the same reason as point 1.

---

## 7. Risk register

| Risk | Severity | Evidence | Mitigation |
|---|---|---|---|
| Fork will not build cleanly off Windows | **Was high, now medium** | Ten CI rounds. Two real Reborn issues found, both Windows-only build assumptions, both fixed. Linux reached 1204 of 1205 objects | Keep CI green as a permanent gate |
| PlayStation support is entirely unbuilt | **High** | No device backend exists; twelve-scenario matrix untested | Earliest possible SDL3 spike, with real hardware |
| macOS unverifiable | **High** | Only artifact found was a zero-byte placeholder | Ship flagged, be explicit in release notes |
| Credentials unrotated | **High** | Token compiled into `NflsStatUpload.cpp`; `JWT_SECRET` and Discord secret in a Downloads `.env` | Rotate before any public build. Repository stays private until then |
| Music rebuild time | Medium | Four minutes per rebuild, plus extraction | Batch changes, background it, never block Play |
| Single developer, 14 days | **High** | Adam's own statement | Recorded, not argued |
| SiteGround plan unknown | Low | Resolved by moving real time to Railway | None needed |

---

## 8. Build order

Dependency order, not a schedule.

```
 0  Rotate credentials                        blocks any public build
 1  Library and preparation service           unblocks everything
 2  Runtime packaging + supervisor            unblocks Play Now
 3  Controller service (SDL3)                 the long pole
 4  Onboarding                                needs 1, 2, 3
 5  Content and update service                needs 1
 6  Mods, then music                          needs 5, and music needs 1
 7  Backend + online client                   needs 2 and 5
 8  Stat service                              needs 7; the Dolphin half works
 9  Xbox wizard                               independent, cheap
10  Linux and macOS runtimes                  CI now proven partway
```

Step 0 is not sequenced. It takes minutes and it gates publication.

---

## Open items

1. **Token rotation** is deferred by Adam and blocks making the fork repository public, which GPL requires once binaries are distributed.
2. **A real import pipeline** stays out of scope. If that changes, section 5.1 changes shape.
3. **Nothing is implemented.** This is a proposal awaiting the step 7 decision checkpoint.
