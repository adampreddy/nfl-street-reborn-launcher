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

**Pulled today, on purpose:** the full modded image (`offline-content`).
The product moved to the patch route: the installer ships a 687 MB
`reborn.xdelta` and the player supplies their own disc. A downloadable
modded game on a public URL had no place in that. Copies are kept at
`C:\Reborn Origin-stage\pulled\` if ever needed.

## The order of work

### 1. Land the other session's launcher work (blocks everything below)

68 uncommitted files in the launcher repo from the other session: the UI
rebuild (`reborn-ui/*`, `app-shell.js`, `launcher.css`, `update-notice.js`,
`controller-test.js`), `build.ps1`'s `-GamePatch` / `PromptPackages` model,
`game_import.rs` (untracked), `settings.rs`, `lib.rs` (`reshade`),
`Tools/test-all.ps1` (`-Browser`), browser tests, three design docs. Nothing
of theirs is committed. Until it is, do not edit under it. Have that session
commit, or Adam says "take it over".

Three things to raise with that session, found while reading their work:

- **A game update needs the disc again.** Import discards the player's
  original after patching. A new Reborn build (new xdelta from the stock
  disc) would need the disc re-inserted, or a retained 1.4 GB source. The
  cheaper design: build-to-build xdeltas (measured 1.1 MB between
  consecutive builds) applied to the image the player already has. That
  needs the import to stay pristine at `library/imported/<hash>/NSR.iso`
  and the roster to be applied into a copy Dolphin boots, not in place.
- **`find_game_image` in `session.rs` must look in `library/imported`**, or
  Play will not find what onboarding built. `roster::find_imported_image`
  already does (commit 129a668).
- **`game_import.rs` uses `std::process::Command` and `#[cfg(windows)]`**
  outside `platform/`, which the core forbids; `ProcessHost::run_capturing`
  is the seam.

### 2. Three launcher items, once 1 lands

1. **The window checks for content updates on its own** when
   `automatic_updates` is on: fetch release, plan each component, download
   in the background, promote only when the game is closed. Copy:
   "Update ready. N MB. Installs the next time you close the game." The
   launcher self-update notice (`update-notice.js`) is theirs and already
   does this shape for the installer; the content flow mirrors it.
   Core side: move the six steps from `rebornctl update content`
   (`crates/reborn-cli/src/main.rs::update_content`) into a core
   `content::run_update` so the window calls one function.
2. **A one-file installer build** for launcher self-update. `latest.json`
   pins one file; `build.ps1` writes it only for a single-file build, and
   the first-run installer is spanned. Needs a build mode that leaves the
   big content out (players get that through the content system).
3. **Ship `reborn-builder-cli.exe`** beside Dolphin (`build.ps1`). Today
   the launcher finds it in `<install>\tools`, beside the launcher, or in
   `%APPDATA%\NFL Street Reborn\Tools`.

### 3. The game-patch component (small game updates)

A `game-patch` content component carrying a build-to-build xdelta plus the
expected output hash, applied with the bundled `xdelta3.exe` the way
`game_import.rs` does, then the current roster re-applied. Depends on the
layering decision in item 1. About a day. Removes the only case where a
player would ever download hundreds of megabytes again.

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
- A planned mono-repo (`C:\Games\Reborn Website Files`, its own CLAUDE.md,
  SFTP 18765, sync `public_html` only) does not exist on this machine yet.
  When it does, its `rdatabase/` copy must be taken from the server AFTER
  today's token change, or a sync would revert it.

## Where the detail lives

- `docs/RUNNING_UPDATES.md`: the publishing guide, start to finish.
- `docs/architecture/CONTENT_AND_UPDATE_MODEL.md`: the design.
- Memory: `project_reborn_update_system.md`,
  `project_nsr_site_repo_conventions_2026_09.md`.
