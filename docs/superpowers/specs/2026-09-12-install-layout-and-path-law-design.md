# Install layout and path law

Date: 2026-09-12. Branch `paths-overhaul` off `integration`.
Approved by Adam in chat, 2026-09-12 (approach A, tuck-first, no migrated-textures copy).

## What Adam asked for

1. The stable launcher installs to `NFL Street Reborn`, not `NFL Street Reborn Launcher`. In AppData Roaming. Beta follows the same rule.
2. Paths are protected: publishing a texture update must not break what the game loads.
3. The texture folder exists once. It is not in `mods\` as well.
4. The base of every new Dolphin user folder is the tuned one at `nflstreet2_clean\DOLPHIN ONLINE TEST\NFL Street Reborn Dolphin\User`, "the real basic stuff", no texture packs.

## What exists today, measured on this PC

- Stable data root `%APPDATA%\NFL Street Reborn Launcher`, program in `app\` inside it. Beta adds ` Beta` to the name. `PRODUCT_DATA` in `platform/imp.rs` spells the name; the installer's `DataDir` define spells it again.
- `%APPDATA%\NFL Street Reborn` is the 2.1 install: `NFL Street Reborn Dolphin.exe`, `User` (memory cards, the live texture pack), `Games`, `MODS` (JSGME), `Tools`, `Sys`, `unins000.exe` registered under AppId `{DCD6B9B9-...}`. The stable installer already reuses that AppId.
- The supervisor refuses to start when the data root is a 2.1 install (`Supervisor::acquire_at`, `OLDER_INSTALL_IN_THE_WAY`). Three tests in `lib.rs` and `supervisor.rs` assert the data root is never that folder.
- Textures on disk three times, hardlinked: `app\content\textures\GN7E69` (installer payload), `library\textures\GN7E69` (pristine, the update target, seeded from the payload), `mods\active\GN7E69` (composed = pristine links + mod overlays). The user folder's `Load\Textures\GN7E69` is a junction, absolute, into `mods\active`.
- `ComposedRecord` records the pristine root as a path plus entry counts and overlay hashes. A texture update swaps the pristine directory whole (`content.rs`, prior/), so an update with the same file names and count leaves the composed hardlinks pointing at the old bytes and nothing detects it.
- `resolve_portable_paths` rewrites Dolphin.ini's LoadPath, DumpPath, NANDRootPath, ResourcePackPath and memory card paths to absolute paths inside the user folder at every Play. `memory_card_overlay` (2fa4a78) writes an absolute `MemcardAPath` when slot A is none.
- The runtime template ships three INIs (`User\Config\Dolphin.ini`, `GFX.ini`, `GameSettings\GN7E69.ini`), made from the reference folder by a scratch script. `seed_user_from_template` copies only `.ini` files from `Config` and `GameSettings`.
- Migration copies the whole 2.1 `User` (2 GB of textures included) into `dolphin-users\migrated-<stamp>`, then copies the textures again into `migrated-textures\<stamp>`, then the first sync moves the user folder's texture directory aside and junctions over it.
- The reference User folder differs from the shipped template only by sanitizing (analytics ID, ISO paths, DefaultISO, netplay identity, dumping and FPS counter). It also holds pad, hotkey, Qt, logger and Wiimote INIs, pipeline UID caches and D3D shader caches (5.7 MB), Adam's memory card, and two 1.1 GB CR4E12 texture packs.

## Design

### 1. Names and roots

| Channel | Data root | Program |
|---|---|---|
| Stable | `%APPDATA%\NFL Street Reborn` | `%APPDATA%\NFL Street Reborn\app` |
| Beta | `%APPDATA%\NFL Street Reborn Beta` | `%APPDATA%\NFL Street Reborn Beta\app` |

- `PRODUCT_DATA` is deleted; the data root is `PRODUCT` plus the channel suffix on every platform. `previous_data_root()` names the `Launcher`-era folder for adoption.
- Cache root stays `%LOCALAPPDATA%\NFL Street Reborn[ Beta]\cache`.
- Programs and Features, Start menu and desktop shortcuts say `NFL Street Reborn` / `NFL Street Reborn Beta` (no "Launcher"). Uninstall shortcut likewise.
- Unchanged: AppIds, installer output file names (`NFL Street Reborn Launcher Setup <v>.exe`, `... Update <v>.exe`), `latest.json` / `latest-beta.json`, the Publisher and its install folder, the site's download pointer, the HTTP user agent string.

### 2. First start, before the supervisor lock

A new core module `layout` runs two steps at every start of the WINDOW, before `Supervisor::acquire`, before seeding, before the page loads. Both are idempotent and log one line each. `rebornctl` never runs them implicitly: it is what every end-to-end script, the Publisher and the build run on a developer's PC against scratch roots, and a first version that prepared the real root from the CLI moved Adam's 2.1 folder aside in the middle of a test run (2026-09-12, reversed by rename).

**Adopt** a `Launcher`-era root. Condition: the new root has no `config\settings.json` and the previous root (`<parent>\NFL Street Reborn Launcher[ Beta]`) has one. Action: rename every top-level entry whose name is a known 3.0 layout name (`DataDir::ALL` roots: `library`, `runtime`, `mods`, `music`, `saves`, `screenshots`, `stats`, `versions`, `recovery`, `backups`, `cache`, `logs`, `config`, `downloads`, `staging`, plus `dolphin-users`, `journal.json`, `migrated-effects`, `migrated-textures`) into the new root. Skip `app`. Unknown entries (`publisher`) stay. A collision is skipped and logged, never overwritten. If the previous root ends up empty it is removed.

**Tuck** a 2.1 install. Condition: `LegacyInstall::detect(root)` finds one at the data root itself. Action: rename every top-level entry that is not a 3.0 layout name and not `app` into `<root>\Reborn 2.1\`. Nothing is deleted. Before moving, `ensure_runtime_idle` on the 2.1 Dolphin; if anything is locked, the step reports and the launcher retries at the next start. This runs first so the launcher never writes into a folder where `mods` is JSGME's `MODS`.

The supervisor's refusal of a 2.1 data root becomes a refusal only when the tuck could not complete (a locked file), with the same player sentence adjusted to say to close the old game and try again. The three tests asserting the data root is never the 2.1 folder are replaced by tests of tuck and adopt.

Onboarding's default legacy root becomes `<root>\Reborn 2.1` when it exists, then the old guess. Migration copies out of it as today; the tucked copy is the backup and is never touched again.

### 3. Textures composed into the user folder

- `composed_root(user_dir, game)` = `<user_dir>\Load\Textures\<game>`. `DataDir::ModsActive` is removed; `activation_path` derives `mods\` from `ModsPackages`.
- `Composer::apply` already builds beside and swaps (4304e0d). `<game>.building` and `<game>.previous` sit in `Load\Textures\`, where Dolphin loads only `<game>\` and folders carrying a `<game>.txt` marker, so they are invisible to the game.
- `link_textures` and the junction are removed. `SyncRequest.link_into_user_dir` becomes `manage_user_texture_folder`: false means the sync composes nothing and leaves `Load\Textures\<game>` alone (the texture author's mode).
- Transition, in `sync_active` before planning: if `Load\Textures\<game>` is a junction or symlink, remove the link (no content). If it is a real directory without `composed.json` and not empty, rename it to `<game>.before-reborn.<ms>`. If `mods\active\<game>` exists and carries `composed.json`, remove it once.
- `ComposedRecord` gains `pristine_identity: Option<String>`: the textures content pointer's version and sha256 when the pristine root is the delivered tree, else a fingerprint of the pristine root (entry count plus the newest mtime). `Composer::plan` computes it; `verify` reports `PristineIdentityDiffers` when it changed. Test: a pristine tree swapped for one with identical names and counts but a new identity rebuilds the composed tree and the composed files are the new bytes.
- Migration no longer copies `User\Load\Textures` into the migrated user folder (`copy_tree_with_progress` gains an exclusion list) and no longer writes `migrated-textures\<stamp>`. `Settings.migrated_texture_root` stays readable for installs that have one; `pristine_root` keeps honoring it.

### 4. Path law for Dolphin.ini

A Reborn user folder is self-contained: Dolphin derives every path from the `-u` folder.

- At every launch, and when the template is staged, these keys are removed when their value is relative or points inside any Dolphin user folder (this one or another): `[General] LoadPath, DumpPath, NANDRootPath, ResourcePackPath, WiiSDCardPath`, `[Core] MemcardAPath, MemcardBPath, GCIFolderAPathOverride, GCIFolderBPathOverride, AgpCartAPath, AgpCartBPath`, `[GBA] BIOS, SavesPath, Rom1..4`. An absolute path outside every Dolphin user folder is a player's own choice and is kept. `portable_path_overlay` becomes `path_law_overlay` returning removals; `IniDocument` gets a remove operation if it lacks one.
- `[Core] DefaultISO` and `[General] ISOPath*`, `ISOPaths` are removed always (the launcher passes `-e`).
- `memory_card_overlay`: slot A none becomes `SlotA = 1` with no `MemcardAPath`; an existing `MemcardAPath` is kept only when it is outside the user folder and the file exists.
- The card gear sync and every other reader of the card path fall back to `<user>\GC\MemoryCardA.USA.raw` already (session.rs); confirmed in the plan by grep before relying on it.
- To prove on the fork before the law is final: launch with Dolphin.ini carrying no path keys and no `MemcardAPath`, then read back that textures loaded from `<user>\Load\Textures\GN7E69`, the card was created at `<user>\GC\MemoryCardA.USA.raw`, and Dolphin.ini on exit still carries no path key. If Dolphin writes a path back, the launch-time removal still makes the next launch correct, and that fact is recorded in the doc.
- **Observed on the fork, 2026-09-12 (Tools\walk-paths-overhaul.ps1, two runs):** with no path keys, Dolphin's video log names `<user>/Load/Textures/GN7E69` as the folder it scans for custom textures (its default from `-u`), the migrated memory card sits at `<user>\GC\MemoryCardA.USA.raw` with `SlotA = 1` and no path, and after a graceful close Dolphin.ini still carries none of the nine path keys: the fork writes nothing back. The law holds without the launch-time removal doing anything on later launches.

### 5. User template

- `rebornctl runtime template --from <User> --into <runtime>\User` (core `runtime::template`). Stages: `Config\*.ini`, `GameSettings\*.ini`, `Cache\*.uidcache`, `Cache\Shaders\*.cache`. Nothing else: no `Load`, `GC`, `Dump`, `Logs`, `GameCovers`, `gamelist.cache`, `Styles`, `Wii`.
- Sanitizing, as code with tests: Dolphin.ini drop `[Analytics] ID`, `[General] ISOPath*`, `ISOPaths`, `[Core] DefaultISO`, `[NetPlay] HostCode, Nickname, IndexName, IndexPassword`, every path-law key; set `SlotA = 1`, `SlotB = 255`. GFX.ini set `DumpTextures`, `DumpBaseTextures`, `DumpMipTextures` to False and `ShowFPS` to False. Qt.ini drop `lastdir` and any absolute path. Logger.ini: the launcher overlays it at launch; ship as is. GCPadNew.ini ships as the keyboard-and-XInput fallback; the launcher generates it per profile at launch anyway.
- `build.ps1 -UserTemplate <User>` replaces the copy of `User\Config` and `User\GameSettings` from the runtime folder. Without the flag the build fails, so a runtime cannot ship with no baseline.
- `seed_user_from_template` copies the whole template tree file by file, no clobber, any extension.
- This is a new runtime version. Per the standing law, the content release that pins the new runtime is published right after the build (Publisher, Textures runtime mods page), or every install's next check pulls the old runtime back over it.

### 6. Installer

- `DataDir` defines: `NFL Street Reborn` / `NFL Street Reborn Beta`. `AppName` unchanged. `UninstallDisplayName={#AppName}`; shortcut names drop "Launcher".
- `InstallDirIsSafe`: true only for `{userappdata}\{#DataDir}\app`. Refuses both new data roots, both old `Launcher` roots, and anything inside any of them. The 2.1 root is the stable data root now, so its special case folds into the rule above.
- `RemovePreviousInstall` (was `RemovePreviousProgramsInstall`): also handles a registered uninstaller under `{userappdata}\NFL Street Reborn Launcher[ Beta]\app`. Guard stays: the uninstaller's folder must hold `reborn-launcher.exe`, so the 2.1 `unins000.exe` registered under the same AppId is never run. After it runs, the leftover `app` folder is removed; the data beside it is left for the launcher to adopt.

### 7. Consequences stated for Adam

- On Adam's PC a stable install tucks the dev tree (`Games` 15 GB, `Tools` with the Builder, `User` with the live pack) into `Reborn 2.1\`, and adopts the stale test data in `NFL Street Reborn Launcher`. Delete or rename those first if that is unwanted. The beta root renames itself to `NFL Street Reborn Beta` on the next beta.
- Players still running the 2.1 Dolphin after installing 3.0 find it under `Reborn 2.1\` with its absolute paths pointing at folders that moved; 3.0 replaces it.
- The 09-08 early stable (3.0.0) installs reinstall by hand as already decided; the new installer removes their old `app` and the launcher adopts their data.

### 8. Testing and proof

- Unit: tuck (fresh, partial, locked, idempotent, `mods` case collision), adopt (fresh, collision, unknown entries stay, empty old root removed), path law (relative, inside this folder, inside another folder, foreign absolute kept, DefaultISO always removed), memory card default, template staging and sanitizing against a copy of the reference folder, composed identity rebuild, migration exclusion, `activation_path` without `ModsActive`, installer define review via `test-all`'s input checks.
- `Tools\test-all.ps1` green: Rust suite, checkers, browser suites.
- Real window on a scratch root: a fake 2.1 folder (small `User` with a 16 MB card, `MODS` with one JSGME mod, `Games\NSR.iso` hardlinked) at the data root; start the payload launcher with the debugging port; observe the tuck; walk "I already have Reborn" from `Reborn 2.1`; Play; then read back: real composed folder with `composed.json`, no `mods\active`, no path keys in Dolphin.ini, card at the default path, Dolphin's log shows custom textures loaded. Screenshots into `target\review`.
- Beta rename: run the launcher against an old-named scratch root and confirm adoption by listing both roots.

### Result (2026-09-12)

Tasks 1 to 13 of the plan are done on `paths-overhaul` (commits 44ebe3a..ad132dd). 981 Rust tests pass; `Tools\test-all.ps1 -Browser` is 28 of 29, the miss being "a stable installer exists in dist" until one is built. The real-window walk (`Tools\walk-paths-overhaul.ps1`, a fake 2.1 install at a scratch beta data root plus a Launcher-era root beside it, beta.24 payload) is ALL PASS on its second run: tuck and adopt before the window drew; "I already have Reborn" found `Reborn 2.1`, its disc and its card by itself; import, wizard, Play; no path keys, slot A default; `Load\Textures\GN7E69` a real composed directory of 8,319 entries whose files are one hardlink shared with `library\textures` and the payload; no `mods\active`; Dolphin started, closed on request, wrote no path keys back. The first run's three texture FAILs were the driver checking before the launch-time sync; the driver now syncs first.

Two things the walk surfaced that are NOT this design's:
- The game's main menu shows the disc's "MISSING TEXTURES" placeholder slide. The `Menu Slideshow` override folder is absent from every texture pack on this PC, the live 2.1 pack and the staged textures 1.2.0 included, so the published pack lacks it and every player sees the placeholder. A content publish, not a launcher change.
- Dolphin's video log reports duplicate texture names and an oversized 3840x2160 texture in the pack; both are pack content.

One incident during the work: the first version prepared the real data root from `rebornctl`, and `test-all`'s end-to-end scripts ran it on this PC, so the 2.1 folder was tucked aside for a few minutes. Reversed by rename the same hour, nothing lost; the CLI no longer touches the real roots (56eff37).

### Sequencing

Sections 1, 2, 5 and 6 are the visible contract and belong in today's stable. Sections 3 and 4 are internal and could follow in 3.0.1 without a player-visible migration. All of it is built now on `paths-overhaul`; the real-window walk decides what ships.

### Out of scope

The Publisher's own install folder, installer file names, the cache root, the content pipeline beyond the marker identity, macOS and Linux behavior beyond renaming the constant.
