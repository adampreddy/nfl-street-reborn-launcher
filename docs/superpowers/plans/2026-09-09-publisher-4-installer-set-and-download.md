# Reborn Publisher, plan 4 of 4: the full installer set and the download link

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The last operation the Publisher is missing: build the full first-run set for new players, upload it, and point the website's download button at it without a manual deploy ever again.

**Architecture:** One more page section in the app driving the existing `installer\windows\build.ps1` full build, then plan 2's uploader, then a small `launcher/download.json` on the bucket that the league website reads instead of a constant in its PHP. One change outside this repository, made once: the site fetches that file.

**Tech Stack:** As plans 2 and 3. The site change is PHP in the `nsr` site repository, deployed the way that repository's own conventions require (one `put` per file, read back and compare, `src/Config.php` never uploaded).

**Spec:** `docs/superpowers/specs/2026-09-08-reborn-publisher-design.md`, section 5.5, plus section 9 for delivery.

**Predecessors:** plan 2 (upload, status, journal) and plan 3 (the app, its operation panel and its pages).

## Global Constraints

- Branch: `git checkout -B publisher-set main` after plan 3 merges.
- The zip is about 2 GB and the whole set about 2.2 GB. Nothing may hold it in memory; the uploader streams from disk (plan 2's `Body::File`).
- **The site is live.** Any site change is deployed the way that repository requires and read back and compared before it is called done. If the site cannot be reached or its credentials are absent, build everything else and say plainly that the site half is not done, rather than claiming it.
- The Publisher installer is never uploaded to the bucket and never shipped to a player.
- US English, no em dashes.
- Commit trailers:
  `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`
  `Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK`

## File structure

| File | Responsibility |
|---|---|
| `crates/reborn-core/src/publish/download.rs` (create) | `DownloadPointer`, write it, upload it, read it back. |
| `crates/reborn-publisher/src/set.rs` (create) | The full-set commands. |
| `crates/reborn-publisher/ui/set.js`, `ui/set.html` (create) | The page. |
| `crates/reborn-publisher/src/main.rs` (modify) | Declare and register the new commands. The one place plan 3 said not to touch; this plan is the exception and it is a single edit. |
| `Tools/check-publisher-pages.mjs` (modify) | Cover the new page. |
| The `nsr` site repository, `_site/shell.php` | Read `download.json` instead of a constant. |

---

### Task 1: The download pointer

**Files:** `crates/reborn-core/src/publish/download.rs`, `crates/reborn-core/src/publish/mod.rs` (one line)

```rust
pub const DOWNLOAD_FILE: &str = "launcher/download.json";
#[derive(Serialize, Deserialize, PartialEq)]
pub struct DownloadPointer {
    pub schema: String,     // "nsr.download.v1"
    pub version: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
    pub published: String,  // RFC 3339
}
pub fn pointer_for(zip: &Path, version: &str, public_base: &str, now: SystemTime) -> Result<DownloadPointer>;
pub fn read_hosted(http: &dyn Http, public_base: &str) -> Result<Option<DownloadPointer>>;
```

`pointer_for` hashes the zip and builds the URL by percent-encoding the file name onto `public_base` + `launcher/`, matching how `latest.json`'s URL is built today so the two cannot disagree. `read_hosted` returns `None` for a 404, which is the honest answer before the first publish.

**Tests:** a pointer built from a temp file has that file's real size and hash; the URL encodes a space as `%20`; `read_hosted` returns `None` on 404 and `Err` on malformed JSON; a round trip through `serde_json` is identical.

**Commit:** `core publish: the download pointer the site reads`.

---

### Task 2: The full set page

**Files:** `crates/reborn-publisher/src/set.rs`, `crates/reborn-publisher/ui/set.js`, `ui/set.html`, `crates/reborn-publisher/src/main.rs`, `Tools/check-publisher-pages.mjs`

**Commands:** `set_plan()`, `set_run(dry_run)`, `set_verify()`.

**Behavior.** Inputs, all remembered from the last run and each with a Browse: the game-import bundle (default `C:\Reborn Origin-stage\game-import-bundle-fields-12-19`), the runtime tree, the texture tree, the prompt packages. Plan shows each input with its size and file count, the version to build, what the bucket serves now, and a warning when a version equal to the hosted one is about to replace it.

Run executes the full `build.ps1` with `-GamePatch -Runtime -Textures -PromptPackages -OfflineWebView2 -OfflineSuffix ""`, streaming its output into the panel. The build takes about twenty minutes and produces a spanned set plus a zip; the panel must stay responsive and show progress throughout. Then it uploads the zip through plan 2's uploader and writes and uploads `launcher/download.json` **last**, after the zip, so the site never points at a file still in flight.

**Two guards the plan must enforce**, both learned the hard way:
- The build's staging folder is deleted before staging. If the `-GamePatch` bundle is inside `target\installer-payload`, the build would delete its own input. `build.ps1` already refuses this; surface that refusal in the panel rather than letting it look like a build failure.
- The zip is packed from an exact file set, the setup exe plus its numbered `.bin` slices for this version only. A wildcard once swept another build's slices into a 5 GB zip over a good one. The plan step must list exactly the files that will go into the zip and their sizes, so a wrong set is visible before the run.

Verify reads `download.json` and the zip's headers back over the public host, compares the pointer's hash and size against the local zip, and reports the download link the site will render.

**Commit:** `publisher: build, upload and point the download link at the full set`.

---

### Task 3: The site reads the pointer

**Files:** the `nsr` site repository, `_site/shell.php`

Replace the constant download URL with a fetch of `https://files.nflstreetreborn.com/launcher/download.json`, cached a few minutes, falling back to the last known good URL if the fetch fails so the button never disappears. The release gate that decides when the button appears is unchanged: it still opens at 2026-09-12 17:00 America/New_York by the site's own rule.

**Deploy it live**, as that repository requires: one `put` per file from PowerShell, then read the file back and compare its sha256. Then load the page and confirm the button renders with the URL the pointer names.

**If the site's credentials are not available in this environment, do not fake it:** leave the PHP change committed locally, say clearly in the report and to Adam that the site half is not deployed, and print the exact command to run.

**Commit:** in the site repository, its own conventions.

---

### Task 4: Close the loop

**Files:** `docs/RUNNING_UPDATES.md`, `docs/NEXT_STEPS.md`, `Tools/test-all.ps1`

Rewrite section 7c and the hosting steps of `RUNNING_UPDATES.md` so they describe the Publisher as the way these things are done, with the console commands kept as the fallback for when the app cannot run. Add a `test-all.ps1` check that `download.json`, when present in `dist`, pins the zip beside it by hash, size and name, the way the existing `latest.json` check does.

**Commit:** `docs: the Publisher is how updates are published now`.

---

## Self-review

**Spec coverage:** 5.5 in full (Tasks 1 to 3), section 9 delivery (plan 3 Task 10 built the installer; this plan's Task 4 documents it). Nothing in the spec is left after this plan.

**Placeholders:** none.

**The honest limit:** Task 3 touches a live website outside this repository. It is written so that a missing credential produces a clear "not done" rather than a false claim, because a download button pointing at nothing is worse than a button that has not moved yet.
