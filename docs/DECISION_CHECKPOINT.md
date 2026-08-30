# Decision Checkpoint

## NFL Street Reborn Launcher

**Date:** August 30, 2026
**Covers:** `CLAUDE_START_HERE.md` step 7
**Read this before implementation starts.** The protocol requires your approval here.

Thirteen days to September 12. One developer. **Zero lines of launcher code written.**

---

## 1. What was found

Five bodies of work that shared nothing but a hard drive.

| | Finding |
|---|---|
| **The patcher never patched** | `rebornpatcherv6.py`'s core function is named `fake_patch`. It sleeps through twelve hardcoded messages for about 14 seconds and never opens the file the user selected. The playable game is the shipped `NSR.iso` |
| **Your Dolphin work was uncommitted** | All four `Nfls*` sources untracked, twelve upstream modifications uncommitted, on one machine. Now preserved at `eca379f` |
| **Uninstall deletes saves** | `DelTree({app}, True, True, True)` over a folder containing `User\GC` memory cards |
| **Update backup never worked** | `BackupUserData` passes wildcard paths to Inno's `FileCopy`, which copies one file and does not expand wildcards. It has always failed silently |
| **PlayStation support is cosmetic** | `GCPadNew.ini` binds to `XInput/0/Gamepad`. The `PS Controls` mod is textures only. No device backend exists |
| **Both runtimes now build** | Linux and macOS, from clean checkout, all four Reborn objects verified present by a gate step |
| **Three latent defects found by building** | `NflsUiSplit.cpp` absent from the CMake file list while two files called it; `NflsUiSplit.h` used `u32` with no include; upstream `Resources.h` declared `QIcon` without including it |
| **The architecture contradicted itself** | 172 internal contradictions across six documents, including two opposite safety rules for the same memory card file |

---

## 2. What is authoritative

| Artifact | Status |
|---|---|
| `docs/architecture/RECONCILIATION_RULINGS.md` | **Wins over every other document.** 154 rulings |
| `docs/architecture/SYSTEM_ARCHITECTURE.md` section 4 | Sole authority for the on-disk layout |
| `docs/discovery/PROJECT_QUESTIONNAIRE.md` | Sole authority for what you decided |
| `docs/design/*` | Settled. Ten deliverables, no blockers |
| `adampreddy/reborn-dolphin` branch `nfls-reborn/preserve-2026-08-29` | Sole authority for the runtime source |
| `nflstreet2_clean\GameCube Version\root` plus `tools\` | Sole authority for game content |
| The four generated architecture documents | **Reference, superseded by the rulings.** Richly researched, still partly contradictory |

---

## 3. Reusable immediately

| Component | Verdict | Note |
|---|---|---|
| `NflsPlayHide` | WRAP | Already resolves NetPlay port to in-game team. Half the winner chain |
| `NflsStatTracker` | WRAP, additive | Captures teams, score, 12 categories, 14 players. Needs one new key |
| `NflsUiSplit` | REUSE | Ships, not a release gate |
| Reborn Dolphin runtime | PORT | Builds on all three platforms now |
| `music_editor.py` and its chain | WRAP | Proven, including the any-length-song fix |
| `music_pack.py`, `.nsmp` | REUSE | |
| The 13 JSGME directories | 11 packages migrate | 2 are control folders |
| `PS Controls`, `Xbox Controls` | REUSE as prompt packs | That is all they ever were |
| Xbox `Step 2` layout | REUSE the shape | Every stateful file inside it is retired or replaced |
| `elo.js`, `schema.sql`, socket handlers | HARVEST | Shape is right, rebuild against Postgres |
| ReShade, `NSRShaderlessMenu` | REUSE, Windows only | |

---

## 4. Must be rewritten

| Component | Verdict | Why |
|---|---|---|
| The patcher | RETIRE | It never worked. Replaced by copy plus verify |
| `NSR_UpdateChecker.exe` | RETIRE | No source located |
| `rebornsetupv10.iss` | REPLACE | Deletes saves; backup never worked |
| JSGME plus `!BACKUP` | RETIRE | Composition removes the failure entirely |
| `GCPadNew.ini`, `Profiles\GCPad\` | REPLACE | Whole model changes to per-device SDL3 |
| Online client | REBUILD | Three divergent copies, all reference |
| `netplay_uia.ps1` | RETIRE | Windows UI Automation does not port |
| `NflsStatUpload` | RETIRE from Dolphin | Logic moves to the launcher |
| Shipped `Dolphin.exe` (`8705a96b`, Qt5) | REPLACE | By `617321bf`. A migration, not a swap |

---

## 5. Risky or unverified

Ranked by what actually hurts.

| # | Risk | Status |
|---|---|---|
| 0 | A second Dolphin tree exists at `nflstreet2_clean\dolphin` holding abandoned netcode experiments, about 108 KB, on a June 2026 base. **Not the product**, per Adam. Preserved at `6de5eb643b` purely so the experiments are not lost | Closed as a risk. Kept as an archive |
| 1 | **Nobody has run the Linux or macOS build.** Builds, not works | **Unverifiable without hardware** |
| 2 | **PlayStation support is entirely unbuilt.** Twelve-scenario matrix untested | Needs real devices |
| 3 | **Three credential sets unrotated.** Compiled-in bearer token, `JWT_SECRET`, Discord client secret | Blocks lawful distribution |
| 4 | **No Windows CI.** The shipping platform is the only unreproducible one | Embarrassing, cheap to fix |
| 5 | **`UsePreviousAppDir` defaults to yes.** Retaining the AppId restores `{app}` to the old data root | Fix is one directive |
| 6 | Music rebuild needs about 1.5 GB scratch plus bundled Python and FFmpeg | Accepted cost |
| 7 | Mod composition falls back to a 2.0 GB copy on exFAT and FAT32 | Common case on handhelds |
| 8 | MSBuild and CMake file lists can drift again | This already happened once |

---

## 6. Remaining decisions

**Blocking. Nothing ships without these.**

| # | Decision | Why it blocks |
|---|---|---|
| B1 | **Rotate the three credential sets** | The Dolphin repo must be public for GPL corresponding source once binaries ship. Rotation gates that. Ten minutes of work sitting on the critical path |
| B2 | **Approve or reject the eleven remaining Decided-row reversals** in `SYSTEM_ARCHITECTURE.md` section 1A | Row 4.2 is already resolved: Adam requires Fair Input Delay, so that reversal is rejected and it stays on the cannot-regress list. Eleven others remain |
| B3 | **What "finished on all platforms" means** given nobody can run two of the three | Determines whether Linux and macOS ship flagged or not at all |
| B4 | **RESOLVED by Adam, August 30.** The product is `dolphin-exgl`, the tree behind `DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN`. Adam on the second tree: "None of this shit works." It is abandoned experimental work | Closed. No further decision needed |

**High. Needed within days, not before the first commit.**

| # | Decision |
|---|---|
| H1 | Can you get five minutes on any Apple Silicon Mac or any Linux box? The macOS artifact is downloadable until November 28. Launching it once converts the largest unknown in the project into a fact |
| H2 | Fix the save-destroying uninstall in the live 2.1 installer, or leave it until the launcher replaces it? Asked in batch 2, never answered |
| H3 | Backend host confirmed as Railway, and who owns the account |
| H4 | Elo formula and provisional period |

**Deferred by you, recorded so they do not resurface as surprises:** marketplace governance, cross-device save sync, rollback netcode, physical disc dumping.

---

## 7. Implementation order

Dependency order. Each step is blocked by the one above it.

```
 0  Rotate credentials                    B1. Gates publication, not code
 1  Platform seams                        traits + Windows impl + stubs
 2  Library service                       copy, verify, repair
 3  Runtime packaging + supervisor        unblocks Play Now
 4  Controller service (SDL3)             THE LONG POLE
 5  Onboarding                            needs 2, 3, 4
 6  Content and update service            needs 2
 7  Mods, then music                      needs 6; music needs 2
 8  Dolphin source change (reborn_match)  needs 3
 9  Backend + online client               needs 3, 6, 8
10  Stat service                          needs 9
11  Xbox wizard                           independent, cheap
12  Linux and macOS runtimes              CI proven, running unproven
```

**Step 1 is new and it is the reason the port stays cheap.** Write the five
platform seams as traits with a Windows implementation and compiling stubs
elsewhere, from the first commit. Same speed on Windows. The port becomes days
rather than a rewrite. The seams: data root, credential vault, process spawn and
exit classification, atomic rename and file locking, removable drive
enumeration.

**Step 8 moved.** `NflsStatTracker` needs a source change, so online results now
sit behind a Dolphin rebuild rather than beside it. The earlier plan assumed no
Dolphin change was needed for capture, and that was wrong.

### What 13 days actually buys

Honestly, and this is a statement of arithmetic rather than an argument about scope:

| Achievable by September 12 | Not achievable |
|---|---|
| Windows launcher: onboarding, Play Now, controllers, settings, updates | Online: backend, client, results protocol |
| Library copy, verify, repair | Music editing embedded |
| Migration from 2.1 with saves preserved | Mods marketplace |
| Xbox USB wizard | Verified Linux and macOS |
| Linux and macOS builds, unverified | PlayStation acceptance matrix |

You have said the scope stands. That is your call and I will build to it. My job
here is to state what the schedule buys, not to argue with it.

---

## 8. Acceptance criteria

Testable conditions. Not "controllers work" but something a person can check.

### Ship gates, all platforms

| # | Criterion |
|---|---|
| A1 | Uninstall on a machine with a memory card present leaves `saves/` intact. Verified by hashing the card before and after |
| A2 | Update from 2.1 preserves saves, screenshots and custom music. Hash-compared before and after |
| A3 | No credential appears in any shipped binary. Verified by string search of the release artifact |
| A4 | Every shipped manifest verifies against the release signature, and a tampered file is refused |
| A5 | Custom music survives a Verify and Repair. Rebuild the soundtrack, run repair, confirm the file is untouched |
| A6 | The Xbox wizard leaves `GC\` byte-identical in every mode except Restore Xbox Save |
| A7 | No `nsr_track_offline.txt` or `nsr_debug.txt` in any packaged runtime, and the gate is compiled in |
| A8 | Play Now launches the game with no Dolphin window visible at any point |
| A9 | Launcher returns to the same page after the game exits |
| A10 | Every screen is reachable and exitable with a controller alone, mouse unplugged |
| A11 | Focus is visible in a grayscale screenshot taken at 10 feet |
| A12 | A forced crash produces the recovery page, never a silent restart loop |

### Windows

| # | Criterion |
|---|---|
| W1 | Clean install on a machine that has never had Reborn, to a playable game, with no manual step |
| W2 | Xbox, DualShock 4 and DualSense each detected, mapped, and showing matching prompts, over USB and Bluetooth |
| W3 | Steam Input running is detected and explained rather than producing double input |
| W4 | Windows at 150% scaling shows the Play panel without clipping |

### Linux and macOS

| # | Criterion |
|---|---|
| L1 | CI produces an artifact from clean checkout with all four Reborn objects present |
| L2 | **A human launches it once and reaches the main menu.** Currently unmet and unmeetable without hardware |
| L3 | One controller is detected and moves focus |

**L2 is the honest line between "we built it" and "it works."** Until someone
runs one of those artifacts, Linux and macOS ship flagged, and the release notes
should say so plainly rather than letting a player discover it.

---

## 9. What I need from you

1. **B1, B2, B3 above.** Those three block release, not code.
2. **H1:** five minutes on a Mac or a Linux box, if you can find one.
3. **Approval to start writing the launcher**, Windows-first with the platform seams abstracted from the first commit.

Everything else can be decided while code is being written.
