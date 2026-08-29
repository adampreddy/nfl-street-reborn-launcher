# Project Questionnaire

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Status:** Living document. Updated after every answer.
**Sources pre-filled from:** the product blueprint, the chat transcript, `CLAUDE_START_HERE.md`, the UX research brief, and the read-only inventory in [`COMPONENT_INVENTORY.md`](./COMPONENT_INVENTORY.md).

### Labels

| Label | Meaning |
|---|---|
| **Decided** | Already answered. Recorded here so it is never asked again |
| **Confirm** | A likely answer exists, but the ambiguity changes the design. One short confirmation needed |
| **Open** | No answer yet. Adam has to decide |
| **Research** | Should be answered by inspection or cited research, not by burdening Adam |

**Deferred** is added to any item Adam says "later" to.

### Counts

Decided 78 · Confirm 18 · Open 21 · Deferred 3 · Research 14

Updated after batch 1. Seven items moved to Decided, one to Deferred, one closed by action.

Only **Open** and **Confirm** items are ever asked. They are asked in batches of ten, hardest and most blocking first. Batch 1 is answered. Batch 2 is at the end of this document.

---

## 1. Branding, typography, artwork rights

| # | Item | Label | Answer or status |
|---|---|---|---|
| 1.1 | Product name | **Decided** | NFL Street Reborn Launcher |
| 1.2 | Dolphin's user-facing name | **Decided** | NFL Street Reborn Dolphin. Already implemented: `Version.cpp` sets the revision string to `NFL Street Reborn Dolphin (BETA)` |
| 1.3 | Visual direction | **Decided** | Clean modern Steam and Playnite, per moodboard |
| 1.4 | Moodboard authority | **Decided** | `docs/design/Reborn Launcher Moodboard.png` is directional, not to be copied literally |
| 1.5 | Logo and icon source files | **Confirm** | Canonical assets are now in `assets/`: `NFL Street Reborn Icon Logo.png`, the stroke variant, and `NSRLOGO.png`. All raster, 4096x4096. **No vector or layered source located.** Small window icons and Steam Deck scale will look soft. Trace a clean vector during the UX phase unless an Illustrator or Figma source turns up |
| 1.6 | Typeface | **Open** | Blueprint says "modern UI sans serif with excellent small-size legibility". No specific face chosen. Licensing for desktop embedding needed |
| 1.7 | Accent color | **Decided** | **Reborn red is the primary accent.** Adam, batch 3. This overrides the blueprint proposal of an electric blue or indigo primary, which fought the logo. Near-black shell, red primary, teal or cyan reserved for data and live status so it never competes with red. Error states must be distinguishable from brand red by icon and text, not hue alone |
| 1.8 | NFL and team trademarks | **Open** | Every screen shows NFL marks, team logos, and player likenesses. Distribution policy is Adam's call |
| 1.9 | Hero and cover artwork rights | **Confirm** | `NSR.png` and `CR4E12.png` exist. Provenance unknown |
| 1.10 | Third-party attribution already owed | **Decided** | `README.txt` credits BlueSwordM (Dolphin fork) and nfl_numbers (jersey number textures). Both must carry forward |
| 1.11 | Moodboard region decomposition | **Research** | Ten regions identified: Steam library, console detail page, Discord onboarding cards, dark settings, NBA-style discovery, Steam app concept, PS3 XMB detail, dark data dashboard, Switch detail page, Reborn welcome screen. Full decomposition is a UX phase deliverable |

---

## 2. Onboarding sequence and copy tone

| # | Item | Label | Answer or status |
|---|---|---|---|
| 2.1 | Onboarding owns configuration, installer does not | **Decided** | Explicit. Inno Setup installs only |
| 2.2 | Twelve-step flow | **Decided** | Welcome, migration, import, storage, system check, display, controllers, audio, Discord, privacy, prepare, ready |
| 2.3 | Resumable and idempotent | **Decided** | Required. Crash, restart, and power loss safe |
| 2.4 | Controller-navigable from screen one | **Decided** | Required |
| 2.5 | Onboarding schema version | **Decided** | Record it so future releases add one step, not a replay |
| 2.6 | Run Setup Again, Reset Only This Section | **Decided** | Both required in Settings |
| 2.7 | Tone | **Decided** | Reassuring and direct. Not technical, not playful. Banned phrases listed in the UX brief |
| 2.8 | Step 2 migration source | **Confirm** | Migration must read the **Qt5** installed layout at `%APPDATA%\NFL Street Reborn`. Verify what actually converts: saves and textures yes, `GCPadNew.ini` probably not since the controller model changes entirely |
| 2.9 | What happens with no legally owned copy | **Open** | Depends entirely on question 1 in batch 1 |

---

## 3. Canonical source trees and obsolete copies

| # | Item | Label | Answer or status |
|---|---|---|---|
| 3.1 | Launcher repository path | **Decided** | `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher` |
| 3.2 | Chosen Dolphin runtime | **Decided** | `nflstreet2_clean\DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN` |
| 3.3 | Build lineage of the selected runtime | **Research, ANSWERED** | Proven. SHA-256 `617321bf...` matches `dolphin-exgl\Build\x64\Release\Dolphin\bin\Dolphin.exe` byte for byte |
| 3.4 | Git remote for the launcher | **Decided, DONE** | GitHub account is **`adampreddy`** (renamed from `michaelpenixfan2390`, same account id 188915146). Launcher pushed to `adampreddy/nfl-street-reborn-launcher`, private, commit `1106c76`. Fork target is `adampreddy/reborn-dolphin`, private until the bearer token is rotated |
| 3.5 | Preserving `dolphin-exgl` | **Decided, DONE** | Committed August 29, 2026 as `eca379f` on branch `nfls-reborn/preserve-2026-08-29`. Local only, not pushed. Adam noted the work is unfinished, so it is labeled a WIP snapshot. Build logs excluded. Push is blocked until the bearer token is rotated |
| 3.6 | Canonical online copy | **Decided** | **Rebuild, use the existing copies as reference.** Adam, batch 2. All three copies stay untouched as reference material. No copy is promoted to canonical |
| 3.7 | Ownership of `MiltyTheGreat/RebornOnline` and `michaelpenixfan2390/nflstreetrebornlauncher` | **Resolved** | `michaelpenixfan2390` is **Adams own account**, since renamed to `adampreddy`. Verified: the `gh` CLI on this machine is authenticated as it. `adampreddy/nflstreetrebornlauncher` is private and holds the June 2026 Electron prototype, so it is reference material, not the new launcher. `MiltyTheGreat` remains unidentified and nothing is pushed there |
| 3.8 | Version control for `nflstreet2_clean` | **Confirm** | Also not a git repository. 984 tools and the live game root are unprotected. Recommend at minimum a backup policy |
| 3.9 | Obsolete artifacts identified | **Decided** | Nine superseded `.iss` files, `rebornpatcherv3.py` and `v5.py`, two Wiimote `Sluggers` profiles, `1NEWEST DOLPHIN 4 MIKEL.zip`, `PERFECTLY FINE DOLPHIN`, `Dolphin-x64`. None deleted, per the read-only rule |
| 3.10 | `TOTALLY NOT MALWARE.iso` | **Confirm** | 1.46 GB image in the selected runtime's `Games\`. Meaningful build or joke copy? |

---

## 4. Dolphin build lineage and features that cannot regress

| # | Item | Label | Answer or status |
|---|---|---|---|
| 4.1 | Upstream | **Decided** | ExGL fork, `BlueSwordM/dolphin`, HEAD `7f4875e` |
| 4.2 | Features that cannot regress | **Decided** | `NflsPlayHide`, `NflsStatTracker`, `NflsStatUpload`, `NflsUiSplit`, Fair Input Delay NetPlay, ReShade add-ons, stage presets |
| 4.3 | No rebase or replace before a written migration plan | **Decided** | `CLAUDE_START_HERE.md` item 8 |
| 4.4 | `NflsUiSplit` release status | **Confirm** | Modified today, August 29, 2026. `Dolphin.exe.bak_2026-08-29_pre_uisplit` exists, so it is actively in flux. Recommend it does not gate a release |
| 4.5 | Debug toggles must never ship | **Decided** | Adam, batch 2: clean it up. Runtime packaging strips `nsr_track_offline.txt` and `nsr_debug.txt`, and the netplay gate becomes a compiled-in setting so a dropped text file cannot defeat it. Files left in place for now, discovery is read-only |
| 4.6 | Reproducible build | **Open** | No CI, no committed build script, no pinned toolchain. A second machine cannot currently reproduce `617321bf...` |
| 4.7 | Linux and macOS Dolphin builds | **Open, cheaper than assumed** | Neither binary exists, but the port is mostly already done. The three shipping Reborn features are portable and already in the CMake build, and `BuildMacOSUniversalBinary.py` is present in the fork root. Remaining unknowns: whether ExGL's renderer changes still build on non-Windows, and that neither of us can run the result. Batch 3 |
| 4.8 | Windows-only code | **Research, ANSWERED** | Windows-only: `dxgi.dll` ReShade injection, `NSRShaderlessMenu.addon64`, and `NflsUiSplit.cpp` (includes `windows.h`, three `_WIN32` guards, and **not listed in `VideoCommon/CMakeLists.txt`** so it does not build under CMake at all). **Portable:** `NflsPlayHide` (zero Windows code), `NflsStatTracker` (one guard), `NflsStatUpload` (uses Dolphin's cross-platform `Common/HttpRequest.h`, **not** WinHTTP as an earlier draft of the inventory stated). All three are already wired into `Source/Core/Core/CMakeLists.txt` lines 442-447 |
| 4.9 | Save-state compatibility | **Decided** | `Version.cpp` comment notes the rename shifts payload offset in `.sav` headers. External parsers must scan, not assume a fixed offset |
| 4.10 | Existing users migrating Qt5 to Qt6 | **Open** | Shipped `8705a96b...` versus selected `617321bf...`. Config and profile schema differences are unverified |

---

## 5. Platform minimums and architecture policy

| # | Item | Label | Answer or status |
|---|---|---|---|
| 5.1 | Platforms | **Decided** | Windows 10 and 11, native Linux, Steam Deck, native macOS |
| 5.2 | Windows minimum | **Confirm** | `build_info.txt` records Windows 10 build 18362 and Windows 11 build 22000. Adopt as the launcher minimum? |
| 5.3 | macOS minimum | **Decided** | macOS 11 Big Sur or newer, aligned with Dolphin |
| 5.4 | Apple Silicon | **Decided** | Must be supported |
| 5.5 | macOS Intel | **Open** | Desirable, undecided |
| 5.6 | Linux packaging | **Confirm** | Blueprint proposes AppImage first, Flatpak desirable. Confirm the priority |
| 5.7 | Architecture | **Confirm** | x86-64 everywhere plus arm64 on macOS. Linux arm64 assumed out of scope |
| 5.8 | Runtime prerequisites | **Research** | `build_info.txt` names VC++ redistributable 14.44.35211.0 with an install URL. The installer must detect or install it |

---

## 6. Steam Deck

| # | Item | Label | Answer or status |
|---|---|---|---|
| 6.1 | Gaming Mode operation | **Decided** | Controller-first, required |
| 6.2 | Resolution target | **Decided** | 1280x800 |
| 6.3 | Steam library integration | **Decided** | Automate the shortcut where possible |
| 6.4 | Verification | **Decided** | CI-built, marked unverified until hardware exists |
| 6.5 | Flatpak sandbox permissions | **Research** | Needs game data, controller access, and permission to launch the bundled runtime |
| 6.6 | Battery-aware presets | **Decided** | Required |
| 6.7 | Steam Input conflicts | **Decided** | Detect and explain. Currently unbuilt |

---

## 7. Xbox Dev Mode

| # | Item | Label | Answer or status |
|---|---|---|---|
| 7.1 | One PC-side wizard | **Decided, scope reduced** | The wizard is a **safe file mover**, not an app deployer. Verified source folder: `Downloads\NFL Street Reborn Xbox Dev Mode 2.1(1)\NFL Street Reborn Xbox Dev Mode 2.1`, 2.9 GB, no nested duplicate. Payload is `Config\`, `GameSettings\GN7E69.ini`, `GC\` (16 MB memory card plus SRAM), `Load\Textures\GN7E69\`, `Cache\GameCovers\GN7E69.png` |
| 7.2 | Seven wizard modes | **Decided** | New, Update, Repair USB, Back Up Save, Restore Save, Deploy or Reinstall App, Test Connection |
| 7.3 | USB safety rules | **Decided** | Never auto-pick a drive, never format without separate confirmation, refuse system disks, preserve `GC\` saves, timestamped backups, safe eject |
| 7.4 | Existing layout | **Decided** | `Config\`, `GameSettings\GN7E69.ini`, `GC\` with `MemoryCardA.USA.raw` and `SRAM.raw`, `Load\Textures\`, `NSR.png` |
| 7.5 | Nested duplicate | **Decided** | Confirmed present. Normalize it |
| 7.6 | Source of the UWP app package | **Decided, not needed** | Adam, batch 2: the installer just moves all the files for the user. **No app deployment, no Device Portal, no certificate.** The wizard copies the prepared payload to the USB drive, verifies it, and preserves saves. The user installs the Xbox app themselves as they do today |
| 7.7 | `E:\Games` assumption | **Research** | Not verified. `Dolphin.ini` and `GCPadNew.ini` in `Step 2\Config\` were not read out |
| 7.8 | WGInput mapping | **Research** | Same, not read out |
| 7.9 | Console pairing credentials | **Decided** | Windows credential store. Never logged |
| 7.10 | Xbox scope for September 12 | **Decided** | In scope, and much cheaper than assumed. Drive picker, copy, verify, preserve `GC\`, safe eject. No Device Portal work |

---

## 8. Game import, hashes, and retained base

**This section is the project's biggest unknown and it blocks the content model.**

| # | Item | Label | Answer or status |
|---|---|---|---|
| 8.1 | Accepted sources | **Decided** | ISO, RVZ, extracted folder, physical disc where supported |
| 8.2 | Required revision | **Decided** | USA GameCube, `GN7E69` |
| 8.3 | Import once, forever | **Decided** | Retain a verified private base, never ask again |
| 8.4 | Data location | **Decided** | Roaming AppData. Already true today |
| 8.5 | Does any real patch exist | **Research, ANSWERED, NEGATIVE** | **No.** `rebornpatcherv6.py` calls a function named `fake_patch` that sleeps through twelve hardcoded messages for about 14 seconds and never opens the selected file. The shipped `NSR.iso` is a complete prebuilt image. The Xbox package does the same with `nsrpatch.iso` |
| 8.6 | Base-plus-patch or ship-the-image | **Decided** | **Always ship the finished image.** Adam, batch 1. Import becomes optional verification at most. Consequences: no differential delivery for game content, ~1.5 GB per game-content update, and the legally owned source requirement stays unenforced |
| 8.7 | Accepted source hashes | **Open** | No hash allowlist exists. Redump values would need sourcing |
| 8.8 | Disk budget | **Confirm** | Base plus offline plus online is roughly 4.4 GB of images, plus about 2.0 GB textures, plus runtime. Roughly 7 GB. Acceptable? |
| 8.9 | Physical disc dumping | **Confirm** | Listed as "where platform support permits". Realistically Windows-only and niche. Defer to post-launch? |
| 8.10 | Existing users who never had a real source | **Open** | Every current user got `NSR.iso` from the installer without providing anything. If import becomes real, they get asked for a disc they may not own. Migration policy needed |

---

## 9. Offline and online builds

| # | Item | Label | Answer or status |
|---|---|---|---|
| 9.1 | Two builds | **Decided** | Offline is customizable, online is standardized |
| 9.2 | Online build name | **Decided** | `NSRONLINE.iso` unless the technical design chooses another container |
| 9.3 | Music is offline only | **Decided** | Explicit |
| 9.4 | Texture mods allowed online | **Decided** | Client-side, permitted |
| 9.5 | Automatic verify and repair before matchmaking | **Decided** | Required |
| 9.6 | Relationship to `FR4N31.iso` | **Confirm** | `FR4N31.iso` dated May 8, 2026 appears to be the existing online build. Is it the ancestor of `NSRONLINE.iso`? |
| 9.7 | Relationship to `CR4E12.iso` | **Confirm** | Creation League build, dated July 26, 2026. League is out of scope, so does the launcher ignore it entirely? |
| 9.8 | Distinct game IDs | **Confirm** | `GN7E69` stock, `FR4N31` online, `CR4E12` league. Does `NSRONLINE.iso` keep `FR4N31`? Game ID drives texture folder and `GameSettings` naming |

---

## 10. Controllers

| # | Item | Label | Answer or status |
|---|---|---|---|
| 10.1 | Device families | **Decided** | Xbox, DualShock 4, DualSense, Switch Pro, generic, GameCube adapter, Steam Input, Steam Deck, keyboard |
| 10.2 | Four local players | **Decided** | Required |
| 10.3 | PlayStation is a priority | **Decided** | Explicit. Long-standing pain point |
| 10.4 | Current state | **Research, ANSWERED** | `GCPadNew.ini` binds `[GCPad1]` to `XInput/0/Gamepad`. The `PS Controls` mod contains **only** `User\Load\Textures\`. So PlayStation support today is Xbox bindings plus PlayStation button art. **No device backend exists** |
| 10.5 | Acceptance matrix | **Decided** | Twelve scenarios in blueprint section 7. All twelve are currently unbuilt |
| 10.6 | Full Dolphin-equivalent configuration | **Decided** | Dead zones, calibration, trigger thresholds, rumble, sensitivity |
| 10.7 | Profiles keyed to physical device | **Decided** | Required, persisting across updates |
| 10.8 | Prompt textures follow the controller | **Decided** | Automatic |
| 10.9 | SDL3 versus Dolphin's own input | **Open** | SDL3 drives launcher navigation, but Dolphin has its own input stack. Does the launcher generate `GCPadNew.ini` for Dolphin, or does Dolphin gain an SDL3 path? Affects how much of the fork changes |
| 10.10 | Account-synced profiles | **Decided** | Optional, later |

---

## 11. Mods and marketplace

| # | Item | Label | Answer or status |
|---|---|---|---|
| 11.1 | Initial categories | **Decided** | Music Packs, Balls, Controller Prompts |
| 11.2 | Curated only at launch | **Decided** | Yes, with user submission and approval |
| 11.3 | Package metadata | **Decided** | ID, name, version, author, category, description, art, size, hashes, dependencies, conflicts, minimum versions, changelog |
| 11.4 | Atomic activate and deactivate with exact restore | **Decided** | Replaces JSGME. Interrupted-install recovery required |
| 11.5 | Named mod profiles | **Decided** | Not required. Two builds provide the separation |
| 11.6 | Existing catalog to migrate | **Decided** | 13 JSGME entries: 7 footballs (Aqua, Blue, Camo, Green, Panda, Pink, Purple), PS Controls, Xbox Controls, Patriots Throwback, Saints Alternate, plus `!BACKUP` and `!INSTLOGS` control folders |
| 11.7 | Package format | **Open** | Not specified. All current mods are pure `User\Load\Textures\` overlays, which makes a first format simple |
| 11.8 | JSGME redistribution rights | **Research** | Third-party freeware. Terms need checking if it ships during a transition |
| 11.9 | Migrating a user mid-JSGME | **Confirm** | A user with mods enabled has overlaid files plus `!BACKUP`. Migration must un-apply through JSGME's own backup before importing, or the base is polluted |
| 11.10 | Marketplace governance | **Deferred** | Adam: "Marketplace details we'll work on later" |

---

## 12. Music editor

| # | Item | Label | Answer or status |
|---|---|---|---|
| 12.1 | Twenty-one slots | **Decided** | Confirmed in `music_editor.py` |
| 12.2 | Input formats | **Decided** | WAV, MP3, FLAC, M4A, OGG, AAC, WMA, Opus, AIFF via FFmpeg |
| 12.3 | Full suite | **Decided** | Preview, waveform, trim, fades, volume, normalization, validation, rebuild estimate |
| 12.4 | Metadata | **Decided** | Title, artist, album, cover art, optional explicit label |
| 12.5 | `.nsmp` packs | **Decided** | Import, export, share, submit. Handled by `music_pack.py` |
| 12.6 | Restore | **Decided** | Per slot and global |
| 12.7 | Soundtrack profiles | **Decided** | Yes, with leeway for individual slots too |
| 12.8 | Any-length songs | **Decided** | Solved. PFDx TOC plus virgin-guard fixes remove the 3:57 limit |
| 12.9 | **How the engine ships** | **Decided** | **Embedded in the launcher.** Adam, batch 1. Full editing UI inside the Music page, no external window, no separate tool. See 12.12 for the open mechanism question this creates |
| 12.10 | `.nsmp` schema | **Research** | `music_pack.py` is the authority. Not read in this pass |
| 12.11 | FFmpeg distribution | **Confirm** | Required at runtime. LGPL or GPL build choice affects bundling |
| 12.12 | **Rebuild mechanism, created by 12.9** | **Decided** | Adam, batch 2: whatever is best. **Extract on demand and cache.** The launcher extracts the shipped ISO to a working folder the first time a user edits music, keeps it cached, and reuses it. Bundles Python and FFmpeg. Preserves the proven chain including the any-length-song fix, and avoids doubling install size for users who never touch music. A native Rust rewrite of the PFDx and `template.dat` writers is the follow-up, not the launch path |

---

## 13. Discord, identity, account actions

| # | Item | Label | Answer or status |
|---|---|---|---|
| 13.1 | Discord OAuth for online identity | **Decided** | Preferred method |
| 13.2 | Optional for offline | **Decided** | Play Now never requires an account |
| 13.3 | Rich Presence | **Decided** | Must-have. Menus, offline play, casual queue, ranked queue, private lobby, live match |
| 13.4 | Minimal scopes | **Decided** | Identity only |
| 13.5 | Token storage | **Decided** | OS credential vault |
| 13.6 | Unlink, sign out, export, delete | **Decided** | All required |
| 13.7 | Existing Discord application | **Confirm** | `server\.env` already holds `DISCORD_CLIENT_ID`, `DISCORD_CLIENT_SECRET`, `DISCORD_REDIRECT_URI`. So an app exists. Reuse it or create a fresh one? **It must be rotated either way** |
| 13.8 | Community Discord | **Decided** | `https://discord.gg/Uac7RKHWY2`, in the shipped README |
| 13.9 | Join links from Discord | **Decided** | Eligible private lobbies, via `reborn://join/<code>` |

---

## 14. Backend, hosting, budget

| # | Item | Label | Answer or status |
|---|---|---|---|
| 14.1 | Backend required | **Decided** | Auth, Elo, matchmaking, lobbies, chat, results, moderation, manifests |
| 14.2 | Actual gameplay stays P2P | **Decided** | Dolphin NetPlay |
| 14.3 | Existing prototype | **Decided** | Node plus SQLite. `elo.js`, `liveState.js`, `db\schema.sql`, `middleware\auth.js`, `routes\{admin,auth,matches,users}.js`, `socket\{queue,match,custom}.js` |
| 14.4 | Host | **Decided** | **Split.** Website stays on SiteGround. The matchmaking and real-time backend goes on **Railway**. Adam, batch 1 and batch 2 |
| 14.5 | Database in production | **Open** | SQLite prototype. Postgres for production? |
| 14.6 | Object storage and CDN | **Open** | Undecided. Needed for content and marketplace delivery |
| 14.7 | Domain plan | **Confirm** | `nflstreetreborn.com` exists. Subdomain for the API? |
| 14.8 | Budget | **Open** | Adam: "16 we broke" regarding code signing. Hosting budget unstated |
| 14.9 | Relationship to the existing PHP site | **Decided** | **Completely new backend.** Adam, batch 1. The launcher does not post to `ingest.php`. The league site keeps its own ingest separately |
| 14.11 | **SiteGround real-time constraint, created by 14.4** | **Decided** | **Matchmaking backend goes on Railway**, separate from SiteGround. Adam, batch 2. Railway runs persistent Node processes and WebSockets, which is exactly the prototype's shape |
| 14.10 | Credential rotation | **Deferred** | Adam, batch 1: later. Three sets outstanding: the compiled-in bearer token, `JWT_SECRET`, the Discord client secret. **Blocks pushing the fork to any public remote** |

---

## 15. Matchmaking

| # | Item | Label | Answer or status |
|---|---|---|---|
| 15.1 | Modes | **Decided** | Casual 1v1, Ranked 1v1, Private 1v1 |
| 15.2 | Strictly 1v1 | **Decided** | No 2v2 |
| 15.3 | Ping is the priority input | **Decided** | Region and ping first, then Elo |
| 15.4 | Lobby codes and invite links | **Decided** | Required |
| 15.5 | Text chat | **Decided** | Pre-match. No voice |
| 15.6 | Friends list | **Decided** | Not required |
| 15.7 | Spectating | **Decided** | Out of scope. Results are viewable |
| 15.8 | NetPlay browser hidden | **Decided** | Never exposed in normal use |
| 15.9 | Standardized NetPlay config | **Decided** | Launcher-generated. Input delay from a small approved range based on ping |
| 15.10 | Rollback netcode | **Decided** | Explicitly deferred. Do not let it influence architecture |
| 15.11 | How NetPlay is actually driven | **Open** | `netplay_uia.ps1` in one online copy suggests the prototype **automated Dolphin's NetPlay UI with Windows UI Automation**. That does not port to Linux or macOS. Does the fork need a real command-line or IPC NetPlay entry point? |
| 15.12 | Queue cancellation and timeouts | **Open** | Unspecified |
| 15.13 | Chat moderation retention | **Deferred** | "Limited to the period required for abuse moderation". Exact period undecided |

---

## 16. Results, disputes, Elo

| # | Item | Label | Answer or status |
|---|---|---|---|
| 16.1 | Winner determination | **Decided** | Six-link identity chain from Discord account to final score |
| 16.2 | What already works | **Research, ANSWERED** | The bottom three links exist. `NflsPlayHide` resolves NetPlay port to in-game team. `NflsStatTracker` emits `sides_verified`, `team_totals`, `players[14]`, `teams[2]`. Only the account and matchmaking links are missing |
| 16.3 | Two-client confirmation | **Decided** | Both submit under the server match ID. Agreement auto-accepts |
| 16.4 | Conservative failure policy | **Decided** | Crash, desync, and conflict are held for review. Elo does not move |
| 16.5 | Explicit quit in ranked | **Decided** | Likely forfeit if opponent and session evidence agree |
| 16.6 | Reversibility | **Decided** | Moderation can reverse any result |
| 16.7 | Elo formula and K-factor | **Open** | `elo.js` exists in the prototype but its formula was not read. Provisional-player behavior undecided |
| 16.8 | Private match Elo | **Decided** | Record the result, do not change Elo |
| 16.9 | Score limit as evidence | **Decided** | The site auto-applies at 36 today. Supporting signal only, never sole proof |
| 16.10 | Grace period lengths | **Open** | Needs real-world testing. Blueprint says prefer "held for review" |

---

## 17. Statistics

| # | Item | Label | Answer or status |
|---|---|---|---|
| 17.1 | All games tracked | **Decided** | Offline and online, clearly distinguished |
| 17.2 | Views | **Decided** | Match history, box scores, player and team leaders, head-to-head, streaks, field performance, lifetime totals, Elo, recent opponents |
| 17.3 | Season summaries and achievements | **Decided** | Out of scope |
| 17.4 | Offline private by default | **Decided** | Ranked and casual results are public |
| 17.5 | Capture stays in Dolphin, upload moves out | **Decided** | Explicit |
| 17.6 | Local durability | **Decided** | Write before upload, retry with backoff, idempotent, resume after restart |
| 17.7 | Incomplete games | **Decided** | Held for review, case by case |
| 17.8 | **The offline gate is currently off** | **Decided, fix scheduled** | `nsr_track_offline.txt` is present in the selected runtime. Its README states this causes every offline CPU game to upload, and league-team games can land on real fixtures |
| 17.9 | Two capture mechanisms | **Research, ANSWERED** | Production is the in-emulator `NflsStatTracker`. The `Stat Tracking\Start.dol.statread1` DOL hook is a separate research path needing a modified `Start.dol` and an ISO rebuild. Do not confuse them |
| 17.10 | Retention | **Open** | Unspecified |

---

## 18. Updates, repair, rollback

| # | Item | Label | Answer or status |
|---|---|---|---|
| 18.1 | Eight independently updateable components | **Decided** | Launcher, runtime, offline patch, online patch, textures, mods, controller db, news config |
| 18.2 | Game-like automatic updates | **Decided** | User does nothing |
| 18.3 | Signed manifests, hash verification, atomic swap | **Decided** | Required |
| 18.4 | Differential delivery | **Decided** | Required, "no unnecessary multi-gigabyte redownloads" |
| 18.5 | **Differential delivery is blocked** | **Open** | Depends on 8.6. Today there is no diff between retail and `NSR.iso`; the whole 1.46 GB image ships. Until a base-plus-patch model exists, game content cannot be delivered differentially |
| 18.6 | Rollback | **Decided** | Keep one prior launcher and runtime version. Auto-rollback on failed health check |
| 18.7 | Update channels | **Decided** | None. Stable only |
| 18.8 | Manifest signing key custody | **Open** | Where does the private key live? |
| 18.9 | Forced-online minimum version | **Confirm** | Implied by "expected online build version". Confirm the launcher blocks queueing below it |
| 18.10 | Current updater | **Research** | `NSR_UpdateChecker.exe`, 128 KB, no source located. Its endpoint is unknown |

---

## 19. Saves, backup, sync

| # | Item | Label | Answer or status |
|---|---|---|---|
| 19.1 | Save protection before risky operations | **Decided** | Recovery point before updates, repairs, migrations, mod and music operations |
| 19.2 | Manual backup, restore, export, import | **Decided** | Desirable despite difficulty |
| 19.3 | Cross-device sync | **Decided** | Optional, after reliable local protection exists |
| 19.4 | Uninstall preserves user data | **Decided** | Saves, music sources, mods, screenshots preserved by default |
| 19.5 | **Current uninstall deletes everything** | **Decided, fix scheduled, backup first** | `CurUninstallStepChanged` runs `DelTree({app}, True, True, True)` over a folder containing `User\GC` memory cards |
| 19.6 | **Current update backup silently fails** | **Decided, fix scheduled, backup first** | `BackupUserData` and `RestoreUserData` call Inno Setup's `FileCopy` with wildcard paths. `FileCopy` copies a single file and does not expand wildcards, so both log a failure and continue. User data survives only by the `onlyifdoesntexist` flag |
| 19.7 | Conflict handling for sync | **Open** | Undecided |
| 19.8 | Memory card format | **Research** | `MemoryCardA.USA.raw` plus `SRAM.raw`. Portable across platforms, verification pending |

---

## 20. Accessibility and localization

| # | Item | Label | Answer or status |
|---|---|---|---|
| 20.1 | Text scaling, reduced motion, high contrast, color-safe status, screen readers, keyboard access | **Decided** | All required |
| 20.2 | Externalized strings | **Decided** | Localization-ready. No translations committed |
| 20.3 | Ten-foot layout | **Decided** | Television and Steam Deck legibility |
| 20.4 | Controller focus visibility | **Decided** | More than a color shift. Border plus elevation or scale plus background change |
| 20.5 | Launch languages | **Confirm** | US English only assumed at launch |
| 20.6 | Accessibility conformance target | **Open** | WCAG 2.2 AA is the sensible default for contrast and focus. Confirm as the internal bar |

---

## 21. Migration from the current release

| # | Item | Label | Answer or status |
|---|---|---|---|
| 21.1 | Replace, not install alongside | **Decided** | Explicit |
| 21.2 | What migrates | **Decided** | Saves, memory cards, screenshots, controller config, textures, JSGME mods where convertible, custom soundtrack, graphics settings, ReShade presets |
| 21.3 | Backup before altering the old install | **Decided** | Required |
| 21.4 | Source layout to read | **Decided** | `%APPDATA%\NFL Street Reborn` with `Games\NSR.iso`, `User\`, `MODS\`, Qt5 Dolphin |
| 21.5 | Controller config does not migrate cleanly | **Confirm** | `GCPadNew.ini` is XInput-only and the new model is per-device SDL3. Recommend re-running controller setup rather than importing |
| 21.6 | Users with no legally owned source | **Open** | Same as 8.10. Depends on batch 1 question 1 |
| 21.7 | Mid-JSGME users | **Confirm** | Same as 11.9 |
| 21.8 | Qt5 to Qt6 config compatibility | **Open** | Same as 4.10 |

---

## 22. Crash, diagnostics, privacy redaction

| # | Item | Label | Answer or status |
|---|---|---|---|
| 22.1 | No telemetry, no analytics, no automatic crash upload | **Decided** | Privacy is core |
| 22.2 | Friendly error page with six recovery actions | **Decided** | Relaunch, Verify and Repair, Start with Mods Disabled, Start with Performance Graphics, Open Reborn Dolphin, Create Diagnostic Bundle |
| 22.3 | No silent restart loop | **Decided** | Track repeated local crash signatures without uploading |
| 22.4 | Diagnostic bundle contents | **Decided** | Logs, versions, hashes, mod list, graphics config, controller summary, exit code, redacted system summary |
| 22.5 | Redaction rules | **Open** | Paths contain the Windows username. Bundles must scrub before the user shares |
| 22.6 | P2P IP disclosure | **Decided** | Disclosed in onboarding. Raw IPs never become public profile data |

---

## 23. Distribution, signing, licensing

| # | Item | Label | Answer or status |
|---|---|---|---|
| 23.1 | Code signing | **Decided** | Not available. Adam: "16 we broke". Windows SmartScreen and macOS Gatekeeper warnings are a known limitation |
| 23.2 | macOS notarization | **Decided** | Blocked until an Apple Developer account exists |
| 23.3 | Dolphin GPLv2+ obligations | **Decided** | Attribution plus corresponding source for the modified emulator. Keep the fork in a separate public repository |
| 23.4 | Attribution already owed | **Decided** | BlueSwordM and nfl_numbers, per the shipped README |
| 23.5 | Third-party components needing license review | **Open** | JSGME, ReShade, `ReshadeEffectShaderToggler.addon64`, `dxgi.dll`, FFmpeg, Qt6, SDL3, `wit` (`wit-v3.05a-r8638-cygwin64.zip` is in the installer folder), fonts, icons |
| 23.6 | Qt licensing | **Confirm** | Dolphin ships Qt under LGPL. The launcher uses Tauri and a system webview, so it should carry no Qt obligation of its own |
| 23.7 | Shipping a complete game image | **Open** | Currently the product does this. Same decision as batch 1 question 1 |
| 23.8 | Where releases are hosted | **Open** | Undecided |

---

## 24. Release acceptance and the September 12 target

| # | Item | Label | Answer or status |
|---|---|---|---|
| 24.1 | Target date | **Decided** | September 12, 2026. Fourteen days from the blueprint date |
| 24.2 | Full scope retained | **Decided** | Adam: "we're doing it all". Recorded, not argued |
| 24.3 | Risks recorded honestly | **Decided** | Single developer, no Mac hardware, no Steam Deck hardware, no signing credentials |
| 24.4 | **What September 12 means** | **Decided** | **Completely finished public production release, all platforms.** Adam, batch 1. Recorded as the target. The named blocker is that no macOS Dolphin build exists in any form, see 4.7 and batch 2 question 7 |
| 24.5 | Per-platform acceptance criteria | **Open** | Windows can be tested. Linux, Steam Deck, and macOS cannot be verified without hardware |
| 24.6 | Definition of done for unverifiable platforms | **Open** | "CI builds and starts" versus "a human played a match on it" |
| 24.7 | Existing users during transition | **Open** | Does the current 2.1 installer keep working while the launcher is built? |

---

## Batch 1: asked and answered

Asked August 29, 2026. Adam's answers verbatim in bold.

| # | Question | Answer | Result |
|---|---|---|---|
| 1 | Build the game from the user's copy, or keep shipping the finished image? | **"Keep shipping finished image always."** | Decided. Recorded at 8.6. Import is verification at most. No differential delivery for game content |
| 2 | Commit and push the Dolphin fork before anything else? | **"Yeah but it's like not done at all."** | Done. Committed `eca379f` on `nfls-reborn/preserve-2026-08-29`, labeled a WIP snapshot, local only. Recorded at 3.5 |
| 3 | Rotate the three credential sets now? | **"We'll do it later."** | Deferred. Recorded at 14.10. Blocks pushing the fork publicly |
| 4 | Which online copy is canonical, and who owns those GitHub accounts? | **"Idk what u talking abt."** | Not understood. Re-asked in plainer terms as batch 2 question 1. Nothing touched |
| 5 | How should music editing ship? | **"Needs to be embedded in the launcher."** | Decided. Recorded at 12.9. Creates the new open item 12.12 |
| 6 | Fix the two save-destroying bugs in 2.1, or only in the launcher? | **"What?"** | Not understood. Re-asked in plainer terms as batch 2 question 2 |
| 7 | Existing PHP endpoint or a new backend, and where hosted? | **"New backend completely, siteground."** | Decided. Recorded at 14.4 and 14.9. Creates the new open item 14.11 |
| 8 | What does September 12 mean? | **"COMPLETELY FINISHED!"** | Decided. Full public production release, all platforms. Recorded at 24.4 |
| 9 | Confirm the debug toggles get cleared structurally? | not answered | Carried to batch 2 question 3 |
| 10 | Is there an Xbox Dev Mode app package anywhere? | not answered | Carried to batch 2 question 6 |

---

## Batch 2: seven questions

Two re-asks in plainer language, two carried over, three created by the batch 1 answers.

---

**1. There are three copies of the online matchmaking app on your PC, and two GitHub accounts attached to them. Do you recognize either name?**

Plainer version of batch 1 question 4. In `C:\Users\Adam\Downloads` there are three folders that all contain a program called `nfl-street-reborn-online`. It is the Electron matchmaking app with the queue, the lobby, and the Elo server.

One of those folders is set up to sync with two GitHub accounts:

- `MiltyTheGreat`
- `michaelpenixfan2390`

- Do you know who those are? A friend, a collaborator, an old account of yours?
- The three copies are not identical. One has settings screens the others do not. A different one has a script called `netplay_uia.ps1` that appears to click through Dolphin's NetPlay window automatically, which nothing else does.
- I have not touched any of them. I just need to know whether we are allowed to build on that code, and whether you want me to keep all three or pick one.

If you would rather not deal with this, say "keep all three, rebuild from scratch" and I will treat all of it as reference only.

---

**2. The installer that is live right now deletes people's saves when they uninstall. Do you want that fixed?**

Plainer version of batch 1 question 6. Two real bugs in `rebornsetupv10.iss`, the setup people download today:

- **Uninstalling wipes memory cards.** The last thing the uninstaller does is delete the whole install folder, and their memory cards live inside it. Anyone who uninstalls loses their franchise saves. There is no "keep my saves" option.
- **The update backup does not work.** The script tries to back up the `User` folder before updating, but it uses a command that only copies one file at a time and it was given a whole folder. So it fails and moves on. It has probably never once worked. Updates also delete the entire `MODS` folder, which is where the mod manager keeps its undo data.

- **A.** Fix 2.1 now so nobody else loses saves before the launcher ships.
- **B.** Leave it. The launcher replaces it soon anyway.
- **C.** Do not fix 2.1, but make sure the launcher's migration rescues saves before it touches anything.

---

**3. Carried over. The Dolphin build you picked is currently set to upload every offline game to the live league site.**

There are two files sitting in `CREATION LEAGUE AND NEW DOLPHIN\User\`: `nsr_track_offline.txt` and `nsr_debug.txt`. The first one turns off the safety check that limits stat uploads to online games. That folder's own README says so:

> If it ships, every player's OFFLINE games against the CPU get captured and uploaded, and if they happen to pick two league teams, those fake results land on a real fixture.

I have not deleted them.

- **A.** Make it structural. When the launcher packages the runtime it strips both files, and the gate becomes a compiled-in setting so dropping a text file cannot defeat it.
- **B.** Just remember to delete them at packaging time.
- **C.** Keep the override for testing, but give it a mechanism that cannot ship by accident.

---

**4. Created by your answers. The music editor now has to rebuild the game on the player's PC. How?**

You said keep shipping the finished image, and you said music editing has to be embedded in the launcher. Those two together mean the launcher has to rebuild the offline ISO on the user's machine whenever they change a song, because we are not shipping a patch system that could do it server-side.

The proven tool chain (`music_editor.py` driving `extend_slot_40big.py`, `fix_pfdx_toc.py`, `edit_music_meta.py`, `build_iso_with_game12.py`) does not work on an ISO. It works on the **extracted game filesystem**. So one of these has to happen:

- **A.** Ship an extracted copy of the game alongside the ISO, plus a bundled Python and FFmpeg. Preserves your working code exactly. Costs roughly another 1.5 GB of disk, a slow rebuild, and it is awkward on macOS and Linux.
- **B.** Rewrite the PFDx and `template.dat` writers natively in Rust so the launcher can rebuild without Python. Clean, fast, one payload, cross-platform. Real work, and it risks regressing the any-length-song fix you already solved.
- **C.** Launcher extracts the ISO to a working folder on first music edit, uses it, keeps it cached. Middle ground on disk, still needs Python and FFmpeg bundled.

*Recommendation: C. It avoids doubling the install size up front and it preserves the proven chain. B is the right long-term answer.*

---

**5. Created by your answer. SiteGround probably cannot run the matchmaking server. Is a second host acceptable?**

You said new backend, on SiteGround. SiteGround is fine for the website and for a normal request-and-response API. Matchmaking is the problem: a queue that pairs two players and a lobby that both clients stay connected to needs a process that runs continuously and holds open WebSocket connections. Shared hosting plans normally do not allow either. The existing prototype is exactly that shape, a Node server with `socket\queue.js`, `socket\match.js`, and `socket\liveState.js`.

- **A.** Website and API stay on SiteGround, matchmaking and real-time move to a small separate host. Most likely to work.
- **B.** All on SiteGround, and I redesign matchmaking around polling instead of sockets. Possible, feels worse, adds queue latency.
- **C.** Check your SiteGround plan first. If it is a cloud or dedicated plan rather than shared, A becomes unnecessary. Tell me the plan and I will confirm.

*Recommendation: C first, then A if it is shared hosting.*

---

**6. Carried over. Is there an Xbox app package anywhere?**

To install onto an Xbox in Dev Mode, the wizard needs an actual app package (`.appx` or `.msix`), a certificate, and its dependencies, uploaded through Device Portal. The `NFL Street Reborn Xbox Dev Mode 2.1` folder has only the USB payload: settings, memory card data, textures, cover art, and the same fake patcher. No package, no certificate, no deployment tooling anywhere I looked.

- Do you have the Xbox Dolphin package somewhere else, maybe on the Xbox itself or a different drive?
- Is it a public Dolphin UWP build? If so, which one, so I can pin the version?
- If nothing exists, the Xbox wizard becomes "prepare and verify the USB" for v1 and app deployment waits. Acceptable?

---

**7. Created by your answer. September 12 is a full release on all platforms, and there is no macOS Dolphin.**

You said completely finished, so that is the target and I am building to it. One blocker needs naming rather than discovering on September 10.

There is no macOS Dolphin build anywhere in your material. The only macOS artifact I found is a zero-byte file named `PUT_DOLPHIN_APP_HERE.txt`. There is also no Linux build, no Mac, no Steam Deck, and no Apple Developer account. So for macOS specifically, the emulator itself has to be compiled first, and neither of us can run it.

- **A.** I set up CI to build Reborn Dolphin for macOS and Linux and we ship them untested, with a plain warning in the launcher. Everything ships on the 12th, some of it unverified.
- **B.** You get access to a Mac before the 12th, borrowed, cloud, or otherwise, and we verify.
- **C.** Windows is verified on the 12th, Linux and macOS ship the same day flagged as beta, and drop the beta flag once someone confirms them.

*Recommendation: A or C. Both ship everything on the date. C is more honest to users about what has been tested.*

---

## Change log

| Date | Change |
|---|---|
| 2026-08-29 | Created from blueprint, transcript, START_HERE, UX brief, and the read-only inventory. Batch 1 issued |
| 2026-08-29 | Batch 1 answered. 8.6, 3.5, 12.9, 14.4, 14.9, 24.4 decided; 14.10 deferred; 3.6, 3.7 not understood and re-asked. New open items 12.12 and 14.11 created by the answers. Fork preserved as `eca379f`. Batch 2 issued |
