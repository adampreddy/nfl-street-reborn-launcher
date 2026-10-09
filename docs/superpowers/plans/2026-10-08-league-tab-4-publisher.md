# League Tab — Plan 4: Publisher + Update Pipeline for Members-Only Components

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Adam publish the league build (`league-game`: league ISO + league textures) and the league seed (`league-content`: `league.s01`) through the existing signed release pipeline as **members-only** components. Their manifests are public and signed like every other component, but their blobs are stored in a private R2 bucket that only the Reborn backend can hand out (as short-lived signed URLs).

**Architecture:** Three changes to the shared core. (1) Two new `ComponentId`s, plus a `members_only` flag on `nsr.component.v1` that is derived from the identity, the same way `customizable` is. (2) A second, private blob store in the local origin (`private/blobs/by-hash/…`). `publish_component` and the release signer use it for members-only components, the public `plan_origin` never reads it, and a new `plan_private` uploads it to a separate bucket. (3) A `publish/league.rs` stager with two CLI commands, plus one Publisher page. Signing, the release schema and the carry-forward are unchanged.

**Tech Stack:** Rust (`reborn-core` with the `publish` feature, `reborn-cli` = `rebornctl`, `reborn-publisher` Tauri app), Cloudflare R2 (S3 SigV4, path-style), rclone as the manual fallback.

**Spec:** `docs/superpowers/specs/2026-10-08-league-tab-design.md` (v2, section 4 "League edition"). **Binding contracts:** `docs/superpowers/plans/2026-10-08-league-contracts.md` ("Plan 4 → Plans 2/3b").

## Global Constraints

- **Branch base: `port/linux-deck-on-3.0.35` @6b48d3c**, NOT `feat/league-tab` (3.0.35). The live release pins `runtime-linux` and `game-import`, which only that line's `ComponentId` knows. `republish_pinning` refuses to sign a release that pins a name the build does not know (`release.rs:315`), so a publisher built from 3.0.35 cannot sign at all. New worktree: `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher leaguepub`, branch `feat/league-publish`. Never work in the `deck` worktree: another session has it on `port/macos`.
- Build and test with `CARGO_BUILD_JOBS=1` (memory: OOM otherwise). Core tests: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core --features publish <filter>`.
- `.rs` files stay **LF**. CRLF breaks the source-scanning tests (memory `project_reborn_launcher_crlf_breaks_tests`).
- Component names on the wire are exactly `league-game` and `league-content`. Install roots are exactly `library/league` and `library/league-content`.
- `members_only` is **serialized only when true** (`skip_serializing_if`), and parses as `false` when absent. Every manifest already published must keep round-tripping byte for byte (`release.rs` refuses one that does not).
- Members-only blobs never enter `blobs/by-hash` in the local origin, never enter `plan_origin`, and are never uploaded to the public bucket `reborn-content`. Private origin path: `private/blobs/by-hash/<xx>/<sha256>`. Key in the private bucket: `blobs/by-hash/<xx>/<sha256>`. Private bucket name: **`reborn-league`**.
- A private upload never reads back through the public CDN (`public_base: None`). **LAW: never HTTP-probe a blob URL before it is uploaded.** Cloudflare caches the 404 for 4 hours.
- League game id: read from the ISO header (bytes 0..6, `[A-Z0-9]{6}`). Refused if it is `GN7E69` or `FR4N31` (retail) or `LGXXXX` (the contracts' placeholder).
- Upload order is blobs (private), then manifest, then release files. The type enforces it within one bucket; across the two buckets the caller orders them: private plan first.
- Secrets: R2 credentials come only from the vault (`nsr.r2.access` / `nsr.r2.secret`). Never print them, never put them on a command line.
- The first real publish is **gated on Adam** (Task 9).

## Review Focus

1. **An old manifest re-read by the new code**, for example `runtime-1.2.11.json` with no `members_only` key. It must parse as `false`, and re-serializing it must give identical bytes so the release signer's round-trip check still passes. Pinned in Task 1.
2. **The same blob shared by retail and league content** (a league texture identical to a retail one). The league copy must still land in the private store, and the public copy must be untouched. Pinned in Task 2.
3. **Someone runs `publish upload` (public) after a league publish.** Nothing under `private/` may appear in the plan. Pinned in Task 2.
4. **The league ISO is actually a retail disc**, or the textures folder is for another game id. Refused before anything is staged. Pinned in Task 4.
5. **The live texture folder handed in with `texconv.log` and `.blanked_off` backups.** Those must not be published (memory: textures 1.0.0 shipped the log). Pinned in Task 4.

---

## File map

| Path | Status | Responsibility |
|---|---|---|
| `crates/reborn-core/src/inventory.rs` | modify | `ComponentId::{LeagueGame, LeagueContent}`, `is_members_only`, install roots, scan sets `members_only` |
| `crates/reborn-core/src/platform/mod.rs` | modify | `DataDir::{LibraryLeague, LibraryLeagueContent}` |
| `crates/reborn-core/src/manifest.rs` | modify | `Component.members_only` (serde default, skip when false) |
| `crates/reborn-core/src/origin.rs` | modify | `PRIVATE_BLOB_DIR`, `private_blob_path` |
| `crates/reborn-core/src/publish/mod.rs` | modify | `stored_blob`, `default_stage` (moved from game_full.rs), `pub mod league` |
| `crates/reborn-core/src/publish/component.rs` | modify | blobs land in the store `stored_blob` picks |
| `crates/reborn-core/src/publish/release.rs` | modify | blob presence check against the right store |
| `crates/reborn-core/src/publish/upload.rs` | modify | `plan_private` |
| `crates/reborn-core/src/publish/game_full.rs` | modify | use `publish::default_stage` |
| `crates/reborn-core/src/publish/league.rs` | create | stage + publish `league-game` / `league-content` |
| `crates/reborn-cli/src/publish.rs` | modify | `publish league-game`, `publish league-content`, `publish upload --private` |
| `crates/reborn-publisher/src/league.rs` + `ui/league.{html,js}` | create | one page: "Publish league files" |
| `crates/reborn-publisher/src/settings.rs` | modify | `league_bucket_name` (default `reborn-league`) |
| `crates/reborn-publisher/src/main.rs`, `ui/shell.js` | modify | register commands, sidebar entry |
| `docs/RUNNING_UPDATES.md` | modify | "League files (members only)" section |

---

### Task 0: Worktree and baseline

**Files:** none.

- [ ] **Step 1: Create the worktree** (git is slow here; use 300000 ms timeouts)

```bash
cd "/c/Users/Adam/Desktop/Reborn Installer/NFL Street Reborn Launcher league"
git worktree add -b feat/league-publish "../NFL Street Reborn Launcher leaguepub" port/linux-deck-on-3.0.35
cd "../NFL Street Reborn Launcher leaguepub" && git log --oneline -1
```
Expected: `6b48d3c Game image: imports reach the newest build; ...`

- [ ] **Step 2: Baseline**

```bash
CARGO_BUILD_JOBS=1 cargo test -p reborn-core --features publish publish:: 2>&1 | tail -5
CARGO_BUILD_JOBS=1 cargo test -p reborn-core --features publish inventory:: manifest:: 2>&1 | tail -5
```
Record the pass counts in the report. Any failure counts as the baseline: note it and don't fix it.

---

### Task 1: League component identities and the `members_only` field

**Files:**
- Modify: `crates/reborn-core/src/platform/mod.rs` (the `DataDir` enum, `relative`, `ALL`)
- Modify: `crates/reborn-core/src/inventory.rs` (`ComponentId`, `ALL`, `as_str`, new `is_members_only`, `install_root`, the `Component { … }` literal in the scan, `SerializedComponent::new`)
- Modify: `crates/reborn-core/src/manifest.rs` (`Component`)
- Test: `manifest.rs` and `inventory.rs` test modules

**Interfaces:**
- Produces: `ComponentId::LeagueGame` (`"league-game"`), `ComponentId::LeagueContent` (`"league-content"`), `ComponentId::is_members_only(self) -> bool`, `DataDir::LibraryLeague` (`"library/league"`), `DataDir::LibraryLeagueContent` (`"library/league-content"`), `Component.members_only: bool`.

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `manifest.rs`:

```rust
    /// Every manifest published before league content existed has no
    /// `members_only` key. It must still parse, as public.
    #[test]
    fn a_manifest_without_members_only_parses_as_public() {
        let c = Component::parse(component_json("offline-content", true).as_bytes()).unwrap();
        assert!(!c.members_only);
    }

    /// And it must serialize back without the key, byte for byte, or the
    /// release signer's round-trip check refuses every old manifest.
    #[test]
    fn a_public_manifest_serializes_without_the_members_only_key() {
        let c = Component::parse(component_json("runtime", false).as_bytes()).unwrap();
        let out = serde_json::to_string_pretty(&c).unwrap();
        assert!(!out.contains("members_only"), "{out}");
    }

    #[test]
    fn a_members_only_manifest_says_so_and_round_trips() {
        let json = component_json("league-game", false)
            .replace("\"customizable\": false,", "\"customizable\": false,\n  \"members_only\": true,");
        let c = Component::parse(json.as_bytes()).unwrap();
        assert!(c.members_only);
        let out = serde_json::to_string_pretty(&c).unwrap();
        assert!(out.contains("\"members_only\": true"), "{out}");
    }
```

Append to the `tests` module in `inventory.rs`:

```rust
    #[test]
    fn only_the_two_league_components_are_members_only() {
        for id in ComponentId::ALL {
            let expected = matches!(id, ComponentId::LeagueGame | ComponentId::LeagueContent);
            assert_eq!(id.is_members_only(), expected, "{id:?}");
        }
    }

    #[test]
    fn league_components_have_fixed_wire_names_and_install_roots() {
        assert_eq!(ComponentId::LeagueGame.as_str(), "league-game");
        assert_eq!(ComponentId::LeagueContent.as_str(), "league-content");
        assert_eq!(ComponentId::parse("league-game"), Some(ComponentId::LeagueGame));
        assert_eq!(ComponentId::parse("league-content"), Some(ComponentId::LeagueContent));
        assert_eq!(ComponentId::LeagueGame.install_root(), Some("library/league"));
        assert_eq!(ComponentId::LeagueContent.install_root(), Some("library/league-content"));
        assert!(!ComponentId::LeagueGame.is_customizable());
    }

    #[test]
    fn a_scanned_league_tree_is_marked_members_only_and_a_retail_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), b"league").unwrap();
        for (id, expected) in [(ComponentId::LeagueGame, true), (ComponentId::Runtime, false)] {
            let spec = ComponentSpec::new(id, "1.0.0", 1).unwrap();
            let ScanOutcome::Complete(scan) = scan_component(dir.path(), &spec, &RunToCompletion).unwrap() else {
                panic!("cancelled")
            };
            let ser = SerializedComponent::new(scan.component).unwrap();
            assert_eq!(ser.component().members_only, expected, "{id:?}");
        }
    }
```

(If the `inventory.rs` test module does not already import `tempfile`, `scan_component`, `ScanOutcome`, `RunToCompletion` and `SerializedComponent`, add them with `use super::*;`. They are all defined in this file.)

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core --features publish members_only league_components a_scanned_league 2>&1 | tail -20`
Expected: compile errors, `no variant named LeagueGame` and `no field members_only`.

- [ ] **Step 3: Implement**

`platform/mod.rs`: add to `enum DataDir`, after `LibraryGameImport`:

```rust
    /// The league build (`league-game`): `game/league.iso`, the league
    /// texture tree `textures/<league game id>/` and `league.json`.
    /// Members only: its blobs come from the backend, never the public CDN.
    LibraryLeague,
    /// The league seed (`league-content`): `seed/league.s01`.
    LibraryLeagueContent,
```

In `relative()`, add `DataDir::LibraryLeague => "library/league",` and `DataDir::LibraryLeagueContent => "library/league-content",`. In `ALL`, add both after `DataDir::LibraryGameImport,`.

`manifest.rs`: in `struct Component`, directly after `pub customizable: bool,`:

```rust
    /// Whether this component's blobs are withheld from the public CDN.
    ///
    /// True for `league-game` and `league-content` only, derived from the
    /// identity like `customizable`. The manifest itself is public and pinned
    /// like any other. A launcher fetches the blobs through the backend's
    /// `POST /v1/league/download` (signed, short-lived URLs), never from
    /// `<origin>/blobs/by-hash`. Absent means false, and false is never
    /// written, so every manifest published before this field existed still
    /// round-trips byte for byte.
    #[serde(default, skip_serializing_if = "is_false")]
    pub members_only: bool,
```

and at module level in `manifest.rs`:

```rust
fn is_false(b: &bool) -> bool {
    !*b
}
```

`inventory.rs`:
- In `enum ComponentId`, after `GameImport,`:

```rust
    /// The league build: league ISO + league texture tree. Members only.
    LeagueGame,
    /// The league seed save state. Members only.
    LeagueContent,
```
- In `ALL`, add `ComponentId::LeagueGame, ComponentId::LeagueContent,` at the end.
- In `as_str`, add `ComponentId::LeagueGame => "league-game", ComponentId::LeagueContent => "league-content",`.
- After `is_customizable`, add:

```rust
    /// Whether the blobs are withheld from the public CDN and handed out by
    /// the backend to league members only. A property of the identity, like
    /// [`Self::is_customizable`], so a generator cannot get it wrong.
    pub fn is_members_only(self) -> bool {
        matches!(self, ComponentId::LeagueGame | ComponentId::LeagueContent)
    }
```
- In `install_root`, add `ComponentId::LeagueGame => Some(DataDir::LibraryLeague.relative()), ComponentId::LeagueContent => Some(DataDir::LibraryLeagueContent.relative()),`.
- In the scan's `Component { … }` literal (the one with `customizable: spec.id.is_customizable(),`), add `members_only: spec.id.is_members_only(),`.
- In `SerializedComponent::new`, after the existing `parsed.customizable != component.customizable` check, add the same check for `members_only`:

```rust
        if parsed.members_only != component.members_only {
            return Err(refuse(format!(
                "the manifest this generator produced for {:?} lost its members_only \
                 flag on the way out. Publishing it would put members-only content \
                 on the public CDN",
                component.component
            )));
        }
```

Then build: `CARGO_BUILD_JOBS=1 cargo build -p reborn-core --features publish 2>&1 | grep -E "^error" -A5`. Every **exhaustive** `match` on `ComponentId` or `DataDir` that no longer compiles gets an arm. Treat the two league ids exactly like `GameFull`: never auto-updated, no special handling. Any other `Component { … }` literal (tests, fixtures) gets `members_only: false`. List each touched site in the report.

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core --features publish 2>&1 | tail -8`
Expected: the new tests pass, and the totals are baseline + 6 with no new failures.

- [ ] **Step 5: Commit**

```bash
git add -A crates/reborn-core
git commit -m "core: league-game + league-content components, members_only manifest flag

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The private blob store

**Files:**
- Modify: `crates/reborn-core/src/origin.rs`, `publish/mod.rs`, `publish/component.rs`, `publish/release.rs`, `publish/upload.rs`
- Test: `publish/release.rs` and `publish/upload.rs` test modules

**Interfaces:**
- Consumes: `ComponentId::is_members_only`, `Component.members_only` (Task 1).
- Produces: `origin::PRIVATE_BLOB_DIR = "private/blobs/by-hash"`, `origin::private_blob_path(&Sha256Hex) -> String`, `publish::stored_blob(origin: &Path, members_only: bool, sha: &Sha256Hex) -> PathBuf`, `upload::plan_private(origin: &Path, already: &dyn Fn(&str) -> Option<u64>) -> Result<UploadPlan>`.

- [ ] **Step 1: Write the failing tests**

Append to `release.rs`'s `tests` module (it already has `world`, `publish`, `sign`, `pin`):

```rust
    #[test]
    fn a_league_component_lands_in_the_private_store_and_signs() {
        let w = world();
        publish(&w, ComponentId::LeagueGame, "2026.10.08", 1, &[("game/league.iso", b"ZZLG01 league disc")]);
        let sha = crate::manifest::Sha256Hex::from_bytes(b"ZZLG01 league disc");
        assert!(w.origin.join(crate::origin::private_blob_path(&sha)).is_file(), "private copy");
        assert!(!w.origin.join(crate::origin::blob_path(&sha)).exists(), "never public");
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::LeagueGame, "2026.10.08")], true);
        assert!(outcome.is_done(), "{lines:#?}");
    }

    #[test]
    fn a_league_blob_shared_with_retail_is_still_stored_privately() {
        let w = world();
        publish(&w, ComponentId::Runtime, "1.0.0", 1, &[("same.bin", b"identical")]);
        publish(&w, ComponentId::LeagueGame, "2026.10.08", 1, &[("textures/ZZLG01/same.bin", b"identical")]);
        let sha = crate::manifest::Sha256Hex::from_bytes(b"identical");
        assert!(w.origin.join(crate::origin::blob_path(&sha)).is_file());
        assert!(w.origin.join(crate::origin::private_blob_path(&sha)).is_file());
    }

    #[test]
    fn a_league_pin_whose_private_blob_is_missing_is_refused_even_if_a_public_copy_exists() {
        let w = world();
        publish(&w, ComponentId::LeagueGame, "2026.10.08", 1, &[("game/league.iso", b"disc")]);
        let sha = crate::manifest::Sha256Hex::from_bytes(b"disc");
        let private = w.origin.join(crate::origin::private_blob_path(&sha));
        let public = w.origin.join(crate::origin::blob_path(&sha));
        std::fs::create_dir_all(public.parent().unwrap()).unwrap();
        std::fs::rename(&private, &public).unwrap();
        let (outcome, lines) = sign(&w, "3.0.0", 1, &[pin(ComponentId::LeagueGame, "2026.10.08")], true);
        assert!(!outcome.is_done(), "{lines:#?}");
        assert!(lines.iter().any(|l| l.contains("blob(s) are not in the origin")), "{lines:#?}");
    }
```

Append to `upload.rs`'s test module:

```rust
    fn write_blob(dir: &std::path::Path, bytes: &[u8]) -> Sha256Hex {
        let sha = Sha256Hex::from_bytes(bytes);
        let p = dir.join(&sha.as_str()[..2]).join(sha.as_str());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
        sha
    }

    #[test]
    fn the_public_plan_never_contains_a_private_blob() {
        let t = tempfile::tempdir().unwrap();
        write_blob(&t.path().join(crate::origin::PRIVATE_BLOB_DIR), b"league only");
        let plan = plan_origin(t.path(), &|_| None).unwrap();
        assert!(plan.is_empty(), "{:?}", plan.items().map(|i| &i.key).collect::<Vec<_>>());
    }

    #[test]
    fn the_private_plan_holds_only_private_blobs_under_the_public_key_layout() {
        let t = tempfile::tempdir().unwrap();
        let public = write_blob(&t.path().join(BLOB_DIR), b"retail");
        let private = write_blob(&t.path().join(crate::origin::PRIVATE_BLOB_DIR), b"league only");
        let plan = plan_private(t.path(), &|_| None).unwrap();
        let keys: Vec<&str> = plan.items().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, vec![blob_path(&private).as_str()]);
        assert!(!keys.contains(&blob_path(&public).as_str()));
        assert!(plan.items().all(|i| i.group == Group::Blob));
        let already_there = plan_private(t.path(), &|k| (k == blob_path(&private)).then_some(11)).unwrap();
        assert!(already_there.is_empty(), "a blob the bucket holds at the right length is skipped");
    }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core --features publish private 2>&1 | tail -15`
Expected: compile errors for `private_blob_path`, `PRIVATE_BLOB_DIR` and `plan_private`.

- [ ] **Step 3: Implement**

`origin.rs`, after `BLOB_DIR`:

```rust
/// Where members-only blobs (league content) sit in the LOCAL origin.
/// Deliberately outside `blobs/`: `plan_origin` and every rclone push of
/// `blobs/` to the public bucket can never pick them up. On the private
/// bucket (`reborn-league`) the same object lives at the ordinary
/// [`blob_path`] key; only the local directory differs.
pub const PRIVATE_BLOB_DIR: &str = "private/blobs/by-hash";

/// A members-only blob's path relative to the local origin.
pub fn private_blob_path(sha256: &Sha256Hex) -> String {
    let s = sha256.as_str();
    format!("{PRIVATE_BLOB_DIR}/{}/{s}", &s[..2])
}
```

`publish/mod.rs`, next to `manifest_path`:

```rust
/// Where a component's blob is stored in the local origin: the private store
/// for members-only components, the public one for everything else. The one
/// place this choice is made, so the publisher and the release signer cannot
/// disagree about it.
pub fn stored_blob(origin: &Path, members_only: bool, sha256: &crate::manifest::Sha256Hex) -> PathBuf {
    if members_only {
        origin.join(crate::origin::private_blob_path(sha256))
    } else {
        origin.join(crate::origin::blob_path(sha256))
    }
}
```

`publish/component.rs`: in `publish_component`, replace `let dst = origin.join(blob_path(&file.sha256));` with `let dst = crate::publish::stored_blob(origin, spec.id().is_members_only(), &file.sha256);`. Remove the then-unused `blob_path` import if the compiler says so. After the "Blobs" summary line, add:

```rust
    if spec.id().is_members_only() {
        out!(sink, "              Members only: blobs go to {}, never the public CDN.", crate::origin::PRIVATE_BLOB_DIR);
    }
```

`publish/release.rs`: in `release()`, replace `let blob = origin.join(blob_path(&file.sha256));` with `let blob = crate::publish::stored_blob(origin, serialized.component().members_only, &file.sha256);`.

`publish/upload.rs`: import `PRIVATE_BLOB_DIR` next to `BLOB_DIR`, then add after `plan_origin`:

```rust
/// The members-only blobs, for the PRIVATE bucket (`reborn-league`): every
/// file under `<origin>/private/blobs/by-hash` the bucket does not already
/// hold at the right length, keyed by the ordinary `blobs/by-hash/<xx>/<sha>`
/// layout. Blobs only. The manifests and release files that name them are
/// public and go up through [`plan_origin`] to the public bucket, after this
/// plan has finished.
pub fn plan_private(origin: &Path, already: &dyn Fn(&str) -> Option<u64>) -> Result<UploadPlan> {
    let mut plan = UploadPlan::new();
    let dir = origin.join(PRIVATE_BLOB_DIR);
    if !dir.is_dir() {
        return Ok(plan);
    }
    let mut blobs: Vec<(String, PathBuf, u64, Sha256Hex)> = Vec::new();
    for prefix in sorted_dir_entries(&dir)? {
        if !prefix.is_dir() {
            continue;
        }
        for file in sorted_dir_entries(&prefix)? {
            if !file.is_file() {
                continue;
            }
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let sha256 = Sha256Hex::parse(&name)
                .map_err(|_| invalid(format!("{} is not a sha-256 file name; refusing to plan its upload", file.display())))?;
            let bytes = std::fs::metadata(&file).map_err(|e| io_err(&file, e))?.len();
            blobs.push((blob_path(&sha256), file, bytes, sha256));
        }
    }
    blobs.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, from, bytes, sha256) in blobs {
        if already(&key) != Some(bytes) {
            plan.blob(key, from, bytes, sha256);
        }
    }
    Ok(plan)
}
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core --features publish 2>&1 | tail -8`
Expected: all pass, Task 1's total + 5.

- [ ] **Step 5: Commit**

```bash
git add -A crates/reborn-core
git commit -m "publish: private blob store for members-only components + plan_private

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `rebornctl publish upload --private`

**Files:**
- Modify: `crates/reborn-cli/src/publish.rs` (`PublishCommand::Upload`, `run`, `upload_command`)

**Interfaces:**
- Consumes: `plan_private` (Task 2).
- Produces: `rebornctl publish upload --origin <o> --private --bucket reborn-league [--apply]`.

- [ ] **Step 1: Write the failing test**

Append to `crates/reborn-cli/src/publish.rs` (create the module if the file has none):

```rust
#[cfg(test)]
mod private_upload_tests {
    use super::private_target;

    #[test]
    fn a_private_upload_needs_a_named_bucket() {
        assert!(private_target(None, None).is_err());
    }

    #[test]
    fn a_private_upload_never_reads_back_through_the_public_cdn() {
        assert!(private_target(Some("reborn-league"), Some("https://files.nflstreetreborn.com/")).is_err());
    }

    #[test]
    fn a_private_upload_refuses_the_public_bucket() {
        assert!(private_target(Some("reborn-content"), None).is_err());
    }

    #[test]
    fn the_league_bucket_is_accepted() {
        assert_eq!(private_target(Some("reborn-league"), None), Ok("reborn-league".to_string()));
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-cli private_upload 2>&1 | tail -10`
Expected: `cannot find function private_target`.

- [ ] **Step 3: Implement**

In `PublishCommand::Upload`, after `public_base`:

```rust
        /// Upload the MEMBERS-ONLY blobs (`<origin>\private\blobs`) to the
        /// private bucket named by --bucket, and nothing else. Run this
        /// BEFORE the public upload that carries their manifests. Never
        /// reads back through the public CDN.
        #[arg(long)]
        private: bool,
```

In `run`, destructure `private` and pass it: `upload_command(&origin, endpoint.as_deref(), bucket.as_deref(), public_base.as_deref(), private, apply, &mut sink)`.

Add the helper:

```rust
/// The bucket a private upload goes to, or why it cannot run. The public
/// bucket is refused by name, so a typo can never send league blobs where
/// the CDN serves them.
fn private_target(bucket: Option<&str>, public_base: Option<&str>) -> Result<String, String> {
    let Some(name) = bucket.map(str::trim).filter(|b| !b.is_empty()) else {
        return Err("--private needs --bucket <private bucket>, for example --bucket reborn-league.".to_string());
    };
    if public_base.is_some() {
        return Err("--private never reads back through a public host; drop --public-base.".to_string());
    }
    if name == "reborn-content" {
        return Err("reborn-content is the PUBLIC bucket. Members-only blobs never go there.".to_string());
    }
    Ok(name.to_string())
}
```

In `upload_command`, add the `private: bool` parameter after `public_base`. Then replace everything from `let transport = UreqTransport::new();` to the end of the function with:

```rust
    if private {
        match private_target(bucket_name, public_base) {
            Ok(name) => bucket.bucket = name,
            Err(why) => {
                eprintln!("{why}");
                return Ok(misuse());
            }
        }
    }

    let transport = UreqTransport::new();
    let (already, how) = reborn_core::publish::upload::already_on_bucket(&transport, &bucket)?;
    sink(Event::out(how));
    let plan = if private {
        reborn_core::publish::upload::plan_private(origin, &*already)?
    } else {
        plan_origin(origin, &*already)?
    };

    let http = reborn_core::net::UreqHttp::new(concat!("rebornctl/", env!("CARGO_PKG_VERSION")));
    let public_base_owned = public_base.map(str::to_string).unwrap_or_else(|| reborn_core::origin::DEFAULT_ORIGIN.to_string());
    let req = UploadRequest {
        bucket: &bucket,
        plan: &plan,
        multipart_threshold: DEFAULT_MULTIPART_THRESHOLD,
        part_size: DEFAULT_PART_SIZE,
        attempts: DEFAULT_ATTEMPTS,
        public_base: if private { None } else { Some(&public_base_owned) },
        apply,
    };
    Ok(exit_of(&run_upload(&transport, &http, &req, sink)?))
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-cli 2>&1 | tail -6`
Expected: 4 new tests pass and nothing regresses.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-cli/src/publish.rs
git commit -m "rebornctl: publish upload --private (league bucket, no public read-back)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Stage and publish the league build and seed (`publish/league.rs`)

**Files:**
- Create: `crates/reborn-core/src/publish/league.rs`
- Modify: `crates/reborn-core/src/publish/mod.rs` (`pub mod league;`, move `default_stage` here)
- Modify: `crates/reborn-core/src/publish/game_full.rs` (use `crate::publish::default_stage`; delete its private copy)

**Interfaces:**
- Consumes: `ComponentId::{LeagueGame, LeagueContent}`, private store (Tasks 1-2), `publish_component`, `republish_pinning`/`CarryForward`, `next_sequence`, `roster::Push`.
- Produces:
  - `pub const LEAGUE_ISO_PATH: &str = "game/league.iso"`, `LEAGUE_INFO_FILE = "league.json"`, `LEAGUE_SEED_PATH = "seed/league.s01"`, `LEAGUE_SEED_JSON_PATH = "seed/seed.json"`, `PLACEHOLDER_GAME_ID = "LGXXXX"`.
  - `pub fn read_game_id(iso: &Path) -> Result<String>`
  - `pub fn check_league_game_id(id: &str) -> std::result::Result<(), String>`
  - `pub struct LeagueGameRequest<'a>` / `pub fn publish_league_game(req, observer, sink) -> Result<Outcome<Push>>`
  - `pub struct LeagueContentRequest<'a>` / `pub fn publish_league_content(req, observer, sink) -> Result<Outcome<Push>>`
  - Staged `league.json`: `{"schema":"nsr.league-game.v1","game_id":"<ID>","iso":"game/league.iso","textures":"textures/<ID>"}`. This is a contract addition for Plan 3b: the launcher cross-checks it against its `LEAGUE_GAME_ID`.

- [ ] **Step 1: Write the failing tests** (bottom of the new `league.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::RunToCompletion;
    use crate::manifest::Component;
    use crate::publish::keys::keygen;
    use crate::publish::{manifest_path, Event};

    struct World {
        dir: tempfile::TempDir,
        origin: PathBuf,
        key: PathBuf,
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        let key = match keygen(&dir.path().join("keys"), &mut |_| {}).unwrap() {
            Outcome::Done(w) => w.publisher_path,
            o => panic!("{o:?}"),
        };
        World { dir, origin, key }
    }

    fn iso(w: &World, id: &[u8; 6]) -> PathBuf {
        let p = w.dir.path().join("league-src.iso");
        let mut bytes = id.to_vec();
        bytes.extend_from_slice(&[0u8; 2048]);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn textures(w: &World, folder: &str, files: &[&str]) -> PathBuf {
        let root = w.dir.path().join("tex").join(folder);
        for f in files {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, f.as_bytes()).unwrap();
        }
        root
    }

    fn game_req<'a>(w: &'a World, iso: &'a Path, tex: &'a Path, apply: bool) -> LeagueGameRequest<'a> {
        LeagueGameRequest {
            iso, textures: tex, version: "2026.10.08", stage: None, origin: &w.origin,
            sequence: None, key: Some(&w.key), release_version: Some("3.0.0"), release_sequence: Some(1),
            notes: Some("League build 1"), apply,
        }
    }

    fn run_game(r: &LeagueGameRequest<'_>) -> (Outcome<Push>, Vec<String>) {
        let mut lines = Vec::new();
        (publish_league_game(r, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap(), lines)
    }

    #[test]
    fn the_game_id_is_the_first_six_bytes_of_the_disc() {
        let w = world();
        assert_eq!(read_game_id(&iso(&w, b"ZZLG01")).unwrap(), "ZZLG01");
    }

    #[test]
    fn retail_ids_the_placeholder_and_garbage_are_refused() {
        for bad in ["GN7E69", "FR4N31", "LGXXXX", "zzlg01", "ZZ-G01", "ZZLG0"] {
            assert!(check_league_game_id(bad).is_err(), "{bad}");
        }
        assert!(check_league_game_id("ZZLG01").is_ok());
    }

    #[test]
    fn a_retail_disc_is_refused_before_anything_is_staged() {
        let w = world();
        let disc = iso(&w, b"GN7E69");
        let tex = textures(&w, "GN7E69", &["a.dds"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(!outcome.is_done(), "{lines:#?}");
        assert!(!w.origin.exists(), "nothing written");
    }

    #[test]
    fn a_textures_folder_for_another_game_is_refused() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "GN7E69", &["a.dds"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        assert!(!outcome.is_done());
        assert!(lines.iter().any(|l| l.contains("ZZLG01")), "{lines:#?}");
    }

    #[test]
    fn a_folder_holding_the_id_folder_is_accepted() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let parent = textures(&w, "pack", &["ZZLG01/a.dds"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &parent, true));
        assert!(outcome.is_done(), "{lines:#?}");
    }

    #[test]
    fn a_dry_run_writes_nothing() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds"]);
        let (outcome, _) = run_game(&game_req(&w, &disc, &tex, false));
        assert!(matches!(outcome, Outcome::Described));
        assert!(!w.origin.exists());
    }

    #[test]
    fn apply_stages_publishes_privately_skips_logs_and_signs() {
        let w = world();
        let disc = iso(&w, b"ZZLG01");
        let tex = textures(&w, "ZZLG01", &["a.dds", "sub/b.png", "texconv.log", "c.dds.blanked_off"]);
        let (outcome, lines) = run_game(&game_req(&w, &disc, &tex, true));
        let Outcome::Done(push) = outcome else { panic!("{lines:#?}") };
        assert!(push.release.is_some(), "signed");

        let bytes = std::fs::read(manifest_path(&w.origin, ComponentId::LeagueGame, "2026.10.08")).unwrap();
        let c = Component::parse(&bytes).unwrap();
        assert!(c.members_only);
        assert_eq!(c.install_root, "library/league");
        let mut paths: Vec<&str> = c.files.iter().map(|f| f.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["game/league.iso", "league.json", "textures/ZZLG01/a.dds", "textures/ZZLG01/sub/b.png"]);

        let info = c.files.iter().find(|f| f.path == "league.json").unwrap();
        let info_bytes = std::fs::read(w.origin.join(crate::origin::private_blob_path(&info.sha256))).unwrap();
        let info: serde_json::Value = serde_json::from_slice(&info_bytes).unwrap();
        assert_eq!(info["game_id"], "ZZLG01");
        assert_eq!(info["schema"], "nsr.league-game.v1");
        assert!(!w.origin.join(crate::origin::BLOB_DIR).exists(), "no public blobs at all");
    }

    #[test]
    fn the_league_seed_is_staged_as_league_s01() {
        let w = world();
        let seed = w.dir.path().join("captured.s01");
        std::fs::write(&seed, b"seed state").unwrap();
        let req = LeagueContentRequest {
            seed: &seed, seed_json: None, version: "1.0.0", stage: None, origin: &w.origin,
            sequence: None, key: Some(&w.key), release_version: Some("3.0.0"), release_sequence: Some(1),
            notes: None, apply: true,
        };
        let mut lines = Vec::new();
        let outcome = publish_league_content(&req, &RunToCompletion, &mut |e: Event| lines.push(e.line)).unwrap();
        assert!(outcome.is_done(), "{lines:#?}");
        let c = Component::parse(&std::fs::read(manifest_path(&w.origin, ComponentId::LeagueContent, "1.0.0")).unwrap()).unwrap();
        assert!(c.members_only);
        assert_eq!(c.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["seed/league.s01"]);
    }

    #[test]
    fn a_seed_that_is_not_an_s01_is_refused() {
        let w = world();
        let seed = w.dir.path().join("notes.txt");
        std::fs::write(&seed, b"x").unwrap();
        let req = LeagueContentRequest {
            seed: &seed, seed_json: None, version: "1.0.0", stage: None, origin: &w.origin,
            sequence: None, key: None, release_version: None, release_sequence: None, notes: None, apply: true,
        };
        assert!(!publish_league_content(&req, &RunToCompletion, &mut |_| {}).unwrap().is_done());
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Add `pub mod league;` to `publish/mod.rs` and create `league.rs` holding only the test module. Then run:
`CARGO_BUILD_JOBS=1 cargo test -p reborn-core --features publish publish::league 2>&1 | tail -10`
Expected: compile errors for the missing items.

- [ ] **Step 3: Implement** (top of `league.rs`, above the tests)

First move `default_stage` from `game_full.rs` into `publish/mod.rs` as `pub(crate) fn default_stage(origin: &Path, leaf: &str) -> PathBuf`, keeping the same body and doc. In `game_full.rs`, delete it and `use crate::publish::default_stage;`.

```rust
//! The league build and the league seed: two MEMBERS-ONLY components.
//!
//! `league-game` is the league's own disc (its own game id) plus its texture
//! tree. `league-content` is the seed save state league games boot from.
//! Both publish through the ordinary component + release path. What differs
//! is where their blobs go: `private/blobs/by-hash` in the local origin
//! (`crate::publish::stored_blob`), then the private bucket `reborn-league`
//! (`rebornctl publish upload --private`). The backend hands those out to
//! league members as short-lived signed URLs. Their manifests are public
//! and pinned like any other.
//!
//! The stage is a fresh tree per version under `<origin>-stage`, built from
//! hard links (copies across volumes), for the same reason as
//! `game_full::stage`: a component is every file under its root, so
//! publishing straight from a working folder would publish whatever else
//! sits there. Texture logs and `.blanked_off` backups are dropped while
//! staging; textures 1.0.0 once shipped `texconv.log` to every player.

use std::path::{Path, PathBuf};

use crate::inventory::{ComponentId, ComponentSpec, ScanObserver};
use crate::publish::component::{publish_component, ComponentRequest};
use crate::publish::release::{republish_pinning, CarryForward};
use crate::publish::roster::Push;
use crate::publish::{default_stage, io_err, next_sequence, out, refuse, thousands, Outcome, Sink};
use crate::Result;

pub const LEAGUE_ISO_PATH: &str = "game/league.iso";
pub const LEAGUE_INFO_FILE: &str = "league.json";
pub const LEAGUE_SEED_PATH: &str = "seed/league.s01";
pub const LEAGUE_SEED_JSON_PATH: &str = "seed/seed.json";
/// The launcher's stand-in until Adam names the league game id. A disc
/// carrying it is a test disc and must never be published.
pub const PLACEHOLDER_GAME_ID: &str = "LGXXXX";
const RETAIL_GAME_IDS: &[&str] = &["GN7E69", "FR4N31"];

#[derive(Debug)]
pub struct LeagueGameRequest<'a> {
    pub iso: &'a Path,
    /// The league texture folder: either named after the game id, or holding
    /// a folder named after it.
    pub textures: &'a Path,
    pub version: &'a str,
    /// Defaults to `<origin>-stage\league-game-<version>`.
    pub stage: Option<&'a Path>,
    pub origin: &'a Path,
    pub sequence: Option<u64>,
    pub key: Option<&'a Path>,
    pub release_version: Option<&'a str>,
    pub release_sequence: Option<u64>,
    pub notes: Option<&'a str>,
    pub apply: bool,
}

#[derive(Debug)]
pub struct LeagueContentRequest<'a> {
    /// The captured league seed (a `.s01` save state).
    pub seed: &'a Path,
    /// make_seed.py's capture record, when there is one.
    pub seed_json: Option<&'a Path>,
    pub version: &'a str,
    /// Defaults to `<origin>-stage\league-content-<version>`.
    pub stage: Option<&'a Path>,
    pub origin: &'a Path,
    pub sequence: Option<u64>,
    pub key: Option<&'a Path>,
    pub release_version: Option<&'a str>,
    pub release_sequence: Option<u64>,
    pub notes: Option<&'a str>,
    pub apply: bool,
}

/// The six-character game id a GameCube disc carries in its first bytes.
pub fn read_game_id(iso: &Path) -> Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(iso).map_err(|e| io_err(iso, e))?;
    let mut id = [0u8; 6];
    f.read_exact(&mut id).map_err(|e| io_err(iso, e))?;
    Ok(String::from_utf8_lossy(&id).into_owned())
}

/// A league game id is six capitals or digits, and is not a retail id or
/// the launcher's placeholder.
pub fn check_league_game_id(id: &str) -> std::result::Result<(), String> {
    if id.len() != 6 || !id.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
        return Err(format!("{id:?} is not a GameCube game id (six capitals or digits)."));
    }
    if RETAIL_GAME_IDS.contains(&id) {
        return Err(format!("{id} is a RETAIL game id. The league build must carry its own id."));
    }
    if id == PLACEHOLDER_GAME_ID {
        return Err(format!("{id} is the launcher's placeholder, not a real league id. Set the real id in the disc first."));
    }
    Ok(())
}

/// The folder whose CONTENTS are the textures for `id`.
fn resolve_textures(textures: &Path, id: &str) -> std::result::Result<PathBuf, String> {
    if textures.file_name().and_then(|n| n.to_str()) == Some(id) && textures.is_dir() {
        return Ok(textures.to_path_buf());
    }
    let child = textures.join(id);
    if child.is_dir() {
        return Ok(child);
    }
    Err(format!(
        "{} is not a texture folder for {id}: name the folder {id}, or point at the folder that holds {id}.",
        textures.display()
    ))
}

fn skipped(name: &str) -> bool {
    name.ends_with(".log") || name.contains(".blanked_off")
}

/// Hard-link (else copy) every file under `from` into `to`, keeping relative
/// paths and dropping logs and backups. Returns (files, bytes, dropped).
fn link_tree(from: &Path, to: &Path) -> Result<(u64, u64, u64)> {
    let (mut files, mut bytes, mut dropped) = (0u64, 0u64, 0u64);
    for entry in std::fs::read_dir(from).map_err(|e| io_err(from, e))? {
        let entry = entry.map_err(|e| io_err(from, e))?;
        let src = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let dst = to.join(&name);
        if src.is_dir() {
            let (f, b, d) = link_tree(&src, &dst)?;
            files += f;
            bytes += b;
            dropped += d;
        } else if skipped(&name) {
            dropped += 1;
        } else {
            std::fs::create_dir_all(to).map_err(|e| io_err(to, e))?;
            if std::fs::hard_link(&src, &dst).is_err() {
                std::fs::copy(&src, &dst).map_err(|e| io_err(&dst, e))?;
            }
            files += 1;
            bytes += std::fs::metadata(&dst).map_err(|e| io_err(&dst, e))?.len();
        }
    }
    Ok((files, bytes, dropped))
}

fn link_file(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let _ = std::fs::remove_file(dst);
    if std::fs::hard_link(src, dst).is_err() {
        std::fs::copy(src, dst).map_err(|e| io_err(dst, e))?;
    }
    Ok(())
}

/// A stage directory is ours to clear only when it is empty or a previous
/// stage of the same component (it carries our marker file).
fn fresh_stage(dir: &Path, marker: &str) -> std::result::Result<(), String> {
    if dir.exists() {
        let empty = std::fs::read_dir(dir).map(|mut d| d.next().is_none()).unwrap_or(false);
        if !empty && !dir.join(marker).exists() {
            return Err(format!("{} exists and is not a league stage. Choose another --stage.", dir.display()));
        }
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

fn publish_staged(
    id: ComponentId,
    stage_dir: &Path,
    origin: &Path,
    version: &str,
    sequence: Option<u64>,
    carry: (Option<&Path>, Option<&str>, Option<u64>, Option<&str>),
    observer: &dyn ScanObserver,
    sink: Sink<'_>,
) -> Result<Outcome<Push>> {
    let sequence = match sequence {
        Some(s) => s,
        None => next_sequence(origin, id)?,
    };
    let spec = match ComponentSpec::new(id, version, sequence) {
        Ok(s) => s,
        Err(e) => return Ok(refuse(sink, &[e.to_string()])),
    };
    let component = match publish_component(&ComponentRequest { root: stage_dir, spec, origin, apply: true }, observer, sink)? {
        Outcome::Done(c) => c,
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Described => return Ok(Outcome::Described),
    };
    let (key, release_version, release_sequence, notes) = carry;
    let req = CarryForward { origin, id, version, key, release_version, release_sequence, notes, apply: true };
    Ok(match republish_pinning(&req, sink)? {
        Outcome::Done(s) => Outcome::Done(Push { component, release: Some(s) }),
        Outcome::Described => Outcome::Done(Push { component, release: None }),
        Outcome::Refused(r) => Outcome::Refused(r),
    })
}

/// Stage the league disc + textures + league.json, publish `league-game`
/// (blobs to the private store), and re-sign the release carrying every
/// other pin forward.
pub fn publish_league_game(req: &LeagueGameRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>> {
    if !req.iso.is_file() {
        return Ok(refuse(sink, &[format!("--iso {} is not a file. Nothing was published.", req.iso.display())]));
    }
    let id = read_game_id(req.iso)?;
    if let Err(why) = check_league_game_id(&id) {
        return Ok(refuse(sink, &[why, "Nothing was published.".to_string()]));
    }
    let tex = match resolve_textures(req.textures, &id) {
        Ok(t) => t,
        Err(why) => return Ok(refuse(sink, &[why, "Nothing was published.".to_string()])),
    };
    out!(sink, "League disc   {} (game id {id})", req.iso.display());
    out!(sink, "Textures      {}", tex.display());
    out!(sink, "Origin        {}", req.origin.display());

    if !req.apply {
        out!(sink);
        out!(sink, "Nothing was staged or written. Re-run with --apply to stage, publish league-game");
        out!(sink, "{} (members only) and re-sign the release.", req.version);
        return Ok(Outcome::Described);
    }

    let stage_dir = match req.stage {
        Some(s) => s.to_path_buf(),
        None => default_stage(req.origin, &format!("league-game-{}", req.version)),
    };
    if let Err(why) = fresh_stage(&stage_dir, LEAGUE_INFO_FILE) {
        return Ok(refuse(sink, &[why]));
    }
    link_file(req.iso, &stage_dir.join(LEAGUE_ISO_PATH))?;
    let (files, bytes, dropped) = link_tree(&tex, &stage_dir.join("textures").join(&id))?;
    out!(sink, "Staged        {} texture file(s), {} bytes; dropped {} log/backup file(s)", thousands(files), thousands(bytes), dropped);
    let info = serde_json::json!({
        "schema": "nsr.league-game.v1",
        "game_id": id,
        "iso": LEAGUE_ISO_PATH,
        "textures": format!("textures/{id}"),
    });
    let info_path = stage_dir.join(LEAGUE_INFO_FILE);
    std::fs::write(&info_path, serde_json::to_vec_pretty(&info).expect("static json")).map_err(|e| io_err(&info_path, e))?;

    publish_staged(
        ComponentId::LeagueGame, &stage_dir, req.origin, req.version, req.sequence,
        (req.key, req.release_version, req.release_sequence, req.notes), observer, sink,
    )
}

/// Stage `seed/league.s01` (+ `seed/seed.json`), publish `league-content`
/// members only, and re-sign the release.
pub fn publish_league_content(req: &LeagueContentRequest<'_>, observer: &dyn ScanObserver, sink: Sink<'_>) -> Result<Outcome<Push>> {
    let is_s01 = req.seed.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("s01")).unwrap_or(false);
    if !req.seed.is_file() || !is_s01 {
        return Ok(refuse(sink, &[format!("--seed {} is not a .s01 save state. Nothing was published.", req.seed.display())]));
    }
    if !req.apply {
        out!(sink, "Seed          {}", req.seed.display());
        out!(sink, "Nothing was staged or written. Re-run with --apply.");
        return Ok(Outcome::Described);
    }
    let stage_dir = match req.stage {
        Some(s) => s.to_path_buf(),
        None => default_stage(req.origin, &format!("league-content-{}", req.version)),
    };
    if let Err(why) = fresh_stage(&stage_dir, LEAGUE_SEED_PATH) {
        return Ok(refuse(sink, &[why]));
    }
    link_file(req.seed, &stage_dir.join(LEAGUE_SEED_PATH))?;
    if let Some(j) = req.seed_json {
        link_file(j, &stage_dir.join(LEAGUE_SEED_JSON_PATH))?;
    }
    publish_staged(
        ComponentId::LeagueContent, &stage_dir, req.origin, req.version, req.sequence,
        (req.key, req.release_version, req.release_sequence, req.notes), observer, sink,
    )
}
```

(Check the `fresh_stage` marker for league-content: a previous stage carries `seed/league.s01`, and `dir.join("seed/league.s01")` works on Windows too.)

If `CarryForward` field names differ from `origin, id, version, key, release_version, release_sequence, notes, apply` on this branch, use the branch's names. `game_full.rs` shows them as listed.

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core --features publish 2>&1 | tail -8`
Expected: `publish::league` passes 9/9 and the game_full tests still pass.

- [ ] **Step 5: Commit**

```bash
git add -A crates/reborn-core
git commit -m "publish: stage + publish league-game and league-content (members only)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: `rebornctl publish league-game` / `league-content`

**Files:**
- Modify: `crates/reborn-cli/src/publish.rs`

**Interfaces:**
- Consumes: `publish_league_game`, `publish_league_content` (Task 4); the existing `read_notes`, `ScanLog`, `exit_of`.
- Produces: the contracts' commands. `--origin` and `--version` are required, as on every other publish command:
  - `rebornctl publish league-game --iso <path> --textures <dir> --origin <o> --version <v> [--key <k>] [--notes-file <md> | --notes <s>] [--sequence N] [--release-version V] [--release-sequence N] [--stage <dir>] [--apply]`
  - `rebornctl publish league-content --seed <league.s01> [--seed-json <json>] --origin <o> --version <v> [same optional flags] [--apply]`

- [ ] **Step 1: Write the failing test** (in the same `#[cfg(test)]` module as Task 3)

```rust
    #[test]
    fn league_commands_parse_with_the_contract_flags() {
        use clap::Parser;
        #[derive(clap::Parser)]
        struct Cli {
            #[command(subcommand)]
            cmd: super::PublishCommand,
        }
        let g = Cli::try_parse_from([
            "x", "league-game", "--iso", "a.iso", "--textures", "t", "--origin", "o",
            "--version", "2026.10.08", "--notes-file", "n.md", "--apply",
        ]);
        assert!(g.is_ok(), "{:?}", g.err());
        let c = Cli::try_parse_from(["x", "league-content", "--seed", "league.s01", "--origin", "o", "--version", "1.0.0"]);
        assert!(c.is_ok(), "{:?}", c.err());
    }
```

- [ ] **Step 2: Run it and watch it fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-cli league_commands 2>&1 | tail -8`
Expected: FAIL, `unrecognized subcommand 'league-game'`.

- [ ] **Step 3: Implement**

Add these variants to `PublishCommand`, after `GameFull`:

```rust
    /// Publish the league build (members only): the league disc and its
    /// texture folder, staged with league.json, published to the PRIVATE
    /// blob store, then a release carrying every other pin forward. Upload
    /// with `publish upload --private --bucket reborn-league` FIRST, then
    /// `publish upload`.
    LeagueGame {
        #[arg(long)]
        iso: PathBuf,
        /// Named after the league game id, or holding a folder that is.
        #[arg(long)]
        textures: PathBuf,
        #[arg(long)]
        origin: PathBuf,
        #[arg(long)]
        version: String,
        #[arg(long)]
        stage: Option<PathBuf>,
        #[arg(long)]
        sequence: Option<u64>,
        #[arg(long)]
        key: Option<PathBuf>,
        #[arg(long)]
        release_version: Option<String>,
        #[arg(long)]
        release_sequence: Option<u64>,
        #[arg(long, conflicts_with = "notes_file")]
        notes: Option<String>,
        #[arg(long)]
        notes_file: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
    },
    /// Publish the league seed (members only) as seed/league.s01.
    LeagueContent {
        #[arg(long)]
        seed: PathBuf,
        #[arg(long)]
        seed_json: Option<PathBuf>,
        #[arg(long)]
        origin: PathBuf,
        #[arg(long)]
        version: String,
        #[arg(long)]
        stage: Option<PathBuf>,
        #[arg(long)]
        sequence: Option<u64>,
        #[arg(long)]
        key: Option<PathBuf>,
        #[arg(long)]
        release_version: Option<String>,
        #[arg(long)]
        release_sequence: Option<u64>,
        #[arg(long, conflicts_with = "notes_file")]
        notes: Option<String>,
        #[arg(long)]
        notes_file: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
    },
```

In `run`, add the arms:

```rust
        PublishCommand::LeagueGame {
            iso, textures, origin, version, stage, sequence, key,
            release_version, release_sequence, notes, notes_file, apply,
        } => {
            let notes = read_notes(notes, notes_file.as_deref())?;
            let req = reborn_core::publish::league::LeagueGameRequest {
                iso: &iso, textures: &textures, version: &version, stage: stage.as_deref(), origin: &origin,
                sequence, key: key.as_deref(), release_version: release_version.as_deref(), release_sequence,
                notes: notes.as_deref(), apply,
            };
            let log = ScanLog::new();
            Ok(exit_of(&reborn_core::publish::league::publish_league_game(&req, &log as &dyn ScanObserver, &mut sink)?))
        }
        PublishCommand::LeagueContent {
            seed, seed_json, origin, version, stage, sequence, key,
            release_version, release_sequence, notes, notes_file, apply,
        } => {
            let notes = read_notes(notes, notes_file.as_deref())?;
            let req = reborn_core::publish::league::LeagueContentRequest {
                seed: &seed, seed_json: seed_json.as_deref(), version: &version, stage: stage.as_deref(), origin: &origin,
                sequence, key: key.as_deref(), release_version: release_version.as_deref(), release_sequence,
                notes: notes.as_deref(), apply,
            };
            let log = ScanLog::new();
            Ok(exit_of(&reborn_core::publish::league::publish_league_content(&req, &log as &dyn ScanObserver, &mut sink)?))
        }
```

If `PublishCommand` does not derive `clap::Subcommand` in a way the test's wrapper accepts, adapt the wrapper so it goes through the real top-level `Cli` in `main.rs` (`rebornctl publish league-game …`). Keep the assertion.

- [ ] **Step 4: Run the tests and the CLI help**

```bash
CARGO_BUILD_JOBS=1 cargo test -p reborn-cli 2>&1 | tail -5
CARGO_BUILD_JOBS=1 cargo run -q -p reborn-cli -- publish league-game --help | head -20
```
Expected: tests pass, and the help lists `--iso`, `--textures` and `--notes-file`.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-cli/src/publish.rs
git commit -m "rebornctl: publish league-game / league-content

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: The Publisher's "League files" page

**Files:**
- Create: `crates/reborn-publisher/src/league.rs`, `crates/reborn-publisher/ui/league.html`, `crates/reborn-publisher/ui/league.js`
- Modify: `crates/reborn-publisher/src/settings.rs` (`league_bucket_name`), `crates/reborn-publisher/src/main.rs` (module + `invoke_handler`), `crates/reborn-publisher/ui/shell.js` (sidebar entry "League files")

**Interfaces:**
- Consumes: `publish_league_game`, `publish_league_content`, `plan_private`, `plan_origin`, `upload::upload`, `ops::run_operation`, `content.rs`'s `settings_and_platform` / `origin_dir` / `key_path` / `bucket_from` (make them `pub(crate)`).
- Produces: Tauri commands (`rename_all = "snake_case"`, the Publisher law):
  - `league_game_run(iso: String, textures: String, version: String, notes: String, dry_run: bool) -> Result<Value, String>`
  - `league_content_run(seed: String, seed_json: Option<String>, version: String, notes: String, dry_run: bool) -> Result<Value, String>`

- [ ] **Step 1: Write the failing test** (in `settings.rs`'s test module, or create one)

```rust
    #[test]
    fn the_league_bucket_defaults_to_reborn_league_and_can_be_set() {
        let s = Settings::default();
        assert_eq!(s.league_bucket_name.as_deref(), Some("reborn-league"));
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.league_bucket_name.as_deref(), Some("reborn-league"));
        let old: Settings = serde_json::from_str(r#"{"origin":"C:\\Reborn Origin","public_base":"https://files.nflstreetreborn.com/","launcher_manifest":"x"}"#).unwrap();
        assert_eq!(old.league_bucket_name.as_deref(), Some("reborn-league"), "old settings files gain the default");
    }
```

(If `Settings` deserialization from that minimal JSON fails for other missing required fields, add them to the literal. The point is that an old file lacking `league_bucket_name` loads with the default.)

- [ ] **Step 2: Run it and watch it fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-publisher league_bucket 2>&1 | tail -8`
Expected: `no field league_bucket_name`.

- [ ] **Step 3: Implement**

`settings.rs`: add the field

```rust
    /// The PRIVATE bucket members-only (league) blobs go to. Never the
    /// public bucket.
    #[serde(default = "default_league_bucket")]
    pub league_bucket_name: Option<String>,
```

with `fn default_league_bucket() -> Option<String> { Some("reborn-league".to_string()) }`. Also add `league_bucket_name: default_league_bucket(),` in `Default`, and a `"league_bucket_name"` arm in `publisher_set_setting` mirroring `"bucket_name"`.

`league.rs`:

```rust
//! The "League files" page: publish the league build or the league seed,
//! upload their blobs to the PRIVATE bucket first, then the manifest and
//! release to the public bucket. One button per job, like every page.

use reborn_core::inventory::ScanObserver;
use reborn_core::net::UreqHttp;
use reborn_core::publish::league::{publish_league_content, publish_league_game, LeagueContentRequest, LeagueGameRequest};
use reborn_core::publish::roster::Push;
use reborn_core::publish::upload::{self, plan_origin, plan_private, Bucket, UploadRequest, UreqTransport};
use reborn_core::publish::{refuse, Outcome, Sink};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::content::{bucket_from, key_path, origin_dir, settings_and_platform};
use crate::ops;

fn trimmed(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Private blobs first, then the public plan (manifest + release files).
fn upload_both(public: &Bucket, private: &Bucket, origin: &std::path::Path, public_base: &str, sink: Sink<'_>) -> reborn_core::Result<Outcome<Value>> {
    let transport = UreqTransport::new();
    let http = UreqHttp::new(concat!("reborn-publisher/", env!("CARGO_PKG_VERSION")));

    let (already_private, how) = upload::already_on_bucket(&transport, private)?;
    sink(reborn_core::publish::Event::out(how));
    let private_plan = plan_private(origin, &*already_private)?;
    let req = UploadRequest {
        bucket: private, plan: &private_plan,
        multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD, part_size: upload::DEFAULT_PART_SIZE,
        attempts: upload::DEFAULT_ATTEMPTS, public_base: None, apply: true,
    };
    let private_done = match upload::upload(&transport, &http, &req, sink)? {
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Done(u) => u,
        Outcome::Described => Default::default(),
    };

    let (already_public, how) = upload::already_on_bucket(&transport, public)?;
    sink(reborn_core::publish::Event::out(how));
    let public_plan = plan_origin(origin, &*already_public)?;
    let req = UploadRequest {
        bucket: public, plan: &public_plan,
        multipart_threshold: upload::DEFAULT_MULTIPART_THRESHOLD, part_size: upload::DEFAULT_PART_SIZE,
        attempts: upload::DEFAULT_ATTEMPTS, public_base: Some(public_base), apply: true,
    };
    let public_done = match upload::upload(&transport, &http, &req, sink)? {
        Outcome::Refused(r) => return Ok(Outcome::Refused(r)),
        Outcome::Done(u) => u,
        Outcome::Described => Default::default(),
    };
    Ok(Outcome::Done(json!({
        "private_objects": private_done.objects, "private_bytes": private_done.bytes,
        "public_objects": public_done.objects, "public_bytes": public_done.bytes,
        "verified": public_done.verified.first().cloned(),
    })))
}

fn finish(push: Outcome<Push>, what: String, buckets: Option<(Bucket, Bucket)>, origin: PathBuf, public_base: String, sink: Sink<'_>) -> reborn_core::Result<Outcome<Value>> {
    match push {
        Outcome::Refused(r) => Ok(Outcome::Refused(r)),
        Outcome::Described => Ok(Outcome::Done(json!({"summary": format!("Checked {what}. Nothing was written.")}))),
        Outcome::Done(p) => {
            let Some((public, private)) = buckets else {
                return Ok(refuse(sink, &["No bucket was resolved for this real publish. Nothing was uploaded.".to_string()]));
            };
            let sequence = p.release.as_ref().map(|s| s.sequence);
            match upload_both(&public, &private, &origin, &public_base, sink)? {
                Outcome::Done(v) => Ok(Outcome::Done(json!({
                    "summary": format!("Published {what}: release sequence {}. Members can download it now.", sequence.map(|s| s.to_string()).unwrap_or_else(|| "?".into())),
                    "upload": v,
                }))),
                other => Ok(other),
            }
        }
    }
}

fn buckets(dry_run: bool) -> Result<(PathBuf, String, Option<PathBuf>, Option<(Bucket, Bucket)>), String> {
    let (platform, settings) = settings_and_platform()?;
    let origin = origin_dir(&settings);
    let public_base = settings.public_base.clone();
    if dry_run {
        return Ok((origin, public_base, None, None));
    }
    let key = key_path(&settings)?;
    let public = bucket_from(&platform, &settings)?;
    let league = settings.league_bucket_name.clone().filter(|b| !b.trim().is_empty() && b != "reborn-content")
        .ok_or_else(|| "No private league bucket is configured (and reborn-content is never allowed). Set one in Settings.".to_string())?;
    let private = Bucket { bucket: league, ..bucket_from(&platform, &settings)? };
    Ok((origin, public_base, Some(key), Some((public, private))))
}

#[tauri::command(rename_all = "snake_case")]
pub fn league_game_run(iso: String, textures: String, version: String, notes: String, dry_run: bool) -> Result<Value, String> {
    let iso = PathBuf::from(trimmed(&iso).ok_or("Choose the league disc first.")?);
    let textures = PathBuf::from(trimmed(&textures).ok_or("Choose the league texture folder first.")?);
    let version = trimmed(&version).ok_or("Choose a version first.")?;
    let notes = trimmed(&notes);
    let (origin, public_base, key, buckets) = buckets(dry_run)?;
    let inputs = json!({"kind": "league-game-run", "iso": iso.display().to_string(), "textures": textures.display().to_string(), "version": version, "dry_run": dry_run});
    ops::run_operation("league:game", dry_run, inputs, move |sink: Sink<'_>| {
        let req = LeagueGameRequest {
            iso: &iso, textures: &textures, version: &version, stage: None, origin: &origin,
            sequence: None, key: key.as_deref(), release_version: None, release_sequence: None,
            notes: notes.as_deref(), apply: !dry_run,
        };
        let push = publish_league_game(&req, &reborn_core::inventory::RunToCompletion as &dyn ScanObserver, sink)?;
        finish(push, format!("league-game {version}"), buckets.clone(), origin.clone(), public_base.clone(), sink)
    })
    .map(|()| json!({"accepted": true}))
}

#[tauri::command(rename_all = "snake_case")]
pub fn league_content_run(seed: String, seed_json: Option<String>, version: String, notes: String, dry_run: bool) -> Result<Value, String> {
    let seed = PathBuf::from(trimmed(&seed).ok_or("Choose the league seed (.s01) first.")?);
    let seed_json = seed_json.as_deref().and_then(trimmed).map(PathBuf::from);
    let version = trimmed(&version).ok_or("Choose a version first.")?;
    let notes = trimmed(&notes);
    let (origin, public_base, key, buckets) = buckets(dry_run)?;
    let inputs = json!({"kind": "league-content-run", "seed": seed.display().to_string(), "version": version, "dry_run": dry_run});
    ops::run_operation("league:content", dry_run, inputs, move |sink: Sink<'_>| {
        let req = LeagueContentRequest {
            seed: &seed, seed_json: seed_json.as_deref(), version: &version, stage: None, origin: &origin,
            sequence: None, key: key.as_deref(), release_version: None, release_sequence: None,
            notes: notes.as_deref(), apply: !dry_run,
        };
        let push = publish_league_content(&req, &reborn_core::inventory::RunToCompletion as &dyn ScanObserver, sink)?;
        finish(push, format!("league-content {version}"), buckets.clone(), origin.clone(), public_base.clone(), sink)
    })
    .map(|()| json!({"accepted": true}))
}
```

Notes for the implementer:
- `Bucket` has no `Clone`. Add `#[derive(Clone)]` to it in `upload.rs`; its hand-written `Debug` already keeps the secret out of prints.
- If `ops::run_operation`'s closure signature or its `Fn`/`FnOnce` bound differs from the above, follow `content.rs`'s `content_run`, which is the model.
- In `content.rs`, make `settings_and_platform`, `origin_dir`, `key_path` and `bucket_from` `pub(crate)`.
- Register both commands in `main.rs`'s `invoke_handler`.

`ui/league.html` + `ui/league.js`: copy `roster.html` / `roster.js` (the one-button-per-job pattern through `job.js`) and change the fields:
- **Section "League build":** disc picker (`iso`), folder picker (`textures`), `version` text (default today `YYYY.MM.DD`), `notes` textarea, button **"Publish league build {version}"**. The check calls `league_game_run` with `dry_run: true`; "Yes" calls it with `dry_run: false`.
- **Section "League seed":** `.s01` picker (`seed`), optional `seed.json` picker, `version`, `notes`, button **"Publish league seed {version}"**, calling `league_content_run` the same way.
- **One line under each:** "Members only. Files go to the private league bucket; the public CDN never serves them."

`ui/shell.js`: add a sidebar entry "League files" → `league.html` after "Roster".

- [ ] **Step 4: Tests + one real-window call** (Publisher LAW: any new Tauri command boundary gets ONE real-window call before it is done)

```bash
CARGO_BUILD_JOBS=1 cargo test -p reborn-publisher 2>&1 | tail -5
```

Then run the Publisher from this worktree (`cargo run -p reborn-publisher`, or the dev command `crates/reborn-publisher` documents) and on the League files page press the check (dry run) for the league build with a scratch disc. Make it with PowerShell: `[IO.File]::WriteAllBytes("$env:TEMP\zz.iso", [byte[]](90,90,76,71,48,49) + (,0 * 2048))` (header `ZZLG01`), plus a scratch texture folder named `ZZLG01`. Expected card: "Checked league-game …. Nothing was written."
Also run the existing `node Tools/check-publisher-pages.mjs` and add `league` to its page list if it enumerates pages.

- [ ] **Step 5: Commit**

```bash
git add -A crates/reborn-publisher crates/reborn-core/src/publish/upload.rs Tools
git commit -m "publisher: League files page (private bucket first, then public)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Runbook

**Files:**
- Modify: `docs/RUNNING_UPDATES.md` (new section "League files (members only)")

- [ ] **Step 1: Write the section** (append, with this exact text plus the commands)

```markdown
## League files (members only)

`league-game` (league disc + league textures) and `league-content` (league seed) are
MEMBERS ONLY. Their manifests are public and signed like everything else; their blobs
live in the PRIVATE bucket `reborn-league` (no public domain, no r2.dev), and the
Reborn backend hands members short-lived signed URLs (`POST /v1/league/download`).

Local origin: members-only blobs are in `C:\Reborn Origin\private\blobs\by-hash\`,
NEVER `blobs\`. Public `publish upload` and any rclone push of `blobs\` cannot see them.

Publish (Publisher → League files, or the CLI):

    rebornctl publish league-content --seed <league.s01> [--seed-json <seed.json>] --origin "C:\Reborn Origin" --version 1.0.0 --key "<publisher.key>" --notes-file notes.md --apply
    rebornctl publish league-game --iso <league.iso> --textures <folder named after the league id> --origin "C:\Reborn Origin" --version 2026.10.08 --key "<publisher.key>" --notes-file notes.md --apply
    rebornctl publish upload --origin "C:\Reborn Origin" --private --bucket reborn-league --apply   # FIRST
    rebornctl publish upload --origin "C:\Reborn Origin" --apply                                    # THEN

rclone fallback, same order:

    rclone copy "C:\Reborn Origin\private\blobs" r2:reborn-league/blobs
    then the usual public order: blobs -> components -> release.json + .sig

LAWS
- Never rclone `private\` into `r2:reborn-content`. Never give `reborn-league` a public domain.
- Never HTTP-probe a league blob URL (or any unpublished URL): Cloudflare caches the 404.
- Sign only with a rebornctl / Publisher that knows `league-game` and `league-content`.
  An older build refuses to sign a release that pins them (it does not silently drop them).
- The disc's game id (bytes 0..6) must be the league id. Retail ids and `LGXXXX` are refused.
```

- [ ] **Step 2: Commit**

```bash
git add docs/RUNNING_UPDATES.md
git commit -m "docs: league files runbook (private bucket, order, laws)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Full dry run against a scratch origin (no network)

**Files:** none committed. Results go into the report.

- [ ] **Step 1: Build rebornctl**

`CARGO_BUILD_JOBS=1 cargo build --release -p reborn-cli` (binary `target\release\rebornctl.exe`).

- [ ] **Step 2: Scratch publish, end to end, on a throwaway origin** (PowerShell)

```powershell
$s = "$env:TEMP\league-dry"; Remove-Item -Recurse -Force $s -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$s\tex\ZZLG01\sub","$s\keys" | Out-Null
[IO.File]::WriteAllBytes("$s\league.iso", [byte[]](90,90,76,71,48,49) + (,0 * 4096))
Set-Content -NoNewline "$s\tex\ZZLG01\a.dds" 'a'; Set-Content -NoNewline "$s\tex\ZZLG01\sub\b.png" 'b'; Set-Content -NoNewline "$s\tex\ZZLG01\texconv.log" 'log'
[IO.File]::WriteAllBytes("$s\league.s01", [byte[]](1..64))
$ctl = ".\target\release\rebornctl.exe"
& $ctl publish keygen --out "$s\keys"
& $ctl publish league-content --seed "$s\league.s01" --origin "$s\origin" --version 1.0.0 --key "$s\keys\publisher.key" --release-version 3.0.0 --release-sequence 1 --apply
& $ctl publish league-game --iso "$s\league.iso" --textures "$s\tex\ZZLG01" --origin "$s\origin" --version 2026.10.08 --key "$s\keys\publisher.key" --apply
Get-ChildItem -Recurse "$s\origin" | Select-Object FullName,Length
```

Expected:
- Both commands exit 0, and the second one carries `league-content` forward.
- `origin\private\blobs\by-hash\…` holds 5 blobs (iso, a.dds, b.png, league.json, league.s01).
- `origin\blobs` does **not** exist.
- `components\league-game-2026.10.08.json` contains `"members_only": true` and no `texconv.log`.
- `release.json` pins both components.

- [ ] **Step 3: Prove the real origin was never touched**

`& $ctl publish status` (the default local origin is `C:\Reborn Origin`). Expected: the same sequence and pins as before this task, with "signature verified". Paste the line into the report. Nothing is uploaded in this task.

---

### Task 9 (GATED ON ADAM): Bucket, token, first real publish

Do not start until Adam has (a) named `LEAGUE_GAME_ID`, (b) built the league disc carrying that id, (c) provided the league texture folder, and (d) captured `league.s01` on that disc (Hard + GB Off, single core, like `ranked.s01`). Ask before every step that touches Cloudflare or the bucket.

- [ ] **Step 1 (Adam, Cloudflare dashboard):**
  - Create the R2 bucket **`reborn-league`**. **No custom domain, r2.dev access OFF.**
  - Create an R2 API token **"reborn-league-read"**: Object Read only, scoped to `reborn-league`. Hand its access key id and secret to the backend's Railway env (Plan 2), never into this repo or the vault.
  - Make sure the publisher's existing write token can write `reborn-league` (edit its bucket scope). Otherwise the private upload answers 403.
- [ ] **Step 2:** Install the Publisher built from `feat/league-publish` (`build.ps1 -Publisher`), and rebuild `rebornctl` from the same branch. The older ones refuse to sign once league pins exist.
- [ ] **Step 3:** Publisher → Settings: `league_bucket_name = reborn-league`.
- [ ] **Step 4:** Publish the league seed, then the league build, from the League files page. Check each card first (dry run), then "Yes". Expect both cards to finish with "Members can download it now."
- [ ] **Step 5:** `rebornctl publish status` shows a new sequence, "signature verified" and nothing pending. Then list the private bucket (`rclone ls r2:reborn-league/blobs | Measure-Object`) and compare against the manifests' file counts. **Do not** GET any blob URL over HTTP.
- [ ] **Step 6:** Record the versions and sequence in memory (`project_reborn_league_tab.md`) and tell Plan 2's owner the published versions.

---

## For Plan 2 (backend `POST /v1/league/download`)

- **Storage:**
  - Cloudflare R2 bucket **`reborn-league`**, path-style, at the same account endpoint as the publisher's `bucket_endpoint` (`https://<account>.r2.cloudflarestorage.com`), region `"auto"`.
  - Object key for a file with hash `h`: `blobs/by-hash/<h[0..2]>/<h>`.
- **File list:**
  - The public, signed manifest is at `https://files.nflstreetreborn.com/components/<component>-<version>.json`. `<component>` is `league-game` or `league-content`.
  - The latest version is whichever one `https://files.nflstreetreborn.com/release.json` pins under `components["league-game"|"league-content"].url`.
  - Refuse unless the manifest has `"members_only": true` and `component` equals the requested name.
  - Each file is `{path, bytes, sha256}`. Note **`bytes`, not `size`**: map `bytes` to the contract's `size`.
- **Signing:**
  - AWS SigV4 **query-string presign** (`X-Amz-Algorithm=AWS4-HMAC-SHA256`, `X-Amz-Credential`, `X-Amz-Date`, `X-Amz-Expires`, `X-Amz-SignedHeaders=host`, `X-Amz-Signature`) for `GET https://<account>.r2.cloudflarestorage.com/reborn-league/blobs/by-hash/<xx>/<sha>`.
  - Recommended `X-Amz-Expires = 21600` (6 h, enough for an 8 GB download; the maximum is 604800).
  - Range requests work against a presigned GET because `Range` is not a signed header, so the launcher's resume keeps working.
  - Use `@aws-sdk/s3-request-presigner` + `@aws-sdk/client-s3` (`forcePathStyle: true`, `region: "auto"`), or hand-roll it like `upload.rs::sign`.
- **Credentials (Railway env, new):** `LEAGUE_R2_ENDPOINT`, `LEAGUE_R2_BUCKET=reborn-league`, `LEAGUE_R2_ACCESS_KEY_ID`, `LEAGUE_R2_SECRET_ACCESS_KEY`. They come from the **read-only, bucket-scoped** token "reborn-league-read" (Task 9 Step 1), never the publisher's write token.
- **Never** HEAD/GET the blob URLs from the backend to "check" them. The manifest is the source of truth, and the launcher verifies the sha256 after download.
- **Trust chain (matches Plan 2's Tasks 4 and 10):**
  - `release.json` is the ONLY signed object. Its signature is `release.json.sig`: 128 hex characters, Ed25519, made by the publisher key whose public half is in the launcher's `TRUSTED_KEYS`.
  - The release pins each component manifest as `components[name] = {url, sha256}`.
  - The backend fetches the component manifest from that `url`, relative to `https://files.nflstreetreborn.com/`. It must check that the manifest bytes hash to the pinned `sha256` before using it.
  - Verifying `release.json.sig` is optional for the backend: the launcher re-verifies everything.
- **Manifest fields** (`nsr.component.v1`): `component`, `version`, `sequence`, `install_root`, `customizable`, `members_only` (present and `true` only on league components), and `files: [{path, bytes, sha256}]`.
  - ⚠ The size field is **`bytes`**, not `size`. Plan 2 maps it to the contract's `size` in the download response.

## Self-review

- **Spec section 4 coverage:**
  - Components and members-only flag: Tasks 1, 2.
  - Publisher publishing: Tasks 4-6.
  - Signed manifest, signing unchanged: Tasks 1, 2 (the release signer only learns where to look).
  - Gated first publish: Task 9.
  - Download gate inputs for Plan 2: "For Plan 2" section.
- **Contracts:**
  - `members_only` absent = false: Task 1.
  - No public URL: Tasks 2, 3 (private dir + private bucket).
  - Publisher commands match the contract flags, with `--origin` and `--version` added like every other publish command: Task 5.
- **Placeholders:** none. Task 9 is deliberately gated, with concrete steps.
- **Names are consistent across tasks:** `PRIVATE_BLOB_DIR`, `private_blob_path`, `stored_blob`, `plan_private`, `publish_league_game`, `publish_league_content`, `LeagueGameRequest`, `LeagueContentRequest`, `league_bucket_name`, `league_game_run`, `league_content_run`.
- **Review Focus coverage:**
  - Old manifest round-trip: Task 1 tests.
  - Shared blob: Task 2.
  - Public plan excludes private: Task 2.
  - Retail disc / wrong texture id: Task 4.
  - Logs dropped: Task 4.
