# Xbox Dev Mode Installer

## NFL Street Reborn Launcher

**Date:** August 29, 2026
**Covers:** `CLAUDE_START_HERE.md` step 6, the `XBOX_DEV_MODE_INSTALLER.md` deliverable, and section 5.7 of [`SYSTEM_ARCHITECTURE.md`](SYSTEM_ARCHITECTURE.md)
**Depends on:** [`COMPONENT_INVENTORY.md`](../discovery/COMPONENT_INVENTORY.md) section 13, [`PROJECT_QUESTIONNAIRE.md`](../discovery/PROJECT_QUESTIONNAIRE.md) rows 7.1 to 7.10, [`WIREFRAMES.md`](../design/WIREFRAMES.md) section 16, [`INFORMATION_ARCHITECTURE.md`](../design/INFORMATION_ARCHITECTURE.md), [`CONTROLLER_AND_ACCESSIBILITY_AUDIT.md`](../design/CONTROLLER_AND_ACCESSIBILITY_AUDIT.md)

**This document supersedes [`COMPONENT_MAP.md`](../discovery/COMPONENT_MAP.md) section 9.** That diagram still contains "pair by IP or PIN", "Device Portal, HTTPS to deploy or update UWP package", "reuse retained verified base", and "patch and prepare Xbox game image". All four contradict decisions Adam has since locked. Section 9 of the map needs correcting or deleting; until it is, this document wins.

---

## 1. What this is, and the gap it does not close

Adam's original request, transcript line 525:

> "We need to make an easy Xbox Dev Mode installer, where the user can just plug in their Xbox Dev Mode USB stick and it installs."

The delivered scope, questionnaire row 7.6: **"the installer just moves all the files for the user."** No Device Portal, no app deployment, no certificate.

Those two are not the same thing, and the difference is not a shortcut. The blueprint already records why, at line 1297:

> "Xbox Dev Mode does not expose a normal consumer USB-autorun installation mechanism for sideloaded UWP applications... A USB drive can carry the game and Dolphin data, but plugging it directly into the console cannot by itself perform an unrestricted app-package installation."

**So the honest description of this feature is: it prepares a drive, and the user installs the app on the console themselves.** Every string in the wizard has to say that, because the mode names ("New Xbox Installation") imply otherwise. The wireframe already gets this right: "This copies NFL Street Reborn onto a drive you plug into your Xbox. You install the app on the Xbox yourself."

There is a second reason nothing here can deploy an app: **no app package exists anywhere in the material.** Verified: the entire shipped package contains five file extensions, `exe`, `ini`, `iso`, `png`, `raw`. No `.appx`, no `.msix`, no `.cer`, no `.pfx`, no manifest, no XML, no README. There is nothing to deploy even if a deployment path were in scope.

### Units note, because three different figures are circulating

Earlier documents call this feature "2.9 GB." That figure matches nothing measured, and two genuinely different quantities are being confused. This document uses bytes as the primary unit and decimal GB (10^9) as the secondary, and it names which quantity it means every time.

| Quantity | Bytes | Decimal GB | GiB |
|---|---:|---:|---:|
| The `(1)` package as extracted on disk (`Step 1` plus `Step 2`) | 3,024,024,332 | 3.02 | 2.82 |
| What the wizard writes to the drive (payload plus `NSR.iso`) | 3,013,595,002 | 3.01 | 2.81 |
| The `Step 2 - Transfer to Xbox` payload alone, no disc image | 1,553,616,762 | 1.55 | 1.45 |

All three measured for this document. The number a user sees on the confirm screen is the second one, because that is the one that has to fit on their drive. See section 4.

---

## 2. Component verdicts

Every existing component this feature touches, with exactly one verdict.

| Existing component | Verdict | Reason |
|---|---|---|
| `Step 2 - Transfer to Xbox` **layout and configuration** | **REUSE** | 13 directories and 11 configuration and settings files, measured: `Config\` holds exactly 10 (`Dolphin`, `DSUClient`, `FreeLook`, `GCKeyNew`, `GCPadNew`, `GFX`, `Logger`, `RetroAchievements`, `TimePlayed`, `WiimoteNew`), plus `GameSettings\GN7E69.ini`. Section 4's per-file table is the authority for the count. This is the only description of a working Xbox layout that exists |
| `Step 2\GC\MemoryCardA.USA.raw`, `SRAM.raw` | **REPLACE** | Verified to contain the author's real saves (`GN7E-69Options`, `GN7E-69UserIDJerseyMode`). Shipping them overwrites returning users. The launcher ships a blank formatted card instead |
| `Step 2\Config\TimePlayed.ini` | **REPLACE** | Contains `GN7E69 = 0x0000000000458712`. Verified: `0x458712` decodes to **4,556,562**, someone's recorded play time. The unit is unverified: `TimePlayed.cpp` does not exist in the 2022-era Reborn fork, so this document does not claim milliseconds. The same 43 byte file with the identical value also ships in the older `oldski` package |
| `Step 2\Config\Dolphin.ini` `[Analytics] ID` | **REPLACE** | `d13f3034d7c4d1bc575151c5a3595c43` is distributed identically to every user. Generate per install, or omit the key |
| `Step 2\Config\Logger.ini` | **REPLACE** | Ships `WriteToConsole = True` and `WriteToWindow = True` in a consumer payload |
| `Step 2\Load\Textures\GN7E69\` | **REUSE** | 2,782 PNG files, 1,527,925,456 bytes. Same base names as the PC set, different encoding, see section 4 |
| `Step 2\Cache\GameCovers\GN7E69.png` | **REUSE** | Correctly scoped, one cover for one game |
| `Step 2\NSR.png` at the payload root | **REUSE, relocated** | Byte-identical to the cover, both SHA-256 `9e14f3fe2dbe77cd72462da8547c8d9130e8e3aea0c4b21873ec46978b48425c`, verified. **Correction to an earlier draft, which gave this a RETIRE verdict.** Nothing is retired: the file moves from the payload root to `Games\NSR.png` beside the disc image, and the cache copy at `Cache\GameCovers\GN7E69.png` stays. Both are written, which is why sections 4, 6 and 9.4 all count 2,798 files and 3,013,595,002 bytes. See section 6 for why two copies of one 4,454,524 byte image is the intended outcome and what it would take to drop one |
| `Step 1 - Patch Game File\NFL Street Reborn Patcher.exe` | **RETIRE** | 12 hardcoded messages, sleeps summing to 14.0 seconds, never reads the user's file |
| `Step 1\resources\nsrpatch.iso` | **RETIRE** | Byte-identical to the shipped `NSR.iso`. There is no reason to carry a second copy |
| `rebornpatcherxbox.py` | **RETIRE** | Its only validation, `verify_iso()`, accepts any file between 100,000,000 and 2,000,000,000 bytes |
| Nested duplicate package folder | **RETIRE** by normalization | See section 10 |
| Library and preparation service (`SYSTEM_ARCHITECTURE.md` 5.1) | **REUSE** | Supplies the already-verified `NSR.iso` and its hash. No Xbox-specific image work exists |
| `netplay_uia.ps1`, JSGME, ReShade, `Nfls*` Dolphin sources | **not touched** | None of them run on the Xbox side. See section 3 |
| `rebornsetupv10.iss` backup and uninstall behavior | **REPLACE as precedent** | Lines 127 and 153 pass wildcards to Inno Setup's `FileCopy`, which does not expand them, so the backup has never worked. Line 309 `DelTree`s a folder containing memory cards. The Xbox wizard must not repeat either mistake |

**One verdict changed from `SYSTEM_ARCHITECTURE.md` 5.7.** That section says "Reuse as the payload" without qualification. A literal reuse ships another person's saves, play time, and analytics GUID. The corrected verdict is **reuse the layout and the configuration, regenerate the state.**

---

## 3. What the Xbox build is not

Verified from the shipped `Config\Dolphin.ini`: it sets `FrontendTheme = Flipper 2.2 - Blackout (Performance)`, `LastCarouselCategory` and `LastCarouselGame`. **None of those three keys exists anywhere in the Reborn Dolphin source tree.** `ISOPaths` and `ISOPath0` do exist, as genuine upstream `Main.General` keys.

**INFERRED, not verified: the Xbox side is a third-party Dolphin build plus Reborn data.** The three absent config keys are strong evidence, and no counter-evidence exists, but absence of a key from one source tree does not by itself identify a binary. The honest reading is "almost certainly not the Reborn fork." The document's own open item 2 still classes the build identity as a BLOCKING unknown, and this conclusion cannot be firmer than that item allows.

What follows from it changes what the wizard can promise:

| Reborn feature | On PC | On Xbox |
|---|---|---|
| `NflsStatTracker` stat capture | Yes | **No** |
| `NflsStatUpload` | Being retired, moves to the launcher | **No** |
| `NflsPlayHide` play-call hiding | Yes | **No** |
| `NflsUiSplit` | Experimental | **No** |
| ReShade, per-field presets | Windows only | **No** |

**Correction: "Fair Input Delay" was listed in this table as a Reborn feature and it is not one.** It is stock upstream Dolphin. Verified: `dolphin-exgl\Source\Core\DolphinQt\NetPlay\NetPlayDialog.cpp` line 184 reads `m_fixed_delay_action = m_network_menu->addAction(tr("Fair Input Delay"));`. `ONLINE_AND_RESULTS_PROTOCOL.md` section 1 already flags this exact framing as "False for the selected fork," and this document was repeating the mistake it corrects. There is nothing Reborn about it to lose on Xbox.

What is true about NetPlay on the Xbox side is narrower and is INFERRED: no Reborn NetPlay code runs there, and nothing in the payload configures NetPlay. Whether the third-party Xbox build exposes upstream NetPlay at all is unknown, and it depends on open item 2.

**So the Xbox side is an offline, stat-less install.** The wizard must say so on the first screen, because a user who copies 3.01 GB and then finds their games do not appear in the league has been misled by omission.

**It is not established that it is a couch install either, and the wizard must not say that it is.** Open item 8 records the reason: `[GCPad2]` through `[GCPad4]` in the shipped `Config\GCPadNew.ini` each name the same single device (`WGInput/0/Xbox One Game Controller`) and carry no bindings at all. One controller is configured. Whether the app auto-assigns a second is untested. Until someone tests it on hardware, the first screen claims single player and says local multiplayer is unconfirmed.

---

## 4. The payload, measured

Independently verified for this document with `find` over `C:\Users\Adam\Downloads\NFL Street Reborn Xbox Dev Mode 2.1(1)\NFL Street Reborn Xbox Dev Mode 2.1\Step 2 - Transfer to Xbox`.

| Group | Files | Bytes |
|---|---:|---:|
| `Cache\GameCovers\GN7E69.png` | 1 | 4,454,524 |
| `Config\` (10 files) | 10 | 3,569 |
| `GameSettings\GN7E69.ini` | 1 | 1,405 |
| `GC\` (card plus SRAM) | 2 | 16,777,284 |
| `Load\Textures\GN7E69\` | 2,782 | 1,527,925,456 |
| `NSR.png` at payload root | 1 | 4,454,524 |
| **Payload total** | **2,797** | **1,553,616,762** |
| `NSR.iso` | 1 | 1,459,978,240 |
| **Total written to the drive** | **2,798** | **3,013,595,002** |

**The confirm screen must count the disc image, and "2.9 GB" does not describe anything measured.** The shipped `Step 2` folder is 1,553,616,762 bytes without the image, and the `(1)` package on disk is 3,024,024,332 bytes, so 2.9 GB is neither. What the user needs on the confirm screen is the amount that lands on their drive: **3,013,595,002 bytes, 3.01 GB.** `WIREFRAMES.md` section 16 line 682 has "2.9 GB" inside its invented-data markers; replace it with 3.01 GB, or with the byte figure. See the units note in section 1 for all three quantities side by side.

Exact per-file list for the small files, all verified by size:

```
Config\Dolphin.ini              303
Config\GCPadNew.ini           1,745
Config\GFX.ini                  537
Config\Logger.ini               941
Config\TimePlayed.ini            43
Config\DSUClient.ini              0
Config\FreeLook.ini               0
Config\GCKeyNew.ini               0
Config\RetroAchievements.ini      0
Config\WiimoteNew.ini             0
GameSettings\GN7E69.ini       1,405
GC\MemoryCardA.USA.raw   16,777,216
GC\SRAM.raw                      68
```

Three measured facts that change how the copy engine has to be written:

1. **Five files are zero bytes** (`DSUClient`, `FreeLook`, `GCKeyNew`, `RetroAchievements`, `WiimoteNew`). Hash verification of an empty file is vacuous. Verify by presence and length, and do not report a green check that means nothing.
2. **Four directories are empty**: `Config\GraphicMods`, `GC\EUR`, `GC\JAP`, `GC\USA\Card A`. A copy engine driven by a file list will silently omit all four. Whether the app needs them is unknown, so create them.
3. **`Load\Textures\GN7E69` is flat**, 2,782 files in one directory, largest 42,488,367 bytes (`tex1_256x256_m_3742e44b0cc2dd28_14.png`). **Per-file copy and hash of that one directory is the dominant cost of the entire operation**, not the 1.36 GB disc image. Progress reporting has to be file-count-weighted as well as byte-weighted or it will sit at 48% for several minutes.

### The texture set is not the PC texture set

**Two different PC texture folders exist on this machine and they do not measure the same, so this table names the folder for every column.** An earlier draft gave one PC column without saying which folder it came from, and it came from the wrong one.

| | Xbox payload | PC, selected runtime | PC, shipped older runtime |
|---|---|---|---|
| Folder | `...2.1(1)\...\Step 2 - Transfer to Xbox\Load\Textures\GN7E69` | `...\CREATION LEAGUE AND NEW DOLPHIN\User\Load\Textures\GN7E69` | `...\Files\Reborn Dolphin\User\Load\Textures\GN7E69` |
| Format | 2,782 PNG | 2,763 DDS plus 19 PNG | 2,763 DDS |
| Files | 2,782 | 2,782 | 2,763 |
| Bytes | 1,527,925,456 | 2,079,212,285 | 2,077,362,716 |
| Base names vs the Xbox set | | **0 Xbox-only, 0 PC-only** | 19 Xbox-only, 0 PC-only |

All three columns measured for this document. The middle column is the one that matters: it sits beside the `Dolphin.exe` whose SHA-256 is `617321bf37ac7b2f9bdfcd3af803a019e862591259c770e9e688d53834df1338`, which is the selected runtime that `COMPONENT_INVENTORY.md` section 4 names the production foundation. **The 19 file gap is an artifact of measuring against the runtime that is being replaced, not a real difference between the platforms.** Against the runtime that will actually ship, the two trees carry the same 2,782 base names, and the 19 PNGs are present on both sides.

So the difference between the trees is encoding, not content: PNG on Xbox, DDS plus those same 19 PNGs on PC.

`COMPONENT_INVENTORY.md` section 13 lists `Step 2\Load\Textures\` without noting this, and separately notes the PC folder at about 2.0 GB. A reader could conclude one tree serves both platforms. **The release still carries two texture trees, 1,527,925,456 plus 2,079,212,285 = 3,607,137,741 bytes, 3.61 GB combined, measured against the selected runtime.** Whether the Xbox app can load DDS, which would collapse them into one and save about 1.53 GB, is unknown and worth one experiment (section 13).

---

## 5. The seven modes, reduced to what a file mover can honestly do

The seven come from the blueprint verbatim at line 1316 and are recorded as Decided in questionnaire row 7.2 and repeated in `INFORMATION_ARCHITECTURE.md`. Row 7.6 in the same questionnaire then removes the basis for two of them. This is the resolution.

| # | Blueprint mode | Status | What it actually does |
|---|---|---|---|
| 1 | New Xbox Installation | **Keep** | Empty or non-Reborn drive. Write the full layout, verify, eject |
| 2 | Update Existing Xbox Installation | **Keep** | Reborn layout detected. Back up user state, write changed files only, verify, eject |
| 3 | Repair Xbox USB | **Keep, reduced** | Hash every file the manifest names, rewrite only the mismatches, and apply the stale-file rules in section 9.5 to everything the manifest does not name |
| 4 | Back Up Xbox Save | **Keep** | Copy `GC\` (16,777,284 bytes) to the PC data root, timestamped. Cheap, fast, and the only irreplaceable data on the drive |
| 5 | Restore Xbox Save | **Keep** | Pick a timestamped backup, write `GC\` back. Its own confirm, because it overwrites a card |
| 6 | Deploy or Reinstall Xbox App | **RETIRE** | No package, no certificate, no Device Portal, and none in scope. Keeping the card would be a lie on the home screen |
| 7 | Test Xbox Connection | **RETIRE** | There is no connection. The wizard never speaks to a console |

**Correction to an earlier draft of mode 3.** It said Repair "degrades to a full rewrite against a pre-launcher 2.1 drive, because no baseline hashes exist to compare against." That reason is wrong. The baseline is the manifest the launcher ships with itself (section 9.1), not anything recorded on the drive, so a hand-prepared 2.1 drive is hashed file by file against that manifest exactly like any other drive, and only genuine mismatches are rewritten. Repair degrades toward a full rewrite when the content on the drive really does differ, which is the expected case for the `oldski` generation and for a drive whose textures were never copied. Provenance is irrelevant; content is what is compared.

**Five cards, not seven.** `INFORMATION_ARCHITECTURE.md` line 68 and line 234 both say seven and both need updating.

The gap left by mode 6 is real and should be filled with content rather than left blank: a **"How to install the app on your Xbox"** card that opens instructions, not automation. It is the last step of every successful copy anyway (section 12), so it belongs on the mode screen too.

---

## 6. Destination layout

```text
<usb root>
  Games\
    NSR.iso                 1,459,978,240   from library/offline, hash-verified
    NSR.png                     4,454,524   cover, paired by filename
  <user root>\
    Cache\GameCovers\GN7E69.png 4,454,524   same bytes as Games\NSR.png, on purpose
    Config\                         3,569   10 files, 5 of them empty
    Config\GraphicMods\                     empty, created explicitly
    GameSettings\GN7E69.ini         1,405
    GC\MemoryCardA.USA.raw     16,777,216   preserved if present, see section 8
    GC\SRAM.raw                        68   preserved if present
    GC\EUR\  GC\JAP\  GC\USA\Card A\        empty, created explicitly
    Load\Textures\GN7E69\   1,527,925,456   2,782 PNG files, flat
```

**`Games\` is well supported and `<user root>` is not.**

The `Games\` half rests on two pieces of evidence. `Config\Dolphin.ini` contains `ISOPath0 = E:\Games` and `ISOPaths = 1`, read out and confirmed for this document. And the disc-image-plus-cover pairing by filename is visible in **two** Games folders on this machine, carrying three paired names between them. An earlier draft said three folders and then cited two; the corrected reading:

| Games folder | Contents, measured |
|---|---|
| `Files\Reborn Dolphin\Games` (shipped older runtime) | `NSR.iso` 1,459,978,240 paired with `NSR.png` 4,454,524 |
| `...\CREATION LEAGUE AND NEW DOLPHIN\Games` (selected runtime) | `NSR.iso` / `NSR.png`, `CR4E12.iso` / `CR4E12.png` 92,672, `FR4N31.iso` / `FR4N31.png` 88,701, **and `TOTALLY NOT MALWARE.iso` 1,459,978,240 with no paired PNG** |

So `NSR.png` belongs beside the disc image, not at the drive root where the current package leaves it.

**The pairing is a convention, not an invariant, and the counterexample is in the same folder as the evidence.** `TOTALLY NOT MALWARE.iso` has no cover and sits alongside three images that do. What the app does with an unpaired image is not established here, and this document does not claim it. The consequence for the wizard is narrow but worth writing down: write the cover, and **do not treat a missing cover on an existing drive as corruption or as a reason to fail Repair.** It is a cosmetic file.

**Why the same 4,454,524 byte image is written twice.** `Games\NSR.png` serves the filename pairing above. `Cache\GameCovers\GN7E69.png` is keyed on the game ID, not the filename, and is scoped to the app's cover cache. They are two consumers with two naming rules, so both copies are written and both are counted in the 2,798 file / 3,013,595,002 byte totals. **Whether the Xbox app actually reads both is unverified** and cannot be settled without open item 2. Writing 4.4 MB twice costs 0.15% of the transfer, which is cheaper than guessing wrong, so the duplication stays until someone with a console can drop one. If one is later confirmed redundant, it is a profile edit (remove one `entries` row), not a code change.

**`<user root>` is a blocking unknown.** No README, no manifest, no instruction file exists in the package. The only clues are the folder name "Step 2 - Transfer to Xbox" and the patcher's dialog: "Next step: Transfer NSR.iso to your Xbox Dev Mode games folder." Nothing states where the Xbox Dolphin app reads its User directory from.

The package is also internally inconsistent about this: `Dolphin.ini` points at `E:\Games`, the payload contains no `Games` directory at all, and the patcher writes `NSR.iso` to the payload root next to `NSR.png`. Creating `Games\` and moving two files into it is manual work the current package never does and never documents.

**Design response: the destination map is data, not code.**

```json
{
  "profile": "xbox-devmode-flipper-2.2",
  "confirmed_on_hardware": false,
  "game_dir": "Games",
  "user_dir": ".",
  "entries": [
    { "src": "config/Dolphin.ini", "dst": "Config/Dolphin.ini", "sha256": "...", "size": 303 },
    { "src": "textures/GN7E69/...", "dst": "Load/Textures/GN7E69/...", "sha256": "...", "size": 0 }
  ],
  "empty_dirs": ["Config/GraphicMods", "GC/EUR", "GC/JAP", "GC/USA/Card A"]
}
```

Ship one profile with `confirmed_on_hardware: false`, surface that honestly in the UI ("This layout has not been tested on a console"), and let a second profile be added without a launcher release when someone with an Xbox confirms the right answer. **A wrong constant compiled into the copy engine is a re-release; a wrong value in a profile file is an edit.**

---

## 7. Drive enumeration, the picker, and refusal

The twelve USB safety requirements are verbatim in the blueprint at lines 1326 to 1338. This section is how each is met.

### Enumeration

Per platform, because there is no portable answer:

| Platform | Source of truth | Notes |
|---|---|---|
| Windows | `SetupDiGetClassDevs` plus `IOCTL_STORAGE_QUERY_PROPERTY` for `BusType` and `RemovableMedia`, volume GUID paths for identity | Do not use `GetDriveType` alone. It reports `DRIVE_FIXED` for many USB SSDs and external enclosures |
| Linux | `/sys/block/*/removable`, `ID_BUS=usb` from udev, mountpoints from `/proc/self/mountinfo` | |
| macOS | `DADiskCopyDescription`, `DAMediaRemovable` and `DADeviceInternal` | |

Whether the wizard exists on Linux and macOS at all is undecided (section 13). All three rows are written down so that the answer is a scoping decision rather than a rewrite.

### What the picker shows

Per the blueprint: label, capacity, filesystem, free space, and physical identity. The wireframe already draws this. Physical identity means the volume GUID or device path, not the drive letter, because the letter is the one attribute guaranteed not to be stable.

### The selection rule

**Nothing is preselected when more than one removable drive is present.** The controller audit already specifies this: "Drive picker focuses nothing preselected. The first drive is focused but not selected." The confirm screen focuses `Back`, not `Copy to drive`.

This document tightens it in one place: **preselect nothing, ever, even when exactly one removable drive is present.** The blueprint's rule is conditional on there being more than one, but the extra A-press costs nothing and the rule becomes something a reviewer can check by inspection rather than by simulating drive counts. A single-drive case still means one deliberate selection.

### Refusal

| Volume | Behavior |
|---|---|
| System or boot volume | **Never listed.** Shown once as a grayed line explaining why, as the wireframe does with `G: (system)` |
| Any volume hosting the OS, the launcher install root, or the launcher data root | **Never listed** |
| Non-removable fixed disk | **Never listed by default.** An "Show all drives" toggle may exist behind Settings, off by default, and it must still refuse the three cases above |
| Network or mapped drive | Not listed. Copying 3 GB to a share and calling it an Xbox drive is a support ticket |
| Optical or read-only volume | Listed and disabled, with the reason |
| Removable, writable, not a system volume | Listed and selectable |

**Refusal is by volume role, not by size.** A 2 TB external drive is a legitimate target. A 128 GB internal NVMe is not.

---

## 8. Preservation, backup, and the "will not format" contract

### The contract, stated as a promise the code keeps

```
The Xbox wizard never formats a drive.
The Xbox wizard never deletes a file it did not write, unless the user
  confirms that exact file by name on a screen that lists it.
The Xbox wizard never overwrites GC\ without a separate confirmation.
The Xbox wizard never moves or removes a directory it did not create,
  unless the user confirms that specific restructuring by name.
```

**Lines 2 and 4 carry an explicit user-confirmed exception, and that is a change from an earlier draft that stated line 2 absolutely.** The absolute form was contradicted twice inside this document. Section 10's detection table wants to restructure a hand-copied `Step 2 - Transfer to Xbox\` folder sitting at the drive root, which means moving a directory tree the wizard did not create. Section 9.5's stale-file policy wants to remove content a previous release wrote and the current manifest no longer names. Neither is possible under an absolute rule, and both are things a user reasonably wants.

The resolution is not to weaken the promise, it is to state it correctly:

- **Silent deletion never happens.** Not for stale files, not for restructuring, not for anything.
- **Anything the wizard did not write is only ever touched from a screen that names it,** shows the count and the byte total, and defaults to the non-destructive choice.
- **`GC\` is exempt from every one of these paths.** It is preserved first and is never inside a restructuring or stale-file set. See the GC rules below.

The wireframe already prints "Will not format or erase the drive" on the confirm screen. That line is only true if there is no format path anywhere in the flow, so there is not one.

**This contradicts `INFORMATION_ARCHITECTURE.md` line 112**, which lists "format a USB drive" among the five actions using the 800 ms hold-to-confirm. The blueprint's own wording is "prefer not to format at all; explain the required filesystem and let the user opt into a guided format only if absolutely necessary."

**Resolution: no format path in v1, and the IA list is wrong.** Not because formatting is philosophically bad, but because of a concrete blocker: **nobody knows what filesystem the Xbox requires.** No document, config file, or README in the material states it. A format feature would have to pick a target filesystem, and picking one by guess is exactly the kind of confident wrongness that destroys a user's drive. If and when the filesystem question is answered, a guided format can be added as its own mode, off the main flow, behind hold-to-confirm and a re-selection of the drive.

Note for accuracy: **FAT32's 4 GiB per-file limit does not block this payload.** The largest file is `NSR.iso` at 1,459,978,240 bytes, 1.36 GiB. Do not tell the user FAT32 is impossible on file-size grounds. Whether the console accepts FAT32 is a separate and unanswered question.

### Filesystem precondition at write time

Not formatting is not the same as not caring. Open item 3 records that **nobody knows which filesystem the console requires**, and section 7 already specifies that the picker displays the filesystem it detected. Neither of those states what the wizard does when it is about to write. This does.

**The filesystem is read before the first byte is written, it is recorded in the journal, and it changes three behaviors.** It never blocks a copy on its own, because refusing a drive on a guess is exactly the confident wrongness open item 3 warns against.

| Detected filesystem | Behavior |
|---|---|
| exFAT | Proceed. Note on the confirm screen: "exFAT. This is what the sample drives in the design material use, but it is not confirmed against a console" |
| NTFS | Proceed with a caution line: "NTFS. Some consoles and embedded readers do not mount NTFS. If the Xbox does not see the drive, exFAT is the next thing to try." The wizard states this as a possibility, not as a diagnosis |
| FAT32 | Proceed with a caution line, and state the real constraint rather than the myth: the payload fits, since the largest file is 1.36 GiB against a 4 GiB limit, but 2,782 files in one flat directory is heavy for FAT32 and console support is unconfirmed |
| Anything else, or undetectable (ext4, APFS, HFS+, unknown) | **Refuse to write, and say why.** "This drive is formatted `<name>`. NFL Street Reborn cannot tell whether an Xbox can read it, and copying 3.01 GB to find out wastes your time." Offer Back, not Continue anyway |
| Read-only mount | Refuse. Already covered by section 7's refusal table |

**The staged-rename guarantee in section 9.2 is also conditioned on this reading.** Rename-replace behavior is not uniform across filesystems, so the wizard records what it detected and downgrades its own promise, in the UI and in the verify report, when the target is not one it can reason about. Section 9.2 states the condition; this is where the value comes from.

**Open item, and it is the same one as before.** The correct entry in the table above is unknown for every row until someone runs a console. This section decides what to do in the absence of that answer, which is a different thing from answering it.

### GC preservation

Verified: the shipped `MemoryCardA.USA.raw` is a real 16 MB card. Its directory block at offset `0x2000` holds two entries with game code `GN7E69`, `GN7E-69Options` and `GN7E-69UserIDJerseyMode`, read out for this document. Verified further: **both packages sampled ship the identical card**, SHA-256 `cbc27adb86d70bcd96226d55f0e8b14f38a40aae904979ceadef2b62b48065ef`, in `Downloads\oldski\NFL Street Reborn Xbox Dev Mode` and in `Downloads\NFL Street Reborn Xbox Dev Mode 2.1(1)`. The two `SRAM.raw` files match each other too, both `60ee4eb2c90c75b18155d221e4d198742e9873d6b36a3e3e97a6e362d3621a11`.

**That is two samples, and it supports exactly one claim: both packages sampled carry the same author card.** An earlier draft went further and said "every release since then has offered the same stale author card, and every blind overwrite has wiped the user's progress." Neither half is supported.

- Only two packages exist in the workspace to sample. "Every release" describes releases nobody here has seen.
- **Nothing in the material performs an overwrite of a user's card at all.** Verified: `C:\Users\Adam\Downloads\Reborn Patcher Dev Mode\rebornpatcherxbox.py` writes exactly one file, `Step 2 - Transfer to Xbox\NSR.iso`, and it prompts before overwriting even that. `GC\` reaches a drive because the user copies it there by hand. Whether any user has lost a card is unrecorded, and stating it as history was an inference dressed as evidence.

The risk is real without the invented history: **a payload that contains a stale card, copied by hand onto a drive that already has one, silently replaces it.** That is a plausible failure mode, and it is enough to justify everything below. It does not need a body count.

**Which PC card, because there are two.** The PC-side comparison depends on which runtime is meant, the same trap as the texture trees in section 4. Measured for this document:

| Card | SHA-256 | `SRAM.raw` |
|---|---|---|
| Xbox payload, both packages | `cbc27adb...65ef` | `60ee4eb2...1a11` |
| PC, selected runtime (`CREATION LEAGUE AND NEW DOLPHIN\User\GC`) | `0f61c5b03251ae8959111bfa50d5ce3f8a29bd49791751466c8c36156c83d36d` | `fc04d361...0a1f` |
| PC, shipped older runtime (`Files\Reborn Dolphin\User\GC`) | `a11d44cf8af46ca42723239792fb3689727450f78073339b1bffe59bdcf7c966` | `6d9569a3...c4c9` |

All three cards are 16,777,216 bytes and all differ. An earlier draft quoted only the third row and called it "the PC card." **PC and Xbox save state are not one thing, the two PC runtimes are not one thing either, and the wizard syncs none of them.**

Rules:

| Situation | Behavior |
|---|---|
| `GC\MemoryCardA.USA.raw` exists on the drive **and passes the validity check below** | **Keep it. Never write over it.** Report "your existing Xbox saves (16 MB) will be kept" |
| Exists but **fails** the validity check | **Never delete it and never silently replace it.** Rename it to `MemoryCardA.USA.raw.unreadable-<ISO timestamp>`, copy it to the PC backup folder, write a blank card, and say so plainly on screen |
| Not present | Write a **blank formatted 16,777,216 byte card** generated by the launcher, not the author's |
| `SRAM.raw` exists | Keep it. 68 bytes, and length is the only check worth making |
| User explicitly chooses Restore Xbox Save | Back up the current card first, then write, on its own confirm screen |

### The card is checked before it is trusted

**An earlier draft preserved any file at that path unconditionally, and that is the one file with no recovery path.** A zero-byte, truncated, or corrupt card that is preserved forever can never be repaired by any mode in section 5: Repair does not touch it, Update does not touch it, and Restore only helps if a backup exists, which on a first run it does not. Unconditional preservation turns a broken card into a permanent one.

The check is cheap and uses structure this document already reads:

1. **Length is exactly 16,777,216 bytes.** Anything else, including 0, is a fail.
2. **The directory block at `0x2000` is readable** and its entries parse. An all-`0xFF` or all-`0x00` block is the signature of a card that was never formatted.
3. **At least one entry, or a validly empty directory.** A card with no `GN7E69` entries is a pass, not a fail: a user who has an Xbox card from another game, or a freshly formatted one, has a legitimate card and must not have it renamed out from under them.

**Steps 1 and 2 are fail conditions. Step 3 is informational only** and feeds the on-screen message ("your existing Xbox saves" versus "your existing Xbox memory card, no NFL Street saves on it yet"), never the decision.

**What this check is not.** It is not a checksum validation of GameCube memory card block structure, and it does not attempt repair. It distinguishes "a card" from "not a card." Anything stronger needs the app identity from open item 2, since a card the wizard rejects and a card the console rejects are not guaranteed to be the same set.

### Backup

Timestamped backup before Update or Repair, as the blueprint requires. Two questions the material never answered: what to back up, and where to put it.

**What: the user state only.** `GC\` is 16,777,284 bytes and is the only thing on the drive a user cannot regenerate. The disc image, textures, configs, and cover art are all reproducible from the launcher in minutes. Backing up 3 GB to protect 16 MB is theater.

**Where: the PC data root**, `<data root>/backups/xbox/<ISO timestamp>/`, not the USB.

`WIREFRAMES.md` currently shows `E:\Reborn_backup_...` on the drive itself. Verified consequence: an on-USB full backup needs up to another 3,013,595,002 bytes, roughly doubling the free-space requirement to about 6.03 GB on the device most likely to be short of space. Backing up to the PC also survives the drive failing, which is the actual disaster case.

If the PC data root is unavailable or full, fall back to the USB, say so, and let the user decline.

Existing config files that the user may have hand-edited on the drive are also copied into the backup folder, since they are small and the wizard is about to replace them.

### Free space preconditions

| Case | Requirement |
|---|---|
| New install | 3,013,595,002 bytes plus a small margin for the journal and temporary names |
| Update, staging one file at a time | Existing install plus the largest single staged file. Peak 3,013,595,002 + 1,459,978,240 = **4,473,573,242 bytes** while the image is being replaced |
| Save backup to PC | 16,777,284 bytes in the data root |
| Backup forced onto the USB | Add up to 3,013,595,002 bytes, roughly 6.03 GB total |

**Check free space before writing a single byte, and check it against the peak, not the total.** The failure the check exists to prevent is a drive that fills at file 2,600 of 2,798.

---

## 9. The copy engine

Four properties, in the order they matter.

### 9.1 The launcher authors its own manifest

Verified: no checksum, hash list, or manifest file exists anywhere in the package. There is nothing to inherit. **The launcher generates and ships a manifest with the payload**, and it is the same signed-manifest mechanism `CONTENT_AND_UPDATE_MODEL.md` already needs for PC content. The Xbox wizard is a consumer of that mechanism, not a new one.

Without it there is no Repair, no interrupted-copy recovery, and no honest verification, so this is not optional.

### 9.2 Staged writes, then verify, then rename

The blueprint's rule is "stage large files and verify hashes before replacing a working image." Applied per file:

```
for each entry in the profile:
    if target exists and sha256(target) == entry.sha256:  skip, count as done
    write to <target>.rbn-part
    fsync
    sha256(<target>.rbn-part) == entry.sha256 ?  no  -> retry once, then fail the file
    rename <target>.rbn-part -> <target>        (see the atomicity caveat below)
    record in the journal
```

The skip-if-hash-matches step is what makes Update cheap: an update that changes 40 textures copies 40 files, not 2,798.

**PROPOSED, not guaranteed: the crash-safety property.** An earlier draft asserted it flatly, that "a power cut therefore leaves a `.rbn-part` file and an intact previous version, never a half-written `NSR.iso` that looks complete." **That is a guarantee about a filesystem this document does not know.** Open item 3 records that the required filesystem is unstated anywhere in the material, and both halves of the claim are filesystem-dependent:

- **Rename-replace atomicity** is a property of the target filesystem's metadata handling, not of the calling code. It is a reasonable expectation on NTFS and on journaled Linux filesystems. On FAT32 and exFAT, which have no metadata journal, a rename is not guaranteed to survive a power cut as an all-or-nothing operation. Those are the two filesystems most likely to be on a console drive.
- **`fsync` durability on removable media** additionally depends on the device honoring cache-flush commands, which cheap USB controllers are known not to always do. The launcher cannot verify that from userspace.

So the property is stated as a design intent conditioned on section 8's filesystem reading:

| Detected filesystem | What the wizard promises |
|---|---|
| NTFS | Staged rename is expected to be atomic. UI may say the copy is safe to interrupt |
| exFAT, FAT32 | Staged writes still prevent a truncated file from *replacing* a good one under normal completion, but **no crash-safety claim is made.** UI says "do not unplug the drive during the copy," and Resume relies on re-hashing rather than on trusting the journal |
| Unknown or refused | Does not reach this code path, per section 8 |

**The engine does not change per filesystem. Only the promise does.** Staging, fsync, verify, and rename are always performed, because they cost nothing extra and they are correct on the filesystems where they help. What is conditional is what the UI and the verify report are allowed to assert, and whether Resume trusts a journal entry or re-hashes the file.

**This is the one place in the document where a section 13 unknown reaches into the copy engine's stated behavior.** Closing open item 3 is what turns this table into one row.

Cost note: verifying an unchanged install means hashing 3 GB from removable media, which is slow. Cache the last verified state in the journal keyed on drive identity, size and mtime, and offer full re-hash as the explicit Repair mode rather than doing it on every Update.

### 9.3 The journal lives on the PC, not the drive

`<data root>/xbox/journal-<volume-identity>.json`. Records the profile version, the target volume's physical identity, and per-entry state (`pending`, `staged`, `verified`).

**On the PC because the failure being recovered from is often the drive being yanked.** A journal on the drive is unreadable in exactly the case it is needed. Keying it on volume identity rather than drive letter is what lets the user resume after the drive comes back as `H:` instead of `E:`.

Resume behavior on relaunch: detect the journal, detect leftover `.rbn-part` files, offer "Resume copying to XBOX-USB" with the remaining file count and byte total. Declining deletes the `.rbn-part` files and nothing else. `.rbn-part` files are the one class the wizard deletes without a per-file confirmation, because it wrote every one of them itself, they are named with a reserved suffix no other tool produces, and the decline screen states the count.

**A journal is evidence, not authority, and it is discarded whenever it stops agreeing with the drive.** The brief lists interrupted-copy recovery as required, and the case that actually happens is not a clean resume: it is a resume attempted across a launcher update, or against a drive that was edited in between. An earlier draft specified the happy path only. These are the rest:

| Situation | Behavior |
|---|---|
| Journal's profile version or manifest ID differs from the launcher's current one | **Discard the journal, do not resume.** Fall back to a full manifest hash pass, which is Repair. Say "this drive was last written by an older version of the launcher, so it will be re-checked." Resuming a copy against a manifest that has changed underneath it is how a half-old, half-new install gets made |
| Journal's schema version is unrecognized (newer launcher wrote it, user rolled back) | Discard it. Never attempt to parse a schema from the future |
| Two or more journals match the same volume identity | **Trust none of them.** Keep the files for diagnosis, do a full hash pass, then write one fresh journal. This is reachable when a drive is reformatted and reuses a GUID, or when a volume identity is not as unique as assumed on some platform |
| A `.rbn-part` file exists with no journal entry naming it | Delete it after listing it, on the same decline screen as above. It is by definition the wizard's own temporary file, and with no journal entry there is nothing it could resume into |
| A journal entry marks a file `verified` but the file is absent, or its size or mtime differs | The journal is stale. Drop the cached state for that entry and re-hash. The journal never overrides what the filesystem reports |
| Journal exists, volume identity matches, profile and schema match, all entries consistent | Resume, which is the only case an earlier draft covered |
| Journal file is unreadable or is not valid JSON | Discard silently and do a full pass. A corrupt journal must not be able to fail the wizard |

**The rule underneath all seven rows: the journal is a cache that can only ever save work, never authorize it.** Every discard path degrades to a full hash pass, which is slow and correct, and never to a full rewrite, which is fast and wrong.

### 9.4 Verification reports what it actually checked

Verified: five payload files are zero bytes. A hash of an empty file matches trivially and proves nothing. The verify report separates them, and it reports the drive as well as the manifest:

```
2,798 files written                                 <- measured, section 4
2,793 verified by SHA-256                           <- measured
    5 zero-byte config files verified by presence and length   <- measured
    4 empty directories created                     <- measured
    0 mismatches

  117 files on the drive are not in this release   (see below)   <- ILLUSTRATIVE
      112 recognized as older Reborn content       1.1 GB        <- ILLUSTRATIVE
        5 unrecognized, left alone                                <- ILLUSTRATIVE
  exFAT: no crash-safety claim made for this copy  (section 9.2)
```

**The first five lines are measured figures from section 4. The three stale-file numbers are illustrative shape, not data**, because they depend on what is on a particular user's drive and no such drive has been examined. The annotations are there so a reader does not carry 117 forward as a fact the way the 19-texture figure was carried forward in open item 11.

**A report that counts only what the manifest names can print "0 mismatches" on a drive that is still wrong.** The stale-file line is what makes the zero mean something. Section 9.5 is where those files come from.

Honest is more useful than green.

### 9.5 Stale and orphan files, the case the manifest cannot see

**An earlier draft's engine only wrote files and skipped files. It had no rule at all for a file that is on the drive and not in the manifest,** and three of this document's own requirements quietly depend on one:

- **Repair could never converge.** It would rewrite the mismatches, report success, and leave the thing that was actually broken untouched.
- **Update could never shrink anything.** A release that drops 200 textures would leave all 200 on the drive forever, and the drive would grow monotonically across every update for the life of the project.
- **The section 9.4 report was misleading by construction.** "0 mismatches" over a manifest is not "this drive matches this release."

This matters concretely here, not hypothetically: section 4 records the Xbox and PC texture trees drifting by 19 files against one runtime and 0 against another, and section 10 records an `oldski` generation on disk whose texture set is 2,759 files against 2.1's 2,782. **A drive updated from `oldski` to 2.1 will have orphans.** That is the normal case, not the edge case.

**The policy, and it is compatible with the section 8 contract because nothing here deletes silently.**

Every file found on the drive under a directory the profile owns is placed in one of four classes:

| Class | Definition | Behavior |
|---|---|---|
| **Current** | In the manifest, hash matches | Skip. Counted as verified |
| **Mismatched** | In the manifest, hash differs | Rewrite per section 9.2 |
| **Stale** | Not in the current manifest, but named by a **previous** manifest the launcher shipped, or matching a known previous-release hash | **Offer removal, listed, with a count and byte total. Default is remove**, because it is content the launcher itself put there. Never removed without the user seeing the screen |
| **Foreign** | Not in any manifest the launcher knows | **Leave alone. Always.** List it in the report as "not ours, left in place." The user put it there, or another tool did |

**`GC\` is outside this classification entirely.** It is not a profile-owned directory for stale-file purposes, so a memory card can never be classed Stale or Foreign and can never appear on a removal screen. Section 8's rules are the only thing that touches it.

**The launcher ships its previous manifests, and that is what makes the Stale class possible.** A manifest history is small (file lists and hashes, no content) and it is the only way to distinguish "content we shipped and have since dropped" from "content the user added." Without it, everything unknown is Foreign, the wizard can never clean anything, and the drive grows forever. This is a requirement on the manifest mechanism in `CONTENT_AND_UPDATE_MODEL.md`, not a new mechanism: **manifests must be retained and shipped, not just the current one.**

**Where the history is missing, the safe answer is Foreign.** A drive prepared by hand from a 2.1 ZIP predates the launcher and no launcher manifest ever described it, so on first contact its files classify Foreign and nothing is removed. That is the correct outcome for a first run, and it is the honest version of what an earlier draft called "degrades to a full rewrite." The drive converges on the second update, once the launcher's own manifest describes it.

**Removal is by file, never by directory.** The wizard deletes listed files and then removes directories only if they became empty and the profile created them. It never removes a directory tree, which is the mistake `rebornsetupv10.iss` line 309 makes with `DelTree` over a folder containing memory cards.

---

## 10. Normalizing package layouts

Verified state of the two copies on disk:

| Folder | Files | Bytes | Shape |
|---|---:|---:|---|
| `Downloads\NFL Street Reborn Xbox Dev Mode 2.1` | 5,598 | 6,048,048,664 | `Step 1` and `Step 2` at the root **and** a nested `NFL Street Reborn Xbox Dev Mode 2.1\` containing them again. Exactly double |
| `Downloads\NFL Street Reborn Xbox Dev Mode 2.1(1)\NFL Street Reborn Xbox Dev Mode 2.1` | 2,799 | 3,024,024,332 | One wrapper folder, no duplicate |

Both came from the same 2,256,214,980 byte ZIP. The difference is how each was extracted.

**`COMPONENT_INVENTORY.md` section 13 and `PROJECT_QUESTIONNAIRE.md` row 7.1 name different folders as authoritative, and both statements are individually true of the folder each names.** This document names the `(1)` copy as authoritative for the payload, because it is the clean single-wrapper extraction and it is the one measured throughout this document. The inventory's note that duplication is real is also correct, about the other folder.

Once the launcher owns the payload, none of this matters for the shipped product: the wizard writes from its own verified content set, not from a folder a user extracted. Normalization matters for two remaining cases:

1. **Importing an existing package**, if the wizard ever offers that. Detect a nested folder whose name equals its parent's and whose contents are a superset match, and use the inner one.
2. **Reading a drive a user prepared by hand**, which is the realistic Repair case. A hand-copied drive can have `Step 2 - Transfer to Xbox\` sitting at the root instead of its contents, or the whole wrapper folder copied wholesale.

### The oldski generation, re-measured

**An earlier draft described this folder from memory and got every substantive claim wrong.** It said `Downloads\oldski\NFL Street Reborn Xbox Dev Mode` had an empty `Cache\GameCovers`, an empty `Load\Textures`, and no `GC\USA\Card A`. All three are false, and the detection rule built on them ("oldski shape, no textures") would never have matched a real oldski drive. Re-measured for this document:

| Property | oldski | 2.1 `(1)` |
|---|---:|---:|
| Whole package, files / bytes | 2,776 / 2,968,426,197 | 2,799 / 3,024,024,332 |
| `Cache\GameCovers\GN7E69.png` | **present, 4,454,524** | present, 4,454,524 |
| `Load\Textures\GN7E69\`, files / bytes | **2,759 / 1,472,327,351** | 2,782 / 1,527,925,456 |
| `GC\USA\Card A\` | **present, empty** | present, empty |
| `GC\MemoryCardA.USA.raw` | 16,777,216, `cbc27adb...` | 16,777,216, `cbc27adb...` (identical) |
| `GameSettings\GN7E69.ini` | 1,405 | 1,405 |
| `NSR.png` at payload root | 4,454,524 | 4,454,524 |
| `Config\GFX.ini` | 507 | 537 |
| `Config\Dolphin.ini`, `Config\GCPadNew.ini` | 303, 1,745 | **byte-identical**, both hashes match |
| `Config\TimePlayed.ini` | 43, same `0x458712` value | 43, same value |

**The two generations are nearly the same package.** The real difference is 23 textures and one line of `GFX.ini`, not the presence or absence of whole subsystems. The one-line `GFX.ini` difference is `PrefetchCustomTexture = True`, verified by diff, present in 2.1 and absent in oldski. That part of the earlier draft was correct.

**Consequence for detection: "no textures" is not a signal, and the size difference is 3.7%.** A detector cannot separate these two generations by shape at all. It separates them by content, which is what section 9.5's manifest classification already does. Shape detection answers a narrower question: is this a launcher-written layout, a hand-copied one, or an empty drive.

**Detection must handle at least these three shapes**: 2.1 clean, 2.1 nested, and oldski, and the first and third are the same shape.

| Detected shape | Wizard reports | Does it need section 8's restructuring confirmation? |
|---|---|---|
| Profile layout present, manifest matches | Reborn install, up to date | No |
| Profile layout present, manifest differs | Reborn install, update available. Stale files per 9.5 | Only if 9.5 lists stale files |
| Profile layout present, `GFX.ini` is 507 bytes and the texture count is 2,759 | **oldski-generation content**, update available. Reported as content, not as a different shape | Same as above |
| `Step 2 - Transfer to Xbox\` folder at the drive root | Hand-copied install, needs restructuring. Preserve `GC\` from inside it | **Yes.** See below |
| Wrapper folder `NFL Street Reborn Xbox Dev Mode 2.1\` copied wholesale to the root | Same as above, one level deeper | **Yes** |
| No recognizable layout | New install | No |

### Restructuring is a confirmed action, not an automatic one

**Rows 4 and 5 move a directory tree the wizard did not create, which the section 8 contract forbids without confirmation.** An earlier draft asserted both the absolute contract and this table and never reconciled them. The reconciliation:

The wizard **never** silently relocates a hand-copied `Step 2 - Transfer to Xbox\`. It presents a screen that names the folder, states exactly what will move and where, gives the file count and byte total, and defaults to the safe option.

```
This drive has a hand-copied install.

  E:\Step 2 - Transfer to Xbox\     2,797 files, 1.55 GB

The Xbox app expects these files at the drive root, not inside a
folder. NFL Street Reborn can move them for you.

Your memory card is moved first and is never overwritten.
Nothing is deleted.

  [ Move them for me ]   [ Leave it and write alongside ]   [ Back ]
```

Three properties of that screen make it consistent with the contract:

1. **`GC\` moves first, before anything else, and is verified at its destination before the wizard proceeds.** If the move fails, the operation stops with the card intact at its original path.
2. **Move, never copy-then-delete-the-original,** where the filesystem supports a same-volume rename. Where it does not, copy and verify the whole tree first, then remove sources file by file, and never remove a source whose copy did not verify.
3. **"Leave it and write alongside" is a real option, not a dead end.** The wizard writes the correct layout at the root and leaves the old folder untouched. The drive holds two copies and wastes 1.55 GB, which is the user's call to make. Section 9.5 will offer the leftover folder as Stale on a later run, once a manifest describes it.

---

## 11. Drive letters, and the one rewrite the wizard must never make

`Config\Dolphin.ini` ships `ISOPath0 = E:\Games`. Verified present, verified to be an Xbox-side absolute path.

**The wizard writes that value verbatim and never substitutes the letter the drive has on the PC.** This is the highest-value single rule in the document, because the bug it prevents is both easy to write and completely invisible on the PC: a wizard that "helpfully" rewrites `ISOPath0` to `F:\Games` because the drive mounted as `F:` produces a config that is correct on the machine that made it and broken on the console it was made for.

Corollary rules:

- The journal, the drive picker, and every persisted reference key on volume identity (GUID, UUID, or device path), never on the letter.
- Every path the wizard writes into a config file is Xbox-side. Every path it uses to copy is PC-side. They are different namespaces that happen to look alike, and the code should keep them in different types so they cannot be mixed by accident.
- If the profile ever gains a configurable game path, the UI must label it "the path as the Xbox sees it," not "drive."

**Whether an Xbox in Dev Mode actually assigns `E:` to a USB drive is UNVERIFIED.** Nothing in the workspace addresses it. `E:\Games` is a verified shipped value, not a verified working one. This is an open item, not a fact.

---

## 12. Finishing: eject, and what the user still has to do

### Safe eject

Attempt a proper dismount, not just a flush:

| Platform | Mechanism |
|---|---|
| Windows | `FSCTL_LOCK_VOLUME`, `FSCTL_DISMOUNT_VOLUME`, then `IOCTL_STORAGE_EJECT_MEDIA` |
| Linux | `udisksctl unmount` then `power-off` |
| macOS | `DADiskUnmount` |

**Eject will sometimes be refused**, because Windows Search, an antivirus scanner, Explorer, or a thumbnail service holds a handle. After writing 2,798 files the user must not be told only "could not eject."

```
┌──────────────────────────────────────────────────────────────────┐
│  Copy finished. All 2,798 files verified.                        │
│                                                                  │
│  Windows would not release the drive yet. This is normal after   │
│  a large copy and your files are already written and verified.   │
│                                                                  │
│  [ Try eject again ]   [ Show what is using it ]   [ Done ]      │
│                                                                  │
│  Every file has been written and checked. It is safe to use      │
│  Windows' own Eject when it lets go.                             │
└──────────────────────────────────────────────────────────────────┘
```

The distinction that matters: **eject failing is not the copy failing.** Every file is already fsynced and hash-verified, so the correct message is reassurance plus a retry, not an error.

### What the user does on the console, and why

Unavoidable, per the platform constraint in section 1:

1. Put the Xbox into Developer Mode. **Entirely outside this software.** An earlier draft added "requires a Microsoft partner account" as a statement of fact. **No file in this workspace states that, and this document does not have a source for it**, so it is marked UNVERIFIED and must not appear in shipped UI copy in that form. The wizard links to Microsoft's own current documentation rather than restating a requirement it cannot check. Whatever the enrollment terms are, they are Microsoft's to state and they change.
2. Install the Dolphin Xbox app on the console. Its identity and version are an open item (section 13, item 2), so the wizard cannot name a specific app, a version, or a download location. It says what it knows and does not guess.
3. Plug in the drive and launch the app.
4. Point the app at the game if it does not find it, since `E:\Games` may not be the letter the console assigns (section 11, and open item 4).

**Three of those four steps depend on facts this document does not have.** That is the honest state of the feature, and the last screen should read like a checklist with links rather than like instructions written by someone who has done it. Nobody in this workspace has.

**The wizard's last screen is instructions, not a progress bar.** It should state which of these steps are one-time and which repeat per update. Steps 1 and 2 are one-time; steps 3 and 4 are per session or per update.

---

## 13. Open items

Ordered by whether they block shipping.

1. **BLOCKING: the destination path on the drive for `Config`, `Cache`, `GameSettings`, `GC` and `Load`.** No README, manifest, or instruction file exists anywhere in the package (verified: only `exe`, `ini`, `iso`, `png`, `raw` files are present). The wizard cannot write a correct layout until someone with a console confirms where the Xbox Dolphin app reads its User directory from. The profile indirection in section 6 contains the damage; it does not remove it.

2. **BLOCKING: the identity and version of the Xbox-side Dolphin app.** Verified that it is not the Reborn fork and that it uses a "Flipper 2.2 - Blackout (Performance)" theme. Questionnaire batch 2 question 6 asked "Is it a public Dolphin UWP build? If so, which one, so I can pin the version?" and was never answered. Without it, the wizard cannot know that the config keys it writes are the keys the app reads.

3. **BLOCKING for any format feature, and it now reaches further than that: the filesystem the console requires.** Not stated in any document, config file, or README. The wireframe shows both exFAT (line 662) and NTFS (line 665) drives without saying which is acceptable. Do not invent an answer. FAT32's 4 GiB limit is verified not to be a blocker for this payload, which is a different question from whether the console accepts FAT32.

    **This item is not confined to a format feature any more.** Section 8's filesystem precondition decides which drives the wizard will write to at all, and section 9.2's staged-rename crash-safety claim is conditioned on the same reading. Both were written to behave sanely without the answer, and both collapse to one row once the answer exists. Closing this item is the single highest-value console test after item 1.

4. **UNVERIFIED: whether an Xbox in Dev Mode assigns `E:` to a USB drive**, always, sometimes, or never. `E:\Games` is verified as the shipped value in `Dolphin.ini`. Nothing confirms the console honors it.

5. **UNVERIFIED: whether a user's controller enumerates as `WGInput/0/Xbox One Game Controller`.** Verified by reading `Step 2 - Transfer to Xbox\Config\GCPadNew.ini` in full: most `[GCPad1]` bindings are qualified with that exact device string, and **three are unqualified, not two.** An earlier draft counted two and missed the one that matters:

   ```
   Buttons/B = `Button X`
   Buttons/X = `Button B`
   Main Stick/Modifier = `Shift`
   ```

   The first two are the intentional X and B swap, the same swap that exists in the PC config, and they resolve against the default device, which is the same controller. **`Main Stick/Modifier = `Shift`` is a different problem.** It is unqualified *and* it names a keyboard key, on a console where `[GCPad1]`'s default device is a gamepad and where there is usually no keyboard at all. It cannot resolve. The earlier framing, that unqualified bindings "rely on the default device resolving," does not cover this one: the default device has no `Shift`.

   The likely effect is benign, an unbound modifier that does nothing, and Dolphin tolerating an unresolvable expression is the expected behavior rather than a crash. **Neither the effect nor the tolerance is verified**, because it depends on the unknown Xbox build (item 2). The wizard's payload should drop the line rather than ship a binding that is known not to resolve, and that is a one-line profile edit.

   This is also the clearest example of why the whole `Config\` tree needs one review pass against a real console rather than being carried forward verbatim.

6. **No test hardware is recorded.** `COMPONENT_MAP.md` section 8 lists Xbox Dev Mode test hardware as "unknown." Items 1, 2, 3 and 4 cannot be closed without a console. **If no Xbox in Dev Mode is available before September 12, this feature ships with an unconfirmed layout profile and must say so in the UI.**

7. **Undecided: whether the wizard exists on Linux and macOS at all.** No document states it. Drive enumeration, volume-type checks, and safe eject are all platform-specific, and there is no Linux or macOS hardware to verify them on. Section 7 records all three implementations so that this is a scoping decision rather than a redesign.

8. **Unconfirmed: local multiplayer on Xbox.** Verified by reading the file: `[GCPad2]`, `[GCPad3]` and `[GCPad4]` each contain exactly one line, `Device = WGInput/0/Xbox One Game Controller`, the same device as `[GCPad1]`, with no bindings under any of them. The PC config points ports 2 to 4 at `DInput/0/Keyboard Mouse`, also unbound. Whether the Xbox app auto-assigns extra pads, or whether the payload should ship real player 2 to 4 bindings, is untested.

    **Section 3 has been corrected to match this item.** An earlier draft called the Xbox side a "single-player-and-couch install" on the first page while marking couch play unconfirmed here. One controller is configured, and that is all the evidence supports until someone plugs in a second pad.

9. **Unconfirmed: `EFBAccessEnable` is absent from the Xbox `GFX.ini`** while the PC one sets it `True` explicitly. This project has documented that disabling EFB access renders the NFL Challenge portraits black. The Xbox default has never been checked, and it cannot be checked without knowing which build runs there (item 2).

10. **Untested: whether the Xbox app can load DDS hires textures.** If it can, the release carries one texture tree instead of two and saves roughly 1.5 GB. No evidence exists either way. One experiment on a console settles it.

11. **CLOSED, and it was never a real gap.** An earlier draft recorded "19 textures exist only in the Xbox set, mostly 32x32 and 64x64 by base name, with 0 present only on PC" and asked whether that was intentional Xbox content or PC-side drift. **It was neither. It was the wrong PC folder.** The 19 were measured against the shipped older runtime at `Files\Reborn Dolphin\User\Load\Textures\GN7E69` (2,763 files, all DDS, 2,077,362,716 bytes), and the document never said which folder it used. Measured against the **selected** runtime at `...\CREATION LEAGUE AND NEW DOLPHIN\User\Load\Textures\GN7E69` (2,782 files: 2,763 `.dds` plus exactly 19 `.png`, 2,079,212,285 bytes), the base-name comparison against the Xbox set gives **0 Xbox-only and 0 PC-only.** That folder sits beside the `Dolphin.exe` with SHA-256 `617321bf37ac7b2f9bdfcd3af803a019e862591259c770e9e688d53834df1338`, the selected runtime. Section 4's table now states all three columns and names each folder.

    **The lesson generalizes past this item and is why section 4 and section 8 both now name their folders.** Two PC runtimes exist on this machine and they differ in textures (2,763 vs 2,782 files) and in memory card contents (three distinct card hashes across the three locations). Any measurement in any Reborn document that says "the PC folder" without a path is ambiguous and is probably measuring the runtime that is being replaced.

12. **Unconfirmed: whether the empty directories are required.** `Config\GraphicMods`, `GC\EUR`, `GC\JAP` and `GC\USA\Card A` are verified present and empty. This document creates them because omitting them is the riskier default, not because anything says they are needed.

13. **Unanswered, and it sets the precedent for this document's preservation contract:** questionnaire batch 2 question 2 asked whether to fix the save-destroying uninstall and the never-working backup in the live 2.1 Windows installer. No answer is recorded. The Xbox wizard's "preserve `GC\` by default" is the same promise, and it would help to know whether the existing installer is being fixed alongside it.

14. **Unanswered: whether `FR4N31` (online) or `CR4E12` (Creation League) is ever an Xbox target.** Only `NSR.iso` appears in the Xbox package, but nothing states that Xbox is offline-only. Section 3 assumes it is, on the evidence that no Reborn NetPlay code runs there.

15. **Five documents need edits to agree with this one, not three.** An earlier draft listed three and omitted two that this document also invalidates. All five checked against the files for this revision:

    | Document | What is stale | Verified location |
    |---|---|---|
    | `COMPONENT_MAP.md` section 9 | "pair by IP or PIN", "Device Portal, HTTPS to deploy or update UWP package", "reuse retained verified base", "patch and prepare Xbox game image". All four contradict locked decisions | Called out in this document's header |
    | `INFORMATION_ARCHITECTURE.md` | Line 68 `\| Xbox wizard \| 7 modes \| drive picker, verify report \|` and line 234 "Seven modes as option cards" both say seven; section 5 retires two. Line 112 lists "format a USB drive" in the 800 ms hold-to-confirm set; section 8 removes the format path entirely | Lines 68, 112, 234, read out |
    | `WIREFRAMES.md` section 16 | Line 682 "Will copy «2.9 GB»" should be 3.01 GB (section 1 units note). Line 684 backs up to `«E:\Reborn_backup_...»` on the drive; section 8 moves backups to the PC data root | Lines 682, 684, read out |
    | **`PROJECT_QUESTIONNAIRE.md`** | **Row 7.2 still reads `\| Seven wizard modes \| Decided \|` and enumerates "Deploy or Reinstall App, Test Connection", both retired by section 5. Row 7.9 still reads `\| Console pairing credentials \| Decided \| Windows credential store. Never logged`, which is meaningless once there is no console connection to pair with. Row 7.1 also carries the "2.9 GB" figure** | Lines 131, 132, 139, read out |
    | **`COMPONENT_INVENTORY.md`** | **Section 13 line 402 names `C:\Users\Adam\Downloads\NFL Street Reborn Xbox Dev Mode 2.1` as the "Authoritative path". Section 10 of this document overrides that in favor of the `(1)` copy, which is the clean single-wrapper extraction and the one every figure here is measured from. The inventory's line 406 note that the duplication is real remains correct about the folder it names** | Lines 402, 406, read out |

    Rows 4 and 5 are the additions. Neither is cosmetic: row 7.9 marks a retired feature "Decided," and the inventory points implementers at the folder with the doubled contents.

16. **Nothing is implemented.** Verified for this revision: `git rev-list --count HEAD` in the launcher repository returns **12**, and `git ls-files` outside `docs/` returns exactly five paths, `.gitignore`, `CLAUDE_START_HERE.md`, and three logos under `assets/`. An earlier draft said "`docs/` and three PNG logos," which undercounted: **the repository also tracks `CLAUDE_START_HERE.md`, `.gitignore`, and a fourth PNG, `docs/design/Reborn Launcher Moodboard.png`.** Four PNGs total, and the governing protocol document itself is in the tree.

    Every line of this wizard is new work, and `.gitignore` already excludes `*.iso`, `*.raw` and `**/Load/Textures/`, which is precisely the payload described here. It also carries `!docs/design/*.png` and `!assets/*.png` re-inclusions, which is how the four PNGs survive the `*.exe`/`*.dll` binary exclusions above them.
