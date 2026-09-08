# Next steps, as of 2026-09-06 afternoon

Ship date: **2026-09-12**. Written before a context compaction so the next
session starts from facts, not memory.

## Where things stand

**Live and proven today:**

- Signing: Adam's everyday key `eac9ec…` and rotation key `71a34c…` are
  compiled into `crates/reborn-core/src/signing.rs`. A release signed with
  `C:\Users\Adam\Reborn Keys\publisher.key` opens under compiled-in trust
  alone.
- Origin: `https://files.nflstreetreborn.com/` (R2 bucket `reborn-content`,
  DNS on Cloudflare since today, registrar GoDaddy). Serves release
  **3.0.0 sequence 2**: `textures 1.0.0` (8,192 files), `runtime 1.0.0`
  (2,569 files), `roster 2026.09.06`. 10,490 objects, 2.95 GiB.
- Proven over the internet: a launcher with only the compiled-in keys
  installed the runtime and the roster from that URL into a scratch root.
- Edge cache rule `files blobs` (`/blobs/*`, one year) deployed and
  verified: MISS then HIT; `release.json` correctly stays DYNAMIC.
- Roster pushes: `rebornctl publish roster --from <site export url>`
  fetches from the live site with the token in the Windows vault
  (`nsr.site.export`), publishes, re-signs. Each launcher applies the
  roster to its own image with `reborn-builder-cli.exe` in about 6.6 s.
- Site: `/rdatabase` accepts `Authorization: Bearer` for API calls
  (`app_user.api_token_hash`, minted for user `Adam`, runner deleted).
- Add, change, delete, repair, resume, replay refusal, tamper refusal,
  zero-byte files: all covered by `Tools/e2e-update.py` (12 steps) and
  `Tools/e2e-roster.py` (3 steps, real image + real Builder).
- **The update pipeline is one core module** (commit `d4e0a98`, 2026-09-06
  late afternoon): `reborn_core::content::pipeline::Updater` with `check`,
  `fetch`, `promote`, `run` and `apply_current_roster`. `rebornctl update
  content` is now a thin caller of it, and the window calls the same three
  functions. Nine tests over an in-memory origin; both e2e scripts pass on
  the rewired CLI; dry runs against the live origin work. A quick check
  compares the cached manifest with the release pin and touches nothing
  else; the cache now holds the served bytes so that comparison can be true.

**Pulled today, on purpose:** the full modded image (`offline-content`).
The product moved to the patch route: the installer ships a 687 MB
`reborn.xdelta` and the player supplies their own disc. A downloadable
modded game on a public URL had no place in that. Copies are kept at
`C:\Reborn Origin-stage\pulled\` if ever needed.

## The order of work

### 1. DONE 2026-09-06 evening: taken over, landed, and the game-patch component built

Adam said "take it over". The other session's 68 files landed as `94880f2`
(their test run was green; the one failure is the stale dist zip check).
Then the game-patch component, on top of it:

- **Two image layers** (`reborn_core::game_image`): pristine at
  `library/imported/<sha256>/NSR.iso`, never written; booted at
  `library/offline/NSR.iso`, a hardlink of the pristine file until a roster
  is applied into it. `settings.game_path` points at the booted one. The
  roster apply reads pristine and writes booted.
- **`game-patch` component** (`reborn_core::game_patch`,
  `ComponentId::GamePatch`, `library/game-patch`): a chain of xdeltas
  between consecutive builds plus `chain.json`. The pipeline promotes it
  before the roster, applies the chain, re-links the booted image, and
  re-applies the roster. `rebornctl publish game-patch --from --to
  --version` on the publishing side.
- **Proven** by `Tools/e2e-game-patch.py` over the real image with the
  real xdelta3 and Builder. Decision breakdown and measurements in
  `docs/architecture/GAME_PATCH_COMPONENT.md`; the verb in
  `docs/RUNNING_UPDATES.md` 7b.

Still open from the read of their work: `game_import.rs` uses
`std::process::Command` and `#[cfg(windows)]` outside `platform/`, which
the core forbids; `ProcessHost::run_capturing` is the seam. Cosmetic until
someone ports the core.

**The first real game patch must be published FROM the image the
installer's xdelta produces** (the `--target` given to
`Tools/create-game-patch.py`, today `NFL Street 2 FIELDS 12-19.iso`), or
no imported player will be in the chain.

### 2. Three launcher items

1. **DONE 2026-09-06 evening: the window checks for content updates on its
   own.** `crates/reborn-ui/src/content_updates.rs` holds the flow's state
   (five commands: `content_status`, `content_check`, `content_fetch`,
   `content_promote`, `content_dismiss`); `ui/content-notice.js` is the Play
   page notice, mirroring `update-notice.js`; Settings has a Game content
   block. `Tools/check-content-notice.mjs` drives the notice in a real
   browser. `settings.content_origin` overrides `origin::DEFAULT_ORIGIN`.
   Not yet observed: the real WebView2 window against the live origin on a
   real data root. The pieces below it are all observed (CLI e2e, browser).
   The snippet that was here is kept for reference:

   ```rust
   use reborn_core::content::pipeline::{CheckRequest, Depth, RosterHook, Updater};
   let origin = Origin::new("https://files.nflstreetreborn.com/");
   let verifier = Ed25519Verifier::embedded();
   let updater = Updater::new(&platform, &data_root, &origin, &verifier);
   // at startup and on a timer, cheap: no file is hashed unless a newer version is offered
   let check = updater.check(&CheckRequest::default())?;          // Depth::Quick, all six components
   if check.has_updates() { /* show check.download_bytes(), then in the background: */ }
   let fetched = updater.fetch(&check)?;                           // writes only under downloads/ and staging/
   // only when no game session is running:
   let promoted = updater.promote(&check, &ImageHook {
       builder: None,                                              // found in <install>\tools, beside the launcher, extra_dirs
       xdelta: None,                                               // found in <install>\game-import
       extra_dirs: vec![legacy_root.join("Tools")],                // settings.legacy_root, when set
       textures_root: Some(mods::pristine_root(&platform, &game)?),
   })?;
   ```

   Keep the `ContentCheck` in managed state between the calls; the plans
   live in it. `Depth::Verify` is the Repair button only: on textures it
   hashes 3.18 GB and takes 45 s. The import flows in `setup_tools.rs`
   already establish the booted image and apply an installed roster to
   it, so nothing extra is needed there.
2. **DONE 2026-09-06 evening: the one-file installer build.**
   `.\installer\windows\build.ps1 -LauncherOnly -Version <v>` makes
   `dist\NFL Street Reborn Launcher Update <v>.exe` (37 MB) and `latest.json`
   pinning it. It stages in `target\installer-payload-launcher`, never in
   the full payload. `update.rs` passes `/DIR=<install root>` so a custom
   install folder is honored. Publish: the exe and `latest.json` into the
   bucket's `launcher/` folder, which is where `update.rs` now reads
   (`DEFAULT_SOURCE`); the GitHub repo is private, so its releases could
   never have served a player. Guide 7c has the two rclone lines.
   Proof, observed 2026-09-06 20:36 in Windows Sandbox: the full set
   installed, then the Update exe over it with `/RELAUNCH=1 /DIR=...`: exit
   0, launcher still in Programs, runtime kept, textures kept, import bundle
   kept, `tools\reborn-builder-cli.exe` present and runs on a clean Windows,
   `rebornctl` runs, the launcher came back on its own; then uninstall left
   the memory card and the 2.1 folder alone. 52 of 53 checks; the one miss
   is the WebView2 download the sandbox never gets (known).
   Sandbox note: killing the session's window processes can leave the VM
   worker (`vmmemWindowsSandbox`) alive for minutes, and a second launch
   sits dead until it exits; wait for it, then launch. The host dropped to
   2.6 GB free during the run with Dolphin open, which is what kills a
   waiting background task.
3. **DONE with 2: `reborn-builder-cli.exe` ships** under `<install>\tools`
   in both builds, from `%APPDATA%\NFL Street Reborn\Tools` or `-Builder`.
   Only the exe: `clean-tree.txt` beside it names a folder on this machine.

**Where the import bundle lives** (the 687 MB `reborn.xdelta` + manifest +
tools that `-GamePatch` needs): `C:\Users\Adam\Documents\Codex\2026-09-04\
c-users-adam-desktop-reborn-installer\work\game-patch\bundle`. The copy in
`target\installer-payload\game-import` is staging, restaged by every full
build, and was wiped once on 2026-09-06 by a launcher-only build under the
old script (restored from the Codex folder, hash verified). Keep the Codex
folder, or move the bundle somewhere named and point `-GamePatch` at it.
The full build now refuses a `-GamePatch` inside the folder it wipes.

### 3. DONE: the game-patch component (see item 1)

What remains around it: nothing in the launcher window yet says "game
update applied"; that comes with the window wiring in item 2, where
`PromoteState::Applied { game_patch, roster, .. }` carries both results.

### 4b. 2026-09-07 night: the first-check gap, and the ship tree

**Found by measuring:** on an empty data root the first content check
planned 8,192 texture files (3.18 GB) and 2,569 runtime files, all of them
already under the install root. Fixed in `bf9590c`: `content::seed` fills
each empty data-root tree from its install-root twin by hardlink before the
check, and the window adopts a zero-byte update on the spot (pointer and
cache written, nothing shown). Proven against the final payload and the
live bucket: runtime and textures both plan zero fetches on a seeded root.
The origin's `runtime 1.0.0` had been published from a dirty staging tree
(ReShade logs, a `User/Dump`), so `runtime 1.0.1` was published from the
clean final payload and the release re-signed at **sequence 3** (live).

**The site's `DOWNLOAD_URL`** in `_site/shell.php` is set to the zip on
the bucket, deployed and read back identical. The button appears at the
gate's own release moment, 2026-09-12 17:00 America/New_York.

**Another session is editing the UI in this tree at the same time** (the
mods page rebuilt as a store, `dolphin_settings.rs`, `finish.css`, the
`.pad-button` rename). As found, its state broke `check-mods-page` and the
browser suite. Its edits were committed as found (`a8829c9`) so nothing is
lost, and **the shipped installer is built from `bf9590c` in the worktree
`target\ship-tree`**, where every checker passes. That work can ship later
as a 37 MB launcher update once it passes the checkers.

**2026-09-08 daytime, Adam's walkthrough in the sandbox, three findings:**

- `DolphinTool.exe` imports `MSVCP140_ATOMIC_WAIT.dll`, which the VC
  allowlist did not ship; on a clean Windows the import tool could not
  start and onboarding called the disc unreadable. Fixed in `b1ee54c`:
  five VC files ship beside Dolphin and the import tools, and the smoke
  test runs DolphinTool on the clean Windows. After the fix Adam's import
  built and installed the game from `NFL Street 2 (USA).nkit.iso`.
- **The sandbox has no internet while Surfshark's WireGuard tunnel is up
  on the host** (interface metric 5 takes the default route). That was
  every failed WebView2 download on the 6th and the launcher's
  "Connection Failed ... 10060" in the sandbox. VPN off, the launcher's
  own update check read the bucket at once: "Up to date at 3.0.0."
- With a 32 MB roster pending and updates set to ask first, the window
  never adopted the seeded textures and runtime, so every check re-hashed
  the 3 GB pack. Fixed in `810c9fe`: zero-byte components are adopted on
  their own and the check re-run against their pointers.
- `beta-sandbox.ps1 -Disc <folder>` maps a stock disc image in at
  `C:\beta\disc`; manual mode waits for the install and the launcher
  closing instead of a keypress (`5ab5c0f`, `e1efab6`).
- **The installer's game patch targeted the wrong build.** In the real
  window on the clean Windows the whole content flow ran (textures adopted,
  runtime 1.0.1 and the roster installed) and then the Builder refused:
  "This ISO has no Reborn data directory". By hash, the Codex bundle's
  `output_sha256` is `78a987c7…` = `Games\NSR.iso`, an old build, not
  `057ef020…` = `NFL Street 2 FIELDS 12-19.iso`, the release candidate the
  Builder accepts. Rebuilt with `Tools\create-game-patch.py --target
  "…\NFL Street 2 FIELDS 12-19.iso"` into
  **`C:\Reborn Origin-stage\game-import-bundle-fields-12-19`** (the delta
  is 98.7 MB, not 687 MB; the installer shrinks by ~590 MB). Proven on this
  PC: the import example lands `057ef020…` from the stock nkit and the
  Builder applies the roster to it. **That folder is now the `-GamePatch`
  source, and `057ef020…` is the base every game-patch chain must start
  from.** The Codex bundle is superseded.
- **Shipped again 2026-09-08 16:20 with that bundle:** sandbox RESULT: PASS
  (54 checks), zip 2,141,023,049 bytes on the bucket, verified over HTTPS.
  The Update exe and `latest.json` are unchanged from 14:35 (sha `452b1466…`).

**Shipped 2026-09-08 02:28 from `bf9590c`:** `test-all` 19 of 19 on the
ship tree, Windows Sandbox RESULT: PASS (53 checks) with WebView2
152.0.4191.66 present offline (copy at
`target\dist-old-2026-09-06\sandbox-result-ship-bf9590c-2026-09-08.txt`),
then the zip (2,729,209,564 bytes), the Update exe (39,064,978) and
`latest.json` uploaded to `launcher/` and verified over HTTPS; `rebornctl
update check` from the ship build reads the hosted manifest and answers
up to date. This set supersedes the 22:16 one on the bucket.

### 4. DONE 2026-09-06 22:16: the final installer, built and proven

Built from commit `0a69415` with:

```
.\installer\windows\build.ps1 -Version 3.0.0 -OfflineWebView2 -OfflineSuffix "" `
  -GamePatch "<Codex>\work\game-patch\bundle" -Runtime "<Codex>\work\installer-input\runtime" `
  -Textures "%APPDATA%\NFL Street Reborn\User\Load\Textures\GN7E69" `
  -PromptPackages "<Codex>\work\installer-input\content\prompt-packages"
.\installer\windows\build.ps1 -LauncherOnly -Version 3.0.0
```

(`<Codex>` = `C:\Users\Adam\Documents\Codex\2026-09-04\c-users-adam-desktop-reborn-installer`.)
Textures are the 8,192-file tree the origin's `textures 1.0.0` was published
from, so a fresh install is current at its first check. `-OfflineWebView2`
bundles the 246.5 MB runtime: the 1.7 MB bootstrapper worked in the sandbox
on 09-04 and failed in every run on 09-06, and a blank first window is not a
risk worth 250 MB.

In `dist`: `NFL Street Reborn Launcher 3.0.0.zip` (2.54 GB, the exe and two
slices), `NFL Street Reborn Launcher Update 3.0.0.exe` (37.2 MB) and
`latest.json`. `test-all`: 19 of 19 on this set.

**Observed in Windows Sandbox, 22:11 to 22:16, RESULT: PASS (53 checks):**
full install over a fake 2.1 install in under four minutes, WebView2
152.0.4191.62 present with no network, runtime, textures, import bundle,
Builder, shortcuts, memory card untouched, launcher up; the Update exe over
it kept everything and relaunched the launcher; uninstall left the saves;
the beta side by side. Copy of the result:
`target\dist-old-2026-09-06\sandbox-result-final-offline-2026-09-06.txt`.

**Uploaded 22:17 and verified from outside:** all three objects answer
200 at `https://files.nflstreetreborn.com/launcher/` with the exact sizes
(zip 2,729,087,836; Update exe 39,045,742; latest.json 554, served
DYNAMIC), and `rebornctl update check` with no arguments reads the hosted
manifest and answers "Up to date at 3.0.0". The self-update path is live
end to end; the next launcher release is `build.ps1 -LauncherOnly
-Version 3.0.1` and the two rclone lines in the guide.

Still Adam's: point the site's Download button at
`https://files.nflstreetreborn.com/launcher/NFL%20Street%20Reborn%20Launcher%203.0.0.zip`.
Never at a file on SiteGround. Adam's manual walkthrough of the wizard
(`Tools\test-all.ps1 -Manual`) has not been done on this set.

### 5. Adam's own list

- `rotation.key` is still beside `publisher.key` in
  `C:\Users\Adam\Reborn Keys`. USB stick or paper, then delete it from the PC.
- Back up `publisher.key` somewhere that is not this machine.
- Launch day: the site's Download button points at the bucket.

## Commands that matter

```
cd "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher"
.\Tools\test-all.ps1                      # everything local, ~1 min
.\Tools\test-all.ps1 -Sandbox             # plus the install in Windows Sandbox
py -3 Tools\e2e-update.py                 # the update pipeline, 12 steps
py -3 Tools\e2e-roster.py                 # roster apply on the real image

target\release\rebornctl.exe publish roster --from https://nflstreetreborn.com/rdatabase/api/download.php?file=rebornroster --origin "C:\Reborn Origin" --version <date> --key "C:\Users\Adam\Reborn Keys\publisher.key" --apply
<rclone> --config "C:\Users\Adam\Reborn Keys\rclone.conf" copy "C:\Reborn Origin" r2:reborn-content --exclude release.json --exclude release.json.sig
<rclone> --config "C:\Users\Adam\Reborn Keys\rclone.conf" copyto "C:\Reborn Origin\release.json.sig" r2:reborn-content/release.json.sig
<rclone> --config "C:\Users\Adam\Reborn Keys\rclone.conf" copyto "C:\Reborn Origin\release.json" r2:reborn-content/release.json
```

`<rclone>` is
`%LOCALAPPDATA%\Microsoft\WinGet\Packages\Rclone.Rclone_Microsoft.Winget.Source_8wekyb3d8bbwe\rclone-v1.75.1-windows-amd64\rclone.exe`
(not on PATH in every shell). Release files always go up last.

## Traps already paid for

- The site's live database is **MySQL**; the local replica is MariaDB.
  `ADD COLUMN IF NOT EXISTS` fails live. Check, then ALTER.
- `rclone lsd r2:` is 403 with a bucket-scoped token; list inside the bucket.
  `no_check_bucket = true` is required and is set. Check a config file ends
  with a newline before appending to it.
- In Windows PowerShell 5.1 never `2>&1` or `*>>` a native command
  (cargo, node, nslookup, winscp): every stderr line becomes an
  ErrorRecord and a `Stop` preference kills the script. Bash for reads;
  WinSCP only from PowerShell; patch scripts written as files, never
  heredocs.
- `https://nflstreetreborn.com/` answers **503 by design** (launch gate
  countdown until Sept 12). `/rdatabase/login.php` 200 is the liveness check.
- `textures 1.0.0` ships `1ALL NUMBERS/texconv.log` (2.3 MB of bake log).
  Delete it from `C:\Reborn Origin-stage`'s source tree before the next
  textures publish; the update will retire it on every install for free.
- Adam's own data root already holds 7,562 of the 8,192 texture files
  byte-for-byte, so a first textures install there fetches 630 files
  (437 MB), not 3.18 GB. A fresh player fetches everything once.
- A planned mono-repo (`C:\Games\Reborn Website Files`, its own CLAUDE.md,
  SFTP 18765, sync `public_html` only) does not exist on this machine yet.
  When it does, its `rdatabase/` copy must be taken from the server AFTER
  today's token change, or a sync would revert it.

## Where the detail lives

- `docs/RUNNING_UPDATES.md`: the publishing guide, start to finish.
- `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`: the design.
- Memory: `project_reborn_update_system.md`,
  `project_nsr_site_repo_conventions_2026_09.md`.
