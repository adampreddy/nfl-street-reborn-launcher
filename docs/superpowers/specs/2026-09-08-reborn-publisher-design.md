# Reborn Publisher: design

Date: 2026-09-08. Status: approved in conversation, awaiting Adam's review of this text.

## 1. Purpose

One desktop window from which Adam controls everything a player's launcher
can receive: the roster, the game build, the Dolphin runtime, the texture
pack, mods, the launcher program itself, the full first-run installer, and
rollbacks. It also does the Reborn Builder's job (roster into ISO), so the
separate Builder window is no longer needed for day-to-day work.

Today every one of these is a console command (`rebornctl publish ...`,
`reborn-builder-cli`, `build.ps1`, `rclone`) plus a runbook
(`docs/RUNNING_UPDATES.md`). The commands are proven; the control surface is
not usable without the runbook open. The Publisher is that runbook turned
into a program that refuses to do the wrong thing.

## 2. Decisions already made

| Question | Decision |
|---|---|
| Who uses it | Adam only, on this PC. The signing key never leaves it. |
| What it must do | Everything update-related: roster push, launcher update, game patch, textures, runtime, mods, rollback, full installer set, keys and tokens, and the Builder's ISO work. |
| Where it lives | A separate desktop app, `crates/reborn-publisher`, a fourth crate in this workspace. Not a page in the launcher, not the Python Builder window. |
| Insides | Fully native: publishing logic moves from `reborn-cli` into `reborn-core`; the CLI and the app both call it. Child processes only for tools that are external programs. |
| Windows Sandbox | Not part of any proof. Proofs are suites, manifest checks, HTTPS read-back and seeded scratch-root checks. |

## 3. Architecture

### 3.1 `reborn_core::publish` (new module, behind the existing `publish` feature)

Everything in `crates/reborn-cli/src/publish.rs` moves here and stops
printing. Each operation is a function that takes a request struct, a
`&mut dyn FnMut(Event)` progress sink, and returns a typed result or a typed
error. The `publish` feature already gates the Ed25519 signer and the random
source; the player launcher never links this module.

Functions and what they own:

- `keygen(out_dir)`: the publisher and rotation keys, once.
- `component(req)`: hash a tree into the origin (manifest plus blobs), as
  today, plus a **runtime lint** (section 5.6) when the component is the
  runtime.
- `release(req)`: assemble and sign `release.json` from pinned components.
  Verifies every pinned manifest round-trips and every blob exists at its
  published length, writes, reads back, verifies with the compiled-in keys.
- `roster(req)`: fetch the site export (token from the vault) or take a
  local `.rebornroster`, publish the component, re-sign a release carrying
  every other pin forward.
- `game_patch(req)`: xdelta from the base players have to the new build,
  decode-back check, append to the chain, publish, re-sign.
- `rollback(req)`: a release whose pins name older versions. Same code path
  as `release`; the app only chooses the pins.
- `next_sequence(origin, bucket)`: the higher of the origin's and the
  bucket's last sequence, plus one. Never typed by hand.
- `status(origin, hosted)`: the local origin, the hosted `release.json` and
  `launcher/latest.json`, and the difference between them (section 5.1).
- `upload(plan)`: native upload to the bucket (section 3.3).
- `journal`: append one record per operation to
  `<origin>\publisher-journal.jsonl`; read it back for History.

`Event` carries: a stage name, a human line, optional progress
(done bytes, total bytes, done files, total files), and an optional typed
payload (a hash, a sequence, a path). The CLI prints events; the app
streams them to its window. One code path, two front ends.

`rebornctl publish` keeps every subcommand and flag it has now. Its body
becomes: parse, build the request, call the core, print events, map the
result to an exit code. The e2e scripts that drive the CLI are unchanged and
are the regression net for the move.

### 3.2 `crates/reborn-publisher` (new crate)

A Tauri 2 window in the launcher's style (`app-shell.js`, tokens, dark).
Pages: **Status**, **Roster**, **Game** (Builder and patch), **Launcher**,
**Content** (textures, runtime, mods), **History** (and rollback),
**Settings** (paths, keys, vault). Commands are thin: build a request from
the page's fields, call the core on a worker thread, forward events to the
page, return the typed result. No publishing rule lives in the page.

Every page shares one component: the **operation panel**, with the three
steps of section 4 (Plan, Run, Verify), a live log pane fed by events, and a
dry-run checkbox.

### 3.3 Native upload

The bucket is Cloudflare R2, addressed through its S3 API. The core signs
requests itself (AWS SigV4, region `auto`) with the access key and secret
from the credential vault. `ureq`, already a dependency, carries the bytes.

- Objects up to 256 MB go in one `PUT`, verified by the returned ETag
  (MD5 for a single-part upload) and a `HEAD` for the length.
- Larger objects (the 2 GB zip) use multipart: 100 MB parts, each retried
  up to three times on a transport error, then `CompleteMultipartUpload`;
  a `HEAD` for the length and a ranged `GET` of the first and last 1 MB
  compared to the local file. A failed multipart is aborted so no parts
  linger.
- **Order is enforced by the plan type, not by convention.** An upload plan
  has three ordered groups: blobs, component manifests, release files
  (`release.json`, then `release.json.sig`), or for the launcher: the exe,
  then `latest.json`. The uploader cannot be handed the groups in another
  order; it takes the plan.
- Read-back: after the last object, the core fetches `release.json` (or
  `latest.json`) over the public HTTPS host, not the S3 endpoint, and
  compares bytes. Edge caching does not apply to those files.

`rclone` remains documented as the manual fallback; the app does not run it.

### 3.4 Child processes

Only programs that are external tools:

- `reborn-builder-cli.exe` (Python, PyInstaller): ISO build, `dump`,
  `card-sync`. Found beside the launcher's tools folder or by Browse.
- `xdelta3.exe`: making a game patch. Found in the import bundle.
- `cargo` and Inno Setup's `ISCC.exe`, through `installer\windows\build.ps1`,
  for launcher builds.

Each runs through the core's existing `ProcessHost`, output streamed line
by line into the log pane, exit code mapped to a typed error. The app never
parses tool output for a decision; it reads the files the tool produced
(a manifest, a hash it computes itself, an exit code).

### 3.5 Secrets

- `publisher.key` and `rotation.key` stay files. Settings holds the path of
  `publisher.key`; the app opens it at signing time and drops it after.
  Never displayed, never logged, never copied into the app's data.
- The site API token, the R2 access key id and the R2 secret live in the
  Windows credential vault through `platform::CredentialVault`, entries
  `nsr.site.export`, `nsr.r2.access`, `nsr.r2.secret`. Settings has one
  field per secret that writes to the vault and shows only "stored" or
  "missing". The first run offers to import the values from
  `C:\Users\Adam\Reborn Keys\rclone.conf` and then leaves that file alone.
- The log pane and the journal redact anything that came from the vault or
  the key file.

## 4. The shape of every operation

1. **Plan.** The app reads the origin and the bucket and shows exactly what
   will change: sequence before and after, pins before and after, files to
   upload with sizes, tools it will run, and one sentence on what a player
   will see ("an ask-first content update of 31 MB", "a launcher update to
   3.0.1", "nothing until the download page changes"). Nothing is written.
2. **Run.** One click. Local writes first (origin, journal), upload second,
   in the enforced order. Progress and the log stream live. A failure stops
   at the step that failed and says what is now true on disk and on the
   bucket.
3. **Verify.** Read back from the public host; then a **seeded scratch-root
   check**: the core creates a scratch data root, seeds it from a chosen
   install payload when one is configured, runs the same check a player's
   launcher runs against the live bucket, and reports the plan per
   component. Only then does the panel say done, and the journal records
   the verify result.

The dry-run checkbox runs Plan and Verify's local parts and skips every
write and upload.

## 5. Operations

### 5.1 Status (the opening page)

- **Live**: release sequence and pins with versions; launcher version, size
  and hash from `latest.json`; the download link's target from
  `launcher/download.json` (section 5.5). The site gate moment
  (2026-09-12 17:00 America/New_York) shown as a countdown until it passes.
- **Local origin**: components and releases present in `C:\Reborn Origin`
  that the bucket does not serve, marked pending, with an Upload button.
- **This machine**: each tool and input the operations need, found or
  missing, with Browse: `publisher.key`, the Builder, xdelta, cargo, ISCC,
  the R2 and site secrets (stored or missing), the FIELDS 12-19 base ISO
  (`057ef020...`) and the game-import bundle, the runtime and texture
  inputs for a full build.
- **Recent**: the last ten journal records.

### 5.2 Roster push

Inputs: the site export (default: fetched with the vaulted token from the
export URL the CLI uses today) or a `.rebornroster` file. Version defaults
to today's date, editable. Plan shows the roster's player and team counts
read from the file. Run publishes the component, signs the release with the
next sequence, uploads, verifies. Verify's scratch-root check must show the
roster as the pending component at its size and every other component at
zero fetches.

### 5.3 Game: Builder and patch

**Build ISO**: base ISO, roster file, output path. The app runs the Builder
CLI and shows its warnings in a capped, scrollable list with a single
"Build anyway" button that never moves (the Builder's own hard-won rule).
The result's hash is computed by the app and shown. Also here: `dump`
(ISO to `.rebornroster`) and `card-sync` (memory card gear to a roster).

**Game patch**: the new build's ISO. The app computes its hash, checks the
chain's base is the base players have (`057ef020...` today, read from the
chain, not hard-coded), runs xdelta, decode-back verifies, appends the step,
publishes the chain component, signs the release, uploads, verifies. Refuses
a base that is not the chain's tip, with the two hashes shown.

### 5.4 Launcher update

Input: the new version (proposed: current plus a patch bump). Plan shows
the version the bucket serves now and the commit the build will use. Run
executes `build.ps1 -LauncherOnly -Version <v>`, computes the exe hash,
compares it with the `latest.json` the build wrote, uploads the exe then
the manifest, and verifies by reading both back over HTTPS and asking the
core's update check the way a launcher still at the previously hosted
version would; it must answer that <v> is available. The plan warns when
the version equals the hosted one: no launcher would be offered anything.

### 5.5 Full installer set

Inputs: the game-import bundle, the runtime input, the texture input, the
prompt packages, all remembered from the last run. Run executes the full
`build.ps1` (spanned set, zip, offline WebView2), uploads the zip, then
writes `launcher/download.json`:

```
{ "schema": "nsr.download.v1", "version": "3.0.0", "url": "<zip url>",
  "bytes": 2161338617, "sha256": "<hex>", "published": "<iso time>" }
```

The site's download button reads that file (cached a few minutes) instead
of a constant in `_site/shell.php`. That is a one-line site change plus a
fetch, made once; after it, a new set never needs a site deploy. Until the
site change ships, the app shows the URL to paste.

### 5.6 Content components: textures, runtime, mods

Input: a folder, the component, a version (proposed by patch bump), the
sequence (computed). Run publishes and re-signs. **Runtime lint**, before
anything is written: the tree is refused if it contains `ReShade.log*`,
`ReShade.ini.launcher-original`, `ReShadePreset.ini` with content, a `User`
folder, or a `Dump` folder, with the offending paths listed. Those are the
marks of a runtime the launcher has started, and runtimes 1.0.0 and 1.0.1
were both published that way and both offered players a pointless update.

### 5.7 Rollback

History lists every release the origin has signed and every component
version it holds. Pick a component and an earlier version; the app shows
the current pins, the proposed pins, and the size players would fetch (the
launcher keeps replaced files, so a rollback is usually small). Run signs a
new release with the next sequence and uploads it. Sequence goes up,
content goes back.

### 5.8 Keys and vault

Generate the two keys once (refused if they exist). Show which public keys
the shipped launcher trusts (`signing::TRUSTED_KEYS`) and whether the key
on disk matches one of them. Vault fields for the three secrets. A reminder
line when `rotation.key` is on the same disk as `publisher.key`.

## 6. Bookkeeping

- Journal: `<origin>\publisher-journal.jsonl`, one JSON object per line:
  time, operation, inputs (paths and versions, never secrets), sequence,
  hashes, upload list, verify result, dry-run flag. History reads it.
- Sequences: read from the origin's newest manifest and release and from the
  bucket's; the app uses the higher plus one and shows both sources.
- Versions: proposed (roster by date, others by patch bump), editable, and
  refused if the origin already holds that version of that component.

## 7. Safety rules, enforced in code

- No operation reads or writes a real data root. Scratch roots are created
  under the app's temp folder and deleted after Verify.
- Upload order is a property of the plan type (section 3.3).
- The signing key is opened only inside `release` and never cached.
- A release is never overwritten in place on the bucket without a higher
  sequence; the uploader refuses a plan whose release sequence is not above
  the hosted one.
- Rollback, overwriting a component version, and deleting anything each
  show before and after and need their own click.
- Dry run is available on every operation.
- The runtime lint (5.6) and the chain-base check (5.3) refuse before any
  write.

## 8. Testing

- The existing core tests of publishing move with the code. The CLI keeps
  its flags, so `Tools/e2e-update.py` (13 steps), `Tools/e2e-roster.py` and
  `Tools/e2e-game-patch.py` run unchanged and are the regression net for
  the move. They must pass before any app code is written.
- Native upload: unit tests for SigV4 against the published AWS test
  vectors; a loopback S3 stand-in in `Tools/` that checks signatures,
  records the order objects arrive, and serves them back; one real run
  against the bucket under a `test/` prefix that the run deletes.
- The app's pages: the launcher's checkers, adapted (every page call sends
  what its command declares; every page survives every answer, including a
  failure mid-upload and a missing tool).
- `Tools/e2e-publisher.py`: a fresh origin, the loopback bucket, a loopback
  content host; then roster push, launcher update (with a stub build),
  game patch, rollback; each verified from a seeded scratch root.
- Visual: one rendered capture per page state, looked at, as the Builder's
  UI work established.

## 9. Delivery

- `installer\windows\build.ps1 -Publisher -Version <v>` produces
  `Reborn Publisher Setup <v>.exe`, one file, installed only on this PC.
  Not on the bucket, not in the launcher, not in the player zip.
- The Publisher's own version follows the workspace version.

## 10. Order of work

1. Move publishing into `reborn_core::publish` with events; rewire the CLI;
   all three e2e scripts green. Nothing visible changes.
2. Native upload and `status`, with the loopback stand-in and the real
   `test/` prefix run.
3. The app crate with the operation panel and the Status page.
4. Pages in the order of use: Roster, Launcher, Game (Builder, then patch),
   Content, History and rollback, Keys.
5. Full installer set and `download.json`, with the site change.
6. The Publisher installer.

## 11. Out of scope

- Publishing from any other machine, or from the website.
- Replacing the Python Builder pipeline with Rust. The app drives it.
- Player-facing UI. GPTAstra's update center in the launcher is untouched.
- Changing what a player's launcher fetches or how; the release format,
  the component manifests and the chain format are unchanged.
