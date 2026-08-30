# System Architecture

> **Precedence.** Where this document disagrees with
> [`RECONCILIATION_RULINGS.md`](./RECONCILIATION_RULINGS.md), **the ruling wins.**
> Five auditors found 172 internal contradictions across this document set and an
> adjudicator issued one ruling per conflict. The rulings are applied to this file
> progressively; until that is finished, read the rulings first for anything
> touching safety, verdicts, schemas or identifiers.

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

## 1A. Decided-row reversals, awaiting sign-off

The architecture work reversed twelve rows the questionnaire records as
**Decided**. Each reversal may well be correct, but a document is not allowed to
overturn a settled decision quietly, so they are collected here in one place for
one approval pass rather than scattered across six files.

**The rule going forward:** no architecture document may reverse a Decided row
without naming the row number inline, stating the evidence, and tagging it
"Needs Adam's sign-off".

| Row | Was decided | Architecture now says | Why |
|---|---|---|---|
| 8.1, 8.2, 8.3 | Import once, retain a verified base, accept ISO/RVZ/folder/disc | No import, no retained base | Forced by the locked ship-the-image decision. Consequential, not discretionary |
| 9.2 | The online build is named `NSRONLINE.iso` | Name retired | Identifier hygiene in a signed-manifest system |
| 18.1 | Eight independently updateable components | Seven | The game-content diff does not exist, so it cannot be a component |
| 7.2 | Seven Xbox wizard modes | Five | Two modes only made sense with Device Portal, which is out of scope |
| 7.9 | Console pairing credentials in the Windows credential store | Retired | Nothing pairs with a console any more |
| 17.9 | `statread1` is a separate research capture path | Not separate | Evidence review |
| 15.3, 15.9 | Region and ping are priority matchmaking inputs; input delay derived from ping | De-scoped to post-launch | Schedule |
| 16.5 | An explicit quit in ranked is likely a forfeit | No automatic forfeit | Held-for-review is safer while the evidence chain is unproven |
| 11.6 | Thirteen mod packages migrate | Eleven | Two of the thirteen directories are JSGME control folders, `!BACKUP` and `!INSTLOGS` |
| 4.2 | Fair Input Delay is on the cannot-regress list | **Reversal REJECTED by Adam. It stays on the list.** | Adam requires it. The audit was factually right that it is stock upstream (`NetPlayDialog.cpp` line 184, unmodified against `7f4875e`, and the preservation commit `eca379f` touches no NetPlay source), but that makes keeping it FREE rather than pointless: it cannot regress as long as nothing touches the NetPlay mode or its default, and listing it is what makes that a checked constraint instead of an assumption |
| 7.1 | Xbox payload size | 2.9 GB figure changed | Measurement |

**Two reversals were rejected and the original decision stands:**

- **14.9.** The launcher never posts to `ingest.php`. A document had it routing there; that contradicts the Decided row and it has been restored.
- **12.12.** No native Rust rewrite of the music chain on the launch path. Python inside the bundled runtime for v1, native as the follow-up.

**Recommended reading order for approval:** 4.2 first, because removing Fair
Input Delay from the cannot-regress list is the only reversal that looks like an
error rather than a consequence. Then 15.3, 15.9 and 16.5 together, since all
three shrink what online means on day one.

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
| `NflsStatTracker` inside Dolphin | **Wrap, additive.** Capture stays next to game memory, but it does need a source change: one new top-level `reborn_match` key emitted alongside `schema`. The schema string stays exactly `nsr.game.v1`, no existing key is renamed, removed, reordered or redefined, and the `si_capture_hash` hashed set is untouched. Any plan built on "no Dolphin source change is needed for capture" is wrong |

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

**This section is the single authority for the on-disk layout.** Three other
documents previously restated it and all three differed. They now reference this
section rather than repeat it.

```text
INSTALL ROOT          installer owns it, replaceable wholesale
  Windows   %LOCALAPPDATA%\Programs\NFL Street Reborn Launcher
  macOS     /Applications/NFL Street Reborn Launcher.app
  Linux     AppImage or Flatpak

  app/                  the running bundle
  app.staging/          a verified new bundle, briefly
  app.prior/            the previous bundle, kept for rollback

DATA ROOT             launcher owns it, survives uninstall by default
  Windows   %APPDATA%\NFL Street Reborn Launcher
  macOS     ~/Library/Application Support/NFL Street Reborn Launcher
  Linux     $XDG_DATA_HOME/NFL Street Reborn Launcher

  NOT %APPDATA%\NFL Street Reborn, which is 2.1's own live Dolphin user
  tree. See the corrected verdict in section 9.

  downloads/<component>/<version>/
  staging/<component>/<version>/
  library/offline/                    shipped offline image      managed
  library/online/                     shipped online image       managed
  library/textures/<gameid>/          pristine, READ ONLY        managed
  library/xbox/                       Xbox payload               managed
  library/work/                       extracted tree, music only derived
  runtime/<platform>/                 Reborn Dolphin             managed
  mods/staging/<id>/<version>/        unverified download
  mods/packages/<id>/<version>/       validated payloads
  mods/active/<gameid>/               the composed tree Dolphin loads
  mods/activation.json                ordered active ids and versions
  music/sources/                      USER FILES, never delete
  music/packs/
  music/state.json                    recorded rebuild hashes
  music/queue.json                    pending slot changes
  saves/                              memory cards, NEVER DELETE
  stats/queue/                        captures awaiting upload
  stats/local.db
  versions/<component>/current.json
  versions/<component>/prior/         every component EXCEPT the launcher
  recovery/<timestamp>/
  backups/xbox/<ISO timestamp>/
  xbox/journal-<volume-identity>.json
  cache/  logs/  config/              purgeable except config
```

**`library/base/` is gone** from the map in discovery. Shipping the finished
image means there is no user-provided base to retain.

### The launcher updates itself without ever crossing volumes

An atomic rename cannot cross a filesystem. INSTALL ROOT and DATA ROOT are
frequently on different volumes, so a swap that renames between them is not
atomic and can leave the product in neither state.

**The launcher's prior version therefore never lives under DATA ROOT**, and
`versions/launcher/prior/` does not exist. The sequence:

1. Stage and verify the new bundle under `staging/launcher/<version>/` in DATA ROOT.
2. The supervisor **copies** it into `INSTALL ROOT/app.staging/` and re-verifies every hash at the destination.
3. The launcher exits.
4. The supervisor renames `app` to `app.prior` and `app.staging` to `app`. Both are same-directory renames on one volume.
5. The supervisor starts the new launcher and waits for an explicit ready handshake.
6. On failure it reverses both renames.

Every other component stages and swaps entirely inside DATA ROOT, so the
same-volume precondition holds for all of them without a special case.

### Mods compose, they never overwrite

`library/textures/<gameid>/` is pristine, manifest-verified, and **read only**.
Nothing ever writes into it.

`mods/active/<gameid>/` is a **composed** tree: hardlinks to every pristine file,
with active package files placed over the ones they override. It is the only
texture tree Dolphin is pointed at. Deactivating a mod deletes the composed tree
and rebuilds it from `mods/activation.json`.

**There is no reverse map and no backup folder, because nothing is ever
displaced.** That is the whole reason this model is chosen over JSGME's, which
needed `!BACKUP` precisely because it wrote into the tree it had to restore.

**The cost, stated honestly:** a hardlink cannot cross a filesystem, so the
pristine and composed trees must share a volume. On exFAT and FAT32, where
hardlinks are unavailable, composition falls back to a full copy of roughly
2.0 GB. On a handheld with an SD card that is the common case, not the edge
case, and it has to be budgeted as such.

### Ownership classes

| Class | Examples | Rule |
|---|---|---|
| User-provided | `music/sources/` | Never delete without a separate explicit confirmation |
| User-generated | `saves/`, `stats/` | Never delete. Preserved across uninstall by default |
| Managed | `runtime/`, `library/`, `mods/packages/` | Launcher owns it. Safe to replace atomically |
| Derived | `library/work/`, `mods/active/`, `cache/` | Regenerable. Safe to purge for disk space |

**Two verified failure modes this model exists to prevent:**

1. The current uninstaller runs `DelTree({app}, True, True, True)` over an app folder containing `User\GC` memory cards. Separating install root from data root makes that impossible rather than merely discouraged.
2. The current updater deletes `MODS\` including JSGME's `!BACKUP`, leaving overlaid files with no restore path. Composition removes the failure entirely, since the pristine tree was never touched.

| Existing | Verdict |
|---|---|
| `%APPDATA%\NFL Street Reborn` root | **CORRECTED. Do not reuse. Migrate out of it.** Earlier text here said to reuse the root and restructure inside it. That is a data-loss bug, found by running the CLI against a real machine. The folder is a live Dolphin user tree with a JSGME `MODS` directory holding the player's installed mods. Windows paths are case insensitive, so the launcher's `mods/` is that `MODS/`, and `mods/staging` and `mods/active` are classed derived, which means a cache purge may delete them. Launcher data now lives in its own root, and this folder is only ever a migration source |
| `User\GC` memory cards | **Reuse**, moved to `saves/` |
| `User\Load\Textures\GN7E69` | **Reuse**, moved to `library/textures/GN7E69/` as pristine |
| JSGME `!BACKUP`, `!INSTLOGS` | **Retire.** Composition makes both unnecessary |


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
| `build_iso_fields.py`, `build_iso_with_game12.py` | **Port.** The FST and `Game.toc` writing logic and the two gate rules move into a launcher-owned repacker; neither script file ships. Python inside the bundled runtime for v1, not a Rust rewrite, per Decided row 12.12 |

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
| Inno Setup, the tool | **Reuse** |
| `rebornsetupv10.iss`, the script | **Replace.** A new script, carrying over only the `AppId` GUID, and only alongside `UsePreviousAppDir=no`, `UsePreviousData=no`, `DisableDirPage=yes`, no `DelTree` anywhere, and a build-time assertion of all four. Without `UsePreviousAppDir=no` the retained AppId makes Inno restore `{app}` to the old data root |
| `BackupUserData`, `RestoreUserData`, `CleanTexturesOnUpdate`, the uninstall `DelTree({app})`, `ShellExec('runas', JSGME.exe)` | **Retire** |
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
| `netplay_uia.ps1` | **Retire.** Kept outside the product as the calibration record of what Dolphin's NetPlay dialog looks like, which is a preservation obligation and not a verdict |
| `NflsPlayHide` | **Wrap.** Already resolves port to in-game team; the mapping is exposed as a stable interface for the results protocol |
| `NflsUiSplit` | **Reuse.** Ships. Now present in `Source/Core/VideoCommon/CMakeLists.txt`, Windows internals behind existing `_WIN32` guards. Explicitly not a release gate |
| Shipped runtime `NFL Street Reborn Dolphin.exe` (`8705a96b...`, Qt5) | **Replace** by the selected runtime `617321bf...`. A runtime migration, not a file swap |
| `NflsStatTracker` | **Wrap, additive.** See section 2 |
| `ingest.php` on the league site | **Reuse.** Deployment untouched and out of launcher scope. Its proven design is harvested by the new ladder endpoint: canonical `si_capture_hash`, idempotency through a UNIQUE key rather than a SELECT, "review" counted as success by the uploader, prefix-tolerant team resolution, and the 512 KB payload cap |

### 5.7 Xbox Dev Mode

A safe file mover, per Adam. Drive enumeration, explicit picker with nothing preselected, copy, verify, preserve `GC\`, safe eject. No Device Portal, no app deployment. Detail in `XBOX_DEV_MODE_INSTALLER.md`.

| Existing | Verdict |
|---|---|
| `Step 2` layout and configuration shape | **Reuse** |
| `Step 2\GC\MemoryCardA.USA.raw`, `SRAM.raw` | **Retire.** The author's own saves; shipping them would overwrite returning users |
| `Step 2\Config\TimePlayed.ini` | **Replace** with a launcher-generated file |
| `Step 2\Config\Dolphin.ini` `[Analytics]` block | **Replace** |
| `Step 2\Config\Logger.ini` | **Replace**, console and window logging off |
| `Step 2\GameSettings\GN7E69.ini` | **Port** into generated configuration |
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
