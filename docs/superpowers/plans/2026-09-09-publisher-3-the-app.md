# Reborn Publisher, plan 3 of 4: the app

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The window Adam opens to control every update: seven pages over `reborn_core::publish`, each operation running plan, run, verify, with the Reborn Builder's ISO work folded in.

**Architecture:** A fourth workspace crate, `crates/reborn-publisher`, a Tauri 2 app in the launcher's style. Task 1 lays the whole skeleton down at once: the crate, the window, the shared page harness, the operation panel, and a stub Rust file per page with every command signature already registered in `main.rs`. After that each page is one task that fills in its own two files and nothing else, so the pages can be built in parallel with no possibility of collision.

**Tech Stack:** Rust 2021, Tauri 2, `reborn-core` with the `publish` feature, `rfd` for file pickers (as `reborn-ui` uses); plain HTML, CSS and ES modules, no framework and no npm at runtime; the existing Node checkers under `Tools/` for the pages.

**Spec:** `docs/superpowers/specs/2026-09-08-reborn-publisher-design.md`, sections 3.2, 3.4, 4, 5.1 to 5.8, 7, 8.

**Predecessors:** plan 1 (merged, `0cf3996`) put publishing in `reborn_core::publish`. Plan 2 adds `upload`, `status` and `journal` to it. Both ledgers carry deferrals this plan owns: the blob-copy progress is text only and nothing is cancellable past the hashing phase (recorded when the request shapes froze), and the journal's records are written by this plan's call sites.

## Global Constraints

- Branch in the existing worktree: `git checkout -B publisher-app main` after plan 2 merges. Another agent edits the main tree; never `git add -A`.
- **Every rule about publishing lives in the core, never in a page.** A page reads a command, draws what came back, and sends what a person chose. No page computes a sequence, decides an order, or parses tool output.
- **Every operation is plan, run, verify** (spec section 4), through the one shared operation panel. Plan writes nothing. Verify reads back from the public host and then runs a seeded scratch-root check. Only then does the panel say done.
- **No operation may touch a real data root.** Scratch roots go under the app's temp directory and are deleted after verify.
- **Secrets never render.** The signing key is a path; the vault fields show "stored" or "missing" and nothing else. No page ever displays a token, a secret key, or a header value.
- Every page must survive every answer the core can give: a command that answers fully, one that answers with every field null, and one that rejects. This is what `Tools/check-shell-pages.mjs` already enforces for the launcher and what this plan's checker enforces here.
- Windows only for now, matching the launcher. US English, no em dashes, in every string a person reads.
- Commit trailers on every commit:
  `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`
  `Claude-Session: https://claude.ai/code/session_0162ntSTXPjrqYbCJ9aBnBmK`

## File structure

| File | Responsibility |
|---|---|
| `crates/reborn-publisher/Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/` | The crate and its window. |
| `crates/reborn-publisher/src/main.rs` | The Tauri builder, every module declaration, every command registered. Written once in Task 1 and not touched again. |
| `crates/reborn-publisher/src/settings.rs` | The app's own settings: origin path, key path, tool paths, remembered inputs. Its own file so pages read paths without each finding them. |
| `crates/reborn-publisher/src/ops.rs` | The one place an operation runs: takes a request, runs it on a worker thread, streams `Event`s to the window, writes the journal record, returns the outcome. Every page's command calls through this. |
| `crates/reborn-publisher/src/{status,roster,launcher,game,content,history,keys}.rs` | One file per page, holding that page's commands only. |
| `crates/reborn-publisher/ui/{index,roster,launcher,game,content,history,keys}.html` | One page each. |
| `crates/reborn-publisher/ui/{shell,panel,status,roster,launcher,game,content,history,keys}.js` | The harness, the operation panel, and one module per page. |
| `crates/reborn-publisher/ui/publisher.css` | The whole app's style, in the launcher's tokens. |
| `Tools/check-publisher-pages.mjs` | Every page survives every answer. |
| `Tools/review-publisher.mjs` | Interaction and layout checks in a real browser. |
| `Tools/e2e-publisher.py` | The whole thing end to end against loopback services. |

## Parallel batches

- **Batch A (serial, first):** Task 1, the skeleton. Everything else depends on it.
- **Batch B (serial, second):** Task 2, the Status page. It sets the pattern every other page copies, so it is built and reviewed before the rest fan out.
- **Batch C (parallel, six at once):** Tasks 3 to 8, one page each. Every task touches exactly two files, its own `src/<page>.rs` and its own `ui/<page>.js` (plus its `ui/<page>.html`, created in Task 1). No two tasks share a file.
- **Batch D (parallel):** Task 9 (the checkers), Task 10 (the installer).
- **Batch E:** Task 11, the end-to-end script, after everything.

---

### Task 1: The skeleton

**Files:** the whole crate listed above except the per-page `.js` bodies, plus `Cargo.toml` at the workspace root (add the member).

**What to build:**

1. **The crate.** `reborn-publisher`, version from the workspace, depending on `reborn-core` with `features = ["publish"]`, `tauri`, `serde`, `serde_json`, `rfd`. Copy `build.rs`, `capabilities/` and the `tauri.conf.json` shape from `crates/reborn-ui`, changing `productName` to `Reborn Publisher`, the identifier to `com.nflstreetreborn.publisher`, the window title to `Reborn Publisher`, and keeping the same CSP and `withGlobalTauri`.

2. **`settings.rs`.** The app's settings file at `<data>/publisher/settings.json` (use `Platform::data_root()`), holding: `origin` (default `C:\Reborn Origin`), `publisher_key`, `builder`, `xdelta`, `bucket_endpoint`, `bucket_name`, `public_base` (default `https://files.nflstreetreborn.com/`), `launcher_manifest` (default the launcher's `latest.json` URL), and a `remembered` map of the last inputs per operation. Commands `publisher_settings` and `publisher_set_setting`. Never stores a secret; the vault holds those.

3. **`ops.rs`, the heart.** One function every page command calls:

```rust
pub enum Phase { Idle, Planning, Planned, Running, Verifying, Done, Refused, Failed }
pub struct OpState {
    pub phase: Phase,
    pub operation: String,
    pub lines: Vec<String>,      // every Event line, in order
    pub summary: String,
    pub plan: Option<serde_json::Value>,   // what the plan step found
    pub dry_run: bool,
}
/// Run one operation on a worker thread. Events append to `lines` as they
/// arrive; the window polls `op_status`. Only one operation runs at a time.
pub fn run_operation(op: &str, dry_run: bool, body: impl FnOnce(Sink<'_>) -> Result<Outcome<serde_json::Value>> + Send + 'static);
```
Commands: `op_status`, `op_cancel` (sets the scan observer's cancel flag; note in the code that cancellation only reaches the hashing phase today, which plan 2's ledger recorded), `op_clear`. A second operation while one is running is refused, with the running one named.
After a run, `ops.rs` writes the journal record through `reborn_core::publish::journal::append`, filling `inputs` from the page's request with every secret already excluded by the request type.

4. **`shell.js`.** The page harness, copied in spirit from `crates/reborn-ui/ui/shell.js`: `bridge`, `call(name, args)` returning a promise that never throws into the caller (it resolves to `{ok, value|error}`), `el(tag, props, children)`, nav marking, and `fmtBytes`.

5. **`panel.js`, the operation panel.** One exported function `panel(host, {operation, plan, run, verify, describe})` that draws the three steps, a live log pane fed by polling `op_status` every 400 ms while a phase is active, a dry-run checkbox, and the buttons. It shows Run only after Plan has returned, and it shows the plan's own summary above the button. It never decides anything: `plan`, `run` and `verify` are command names it calls.

6. **`publisher.css`.** The launcher's tokens (read `crates/reborn-ui/ui/shell.css` and `product-ui.css`), a left nav with the seven pages, and a content column. Dark, plain, readable at 1280 and at 900 wide.

7. **Stub command files.** `status.rs`, `roster.rs`, `launcher.rs`, `game.rs`, `content.rs`, `history.rs`, `keys.rs`, each holding its page's command functions with the exact signatures listed in Tasks 2 to 8 below, each body `unimplemented!("Task N")`. `main.rs` declares every module and registers every command in one `generate_handler!`. **This is what makes the pages parallel: `main.rs` is complete after this task and no later task touches it.**

8. **The HTML pages**, one per page, each an empty content column with the nav and a `<script type="module" src="<page>.js">`. The `.js` files for pages other than Status are one-line stubs.

**Proof:** `cargo build -p reborn-publisher` succeeds. `cargo run -p reborn-publisher` opens a window with the nav and seven reachable pages, six of them empty. Take a screenshot into `target/publisher-shots/skeleton.png` and look at it.

**Commit:** `publisher: the crate, the window, the shared harness and the operation panel`.

---

### Task 2: Status, the opening page

**Files:** `crates/reborn-publisher/src/status.rs`, `crates/reborn-publisher/ui/status.js`

**Commands:**
```rust
#[tauri::command] fn publisher_status() -> Value;      // reborn_core::publish::status::read_status, serialized
#[tauri::command] fn publisher_environment() -> Value; // every tool and input: found or missing, with the path
#[tauri::command] fn publisher_recent(n: usize) -> Value; // journal::read_last
```

**The page draws four blocks**, in this order, from spec 5.1:
- **Live:** release version, sequence and each pin; the launcher version, size and hash; the download link's target. A line saying whether the hosted release's signature verified.
- **Local origin:** anything published locally the bucket does not serve, marked pending, with an Upload button that runs the upload operation through the panel.
- **This machine:** each of `publisher.key`, the Builder, xdelta, cargo, Inno Setup, the R2 credentials (stored or missing), the site token, the base image and the import bundle. Found shows the path; missing shows a Browse button that saves the path into settings.
- **Recent:** the last ten journal records, newest first, each one line.

**Tests:** the page must draw when `publisher_status` returns a status full of `problems` and nothing else, when every field is null, and when the command rejects. Nothing on the page may be blank without saying why.

**Commit:** `publisher: the Status page`.

---

### Task 3: Roster push

**Files:** `crates/reborn-publisher/src/roster.rs`, `crates/reborn-publisher/ui/roster.js`

**Commands:** `roster_plan(source, version)`, `roster_run(source, version, dry_run)`, `roster_verify()`.
`source` is either `{kind: "site"}` (fetch with the vaulted token from the export URL the CLI uses) or `{kind: "file", path}`.

**Behavior:** version defaults to today's date and is editable. Plan shows the roster's player and team counts read from the file, the sequence before and after, the pins before and after, and one sentence on what a player will see. Run calls `reborn_core::publish::roster::publish_roster` through `ops::run_operation`, then uploads through plan 2's `upload`. Verify reads back and then runs a seeded scratch-root check that must show the roster pending at its size and every other component at zero fetches.

**Commit:** `publisher: the Roster page`.

---

### Task 4: Launcher update

**Files:** `crates/reborn-publisher/src/launcher.rs`, `crates/reborn-publisher/ui/launcher.js`

**Commands:** `launcher_plan(version)`, `launcher_run(version, dry_run)`, `launcher_verify(version)`.

**Behavior:** version proposed as the hosted version with its patch bumped, editable. Plan shows what the bucket serves now, the version to build, and warns when they are equal because no launcher would be offered anything. Run executes `installer\windows\build.ps1 -LauncherOnly -Version <v>` through the core's `ProcessHost` with its output streaming into the panel, computes the produced exe's hash itself, compares it with the `latest.json` the build wrote, and uploads the exe then the manifest. Verify reads both back over HTTPS and asks the core's update check, as a launcher still on the previously hosted version, that the new version is offered.

**Commit:** `publisher: the Launcher page`.

---

### Task 5: Game, the Builder and the patch

**Files:** `crates/reborn-publisher/src/game.rs`, `crates/reborn-publisher/ui/game.js`

**Commands:** `game_build_plan(base, roster, out)`, `game_build_run(...)`, `game_dump(iso, out)`, `game_cardsync(card, roster)`, `game_patch_plan(to)`, `game_patch_run(to, version, dry_run)`, `game_patch_verify()`.

**Behavior, two sections on one page.**
*Build ISO:* pick a base image, a roster file and an output path; run `reborn-builder-cli.exe`; show its warnings in a capped scrolling list with a single "Build anyway" button that never moves whatever the warning count (this is the Builder's own hard-won rule, from a bug where 112 warnings pushed the button off screen); show the result's hash, computed by the app. Also `dump` and `card-sync`.
*Game patch:* pick the new build; the app hashes it, reads the chain's head, and refuses a base that is not the chain's tip with both hashes shown; then encode, decode-verify, publish, re-sign, upload, verify.

**Commit:** `publisher: the Game page, the Builder and the patch`.

---

### Task 6: Content, and the runtime lint

**Files:** `crates/reborn-publisher/src/content.rs`, `crates/reborn-publisher/ui/content.js`

**Commands:** `content_plan(component, folder, version)`, `content_run(...)`, `content_verify(component)`, `content_lint_runtime(folder)`.

**Behavior:** pick a component (textures, runtime, mods), a folder and a version (proposed by patch bump, refused if the origin already holds it). **The runtime lint runs before anything is written and refuses a tree carrying `ReShade.log*`, `ReShade.ini.launcher-original`, a non-empty `ReShadePreset.ini`, a `User` folder or a `Dump` folder**, listing the offending paths. Those are the marks of a runtime the launcher has started, and runtimes 1.0.0 and 1.0.1 were both published that way and both offered every player a pointless update. Say that in the refusal, in one sentence.

**Commit:** `publisher: the Content page, with the runtime lint`.

---

### Task 7: History and rollback

**Files:** `crates/reborn-publisher/src/history.rs`, `crates/reborn-publisher/ui/history.js`

**Commands:** `history_list(limit)`, `history_versions(component)`, `rollback_plan(component, version)`, `rollback_run(component, version, dry_run)`, `rollback_verify()`.

**Behavior:** the journal as a table, newest first, each row expandable to its full record. Below it, rollback: pick a component and an earlier version, see the current pins against the proposed pins and the bytes a player would fetch (usually small, because a launcher keeps replaced files), then sign a release with the next sequence and upload. Rollback needs its own click after the before-and-after is shown.

**Commit:** `publisher: History and rollback`.

---

### Task 8: Keys, the vault, and settings

**Files:** `crates/reborn-publisher/src/keys.rs`, `crates/reborn-publisher/ui/keys.js`

**Commands:** `keys_state()`, `keys_generate(dir)`, `vault_state()`, `vault_store(which, value)`, `vault_forget(which)`, `settings_browse(which)`.

**Behavior:** show which public keys the shipped launcher trusts (`signing::TRUSTED_KEYS`) and whether the key file on disk matches one of them. Generate refuses if either key file exists. Three vault fields (site token, R2 access key id, R2 secret), each showing only "stored" or "missing", each with a Store and a Forget. A one-line reminder when `rotation.key` sits in the same folder as `publisher.key`. Offer once, on first run, to import the R2 values from `C:\Users\Adam\Reborn Keys\rclone.conf`, and leave that file alone afterwards.
**A stored value is never read back into the page.** `vault_store` takes the value and returns only whether it stored.

**Commit:** `publisher: Keys, the vault and settings`.

---

### Task 9: The checkers

**Files:** `Tools/check-publisher-pages.mjs`, `Tools/review-publisher.mjs`, `Tools/test-all.ps1` (add both)

`check-publisher-pages.mjs`: load every page three times against a fake core that answers everything, answers with every field null, and rejects every call, and assert each time that the page draws without throwing and that no element is blank without an explanation. Follow `Tools/check-shell-pages.mjs`'s shape exactly.
`review-publisher.mjs`: a real browser, Playwright as the existing suites use. Assert the operation panel's contract on a fake core: Run is disabled until Plan returns; a dry run never calls the run command; a refusal shows its reason and leaves Run available; a failure mid-run leaves the log visible. Assert the Builder's warning list stays scrollable and the button stays in the viewport with 112 warnings. Capture one screenshot per page into `target/publisher-shots/` and **look at every one**.

**Commit:** `publisher: page checkers and a browser review`.

---

### Task 10: The installer

**Files:** `installer/windows/build.ps1`, `installer/windows/reborn-publisher.iss` (create)

`build.ps1 -Publisher -Version <v>` builds `reborn-publisher.exe`, stages it with `rebornctl.exe` beside it, and compiles one file, `Reborn Publisher Setup <v>.exe`, into `dist`. It is not on the bucket, not in the launcher, and not in the player zip. Add a `test-all.ps1` dist check that when a Publisher installer exists it is one file and carries both binaries.

**Commit:** `publisher: a one-file installer for the admin tool`.

---

### Task 11: End to end

**Files:** `Tools/e2e-publisher.py`

A fresh origin, plan 2's loopback S3 stand-in, and a loopback content host. Then, driving the app's commands through `rebornctl` where a command has a CLI equivalent and through a headless harness where it does not: roster push, launcher update with a stub build, game patch, and a rollback, each followed by the seeded scratch-root check. Assert the journal has one record per operation with the right outcome, and that no scratch root survives.

**Commit:** `publisher: the end to end proof`.

---

## Self-review

**Spec coverage:** 3.2 the crate and its pages (Tasks 1 to 8); 3.4 child processes only for external tools (Tasks 4, 5); 4 plan/run/verify through one panel (Task 1's `panel.js`, used by every page); 5.1 Status (Task 2); 5.2 roster (Task 3); 5.3 Builder and patch (Task 5); 5.4 launcher (Task 4); 5.6 content and the runtime lint (Task 6); 5.7 rollback (Task 7); 5.8 keys and vault (Task 8); 6 the journal written per operation (Task 1's `ops.rs`); 7 safety, scratch roots and per-click confirmation (Tasks 1, 7); 8 testing (Tasks 9, 11). Section 5.5, the full installer set and `download.json`, is plan 4.

**Placeholders:** none. Every task names its files, its command signatures and what its page must draw.

**Type consistency:** every command returns `serde_json::Value` shaped by the core's own types, so a page never re-derives a rule. `ops::run_operation` is the single path from a page to the core, and it is what writes the journal, so no page can run an operation that goes unrecorded.

**The one risk worth naming:** Task 1 fixes every command signature before the pages exist. If a page later needs an argument its signature does not carry, `main.rs` has to change and the parallel property breaks for that task. Mitigation: Tasks 2 to 8 list their signatures here, and Task 1 must implement exactly these, not its own idea of them.
