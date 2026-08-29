# Component Map

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Status:** Discovery. This map is evidence-backed where the inventory verified something, and marked as proposed where it is design intent that does not exist yet.
**Companion:** [`COMPONENT_INVENTORY.md`](./COMPONENT_INVENTORY.md)

Legend used in every diagram:

- `[E]` **Exists** and was verified in this discovery pass.
- `[P]` **Proposed**, does not exist in any form yet.
- `[R]` **Rebuild**, something exists but will be replaced rather than reused.
- `[X]` **Retire**, exists today and goes away.

---

## 1. The system as it exists today

This is what a user actually gets right now, drawn from verified evidence.

```text
                    NFL Street Reborn Setup.exe  [E]
                    Inno Setup v10, 1.63 GB payload
                                 │
                                 ▼
                  %APPDATA%\NFL Street Reborn\
                                 │
        ┌────────────────┬───────┴────────┬─────────────────┐
        ▼                ▼                ▼                 ▼
  NSR_UpdateChecker  Reborn Dolphin   Reborn Mod        Tools\Patcher
  .exe  [E][X]       .exe (Qt5) [E]   Manager           .exe  [E][X]
  Start Menu and     8705a96b...      (JSGME 2.6) [E]   fake_patch()
  desktop shortcut                          │           sleeps 14 s and
        │                  │                │           discards the
        │                  │                ▼           user's file
        └─── launches ─────┘          MODS\ 13 packs
                           │          texture overlays only
                           │          !BACKUP, !INSTLOGS
                           ▼
                  Games\NSR.iso  [E]
                  1,459,978,240 bytes, prebuilt and shipped
                           │
                           ▼
                  User\Load\Textures\GN7E69  ~2.0 GB
                  User\GC  memory cards
                  User\Config  GCPadNew.ini -> XInput/0/Gamepad
```

**What this diagram shows that matters:**

- The user's entry point is already a launcher shim, `NSR_UpdateChecker.exe`, not Dolphin.
- The game image is shipped, not built. The patcher is decoration.
- Mods are file overlays with no transactional guarantee.
- The single controller device is XInput regardless of what hardware is plugged in.

The shipped runtime and the **selected** runtime are different binaries from different lineages:

```text
  Qt5, 14 MB, 8705a96b...              Qt6, 17.8 MB, 617321bf...
  Files\Reborn Dolphin\                DOLPHIN ONLINE TEST\
  NFL Street Reborn Dolphin.exe        CREATION LEAGUE AND NEW DOLPHIN\
           [E] shipped                 Dolphin.exe  [E] selected
                                                │
                                       byte-identical to
                                                │
                                       dolphin-exgl\Build\x64\Release\
                                       Dolphin\bin\Dolphin.exe   PROVEN
```

---

## 2. Source of truth, and where it is not protected

Everything downstream is generated from two trees. Neither is under version control.

```text
  nflstreet2_clean\                          dolphin-exgl\
  NOT a git repository  [E]                  git repo, HEAD 7f4875e  [E]
  ────────────────────────                   remote BlueSwordM/dolphin
        │                                          │
        │  GameCube Version\root\                  │  ALL Nfls* files UNTRACKED
        │  the live extracted game filesystem      │  12 upstream files UNCOMMITTED
        │                                          │
        │  tools\  984 entries                     │  NflsStatTracker.cpp   47 KB
        │    build_iso_fields.py                   │  NflsUiSplit.cpp       26 KB
        │    build_iso_with_game12.py              │  NflsStatUpload.cpp    14 KB
        │    music_editor.py + 4 chained tools     │  NflsPlayHide.cpp       4 KB
        │                                          │
        │  Stat Tracking\  research decoder        │  Version.cpp -> "NFL Street
        │  GAME VERSIONS\  4 retail references     │  Reborn Dolphin (BETA)"
        ▼                                          ▼
   game builds                                Dolphin.exe
   NSR.iso, FR4N31.iso, CR4E12.iso            617321bf...
```

**Trust boundary, and it is currently open:** roughly 98 KB of original C++ that constitutes the entire competitive and statistics feature set exists as working-tree state on one machine, in no commit, on no branch, with no remote. `nflstreet2_clean` has no version control at all. This is the highest-severity item in the inventory.

---

## 3. Target architecture

```text
┌──────────────────────────────────────────────────────────────────────────┐
│                    NFL Street Reborn Launcher  [P]                       │
│              Tauri 2 shell, React and TypeScript UI                      │
│                                                                          │
│   Home    Play    Online    Mods    Music    Stats    Settings           │
│                                                                          │
│   first-boot onboarding  [P]          Xbox Dev Mode wizard  [P]          │
└───────────────────────────────┬──────────────────────────────────────────┘
                                │  Tauri IPC, typed commands
┌───────────────────────────────▼──────────────────────────────────────────┐
│                       Rust native core  [P]                              │
├──────────────┬──────────────┬──────────────┬──────────────┬──────────────┤
│ import and   │ runtime      │ controller   │ content and  │ stat service │
│ build svc    │ supervisor   │ service      │ update svc   │              │
│              │              │ SDL3         │              │              │
│ [P] new      │ [P] new      │ [R] replaces │ [P] new      │ [R] takes    │
│ work, the    │ Game Mode    │ XInput ini   │ signed       │ upload out   │
│ patcher was  │ Suspend      │ profiles     │ manifests    │ of Dolphin   │
│ never real   │              │              │              │              │
├──────────────┴──────────────┴──────────────┴──────────────┴──────────────┤
│                   SQLite local state    OS credential vault              │
└───────┬──────────────────┬───────────────────┬───────────────────────────┘
        │                  │                   │
        ▼                  ▼                   ▼
  managed library    Reborn Dolphin      Reborn backend  [R]
  [P]                runtimes  [E/P]     harvest from
                                         nfl-street-reborn-online
```

### Where each existing component lands

| Existing | Verdict | Becomes |
|---|---|---|
| Inno Setup v10 | wrap and reduce | Installs the launcher only. No patcher page, no JSGME page, no elevation prompt |
| Patcher `fake_patch()` | **retire** | Import and build service, genuinely new work |
| `NSR_UpdateChecker.exe` | retire | Content and update service |
| JSGME plus `MODS\` | retire, migrate content | Transactional package system; the 13 packs become catalog entries |
| `GCPadNew.ini`, `Profiles\GCPad\` | replace | SDL3 controller service with per-device identity |
| `music_editor.py` plus 4 tools | wrap, replace UI | Music page drives the Python chain, or the writers get reimplemented. See open question 5 |
| `nflstreet2_clean\tools\` | wrap | Build backend behind the import and build service |
| `NflsStatTracker` | keep in Dolphin | Capture stays next to game memory |
| `NflsStatUpload` | move out | Dolphin writes JSON; the launcher authenticates and uploads |
| `NflsPlayHide` | keep and extend | Already resolves port to in-game team. Becomes half the winner identity chain |
| `NflsUiSplit` | keep, isolate | Experimental. Must not gate a release |
| ReShade plus `NSRShaderlessMenu` | keep, Windows only | Graphics settings on Windows. No Linux or macOS parity |
| Online client | rebuild | Online page inside the launcher |
| Online server | harvest and rebuild | `elo.js`, `schema.sql`, socket handlers inform the new backend |
| Xbox Dev Mode ZIP | absorb | One PC-side wizard |
| League site | out of scope | Only the stat ingest contract matters |

---

## 4. Process boundaries

Four separate operating system processes, and the boundaries are what keep the product reliable.

```text
   ┌─ Process A ────────────────┐        ┌─ Process B ────────────────┐
   │ Launcher UI                │        │ Supervisor  [P]            │
   │ Tauri webview plus Rust    │  spawn │ tiny Rust binary           │
   │ Owns all user interaction  │───────>│ Owns the game session      │
   │ Exits during gameplay      │        │ Survives the UI exiting    │
   └────────────────────────────┘        └─────────┬──────────────────┘
                                                   │ spawn, direct boot
                                                   ▼
                                         ┌─ Process C ────────────────┐
                                         │ Reborn Dolphin  [E]        │
                                         │ Owns emulation and NetPlay │
                                         │ Never shows its game list  │
                                         └─────────┬──────────────────┘
                                                   │ writes
                                                   ▼
                                          User\LeagueGameStats\*.json
                                                   │
   ┌─ Process D ────────────────┐                  │
   │ Backend  [R]               │<─── uploaded by ─┘
   │ Remote. Owns identity,     │     the launcher, not by Dolphin
   │ Elo, matchmaking, results  │
   └────────────────────────────┘

   Blueprint requirement: "minimize with no resources" is met by Process A
   exiting entirely while Process B, a few megabytes, watches the game.
```

**Why Dolphin must not upload:** the current build uploads from inside the emulator using a compiled-in bearer token, and its gate can be disabled by dropping a text file into `User\`. Moving upload to Process A makes the credential rotatable, the retry queue durable across game crashes, and the gate impossible to defeat by file placement.

---

## 5. Filesystem roots and data ownership

```text
INSTALL ROOT              owner: installer            replaceable wholesale
  Windows   %LOCALAPPDATA%\Programs\NFL Street Reborn Launcher
  macOS     /Applications/NFL Street Reborn Launcher.app
  Linux     AppImage or Flatpak
  Contents  launcher binary, supervisor, bundled web assets

DATA ROOT                 owner: launcher             survives uninstall by default
  Windows   %APPDATA%\NFL Street Reborn                <- already used today  [E]
  macOS     ~/Library/Application Support/NFL Street Reborn
  Linux     $XDG_DATA_HOME/NFL Street Reborn

  library/base/       verified private base       user-provided  NEVER DELETE  [P]
  library/offline/    generated offline build     derived        regenerable
  library/online/     NSRONLINE.iso               managed        replaceable
  runtime/            platform Reborn Dolphin     managed        replaceable
  mods/{packages,staging,active}/                 mixed          restorable
  music/{sources,packs,work}/  sources are user   user           NEVER DELETE
  saves/              memory cards                user           NEVER DELETE  [E]
  stats/{queue,uploaded,local.db}                 user           NEVER DELETE
  cache/ downloads/ logs/                         derived        safe to purge
  backups/ config/                                launcher       preserve
```

**Ownership classes, and the rule each one implies:**

| Class | Examples | Rule |
|---|---|---|
| User-provided | imported base, music source files | Never delete without an explicit separate confirmation. Losing this costs the user a re-import |
| User-generated | saves, memory cards, screenshots, local stats | Never delete. Preserve across uninstall by default |
| Managed | runtime, online build, curated mods | Launcher owns it. Safe to replace atomically |
| Derived | offline build, caches, shader caches | Regenerable. Safe to purge for disk space |

**Two verified failure modes this model exists to prevent:**

1. The current uninstaller runs `DelTree({app}, True, True, True)` over an app folder that contains `User\GC` memory cards. Separating install root from data root makes that impossible.
2. The current updater deletes `MODS\` including JSGME's `!BACKUP`, leaving overlaid files with no restore path. Staging and atomic activation replace that.

---

## 6. Trust boundaries

```text
   UNTRUSTED                    │  SEMI-TRUSTED           │  TRUSTED
   ─────────────────────────────┼─────────────────────────┼──────────────────
   user-supplied game image     │  curated marketplace    │  signed launcher
     -> hash and identify       │  packages               │  manifests
        BEFORE use              │    -> validated at      │    -> signature
                               │       submission, hash- │       checked before
   local ZIP or folder mods     │       pinned on         │       any file is
     -> validate paths,        │       download          │       swapped
        no traversal            │                         │
                               │  peer NetPlay traffic   │  OS credential
   opponent-submitted match     │    -> P2P, exposes IP   │  vault
   result                       │       to opponent,      │    -> only place
     -> never authoritative     │       disclosed in      │       tokens live
        alone, needs both       │       onboarding        │
        clients to agree        │                         │
```

**The winner identity chain**, and which links exist today:

```text
  Discord account            [P] backend does not exist in production
        ▼
  matchmaking participant    [R] prototype has routes and sockets
        ▼
  NetPlay player ID          [E] Dolphin NetPlay assigns this
        ▼
  local controller port      [E] NflsPlayHide already resolves this
        ▼
  in-game team or side       [E] NflsPlayHide already resolves this
        ▼
  final score                [E] NflsStatTracker emits sides_verified,
                                 team_totals, players[14], teams[2]
```

The bottom three links are **built and working**. The top two are the gap. This is the cheapest high-value integration in the whole project, because the hard part, reading game memory reliably, is already solved.

**Result acceptance:** both clients submit independently under a server-issued match ID and nonce. Agreement is accepted automatically. Disagreement, absence, crash, or desync is held for review, and Elo does not move. Conservative by default, per the blueprint.

---

## 7. Update boundaries

Eight independently versioned components. Each needs its own manifest, hash set, and rollback slot.

```text
  Backend content service  [P]
        │  signed manifests, HTTPS, hash-pinned
        ▼
  ┌──────────────────────────────────────────────────────────────┐
  │ launcher      │ blocking update, needs restart               │
  │ runtime       │ apply when Dolphin is not running            │
  │ offline patch │ regenerate offline build from retained base  │
  │ online patch  │ MUST match the server's expected hash        │
  │ base textures │ large, differential delivery matters most    │
  │ curated mods  │ per-package, atomic activate and deactivate  │
  │ controller db │ small, frequent, low risk                    │
  │ news config   │ non-blocking, cache and fail soft            │
  └──────────────────────────────────────────────────────────────┘

  Every component: stage -> verify hash -> atomic swap -> health check
                   -> keep exactly one prior version for rollback
```

**Hard constraint from the inventory:** differential delivery for the game builds is only possible once a real base-plus-patch model exists. Today there is no diff between retail NFL Street 2 USA and `NSR.iso`; the whole 1.46 GB image is shipped. Until the import and build service exists, "no unnecessary multi-gigabyte redownloads" cannot be delivered for game content. That dependency is why the import service sits at the root of the build order.

---

## 8. Platform split

```text
                       Windows   Linux/Deck   macOS      Xbox Dev Mode
  Launcher UI            [P]        [P]        [P]         n/a
  Rust core              [P]        [P]        [P]         n/a
  SDL3 controllers       [P]        [P]        [P]         n/a
  Reborn Dolphin         [E]        [P]        [P] none    [E] manual
  ReShade + presets      [E]        n/a        n/a         n/a
  Code signing           none       n/a        none        n/a
  Test hardware          yes        NO         NO          unknown
```

**Verified gaps:**

- macOS has exactly one artifact anywhere in the inspected material: a zero-byte `PUT_DOLPHIN_APP_HERE.txt`. There is no macOS Dolphin build, no toolchain, no signing account, and no hardware.
- Linux and Steam Deck have no build and no hardware.
- ReShade and `NSRShaderlessMenu` are Windows-only by construction. Per Adam's direction, no parity is required elsewhere, so Linux and macOS ship without the per-field look.

---

## 9. Xbox Dev Mode wizard

```text
  PC side, launcher-owned                        Xbox side
  ──────────────────────────────────────         ─────────────────────
  1. detect removable drives  [P]
  2. explicit drive picker, never auto  [P]
  3. reuse retained verified base  [P]
  4. patch and prepare Xbox game image  [P]
  5. write layout to USB  [E] layout known:
       Config\ GameSettings\ GC\ Load\ NSR.png
  6. verify every file, preserve GC\  [E] cards ship in the package today
  7. pair by IP or PIN  [P]
                          Device Portal, HTTPS  ──>  8. deploy or update
                                                        UWP package  [P]
  9. safe eject  [P]                                       │
                                                            ▼
  10. "plug it into the Xbox"                        E:\Games  [E] assumed
```

**What exists:** only the `Step 2 - Transfer to Xbox` payload layout, and it is duplicated in a nested folder of the same name. **What does not exist:** any app package, certificate, Device Portal client, or deployment automation. Steps 1 through 10 are all new work apart from knowing the destination layout.

---

## 10. Build order implied by the dependency graph

Not a schedule. This is what the map says must precede what.

```text
  0.  PRESERVE  commit dolphin-exgl, rotate the three credential sets
      ────────────────────────────────────────────────────────────────
  1.  import and build service    unblocks everything about content
  2.  runtime packaging + supervisor    unblocks Play Now
  3.  controller service (SDL3)   the known long pole, PlayStation is unbuilt
  4.  onboarding                  needs 1, 2, 3 to have anything to configure
  5.  content and update service  needs 1 for a base-plus-patch model
  6.  mods and music              needs 5 for delivery, 1 for rebuild
  7.  backend + online client     needs 2 for launch, 5 for version enforcement
  8.  stat service                needs 7 for identity, Dolphin side already works
  9.  Xbox wizard                 needs 1 for the patched image
  10. Linux and macOS runtimes    blocked on toolchain and hardware
```

Step 0 is not sequenced against the others. It should happen first, and it takes minutes.

---

## Open items this map cannot resolve

These are carried into [`PROJECT_QUESTIONNAIRE.md`](./PROJECT_QUESTIONNAIRE.md):

1. Base-plus-patch versus ship-the-image. Determines the entire content and update model.
2. Whether the music tool chain ships as Python or gets reimplemented natively.
3. Which online copy is canonical, and who owns the third-party remotes.
4. Whether the launcher posts to the existing `ingest.php` or to a new backend.
5. Backend hosting, database, storage, and budget.
6. macOS and Linux verification strategy without hardware.
7. What September 12 means for a product with three unbuilt platform runtimes.
