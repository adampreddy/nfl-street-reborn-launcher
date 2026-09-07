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
   install folder is honored. Publish: the exe and `latest.json` on a GitHub
   release tagged `v<v>` (`-UpdateBase <url>` for another host). Guide 7c.
   Proof: `test-all` checks the pin; the sandbox smoke test installs the
   full set and then the update over it (result recorded below).
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

### 4. The final installer

The one in `dist` is from 2026-09-05 00:21 and predates the roster
component, the zero-byte fix, the relaunch fix, and the entire UI rebuild.
After items 1 and 2: build with `-GamePatch`, run `Tools\test-all.ps1
-Sandbox` and Adam's manual walkthrough, upload the zip to the bucket,
point the site's `DOWNLOAD_URL` at `files.nflstreetreborn.com`. Never at a
file on SiteGround.

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
