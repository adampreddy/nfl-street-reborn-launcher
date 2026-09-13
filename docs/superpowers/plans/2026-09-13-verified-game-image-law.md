# Verified Game Image Law Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pressing Play in Reborn boots a verified Reborn build, or it does not boot at all.

**Architecture:** One verification primitive in `reborn-core` (`game_identity`) returns a single verdict for any disc image. A launch-time gate and all three writers of `settings.game_path` route through it. Unrecognized NFL Street 2 discs are treated as a fault to repair, with a full-disc download as recovery, and Reborn's texture pack is switched off for any image it did not verify.

**Tech Stack:** Rust 2021, Tauri 2 commands in `reborn-ui`, vanilla JS in `crates/reborn-ui/ui/`, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-09-13-verified-game-image-law-design.md`

## Global Constraints

- Work in the isolated worktree `target/game-image-law` on branch `verified-game-image-law`, cut from `integration` at 0e2d4c7. Do NOT work in `target/merge-tree` or the main checkout: both are shared with other sessions.
- `library::hash_file(path) -> Result<Sha256Hex>` lives in `crate::library`, NOT `game_patch`. Call `.as_str()` for the hex.
- `Chain::parse` (game_patch.rs:105), `Chain::{load,route,base,head}`, `Route` (unit variant `Unknown`), `Step` (pub fields incl. `label`), `CHAIN_FILE` and `component_root` are all already public.
- Run `git status --short` immediately before any build. Never `git add -A`; stage explicit paths only.
- Do not merge `main` into this branch. `main` is behind `integration`.
- UI copy is US English. No em-dashes anywhere in code or copy.
- Never word Tier 2 as "custom". It is a fault to repair. The settings field is `unverified_game_ack`, never `custom_game`.
- `open_runtime` (`crates/reborn-ui/src/main.rs:4347`) is out of scope and must not be modified.
- Verify `settings.js` edits with `node Tools/check-shell-pages.mjs`, not `node --check`.
- Test commands: `cargo test -p reborn-core` and `cargo test -p reborn-ui`.

---

### Task 1: Reproduce the bug as a failing test

Proves the defect exists before anything is fixed. This test asserts current-broken behavior, then Task 8 flips it.

**Files:**
- Test: `crates/reborn-ui/ui/outcome-words.test.mjs` (create)
- Read only: `crates/reborn-ui/ui/onboarding.js:396-406`

**Interfaces:**
- Consumes: nothing.
- Produces: a Node test file proving `outcomeWords` returns a success headline for `'unknown'` and for `'no-chain'`.

- [ ] **Step 1: Extract `outcomeWords` so it can be tested**

In `crates/reborn-ui/ui/onboarding.js`, the function is currently a `const` inside `game_verification`. Move it to module scope, directly above `const SCREENS = {`, unchanged, and export it:

```js
export const outcomeWords=(outcome,label,note,patched)=>{
  const build=label||'NSRV3';
  switch(outcome){
    case 'bundle':return ['Game File Patched!',`Your original NFL Street 2 disc was patched to ${build} and verified. Continue to your controller.`];
    case 'patched':return ['Game File Patched!',`Your disc was updated to ${build} and verified. Continue to your controller.`];
    case 'newest':return ['Game File Imported!',`This disc is already the newest Reborn build (${build}). Continue to your controller.`];
    case 'unknown':return ['Game File Imported!',`This disc is not one Reborn recognizes, so it was kept exactly as it is.${note?' '+note:''} Continue to your controller.`];
    default:return patched?['Game File Patched!','Patched and verified. Continue to your controller.']:['Game File Imported!','Imported and ready. Continue to your controller.'];
  }
};
```

Delete the now-duplicated `const outcomeWords=...` block from inside `game_verification`; the outer one is in scope.

- [ ] **Step 2: Write the failing test**

Create `crates/reborn-ui/ui/outcome-words.test.mjs`:

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { outcomeWords } from './onboarding.js';

test('an unrecognized NFL Street 2 disc must not be reported as success', () => {
  const [headline] = outcomeWords('unknown', '', '', false);
  assert.ok(
    !headline.includes('!'),
    `an unconverted disc reported success: ${headline}`
  );
});

test('a missing patch chain must not be reported as success', () => {
  const [headline] = outcomeWords('no-chain', '', '', false);
  assert.ok(
    !headline.includes('!'),
    `a missing chain reported success: ${headline}`
  );
});
```

- [ ] **Step 3: Run it and confirm both fail**

Run: `node --test crates/reborn-ui/ui/outcome-words.test.mjs`
Expected: both FAIL. First reports `an unconverted disc reported success: Game File Imported!`, second reports `a missing chain reported success: Game File Imported!`. This is the Discord bug, captured.

- [ ] **Step 4: Confirm the page still loads**

Run: `node Tools/check-shell-pages.mjs`
Expected: PASS. The move to module scope must not break the shell pages.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/onboarding.js crates/reborn-ui/ui/outcome-words.test.mjs
git commit -m "test: capture the unconverted-disc success bug"
```

---

### Task 2: The `game_identity` primitive

**Files:**
- Create: `crates/reborn-core/src/game_identity.rs`
- Modify: `crates/reborn-core/src/lib.rs:61` (module list, alphabetical among the `game_*` entries)

**Interfaces:**
- Consumes: `crate::game_patch::{Chain, Route}`, already present at `crates/reborn-core/src/game_patch.rs`.
- Produces:
  - `pub enum Verdict { RebornBuild { label: String, at_head: bool }, Convertible, Unconverted, DifferentGame { game_id: String }, Unreadable { reason: String } }`
  - `pub fn classify(chain: Option<&Chain>, game_id: &str, sha256: &str) -> Verdict`
  - `pub const DISC_ID: &str = "GN7E69";`
  - `impl Verdict { pub fn is_reborn(&self) -> bool }`

- [ ] **Step 1: Write the failing tests**

Create `crates/reborn-core/src/game_identity.rs` with only the tests and the type shells:

```rust
//! What a disc image actually is, in one verdict.
//!
//! Three commands write `settings.game_path` and a fourth boots it. Before
//! this module each answered the question differently or not at all, and a
//! disc nothing recognized was accepted in silence: the player then booted
//! their own NFL Street 2 with Reborn's texture pack over it, which is where
//! "old names with new faces" came from.

use crate::game_patch::{Chain, Route};

/// The game ID every NFL Street 2 (USA) disc reports, Reborn's included.
pub const DISC_ID: &str = "GN7E69";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A known node in the chain. `at_head` means no update is pending.
    RebornBuild { label: String, at_head: bool },
    /// The chain's root: an unmodified USA disc, patchable to head.
    Convertible,
    /// NFL Street 2, but bytes nothing in the chain knows.
    Unconverted,
    /// Not NFL Street 2 at all.
    DifferentGame { game_id: String },
    /// Not a readable disc image.
    Unreadable { reason: String },
}

impl Verdict {
    /// Whether Reborn may treat this image as its own: boot it without a
    /// notice, apply its texture pack, and offer it updates.
    pub fn is_reborn(&self) -> bool {
        matches!(self, Verdict::RebornBuild { .. })
    }
}

pub fn classify(_chain: Option<&Chain>, _game_id: &str, _sha256: &str) -> Verdict {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two steps: root -> mid -> head.
    fn chain() -> Chain {
        Chain::parse(
            br#"{"schema":"nsr.game-patch-chain.v1","disc_id":"GN7E69","steps":[
                {"from_sha256":"aa","to_sha256":"bb","to_bytes":1459978240,
                 "patch":"patches/aa-bb.xdelta","label":"NSRV3"},
                {"from_sha256":"bb","to_sha256":"cc","to_bytes":1459978240,
                 "patch":"patches/bb-cc.xdelta","label":"2026.09.13"}]}"#,
        )
        .expect("the fixture chain parses")
    }

    #[test]
    fn the_chain_head_is_a_reborn_build_with_nothing_pending() {
        assert_eq!(
            classify(Some(&chain()), DISC_ID, "cc"),
            Verdict::RebornBuild { label: "2026.09.13".into(), at_head: true }
        );
    }

    #[test]
    fn a_mid_chain_node_is_a_reborn_build_with_an_update_pending() {
        assert_eq!(
            classify(Some(&chain()), DISC_ID, "bb"),
            Verdict::RebornBuild { label: "NSRV3".into(), at_head: false }
        );
    }

    #[test]
    fn the_chain_root_is_an_unmodified_disc_we_can_convert() {
        assert_eq!(classify(Some(&chain()), DISC_ID, "aa"), Verdict::Convertible);
    }

    /// The Discord bug. A modified NFL Street 2 passes every game-ID check
    /// ever written and is the one image that can impersonate Reborn.
    #[test]
    fn a_modified_nfl_street_2_is_unconverted_not_a_reborn_build() {
        let v = classify(Some(&chain()), DISC_ID, "deadbeef");
        assert_eq!(v, Verdict::Unconverted);
        assert!(!v.is_reborn(), "an unconverted disc must never pass as Reborn");
    }

    #[test]
    fn another_game_is_named_rather_than_called_unconverted() {
        assert_eq!(
            classify(Some(&chain()), "GALE01", "deadbeef"),
            Verdict::DifferentGame { game_id: "GALE01".into() }
        );
    }

    /// Without a chain we cannot tell a Reborn build from a modified disc,
    /// and guessing "Reborn" is the failure this module exists to prevent.
    #[test]
    fn no_installed_chain_is_unconverted_never_a_reborn_build() {
        let v = classify(None, DISC_ID, "cc");
        assert_eq!(v, Verdict::Unconverted);
        assert!(!v.is_reborn());
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/reborn-core/src/lib.rs`, add after line 61 (`pub mod game_image;`):

```rust
pub mod game_identity;
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p reborn-core game_identity`
Expected: all six FAIL with `not implemented`.

`Chain::parse(bytes: &[u8]) -> Result<Chain>` already exists at `game_patch.rs:105`; the fixture needs no temp file and no new constructor.

- [ ] **Step 4: Implement `classify`**

```rust
pub fn classify(chain: Option<&Chain>, game_id: &str, sha256: &str) -> Verdict {
    if game_id != DISC_ID {
        return Verdict::DifferentGame { game_id: game_id.to_string() };
    }
    // No chain means no way to tell Reborn from a mod. Say so rather than
    // assume; assuming is the bug.
    let Some(chain) = chain else { return Verdict::Unconverted };
    match chain.route(sha256) {
        Route::Current => Verdict::RebornBuild {
            label: chain.label_for(sha256).unwrap_or_default(),
            at_head: true,
        },
        Route::Steps(_) if Some(sha256) == chain.base() => Verdict::Convertible,
        Route::Steps(_) => Verdict::RebornBuild {
            label: chain.label_for(sha256).unwrap_or_default(),
            at_head: false,
        },
        Route::Unknown => Verdict::Unconverted,
    }
}
```

Add `label_for` to `Chain` in `game_patch.rs`, returning the label of the step whose `to_sha256` matches, and `None` for the root:

```rust
/// The build label of the node with this hash. The root has no label: it
/// is the unmodified disc, not a Reborn build.
pub fn label_for(&self, sha256: &str) -> Option<String> {
    self.steps.iter().find(|s| s.to_sha256 == sha256).map(|s| s.label.clone())
}
```

`Route` is `Route<'a>` and `Unknown` is a UNIT variant, confirmed at `game_patch.rs:83-90`.

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p reborn-core game_identity`
Expected: 6 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-core/src/game_identity.rs crates/reborn-core/src/lib.rs crates/reborn-core/src/game_patch.rs
git commit -m "feat: add game_identity, one verdict per disc image"
```

---

### Task 3: Read a disc image's identity from disk

`classify` is pure. This is the I/O half that feeds it.

**Files:**
- Modify: `crates/reborn-core/src/game_identity.rs`
- Read only: `crates/reborn-ui/src/setup_tools.rs:377-384` (the DolphinTool invocation to move)

**Interfaces:**
- Consumes: `classify` from Task 2; `crate::game_patch::{component_root, CHAIN_FILE}`.
- Produces:
  - `pub fn read_game_id(tool: &Path, image: &Path) -> Result<String, String>`
  - `pub fn identify(data_root: &Path, tool: &Path, image: &Path) -> Verdict`

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `game_identity.rs`:

```rust
#[test]
fn an_image_that_is_not_a_disc_is_unreadable_not_a_different_game() {
    let dir = tempfile::tempdir().expect("temp dir");
    let image = dir.path().join("not-a-disc.iso");
    std::fs::write(&image, b"this is not a gamecube disc").expect("write");
    let missing_tool = dir.path().join("DolphinTool.exe");
    match identify(dir.path(), &missing_tool, &image) {
        Verdict::Unreadable { .. } => {}
        other => panic!("expected Unreadable, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p reborn-core game_identity::tests::an_image_that_is_not_a_disc`
Expected: FAIL, `identify` not found.

- [ ] **Step 3: Implement both functions**

Add to `game_identity.rs`:

```rust
use std::path::Path;

/// The disc's game ID, read with DolphinTool. Moved here from
/// `setup_tools::check_reborn_id` so one place knows how to ask.
pub fn read_game_id(tool: &Path, image: &Path) -> Result<String, String> {
    let mut cmd = std::process::Command::new(tool);
    cmd.arg("header").arg("-i").arg(image).arg("-j");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("The game file could not be read.".to_string());
    }
    let header: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|_| "The game file could not be read.".to_string())?;
    header["game_id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "The game file has no game ID.".to_string())
}

/// What this image is, end to end: its game ID, its hash, and the installed
/// chain's opinion of it.
pub fn identify(data_root: &Path, tool: &Path, image: &Path) -> Verdict {
    let game_id = match read_game_id(tool, image) {
        Ok(id) => id,
        Err(reason) => return Verdict::Unreadable { reason },
    };
    let sha256 = match crate::library::hash_file(image) {
        Ok(h) => h,
        Err(e) => return Verdict::Unreadable { reason: e.to_string() },
    };
    let chain_path = crate::game_patch::component_root(data_root).join(crate::game_patch::CHAIN_FILE);
    let chain = Chain::load(&chain_path).ok().flatten();
    classify(chain.as_ref(), &game_id, sha256.as_str())
}
```

`library::hash_file(path) -> Result<Sha256Hex>`; call `.as_str()` on it, as `game_patch.rs:427` does.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p reborn-core game_identity`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/game_identity.rs
git commit -m "feat: identify a disc image from disk"
```

---

### Task 4: Record the Tier 2 acknowledgment

**Files:**
- Modify: `crates/reborn-core/src/settings.rs:120` (add field), `:180` (add to `Default`)

**Interfaces:**
- Consumes: nothing.
- Produces: `Settings::unverified_game_ack: Option<String>` holding the SHA-256 of the one image the player accepted, plus `pub fn ack_matches(&self, sha256: &str) -> bool`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `settings.rs`:

```rust
#[test]
fn acknowledging_one_unverified_image_does_not_acknowledge_another() {
    let mut s = Settings::default();
    assert!(!s.ack_matches("aa"), "nothing is acknowledged by default");
    s.unverified_game_ack = Some("aa".into());
    assert!(s.ack_matches("aa"));
    assert!(!s.ack_matches("bb"), "the ack is per image, never a global mode");
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p reborn-core acknowledging_one_unverified_image`
Expected: FAIL, no field `unverified_game_ack`.

- [ ] **Step 3: Implement**

In the `Settings` struct, after `migrated_texture_root` (line 120):

```rust
/// The SHA-256 of the one image the player chose to play despite it not
/// being a Reborn build. Per image on purpose: this is an escape hatch
/// from a fault, not a preference, so it must not survive a swap to a
/// different image.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub unverified_game_ack: Option<String>,
```

In `impl Default for Settings`, add `unverified_game_ack: None,`.

After the struct, in `impl Settings`:

```rust
/// Whether this exact image is the one the player accepted.
pub fn ack_matches(&self, sha256: &str) -> bool {
    self.unverified_game_ack.as_deref() == Some(sha256)
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p reborn-core settings`
Expected: all pass, including the new one.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/settings.rs
git commit -m "feat: record the unverified-image acknowledgment per image"
```

---

### Task 5: The launch gate

The only change that reaches players who are already broken.

**Files:**
- Modify: `crates/reborn-ui/src/main.rs:707-738` (`launch`)

**Interfaces:**
- Consumes: `game_identity::{identify, Verdict}`, `Settings::ack_matches`.
- Produces: `launch` returns `Err` with a repair-shaped message for an unacknowledged `Unconverted` image.

- [ ] **Step 1: Write the failing test**

Add to the test module in `main.rs` alongside the existing command-body assertions near line 5927:

```rust
#[test]
fn launch_verifies_the_image_before_starting_the_emulator() {
    let body = command_body(include_str!("main.rs"), "async fn launch(");
    let verify = body
        .find("game_identity::identify")
        .expect("launch boots game_path without ever asking what it is");
    let request = body
        .find("LaunchRequest {")
        .expect("launch still builds a LaunchRequest");
    assert!(
        verify < request,
        "launch must know what the image is before it builds the request that boots it"
    );
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p reborn-ui launch_verifies_the_image`
Expected: FAIL, `launch boots game_path without ever asking what it is`.

- [ ] **Step 3: Implement the gate**

In `launch`, between the `find_runtime_and_game_for` call (ends line 727) and `let req = LaunchRequest {` (line 728):

```rust
        // What we are about to boot, before we boot it. A disc that is not a
        // Reborn build must never reach the emulator wearing Reborn's
        // textures: that is what produced "old names with new faces".
        let tool = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .unwrap()
            .join("game-import/DolphinTool.exe");
        let data = p.paths.data_root().map_err(|e| e.player_message().to_string())?;
        let verdict = reborn_core::game_identity::identify(&data, &tool, &game_path);
        let settings_file =
            reborn_core::settings::settings_path(&p).map_err(|e| e.player_message().to_string())?;
        let saved = reborn_core::settings::Settings::load(&settings_file)
            .map_err(|e| e.player_message().to_string())?;
        let image_sha = reborn_core::library::hash_file(&game_path)
            .map(|h| h.as_str().to_string())
            .unwrap_or_default();
        let verified = match &verdict {
            reborn_core::game_identity::Verdict::Unconverted if !saved.ack_matches(&image_sha) => {
                return Err(UNVERIFIED_GAME.to_string())
            }
            v => v.is_reborn(),
        };
```

Add the constant near the other player-facing strings in `main.rs`:

```rust
/// Shown when Play is pressed on an NFL Street 2 disc that is not a Reborn
/// build. Worded as the fault it almost always is, not as a preference.
const UNVERIFIED_GAME: &str = "Your game file isn't NFL Street Reborn. This is NFL Street 2, but not a Reborn build, so rosters, faces and stats won't match. This usually means setup couldn't convert your disc. Repair it from the Settings page.";
```

Leave `verified` unused for now; Task 6 consumes it. Silence the warning with `let _ = verified;` and delete that line in Task 6.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p reborn-ui launch_verifies_the_image`
Expected: PASS.

Run: `cargo test -p reborn-ui`
Expected: the existing `launch` body assertions at `main.rs:5927` and `:5944` still pass. If the stats-thread ordering assertion breaks, the gate was inserted too late; move it above `let req`.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/src/main.rs
git commit -m "feat: verify the game image before Play boots it"
```

---

### Task 6: Switch Reborn's textures off for any image it did not verify

The load-bearing half. Without it, "Play it anyway" reproduces the original symptom.

**Files:**
- Modify: `crates/reborn-ui/src/main.rs` (`launch`, the `LaunchRequest` built at :728; and `:1496`)

**Interfaces:**
- Consumes: `verified` from Task 5.
- Produces: `LaunchRequest.manage_user_texture_folder` is false whenever the booted image is not a Reborn build.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn an_unverified_image_never_gets_rebornes_texture_pack() {
    let body = command_body(include_str!("main.rs"), "async fn launch(");
    assert!(
        body.contains("manage_user_texture_folder") && body.contains("verified"),
        "launch does not gate the texture pack on whether the image verified, so a \
         non-Reborn NFL Street 2 still boots wearing Reborn's faces"
    );
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p reborn-ui an_unverified_image_never_gets`
Expected: FAIL.

- [ ] **Step 3: Implement**

Delete the `let _ = verified;` line from Task 5. After `Session::run`'s request is built, set the field on `req`. Because `LaunchRequest`'s literal at line 728 may not list the field, set it immediately after construction:

```rust
        let mut req = LaunchRequest { /* ... unchanged ... */ };
        // Reborn's art only ever paints a disc Reborn verified. A player who
        // chose to play a modified NFL Street 2 anyway sees it as what it is.
        if !verified {
            req.manage_user_texture_folder = false;
        }
```

Change `let req` to `let mut req`. Leave `main.rs:1496` (`req.manage_user_texture_folder = preferences.manage_texture_folder.unwrap_or(true);`) alone: the player preference still applies for verified images, and this only ever narrows it.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p reborn-ui`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/src/main.rs
git commit -m "feat: never paint Reborn textures on an image Reborn did not verify"
```

---

### Task 7: Door 1, stop `setup_import_game` swallowing the reason

**Files:**
- Modify: `crates/reborn-ui/src/setup_tools.rs:166-195`

**Interfaces:**
- Consumes: `game_identity::{identify, Verdict}`.
- Produces: `setup_import_game` returns `outcome` values `"unconverted"` and `"different-game"`, and a `reason` field carrying the verifier's own sentence.

- [ ] **Step 1: Write the failing test**

Add to the test module in `setup_tools.rs` (create one if absent, mirroring `main.rs`'s `command_body` helper):

```rust
#[test]
fn the_import_never_discards_why_a_disc_failed() {
    let src = include_str!("setup_tools.rs");
    let body = command_body(src, "pub async fn setup_import_game");
    assert!(
        !body.contains("Err(_) =>"),
        "setup_import_game still throws away the verifier's reason, which is how an \
         unconverted disc got reported as 'Game File Imported!'"
    );
    assert!(
        body.contains("game_identity::identify"),
        "setup_import_game does not ask what the disc actually is"
    );
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p reborn-ui the_import_never_discards`
Expected: FAIL on the `Err(_)` assertion.

- [ ] **Step 3: Implement**

Replace lines 167-173 with a version that keeps the reason:

```rust
        let (output,patched,bundle_reason)=match reborn_core::game_import::import_with_progress(&bundle,&source,&data,|stage|{let _=app.emit("game-import-progress",stage);}) {
            Ok(output)=>(output,true,None),
            Err(reason) => {
                let _=app.emit("game-import-progress",serde_json::json!({"stage":"Importing your game file...","percent":null}));
                (import_existing(&p,&source)?,false,Some(reason))
            }
        };
```

After `establish_booted` and the existing `patch_to_newest` block, classify what actually landed and let that decide the outcome:

```rust
        let tool=std::env::current_exe().map_err(|e|e.to_string())?.parent().unwrap().join("game-import/DolphinTool.exe");
        let verdict=reborn_core::game_identity::identify(&data,&tool,&booted);
        let (outcome,label,note)=match &verdict {
            reborn_core::game_identity::Verdict::DifferentGame{game_id}=>
                ("different-game".to_string(),None,Some(format!("This is not NFL Street 2 (game ID {game_id}). Reborn's roster, textures and updates do not apply to it."))),
            reborn_core::game_identity::Verdict::Unconverted=>
                ("unconverted".to_string(),None,bundle_reason.clone().or_else(||Some("Reborn does not recognize this disc.".into()))),
            reborn_core::game_identity::Verdict::Unreadable{reason}=>
                ("unreadable".to_string(),None,Some(reason.clone())),
            _=>(outcome,label,note),
        };
```

Keep the existing `(outcome,label,note)` computation above it so the `bundle`/`patched`/`newest` cases are unchanged; the match's `_` arm reuses them. Add `"reason"` to the returned JSON alongside `"note"`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p reborn-ui the_import_never_discards`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/src/setup_tools.rs
git commit -m "fix: carry the real reason a disc failed to convert"
```

---

### Task 8: Door 2 and Door 3, and the wizard tells the truth

**Files:**
- Modify: `crates/reborn-ui/src/main.rs:3979-3996` (`settings_set_path`)
- Modify: `crates/reborn-ui/src/setup_tools.rs:498` (`setup_migrate_install`)
- Modify: `crates/reborn-ui/ui/onboarding.js` (`outcomeWords`)

**Interfaces:**
- Consumes: `game_identity::identify`, the `"unconverted"` / `"different-game"` / `"unreadable"` outcomes from Task 7.
- Produces: Task 1's two tests now pass.

- [ ] **Step 1: Make the Task 1 tests the target**

Run: `node --test crates/reborn-ui/ui/outcome-words.test.mjs`
Expected: still FAIL, exactly as in Task 1. These are the tests this task flips.

- [ ] **Step 2: Give the wizard real failure states**

In `crates/reborn-ui/ui/onboarding.js`, replace the `outcomeWords` body:

```js
export const outcomeWords=(outcome,label,note,patched)=>{
  const build=label||'NSRV3';
  switch(outcome){
    case 'bundle':return ['Game File Patched!',`Your original NFL Street 2 disc was patched to ${build} and verified. Continue to your controller.`];
    case 'patched':return ['Game File Patched!',`Your disc was updated to ${build} and verified. Continue to your controller.`];
    case 'newest':return ['Game File Imported!',`This disc is already the newest Reborn build (${build}). Continue to your controller.`];
    case 'unconverted':return ["Your game file isn't NFL Street Reborn",`This is NFL Street 2, but not a Reborn build, so rosters, faces and stats won't match. This usually means setup couldn't convert your disc.${note?' '+note:''}`];
    case 'different-game':return ['This is not NFL Street 2',`${note?note+' ':''}Reborn's roster, textures and updates do not apply to it.`];
    case 'unreadable':return ['Your game file could not be read',note||'Choose an NFL Street 2 (USA) disc image.'];
    case 'unknown':case 'no-chain':return ["Your game file isn't NFL Street Reborn",'Reborn could not confirm this disc is one of its builds, so it was left exactly as it is. Repair it before playing.'];
    default:return patched?['Game File Patched!','Patched and verified. Continue to your controller.']:['Game File Imported!','Imported and ready. Continue to your controller.'];
  }
};
```

Note the backtick string for `different-game` contains an apostrophe in `Reborn's`; escape it as `Reborn\'s` only if the file's linter requires, otherwise a template literal handles it.

- [ ] **Step 3: Run the Task 1 tests and confirm they now pass**

Run: `node --test crates/reborn-ui/ui/outcome-words.test.mjs`
Expected: 2 passed. The Discord bug is closed at the wizard.

Run: `node Tools/check-shell-pages.mjs`
Expected: PASS.

- [ ] **Step 4: Gate the Continue button on a converted disc**

In `game_verification`'s `fresh` branch, the screen currently returns `ready:()=>!!pending.image`. Replace with:

```js
      const blocked=()=>{try{return ['unconverted','unreadable'].includes(localStorage.getItem('setup:outcome:'+pending.image)||'');}catch(_){return false;}};
      return {nodes:[...],ready:()=>!!pending.image&&!blocked(),nextLabel:'Continue',answer:()=>call('onboarding_confirm_game',{image:pending.image})};
```

Keep the existing `nodes` array unchanged. `different-game` is deliberately not blocked: Tier 1 is allowed, it is only told.

- [ ] **Step 5: Close Door 2**

Replace the body of `settings_set_path` at `main.rs:3991`, for the `Game` case only:

```rust
        if matches!(setting, PathSetting::Game) && !value.trim().is_empty() {
            let data = p.paths.data_root().map_err(|e| e.player_message().to_string())?;
            let tool = std::env::current_exe().map_err(|e| e.to_string())?.parent().unwrap()
                .join("game-import/DolphinTool.exe");
            let image = reborn_core::session::user_path(value.trim());
            match reborn_core::game_identity::identify(&data, &tool, &image) {
                reborn_core::game_identity::Verdict::Unreadable { reason } => return Err(reason),
                reborn_core::game_identity::Verdict::Unconverted => {
                    let sha = reborn_core::library::hash_file(&image)
                        .map(|h| h.as_str().to_string()).map_err(|e| e.to_string())?;
                    let mut saved = reborn_core::settings::Settings::load(&file)
                        .map_err(|e| e.player_message().to_string())?;
                    saved.unverified_game_ack = Some(sha);
                    saved.save(&file).map_err(|e| e.player_message().to_string())?;
                }
                _ => {}
            }
        }
```

This is the one place the acknowledgment is written: choosing an image in Settings is the deliberate act. Task 5's launch gate then lets it boot, with textures off per Task 6.

- [ ] **Step 6: Close Door 3**

In `setup_migrate_install` at `setup_tools.rs:498`, before `saved.game_path=Some(game.clone());`, classify the inherited image and drop it rather than adopt it blind:

```rust
        let tool=std::env::current_exe().map_err(|e|e.to_string())?.parent().unwrap().join("game-import/DolphinTool.exe");
        let inherited=reborn_core::game_identity::identify(&data,&tool,&game);
        // A 2.1 install's disc is adopted only if it is one of ours. Anything
        // else goes to recovery rather than becoming this install's game.
        let game=if inherited.is_reborn()||matches!(inherited,reborn_core::game_identity::Verdict::Convertible){game.clone()}else{return Err(UNVERIFIED_MIGRATED_GAME.to_string())};
```

Add beside it:

```rust
const UNVERIFIED_MIGRATED_GAME:&str="The game file in your old installation isn't an NFL Street Reborn build, so it was not brought over. Choose your NFL Street 2 disc on the next step and Reborn will convert it.";
```

Confirm `data` is in scope at that point; if not, resolve it the same way the surrounding code does.

- [ ] **Step 7: Run the full suite**

Run: `cargo test -p reborn-core && cargo test -p reborn-ui && node --test crates/reborn-ui/ui/outcome-words.test.mjs && node Tools/check-shell-pages.mjs`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add crates/reborn-ui/src/main.rs crates/reborn-ui/src/setup_tools.rs crates/reborn-ui/ui/onboarding.js
git commit -m "fix: close the settings and migrate doors, and stop the wizard reporting failure as success"
```

---

### Task 9: The guard test, so a fourth door cannot reopen this

**Files:**
- Modify: `crates/reborn-ui/src/main.rs` (test module near `:5711`)

**Interfaces:**
- Consumes: all prior tasks.
- Produces: a test that fails if any future code writes `game_path` without verifying.

- [ ] **Step 1: Write the test**

```rust
/// Every writer of `settings.game_path` has to ask what the image is first.
/// Before this law, three of them did not, and a disc nothing recognized
/// became the game Reborn booted, wearing Reborn's textures.
#[test]
fn every_writer_of_game_path_verifies_the_image() {
    for (name, src) in [
        ("main.rs", include_str!("main.rs")),
        ("setup_tools.rs", include_str!("setup_tools.rs")),
    ] {
        for (line_no, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || !line.contains("game_path") {
                continue;
            }
            // Only assignments, not reads.
            if !line.contains("game_path=") && !line.contains("game_path =") {
                continue;
            }
            let window: String = src
                .lines()
                .skip(line_no.saturating_sub(40))
                .take(45)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                window.contains("game_identity::identify"),
                "{name}:{} writes game_path without verifying the image first",
                line_no + 1
            );
        }
    }
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p reborn-ui every_writer_of_game_path`
Expected: PASS, because Tasks 7 and 8 added `identify` near all three writers. If it fails, the failing line number names the writer that still needs the call; add it rather than widening the window.

- [ ] **Step 3: Commit**

```bash
git add crates/reborn-ui/src/main.rs
git commit -m "test: no writer of game_path may skip verification"
```

---

### Task 10: The `game-full` recovery component

**Files:**
- Modify: `crates/reborn-core/src/inventory.rs:141-172` (`ComponentId`), `:185` (the name table)
- Create: `crates/reborn-core/src/publish/game_full.rs`
- Modify: `crates/reborn-core/src/publish/mod.rs` (module list)

**Interfaces:**
- Consumes: `ComponentId`, the existing publish pipeline in `crates/reborn-core/src/publish/game_patch.rs` as the pattern to follow.
- Produces: `ComponentId::GameFull` with wire name `"game-full"`, and `publish::game_full::stage(...)`.

- [ ] **Step 1: Write the failing test**

Add to the test module in `inventory.rs`:

```rust
#[test]
fn the_full_disc_is_a_component_the_publisher_knows() {
    assert_eq!(ComponentId::GameFull.as_str(), "game-full");
    assert!(
        ComponentId::ALL.contains(&ComponentId::GameFull),
        "publishing and name validation need to see it"
    );
}
```

And in `crates/reborn-core/src/publish/release.rs`, beside the existing
carry-forward tests at `:449`:

```rust
/// Every other component is carried forward into the next release
/// automatically. `game-full` must not be: it is 1.36 GB of recovery that a
/// normal install never needs, and carrying it forward would put it in front
/// of every player from the next release onward. Re-pin it deliberately or
/// not at all.
#[test]
fn game_full_is_never_carried_forward_into_the_next_release() {
    let existing = pinned(&[("roster", "2026.09.13"), ("game-full", "2026.09.13.3")]);
    let next = carry_forward(&existing, &[]);
    assert!(next.iter().any(|(n, _)| *n == "roster"), "ordinary pins carry");
    assert!(
        !next.iter().any(|(n, _)| *n == "game-full"),
        "the full disc was carried into a release nobody asked it to be in"
    );
}
```

Match `pinned` and `carry_forward` to the real helper and function names in
`release.rs`; the existing tests at `:449` and `:466` show both.

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p reborn-core the_full_disc_is_a_component`
Expected: FAIL, no variant `GameFull`.

- [ ] **Step 3: Implement**

In `inventory.rs`, add to `ComponentId` after `GamePatch`:

```rust
    /// The complete head-build disc, 1,459,978,240 bytes. Recovery only, for
    /// a player whose own copy cannot be converted. Deliberately absent from
    /// the release carry-forward: a normal install never fetches it.
    GameFull,
```

Add `ComponentId::GameFull` to `ALL`, and add to the name table at `:185`:

```rust
            ComponentId::GameFull => "game-full",
```

There is no default-release constant to split, and none should be invented. Verified in this worktree: `rebornctl publish release` pins components explicitly, one `--component NAME=VERSION` flag each (`crates/reborn-cli/src/publish.rs:76-96`), and `ComponentId::ALL` is used only for name validation and iteration. A component reaches a release solely by being pinned.

So the guarantee lives in exactly one place: the carry-forward in `crates/reborn-core/src/publish/release.rs`, which re-pins every component from the current release into the next one. Exclude `game-full` there, so it can only ever enter a release by an explicit `--component game-full=<version>` on that release's own command line.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p reborn-core inventory`
Expected: all pass. Existing tests that iterate `ALL` and expect nine components will need their count updated to ten; update them, do not weaken them. Run `cargo test -p reborn-core publish::release` as well as `inventory`.

- [ ] **Step 5: Add the publish command**

Create `crates/reborn-core/src/publish/game_full.rs` modeled on `publish/game_patch.rs`, staging one file (the head ISO) and pinning its SHA-256 to the chain head's `to_sha256`. Refuse to stage if the two disagree:

```rust
/// The published full disc and the chain head must be the same bytes, or a
/// player who recovers by download lands on a build no patch can move.
pub fn refuse_if_not_chain_head(iso_sha: &str, chain_head: &str) -> Result<(), String> {
    if iso_sha != chain_head {
        return Err(format!(
            "the full disc is {iso_sha} but the chain head is {chain_head}; publish the chain first"
        ));
    }
    Ok(())
}
```

Register the module in `crates/reborn-core/src/publish/mod.rs`, and add a `publish game-full` subcommand in `crates/reborn-cli/src/publish.rs` following the shape of the existing `game-patch` subcommand at `:75-82`.

- [ ] **Step 6: Run the full core suite**

Run: `cargo test -p reborn-core`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-core/src/inventory.rs crates/reborn-core/src/publish/game_full.rs crates/reborn-core/src/publish/mod.rs crates/reborn-cli/src/publish.rs
git commit -m "feat: add the game-full recovery component, off the default release"
```

---

### Task 11: Offer recovery in the UI

**Files:**
- Modify: `crates/reborn-ui/ui/onboarding.js` (`game_verification`)
- Modify: `crates/reborn-ui/src/setup_tools.rs` (new command)

**Interfaces:**
- Consumes: `ComponentId::GameFull`, the outcomes from Task 7.
- Produces: Tauri command `setup_download_reborn_game() -> Result<String, String>` returning the imported path.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn the_recovery_download_is_offered_only_for_a_disc_we_could_not_convert() {
    let src = include_str!("../ui/onboarding.js");
    assert!(
        src.contains("setup_download_reborn_game"),
        "no recovery is offered, so a player whose disc cannot convert is stranded"
    );
    assert!(
        !src.contains("setup_download_reborn_game') } , 'newest'"),
        "recovery must not be offered to a disc that is already a Reborn build"
    );
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p reborn-ui the_recovery_download_is_offered`
Expected: FAIL.

- [ ] **Step 3: Add the command**

In `setup_tools.rs`, beside `setup_import_game`:

```rust
/// Recovery for a player whose own copy cannot be converted. Resolves the
/// `game-full` component, which is deliberately absent from the default
/// release, so nothing downloads 1.36 GB unless it is actually needed.
#[tauri::command]
pub async fn setup_download_reborn_game(app: tauri::AppHandle) -> Result<String,String> {
    tauri::async_runtime::spawn_blocking(move|| {
        use tauri::Emitter;
        let _guard=LaunchGuard::acquire()?;
        let p=Platform::detect();
        let data=p.paths.data_root().map_err(|e|e.to_string())?;
        let _=app.emit("game-import-progress",serde_json::json!({"stage":"Downloading NFL Street Reborn…","percent":0}));
        let iso=fetch_game_full(&p,&|done,total|{
            let percent=if total>0 {Some((95*done/total) as u8)} else {None};
            let _=app.emit("game-import-progress",serde_json::json!({"stage":"Downloading NFL Street Reborn…","percent":percent}));
        })?;

        // The download and the patch chain must be the same build, or the
        // next game update has nothing to apply to.
        let _=app.emit("game-import-progress",serde_json::json!({"stage":"Verifying…","percent":96}));
        let sha=reborn_core::library::hash_file(&iso).map_err(|e|e.to_string())?;
        let chain_path=reborn_core::game_patch::component_root(&data).join(reborn_core::game_patch::CHAIN_FILE);
        let head=reborn_core::game_patch::Chain::load(&chain_path).ok().flatten()
            .and_then(|c|c.head().map(str::to_string))
            .ok_or("No game updates are installed, so the download could not be checked.")?;
        if sha.as_str()!=head {
            return Err("The downloaded game does not match the current build. Try again.".into());
        }

        let booted=establish_booted(&p,&data,&iso)?;
        let settings=reborn_core::settings::settings_path(&p).map_err(|e|e.to_string())?;
        let mut saved=reborn_core::settings::Settings::load(&settings).map_err(|e|e.to_string())?;
        // A fresh, verified image retires any earlier escape hatch.
        saved.game_path=Some(booted.clone());saved.unverified_game_ack=None;
        saved.save(&settings).map_err(|e|e.to_string())?;
        let _=app.emit("game-import-progress",serde_json::json!({"stage":"NFL Street Reborn is ready","percent":100}));
        Ok(booted.display().to_string())
    }).await.map_err(|e|e.to_string())?
}
```

Register it in the `tauri::generate_handler!` list in `main.rs` beside `setup_import_game`.

**`content::fetch_component` does not exist. Do not write it as shown above; `fetch_game_full` is a placeholder name for work you must design.** Verified in this worktree: the content engine resolves a component through an `UpdatePlan` plus `Engine::download(&self, plan, source: &dyn BlobSource)` (`crates/reborn-core/src/content.rs:1259`), with `installed`, `staging_dir`, `live_root` and `accept_component` around it; `crates/reborn-core/src/origin.rs` supplies the `BlobSource` and the signed release. `update_download` in `main.rs:4248` is the LAUNCHER self-updater and is NOT the path to copy.

This step is therefore a design task, not transcription. Read `content.rs` and whichever UI command already installs a content component, and follow that existing path to resolve `ComponentId::GameFull` from the origin into a local file. Reuse it; do not build a second download stack. If after reading you judge that the existing path cannot fetch a component that is absent from the current release manifest, stop and report BLOCKED with what you found: that is a real design question and the controller will rule on it.

- [ ] **Step 4: Add the button**

In `game_verification`, when the stored outcome is `unconverted` or `unreadable`, render a primary button:

```js
      const recover=el('button','primary','Download the Reborn game (1.36 GB)');
      recover.type='button';
      recover.addEventListener('click',async()=>{
        if(moving)return;moving=true;recover.disabled=true;updateNav();
        try{
          const path=await call('setup_download_reborn_game');
          pending.image=path;
          try{localStorage.setItem('setup:outcome:'+path,'newest');localStorage.setItem('h:game',path);}catch(_){}
          status.textContent='NFL Street Reborn is ready';detail.textContent='Continue to your controller.';
          box.classList.add('patch-complete');
        }catch(e){detail.textContent=String(e.message||e);}
        finally{moving=false;recover.disabled=false;updateNav();}
      });
```

Append `recover` to `box` only when blocked, so a converted disc never sees it.

- [ ] **Step 5: Run everything**

Run: `cargo test -p reborn-core && cargo test -p reborn-ui && node --test crates/reborn-ui/ui/outcome-words.test.mjs && node Tools/check-shell-pages.mjs`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/src/setup_tools.rs crates/reborn-ui/ui/onboarding.js
git commit -m "feat: offer the Reborn game download when a disc cannot be converted"
```

---

### Task 12: The unresolved badge on the Play card

The spec's Tier 2 rule that the state never reads as a settled preference. Without this, a player who used the escape hatch has no standing reminder and no way back.

**Files:**
- Modify: `crates/reborn-ui/src/main.rs` (`launch_check`)
- Modify: `crates/reborn-ui/ui/home.js:60` (already calls `launch_check`)

**Interfaces:**
- Consumes: `game_identity::identify` from Task 3.
- Produces: `launch_check` gains `game_verdict: String`, one of `"reborn"`, `"convertible"`, `"unconverted"`, `"different-game"`, `"unreadable"`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn the_play_page_can_tell_the_player_their_game_is_not_reborn() {
    let body = command_body(include_str!("main.rs"), "async fn launch_check");
    assert!(
        body.contains("game_identity::identify"),
        "launch_check never asks what the game is, so the Play page cannot show \
         that the player is on a disc Reborn did not verify"
    );
    let home = include_str!("../ui/home.js");
    assert!(
        home.contains("game_verdict"),
        "the Play page ignores the verdict"
    );
    assert!(
        !home.contains("Custom game"),
        "Tier 2 must never be worded as a preference the player chose"
    );
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p reborn-ui the_play_page_can_tell`
Expected: FAIL on the first assertion.

- [ ] **Step 3: Report the verdict from `launch_check`**

In `launch_check`, after the game path is resolved, add:

```rust
        let tool = std::env::current_exe().map_err(|e| e.to_string())?.parent().unwrap()
            .join("game-import/DolphinTool.exe");
        let game_verdict = match reborn_core::game_identity::identify(&data, &tool, &game_path) {
            reborn_core::game_identity::Verdict::RebornBuild { .. } => "reborn",
            reborn_core::game_identity::Verdict::Convertible => "convertible",
            reborn_core::game_identity::Verdict::Unconverted => "unconverted",
            reborn_core::game_identity::Verdict::DifferentGame { .. } => "different-game",
            reborn_core::game_identity::Verdict::Unreadable { .. } => "unreadable",
        }
        .to_string();
```

Add `game_verdict: String` to the struct `launch_check` returns and set it. Read the existing struct definition and follow its `#[derive(Serialize)]` and field-naming conventions.

- [ ] **Step 4: Render the badge**

In `home.js`, where the launch-check result is rendered, add:

```js
  if (data.check?.game_verdict === 'unconverted') {
    const badge = el('div', 'play-warning');
    badge.append(el('span', null, 'Not a Reborn build'));
    const repair = el('button', 'link', 'Repair');
    repair.type = 'button';
    repair.addEventListener('click', () => go('settings'));
    badge.append(repair);
    card.append(badge);
  }
```

Match `home.js`'s own element helper and navigation function names; the file already imports them. Use the existing warning style class rather than adding a new one if one exists.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p reborn-ui the_play_page_can_tell && node Tools/check-shell-pages.mjs`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/src/main.rs crates/reborn-ui/ui/home.js
git commit -m "feat: show an unresolved badge when Play is on a disc Reborn did not verify"
```

---

## Manual verification, before any release

Automated tests cover the logic. The spec's "Inferred, not verified" section lists three things that need eyes on a running build:

1. Point a deliberately modified `GN7E69` ISO at setup. Confirm the wizard now says "Your game file isn't NFL Street Reborn" and Continue is disabled.
2. Acknowledge one via Settings, press Play, and confirm in-game that Reborn's faces are **absent**. This is the check that proves Task 6 works; it is the only one that catches the original symptom. Confirm the Play card carries the `Not a Reborn build` badge afterwards, and that nothing anywhere calls it "custom".
3. Point it at a non-`GN7E69` disc and confirm Tier 1 allows it with the plain notice.
4. Confirm a correct disc still reports "Game File Patched!" and boots with Reborn's roster and faces intact.
