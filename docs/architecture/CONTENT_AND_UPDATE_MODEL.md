# Content and Update Model

> **Precedence.** Where this document disagrees with
> [`RECONCILIATION_RULINGS.md`](./RECONCILIATION_RULINGS.md), **the ruling wins.**
> Five auditors found 172 internal contradictions across this document set and an
> adjudicator issued one ruling per conflict. The rulings are applied to this file
> progressively; until that is finished, read the rulings first for anything
> touching safety, verdicts, schemas or identifiers.

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers:** `CLAUDE_START_HERE.md` step 6, the `CONTENT_AND_UPDATE_MODEL.md` deliverable, and section 5.4 of [`SYSTEM_ARCHITECTURE.md`](SYSTEM_ARCHITECTURE.md)
**Depends on:** [`COMPONENT_INVENTORY.md`](../discovery/COMPONENT_INVENTORY.md), [`COMPONENT_MAP.md`](../discovery/COMPONENT_MAP.md) sections 5 and 7, [`PROJECT_QUESTIONNAIRE.md`](../discovery/PROJECT_QUESTIONNAIRE.md) sections 8, 9, 11, 12, 18, 19, [`DOLPHIN_INTEGRATION.md`](DOLPHIN_INTEGRATION.md) section 6, [`INFORMATION_ARCHITECTURE.md`](../design/INFORMATION_ARCHITECTURE.md) sections 6, 7, 9, [`COMPONENT_SPECIFICATION.md`](../design/COMPONENT_SPECIFICATION.md) sections 5, 6, 10, 11, [`WIREFRAMES.md`](../design/WIREFRAMES.md) sections 5, 7, 15, [`UX_RESEARCH_DOSSIER.md`](../design/UX_RESEARCH_DOSSIER.md)

**This document resolves five recorded contradictions between already-committed files, closes one block of open research rows, and makes two new rulings.** The section 0 table has eight rows, and each row is labeled with which of those three it is, so nobody has to work out which five are the contradictions. Each resolution is also named in the section that makes it.

---

## 0. What this document changes in already-committed files

| # | Kind | Where | What it says | What this document rules, and why |
|---|---|---|---|---|
| 1 | **Contradiction** (with `SYSTEM_ARCHITECTURE.md` section 5.4) | `COMPONENT_MAP.md` section 7 | "Eight independently versioned components", including `news config` | **Seven.** News and configuration is folded into the launcher as a cached, fail-soft feed with no manifest, no staging and no rollback slot. Section 3 |
| 2 | **Contradiction** (with `SYSTEM_ARCHITECTURE.md` section 4) | `COMPONENT_MAP.md` sections 5 and 7 | `library/base/` retained base, "regenerate offline build from retained base" | **Gone.** The ship-the-image decision removed the base. `SYSTEM_ARCHITECTURE.md` section 4 already corrected this; the map has not been updated. Section 3 |
| 3 | **Contradiction** (with `SYSTEM_ARCHITECTURE.md` section 4) | `COMPONENT_MAP.md` section 5 | `music/{sources,packs,work}` | **`library/work/`**, not `music/work/`. It is derived from an offline-content version, so it belongs beside the thing it is derived from. Section 15 |
| 4 | **Contradiction** (with `PROJECT_QUESTIONNAIRE.md` itself) | `PROJECT_QUESTIONNAIRE.md` rows 18.4 and 18.5 | Differential delivery is simultaneously Decided/Required and Open/blocked | **Both, in one sentence, in one place.** Required for launcher, runtime, textures, mods and controller database. Impossible for the two disc images. Section 8, with the fetch mechanism in section 5A |
| 5 | **Contradiction** (with `WIREFRAMES.md` section 5) | `COMPONENT_SPECIFICATION.md` section 6 | Preparation tasks "Verify game source / Build offline game / ... / Verify final hashes" | **Superseded by `WIREFRAMES.md` section 5.** Those are import-and-build tasks the ship-the-image decision removed. Section 17 |
| 6 | **Closed research rows** | `PROJECT_QUESTIONNAIRE.md` rows 11.6, 12.10, 18.10 | 13 mods; `.nsmp` schema unread; updater endpoint unknown | **All three closed.** Eleven mods, schema read, endpoint measured. Sections 1, 13, 14 |
| 7 | **New ruling** | `SYSTEM_ARCHITECTURE.md` sections 5.1 and 5.5 | "Wrap" `build_iso_fields.py` and `build_iso_with_game12.py` | **Port, not wrap.** Neither runs outside the development workspace. Section 15 gives the four specific reasons |
| 8 | **New ruling** | The `NSRONLINE.iso` name, in six files (listed below) | The online build is `NSRONLINE.iso` | **The name is retired.** No file by that name exists anywhere inspected. The real artifact is `FR4N31.iso`. Section 3 |

### Ruling 8 desynchronizes six files, not three

The name `NSRONLINE` is not confined to the blueprint, questionnaire row 9.2 and `COMPONENT_MAP.md`. Retiring it silently leaves two other architecture documents out of step, so every occurrence is listed here and each one is an edit that has to be made in the file that owns it. Verified by search on August 29, 2026:

| File | Where | What it carries |
|---|---|---|
| `PROJECT_QUESTIONNAIRE.md` | Rows 9.2, 9.6, 9.8 | The name as the Decided online build name, and two Confirm rows that ask how it relates to `FR4N31.iso` |
| `COMPONENT_MAP.md` | Data-root layout, `library/online/ NSRONLINE.iso` | The on-disk filename |
| `SYSTEM_ARCHITECTURE.md` | Data-root layout, `library/online/ NSRONLINE.iso` | The on-disk filename |
| `ONLINE_AND_RESULTS_PROTOCOL.md` | Result-payload JSON comment, `"image_sha256": "<sha256 of NSRONLINE image>"` | The identity field of the results protocol |
| `ONLINE_AND_RESULTS_PROTOCOL.md` | Ranked-play dependency text and section 14 step 2, "Build NSRONLINE with the `statread1` hook" | The build prerequisite for ranked play |
| `ONLINE_AND_RESULTS_PROTOCOL.md` | Open item 2, "`NSRONLINE` does not currently carry the `statread1` hook" | The blocking defect, see section 3A |

**This document does not edit those files.** It records that the rename is a six-file change so that the rename is done completely or not at all, because a half-renamed identifier in a signed-manifest system is worse than either end state.

---

## 1. What exists today, and why none of it is an update system

Three things in the shipped product claim to handle content. None of them does.

**The update checker.** `NSR_UpdateChecker.exe` is a .NET Framework 4.7.2 program with a compiled-in `CURRENT_VERSION` constant and the literal string `Current Version: 2.1`. It fetches one URL, compares one string, and if the remote string differs it asks "Would you like to download the update now?" and opens a download page. **It downloads nothing and installs nothing.** The same binary is also the game launcher: it locates and starts `NFL Street Reborn Dolphin.exe`, and the Inno script registers it as the Start Menu and desktop shortcut ahead of Dolphin itself.

The endpoint is live and was measured on August 29, 2026:

```
GET https://nflstreetreborn.com/api/version.txt
200 OK   Content-Type: text/plain   3 bytes
2.1
```

Three bytes. No manifest, no hash, no size, no signature, no channel, no per-component breakdown. **The entire versioning surface of the product is a three-byte string, and the client that reads it cannot act on it.**

**The installer.** `rebornsetupv10.iss` overwrites the whole game image on every update (`NSR.iso` carries `ignoreversion`), re-copies the entire 2 GB texture tree, and destroys mod state. Its `CleanTexturesOnUpdate` runs `DelTree` over `{app}\User\Load\Textures\GN7E69` and over the whole `{app}\MODS` folder, which contains JSGME's `!BACKUP` restore data. Built output today is `NFL Street Reborn Setup.exe` at 7,382,037 bytes plus `NFL Street Reborn Setup-1.bin` at 1,629,604,271 bytes, both dated January 11 2026, with `DiskSpanning=yes` because the payload exceeds 2 GB.

**The patcher.** Already recorded as a fake in questionnaire row 8.5. Its core function is named `fake_patch()` and it never opens the user's file.

### The artifact has no version identity at all

The offline game image has exactly one identity:

```
NSR.iso
  bytes    1,459,978,240
  SHA-256  78a987c7780071a84a4d89fa4cfac62e717551fcdc3c2539facbefed970d345e
  SHA-1    887304df839ac39bb9430732783ee8249f31c68e
  game ID  GN7E69          internal title  NFL STREET REBORN
```

**Measured on August 29, 2026.** Every `.iso` of exactly 1,459,978,240 bytes under `C:\Users\Adam\Desktop`, `C:\Users\Adam\Downloads`, `C:\Users\Adam\nflstreet2_clean` and `C:\Users\Adam\AppData\Roaming\NFL Street Reborn` was hashed in full. Sixty-eight files match that size; **twenty-two of them are byte-identical to `78a987c7...`**.

| Where the copies are | Copies |
|---|---|
| The selected runtime, `nflstreet2_clean\DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN\Games\NSR.iso` | 1 |
| Other `DOLPHIN ONLINE TEST` runtime trees (`Dolphin-x64`, plus two under `Downloads`) | 3 |
| The Windows installer source folder, `Files\Reborn Dolphin\Games\NSR.iso` | 1 |
| `%APPDATA%\NFL Street Reborn\Games\NSR.iso` | 1 |
| `Step 1 - Patch Game File\resources\nsrpatch.iso`, in the two Xbox Dev Mode packages, the nested duplicate, and the Handheld package | 4 |
| Online prototype trees under `nfl-street-reborn-test` and `StreetLauncherProject`, in `rom\` and `dolphin\win\Games\` | 9 |
| Loose copies: `Desktop\NSR.iso`, `Downloads\NSR.iso`, `Downloads\NSR BETA\NSR.iso` | 3 |
| **Total** | **22** |

**32,119,521,280 bytes, about 29.9 GiB, holds one artifact with one hash, carrying no version marker of any kind.** Nothing on disk says which release any of them belongs to. The only thing that does is the three-byte file on the website, and it is not attached to the artifact.

**That count is a floor, not a total.** The scan covered four roots and only loose `.iso` files, so it excludes the compressed copy inside `NFL Street Reborn Setup-1.bin`, anything on other drives, and any copy stored under a different extension. Forty-six further files of exactly the same size are *not* this artifact, which is its own problem: size alone does not identify a build here, and only a hash does.

That is the problem this document solves.

---

## 2. Component verdicts

Every existing component this model touches, with exactly one verdict.

| Existing component | Verdict | Reason |
|---|---|---|
| `NSR_UpdateChecker.exe` | **RETIRE** | No source located. It cannot install anything. The launcher and supervisor replace both of its jobs |
| `https://nflstreetreborn.com/api/version.txt` | **RETIRE** | A three-byte body cannot express seven components, hashes, sizes or a signature. Replaced by a signed release manifest at a new path |
| `rebornsetupv10.iss` | **REPLACE** | All four of its user-data procedures are removed and its payload drops from 1.63 GB to the launcher only. Nothing meaningful survives except the `AppId`. Section 16 |
| Inno Setup, the tool | **REUSE** | Still the right Windows installer. `SYSTEM_ARCHITECTURE.md` section 5.4 records this pair as "wrap and reduce"; this is that, named precisely |
| `AppId {DCD6B9B9-11DE-4AC0-A7F5-256E7AC55A7E}` | **REUSE, conditionally** | Keeping the GUID is the migration hook that detects an existing 2.1 install instead of duplicating it. **It is only safe if `UsePreviousAppDir=no` is set explicitly**, because Inno's default would otherwise restore `{app}` to the old data root. Section 16 |
| JSGME, plus `!BACKUP`, `!INSTLOGS`, `MODS\JSGME.ini`, root `JSGME.ini` | **RETIRE** | Root `JSGME.ini` ships an absolute developer path. `MODS\JSGME.ini` is 0 bytes, verified. Both control folders are empty in the payload |
| The eleven mod folders | **PORT** | Migrated to `.nsmod`. Section 14 |
| `music_pack.py` and `.nsmp` format 1 | **REUSE, unchanged** | Magic `NSMP`, `format` 1, 40-stream validation. Kept at format 1 deliberately, so packs stay importable by the existing standalone tool. Section 15 |
| `music_editor.py` | **WRAP** | Engine wrapped, Tkinter UI replaced. Already the verdict in `SYSTEM_ARCHITECTURE.md` section 5.5 |
| `build_iso_fields.py`, `build_iso_with_game12.py` | **PORT** | Not wrap. Four specific blockers in section 15. One rule from `build_iso_fields.py` is adopted unchanged in meaning, restated in this document's punctuation. Section 7 quotes both forms |
| `DolphinTool.exe` (11,677,696 bytes, verified present in the runtime) | **REUSE** | `extract`, `convert`, `verify`, `header`. Note the gap: `verify` supports crc32, md5, sha1 and rchash, **not sha256** |
| `Games\NSR.iso` | **REUSE** as the offline-content artifact | With the caveat in section 4 that the January image is evidence, not the release candidate |
| `Games\FR4N31.iso` | **REUSE as the reference artifact, REPLACE as the shipped one** | It is the real online image and the name `NSRONLINE.iso` is retired, but it cannot ship as version 1 of `online-content`: it does not carry the `statread1` hook, so it produces no box score. Section 3A |
| `Games\CR4E12.iso` | **RETIRE** from launcher scope | Creation League. League functionality is out of scope per `CLAUDE_START_HERE.md` |
| `User\Load\Textures\GN7E69`, the **January installer payload tree** (2,763 DDS, 2,077,362,716 bytes) | **REUSE** as the textures component | All 2,763 DDS files hashed in full and byte-identical to the August runtime's copies. **This payload tree, not the runtime tree, is the pristine component.** The runtime tree is 2,782 files / 2,079,212,285 bytes because it also holds 19 `.png` files totaling 1,849,569 bytes that are byte-for-byte the `Xbox Controls` mod payload, that is, an applied overlay and not part of the component. See section 2A |
| `FR4N31 - PS`, `CR4E12 - PS` duplicated trees | **RETIRE** | 2.03 GB duplicated to deliver 2,057,035 bytes of difference across 19 files. Section 13 |
| `TO PICK YOUR CONTROLS REMOVE PS AND XBOX FROM THE FILENAME.txt` | **RETIRE** | A manual folder rename is not a chooser |
| `User\GameSettings\GN7E69.ini` | **PORT** into generated configuration | Has already drifted three ways with no version identity. Section 16 |
| Dolphin `[AutoUpdate]` `UpdateTrack` | **REUSE the disabled state, enforce it by generation** | Empty in both configs and no `Updater.exe` exists, but the fork defaults the key to `Common::GetScmUpdateTrackStr()`, so a generated config that omits the key re-enables self-update |
| `User\nsr_track_offline.txt`, `User\nsr_debug.txt` | **RETIRE from every package** | Already ruled in `DOLPHIN_INTEGRATION.md` section 6. Repeated here because packaging is where it is enforced |
| The "news config" component | **RETIRE as an updateable component** | Folded into the launcher. Section 3 |
| Controller database | **New, PROPOSED** | No artifact exists anywhere. Section 3 states what it is and what it is not |

### 2A. Which texture tree is the component, and why the question exists

Two trees carry the name `User\Load\Textures\GN7E69` and they are not the same tree. Measured August 29, 2026:

| Tree | Files | Bytes |
|---|---|---|
| January Windows installer payload, `Files\Reborn Dolphin\User\Load\Textures\GN7E69` | 2,763, all `.dds` | 2,077,362,716 |
| Selected runtime, `CREATION LEAGUE AND NEW DOLPHIN\User\Load\Textures\GN7E69` | 2,782: the same 2,763 `.dds` plus 19 `.png` | 2,079,212,285 |

**The 2,763 DDS files are byte-identical across the two trees. Every one of them was hashed, not sampled.** The 19 extra PNG files in the runtime tree total 1,849,569 bytes and are byte-for-byte identical to the payload of the `Xbox Controls` mod in `MODS\Xbox Controls`, and all 19 of their filenames also appear in the Xbox package's PNG texture set.

**Ruling: the component is the 2,763-file DDS tree at 2,077,362,716 bytes.** The 19 PNG files are an *applied mod*, which is exactly the state section 13 exists to make impossible: a mod written into the base tree, with no record that it is there. Under the composition model they live in `mods/packages/reborn.prompts.xbox/` and never in the pristine tree. **Every size given for `textures` in this document is the 2,763-file figure**, and a repair sweep run against today's runtime tree would correctly report 19 extra files, which is the "extra file in a managed tree" row in section 12's classification table.

---

## 3. The component list, settled at seven

`SYSTEM_ARCHITECTURE.md` section 5.4 says seven. `COMPONENT_MAP.md` section 7 and questionnaire row 18.1 say eight, the extra one being "news config". **Seven is authoritative, and the eighth is deliberately not a content component.**

News and a remote configuration blob have no artifact, no schema, and no failure mode worth a rollback slot. They are cached, they fail soft, and a stale news rail is not a defect. Giving a JSON feed its own manifest, staging directory, health check and retained prior version is machinery in search of a problem. It is a launcher feature that reads a cached URL, and if the fetch fails the Home carousel renders from cache with the last-updated label the design already requires.

**This reverses a row the questionnaire marks Decided (18.1), so it needs Adam's sign-off rather than being assumed.**

### The seven

| # | Component id | What it is | Measured size | Prior version worth retaining |
|---|---|---|---|---|
| 1 | `launcher` | Tauri shell, supervisor, bundled web assets, bundled Python and FFmpeg | **Unverified.** Nothing is built yet | Yes, cheap |
| 2 | `runtime` | Platform Reborn Dolphin tree | 108,045,862 bytes across 2,512 files | Yes, cheap |
| 3 | `offline-content` | The customizable game image | 1,459,978,240 bytes as ISO, 718,375,792 as RVZ | **Expensive. See section 11** |
| 4 | `online-content` | The standardized game image, `FR4N31` | 1,459,978,240 bytes as ISO, 718,354,776 as RVZ (measured August 29, 2026) | **Expensive. See section 11** |
| 5 | `textures` | Base texture pack per game ID | 2,077,362,716 bytes across 2,763 DDS files (the payload tree, per section 2A) | Yes, and cheaply: only changed files |
| 6 | `mods` | Curated `.nsmod` and `.nsmp` packages | 11 packages, 57 files, **20,534,456 bytes** (19.58 MiB), all `.nsmod`. **Zero `.nsmp` packages exist**, verified by search | Yes, per package |
| 7 | `controller-db` | SDL mapping table plus Reborn device metadata | **No artifact exists.** See below | Yes, trivially |

Total measured installed footprint of components 2 through 5, as ISO: **5,105,365,058 bytes, about 5.1 GB.** Component 1 is unbuilt and component 7 is undefined, so the real number is higher and cannot be stated yet.

### What the online component actually is

Questionnaire row 9.2 says `NSRONLINE.iso` "unless the technical design chooses another container". The technical design is choosing, and it is also choosing a different name, because **no file called `NSRONLINE.iso` exists in any inspected location.**

```
FR4N31.iso
  bytes    1,459,978,240
  SHA-256  d52a9370bad67935ed0d2855d232919358c32e33a8330b75a217c04c242c42c3
  game ID  FR4N31          internal title  FRANCHISE LEAGUE
```

The on-disk name inside `library/online/` should carry the game ID, because the game ID drives the texture folder name and the `GameSettings` file name. Naming it `NSRONLINE.iso` hides the one identifier that everything else keys on.

Worth recording, because it is a trap for anyone reading the prototype: **the existing online prototype does not use the online image.** Its `rom/` folder holds a byte-identical copy of the offline `NSR.iso`, SHA-1 `887304df...`. That accounts for nine of the twenty-two duplicate copies counted in section 1.

### 3A. The artifact on disk cannot be version 1 of `online-content`

**This is a hard dependency that the rest of this document must not paper over.** `ONLINE_AND_RESULTS_PROTOCOL.md` open item 2 records, verified by probe against `FR4N31.iso`: the online build **does not carry the `statread1` hook**. Without it `players[]` is empty, the ingest contract rejects the capture outright, and `sides_verified` can never become true. That document's section 14 step 2 is therefore "Build NSRONLINE with the statread1 hook, without it there is no box score at all".

The consequence for this document, stated plainly:

- **Every size and hash given here for `online-content` describes today's `FR4N31.iso`, which is evidence and not the release candidate.** `d52a9370...` will not be the hash of the shipped artifact.
- The measured RVZ figure of 718,354,776 bytes remains a sound *size estimate* for the rebuilt image, because adding a DOL hook does not meaningfully change disc content or compressibility. It is an estimate, not the shipped number, and it is labeled that way everywhere it is used.
- `Games\FR4N31.iso` is therefore **REUSE as the reference and measurement artifact, REPLACE as the shipped one**, which is the verdict recorded in section 2.
- Section 4's "which build does the first manifest describe" gap applies to **both** content components, not only to `offline-content`. For `online-content` it is additionally constrained: the build must be one that carries `statread1`, and no such build exists yet.

**Open item.** Nobody has been asked whether the online image is rebuilt from the same source tree as the offline image or forked from it. That decides whether `offline-content` and `online-content` share a release version number, which the example in section 4 currently assumes they do.

### What the controller database is, and what it is not

Named as an updateable component in three documents and specified in none. Here is the honest version.

**Verified:** the fork bundles SDL 2.25.0 with its mapping table compiled in as `Externals/SDL/SDL/src/joystick/SDL_gamecontrollerdb.h`. No `gamecontrollerdb.txt` exists in any inspected tree. No Dolphin source calls `SDL_GameControllerAddMappingsFromFile`. The bundled SDL does define `SDL_HINT_GAMECONTROLLERCONFIG_FILE`.

**Proposed contents:** an SDL-format mapping table, plus a Reborn device metadata file keyed on physical device identity carrying display name, device family for prompt-pack selection, default dead zones, and rumble capability.

**Consumer, v1: the launcher's own SDL3 controller service, and nothing else.** SDL3 loads a mapping file directly, with no doubt attached. Dolphin keeps its compiled-in table, because the launcher generates `GCPadNew.ini` for Dolphin anyway (`DOLPHIN_INTEGRATION.md` section 6), so Dolphin never needs to identify a device by mapping.

**Explicitly not proposed for v1:** delivering the mapping file to Dolphin through `SDL_HINT_GAMECONTROLLERCONFIG_FILE`. That path exists in the bundled SDL and has not been tested here. It is a follow-up, not a launch dependency, and the v1 scoping above removes the need for it.

---

## 4. Versioning

There is no versioning scheme today. The only version string in the entire system is the product-level `2.1`, and it appears in three unconnected places: the Inno script, the update checker's compiled-in constant, and the live `version.txt`.

### Two kinds of version, and they are not the same kind of thing

**Component versions are semver.** `MAJOR.MINOR.PATCH`, one per component, incremented by whoever changes that component.

**A release is a label, not a semver.** A release is a *set* of component versions. It has no API, so semantic versioning of it means nothing. The release label is what the user sees: "Ranked needs version 2.4" in the design copy is a release label, not any component's semver.

```
release 2.4
  ├── launcher        1.4.0
  ├── runtime         1.1.2
  ├── offline-content 2.4.0
  ├── online-content  2.4.0
  ├── textures        1.0.1
  ├── mods            (per package, not release-pinned)
  └── controller-db   1.2.0
```

**The release manifest is the compatibility matrix.** There is no mixing and matching. The project supports exactly the combinations it publishes, which removes the need for a separate compatibility table and removes an entire class of "works on my install" bugs. Each component manifest still declares `requires` ranges, but those are an integrity check on the publisher, not a user-facing feature.

The one exception is `online-content`, and it has two possible sources for the same number, so the precedence has to be written down rather than assumed.

### `online_minimum` has one authority and one mirror

| Source | Role | Authoritative? |
|---|---|---|
| The backend, declared at connect time | The forced-online gate | **Yes.** The server is the only thing that knows what its opponents are running |
| `online_minimum` in the signed release manifest (section 5) | A mirror of the same value, recorded at publish time | **No.** It exists so the launcher can explain the gate *before* it has reached the backend, and so an offline launcher can say why ranked is unavailable |

**Rules, so that the case the blocking screen exists to handle is actually specified:**

1. **The backend value wins whenever the client has one.** The manifest value is used only when the backend has not been reached in this session.
2. **On disagreement, the higher of the two gates.** Fail closed: refusing a match that would have worked is recoverable, joining a match with a mismatched image is not.
3. **A disagreement is a publisher error, not a user error.** Both values are supposed to be generated from the same release. The mismatch goes to diagnostics with both values and the release label, and nothing user-facing mentions it beyond the normal gate copy.
4. **If the backend names a minimum the client's cached manifest does not contain**, the client re-fetches the release manifest once. If the value is still unknown, the blocking screen shows the required version with **no size and no progress bar**, because the size comes from a manifest the client does not have. That is a distinct screen state, and section 17 lists it.
5. `online_minimum` is a **release label** in both sources, not a component semver. Comparison is over the published release ordering, not over semver ranges.

**This is a PROPOSED ruling.** It resolves an ambiguity this document itself created by putting the field in two places, and it has not been reviewed against a backend that does not exist yet.

### Which build the first manifest describes

**Unknown, and this is a real gap.** Every hash and size in this document describes the January 11 2026 `NSR.iso`.

**Measured August 29, 2026** against the current development build (`nflstreet2_clean\GameCube Version\root`, files dated August 29 2026, and the ISO built from it at `%APPDATA%\NFL Street Reborn\Games\NFL Street 2 FIELDS 12-19 [2026-08-29].iso`), file by file through the FST with full SHA-256:

```
  30 files  byte-identical    826,724,672 bytes
  33 files  changed           513,767,580 bytes as shipped, 591,905,861 at the new sizes
   8 files  added             game12.qkl through game19.qkl, 35,913,185 bytes
```

The eight added containers are the eight imported fields recorded in `tools/field_importer/registry.json`: 12 beach, 13 ears, 14 junkyard, 15 legeday, 16 rooftop, 17 warehouse, 18 bronx, 19 latitudes. `root/` holds `game12.qkl` through `game19.qkl`.

**A warning for anyone re-checking these numbers.** An earlier draft of this document reported 41 identical / 22 changed / 3 added. That comparison is real, reproduces to the byte, and is against the wrong image: it matches `%APPDATA%\NFL Street Reborn\Games\TOTALLY NOT MALWARE.iso`, a 66-entry August 5 2026 snapshot, not the current development build. Any comparison in this document has to name the exact image on both sides, because there are 68 files of identical size in these trees and 22 distinct builds among the ones checked.

**The January image is evidence, not the release candidate, and the gap between them is still growing.** The first manifest cannot be written until Adam names the build, and per section 3A that naming has to cover the online build separately.

---

## 5. The signed manifest

Two levels. The release manifest is signed; component manifests are hash-pinned by it. That means one signature per release rather than seven, and it means a component manifest cannot be swapped without breaking the release signature.

```
   signed release manifest                (one signature, small)
        │  pins by sha256
        ▼
   component manifests   x7               (not individually signed)
        │  pins by sha256
        ▼
   artifacts and files                    (verified before any swap)
```

### Release manifest

```json
{
  "schema": "nsr.release.v1",
  "channel": "stable",
  "release": "2.4",
  "generated_at": "2026-09-12T00:00:00Z",
  "previous_release": "2.3",
  "online_minimum": "2.4",
  "components": {
    "launcher": {
      "version": "1.4.0",
      "manifest": "components/launcher-1.4.0.json",
      "sha256": "<64 hex>",
      "bytes": 0,
      "platforms": ["win-x64", "linux-x64", "mac-arm64", "mac-x64"]
    },
    "runtime":         { "version": "1.1.2", "manifest": "...", "sha256": "...", "bytes": 108045862, "platforms": ["win-x64","linux-x64","mac-arm64","mac-x64"] },
    "offline-content": { "version": "2.4.0", "manifest": "...", "sha256": "...", "bytes": 0, "platforms": ["any"] },
    "online-content":  { "version": "2.4.0", "manifest": "...", "sha256": "...", "bytes": 0, "platforms": ["any"] },
    "textures":        { "version": "1.0.1", "manifest": "...", "sha256": "...", "bytes": 2077362716, "platforms": ["win-x64","linux-x64","mac-arm64","mac-x64"] },
    "controller-db":   { "version": "1.2.0", "manifest": "...", "sha256": "...", "bytes": 0, "platforms": ["any"] }
  },
  "catalog": { "manifest": "catalog/packages.json", "sha256": "<64 hex>" }
}
```

`mods` is not a release component in the same sense as the others. It is a catalog of independently versioned packages, pinned by one hash so the catalog itself cannot be tampered with.

### Component manifest

```json
{
  "schema": "nsr.component.v1",
  "component": "textures",
  "version": "1.0.1",
  "platform": "win-x64",
  "requires": { "launcher": ">=1.4.0", "runtime": ">=1.1.0" },
  "install_root": "library/textures/GN7E69",
  "delivery": "per-file",
  "blob_store": { "base": "blobs/by-hash/", "layout": "<sha256[0:2]>/<sha256>", "encoding": "zstd" },
  "artifacts": [
    { "url": "bundles/textures/1.0.1/all.zip", "sha256": "<64 hex>", "bytes": 0,
      "container": "zip", "covers": "all" },
    { "url": "bundles/textures/1.0.0-to-1.0.1.zip", "sha256": "<64 hex>", "bytes": 0,
      "container": "zip", "covers": { "from": "1.0.0" } }
  ],
  "files": [
    { "path": "tex1_128x128_m_ffc5151a141db330_14.dds",
      "sha256": "<64 hex>", "bytes": 1398276, "enc_bytes": 0 }
  ],
  "health_check": "file-hash-full",
  "notes": ""
}
```

**`bytes` is the byte length of the file, never the size it occupies on disk.** The example above is the shared football texture, whose real length is **1,398,276 bytes**. Windows Explorer and `dir` report 1,400,832 for it, which is 342 x 4,096, the NTFS allocated size. A manifest that records 1,400,832 fails its own tier 0 presence-and-size check on a pristine install, on every platform, forever. **Generators must read the file length, and the packaging step must re-stat every listed file and fail the build on any row where `bytes` does not equal the length on disk.** Section 14 records where this document itself got it wrong, so the trap is documented rather than just avoided.

For a disc image, `delivery` is `whole` and `files` has one row:

```json
{
  "schema": "nsr.component.v1",
  "component": "offline-content",
  "version": "2.4.0",
  "platform": "any",
  "install_root": "library/offline",
  "delivery": "whole",
  "artifacts": [
    { "url": "blobs/offline/2.4.0/NSR.rvz", "sha256": "<64 hex>", "bytes": 718375792,
      "container": "rvz", "produces": "NSR.iso" }
  ],
  "files": [
    { "path": "NSR.iso", "sha256": "78a987c7...", "bytes": 1459978240,
      "disc_sha1": "887304df839ac39bb9430732783ee8249f31c68e",
      "game_id": "GN7E69", "internal_title": "NFL STREET REBORN" }
  ],
  "health_check": "disc-verify",
  "customizable": true
}
```

### 5A. How a client fetches one changed file

Sections 8 and 12 require per-file differential delivery for five components and "re-fetch only what is wrong". That is a mechanism, so here is the mechanism. **All of it is PROPOSED. None of it is implemented, and no origin exists to serve it (open item 2).**

**The blob store is content-addressed.** One immutable object per distinct file content, addressed by its own SHA-256:

```
  blobs/by-hash/<first 2 hex of sha256>/<full 64 hex sha256>
```

Content addressing is chosen for one reason that matters here: **the texture component changes by a handful of files at a time out of 2,763, and 2,252 of `FR4N31`'s files are byte-identical to `GN7E69` files (section 13).** Under a content-addressed store those are one object, uploaded once, fetched once, and shared by every component and every version that references them. Under a version-keyed store (`blobs/textures/1.0.1/<path>`) they are re-uploaded per version and per game ID.

**A `files[]` row is a complete fetch instruction.** Its `sha256` is both the integrity check and the address, so no per-file `url` field is needed and none is added: the client composes `blob_store.base` + `layout` + `sha256`. `enc_bytes` is the transfer size after `blob_store.encoding`; `bytes` is always the decoded length, which is what tier 0 and tier 1 check.

**`covers` grammar, which was previously undefined.** An `artifacts[]` entry is an optional bulk bundle. Exactly three forms are legal in v1:

| `covers` value | Meaning |
|---|---|
| `"all"` | The bundle contains every file in `files[]`. Used for a first install |
| `{ "from": "<version>" }` | The bundle contains exactly the files that differ from that version of the same component. Used for the common upgrade |
| `[ "<path>", ... ]` | The bundle contains exactly those paths. Used for platform-specific or feature-specific slices such as the ReShade set |

Any other value is a manifest error and the update is refused. A component with no `artifacts[]` at all is legal and means per-file fetches only.

**The client's decision, which is arithmetic and not a heuristic.** It already holds the old and new component manifests, so it computes the changed set locally, with no server round trip and no server logic:

```
  changed  = files where (path missing locally) or (sha256 differs)
  need     = sum of enc_bytes over changed
  full     = sum of enc_bytes over all files
  bundle   = the artifact whose covers matches, if one exists

  if a matching { "from": <current version> } bundle exists and its bytes <= need
        fetch that one bundle
  else if need >= BULK_THRESHOLD * full
        fetch the covers:"all" artifact
  else
        fetch each changed blob individually
```

**`BULK_THRESHOLD` is not specified here and must not be guessed.** It trades request count against wasted bytes and it depends entirely on the origin's per-request cost, which does not exist yet. It is an open item.

**Transport is plain static HTTP.** `GET` per blob, `Range` for resume, `If-None-Match` on manifests, a bounded parallelism cap so a Steam Deck on hotel wifi does not open 200 sockets. **No server-side logic is required**, which is deliberate: it means blobs can sit on object storage or a CDN and the SiteGround site stays out of the content path entirely.

**No binary delta format is adopted in v1, and that is a decision with a reason, not an omission.** bsdiff, zsync and courgette all reconstruct a new file from an old one plus a patch. The units here do not justify it: the largest per-file unit in the whole differential set is a 1.4 MB texture, and the runtime's largest files are executables that are replaced wholesale anyway. A delta engine would add a dependency, a second failure mode (a patch that applies cleanly to the wrong base), and another artifact class to sign, to save a fraction of a few megabytes. **The two components where a delta would actually pay, the disc images, cannot use one at all for the three reasons in section 8.**

**What this does not solve.** It is per-file, so a component whose files all change (a recompiled runtime) costs the full component either way. That is correct behavior, not a gap.

### 5B. The mods catalog manifest

`catalog/packages.json` is pinned by hash in the release manifest above and, until now, was the only manifest in this document with no schema. Here it is.

```json
{
  "schema": "nsr.catalog.v1",
  "catalog_version": 7,
  "generated_at": "2026-09-12T00:00:00Z",
  "categories": [
    { "id": "music", "name": "Music Packs" },
    { "id": "balls", "name": "Balls" },
    { "id": "prompts", "name": "Controller Prompts" }
  ],
  "packages": [
    {
      "id": "reborn.ball.purple",
      "kind": "nsmod",
      "name": "Purple Football",
      "category": "balls",
      "latest": "1.0.0",
      "versions": [
        {
          "version": "1.0.0",
          "url": "packages/reborn.ball.purple/1.0.0/purple-football.nsmod",
          "sha256": "<64 hex of the .nsmod container>",
          "bytes": 0,
          "published_at": "2026-09-12T00:00:00Z",
          "requires": { "launcher": ">=1.0.0" },
          "targets": ["GN7E69", "FR4N31", "CR4E12"],
          "conflict_group": "ball",
          "online_safe": true,
          "art": "packages/reborn.ball.purple/1.0.0/cover.png",
          "art_sha256": "<64 hex>",
          "yanked": false
        }
      ]
    }
  ]
}
```

| Field | Rule |
|---|---|
| `schema` | Versioned like every other manifest here |
| `catalog_version` | A **monotonically increasing integer**, not a semver. The catalog has no API surface, so semver would mean nothing. A client that holds a higher `catalog_version` than the one it just fetched treats the fetch as stale and keeps what it has |
| `kind` | `"nsmod"` or `"nsmp"`. See below |
| `latest` | The version the store offers. Older versions stay listed so an installed package can still be identified and rolled back |
| `url`, `sha256`, `bytes` | The per-package download address and its pin. **These are the fields whose absence made the catalog unimplementable**, and they are pins on the container, which is what section 13 decision 3 means by "hash-pinned by the signed catalog" |
| `yanked` | A version that must not be newly installed. Existing installs are not touched, they are flagged in the UI |
| Duplicated per-version metadata | `targets`, `conflict_group`, `online_safe` and `requires` also live inside the package's own `package.json`. The catalog copy is what the store browses and filters on **before downloading anything**; the in-package copy is authoritative at install time, and a mismatch is a publish-time error |

**Catalog versioning rule, and its cost, stated rather than hidden.** The catalog is pinned by SHA-256 inside the signed release manifest, so **it is immutable within a release**: publishing one new mod requires generating and signing a new release manifest. That is safe and simple and it makes mod publication as heavy as a product release.

The alternative is a separately signed catalog with its own Ed25519 signature by the same key, with the release manifest pinning `catalog_min_version` instead of a hash. That decouples mod publishing from releases at the cost of a second signing operation and a second thing that can be replayed downgraded. **v1 keeps the hash pin. Whether to decouple is an open item for Adam**, and it is a real question because the answer decides how often the release manifest has to be re-signed by whoever ends up holding the key that section 6 says nobody holds yet.

**`.nsmp` music packs are cataloged, and there are none to catalog.** Verified by search across `Desktop`, `Downloads`, `nflstreet2_clean` and `%APPDATA%\NFL Street Reborn`: **zero `.nsmp` files exist anywhere**, so questionnaire row 11.1's Music Packs category has an empty catalog on day one and there is nothing to migrate. The rules for when one exists:

- `kind: "nsmp"` uses the same package record, the same hash pin and the same versioning.
- It installs into `music/packs/`, not into `mods/packages/`, and it is **never composed into a texture tree**, so `targets` and `conflict_group` are omitted and `online_safe` is meaningless and omitted. Music is offline only, per questionnaire row 9.3.
- Its container format is the existing `.nsmp` format 1 (magic `NSMP`, 40-stream validation), REUSED unchanged per section 2, so packs stay importable by the existing standalone `music_pack.py`.
- Installing one does not modify the game image. It puts a pack on the shelf; applying it is a music edit, which is the section 15 rebuild path.
- **Two versioning surfaces exist and must not be confused**: the `.nsmp` *format* version (1, and stable) and the *package* version in the catalog. The catalog carries the format version so an older launcher can refuse a format 2 pack cleanly.

### What the manifest must contain, stated as requirements

| Field | Why it is mandatory |
|---|---|
| `schema` | Version the format before shipping it, not after. The `.nsmp` format already gets this right |
| `channel` | Fixed at `stable`. Questionnaire row 18.7: no beta or developer channels. The field exists so that adding one later is not a format break |
| `release`, `previous_release` | Rollback needs a name for where to go back to, and the client must not have to guess it |
| `online_minimum` | The publish-time **mirror** of the forced-online gate. The backend's value is authoritative; this one is what the launcher shows before it has reached the backend. Precedence rules in section 4, screen states in section 17 |
| Per component: `version`, `manifest`, `sha256`, `bytes` | `bytes` is what the UI shows before the user commits. "Update ready. 210 MB." comes from here, not from a guess |
| `platforms` | Windows-only material is real and measured: ReShade content totals **42,282,003 bytes** (`reshade-shaders` 40,793,846 across 172 files, `reshade-presets` 622,877 across 81 files, `NSRShaderlessMenu.addon64` 205,312, `ReshadeEffectShaderToggler.addon64` 659,968), and `dxgi.dll` adds 5,487,616, for **47,769,619 bytes together**. Linux and macOS must not download any of it |
| Per file: `path`, `sha256`, `bytes` | The unit of repair. Section 12. `bytes` is the file length, never the allocated size, per the note above |
| Per file: `enc_bytes`, and per component `blob_store` | The unit of *fetch*. Without them a per-file component has no address to fetch one file from. Section 5A |
| `artifacts[].covers` | Says what a bulk bundle contains. Only the three forms in section 5A are legal |
| `catalog` | Pins `catalog/packages.json` by hash. Schema in section 5B |
| For images: `disc_sha1`, `game_id`, `internal_title` | `disc_sha1` survives container conversion, so it is the only identity that is stable across ISO and RVZ. Section 9 |
| `customizable` | Marks the one component a user is allowed to modify locally. Without it, custom music reads as damage. Section 12 |
| `install_root`, `delivery`, `health_check` | The client must never infer where a component goes or how to check it |

### Size of one manifest set

A per-file manifest for one release covers roughly **5,334 entries**: 2,512 runtime files, 2,763 texture files, 57 migrated mod files, and the two disc images. At a path, a 64-character hex digest and two sizes per row, that is on the order of **650 to 800 KB uncompressed**, and it compresses well. The estimate is arithmetic from measured file counts and typical Dolphin texture path lengths; it is **not** a measurement of a generated manifest, because none has been generated. Fetching it on every launch is not free but it is not a problem either. The client caches it by `release` label and only re-fetches when the release label changes, so the routine check is a small conditional request, not a 700 KB download.

---

## 6. Signing and key custody

**This is specified as a requirement and blocked as an implementation.**

What is decided: manifests are signed, hash verification is required, and marketplace packages are hash-pinned by a signed published manifest (questionnaire rows 18.3 and 11.3, blueprint section 19).

What is proposed here:

| Choice | Proposal | Why |
|---|---|---|
| Algorithm | **Ed25519 detached signature** over the canonical serialization of the release manifest | Needs no certificate authority. The project has no code signing at all (questionnaire row 23.1, Adam: "we broke"), so anything that depends on a CA is not available |
| Canonicalization | Sign the exact bytes served, not a re-serialized object | Re-serializing JSON is how signature checks silently start passing on modified content |
| Trust anchor | Public key compiled into the launcher binary | The launcher is the only thing that must be obtained out of band anyway |
| Key rotation | A second, offline-held public key also compiled in, unused until needed | With no CA there is no revocation. A pre-published backup key is the only rotation path that does not require every user to reinstall |

**What cannot be specified here:** who holds the private key, where it lives, how it is protected, how it rotates, and what the client does on the very first run before any key is pinned. Questionnaire row 18.8 is Open and there is no signing infrastructure anywhere in the project. **This is a hard blocker for shipping automatic updates, and it is not a documentation problem.**

One consequence that must be stated rather than discovered: an unsigned or signature-failing manifest is treated as an outage, not as an update. The launcher keeps running on the content it has, shows nothing blocking, and reports the failure only in diagnostics. A failed signature must never be able to stop a user from playing offline.

---

## 7. Hash verification

Three tiers, because a full sweep of 5,334 files including a 1.46 GB image is not something to do on every launch.

| Tier | What it does | Cost | When |
|---|---|---|---|
| **0. Presence** | Every manifest path exists, with the manifest's byte size | Milliseconds | Every launch |
| **1. Content** | SHA-256 of every manifest-listed file | Minutes, dominated by the images | First run, repair, after any swap, before ranked |
| **2. Disc** | `DolphinTool verify` SHA-1 against `disc_sha1` | Seconds to a minute | Cross-check on images only, and after a container conversion |

**Tier 2 exists because of a specific tool gap.** `DolphinTool verify` supports crc32, md5, sha1 and rchash, and **not sha256**. So the launcher computes SHA-256 itself over the file bytes, and uses `DolphinTool` only for the disc-level SHA-1. That is not redundant work: a file-level SHA-256 identifies the *file*, and a disc-level SHA-1 identifies the *disc*, which is the same value whether the container is ISO or RVZ. The measured RVZ of `NSR.iso` verifies to the identical disc SHA-1 `887304df839ac39bb9430732783ee8249f31c68e` while having a completely different file hash. Only the disc hash survives conversion.

**The rule adopted from `build_iso_fields.py`.** The source text reads:

> An unchanged ISO is a FAILED BUILD, never a silent fallback -- divergence WARNS and ABORTS.

Restated in this document's house style, which forbids dashes of that kind, the sense is unchanged and the punctuation is not:

> An unchanged ISO is a FAILED BUILD, never a silent fallback: divergence WARNS and ABORTS.

**That is a restatement, not a quotation.** Anywhere this document says the rule is carried forward "verbatim", it means the rule, not the characters.

Applied to updates, that reads: **a component whose post-swap state is byte-identical to its pre-swap state, when the manifest says the version changed, is a failed update, not a no-op.** It reports and it rolls back. The same tool also collects every gate failure and reports them together before writing anything, including under a dry run, and the update service does the same: verification failures across a staged set are reported as one list, not one at a time.

---

## 8. Delivery: what can and cannot be differential

This is the section that resolves questionnaire rows 18.4 and 18.5, which currently record differential delivery as simultaneously Required and blocked.

**Differential delivery is required for five components and impossible for two. Both halves are true at once, and the split is not negotiable, because it follows directly from the locked decision to ship finished images.**

| Component | Differential | Unit of update | Cost of one update |
|---|---|---|---|
| `launcher` | **Yes** | Per file inside the bundle | Small. Unmeasured, nothing is built |
| `runtime` | **Yes** | Per file, 2,512 of them | Proportional to the change. Full tree is 108,045,862 bytes |
| `offline-content` | **No** | The whole artifact | 1,459,978,240 bytes as ISO, 718,375,792 as RVZ |
| `online-content` | **No** | The whole artifact | 1,459,978,240 bytes as ISO, 718,354,776 as RVZ (measured on today's `FR4N31.iso`; an estimate for the shippable build, per section 3A) |
| `textures` | **Yes** | Per file, 2,763 of them | Proportional. Full tree is 2,077,362,716 bytes |
| `mods` | **Yes** | Per package | Largest package today is `Patriots Throwback` at **3,670,932 bytes** across 5 files |
| `controller-db` | **Yes** | The whole file, which is tiny | Trivial |

### Why the images cannot be differential, stated once and not re-argued

Three independent reasons, each sufficient on its own:

1. **There is no base to diff against.** The ship-the-image decision (questionnaire row 8.6) removed the retained base. A differential needs two known endpoints and the client only has one.
2. **There is no patch engine.** `rebornpatcherv6.py` is a fake and nothing else in the project applies a patch to a disc image.
3. **Nothing in the project can write a file back into a disc image in place.** The FST exposes 63 addressable files, so per-file repair inside the image is conceivable, but no tool was found that does it, and the image is effectively full: total inter-file padding is **52,772 bytes** and the last file ends exactly at the image end.

### What the foreclosed opportunity was actually worth

Recorded honestly, because it is large, and because pretending otherwise would make the size cost in section 18 look inevitable rather than chosen.

Comparing the shipped January image against the **current** development build, file by file through the FST with full SHA-256, measured August 29, 2026. Both sides named exactly, because section 4 explains why that matters:

```
  A  Desktop\Reborn Installer\Files\Reborn Dolphin\Games\NSR.iso   78a987c7...   63 files, 1,340,492,252 bytes of payload
  B  %APPDATA%\NFL Street Reborn\Games\
       NFL Street 2 FIELDS 12-19 [2026-08-29].iso                                71 files, 1,454,543,718 bytes of payload

  30 files  byte-identical    826,724,672 bytes
  33 files  changed           513,767,580 bytes in A, 591,905,861 bytes in B
   8 files  added             game12.qkl .. game19.qkl,  35,913,185 bytes
   0 files  removed
```

`chatdata.dat` alone is **704,770,048 bytes and is still byte-identical**: 52.6 percent of A's payload, 48.5 percent of B's. A file-level differential inside the image would have been worth **826,724,672 of B's 1,454,543,718 payload bytes, about 57 percent of the download.**

**The saving is shrinking as the build diverges.** Against a mid-August snapshot the same comparison gave 41 identical at 885,400,191 bytes, about 66 percent. Three weeks of field imports moved it to 57 percent. The number is real, it is large, and it is not a constant, so it must be re-measured against whichever build is actually named as the release candidate rather than quoted from here.

**That opportunity is closed by a locked decision, not by a technical limit.** Section 9 recovers a comparable amount a different way, without a patch engine.

---

## 9. Container: ISO or RVZ

The largest saving available to this project, and it needs one decision from Adam.

**Measured.** `DolphinTool convert` on `NSR.iso`, zstd level 5, 128 KiB blocks:

```
  ISO   1,459,978,240 bytes
  RVZ     718,375,792 bytes      49.2 percent
  verify  disc SHA-1 887304df839ac39bb9430732783ee8249f31c68e, identical
```

Scrubbing produced no further gain, and Dolphin's own tool warns that it does not.

**Every download and every retained rollback copy halves.** No patch engine, no base retention, no new tooling: `DolphinTool.exe` already ships inside the selected runtime.

**Both images are now measured, so no figure below is derived from an unmeasured one.** Same command, same settings, run August 29, 2026:

```
  NSR.iso     ISO 1,459,978,240   RVZ 718,375,792   49.2 percent
  FR4N31.iso  ISO 1,459,978,240   RVZ 718,354,776   49.2 percent
```

The `FR4N31` figure is a measurement of today's artifact, which per section 3A is not the one that will ship. It is an estimate for the rebuilt image, and a good one, because a DOL hook does not change disc content or compressibility meaningfully.

| Option | Download per content update | Retained prior copy of both images | Cost |
|---|---|---|---|
| ISO everywhere | 1,459,978,240 bytes each | 2,919,956,480 bytes | Simplest, most expensive |
| **RVZ delivered, ISO materialized locally for offline only** | 718,375,792 offline, 718,354,776 online | 1,459,978,240 offline plus 718,354,776 online, 2,178,333,016 bytes, if retained | Recommended. Music needs a real ISO to repack |
| RVZ everywhere | 718,375,792 offline, 718,354,776 online | **1,436,730,568 bytes**, measured | Cheapest, and blocked until the music chain can work from RVZ |

**Recommendation: deliver both images as RVZ. Materialize the offline image to ISO at install time because the music chain writes an ISO. Keep the online image as RVZ permanently, because it is never customized and never repacked.**

Three unknowns block making this final, and none of them is expensive to close:

1. **Whether the launcher's direct-boot path loads an RVZ from `library/`.** Dolphin's own tool produces RVZ for Dolphin, so this is very likely fine, but direct boot by path with no game list has not been tested with RVZ here. One boot test settles it.
2. **Whether the Xbox target accepts RVZ.** Its `Config\Dolphin.ini` sets `ISOPath0 = E:\Games` and `GFXBackend = D3D12`, which says nothing about container support. Untested.
3. **Whether the music chain can operate from RVZ without an intermediate ISO.** Today it cannot: extraction works from either container, but the repack writes an ISO. That is why the recommendation materializes one.

---

## 10. Staging, swap, and the crash-safe state machine

The current updater deletes before it installs. That is the root of two of its three verified failure modes. **Nothing in this model deletes anything before a verified replacement exists on disk.**

### Layout

```
DATA ROOT
  downloads/<component>/<version>/     .part files, resumable, unverified
  staging/<component>/<version>/       fully downloaded, hash-verified, inert
  library/offline/                     active content
  library/online/
  library/textures/<gameid>/
  runtime/<platform>/
  versions/<component>/current.json    { "version": "...", "previous": "..." }
  versions/<component>/prior/          exactly one retained prior version
  recovery/<timestamp>/                recovery points, section 12
  journal.db                           SQLite, one row per in-flight operation
```

### The atomic primitive

**A pointer file written by write-temp-then-rename.** `versions/<component>/current.json` is written to a temporary name in the same directory and then renamed over the existing file. `rename(2)` is atomic on POSIX and `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING` is atomic on NTFS.

This is chosen over directory renames and over symlinks or junctions on purpose: **Windows junctions and POSIX symlinks do not behave alike, and a launcher that uses them has to reason about three different sets of semantics.** A small file that is either entirely the old content or entirely the new content works identically everywhere, and it is the only thing that has to be atomic.

For components where the target must be a real directory that another process opens by fixed path, such as `library/textures/GN7E69`, the swap is a two-step: verified staging directory is renamed into place, previous is renamed aside into `prior/`, both within the same volume, and the journal records which step was reached.

**2026-09-12.** The composed texture tree (section 13) is hardlinks into the pristine folder, and this swap leaves those links on the OLD bytes. The composed record therefore carries the textures pointer's identity (`<version>#<sequence>`), and the next sync rebuilds the tree when it differs, even when every name and count still matches. Data roots are `NFL Street Reborn[ Beta]` since the same date; see the 2026-09-12 spec.

### One volume, and enough of it

**Two preconditions gate every operation in the state machine, and neither of them was previously stated.**

**Precondition 1: one volume.** `downloads/`, `staging/`, `library/`, `runtime/`, `mods/` and `versions/<component>/prior/` all live under DATA ROOT and **must be on the same filesystem**. The atomic primitive is a rename, and a rename across volumes is a copy plus a delete, which is neither atomic nor free. The same constraint applies to the hardlink composition in section 13. A DATA ROOT that spans volumes, for example a Steam Deck user who puts the library on an SD card and leaves the rest on internal storage, is not supported and must be refused at setup with a plain explanation rather than discovered during a swap.

**Precondition 2: free space, checked before anything starts.** Staging is deliberately a full second copy, so the peak requirement is real and large:

```
  need = bytes to download that are not already complete in downloads/
       + bytes materialized into staging/
       + HEADROOM
```

Worst case under the recommended container plan (RVZ delivered, offline image materialized to ISO), for one `offline-content` update, on top of the live copy that is already on disk:

| Item | Bytes |
|---|---|
| `downloads/offline-content/2.4.0/NSR.rvz` | 718,375,792 |
| `staging/offline-content/2.4.0/NSR.iso`, materialized and verified | 1,459,978,240 |
| **Peak additional free space required** | **2,178,354,032** |
| Plus `library/work/` if music has ever been edited, and it is not purged first | 1,343,794,233 |

Retaining a prior image (section 11) does not raise the peak, because the prior copy is the outgoing live copy renamed aside rather than a new copy. It raises the **steady state** by 1,459,978,240 bytes, which is exactly why section 11 recommends against it.

**Rules:**

1. **`PLANNED` cannot be entered without the check passing.** The check runs against the volume that actually holds DATA ROOT, resolved at runtime, never assumed.
2. **`HEADROOM` is not specified here.** It exists so the machine does not fill its own disk to zero, and picking the constant needs a measurement on a Steam Deck rather than a guess. Open item.
3. **Failing the check is a terminal, non-destructive state.** `SPACE_SHORT` reports how much is needed and how much is free, in the four-part error form, and deletes nothing. The one thing it may offer to delete is `library/work/`, because that is derived and purgeable by definition (section 15).
4. **Space can still run out mid-stage**, because other software is also writing to the disk. An out-of-space error during `DOWNLOADING` or while materializing into `staging/` is handled exactly like a download failure: the operation stops before `SWAPPING`, **the live install is untouched by construction**, and the partial staging directory is offered for discard because it is derived and can be rebuilt.
5. The check is per operation, not per session. Two queued component updates are checked as one requirement, because they stage concurrently.

### State machine

```
  IDLE
   │ manifest fetched, signature verified, version differs
   ▼
  PLANNED ──────────────────────────────────────┐
   │ same-volume and free-space precheck        │
   ▼                                            │
  PRECHECKED ── SPACE_SHORT ────────────────────┤
   │ download                                   │
   ▼                                            │
  DOWNLOADING ── network failure ── resumes ────┤
   │ complete                                   │  any failure here
   ▼                                            │  changes NOTHING
  STAGED                                        │  on the live install
   │ tier 1 hash over the staged set            │
   ▼                                            │
  VERIFIED ── mismatch ─────────────────────────┤
   │ quiescent? (section below)                 │
   ▼                                            │
  SWAPPING                                      │
   │ pointer rename                             │
   ▼                                            │
  HEALTH_CHECK ── fail ── AUTO_ROLLBACK ────────┤
   │ pass                                       ▼
   ▼                                          FAILED
  ACTIVE                                      (reported, nothing changed on the
                                               live install; staged bytes are
                                               retained for retry when any exist)
```

`SPACE_SHORT` is a terminal failure like any other, and it is the cheapest one: it happens before a single byte is downloaded.

**Crash recovery is a journal replay.** On startup the launcher reads `journal.db`. A row stuck in `DOWNLOADING` or `STAGED` resumes or is discarded. A row stuck in `SWAPPING` is resolved by reading the pointer file, which is either wholly old or wholly new, verifying whatever it names, and switching to `previous` if that verification fails. **There is no partial state to repair, because the only thing that ever changed atomically is one small file.**

### The quiescence rule

**No swap of `runtime`, `offline-content` or `online-content` happens while Dolphin is running.** This is not a preference, it is a fact about open files: the existing music tool already has to handle it, and `build_with_retry` catches a `PermissionError` on the output ISO and asks the user to close Dolphin and retry.

The launcher is in a better position than that tool, because the supervisor already knows whether the game is running. So the update is refused before it starts rather than failing partway, and the design copy already says exactly this:

> "Update ready. 210 MB. Installs the next time you close the game."

`textures`, `mods` and `controller-db` can swap while the game is closed but the launcher is open. `launcher` requires a restart, which is one of the two permitted blocking surfaces only when it is a required online update.

### Replacing the launcher itself

**The launcher is the one component that cannot apply its own update, and nothing above covers it.** On Windows a running executable is locked and cannot be renamed over. The quiescence rule solves the Dolphin case and says nothing about this one.

**The supervisor owns the launcher swap, and the launcher owns the supervisor swap.** Both binaries already exist in the install list in section 16, so this needs no new component. The rule is that **the process performing a swap is never the process being swapped**, and **never both in one operation**.

```
 Launcher update, performed by the supervisor
 1  launcher stages and verifies the new bundle in staging/launcher/<version>/   (normal machinery)
 2  launcher writes the journal row, asks the supervisor to apply, and EXITS
 3  supervisor waits for the launcher process to be gone, with a timeout
 4  supervisor renames INSTALL ROOT/app aside to versions/launcher/prior/
 5  supervisor renames staging/launcher/<version>/ into INSTALL ROOT/app
 6  supervisor starts the new launcher and waits for a ready signal
 7  ready      -> pointer file updated, ACTIVE, result reported inline on next launch
    not ready  -> supervisor reverses steps 5 and 4 and starts the prior launcher
```

**This answers the question section 11 leaves open.** The launcher's health check is "process starts and signals ready within a timeout", and **the supervisor is what performs it**, because the supervisor is the only participant that is still running and unchanged across the swap. The ready signal is an explicit handshake from the new launcher to the supervisor, not a guess from process liveness, because a process that starts and immediately crashes is still briefly a live process.

**The supervisor updates by the mirror of that sequence**, performed by the launcher. If the supervisor is missing or unresponsive when a launcher update is requested, the update does not start: **a launcher swap with nothing able to roll it back is exactly the failure this design exists to prevent.**

**If both fail**, the recovery path is the platform installer, which the user already has: it writes INSTALL ROOT only and, per section 16, has no code path into DATA ROOT, so re-running it costs a small download and loses nothing. That is the reason the installer's reduction to install-only is a safety property and not just a size saving.

**Timeout values are unmeasured** and must not be guessed here; open item 15 already records that. **All of this is PROPOSED**: no launcher, no supervisor and no bundle exists yet, so none of it has been tested on any platform. The macOS `.app` case in particular is a directory rename rather than a file rename and has not been checked.

---

## 11. Health checks and rollback

### Health check per component

| Component | Health check | Status |
|---|---|---|
| `launcher` | Process starts and signals ready within a timeout, **checked by the supervisor**, which is the only participant still running across the swap (section 10) | **Timeout value unspecified.** Needs measurement, not a guess |
| `runtime` | Sys tree hashes match, binary starts and reports a version | **The version probe is unverified.** No headless version query has been tested against this fork |
| `offline-content` | File SHA-256 plus `DolphinTool verify` disc SHA-1 | Proven tooling |
| `online-content` | Same, plus the hash the backend expects for `online_minimum` | Proven tooling; the backend side does not exist yet |
| `textures` | Manifest file count plus full SHA-256 sweep | Proven |
| `mods` | Composed active tree replays cleanly from the activation record | Section 13 |
| `controller-db` | Parses, and every mapping row is well formed | Trivial |

### What rollback means, per component

The blueprint's definition, which questionnaire rows 18.6 and 18.7 mark Decided, is: keep one prior launcher version, one prior Dolphin runtime version, "enough patch state to regenerate or restore the previous game build", automatically roll back after a failed health check, allow manual rollback from diagnostics, and ship no beta or developer channels.

**"Enough patch state to regenerate the previous game build" no longer means anything**, because there is no patch state and nothing regenerates. `COMPONENT_MAP.md` section 7 goes further and says every one of its eight components keeps exactly one prior version. **That instruction has a cost that is unstated anywhere in the project, and here it is.**

| Component | Prior version retained as | Retention cost | Rollback is |
|---|---|---|---|
| `launcher` | Prior bundle | Small, unmeasured | Local, instant |
| `runtime` | Prior tree | 108,045,862 bytes | Local, instant |
| `offline-content` | Prior image, or nothing | **1,459,978,240 bytes as ISO, 718,375,792 as RVZ** (measured) | Local if retained, otherwise a full re-download |
| `online-content` | Prior image, or nothing | **1,459,978,240 bytes as ISO, 718,354,776 as RVZ** (measured on today's `FR4N31.iso`; an estimate for the rebuilt image, per section 3A) | Same |
| `textures` | Prior versions of changed files only | Proportional to the change | Local, instant |
| `mods` | Prior package version | Kilobytes to a few MB | Local, instant |
| `controller-db` | Prior file | Kilobytes | Local, instant |

Retaining a prior copy of **both** images costs **2,919,956,480 bytes as ISO, or 1,436,730,568 bytes as RVZ**, permanently, on top of the current copies. Both RVZ figures are measured, not derived: 718,375,792 + 718,354,776. On a Steam Deck that is a meaningful fraction of the drive.

**Recommended: do not retain prior disc images locally. Define rollback for the two content components as a re-download of the prior release's artifact, and require the host to keep the previous release's blobs available.**

Reasoning: a game-content rollback is rare, while the disk cost is continuous. Trading a rare 718 MB download against a permanent 1.4 GB of disk is the right trade on a handheld, and it is the wrong trade only for a user who rolls back frequently, which is not a real user. **This needs Adam's decision, because it contradicts `COMPONENT_MAP.md` section 7's blanket rule.**

The consequence must be stated plainly in the UI when it happens, per the four-part error rule: rolling back the game is a download, not an undo, and the user is told the size before it starts.

### Automatic rollback

A failed health check rolls back automatically and without asking, because at that moment the product is broken and a dialog is not a fix. It then reports what happened, what remains safe, and what to do next. The one exception is a content rollback that requires a download: that cannot be automatic, so a failed content health check leaves the component marked invalid, the Play button becomes `[ Repair and Play ]` per `WIREFRAMES.md` section 7, and the repair path takes over.

---

## 12. Repair

### Entry points, all four already designed

| Entry | Source |
|---|---|
| Play page secondary action, and the X hint | `WIREFRAMES.md` section 7 |
| The Play button itself, as `[ Repair and Play ]` with a plain-language reason | `WIREFRAMES.md` section 7, broken variant |
| Crash recovery page, one of six actions | `WIREFRAMES.md` section 15 |
| `reborn://repair`, "always available" | `INFORMATION_ARCHITECTURE.md` section 9 |

The deep-link security rule holds: a deep link may navigate and pre-fill, it may never install, delete, queue or spend. `reborn://repair` opens the repair screen. It does not start a repair.

### Sequence

```
 1  Take a recovery point                    ALWAYS, before anything
 2  Tier 0 sweep                             presence and size
 3  Tier 1 sweep                             SHA-256 over the manifest
 4  Classify every difference                table below
 5  Re-fetch only what is wrong              per component granularity
 6  Verify, swap, health check               the same machinery as section 10
```

**Step 1 is not optional.** Questionnaire row 19.1 requires a recovery point before updates, repairs, migrations, and mod and music operations. A recovery point is a copy of `saves/` (the memory card is 16,777,216 bytes and `SRAM.raw` is 68 bytes), the mods activation record, the music state record, and the current pointer set. **About 16 MB and a few small JSON files, which is cheap enough to take every time without asking.** Retention count is an open item.

### Classification

| Finding | Meaning | Action |
|---|---|---|
| Missing | File absent | Re-fetch that file |
| Wrong size | Truncated or partial | Re-fetch that file |
| Hash mismatch, non-customizable component | Damage | Re-fetch that file |
| Hash mismatch, `customizable: true`, matches the launcher's own recorded rebuild hash | **Custom music. Not damage** | Leave alone |
| Hash mismatch, `customizable: true`, matches nothing recorded | Damage | Re-fetch the whole artifact |
| Extra file not in the manifest, inside a mod-composed tree | Expected | Leave alone |
| Extra file not in the manifest, inside a managed tree | Stale or foreign | Report, do not delete without confirmation |

### Repair granularity, stated honestly

`runtime`, `textures` and `mods` repair **per file**, which is why their manifests carry a row per file. That is genuinely cheap: one bad texture costs one texture.

**The two disc images do not repair per file. A mismatched image is a full re-fetch of that artifact, 1.46 GB as ISO or 718 MB as RVZ.** The FST exposes 63 addressable files inside the image, so per-file repair inside the image is conceivable in principle, but nothing in the project does it and no tool was found that writes a file back into an image in place. Treat per-file image repair as unbuilt, not as available.

### The false positive that will otherwise define this feature

**A user with custom music has a locally modified offline image whose hash will never match the manifest. This is the most likely repair-path false positive in the product, and without the mechanism below, repair would silently delete every user's soundtrack.**

Editing a slot grows `pathfind.dat`. The shipped image holds it at 114,735,104 bytes; the development tree's live copy is 119,836,672 bytes (verified). The disc is full, and `pathfind.dat` is the **61st of 63 files by offset** (at offset 631,017,472, with only `crowddat.dat` and `chatdata.dat` after it; `template.dat` is 13th and `chatdata.dat` is 63rd and last). Total inter-file padding is 52,772 bytes and the last file ends exactly at 1,459,978,240, so growth forces a full repack. **The rebuilt image is a different size and a different hash by construction. This is unavoidable, not a bug to fix.**

Resolution: the offline image has **two** recorded identities, not one.

```
  manifest sha256          from the signed manifest, the pristine artifact
  local sha256             recorded by the launcher after every rebuild it performs
  music state record       base version, ordered edit list, resulting hash
```

| local vs recorded | Verdict |
|---|---|
| local == manifest | Pristine |
| local == the launcher's own recorded rebuild hash, and a music state record exists | **Customized. Not damaged** |
| Neither | Damaged. Repair by re-fetching the artifact, then re-applying the music state record |

**`online-content` is never customizable.** Music is offline only (questionnaire row 9.3), so the online image is always pristine and always repairs cleanly. That is precisely why the already-written copy can promise what it promises:

> "Your online game doesn't match the current version. We can repair it now, about 90 seconds. Your offline game and custom music aren't affected."

The 90 second figure in that copy is **unverified**. It is a plausible local verify-and-swap time and an implausible 718 MB download time on most connections. The copy should carry a computed estimate, not a fixed number.

---

## 13. The mod package format

Questionnaire row 11.7 is Open. This proposes the format, and the proposal is deliberately small, because **all eleven existing mods are pure texture overlays and every one of their 57 files sits under `User\Load\Textures\GN7E69` and nowhere else.** A first format that only has to express texture overlays can be simple and still be correct.

### Container

`.nsmod`, a ZIP, named for symmetry with the existing `.nsmp`.

```
purple-football.nsmod            (ZIP)
  package.json                   manifest, magic NSMD
  art/cover.png                  16:9, feeds the content card in COMPONENT_SPECIFICATION.md section 5
  files/
    GN7E69/
      tex1_128x128_m_ffc5151a141db330_14.dds
    FR4N31/
      tex1_128x128_m_ffc5151a141db330_14.dds
    CR4E12/
      tex1_128x128_m_ffc5151a141db330_14.dds
  LICENSE.txt                    optional
```

### Manifest

```json
{
  "schema": "nsr.mod.v1",
  "magic": "NSMD",
  "id": "reborn.ball.purple",
  "name": "Purple Football",
  "version": "1.0.0",
  "author": "unknown",
  "category": "balls",
  "description": "",
  "targets": ["GN7E69", "FR4N31", "CR4E12"],
  "conflict_group": "ball",
  "online_safe": true,
  "requires": { "launcher": ">=1.0.0" },
  "files": [
    { "path": "GN7E69/tex1_128x128_m_ffc5151a141db330_14.dds",
      "sha256": "<64 hex>", "bytes": 1398276 }
  ],
  "changelog": ""
}
```

Every field in questionnaire row 11.3 is present: ID, name, version, author, category, description, art, size, hashes, dependencies, conflicts, minimum versions, changelog.

### Four design decisions, each with its evidence

**1. `targets` is a game-ID-keyed subtree, because today no mod applies to more than one build.** All eleven target `GN7E69` only, so none of them works on `FR4N31` or `CR4E12`. Keying the payload on game ID is what lets one package cover all three builds instead of shipping three packages.

**2. Conflicts are computed AND declared, and the computed one wins.** The data already determines the conflict groups: all seven football mods write the identical single path `tex1_128x128_m_ffc5151a141db330_14.dds`, and `PS Controls` and `Xbox Controls` have identical 19-file name sets. **They are mutually exclusive by construction, not by declaration.** So overlapping paths are an authoritative hard conflict that no manifest can override, and `conflict_group` exists only to drive the pick-one UI for packages that conflict conceptually without overlapping by path.

**3. Packages are not individually signed. They are hash-pinned by the signed catalog.** This matches blueprint section 19: "Clients download immutable versions by hash." A locally sideloaded package has no catalog entry, therefore no pin, and the UI marks it as such. That is the honest distinction and it costs nothing to implement.

**4. `nsr.mod.v1` permits texture overlay paths and nothing else.** Any entry outside `files/<gameid>/` is rejected at import. That is simultaneously the format's scope limit and its path-traversal defense, which the trust-boundary model in `COMPONENT_MAP.md` section 6 requires. A future format version can widen it; version 1 does not need to.

`online_safe` is `true` for texture overlays, because texture mods are client-side and permitted online (questionnaire row 9.4). Nothing that is not a texture overlay can be marked safe, and version 1 cannot express anything else, so the field is currently always `true`. It exists so that the first non-texture package does not require a format break.

### Activation, and why it never needs a backup folder

**JSGME applies a mod by moving the original file aside into `!BACKUP` and copying the mod file into the base tree.** That is why losing `!BACKUP` destroys the base, and it is exactly what the current installer does: `CleanTexturesOnUpdate` deletes the whole `MODS` folder, `!BACKUP` included, leaving overlaid files with no restore path.

**Replacement: the base texture tree is never written to.**

```
  library/textures/GN7E69/                     pristine, manifest-verified, READ ONLY
  mods/packages/<id>/<version>/                extracted package payloads
  <user folder>/Load/Textures/GN7E69/          COMPOSED tree, what Dolphin actually loads
                                               (was mods/active/GN7E69/ plus a junction until 2026-09-12)
  mods/activation.json                         ordered list of active package ids and versions
```

The composed tree is built by hardlinking every pristine file, then replacing the entries that active packages override. **Deactivation is deleting the composed tree and rebuilding it from `activation.json`.**

**The cost of composing is PROPOSED, not measured.** No composition has been performed, no link count or timing exists, and the claim rests on the general property that a hardlink is a directory entry rather than a copy. What can be stated as fact is the shape of the cost and its preconditions, which matter more than the number:

| Condition | Status |
|---|---|
| Pristine tree and composed tree on the **same volume** | **Required.** A hardlink cannot cross filesystems. This is the same constraint the rename primitive imposes in section 10, and it was previously stated there and not here |
| NTFS, ext4, APFS | Support file hardlinks. **Untested here for a 2,763-entry tree** |
| FAT32 and exFAT | **Do not support hardlinks at all.** This is not hypothetical: it is the default format of a Steam Deck SD card and of most external drives, which is the Xbox transfer target |
| Fallback where links are unavailable | A full copy, **2,077,362,716 bytes per composed game ID**, plus the time to write it |
| Composition time | **Unmeasured.** 2,763 directory entries is small, but no run has been timed on a Steam Deck |

**What has to be measured before this is stated as a saving:** compose the `GN7E69` tree once on NTFS and once on ext4, record wall time and the actual disk delta, and record what happens on exFAT. Until then the correct claim is "composition is expected to cost directory entries rather than bytes on NTFS, ext4 and APFS, and to cost a full copy elsewhere", and the exFAT fallback has to be budgeted for as though it is the common case on a handheld.

This delivers questionnaire row 11.4 (atomic activate and deactivate with exact restore, plus interrupted-install recovery) almost for free: an interrupted composition leaves a partial tree that is **discarded and rebuilt**, never repaired, because the source of truth is a pristine tree that was never touched plus a small ordered list.

**One unverified dependency, and it matters.** This assumes Dolphin can be pointed at the composed directory as its texture load path. That is a directory path in configuration and the launcher generates the configuration, so it is very likely fine. What is **not** verified is the alternative design, where Dolphin resolves multiple stacked load paths in a defined precedence order. **Composition avoids needing that, which is the main reason to compose rather than to stack.**

### The 2 GB the overlay model recovers

Controller prompt selection today is implemented by duplicating an entire texture tree. `FR4N31` (2,036,699,988 bytes) and `FR4N31 - PS` (2,036,907,454 bytes) contain exactly the same 2,827 filenames. **All 2,827 were hashed in full, not sampled: 2,808 are byte-identical and exactly 19 differ**, totaling 1,849,569 bytes on the Xbox side and 2,057,035 bytes on the PlayStation side. **Two gigabytes are duplicated to deliver two megabytes of difference, and the chooser is a manual folder rename.**

The same duplication exists across game IDs: of `FR4N31`'s 2,354 top-level files, 2,252 files totaling 1,955,293,013 bytes (98.6 percent by size) share a filename and size with the `GN7E69` pack.

**The five texture trees in the selected runtime total 8,476,993,252 bytes, which is 8,084.3 MiB.** Measured August 29, 2026:

| Tree | Files | Bytes |
|---|---|---|
| `GN7E69` | 2,782 | 2,079,212,285 |
| `FR4N31` | 2,827 | 2,036,699,988 |
| `FR4N31 - PS` | 2,827 | 2,036,907,454 |
| `CR4E12` | 1,332 | 1,162,261,971 |
| `CR4E12 - PS` | 1,332 | 1,161,911,554 |
| **Total** | **11,100** | **8,476,993,252** |

An earlier draft said 8,111 MiB, which is neither the byte total (8,084.3 MiB) nor the 4 KiB-allocated total (8,104.3 MiB). **Every size in this document is a byte length**; where an allocated size is meant it is named as one.

**A 19-file prompt package composed over one pristine tree retires all of that.** It is conditional on the composition model above working as described, which is why it is stated as a consequence and not as a completed saving.

---

## 14. Migrating the eleven packages

### There are eleven, not thirteen

`COMPONENT_INVENTORY.md` and questionnaire row 11.6 both say 13. **Verified directly: the `MODS` folder holds 13 directory entries plus `JSGME.ini` at 0 bytes. Two of the directories, `!BACKUP` and `!INSTLOGS`, are JSGME control folders, both empty. There are eleven migratable packages.** A migration plan written against thirteen will try to import JSGME's own restore machinery.

The zero-byte `JSGME.ini` also means **no mod is enabled in the shipped state**, which makes a clean install the easy migration case.

### The eleven, measured

**Sizes are byte lengths, measured August 29, 2026 in `Files\Reborn Dolphin\MODS`.** An earlier draft of this table carried disk-allocated sizes (1,368 KB, 2,052 KB, and so on), which are 4 KiB-rounded and are **not** what a manifest may record. Section 5 explains why that distinction is load bearing: a `bytes` value taken from an allocated size fails the tier 0 check on a pristine install.

| Package | Files | Bytes | KiB | Proposed id | Conflict group | Category |
|---|---|---|---|---|---|---|
| Aqua Football | 1 | 1,398,276 | 1,365.5 | `reborn.ball.aqua` | `ball` | Balls |
| Blue Football | 1 | 1,398,276 | 1,365.5 | `reborn.ball.blue` | `ball` | Balls |
| Camo Football | 1 | 1,398,276 | 1,365.5 | `reborn.ball.camo` | `ball` | Balls |
| Green Football | 1 | 1,398,276 | 1,365.5 | `reborn.ball.green` | `ball` | Balls |
| Panda Football | 1 | 1,398,276 | 1,365.5 | `reborn.ball.panda` | `ball` | Balls |
| Pink Football | 1 | 1,398,276 | 1,365.5 | `reborn.ball.pink` | `ball` | Balls |
| Purple Football | 1 | 1,398,276 | 1,365.5 | `reborn.ball.purple` | `ball` | Balls |
| PS Controls | 19 | 2,057,035 | 2,008.8 | `reborn.prompts.ps` | `prompts` | Controller Prompts |
| Xbox Controls | 19 | 1,849,569 | 1,806.2 | `reborn.prompts.xbox` | `prompts` | Controller Prompts |
| Patriots Throwback | 5 | 3,670,932 | 3,584.9 | `reborn.jersey.patriots-throwback` | `jersey.ne` | **No category exists** |
| Saints Alternate | 7 | 3,168,988 | 3,094.7 | `reborn.jersey.saints-alternate` | `jersey.no` | **No category exists** |
| **Total** | **57** | **20,534,456** | **20,053.2** | | | |

**57 files, 20,534,456 bytes, 19.58 MiB.** The seven footballs are the same single file `tex1_128x128_m_ffc5151a141db330_14.dds` at seven different contents, 1,398,276 bytes each; that is the file whose length section 5 uses as the worked example of allocated size versus byte length. `Xbox Controls` is byte-for-byte the 19 PNG files currently sitting applied in the selected runtime's texture tree, per section 2A.

### Migration sequence

```
 0  Detect JSGME state
      MODS\JSGME.ini non-empty, or !BACKUP non-empty
        -> the user has mods applied and the base tree is polluted
 1  Un-apply through JSGME's own !BACKUP first          questionnaire row 11.9
      if un-apply cannot complete, DO NOT partially restore
        -> re-fetch the pristine texture component instead (a repair)
 2  Extract each of the eleven folders into .nsmod payloads under files/GN7E69/
 3  Assign ids, version 1.0.0, categories, conflict groups
 4  Hash every file, write package.json
 5  Publish to the catalog, or hold back (see below)
 6  Re-activate whatever the user had enabled, through composition
```

**Step 1's failure branch is the important one.** A partial JSGME un-apply leaves a base tree that is neither pristine nor correctly modded, and there is no way to tell which files are which. Re-fetching the pristine texture component is slower and it is certain, and because the texture manifest is per file, it only re-fetches what actually differs.

### Three things migration cannot invent

1. **Authorship.** No folder carries an author, a version, a description or a license. The folder name is the only identity. The only attribution recorded anywhere in the project is the shipped `README.txt` credit to BlueSwordM (Dolphin fork) and nfl_numbers (jersey number textures). Author is recorded as `unknown` and **must be filled in by Adam before catalog publication.** Publishing eleven packages with a fabricated author is worse than publishing them with none.
2. **Categories for the two jersey mods.** Questionnaire row 11.1 fixes the initial categories as Music Packs, Balls, Controller Prompts. Patriots Throwback and Saints Alternate fit none of them. Either a Jerseys category is added or those two are held back from the initial catalog. **A decision, not a technicality.**
3. **Licensing.** None of the eleven carries a license file. Whether they can be redistributed through a curated catalog is unanswered.

### Where the prompt packs go

`PS Controls` and `Xbox Controls` migrate into the same `.nsmod` format, but they are **owned by the controller service, not browsed in Mods.** `SYSTEM_ARCHITECTURE.md` section 5.3 already rules them "reuse as prompt packs, since that is all they ever were", and they are selected by detected device family rather than picked from a catalog. They are recorded here so the two documents describe the same objects.

JSGME's own redistribution terms are unchecked (questionnaire row 11.8). **This model does not ship JSGME at any point, including during migration**, so the question only matters if that changes.

---

## 15. Music and content versioning

> **As built (2026-09-08, `docs/superpowers/specs/2026-09-08-music-editor-design.md`):**
> the shipped design has no `library/work/` tree. An apply reads the two files
> it changes straight out of the pristine and booted images and streams a new
> ISO with a regenerated FST, so nothing is extracted and nothing is purged.
> The record is `music/soundtrack.json` (schema `nsr.soundtrack.v2`: the
> switch, the base soundtrack package, the 21 slots, the staged changes);
> `music/state.json` stays the shared `RebuildState` this section describes.
> After a roster or game patch rebuilds the image, `Updater::promote` replays
> the soundtrack through `apply_current_soundtrack`, which is the "Reapply your
> custom music" step below, done inline rather than queued. The rest of this
> section is the design as it was proposed.

The music feature is the only thing in the product that deliberately makes a managed artifact stop matching its manifest. That makes it a content-versioning problem, not only a feature.

### `library/work/`, and what it is derived from

`COMPONENT_MAP.md` section 5 places the work folder at `music/work/`. **It belongs at `library/work/`**, beside the thing it is extracted from, and `SYSTEM_ARCHITECTURE.md` section 4 already has it there. This document confirms that placement, and adds the field that makes it safe.

```
library/work/
  .source.json      { "component": "offline-content",
                      "version": "2.4.0",
                      "artifact_sha256": "<the artifact this came from>",
                      "extracted_at": "..." }
  .pristine/        untouched copies of the files the chain mutates
  sys/              boot.bin, bi2.bin, apploader.img, fst.bin, main.dol
  files/            the 63 game files
```

**Extraction is already tooled, and the byte accounting is exact.** `DolphinTool extract` produced 68 files totaling 1,343,794,233 bytes, laid out as `sys/{boot.bin 1,088 B, bi2.bin 8,192 B, apploader.img 119,332 B, fst.bin 1,529 B, main.dol 3,171,840 B}` plus `files/`. The total checks out arithmetically: 1,340,492,252 bytes of FST payload plus 3,301,981 bytes of system files, and `ISO.hdr` 9,280, `Game.toc` 1,529 and `AppLoader.ldr` 119,332 all match the live `&&systemdata` in the development tree.

**The timing is one unrepeated run on one Windows developer machine and must not be treated as a specification.** That run took about 1.9 seconds. It was not repeated, the machine's storage was not recorded, and nothing comparable has been run on a Steam Deck, on a mechanical drive, or on macOS. **This document applies to that number exactly the rule it applies to the four-minute rebuild estimate below: it does not go in the UI, and it is not the justification for a design on its own.** It is quoted here as evidence that extraction is cheap enough to be worth re-measuring, not as evidence that it takes 1.9 seconds anywhere else.

**That layout maps one to one onto what the proven Python chain expects**, with a rename shim and one concatenation:

| DolphinTool output | Chain expects | Relationship |
|---|---|---|
| `boot.bin` (1,088) + `bi2.bin` (8,192) | `&&systemdata/ISO.hdr` (9,280) | Concatenation. 1,088 + 8,192 = 9,280 |
| `fst.bin` (1,529) | `&&systemdata/Game.toc` (1,529) | Same size, same role, rename |
| `apploader.img` (119,332) | `&&systemdata/AppLoader.ldr` (119,332) | Identical size, rename |
| `main.dol` | `&&systemdata/Start.dol` | Rename |

**`.pristine/` replaces the dated backup file.** The existing chain restores by copying `GOOD_BASE`, which is `pathfind.dat.bak_good_2026-07-02` at 114,735,104 bytes, exactly the size of `pathfind.dat` inside the shipped ISO. A dated `.bak` file on one developer's disk is not a restore point for a shipped product, and it violates the standing rule against restoring from stale backups. The launcher's equivalent is a pristine copy captured at extraction time, whose provenance is recorded in `.source.json`.

### Invalidation

**`library/work/` is derived and purgeable. It is never repaired.**

When `offline-content` changes version, `.source.json` no longer matches the active artifact, the work tree is discarded, and the next music operation re-extracts.

**Why discard-and-re-extract is right does not depend on the 1.9 second number.** It is right because the alternative is repairing a derived tree against an artifact it was not derived from, which needs its own manifest, its own hashes and its own failure modes for a directory that can always be rebuilt from something already verified. The cost of re-extraction is one artifact read and 1,343,794,233 bytes written, on a volume that section 10's precheck has already confirmed has room. **If a real measurement on the slowest supported machine ever shows re-extraction is expensive enough to matter, the answer is to keep the work tree across compatible versions, not to start repairing it.**

**Queued edits survive this, and that is the whole point of storing them separately.** The edit queue is a list of *intents* (slot number, source file or `.nsmp` reference, metadata), not a list of byte diffs. Intents re-apply against a new base. Byte diffs would not.

```
  music/sources/        USER FILES, never deleted
  music/packs/          .nsmp packs, never deleted
  music/state.json      base version, ordered applied edits, resulting hash
  music/queue.json      pending intents, survives a base change
  library/work/         DERIVED, discarded when the base changes
```

`music/state.json` serves two features with one mechanism: it is what tells repair that a modified image is customized rather than damaged (section 12), and it is what re-applies a soundtrack after a content update.

### What a content update does to a customized game

**Proposed, and it needs Adam's confirmation because it is the behavior that decides whether the feature feels safe.**

1. The content update stages, verifies and swaps normally. The offline image becomes pristine at the new version.
2. The launcher queues a named background task: **"Reapply your custom music."**
3. It re-extracts, replays `music/state.json` against the new base, and does one rebuild.
4. Any slot whose `.nsmp` or source file no longer exists is listed and skipped, never silently dropped.
5. Result surfaces inline on the Music page and on the Play page, never as a toast, per `INFORMATION_ARCHITECTURE.md` section 7.

**Nothing in this sequence blocks, and the game is playable at every step**, with the standard soundtrack in the window between the swap and the rebuild.

### Batching, preserved exactly as the proven tool does it

`apply_edits` queues any number of slot edits, then runs encode per slot, `fix_pfdx_toc` per slot, metadata writes, and **exactly one ISO build at the end.** Pack import splices pre-encoded blobs with no FFmpeg and no re-encode, then does one build.

**The launcher preserves that shape: one build per commit, never one per song.** A rebuild per song edit would be unusable, which is why `SYSTEM_ARCHITECTURE.md` section 5.5 already requires batching.

### Why the two ISO builders are ported and not wrapped

`SYSTEM_ARCHITECTURE.md` sections 5.1 and 5.5 say wrap. **Neither script can run on a player's machine, for four specific reasons.**

`build_iso_with_game12.py`:
1. Hardcodes its output to `%APPDATA%\NFL Street Reborn\Games\NFL Street 2 BEACH FIELD13.iso`.
2. Adds `game12.qkl`, which is not present in the shipped image.
3. Carries a `GOOD_BEACH` silent-restore guard, which is exactly the silent-fallback behavior its own successor forbids.

`build_iso_fields.py`:
4. Imports `field_importer/paths.py`, `bake_carry_table`, `ensure_audio_guards` and `verify_import`, and is driven by `registry.json` field ids plus `&&systemdata/Game.toc`. None of that exists outside the development workspace.

**Verdict: PORT.** A launcher-owned repacker takes the work tree and writes an image, carrying forward the rule restated in section 7 (the rule itself, not its punctuation) and the collect-all-failures-then-abort behavior with it. `music_editor.py`'s engine is still wrapped; it is only the *build* step that is ported.

### The rebuild time nobody has measured

The figure "about four minutes" appears in `SYSTEM_ARCHITECTURE.md` and twice in the UX language guide:

> "Rebuilding your offline game. About 4 minutes. Your saves and online game are untouched."
> "Changing this rebuilds your offline game. About 4 minutes."

**That number has no measured source anywhere.** No timing data exists in the music tools, in the `audio_soundtrack` project, or in any document. Extraction has one unrepeated developer-machine sample of about 1.9 seconds, which is a data point and not a measurement. Encode and repack have nothing at all, they are hardware-dependent, and the machines this ships to include a Steam Deck.

**Recommendation: the UI must not print a fixed number until one real rebuild is timed.** Before any measurement exists, say "a few minutes". After the first rebuild on a given machine, use that machine's own measurement, which is both more honest and more accurate than any global constant.

### One more drift worth recording

The shipped image holds `template.dat` at 500,928 bytes. The development tree's live copy is **503,336 bytes** (verified, dated August 29 2026). That is a third artifact drifting with no version identity, alongside the three-way drift in `GameSettings\GN7E69.ini` recorded in section 16.

---

## 16. Reducing the Inno Setup installer to install-only

**2026-09-12.** The installer's `DataDir` is `NFL Street Reborn` (beta: `NFL Street Reborn Beta`) and the program goes to `<DataDir>\app`. The older `... Launcher[ Beta]\app` program folder is uninstalled by the installer (`PreviousLauncherInstall`, which never picks the 2.1 uninstaller registered under the same AppId) and the data beside it is adopted by the launcher at first start. The section below is the 2026-09-05 analysis and stands as history.

### What is wrong with it, all verified

| # | Defect | Consequence |
|---|---|---|
| 1 | `CurUninstallStepChanged` runs `DelTree({app}, True, True, True)` over a folder containing `User\GC` | **Uninstall deletes the player's memory card** |
| 2 | `BackupUserData` and `RestoreUserData` pass wildcard paths to Inno Setup's `FileCopy`, which copies a single file and does not expand wildcards | **The backup has never worked.** Both log a failure and continue |
| 3 | `RestoreUserData` reads `{tmp}\JSGME_backup.ini`, which nothing in `rebornsetupv10.iss` or in any of the nine older `.iss` files ever creates | **A third silent failure. That branch can never run** |
| 4 | `CleanTexturesOnUpdate` deletes `{app}\User\Load\Textures\GN7E69` and the whole `{app}\MODS`, `!BACKUP` included | **Every update destroys mod restore data and re-copies 2 GB** |
| 5 | User data survives only because `User\*` carries `onlyifdoesntexist`, and a 16,777,216 byte `MemoryCardA.USA.raw` plus a 68 byte `SRAM.raw` ship inside the payload | **One wrong flag overwrites a player's memory card with the shipped one** |

Three of those are silent failures the user cannot see, and the fifth is a single-character distance from data loss.

### What the installer becomes

```
  Installs into INSTALL ROOT only:
    launcher binary
    supervisor binary
    bundled web assets
    bundled Python and FFmpeg

  Ships NO game image, NO textures, NO MODS, NO User tree
  DiskSpanning becomes unnecessary; the payload drops from
  1,629,604,271 bytes to tens of MB
  All four user-data procedures are DELETED, not repaired
  Uninstall touches INSTALL ROOT only and has no code path
  that can reach DATA ROOT
    enforced by: UsePreviousAppDir=no, UsePreviousData=no,
                 no DelTree anywhere, asserted at build time
    see the subsection below, this does NOT hold by default
```

**The four procedures are deleted rather than fixed on purpose.** Backup, restore, texture cleanup and mod-state preservation are staging, hashing and rollback problems, and the content service does all four properly. None of them belongs in an installer script, and repairing them there would mean maintaining two implementations of the same guarantee.

### Keeping the AppId is safe only with two directives that are not in the file today

`AppId {DCD6B9B9-11DE-4AC0-A7F5-256E7AC55A7E}` is kept, because it is the hook that recognizes an existing 2.1 install. The existing install's `DefaultDirName` is `{userappdata}\NFL Street Reborn`, which is the same path the new data root uses. **That is convenient for migration and dangerous for uninstall**, so the install root moves to `%LOCALAPPDATA%\Programs\NFL Street Reborn Launcher` per `SYSTEM_ARCHITECTURE.md` section 4, and the old folder is treated as a data root to migrate *from*, never as an app folder to delete. `PrivilegesRequired=lowest` is retained; nothing here needs elevation.

**Verified in `rebornsetupv10.iss` on August 29, 2026: the file contains no `UsePreviousAppDir` and no `UsePreviousData` directive.** `AppId` is on line 10, `DefaultDirName={userappdata}\{#MyAppName}` on line 17, `PrivilegesRequired=lowest` on line 21, and nothing else in the `[Setup]` section touches directory reuse.

**That is the whole problem.** Inno Setup defaults `UsePreviousAppDir` to **yes**. With the AppId retained and the default left alone, the new installer looks up the previous install's recorded directory and sets `{app}` to it: `{userappdata}\NFL Street Reborn`. The new `DefaultDirName` is then ignored on exactly the machines that matter, every existing 2.1 install, and the install root silently becomes the data root. Everything the section above says about uninstall reaching only INSTALL ROOT stops being true at that moment, because INSTALL ROOT *is* DATA ROOT again.

**The safety property has to be enforced by named directives, not asserted:**

| Directive | Value | Why |
|---|---|---|
| `UsePreviousAppDir` | `no` | Forces `{app}` to `DefaultDirName` even when a prior install is detected. Without this line, retaining the AppId is unsafe |
| `UsePreviousData` | `no` | Stops other remembered values from the 2.1 install leaking into the new one for the same reason |
| `DefaultDirName` | `{localappdata}\Programs\NFL Street Reborn Launcher` | The new install root |
| `AppId` | unchanged | The migration hook. It is what lets the new installer *find* the old folder in order to migrate from it |
| `[UninstallDelete]` and `CurUninstallStepChanged` | **no `DelTree` of any kind** | Defect 1 above. The uninstaller removes only what `[Files]` installed |

**Packaging asserts all five.** The build step greps the compiled script for `UsePreviousAppDir=no`, `UsePreviousData=no` and the absence of `DelTree`, and fails the build if any is missing, in the same collect-all-failures-then-abort style as section 7. **This is the mechanism the safety claim depends on, and it is PROPOSED: no replacement `.iss` has been written and no upgrade over a real 2.1 install has been tested.** An upgrade test on a machine with an existing 2.1 install is the one thing that proves it, and it has not been run.

### Releases are built from a tag, never from a working folder

The installer source folder is **not a frozen release artifact.** It contains `GFX.ini.bak_pre_efbaccess_2026-08-25_034309` and `GameSettings\GN7E69.ini.bak_pre_efbaccess_2026-08-25_034328`, both dated more than seven months after the January 11 2026 release build. **Packaging from that folder today would ship content that was never released.**

So: a release is built from a tagged tree, its manifest is generated from what was built, and the manifest is the record of what shipped. That is the rule this whole document exists to make possible, and it is stated here because packaging is where it is broken today.

### Configuration is generated, never copied

Per-game configuration is a distinct content class that has already drifted three ways with no version identity:

| Copy | Size | Difference |
|---|---|---|
| Windows payload `GameSettings\GN7E69.ini` | 1,650 bytes | Contains a `[Video_Hacks]` `EFBAccessEnable = True` block with a comment explaining that created-player portraits are captured through EFB CPU reads |
| Xbox package copy | 1,405 bytes | **Does not contain that block** |
| Development runtime copy | 1,731 bytes | Adds two developer Gecko codes ("Preview AWAY Jerseys", "Force AWAY jersey NUMBER"), present but not enabled |

Copying configuration forward also ships one machine's identifiers: the Xbox `Dolphin.ini` carries an `[Analytics]` ID line with no `Enabled` key, while the desktop configs set `Enabled = False` and `PermissionAsked = True`. The root `JSGME.ini` ships `FullPath=C:\Users\Adam\Desktop\Reborn Installer\Files\Reborn Dolphin\Reborn Mod Manager (JSGME).exe`.

**Generation removes all of it**, and it is already the ruling in `DOLPHIN_INTEGRATION.md` section 6. Configuration therefore is not one of the seven components; it is an output of the launcher's settings model, versioned with the launcher.

**One trap the generator must not fall into.** Dolphin's own self-update is off today and must stay off: both configs carry an `[AutoUpdate]` section with an empty `UpdateTrack`, and there is no `Updater.exe` in either folder. But the fork defaults `UpdateTrack` to `Common::GetScmUpdateTrackStr()`, so **a generated config that simply omits the key re-enables Dolphin's self-updater.** The generator must write the key explicitly and empty, and packaging should assert it.

### The Xbox target is not versioned at all

The Xbox package is a third delivery target with its own copies of everything: `Step 1` is 1,403 MiB (the identical image under another name) and `Step 2` is 1,489 MiB. Its payload has **already drifted** from the Windows payload, missing the EFB access block above.

The locked decision keeps the Xbox wizard to a file mover, so **there is no update mechanism for the Xbox copy, and there is currently no way for a user to learn that theirs is stale.** The minimum this model can offer is that the wizard writes a small version stamp alongside the payload it copies, so the wizard can say "this drive has release 2.3, current is 2.4" on the next run. That is a proposal, and `XBOX_DEV_MODE_INSTALLER.md` owns whether it is adopted.

---

## 17. The blocking-update UX contract

Already fixed by the design documents. Restated here as a contract because this is the subsystem most likely to break it.

### The contract

| # | Clause | Source |
|---|---|---|
| 1 | **Exactly two blocking surfaces exist**: a required online version update when the user is trying to play online, and first-run preparation. Nothing in this document may add a third | `INFORMATION_ARCHITECTURE.md` section 6 |
| 2 | Blocking modals are not dismissible. B does nothing | `INFORMATION_ARCHITECTURE.md` section 6 |
| 3 | **Even the blocking screen offers a route out** to what still works | `COMPONENT_SPECIFICATION.md` section 10 |
| 4 | Background state lives in exactly three places: the top-bar downloads indicator and its popover, the hint strip, and inline on the object. No downloads page, no notification center | `INFORMATION_ARCHITECTURE.md` section 7 |
| 5 | Toasts never carry background-work completion | `COMPONENT_SPECIFICATION.md` section 10 |
| 6 | Progress is a named-task list, not console output. A failed task shows a reason and a Retry that **resumes rather than restarts** | `COMPONENT_SPECIFICATION.md` section 6 |
| 7 | Error copy has four parts: what failed, why, **what remains safe**, what to do next | `UX_RESEARCH_DOSSIER.md` |
| 8 | Deep links may navigate and pre-fill. They may never install, delete, queue or spend | `INFORMATION_ARCHITECTURE.md` section 9 |
| 9 | A recovery point precedes every content operation | Questionnaire row 19.1 |

### The one blocking screen this subsystem owns

```text
              [NSR logo]

        Ranked needs version 2.4

   ████████████████░░░░░░░░  68%
   142 MB of 210 MB · about 40 seconds

   Offline play, mods, and music still work.
             [ Play offline instead ]
```

The "142 MB of 210 MB" figures come from the manifest's `bytes` fields, not from an estimate, which is why `bytes` is mandatory in section 5. The version string is the release label from section 4, not a component semver.

**This screen appears only when the user is trying to play online and the client is below `online_minimum`.** It never appears on launch, it never appears on Home, and a content update that is merely available never blocks anything.

**Which `online_minimum`, and what happens when the two sources disagree**, per the precedence rules in section 4:

| Situation | What the screen shows |
|---|---|
| Backend reached, value known, manifest agrees | The screen above, with real size and progress |
| Backend reached, manifest disagrees | The **higher** of the two gates, fail closed. Same screen. The disagreement goes to diagnostics as a publisher error and is never shown to the user |
| Backend not reached yet (the user pressed Ranked while offline) | The manifest's mirrored value, and the copy says the version is required rather than that a download is in progress |
| Backend names a release the client's cached manifest does not contain | Re-fetch the release manifest once. If it is still unknown: **the required version with no size and no progress bar**, because the size comes from a manifest the client does not have. `[ Play offline instead ]` is still present, per clause 3 |

The last row is a distinct screen state and it needs its own copy, which has not been written. It is the one state where the screen cannot honor the "size before commitment" rule in section 5, and saying so plainly is better than showing a fabricated number.

### Every update event, mapped to its surface

| Event | Surface | Copy, already written |
|---|---|---|
| Update available, game closed | Downloads indicator, hint strip | "Update ready. 210 MB. Installs the next time you close the game." |
| Update available, game running | Downloads indicator only. **No prompt** | Silent until the game exits |
| Required online update, user is queueing | **Blocking screen** | "Ranked needs version 2.4. Downloading now, 40 seconds left. Offline play still works." |
| First-run preparation | **Blocking, stepped progress** | `WIREFRAMES.md` section 5 task list |
| Update installing | Inline on the object, plus the indicator | Content card `Installing` state: bar replaces the button, percentage and rate below |
| Update failed verification | Inline on the object, `error` left border | "The download finished but the file didn't match its checksum, so nothing was changed. Your game, saves, and other mods are untouched." `[ Try again ] [ Report this ]` |
| Content invalid at launch | Play button becomes `[ Repair and Play ]`, plain-language reason | `WIREFRAMES.md` section 7, broken variant |
| Online image mismatched | Repair prompt | "Your online game doesn't match the current version. We can repair it now, about 90 seconds. Your offline game and custom music aren't affected." |
| Manifest fetch or signature failed | **Nothing user-facing.** Diagnostics only | Treated as an outage, per section 6 |
| Rollback after a failed health check | Inline, after the fact | Must name what remains safe |

### The preparation task list, corrected

`COMPONENT_SPECIFICATION.md` section 6 still shows the old example: "Verify game source / Build offline game / Install Reborn Dolphin / Apply default textures / Verify final hashes". **Those are import-and-build tasks that the ship-the-image decision removed.** `WIREFRAMES.md` section 5 has the corrected list and it is the one this subsystem implements:

```text
✓  Copy game files                          done
✓  Install Reborn Dolphin                   done
◐  Apply textures              ▓▓▓▓▓▓░░░░   62%
○  Apply your controller and graphics       waiting
○  Check everything                         waiting
```

"Check everything" is the tier 1 hash sweep from section 7. It is the last task because it is the slowest, and because a failure there means the preparation failed rather than that some content is merely absent.

---

## 18. The honest size statement

Required by the task and by the anti-vibecoding rule: state what cannot be delivered differentially, and what that costs.

### The measured install

| Component | Bytes | Note |
|---|---|---|
| `offline-content` | 1,459,978,240 | 718,375,792 if RVZ |
| `online-content` | 1,459,978,240 | 718,354,776 if RVZ, measured. Today's `FR4N31.iso`, which per section 3A is not the shippable build |
| `textures` (Windows DDS) | 2,077,362,716 | 2,763 files, the January payload tree. Section 2A |
| `runtime` | 108,045,862 | 2,512 files. 47,769,619 bytes of it is Windows-only: 42,282,003 of ReShade content plus `dxgi.dll` at 5,487,616 |
| **Subtotal** | **5,105,365,058** | **about 5.1 GB** |
| `library/work/`, once music is edited | 1,343,794,233 | Derived, purgeable |
| `launcher` | **Unverified** | Nothing is built |
| `controller-db` | **Unverified** | No artifact exists |

**The install-size numbers in the design documents are wrong in both directions.** `WIREFRAMES.md` section 2 says 4.4 GB with 2.0 GB of textures, written in guillemets as placeholders. `UX_RESEARCH_DOSSIER.md` has an onboarding example reading "47.2 GB installed", which is an order of magnitude out.

**Resolution: install size is a manifest-derived number. The UI computes it from `bytes` and never hardcodes it.** That is the only way it stays correct across releases, and it makes both existing figures moot rather than requiring them to be corrected by hand.

### The cost of the locked decision

**Every change to game content costs a full artifact download: 1,459,978,240 bytes as ISO, or 718,375,792 bytes as RVZ. Rollback of game content costs the same again, because it is a re-download and not an undo.**

A file-level differential would have been worth **about 57 percent** of that, measured against the current development build on August 29, 2026: 30 of 71 files byte-identical at 826,724,672 bytes out of a 1,454,543,718-byte payload, with `chatdata.dat` alone accounting for 704,770,048 of it, unchanged. **That is a real, large, measured saving that the ship-the-image decision forecloses.** It is recorded so nobody rediscovers it and mistakes it for an oversight.

**The percentage is not a constant and must not be quoted as one.** The same comparison against an August 5 snapshot gave 66 percent. Every field import moves it. Section 8 gives the full comparison with both images named, and it has to be re-run against whichever build is finally named as the release candidate.

Container choice recovers a comparable amount without any patch engine, which is why section 9 asks for that decision and not for a reconsideration of the locked one.

### Two things that are still not sized

1. **Textures exist in two mutually incompatible encodings and nobody has decided which platforms get which.** The Windows payload ships 2,763 DDS files at 2,077,362,716 bytes; the Xbox package ships 2,782 PNG files at 1,527,925,456 bytes. All 2,763 DDS stems appear among the 2,782 PNG stems, and the two sets differ by extension, **so no full filename matches between the DDS payload tree and the PNG tree, and neither can repair the other.**

   **One qualification, because the trees on disk are not as clean as that sentence implies.** The *runtime* `GN7E69` tree is not the DDS payload tree: it also holds 19 `.png` files, and all 19 of those filenames **do** appear in the Xbox PNG set (section 2A). They are an applied `Xbox Controls` overlay, not part of the component, but anyone testing the "no overlap" property against a live runtime folder will find 19 matches and should not conclude the statement is wrong. Under the composition model in section 13 the pristine tree has no PNG files in it at all.

   Whether Linux and macOS receive DDS or PNG, and whether the launcher converts, ships both, or picks per platform, is undecided. Until it is, the textures component's real delivery size is unknown for two of four platforms.
2. **Nowhere to host any of it.** The backend split puts the website on SiteGround and real time on Railway. No decision exists on where manifests and multi-gigabyte blobs are served from, whether object storage or a CDN is in the budget, or what 1.46 GB per user per content update costs. **This is the gap that decides whether automatic content updates are affordable at all**, and it is not a technical problem.

---

## Open items

1. **Manifest signing has no key, no custody and no first-run story.** Questionnaire row 18.8 is Open, no signing infrastructure exists anywhere in the project, and row 23.1 records that code signing is not available at all. Section 6 specifies what a signed manifest must contain and cannot say who holds the key. **This blocks shipping automatic updates.**
2. **No decision on where manifests and content are hosted, or what the bandwidth costs.** Questionnaire section 14 leaves host, storage, CDN and budget partly open. Everything in this document assumes an origin that does not exist yet.
3. **Which build is the September release candidate is not recorded anywhere.** Every hash and size here describes the January 11 2026 image. Measured August 29 2026 against the current development build, it differs in **33 of 63 files** (513,767,580 bytes as shipped) and lacks **eight** field containers, `game12.qkl` through `game19.qkl` (35,913,185 bytes), matching the eight field ids in `tools/field_importer/registry.json`. The first manifest cannot be written until Adam names the build, and **per open item 19 the online build has to be named separately.**
4. **ISO or RVZ is undecided**, and it is worth 741,602,448 bytes per offline content download and 741,623,464 per online one. Both RVZ sizes are now measured, so the *value* of the decision is no longer an estimate; only the decision is open. Three cheap tests close it: boot an RVZ by direct path, confirm the Xbox target accepts one, and confirm the music chain still needs a materialized ISO.
5. **Retaining prior disc images is unpriced in every existing document.** `COMPONENT_MAP.md` section 7 says every component keeps one prior version; for the two images that is 2,919,956,480 bytes as ISO or **1,436,730,568 bytes as RVZ** (718,375,792 + 718,354,776, both measured), permanently. Section 11 recommends not retaining them and defining image rollback as a re-download. **Needs Adam's decision.**
6. **Seven components, not eight.** Folding news and configuration into the launcher reverses questionnaire row 18.1, which is marked Decided. **Needs Adam's sign-off.**
7. **Texture encoding per platform is undecided**, DDS at 2,077,362,716 bytes against PNG at 1,527,925,456 bytes with zero filename overlap. Linux and macOS have no answer.
8. **The prompt-pack overlay is unproven**, and it is worth about 2 GB per game ID. Composition is proposed specifically to avoid depending on Dolphin resolving stacked load paths, but even the composed directory as a texture load path has not been tested here.
9. **The "about four minutes" rebuild estimate has no measured source, and neither does the counterweight.** Encode and repack have no timing data at all. Extraction has exactly one unrepeated sample of about 1.9 seconds on one Windows developer machine, with the storage type unrecorded and nothing comparable run on a Steam Deck; that is a data point, not a measurement, and section 15 no longer rests any design on it. The UI should say "a few minutes" until one real rebuild is timed on real hardware, and then use that machine's own number.
10. **Post-update music restoration is proposed, not confirmed.** Section 15 describes re-applying `music/state.json` to the new base automatically. Adam has not been asked whether that should be automatic, prompted, or opt-in.
11. **Per-file repair inside a disc image does not exist.** The FST exposes 63 addressable files so it is conceivable, but nothing in the project writes a file back into an image in place. A mismatched image is a whole re-fetch.
12. **Authorship and licensing for the eleven migrated mods is unknown**, and cannot be invented. Two of them (Patriots Throwback, Saints Alternate) also have no category in the approved list.
13. **Forced-online minimum version enforcement is unconfirmed** (questionnaire row 18.9). It is unknown what the server declares and how the client proves its online image matches, beyond the fact that the image has a stable SHA-256 that could be attested.
14. **Recovery point retention count is unspecified.** Each is about 16 MB. How many are kept, and for how long, is undecided.
15. **Health check probes for `launcher` and `runtime` are unverified.** No headless version query has been tested against this fork, and no startup timeout has been measured.
16. **The Xbox copy has no update path at all**, by decision. The best available mitigation is a version stamp written by the wizard so staleness is at least visible. `XBOX_DEV_MODE_INSTALLER.md` owns that call.
17. **The differential fetch design in section 5A is proposed and unparameterized.** The content-addressed blob store, the `covers` grammar and the client's changed-set arithmetic are new in this document and nothing implements them. `BULK_THRESHOLD` cannot be chosen without an origin's per-request cost, which open item 2 says does not exist. The decision to ship no binary delta format is deliberate and reversible.
18. **Launcher and supervisor self-update is designed and untested.** Section 10 assigns the launcher swap to the supervisor and the supervisor swap to the launcher, and makes the supervisor the thing that performs the launcher health check. Neither binary exists. The Windows locked-executable path, the macOS `.app` directory rename, and the ready-signal handshake have all been reasoned about and none has been run. **The ready timeout is the same unmeasured value as open item 15.**
19. **The shippable online image does not exist yet.** `ONLINE_AND_RESULTS_PROTOCOL.md` open item 2 records that `FR4N31.iso` does not carry the `statread1` hook, so version 1 of `online-content` must be a rebuilt image and `d52a9370...` is not the shipped hash. Section 3A. Related and unasked: whether the online image is rebuilt from the same source tree as the offline one, which decides whether the two share a release version number.
20. **The free-space `HEADROOM` constant is unspecified.** Section 10 requires a precheck before any operation and gives the measured peak (2,178,354,032 bytes on top of the live copy for one offline content update), but the safety margin needs a measurement on a Steam Deck rather than a guess. The same section's same-volume requirement for DATA ROOT has also not been tested against a split internal-plus-SD-card layout.
21. **`online_minimum` precedence is a proposed ruling.** Section 4 makes the backend authoritative and the manifest field a mirror, and fails closed on disagreement. No backend exists to review it against, and **the "backend names a release the client has never heard of" screen state has no written copy** (section 17).
22. **The mods catalog pinning model needs a decision.** Section 5B keeps the SHA-256 pin, which makes the catalog immutable within a release and therefore makes publishing one mod require re-signing a release manifest. The alternative is a separately signed catalog with `catalog_min_version`. This interacts directly with open item 1, because it changes how often whoever holds the key has to use it.
23. **The composition cost in section 13 is not measured.** No hardlink composition has been performed and no timing exists. **exFAT and FAT32 do not support hardlinks at all**, which is the default format of a Steam Deck SD card, so the full-copy fallback of 2,077,362,716 bytes per composed game ID may be the common case on a handheld rather than the exception.
24. **The reduced Inno script has not been written or tested.** Section 16 shows that `rebornsetupv10.iss` contains no `UsePreviousAppDir` and no `UsePreviousData` directive, so Inno's default of `UsePreviousAppDir=yes` would put `{app}` back on the old data root on every existing 2.1 machine. The five directives and the build-time assertion are proposed. **An upgrade over a real 2.1 install is the only thing that proves it and has not been run.**
25. **The `NSRONLINE` rename is a six-file edit that has not been made.** Section 0 lists every occurrence across `PROJECT_QUESTIONNAIRE.md`, `COMPONENT_MAP.md`, `SYSTEM_ARCHITECTURE.md` and `ONLINE_AND_RESULTS_PROTOCOL.md`. A half-completed rename in a signed-manifest system is worse than either end state.
26. **Nothing here is implemented.** This is a proposal awaiting the step 7 decision checkpoint.
