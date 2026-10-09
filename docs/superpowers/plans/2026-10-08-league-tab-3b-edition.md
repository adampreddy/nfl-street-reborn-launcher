# League Tab, Plan 3b: Launcher League Edition — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a league member install, update and play the **League edition**, a separate build with its own ISO, texture tree and game ID. It plays offline, in members-only friendlies (league seed or raw game), and in fixture-tagged league best-of-3 games. Mods and controller textures apply to it, and nothing about the retail game changes.

**Architecture:**
- **One new build kind.** The launcher already has an edition abstraction, `runtime::GameBuild` (`Offline`, `Online`), which carries `install_root()`, `is_customizable()` and `game_id()`. This plan adds a third variant, `GameBuild::League`, and puts the per-game-ID behaviour behind it:
  - **Settings:** per-game `GameSettings/<id>.ini`. `Session` already writes it for `req.build.game_id()`.
  - **Textures:** the pristine tree comes from `library/league/textures/<ID>`.
  - **Mods:** composition falls back to a package's `GN7E69` payload. JSGME mods and controller prompt packs are both mod packages, so one shared list covers both.
  - **Seeds:** `library/league-content/seed/league.s01`.
  - **Image:** `library/league/game/league.iso`. It is never modified, because `League` is not customizable.
- **New core module.** `reborn-core::league_edition` holds:
  - the `LEAGUE_GAME_ID` constant and the on-disk layout (fixed by Plan 4);
  - the backend clients for `POST /v1/league/download`, `/v1/league/room` and `/v1/league/friendly`;
  - `LeagueSource`, a `ReleaseSource` that reads the public signed release and manifests from `Origin` but opens blobs only through the backend's signed URLs.
- **Install.** Install and update reuse the existing `content::pipeline::Updater` (check, fetch, promote), so hashing, resume, staging and rollback are the ones every other component already uses.
- **New Tauri commands.** `reborn-ui/src/league_edition.rs` implements the 3b commands in the contracts file. Netplay (`netplay_open`, the relay's `run_step`, `mm_preflight`) learns `edition`, so a league room boots the league image with the right seed.

**Tech Stack:** Rust (reborn-core, reborn-ui / Tauri 2, ureq), vanilla ES modules, PowerShell (installer build script), `node --test` + the `Tools/` checkers.

**Spec:** `docs/superpowers/specs/2026-10-08-league-tab-design.md` (v2) §4 "League edition".

**Binding contracts:** `docs/superpowers/plans/2026-10-08-league-contracts.md`, all sections including both "Rulings" blocks. Plan 4 (`2026-10-08-league-tab-4-publisher.md`, Task 1 and Task 4) fixed the on-disk layout and the `ComponentId` definitions; this plan copies them **identically**.

**Replaces:** the old Plan 3's Task 3 (seed by ruleset, `intent.league` + `intent.fixture_id`, the Online room card's league title) and its `open_room` client and `league_room_open` command now live here, rewritten for `edition`/`ruleset` and the version check.

**Worktree / branch:** `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league`, branch `feat/league-tab` (base 3.0.35 @ d37500a).

**Order:**
- **Before this plan:** 3a Task 1 must be merged. Task 8 here reads its `reborn_core::league_home::{cache_path, load_cache}` membership cache.
- **Independence:** every other task here is independent of 3a.
- **Before the manual run:** 3a Task 6's manual run needs this plan merged and Plan 2 deployed.

## Global Constraints

- Build and test Rust with `CARGO_BUILD_JOBS=1` (parallel builds have run this PC out of memory). The `reborn-core` tests run with `cargo test -p reborn-core`; the `reborn-ui` ones with `cargo test -p reborn-ui`.
- Every new or edited `.rs`/`.js`/`.html`/`.css`/`.ps1` file is **LF**. CRLF breaks the repo's source-scanning tests (memory `project_reborn_launcher_crlf_breaks_tests.md`). Before each commit run `git diff --stat` and `git ls-files --eol <files>`, and expect `w/lf`.
- **Plan 4's layout and names, verbatim:**
  - `ComponentId::LeagueGame` = `"league-game"`, installed at `library/league/`:
    - `league.json` = `{"schema":"nsr.league-game.v1","game_id":"<ID>","iso":"game/league.iso","textures":"textures/<ID>"}`
    - `game/league.iso`
    - `textures/<ID>/...`
  - `ComponentId::LeagueContent` = `"league-content"`, installed at `library/league-content/`, holding `seed/league.s01`.
  - `DataDir::LibraryLeague` = `"library/league"` and `DataDir::LibraryLeagueContent` = `"library/league-content"`.
  - The manifest field is `members_only` (`#[serde(default, skip_serializing_if = "is_false")]`).
  - These definitions must be byte-identical to Plan 4's, so the merge resolves them as equal.
- `LEAGUE_GAME_ID` placeholder is `"LGXXXX"`. While it equals the placeholder, a stable-channel installer build fails (Task 1).
- **The league image is never written to.** Everything that edits a booted image (`gamecube_glyph`, soundtrack, roster apply) is gated on `GameBuild::is_customizable()`, which is `false` for `League`. Two members boot the same bytes in netplay; one rewritten byte is a desync.
- **League components are never in `content::pipeline::UPDATABLE`.** Their blobs are not on the public CDN, so the normal update loop must never try to fetch them. A test enforces this.
- **Retail is unchanged:** the Play and Online tabs boot exactly what they boot today, and every new parameter defaults to retail behaviour.
- **No player ever sees a status code, URL, token or raw error.**
  - Commands reject with `{ kind, message }` (plus `required` for an update), and `message` is one of the fixed sentences in this plan.
  - The one shape Plan 3a matches on is `kind: "league_update_required"`.
- **Membership lost:** when the cached League answer (3a's `league.json`) says not a member, every league launch is refused with `NOT_A_MEMBER`. The files stay on disk.
- **Signed URLs** last 6 h (Plan 4 ruling). A fetch that fails with an expired link (`403`) asks the backend for fresh links once and retries.
- `league_host_friendly` and `league_room_open` do **not** navigate. The League page navigates itself.
- No release, version bump or publish in this plan.

## Review Focus

1. **A league offline boot rewrites the league ISO.** `Session::run` patches the Gamebreaker glyph into any image that is not the online copy. Expected: the league image is byte-identical before and after an offline boot, so the next netplay game against another member does not desync. → Task 3 `the_league_image_is_never_customized` + Task 10 hash check.
2. **The player toggles a retail mod or the PS prompt pack, then plays league.** Expected: the same mod and prompts appear in the league game (`GN7E69` payload composed into `Load/Textures/<LEAGUE_ID>`). A package with its own `<LEAGUE_ID>/` payload uses that instead. → Task 2 `a_retail_mod_is_composed_into_the_league_tree` and `a_league_payload_beats_the_retail_one`.
3. **Two members on different league-game versions.** Expected: the joiner gets `kind: "league_update_required"` with `required`, never a raw `league_version_mismatch` string. This holds whether they join from the League page or by typing the code on the Online page. → Task 6 `a_version_refusal_is_its_own_error` + Task 7 `join_error_shapes`.
4. **The download links expire mid-install** (a 6 GB ISO on a slow line). Expected: one automatic re-request of links and the install finishes; partial `.part` files are resumed, never restarted. → Task 4 `expired_links_are_requested_again_once` + Task 8 install loop.
5. **League files installed but `league.json` names a different game ID than this launcher** (an old launcher after Adam re-IDs the build). Expected: refused with `WRONG_BUILD` before compose or boot. Never composes textures under the wrong folder. → Task 1 `league_info_must_name_this_launchers_game_id`.

---

## File map

| File | Status | Responsibility |
|---|---|---|
| `crates/reborn-core/src/platform/mod.rs` | Modify | `DataDir::LibraryLeague`, `LibraryLeagueContent` |
| `crates/reborn-core/src/inventory.rs` | Modify | `ComponentId::LeagueGame`, `LeagueContent`, `is_members_only`, install roots, scan literal |
| `crates/reborn-core/src/manifest.rs` | Modify | `Component.members_only` |
| `crates/reborn-core/src/content/pipeline.rs` | Modify | `install_dir` arms; `UPDATABLE` unchanged (test) |
| `crates/reborn-core/src/league_edition.rs` | Create | `LEAGUE_GAME_ID`, layout paths, `LeagueInfo`, backend clients, `LeagueSource` |
| `crates/reborn-core/src/lib.rs` | Modify | `pub mod league_edition;` |
| `crates/reborn-core/src/runtime.rs` | Modify | `GameBuild::League` |
| `crates/reborn-core/src/supervisor.rs` | Modify | `RecordedBuild` mapping for `League`; `SessionIntent.league_fixture_id` |
| `crates/reborn-core/src/mods.rs` | Modify | league pristine root + identity; payload fallback to `GN7E69` |
| `crates/reborn-core/src/session.rs` | Modify | glyph gated on `is_customizable()` |
| `crates/reborn-core/src/origin.rs` | Modify | `open_blob_url` shared by `Origin` and `LeagueSource` |
| `crates/reborn-core/src/matchmaking.rs` | Modify | `Room`/`MatchView` `edition`, `ruleset`, `league`, `league_game_version`; `join_room_as`; `MmError::LeagueUpdateRequired` |
| `crates/reborn-core/src/backend.rs` | Modify | `SessionFacts.league_fixture_id`; envelope `intent.fixture_id` |
| `crates/reborn-ui/src/online_mm.rs` | Modify | `seed_for_match`, `edition_of`, `current_match_league`, `with_client_mm`, `mm_room_join` error shape, snapshot fields |
| `crates/reborn-ui/src/dolphin_settings.rs` | Modify | `netplay_boot_state` league/raw; `netplay_open` + `mm_preflight` take `edition` |
| `crates/reborn-ui/src/main.rs` | Modify | `launch` takes `edition`; `sync_active_judged_for`; `mod league_edition;` + registration |
| `crates/reborn-ui/src/league_edition.rs` | Create | the five 3b commands + `ready_league_image` + `installed_league_version` |
| `crates/reborn-ui/ui/online-mm.js` | Modify | room card title for league game / league friendly; no kind switch |
| `crates/reborn-ui/ui/mods.html`, `mods.js` | Modify | "may not match the league build" note |
| `installer/windows/build.ps1` | Modify | stable builds run the `LEAGUE_GAME_ID` gate |

---

### Task 1: League components, the `League` build, and the game-ID gate (core)

**Files:**
- Modify: `crates/reborn-core/src/platform/mod.rs` (`enum DataDir`, `relative`, `ALL`)
- Modify: `crates/reborn-core/src/inventory.rs` (`ComponentId`, `ALL`, `as_str`, `is_members_only`, `install_root`, scan literal, `SerializedComponent::new`)
- Modify: `crates/reborn-core/src/manifest.rs` (`Component`)
- Modify: `crates/reborn-core/src/content/pipeline.rs` (`install_dir`)
- Create: `crates/reborn-core/src/league_edition.rs`
- Modify: `crates/reborn-core/src/lib.rs`
- Modify: `crates/reborn-core/src/runtime.rs` (`GameBuild`)
- Modify: `crates/reborn-core/src/supervisor.rs` (`From<GameBuild> for RecordedBuild`)
- Modify: `installer/windows/build.ps1`

**Interfaces:**
- Produces:
  - `ComponentId::{LeagueGame, LeagueContent}`, `ComponentId::is_members_only`
  - `DataDir::{LibraryLeague, LibraryLeagueContent}`
  - `Component.members_only: bool`
  - `GameBuild::League` (`install_root() == "library/league"`, `is_customizable() == false`, `game_id() == LEAGUE_GAME_ID`)
  - `league_edition::{LEAGUE_GAME_ID, LEAGUE_GAME_ID_PLACEHOLDER, RETAIL_GAME_ID, LEAGUE_INFO_SCHEMA, league_dir, league_image_path, league_textures_root, league_info_path, league_seed_path, LeagueInfo, read_league_info, LeagueInfoError}`

- [ ] **Step 0: Check whether Plan 4's definitions already landed on this line**

Run: `grep -n "LeagueGame\|LibraryLeague\|members_only" crates/reborn-core/src/inventory.rs crates/reborn-core/src/platform/mod.rs crates/reborn-core/src/manifest.rs`

- **If anything is printed:** those parts already exist. Confirm they match the code below exactly, skip re-adding them, and note it in the report.
- **If nothing is printed:** add everything below.

- [ ] **Step 1: Write the failing tests**

Append to `inventory.rs`'s test module:

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
```

Append to `manifest.rs`'s test module (it already has a `component_json(name, customizable)` helper; if it is named differently, use that helper):

```rust
    #[test]
    fn a_manifest_without_members_only_parses_as_public() {
        let c = Component::parse(component_json("offline-content", true).as_bytes()).unwrap();
        assert!(!c.members_only);
    }

    #[test]
    fn a_public_manifest_serializes_without_the_members_only_key() {
        let c = Component::parse(component_json("runtime", false).as_bytes()).unwrap();
        let out = serde_json::to_string_pretty(&c).unwrap();
        assert!(!out.contains("members_only"), "{out}");
    }
```

Append to `content/pipeline.rs`'s test module (create `#[cfg(test)] mod tests { use super::*; }` at the bottom if the file has none):

```rust
    /// League blobs are not on the public CDN: the automatic update loop must never try to fetch them.
    #[test]
    fn league_components_are_never_auto_updated() {
        assert!(!UPDATABLE.contains(&ComponentId::LeagueGame));
        assert!(!UPDATABLE.contains(&ComponentId::LeagueContent));
        assert_eq!(install_dir(ComponentId::LeagueGame), Some(DataDir::LibraryLeague));
        assert_eq!(install_dir(ComponentId::LeagueContent), Some(DataDir::LibraryLeagueContent));
    }
```

Append to `runtime.rs`'s test module:

```rust
    #[test]
    fn the_league_build_is_its_own_non_customizable_game() {
        assert_eq!(GameBuild::League.install_root(), "library/league");
        assert!(!GameBuild::League.is_customizable(), "two members must boot the same bytes");
        assert_eq!(GameBuild::League.game_id(), crate::league_edition::LEAGUE_GAME_ID);
        assert_ne!(GameBuild::League.game_id(), GameBuild::Offline.game_id());
        assert_eq!(crate::supervisor::RecordedBuild::from(GameBuild::League), crate::supervisor::RecordedBuild::Offline);
    }
```

Create `crates/reborn-core/src/league_edition.rs` with only its test module (so it fails to compile):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn write_info(root: &std::path::Path, json: &str) {
        let p = league_info_path(root);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, json).unwrap();
    }

    #[test]
    fn the_layout_is_plan_4s() {
        let root = std::path::Path::new("/d");
        assert_eq!(league_dir(root), root.join("library/league"));
        assert_eq!(league_image_path(root), root.join("library/league").join("game").join("league.iso"));
        assert_eq!(league_textures_root(root), root.join("library/league").join("textures").join(LEAGUE_GAME_ID));
        assert_eq!(league_seed_path(root), root.join("library/league-content").join("seed").join("league.s01"));
    }

    #[test]
    fn league_info_must_name_this_launchers_game_id() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Missing));
        write_info(dir.path(), "not json");
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Unreadable));
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v1","game_id":"ZZZZ99","iso":"game/league.iso","textures":"textures/ZZZZ99"}}"#));
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::WrongGameId("ZZZZ99".into())));
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v2","game_id":"{LEAGUE_GAME_ID}","iso":"game/league.iso","textures":"textures/{LEAGUE_GAME_ID}"}}"#));
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Unreadable));
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v1","game_id":"{LEAGUE_GAME_ID}","iso":"elsewhere.iso","textures":"textures/{LEAGUE_GAME_ID}"}}"#));
        assert_eq!(read_league_info(dir.path()), Err(LeagueInfoError::Unreadable), "the layout is fixed, never taken from the file");
        write_info(dir.path(), &format!(
            r#"{{"schema":"nsr.league-game.v1","game_id":"{LEAGUE_GAME_ID}","iso":"game/league.iso","textures":"textures/{LEAGUE_GAME_ID}"}}"#));
        assert_eq!(read_league_info(dir.path()).unwrap().game_id, LEAGUE_GAME_ID);
    }

    /// Release gate. `installer/windows/build.ps1 -Channel stable` runs this with `--ignored`; it fails until Adam
    /// names the league game ID (contracts file). A beta or a dev build is not held by it.
    #[test]
    #[ignore = "release gate: run by build.ps1 for the stable channel"]
    fn league_game_id_is_named_before_a_stable_release() {
        assert_ne!(LEAGUE_GAME_ID, LEAGUE_GAME_ID_PLACEHOLDER, "set LEAGUE_GAME_ID in league_edition.rs before a stable release");
        assert!(crate::mods::GameId::parse(LEAGUE_GAME_ID).is_ok());
    }

    #[test]
    fn the_placeholder_is_still_a_valid_game_id() {
        assert!(crate::mods::GameId::parse(LEAGUE_GAME_ID).is_ok(), "the tree is filed under this name");
        assert_eq!(LEAGUE_GAME_ID.len(), 6);
    }
}
```

Add `pub mod league_edition;` to `lib.rs` next to the other `pub mod` lines (alphabetical where the file is alphabetical).

- [ ] **Step 2: Run the tests and watch them fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core league 2>&1 | tail -20`
Expected: compile errors (`no variant named LeagueGame`, `no field members_only`, `cannot find function league_dir`, `no variant League`).

- [ ] **Step 3: Implement**

**`platform/mod.rs`:** add to `enum DataDir`, directly after `LibraryGameFull,`:

```rust
    /// The league build (`league-game`): `game/league.iso`, the league
    /// texture tree `textures/<league game id>/` and `league.json`.
    /// Members only: its blobs come from the backend, never the public CDN.
    LibraryLeague,
    /// The league seed (`league-content`): `seed/league.s01`.
    LibraryLeagueContent,
```

In `relative()`, add `DataDir::LibraryLeague => "library/league",` and `DataDir::LibraryLeagueContent => "library/league-content",`. In `ALL`, add both directly after `DataDir::LibraryGameFull,`. Neither is user data and neither is derived, so `is_user_data`/`is_derived` stay as they are.

**`manifest.rs`:** in `struct Component`, directly after `pub customizable: bool,`:

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

**`inventory.rs`:**
- In `enum ComponentId`, after `GameFull,` (with its doc comment):

```rust
    /// The league build: league ISO + league texture tree. Members only.
    LeagueGame,
    /// The league seed save state. Members only.
    LeagueContent,
```
- In `ALL`, add `ComponentId::LeagueGame, ComponentId::LeagueContent,` at the end.
- In `as_str`, add `ComponentId::LeagueGame => "league-game", ComponentId::LeagueContent => "league-content",`.
- Directly after `is_customizable`, add:

```rust
    /// Whether the blobs are withheld from the public CDN and handed out by
    /// the backend to league members only. A property of the identity, like
    /// [`Self::is_customizable`], so a generator cannot get it wrong.
    pub fn is_members_only(self) -> bool {
        matches!(self, ComponentId::LeagueGame | ComponentId::LeagueContent)
    }
```
- In `install_root`, add `ComponentId::LeagueGame => Some(DataDir::LibraryLeague.relative()), ComponentId::LeagueContent => Some(DataDir::LibraryLeagueContent.relative()),`.
- In the scan's `Component { … }` literal (the one containing `customizable: spec.id.is_customizable(),`), add `members_only: spec.id.is_members_only(),`.
- In `SerializedComponent::new`, directly after the existing check that compares `parsed.customizable != component.customizable`, add:

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

**`content/pipeline.rs` `install_dir`:** add, beside the `GameFull` arm:

```rust
        // Members only, and NOT in `UPDATABLE`: their blobs come from the backend's signed links, so only the League
        // tab's install (reborn-ui `league_edition`) fetches them, through `league_edition::LeagueSource`.
        ComponentId::LeagueGame => Some(DataDir::LibraryLeague),
        ComponentId::LeagueContent => Some(DataDir::LibraryLeagueContent),
```

Build: `CARGO_BUILD_JOBS=1 cargo build -p reborn-core 2>&1 | grep -E "^error" -A5`.
- **Non-exhaustive `match` errors:** each exhaustive `match` on `ComponentId` or `DataDir` that no longer compiles gets an arm that treats the two league ids exactly like `GameFull` / `LibraryGameFull`.
- **`Component { … }` literals:** any other literal (tests, fixtures) gets `members_only: false`.
- **Report:** list every site you touched.

**`league_edition.rs`:** put this above the test module:

```rust
//! The League edition: the league's own build of the game (its own ISO, texture tree and game ID), installed from the
//! League tab by members only (spec 2026-10-08 v2 §4).
//!
//! The on-disk layout is Plan 4's (the publisher writes it, this reads it) and is fixed here, never taken from
//! `league.json`: the file only confirms which build is installed. A manifest that put the disc anywhere else would be
//! a different component, and composing textures under a game ID this launcher does not boot would leave the league
//! game with none.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::platform::DataDir;

/// The disc ID the league build carries (its first six bytes). Dolphin files the league's textures and per-game
/// settings under it. Adam names it; until then this is the placeholder and a stable installer build refuses
/// (`league_game_id_is_named_before_a_stable_release`).
pub const LEAGUE_GAME_ID: &str = "LGXXXX";
pub const LEAGUE_GAME_ID_PLACEHOLDER: &str = "LGXXXX";
/// The retail Reborn disc, whose mod payloads the league build borrows (one shared mod list, spec v2).
pub const RETAIL_GAME_ID: &str = "GN7E69";
pub const LEAGUE_INFO_SCHEMA: &str = "nsr.league-game.v1";

const INFO_FILE: &str = "league.json";
const ISO_PATH: &str = "game/league.iso";
const SEED_PATH: &str = "seed/league.s01";

pub fn league_dir(data_root: &Path) -> PathBuf {
    data_root.join(DataDir::LibraryLeague.relative())
}

pub fn league_image_path(data_root: &Path) -> PathBuf {
    let mut p = league_dir(data_root);
    for seg in ISO_PATH.split('/') {
        p = p.join(seg);
    }
    p
}

/// The pristine league texture tree the composed `Load/Textures/<LEAGUE_GAME_ID>` is built from.
pub fn league_textures_root(data_root: &Path) -> PathBuf {
    league_dir(data_root).join("textures").join(LEAGUE_GAME_ID)
}

pub fn league_info_path(data_root: &Path) -> PathBuf {
    league_dir(data_root).join(INFO_FILE)
}

pub fn league_seed_path(data_root: &Path) -> PathBuf {
    let mut p = data_root.join(DataDir::LibraryLeagueContent.relative());
    for seg in SEED_PATH.split('/') {
        p = p.join(seg);
    }
    p
}

/// `league.json`, as Plan 4's publisher stages it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LeagueInfo {
    pub schema: String,
    pub game_id: String,
    pub iso: String,
    pub textures: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeagueInfoError {
    /// No league build installed.
    Missing,
    /// There, but not a `nsr.league-game.v1` file in Plan 4's layout.
    Unreadable,
    /// A league build for another game ID: this launcher would compose and boot the wrong thing.
    WrongGameId(String),
}

/// The installed build's `league.json`, accepted only when it is exactly the layout this launcher boots.
pub fn read_league_info(data_root: &Path) -> Result<LeagueInfo, LeagueInfoError> {
    let bytes = match std::fs::read(league_info_path(data_root)) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(LeagueInfoError::Missing),
        Err(_) => return Err(LeagueInfoError::Unreadable),
    };
    let info: LeagueInfo = serde_json::from_slice(&bytes).map_err(|_| LeagueInfoError::Unreadable)?;
    if info.schema != LEAGUE_INFO_SCHEMA
        || info.iso != ISO_PATH
        || info.textures != format!("textures/{}", info.game_id)
    {
        return Err(LeagueInfoError::Unreadable);
    }
    if info.game_id != LEAGUE_GAME_ID {
        return Err(LeagueInfoError::WrongGameId(info.game_id));
    }
    Ok(info)
}
```

**`runtime.rs` `GameBuild`:** add the variant, then an arm in each of the three methods:

```rust
    /// The League edition (spec 2026-10-08 v2 §4): its own disc and game ID, members only. Not customizable, for the
    /// same reason as `Online`: two members play it in lockstep, so nothing may rewrite it.
    League,
```

- `install_root`: `GameBuild::League => "library/league",`
- `game_id`: `GameBuild::League => crate::league_edition::LEAGUE_GAME_ID,`
- `is_customizable`: it is `matches!(self, GameBuild::Offline)`, so it already answers `false`. Leave it.

**`supervisor.rs` `From<GameBuild> for RecordedBuild`:** add an arm.

```rust
            // The record keeps its two-value wire format (a backend reads it). A retail netplay session is already
            // recorded `Offline` (netplay_open boots with build Offline), and a league one is the same kind of session.
            GameBuild::League => RecordedBuild::Offline,
```

Build again and fix every exhaustive `match` on `GameBuild` the compiler names. In `reborn-ui`, `cargo build -p reborn-ui` must also be clean; no `reborn-ui` match on `GameBuild` is expected, but fix any the compiler reports.

**`installer/windows/build.ps1`:** directly before the line `cargo build --release -p reborn-ui -p reborn-cli` (inside the same branch), add:

```powershell
        if ($Channel -eq "stable") {
            # Plan 3b release gate: a stable launcher must boot the real league disc ID, never the placeholder.
            cargo test -p reborn-core --lib league_game_id_is_named_before_a_stable_release -- --ignored
            if ($LASTEXITCODE -ne 0) { throw "LEAGUE_GAME_ID is still the placeholder (crates/reborn-core/src/league_edition.rs). Name the league game ID before a stable build." }
        }
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core 2>&1 | tail -8`
Expected: the new tests pass (the gate test is reported as ignored) and there are no new failures against the pre-task baseline. Record the baseline counts before Step 1 with the same command.

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core --lib league_game_id_is_named_before_a_stable_release -- --ignored 2>&1 | tail -5`
Expected: **FAIL** with "set LEAGUE_GAME_ID …". The gate works.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core installer/windows/build.ps1
git commit -m "core: league-game/league-content components, GameBuild::League, LEAGUE_GAME_ID release gate

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: League textures and the shared mod list (core `mods.rs`)

**Files:**
- Modify: `crates/reborn-core/src/mods.rs` (`pristine_root`, `pristine_identity`, `ModPackage`, `ComposeRequest`/`SyncRequest` and their `in_data_root` constructors, `detect_conflicts`, `Composition::plan`)

**Interfaces:**
- Consumes: `league_edition::{LEAGUE_GAME_ID, RETAIL_GAME_ID, league_textures_root}` (Task 1).
- Produces:
  - `mods::payload_fallback_for(game: &GameId) -> Option<GameId>`: `Some(GN7E69)` for the league ID, else `None`.
  - `ModPackage::payload_game(&self, game: &GameId, fallback: Option<&GameId>) -> Option<GameId>`
  - `ComposeRequest.payload_fallback` and `SyncRequest.payload_fallback`: `Option<GameId>`

Before Step 1, read the code around `fn plan(platform: &Platform, req: &ComposeRequest)`, `detect_conflicts`, and the place where `sync_active` builds a `ComposeRequest` from a `SyncRequest`. Search with `grep -n "ComposeRequest {" crates/reborn-core/src/mods.rs`. Every struct literal of either request type gets the new field.

- [ ] **Step 1: Write the failing tests** (in `mods.rs`'s test module, which already has `fixture()`, `lay_package(...)`, `fx.request(...)` and `planned(...)` helpers; see `a_package_for_another_build_is_skipped_and_reported`)

```rust
    fn league() -> GameId {
        GameId::parse(crate::league_edition::LEAGUE_GAME_ID).unwrap()
    }

    #[test]
    fn only_the_league_build_borrows_retail_payloads() {
        assert_eq!(payload_fallback_for(&league()), Some(GameId::parse("GN7E69").unwrap()));
        assert_eq!(payload_fallback_for(&GameId::parse("GN7E69").unwrap()), None);
        assert_eq!(payload_fallback_for(&GameId::parse("FR4N31").unwrap()), None);
    }

    /// One shared mod list (spec v2): a mod switched on for Reborn is in the league game too.
    #[test]
    fn a_retail_mod_is_composed_into_the_league_tree() {
        let p = Platform::detect();
        let fx = fixture();
        let pkg = lay_package(&fx.packages, "reborn.ball.purple", None, &[("GN7E69/ball.dds", b"purple")]);
        let mut req = fx.request(vec![pkg]);
        req.game = league();
        req.payload_fallback = payload_fallback_for(&req.game);
        let plan = planned(&p, &req);
        assert_eq!(plan.overlay_count(), 1);
        assert!(plan.not_targeted.is_empty());
    }

    #[test]
    fn a_league_payload_beats_the_retail_one() {
        let both = lay_package(&fixture().packages, "reborn.ball.both", None, &[
            ("GN7E69/ball.dds", b"retail"),
            (&format!("{}/ball.dds", crate::league_edition::LEAGUE_GAME_ID), b"league"),
        ]);
        let gn7 = GameId::parse("GN7E69").unwrap();
        assert_eq!(both.payload_game(&league(), Some(&gn7)), Some(league()));
        let retail_only = lay_package(&fixture().packages, "reborn.ball.retail", None, &[("GN7E69/ball.dds", b"retail")]);
        assert_eq!(retail_only.payload_game(&league(), Some(&gn7)), Some(gn7.clone()));
        assert_eq!(retail_only.payload_game(&league(), None), None, "no fallback, no payload");
        let online_only = lay_package(&fixture().packages, "reborn.ball.online", None, &[("FR4N31/ball.dds", b"online")]);
        assert_eq!(online_only.payload_game(&league(), Some(&gn7)), None);
    }

    #[test]
    fn retail_composition_ignores_the_fallback() {
        let p = Platform::detect();
        let fx = fixture();
        let other = lay_package(&fx.packages, "reborn.ball.other", None, &[("FR4N31/ball.dds", b"online")]);
        let req = fx.request(vec![other]);
        assert_eq!(req.payload_fallback, None);
        assert_eq!(planned(&p, &req).not_targeted.len(), 1, "a retail tree never borrows");
    }
```

If `lay_package` has a different signature or return type, adapt the calls to the helper as it exists (it returns a `ModPackage` in the existing tests) and say so in the report.

```rust
    /// The league tree is composed from the league build's own pristine textures, never the retail pack.
    #[test]
    fn the_league_pristine_tree_is_the_league_components() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let p = crate::platform::Platform::for_test_data_root(root);
        assert_eq!(pristine_root(&p, &league()).unwrap(), crate::league_edition::league_textures_root(root));
    }
```

If the platform has no `for_test_data_root` constructor, find how the existing `pristine_root` tests build a `Platform` over a temp data root (`grep -n "fn .*Platform" crates/reborn-core/src/platform/*.rs`) and use that; note it in the report.

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core mods::tests::a_retail_mod mods::tests::a_league_payload mods::tests::only_the_league mods::tests::retail_composition mods::tests::the_league_pristine 2>&1 | tail -15`
Expected: compile errors (`payload_fallback_for`, `payload_fallback`, `payload_game` not found).

- [ ] **Step 3: Implement**

Add near `pristine_root`:

```rust
/// The build whose mod payloads a tree borrows when a package has none of its own: the league build borrows retail
/// (one shared mod list, spec 2026-10-08 v2). Every other tree composes only what was made for it.
pub fn payload_fallback_for(game: &GameId) -> Option<GameId> {
    (game.as_str() == crate::league_edition::LEAGUE_GAME_ID)
        .then(|| GameId::parse(crate::league_edition::RETAIL_GAME_ID).ok())
        .flatten()
}
```

In `impl ModPackage`, after `files_for`:

```rust
    /// Which of this package's payloads a tree for `game` composes: its own when it has one, else `fallback`'s,
    /// else none.
    pub fn payload_game(&self, game: &GameId, fallback: Option<&GameId>) -> Option<GameId> {
        if self.targets_game(game) {
            return Some(game.clone());
        }
        fallback.filter(|f| self.targets_game(f)).cloned()
    }
```

Add `pub payload_fallback: Option<GameId>,` to both `ComposeRequest` and `SyncRequest` (documented "see [`payload_fallback_for`]"). In both `in_data_root` constructors, set `payload_fallback: payload_fallback_for(&game),` (before `game` is moved). Wherever `sync_active` builds a `ComposeRequest` from a `SyncRequest`, copy the field across.

In `Composition::plan`, replace:

```rust
            if !package.targets_game(&req.game) {
                not_targeted.push(package.id.clone());
                continue;
            }
            for file in package.files_for(&req.game) {
```

with:

```rust
            let Some(payload) = package.payload_game(&req.game, req.payload_fallback.as_ref()) else {
                not_targeted.push(package.id.clone());
                continue;
            };
            for file in package.files_for(&payload) {
```

In `detect_conflicts(packages, game)`, give it a `fallback: Option<&GameId>` parameter and call `payload_game` the same way. Package `p` contributes `composed_paths(&payload)` for its resolved payload game, and only packages with `Some(payload)` take part. Update its caller in `plan` to pass `req.payload_fallback.as_ref()`. `composed_paths` returns tails (`path.tail()`), so a league tree and a retail tree compare the same relative paths.

`pristine_root`: at the very top of the function body:

```rust
    if game.as_str() == crate::league_edition::LEAGUE_GAME_ID {
        // The league build ships its own tree inside its component (Plan 4 layout), never the retail pack.
        return Ok(crate::league_edition::league_textures_root(&platform.paths.data_root()?));
    }
```

`pristine_identity`: replace the leading `if game.as_str() != "GN7E69" { return None; }` with:

```rust
    let component = match game.as_str() {
        "GN7E69" => "textures",
        // A league update swaps the whole league folder, so the composed hardlinks must be rebuilt then too.
        id if id == crate::league_edition::LEAGUE_GAME_ID => "league-game",
        _ => return None,
    };
```

and use `component` in place of the literal `"textures"` in the `pointer_path(...)` call below it.

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core mods 2>&1 | tail -8`
Expected: all `mods` tests pass, old and new.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/mods.rs
git commit -m "mods: league tree composes from the league build and borrows retail mod payloads

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: A league launch never writes into the league image (core `session.rs`)

**Files:**
- Modify: `crates/reborn-core/src/session.rs` (the `gamecube_glyph` block in `Session::run`, around the `if on_booted_offline {` that calls `crate::gamecube_glyph::apply`)

**Interfaces:**
- Consumes: `GameBuild::League.is_customizable() == false` (Task 1).

`Session::run` decides "offline" by the image path (`is_netplay_image`), and only the retail online copy is excluded. A league offline boot would therefore rewrite `gamecube_glyph::FILE_NAME` inside `league.iso`, and the next netplay game against another member would desync. The `[OnFrame]` GameCube patch is already safe (`gamecube_controls::apply` carries it only for `GN7E69`), so only the glyph needs a gate.

- [ ] **Step 1: Write the failing test** (in `session.rs`'s test module; source scans are this file's pattern for launch-path rules, e.g. `the_image_netplay_boots_is_recognised_and_excluded`'s neighbours)

```rust
    /// Review Focus 1. Two members boot the same league bytes in lockstep: an offline league boot must never write a
    /// customization into the league image.
    #[test]
    fn the_league_image_is_never_customized() {
        let src = include_str!("session.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        assert!(
            production.contains("if on_booted_offline && req.build.is_customizable() {"),
            "the Gamebreaker glyph is written into the booted image; it must be gated on a customizable build"
        );
        assert!(!crate::runtime::GameBuild::League.is_customizable());
    }
```

- [ ] **Step 2: Run it and watch it fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core the_league_image_is_never_customized`
Expected: FAIL (the gate text is not there yet).

- [ ] **Step 3: Implement**

In `Session::run`, the block that starts with the comment "The same profile, the same rule, one layer further in: the disc's in-match Gamebreaker prompt…" ends in `if on_booted_offline {`. Change that line to:

```rust
        // ⛔ And only for a build the player may customize: the league image is shared byte for byte between members
        // in netplay (GameBuild::League is not customizable), so it is never rewritten, offline either.
        if on_booted_offline && req.build.is_customizable() {
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core session 2>&1 | tail -6`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/session.rs
git commit -m "session: never write the Gamebreaker glyph into a non-customizable (league) image

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Members-only downloads: `request_download` and `LeagueSource` (core)

**Files:**
- Modify: `crates/reborn-core/src/origin.rs` (extract `open_blob_url`; add `Origin::agent`)
- Modify: `crates/reborn-core/src/league_edition.rs`

**Interfaces:**
- Consumes: `Http::post_json(url, body, bearer) -> Result<HttpResponse>` (`HttpResponse { status, body }`), `crate::net::fake::FakeHttp` (`.reply(url, status, body)`, `.posts`), `content::{BlobSource, BlobRequest}`, `content::pipeline::ReleaseSource`, `Origin`.
- Produces:
  - `origin::open_blob_url(agent: &ureq::Agent, url: &str, req: &BlobRequest<'_>) -> Result<Box<dyn Read>>`
  - `Origin::agent(&self) -> &ureq::Agent`
  - `league_edition::LeagueError { SignedOut, NotMember, UnknownVersion, Unavailable, RateLimited, Unreachable, BadReply }`
  - `league_edition::GatedFile { path, url, sha256, size }`
  - `league_edition::request_download(http, origin, token, component, version) -> Result<Vec<GatedFile>, LeagueError>`
  - `league_edition::LeagueSource::new(origin: Origin, files: &[GatedFile]) -> LeagueSource` (implements `ReleaseSource`)
  - `LeagueSource::with_progress(self, on_bytes: Arc<dyn Fn(u64) + Send + Sync>) -> Self`
  - `LeagueSource::refresh(&mut self, files: &[GatedFile])` (replaces the link table, for expired links)
  - `league_edition::link_expired(err: &crate::Error) -> bool`

- [ ] **Step 1: Write the failing tests** (append inside `league_edition.rs`'s test module)

```rust
    use crate::net::fake::FakeHttp;

    const ORIGIN: &str = "https://api.test";
    const DL: &str = "https://api.test/v1/league/download";
    const SHA: &str = "aa00000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn a_download_request_names_the_component_and_version_with_the_bearer() {
        let body = format!(r#"{{"files":[{{"path":"game/league.iso","url":"https://r2.test/x?sig=1","sha256":"{SHA}","size":6}}],
            "expires_at":"2026-10-08T20:00:00Z","component":"league-game","version":"2026.10.08"}}"#);
        let http = FakeHttp::default().reply(DL, 200, &body);
        let files = request_download(&http, ORIGIN, "tok", "league-game", "2026.10.08").unwrap();
        assert_eq!(files, vec![GatedFile { path: "game/league.iso".into(), url: "https://r2.test/x?sig=1".into(), sha256: SHA.into(), size: 6 }]);
        let posts = http.posts.lock().unwrap();
        assert_eq!((posts[0].0.as_str(), posts[0].2.as_deref()), (DL, Some("tok")));
        let sent: serde_json::Value = serde_json::from_slice(&posts[0].1).unwrap();
        assert_eq!(sent, serde_json::json!({"component":"league-game","version":"2026.10.08"}));
    }

    #[test]
    fn download_refusals_are_named() {
        for (status, body, want) in [
            (401u16, "{}", LeagueError::SignedOut),
            (403, r#"{"ok":false,"error":"not_member"}"#, LeagueError::NotMember),
            (404, r#"{"ok":false,"error":"unknown_version"}"#, LeagueError::UnknownVersion),
            (429, "{}", LeagueError::RateLimited),
            (503, r#"{"ok":false,"error":"league_unavailable"}"#, LeagueError::Unavailable),
            (200, r#"{"files":"nope"}"#, LeagueError::BadReply),
            (200, "<b>Warning</b>", LeagueError::BadReply),
        ] {
            let http = FakeHttp::default().reply(DL, status, body);
            assert_eq!(request_download(&http, ORIGIN, "tok", "league-game", "1").unwrap_err(), want, "{status} {body}");
        }
        assert_eq!(request_download(&FakeHttp::default(), ORIGIN, "tok", "league-game", "1").unwrap_err(), LeagueError::Unreachable);
    }

    #[test]
    fn a_link_never_comes_from_anywhere_but_the_backend() {
        let src = LeagueSource::new(crate::origin::Origin::new("https://cdn.test/"), &[]);
        let file = crate::manifest::ManifestFile {
            path: "game/league.iso".into(),
            bytes: 6,
            sha256: crate::manifest::Sha256Hex::parse(SHA).unwrap(),
        };
        let req = crate::content::BlobRequest { component: "league-game", version: "1", file: &file, offset: 0 };
        let err = crate::content::BlobSource::open(&src, &req).err().expect("no link, no fetch");
        assert!(err.to_string().contains("no download link"), "{err}");
    }

    #[test]
    fn expired_links_are_requested_again_once() {
        // The signed links last 6 h; the blob store answers 403 for an expired one. Only that is "expired".
        assert!(link_expired(&crate::origin::test_not_found("https://r2.test/x answered 403 for game/league.iso")));
        assert!(!link_expired(&crate::origin::test_not_found("https://r2.test/x answered 404 for game/league.iso")));
    }
```

`crate::origin::test_not_found` does not exist yet. Step 3 adds it as a `#[cfg(test)] pub(crate)` wrapper over the private `not_found` constructor, so the test builds the exact error `open_blob_url` produces.

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core league_edition 2>&1 | tail -15`
Expected: compile errors (`request_download`, `GatedFile`, `LeagueSource`, `link_expired`, `test_not_found` not found).

- [ ] **Step 3: Implement**

**`origin.rs`:** move the body of `impl BlobSource for Origin { fn open }` into a free function, and keep the `Origin` method as a thin call:

```rust
/// Open one blob URL from `req.offset` (a `Range` request when resuming). Shared by the public CDN ([`Origin`]) and
/// the league's signed links (`league_edition::LeagueSource`), so resume and error handling are one implementation.
pub(crate) fn open_blob_url(agent: &ureq::Agent, url: &str, req: &BlobRequest<'_>) -> Result<Box<dyn Read>> {
    let mut request = agent.get(url);
    if req.offset > 0 {
        request = request.set("Range", &format!("bytes={}-", req.offset));
    }
    match request.call() {
        // … the existing match arms, unchanged, with `url` in place of `&url` where needed …
    }
}

impl BlobSource for Origin {
    fn open(&self, req: &BlobRequest<'_>) -> Result<Box<dyn Read>> {
        // (keep the existing ?v= comment here)
        let url = format!("{}?v={}", self.url(&blob_path(&req.file.sha256)), &req.file.sha256.as_str()[..8]);
        open_blob_url(&self.agent, &url, req)
    }
}
```

Copy the four existing match arms (`Ok(resp)` with its 206 check, `Status(416)`, `Status(status)`, `Transport`) into `open_blob_url` verbatim. Then add:

```rust
impl Origin {
    pub(crate) fn agent(&self) -> &ureq::Agent {
        &self.agent
    }
}

#[cfg(test)]
pub(crate) fn test_not_found(msg: &str) -> Error {
    not_found(msg.to_string())
}
```

(Put `agent` in the existing `impl Origin` block rather than adding a second one, if you prefer.)

**`league_edition.rs`:** add below `read_league_info`:

```rust
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::Arc;

use crate::content::{BlobRequest, BlobSource, SignatureVerifier};
use crate::content::pipeline::ReleaseSource;
use crate::net::Http;
use crate::origin::{FetchedRelease, Origin};

/// Why a league backend call did not give what was asked. Never shown as is: reborn-ui turns each into a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeagueError {
    SignedOut,
    NotMember,
    UnknownVersion,
    Unavailable,
    RateLimited,
    Unreachable,
    BadReply,
}

fn league_url(origin: &str, path: &str) -> String {
    format!("{}{}", origin.trim_end_matches('/'), path)
}

fn status_error(status: u16, body: &str) -> LeagueError {
    let code = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string));
    match (status, code.as_deref()) {
        (401, _) => LeagueError::SignedOut,
        (403, _) => LeagueError::NotMember,
        (404, _) => LeagueError::UnknownVersion,
        (429, _) => LeagueError::RateLimited,
        _ => LeagueError::Unavailable,
    }
}

/// One file of a members-only component, with the signed link the backend issued for it (6 h).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GatedFile {
    pub path: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct DownloadReply {
    files: Vec<GatedFile>,
}

/// `POST {origin}/v1/league/download {component, version}` (Plan 2). Members only: the backend asks the league site
/// first, then signs one short-lived link per blob.
pub fn request_download(
    http: &dyn Http,
    origin: &str,
    token: &str,
    component: &str,
    version: &str,
) -> std::result::Result<Vec<GatedFile>, LeagueError> {
    let body = serde_json::to_vec(&serde_json::json!({ "component": component, "version": version }))
        .map_err(|_| LeagueError::BadReply)?;
    let reply = http
        .post_json(&league_url(origin, "/v1/league/download"), &body, Some(token))
        .map_err(|_| LeagueError::Unreachable)?;
    if !(200..300).contains(&reply.status) {
        return Err(status_error(reply.status, &reply.body));
    }
    let parsed: DownloadReply = serde_json::from_str(&reply.body).map_err(|_| LeagueError::BadReply)?;
    Ok(parsed.files)
}

/// The content pipeline's source for a members-only component: the release and the component manifests come from the
/// public, signed origin exactly as for every other component; the blobs come only from the backend's signed links.
/// A blob with no link is refused, never fetched from the public CDN (where it does not exist).
#[derive(Clone)]
pub struct LeagueSource {
    origin: Origin,
    links: BTreeMap<String, String>,
    on_bytes: Option<Arc<dyn Fn(u64) + Send + Sync>>,
}

impl std::fmt::Debug for LeagueSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the links themselves: they are credentials for 6 h.
        f.debug_struct("LeagueSource").field("origin", &self.origin.base()).field("links", &self.links.len()).finish()
    }
}

impl LeagueSource {
    pub fn new(origin: Origin, files: &[GatedFile]) -> Self {
        let mut s = Self { origin, links: BTreeMap::new(), on_bytes: None };
        s.refresh(files);
        s
    }

    /// Replace the link table (the links expired, or another component's were added).
    pub fn refresh(&mut self, files: &[GatedFile]) {
        for f in files {
            self.links.insert(f.sha256.to_ascii_lowercase(), f.url.clone());
        }
    }

    /// Called with the number of bytes each read delivered, for the install progress bar.
    pub fn with_progress(mut self, on_bytes: Arc<dyn Fn(u64) + Send + Sync>) -> Self {
        self.on_bytes = Some(on_bytes);
        self
    }
}

struct Counting<R> {
    inner: R,
    on_bytes: Arc<dyn Fn(u64) + Send + Sync>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        (self.on_bytes)(n as u64);
        Ok(n)
    }
}

impl BlobSource for LeagueSource {
    fn open(&self, req: &BlobRequest<'_>) -> crate::Result<Box<dyn Read>> {
        let Some(url) = self.links.get(&req.file.sha256.as_str().to_ascii_lowercase()) else {
            return Err(crate::Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("no download link for {} from the league server", req.file.path),
            )));
        };
        let reader = crate::origin::open_blob_url(self.origin.agent(), url, req)?;
        Ok(match &self.on_bytes {
            Some(cb) => Box::new(Counting { inner: reader, on_bytes: cb.clone() }),
            None => reader,
        })
    }
}

impl ReleaseSource for LeagueSource {
    fn fetch_release(&self, verifier: &dyn SignatureVerifier) -> crate::Result<FetchedRelease> {
        self.origin.fetch_release(verifier)
    }

    fn fetch_component(&self, pinned_url: &str) -> crate::Result<Vec<u8>> {
        self.origin.fetch_component(pinned_url)
    }

    fn describe(&self) -> String {
        self.origin.base().to_string()
    }
}

/// A signed link past its 6 h: the blob store answers 403. Asked again of the backend once, then the fetch resumes.
pub fn link_expired(err: &crate::Error) -> bool {
    err.to_string().contains(" answered 403 ")
}
```

The import line has to match the crate. If `ReleaseSource` is re-exported elsewhere, or `crate::Error::PlainIo` is spelled differently, use the existing names; `net.rs` already uses `Error::PlainIo(std::io::Error::new(...))`. `FetchOutcomeState::Incomplete` carries no error, so the reborn-ui install loop (Task 8) does not use `link_expired` on a fetch outcome. It calls `request_download` again whenever a fetch comes back `Incomplete`, at most once. `link_expired` serves the per-file errors that `ContentService::download` surfaces as `Err`.

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core league_edition origin 2>&1 | tail -8`
Expected: all pass, including every existing `origin` test (its blob behaviour is unchanged).

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/origin.rs crates/reborn-core/src/league_edition.rs
git commit -m "core: members-only league downloads (backend-signed links) through the content pipeline

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: League game and friendly rooms (core client)

**Files:**
- Modify: `crates/reborn-core/src/league_edition.rs`

**Interfaces:**
- Produces:
  - `league_edition::RoomOpen { Created(Value), Join(String), InSeries(Option<String>), UpdateRequired(String), Refused(String) }`
  - `league_edition::open_room(http, origin, token, league_game_version: Option<&str>) -> Result<RoomOpen, LeagueError>`
  - `league_edition::host_friendly(http, origin, token, ruleset: &str, league_game_version: Option<&str>) -> Result<RoomOpen, LeagueError>`

- [ ] **Step 1: Write the failing tests** (append to `league_edition.rs`'s tests)

```rust
    const ROOM: &str = "https://api.test/v1/league/room";
    const FRIENDLY: &str = "https://api.test/v1/league/friendly";

    fn sent(http: &FakeHttp) -> serde_json::Value {
        serde_json::from_slice(&http.posts.lock().unwrap()[0].1).unwrap()
    }

    #[test]
    fn a_league_room_sends_only_the_installed_version() {
        let http = FakeHttp::default().reply(ROOM, 200, r#"{"ok":true,"action":"created","room":{"room_id":"r1","code":"K7"}}"#);
        assert_eq!(open_room(&http, ORIGIN, "tok", Some("2026.10.08")).unwrap(),
            RoomOpen::Created(serde_json::json!({"room_id":"r1","code":"K7"})));
        assert_eq!(sent(&http), serde_json::json!({"league_game_version":"2026.10.08"}), "the server derives the fixture");
        assert_eq!(http.posts.lock().unwrap()[0].2.as_deref(), Some("tok"));
    }

    #[test]
    fn room_answers_map() {
        let join = FakeHttp::default().reply(ROOM, 200, r#"{"ok":true,"action":"join","code":"K7QP2M"}"#);
        assert_eq!(open_room(&join, ORIGIN, "t", None).unwrap(), RoomOpen::Join("K7QP2M".into()));
        let series = FakeHttp::default().reply(ROOM, 409, r#"{"ok":false,"error":"in_series","match_id":"m-1"}"#);
        assert_eq!(open_room(&series, ORIGIN, "t", None).unwrap(), RoomOpen::InSeries(Some("m-1".into())));
        let old = FakeHttp::default().reply(ROOM, 409, r#"{"ok":false,"error":"league_version_mismatch","required":"2026.10.09"}"#);
        assert_eq!(open_room(&old, ORIGIN, "t", Some("2026.10.08")).unwrap(), RoomOpen::UpdateRequired("2026.10.09".into()));
        for (status, body, why) in [
            (403u16, r#"{"ok":false,"error":"not_member"}"#, "not_member"),
            (409, r#"{"ok":false,"error":"opponent_unlinked"}"#, "opponent_unlinked"),
            (409, r#"{"ok":false,"error":"You are already in a lobby."}"#, "You are already in a lobby."),
        ] {
            let http = FakeHttp::default().reply(ROOM, status, body);
            assert_eq!(open_room(&http, ORIGIN, "t", None).unwrap(), RoomOpen::Refused(why.into()), "{body}");
        }
        let odd = FakeHttp::default().reply(ROOM, 200, r#"{"ok":true,"action":"teleport"}"#);
        assert_eq!(open_room(&odd, ORIGIN, "t", None).unwrap_err(), LeagueError::BadReply);
        let down = FakeHttp::default().reply(ROOM, 503, r#"{"ok":false,"error":"league_unavailable"}"#);
        assert_eq!(open_room(&down, ORIGIN, "t", None).unwrap_err(), LeagueError::Unavailable);
        assert_eq!(open_room(&FakeHttp::default(), ORIGIN, "t", None).unwrap_err(), LeagueError::Unreachable);
    }

    #[test]
    fn a_friendly_names_its_ruleset_and_version() {
        let http = FakeHttp::default().reply(FRIENDLY, 200, r#"{"ok":true,"action":"created","room":{"room_id":"f1","code":"F1"}}"#);
        assert_eq!(host_friendly(&http, ORIGIN, "tok", "raw", Some("2026.10.08")).unwrap(),
            RoomOpen::Created(serde_json::json!({"room_id":"f1","code":"F1"})));
        assert_eq!(sent(&http), serde_json::json!({"ruleset":"raw","league_game_version":"2026.10.08"}));
        let bad = FakeHttp::default();
        assert_eq!(host_friendly(&bad, ORIGIN, "tok", "ranked", None).unwrap_err(), LeagueError::BadReply,
            "only league or raw, decided before anything is sent");
        assert!(bad.posts.lock().unwrap().is_empty());
    }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core league_edition 2>&1 | tail -10`
Expected: compile errors (`open_room`, `host_friendly`, `RoomOpen` not found).

- [ ] **Step 3: Implement** (append to `league_edition.rs`, above the tests)

```rust
/// What a league room call decided (Plan 2): this player hosts (`Created`), the opponent's room is open (`Join`), the
/// fixture's match is already running (`InSeries`), this player's league files are older than the room's
/// (`UpdateRequired` with the version needed), or a stated refusal (`Refused` with the server's `error`).
#[derive(Debug, Clone, PartialEq)]
pub enum RoomOpen {
    Created(serde_json::Value),
    Join(String),
    InSeries(Option<String>),
    UpdateRequired(String),
    Refused(String),
}

fn post_room(http: &dyn Http, origin: &str, token: &str, path: &str, body: serde_json::Value) -> std::result::Result<RoomOpen, LeagueError> {
    let bytes = serde_json::to_vec(&body).map_err(|_| LeagueError::BadReply)?;
    let reply = http.post_json(&league_url(origin, path), &bytes, Some(token)).map_err(|_| LeagueError::Unreachable)?;
    if matches!(reply.status, 403 | 409) {
        let v: serde_json::Value = serde_json::from_str(&reply.body).map_err(|_| status_error(reply.status, &reply.body))?;
        let why = v.get("error").and_then(|e| e.as_str()).ok_or_else(|| status_error(reply.status, &reply.body))?;
        return Ok(match why {
            "in_series" => RoomOpen::InSeries(v.get("match_id").and_then(|m| m.as_str()).map(str::to_string)),
            "league_version_mismatch" => RoomOpen::UpdateRequired(
                v.get("required").and_then(|r| r.as_str()).unwrap_or_default().to_string(),
            ),
            other => RoomOpen::Refused(other.to_string()),
        });
    }
    if !(200..300).contains(&reply.status) {
        return Err(status_error(reply.status, &reply.body));
    }
    let v: serde_json::Value = serde_json::from_str(&reply.body).map_err(|_| LeagueError::BadReply)?;
    match v.get("action").and_then(|a| a.as_str()) {
        Some("created") => v.get("room").cloned().map(RoomOpen::Created).ok_or(LeagueError::BadReply),
        Some("join") => v
            .get("code")
            .and_then(|c| c.as_str())
            .filter(|c| !c.is_empty())
            .map(|c| RoomOpen::Join(c.to_string()))
            .ok_or(LeagueError::BadReply),
        _ => Err(LeagueError::BadReply),
    }
}

fn with_version(mut body: serde_json::Value, version: Option<&str>) -> serde_json::Value {
    if let Some(v) = version {
        body["league_game_version"] = serde_json::Value::from(v);
    }
    body
}

/// `POST /v1/league/room`: this week's league game. The server derives the fixture and the opponent from the bearer;
/// the launcher says only which league build it has, so the server can refuse a mismatch before anyone boots.
pub fn open_room(http: &dyn Http, origin: &str, token: &str, league_game_version: Option<&str>) -> std::result::Result<RoomOpen, LeagueError> {
    post_room(http, origin, token, "/v1/league/room", with_version(serde_json::json!({}), league_game_version))
}

/// `POST /v1/league/friendly`: a members-only private room on the league build. `ruleset` is `league` (the league seed,
/// the default) or `raw` (standard netplay, no seed).
pub fn host_friendly(
    http: &dyn Http,
    origin: &str,
    token: &str,
    ruleset: &str,
    league_game_version: Option<&str>,
) -> std::result::Result<RoomOpen, LeagueError> {
    if !matches!(ruleset, "league" | "raw") {
        return Err(LeagueError::BadReply);
    }
    post_room(
        http,
        origin,
        token,
        "/v1/league/friendly",
        with_version(serde_json::json!({ "ruleset": ruleset }), league_game_version),
    )
}
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core league_edition 2>&1 | tail -6`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/league_edition.rs
git commit -m "core: league game and friendly room clients (version-checked)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Matchmaking knows editions, joins say their version, uploads name their fixture (core)

**Files:**
- Modify: `crates/reborn-core/src/matchmaking.rs` (`Room`, `MatchView`, `Refusal`, `MmError`, `Client::join_room`, `decode`)
- Modify: `crates/reborn-core/src/supervisor.rs` (`SessionIntent`)
- Modify: `crates/reborn-core/src/backend.rs` (`SessionFacts`, `session_facts`, `envelope`)
- Modify: every `SessionIntent { … }` / `SessionFacts { … }` literal. Find them with `grep -rn "SessionIntent {\|SessionFacts {" crates/`.

**Interfaces:**
- Consumes (Plan 2 snapshot):
  - `room.edition`, `room.ruleset`, `room.league`, `room.league_game_version`
  - `match.edition`, `match.ruleset`, `match.league`, `match.league_game_version`
  - the join refusal `409 {error:"league_version_mismatch", required}`
- Produces:
  - `Room.{edition, ruleset, league_game_version}: Option<String>` and `Room.league: Option<Value>`
  - `MatchView.{edition, ruleset, league_game_version}: Option<String>` and `MatchView.league: Option<Value>`
  - `Client::join_room_as(code, league_game_version: Option<&str>)`; `join_room(code)` = `join_room_as(code, None)`
  - `MmError::LeagueUpdateRequired { required: String }` (Display = `LEAGUE_UPDATE_SENTENCE`)
  - `pub const matchmaking::LEAGUE_UPDATE_SENTENCE`
  - `SessionIntent.league_fixture_id: Option<u64>` and `SessionFacts.league_fixture_id: Option<u64>`
  - upload `intent.fixture_id`

- [ ] **Step 1: Write the failing tests**

`matchmaking.rs` test module:

```rust
    #[test]
    fn rooms_and_matches_keep_their_edition_ruleset_and_league_tag() {
        let s: Snapshot = serde_json::from_str(r#"{"room":{"id":"r","code":"C","edition":"league","ruleset":"league",
            "league_game_version":"2026.10.08","league":{"league_id":36,"fixture_id":1234,"week":6}},
            "match":{"id":"m","mode":"casual","edition":"league","ruleset":"raw","league_game_version":"2026.10.08",
            "league":{"league_id":36,"fixture_id":1234,"week":6}}}"#).unwrap();
        let r = s.room.unwrap();
        assert_eq!((r.edition.as_deref(), r.ruleset.as_deref(), r.league_game_version.as_deref()), (Some("league"), Some("league"), Some("2026.10.08")));
        assert_eq!(r.league.unwrap()["fixture_id"], 1234);
        let m = s.matched.unwrap();
        assert_eq!((m.edition.as_deref(), m.ruleset.as_deref()), (Some("league"), Some("raw")));
        let plain: Snapshot = serde_json::from_str(r#"{"room":{"id":"r"},"match":{"id":"m"}}"#).unwrap();
        let (pr, pm) = (plain.room.unwrap(), plain.matched.unwrap());
        assert!(pr.edition.is_none() && pr.league.is_none() && pm.edition.is_none() && pm.ruleset.is_none() && pm.league.is_none());
    }

    #[test]
    fn a_join_says_which_league_build_it_has() {
        let http = crate::net::fake::FakeHttp::default().reply("https://api.test/v1/mm/rooms/K7/join", 200, r#"{"ok":true}"#);
        let c = Client::new(&http, "https://api.test".into(), "tok".into());
        c.join_room_as("K7", Some("2026.10.08")).unwrap();
        c.join_room("K7").unwrap();
        let posts = http.posts.lock().unwrap();
        let first: serde_json::Value = serde_json::from_slice(&posts[0].1).unwrap();
        let second: serde_json::Value = serde_json::from_slice(&posts[1].1).unwrap();
        assert_eq!(first["league_game_version"], "2026.10.08");
        assert!(second.get("league_game_version").is_none(), "a launcher without league files sends nothing");
        assert_eq!(first["max_players"], ROOM_MAX_PLAYERS);
    }

    /// Review Focus 3: never a raw server code on any page.
    #[test]
    fn a_version_refusal_is_its_own_error() {
        let http = crate::net::fake::FakeHttp::default().reply("https://api.test/v1/mm/rooms/K7/join", 409,
            r#"{"ok":false,"error":"league_version_mismatch","required":"2026.10.09"}"#);
        let c = Client::new(&http, "https://api.test".into(), "tok".into());
        let e = c.join_room_as("K7", Some("2026.10.08")).unwrap_err();
        assert_eq!(e, MmError::LeagueUpdateRequired { required: "2026.10.09".into() });
        assert_eq!(e.to_string(), LEAGUE_UPDATE_SENTENCE);
    }
```

Before writing these, check two names in the existing test module:
- the `Client::new` signature (`grep -n "pub fn new" crates/reborn-core/src/matchmaking.rs`);
- whether `MmError` derives `PartialEq`. If it does not, add `PartialEq` to its derive; every variant holds a `String`, so it derives cleanly.

`backend.rs` test module:

```rust
    #[test]
    fn a_league_upload_names_its_fixture() {
        let facts = SessionFacts { session_id: "s1".into(), league_match: true, exit: "clean".into(), exit_code: Some(0), match_id: None, league_fixture_id: Some(1234) };
        let env = envelope(&entry("s1", Some("online")), CAP, Some(&facts), "3.1.0", "2026-10-08T21:00:00Z").unwrap();
        assert_eq!((env["intent"]["league"].as_bool(), env["intent"]["fixture_id"].as_u64()), (Some(true), Some(1234)));
        let casual = SessionFacts { league_match: false, league_fixture_id: None, ..facts };
        let env = envelope(&entry("s1", Some("online")), CAP, Some(&casual), "3.1.0", "2026-10-08T21:00:00Z").unwrap();
        assert!(env["intent"].get("fixture_id").is_none(), "only a league game names a fixture");
    }

    #[test]
    fn session_facts_carry_the_fixture_from_the_intent() {
        let old: SessionIntent = serde_json::from_str(r#"{"league_match":true}"#).unwrap();
        assert_eq!(old.league_fixture_id, None, "a record an older build wrote still reads");
        let new: SessionIntent = serde_json::from_str(r#"{"league_match":true,"league_fixture_id":1234}"#).unwrap();
        assert_eq!(new.league_fixture_id, Some(1234));
    }
```

(`entry`, `CAP` and `SessionFacts`' existing fields come from `backend.rs`'s test module. If a field list differs, match the struct as it is.)

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core rooms_and_matches a_join_says a_version_refusal a_league_upload session_facts_carry 2>&1 | tail -15`
Expected: compile errors.

- [ ] **Step 3: Implement**

`matchmaking.rs`: append these fields to `Room` (after `capacity`) and to `MatchView` (after `party`); both structs are `#[serde(default)]`.

```rust
    /// `"league"` for a League edition room (spec 2026-10-08 v2 §4), else absent (retail).
    pub edition: Option<String>,
    /// The rules: `casual`, `ranked`, `league` (league seed) or `raw` (no seed). Absent from an older server: by mode.
    pub ruleset: Option<String>,
    /// `{ league_id, fixture_id, week }` for a league game; absent for friendlies and retail.
    pub league: Option<serde_json::Value>,
    /// The league build this room plays; a joiner on another version is refused (Plan 2).
    pub league_game_version: Option<String>,
```

Add `required: Option<String>` (with `#[serde(default)]` if the struct is not already `#[serde(default)]`) to `struct Refusal`. In `decode`, before the generic `Refused` arm, insert:

```rust
            return Err(match (r.ok, r.error) {
                (false, Some(e)) if e == "league_version_mismatch" => MmError::LeagueUpdateRequired { required: r.required.unwrap_or_default() },
                (false, Some(e)) if !e.trim().is_empty() => MmError::Refused(e),
                _ => MmError::Protocol(format!("HTTP {status} with no readable reason")),
            });
```

(This replaces the existing `return Err(match (r.ok, r.error) { … })`. Keep its two existing arms.)

Add the variant and the sentence:

```rust
    /// The room plays a newer league build than this launcher has installed (Plan 2's version check).
    LeagueUpdateRequired { required: String },
```

```rust
/// The one sentence any page shows for a league version refusal. Plan 3a highlights the Update button on it.
pub const LEAGUE_UPDATE_SENTENCE: &str = "Your league files are out of date. Update league files to play.";
```

with the Display arm `MmError::LeagueUpdateRequired { .. } => f.write_str(LEAGUE_UPDATE_SENTENCE),`. Fix any other exhaustive `match` on `MmError` the compiler names: treat the new variant like `Refused`.

Replace `join_room`:

```rust
    pub fn join_room(&self, code: &str) -> Result<serde_json::Value, MmError> {
        self.join_room_as(code, None)
    }

    /// Join by code, saying which league build this launcher has. The server ignores the field for a retail room and
    /// refuses a League room on another version (`MmError::LeagueUpdateRequired`).
    pub fn join_room_as(&self, code: &str, league_game_version: Option<&str>) -> Result<serde_json::Value, MmError> {
        // `max_players`: this launcher can be the 3rd or 4th player of a room (an older one is refused with a sentence).
        let mut body = serde_json::json!({ "max_players": ROOM_MAX_PLAYERS });
        if let Some(v) = league_game_version {
            body["league_game_version"] = serde_json::Value::from(v);
        }
        self.post(&format!("/v1/mm/rooms/{}/join", url_path(code)), body)
    }
```

`supervisor.rs` `SessionIntent`, after `match_id`:

```rust
    /// The league fixture this session played, when it was a league game (spec 2026-10-08). Goes up as
    /// `intent.fixture_id` beside `intent.league`. Absent for everything else and every record an older build wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub league_fixture_id: Option<u64>,
```

`backend.rs`:
- Add `pub league_fixture_id: Option<u64>,` to `SessionFacts`, after `match_id`, documented "The league fixture, from the intent, when this was a league game".
- In `session_facts`: `league_fixture_id: r.intent.as_ref().and_then(|i| i.league_fixture_id),`.
- In `envelope`, right after `let mut env = serde_json::json!({ … });`:

```rust
    // A league game names its fixture beside `intent.league` (Plan 2: forward.js reads both). Only for a league game.
    if let Some(fixture) = facts.filter(|f| f.league_match).and_then(|f| f.league_fixture_id) {
        env["intent"]["fixture_id"] = serde_json::Value::from(fixture);
    }
```

If `env` is not `mut` today, make it so. Add `league_fixture_id: None` to every `SessionIntent { … }` and `SessionFacts { … }` literal the grep above printed (production and tests, in both crates).

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core 2>&1 | tail -8`, then `CARGO_BUILD_JOBS=1 cargo build -p reborn-ui 2>&1 | grep -E "^error" -A5`
Expected: all core tests pass, and reborn-ui builds. If reborn-ui fails only on a `SessionIntent {` literal or an `MmError` match, fix it here.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core crates/reborn-ui/src
git commit -m "mm: rooms/matches carry edition+ruleset+league; joins send the league build; uploads name the fixture

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: League rooms boot the league image with the right seed (reborn-ui netplay)

**Files:**
- Modify: `crates/reborn-ui/src/dolphin_settings.rs` (`netplay_boot_state`, `mm_preflight`, `netplay_open`)
- Modify: `crates/reborn-ui/src/online_mm.rs` (`seed_for_match`, `edition_of`, `current_match_league`, `with_client_mm`, `mm_room_join`, `run_step`, `snapshot_json`)
- Modify: `crates/reborn-ui/src/main.rs` (`sync_active_judged_for`)
- Modify: `crates/reborn-ui/ui/online-mm.js` (room card)
- Create (stub, filled in Task 8): `crates/reborn-ui/src/league_edition.rs` with `installed_league_version()` and `ready_league_image()`

**Interfaces:**
- Consumes: Task 1 paths/`LeagueInfo`; Task 6 snapshot fields, `join_room_as` and `MmError::LeagueUpdateRequired`.
- Produces:
  - `online_mm::seed_for_match(m: &MatchView) -> &'static str` (`"league" | "raw" | "ranked" | "regular" | "pickup"`)
  - `online_mm::edition_of(m: &MatchView) -> Option<String>`
  - `online_mm::current_match_league() -> Option<u64>`
  - `dolphin_settings::netplay_open(…, seed_kind, edition: Option<String>)`, with `edition` as the new last parameter
  - `dolphin_settings::mm_preflight(runtime, game, seed_kind, edition: Option<&str>)`
  - `main::sync_active_judged_for(p, runtime_hint, already_judged, build: GameBuild)`
  - `league_edition::installed_league_version() -> Option<String>`
  - `league_edition::ready_league_image(p: &Platform) -> Result<PathBuf, String>`
  - `mm_room_join` rejects with `serde_json::Value`: a string, as before, or `{kind:"league_update_required", message, required}`.

- [ ] **Step 1: Write the failing tests**

`online_mm.rs` test module:

```rust
    fn mv(json: &str) -> reborn_core::matchmaking::MatchView {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_seed_follows_edition_then_ruleset_then_mode() {
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","edition":"league","ruleset":"league","mode":"casual"}"#)), "league");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","edition":"league","ruleset":"raw"}"#)), "raw");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","edition":"league"}"#)), "league", "a league room without a ruleset plays league rules");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","mode":"casual","ruleset":"ranked","seed_kind":"regular"}"#)), "ranked");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","mode":"ranked"}"#)), "ranked", "an older server still boots ranked by mode");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","mode":"casual","seed_kind":"pickup"}"#)), "pickup");
        assert_eq!(seed_for_match(&mv(r#"{"id":"m","mode":"casual"}"#)), "regular");
    }

    #[test]
    fn every_seed_choice_goes_through_seed_for_match() {
        let src = include_str!("online_mm.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        assert!(!production.contains("Some(&m.mode)"), "a seed is still picked from mode alone");
        assert_eq!(production.matches("seed_for_match(m)").count(), 2, "ready's preflight and the room launch");
    }

    #[test]
    fn a_league_match_is_launched_as_one() {
        let snap: reborn_core::matchmaking::Snapshot = serde_json::from_str(
            r#"{"match":{"id":"m1","relay_state":"launching","edition":"league","league":{"league_id":36,"fixture_id":1234,"week":6}}}"#).unwrap();
        lock(&STATE).snapshot = Some(snap);
        assert_eq!(current_match_league(), Some(1234));
        let snap: reborn_core::matchmaking::Snapshot = serde_json::from_str(r#"{"match":{"id":"m1","relay_state":"launching","edition":"league"}}"#).unwrap();
        lock(&STATE).snapshot = Some(snap);
        assert_eq!(current_match_league(), None, "a friendly is not a league game");
        lock(&STATE).snapshot = None;
        assert_eq!(current_match_league(), None);
    }

    #[test]
    fn join_error_shapes() {
        let update = join_error(reborn_core::matchmaking::MmError::LeagueUpdateRequired { required: "2026.10.09".into() });
        assert_eq!(update, serde_json::json!({"kind":"league_update_required","message":reborn_core::matchmaking::LEAGUE_UPDATE_SENTENCE,"required":"2026.10.09"}));
        let plain = join_error(reborn_core::matchmaking::MmError::Refused("No such lobby.".into()));
        assert_eq!(plain, serde_json::Value::String("No such lobby.".into()), "every other refusal stays the sentence it was");
    }
```

(If the existing tests put snapshots into `STATE` through a helper, use that helper.)

`dolphin_settings.rs` test module:

```rust
    #[test]
    fn league_and_raw_seeds_are_their_own_files() {
        let d = std::path::Path::new("/data");
        assert_eq!(boot_state_path(d, Some("league")), Some(reborn_core::league_edition::league_seed_path(d)));
        assert_eq!(boot_state_path(d, Some("raw")), None, "a raw room boots the disc");
        assert_eq!(boot_state_path(d, Some("ranked")), Some(d.join("library/online/seed").join("ranked.s01")));
        assert_eq!(boot_state_path(d, None), Some(d.join("library/online/seed").join("regular.s01")));
    }

    #[test]
    fn a_netplay_launch_boots_the_edition_it_was_given() {
        let src = include_str!("dolphin_settings.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        let body = &production[production.find("pub async fn netplay_open").unwrap()..];
        assert!(body.contains("edition:Option<String>"));
        assert!(body.contains("let league=edition.as_deref()==Some(\"league\");"));
        assert!(body.contains("crate::league_edition::ready_league_image(&p)?"));
        assert!(body.contains("build:if league{GameBuild::League}else{GameBuild::Offline}"));
        assert!(body.contains("let fixture=crate::online_mm::current_match_league();"));
        assert!(body.contains("league_fixture_id:fixture"));
    }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-ui the_seed_follows every_seed_choice a_league_match_is join_error_shapes league_and_raw_seeds a_netplay_launch_boots 2>&1 | tail -15`
Expected: compile errors.

- [ ] **Step 3: Implement**

**Create `crates/reborn-ui/src/league_edition.rs`** (Task 8 adds the commands to this file), and add `mod league_edition;` to `main.rs` beside the other `mod` lines:

```rust
//! The League edition's commands (Plan 3b; contracts file "Plan 3b → Plan 3a") and the two helpers the netplay
//! path shares with them. Every rejection is a fixed sentence; nothing here ever shows a status, URL or link.

use std::path::PathBuf;

use reborn_core::league_edition::{self as le, LeagueInfoError};
use reborn_core::platform::Platform;

pub(crate) const NOT_A_MEMBER: &str = "League files are for league members. Sign in with the Discord account your team is linked to.";
pub(crate) const NOT_INSTALLED: &str = "Install the league files from the League tab first.";
pub(crate) const WRONG_BUILD: &str = "These league files are for a different league build. Update the launcher, then update the league files.";
pub(crate) const DAMAGED: &str = "The league files look damaged. Update league files on the League tab to repair them.";

/// The installed league build's version (its content pointer), or `None` when it is not installed.
pub(crate) fn installed_league_version() -> Option<String> {
    let p = Platform::detect();
    let svc = reborn_core::content::ContentService::for_platform(&p).ok()?;
    svc.installed(reborn_core::inventory::ComponentId::LeagueGame.as_str()).ok().flatten().map(|ptr| ptr.current.version)
}

/// Whether the cached League answer says this player is a member (3a Task 1's `league.json`). A member whose league
/// server is down keeps their cache, so an outage never locks them out of offline play.
pub(crate) fn is_member(p: &Platform) -> bool {
    reborn_core::league_home::cache_path(p).map(|path| reborn_core::league_home::load_cache(&path).member).unwrap_or(false)
}

/// The league disc, ready to boot: a member, the build installed, `league.json` naming this launcher's game ID.
pub(crate) fn ready_league_image(p: &Platform) -> Result<PathBuf, String> {
    if !is_member(p) {
        return Err(NOT_A_MEMBER.into());
    }
    let data = p.paths.data_root().map_err(|_| NOT_INSTALLED.to_string())?;
    match le::read_league_info(&data) {
        Ok(_) => {}
        Err(LeagueInfoError::Missing) => return Err(NOT_INSTALLED.into()),
        Err(LeagueInfoError::WrongGameId(_)) => return Err(WRONG_BUILD.into()),
        Err(LeagueInfoError::Unreadable) => return Err(DAMAGED.into()),
    }
    let image = le::league_image_path(&data);
    if !image.is_file() {
        return Err(DAMAGED.into());
    }
    Ok(image)
}
```

**`main.rs`:** split `sync_active_judged` so a build can be named, with retail unchanged:

```rust
pub(crate) fn sync_active_judged(
    p: &Platform,
    runtime_hint: Option<&str>,
    already_judged: Option<reborn_core::game_identity::ImageStanding>,
) -> Result<ModSyncView, String> {
    sync_active_judged_for(p, runtime_hint, already_judged, GameBuild::Offline)
}

/// [`sync_active_judged`] for a named build: the League edition composes `Load/Textures/<LEAGUE_GAME_ID>` from its own
/// pristine tree plus the shared mod list (reborn_core::mods::payload_fallback_for).
pub(crate) fn sync_active_judged_for(
    p: &Platform,
    runtime_hint: Option<&str>,
    already_judged: Option<reborn_core::game_identity::ImageStanding>,
    build: GameBuild,
) -> Result<ModSyncView, String> {
    // … the existing body, with `GameBuild::Offline.game_id()` replaced by `build.game_id()` …
}
```

Move the existing body into `sync_active_judged_for` verbatim; only the `GameId::parse(GameBuild::Offline.game_id())` line changes. If a source-scan test in `main.rs` slices `"\npub(crate) fn sync_active_judged("` up to the next function, it still finds the function. Run it in Step 4 and, if it now slices the thin wrapper, point its slice at `sync_active_judged_for`.

**`dolphin_settings.rs`:**

Replace `netplay_boot_state` with a pure path function plus the existing file check:

```rust
/// The seed file a room boots from. `league` is the league build's own seed (league-content); `raw` is none (the disc
/// boots to its title screen, standard netplay); the rest are online-content's, with ranked falling back to regular on
/// an install that predates ranked.s01.
pub(crate) fn boot_state_path(data_root:&std::path::Path,seed_kind:Option<&str>)->Option<std::path::PathBuf>{
 match seed_kind.map(str::trim) {
  Some("league")=>Some(reborn_core::league_edition::league_seed_path(data_root)),
  Some("raw")=>None,
  other=>{
   let dir=data_root.join("library/online/seed");
   let name=match other { Some("ranked")=>"ranked", o=>seed_kind_or_default(o) };
   Some(dir.join(format!("{name}.s01")))
  }
 }
}

fn netplay_boot_state(p:&Platform,seed_kind:Option<&str>)->Option<std::path::PathBuf>{
 let data=p.paths.data_root().ok()?;
 let seed=boot_state_path(&data,seed_kind)?;
 if seed.is_file() { return Some(seed); }
 let fallback=data.join("library/online/seed").join("regular.s01");
 (seed_kind.map(str::trim)==Some("ranked") && fallback.is_file()).then_some(fallback)
}
```

`mm_preflight`: add a fourth parameter `edition:Option<&str>`. Inside, after `found` is computed:

```rust
 let league=edition==Some("league");
 let image_ok=if league { crate::league_edition::ready_league_image(&p).is_ok() } else { found.is_ok() };
```

Use `image_ok` in place of `found.is_ok()` in the JSON. Add `"edition":edition,"league_game_version":crate::league_edition::installed_league_version(),` to the JSON object. For a league room, the retail `pristine_*`/`chain_head` fields describe the retail disc and the server ignores them for an `edition:"league"` match (Plan 2), so leave them.

`netplay_open`: add `edition:Option<String>` as the **last** parameter. Keep this file's compact style, because the Step 1 source scan matches these exact strings.
- Right after `require_closed(found.path())?;`, add `let league=edition.as_deref()==Some("league");`.
- Change `let chosen_disc=may_play_before_boot(&p,&game_path)?;` to:

```rust
  let chosen_disc=if league { reborn_core::game_identity::ImageStanding::Reborn } else { may_play_before_boot(&p,&game_path)? };
```

- Change `let online=match netplay_image(&p){…};` to:

```rust
  let online=if league { Some(crate::league_edition::ready_league_image(&p)?) } else { match netplay_image(&p){Ok(Some(path))=>Some(path),Ok(None)=>None,Err(e)=>return Err(e)} };
```

- Change the texture sync call to `crate::sync_active_judged_for(&p,runtime.as_deref(),Some(standing),if league{GameBuild::League}else{GameBuild::Offline})`.
- Directly before `let req=LaunchRequest{`, add `let fixture=crate::online_mm::current_match_league();`. In the `LaunchRequest`:
  - replace `build:GameBuild::Offline` with `build:if league{GameBuild::League}else{GameBuild::Offline}`;
  - replace `league_match:false` with `league_match:fixture.is_some()`.
- In the `begin_with_intent` call, use `Some(SessionIntent{league_match:req.league_match,match_id:session_match.clone(),league_fixture_id:fixture})`.

The JS caller (`online.js:86`) passes named args without `edition`, so it arrives as `None`, which is retail, as today.

**`online_mm.rs`:**

Add after `current_match_id()`:

```rust
/// The seed a match boots from. A League edition match plays the league seed unless its ruleset is `raw`; otherwise
/// the retail rule: a ranked ruleset (or, from an older server, ranked mode) boots ranked.s01, else the server's kind.
pub(crate) fn seed_for_match(m: &reborn_core::matchmaking::MatchView) -> &'static str {
    if m.edition.as_deref().map(str::trim) == Some("league") {
        return if m.ruleset.as_deref().map(str::trim) == Some("raw") { "raw" } else { "league" };
    }
    let by = if m.ruleset.as_deref().map(str::trim) == Some("ranked") { "ranked" } else { m.mode.as_str() };
    crate::dolphin_settings::match_seed_kind(m.seed_kind.as_deref(), Some(by))
}

/// `Some("league")` for a League edition match, else `None` (retail).
pub(crate) fn edition_of(m: &reborn_core::matchmaking::MatchView) -> Option<String> {
    m.edition.as_deref().map(str::trim).filter(|e| *e == "league").map(str::to_string)
}

/// The running match's league fixture, when it is a league game: its results then go up with `intent.league = true`
/// and the fixture, which forward.js needs. A league friendly has no fixture and is not a league game.
pub fn current_match_league() -> Option<u64> {
    let s = lock(&STATE);
    s.snapshot.as_ref()?.matched.as_ref()?.league.as_ref()?.get("fixture_id")?.as_u64()
}
```

In `run_step`:
- **`Step::Ready`:** use `crate::dolphin_settings::mm_preflight(runtime.as_deref(), game.as_deref(), m.map(seed_for_match_ref), m.and_then(|m| edition_of(m)).as_deref())`.
  - Write it so the source contains `seed_for_match(m)` literally, e.g. `m.map(|m| seed_for_match(m))`.
  - The edition argument is `m.and_then(edition_of).as_deref()` bound to a local first, so it lives long enough.
- **`Step::OpenRoom`:** the last `seed_kind` argument becomes `m.map(|m| seed_for_match(m).to_string())`, and append `m.and_then(edition_of)` as the new `edition` argument.

Refactor `with_client` so the join can keep the typed error:

```rust
fn with_client_mm<T>(f: impl FnOnce(&Client) -> Result<T, MmError>) -> Result<T, MmError> {
    let p = Platform::detect();
    let token = reborn_core::identity::token(&p).map_err(|e| MmError::Offline(e.to_string()))?.ok_or(MmError::SignedOut)?;
    let origin = reborn_core::identity::origin(&p).map_err(|e| MmError::Offline(e.to_string()))?;
    let http = crate::http_client();
    let client = Client::new(&http, origin, token);
    f(&client)
}

fn with_client<T>(f: impl FnOnce(&Client) -> Result<T, MmError>) -> Result<T, String> {
    with_client_mm(f).map_err(|e| e.to_string())
}

/// A join's rejection: the sentence it always was, or, for a league version refusal, the shape Plan 3a matches on.
pub(crate) fn join_error(e: MmError) -> serde_json::Value {
    match e {
        MmError::LeagueUpdateRequired { required } => serde_json::json!({
            "kind": "league_update_required", "message": reborn_core::matchmaking::LEAGUE_UPDATE_SENTENCE, "required": required
        }),
        other => serde_json::Value::String(other.to_string()),
    }
}
```

The old `with_client` mapped an identity error with `e.to_string()`. The new one maps it to `MmError::Offline`, which displays "Can't reach the Reborn servers right now. Trying again." That is acceptable for a broken identity store; note it in the report.

`mm_room_join`:

```rust
#[tauri::command]
pub async fn mm_room_join(code: String) -> Result<serde_json::Value, serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let version = crate::league_edition::installed_league_version();
        with_client_mm(|c| c.join_room_as(code.trim(), version.as_deref())).map_err(join_error)
    })
    .await
    .map_err(|e| serde_json::Value::String(e.to_string()))?
}
```

`online-mm.js` calls it through `act()`, which shows `reason(e)`. `shell.js`'s `reason` returns a string as is and an object's `.message`, so both shapes display correctly.

`snapshot_json`:
- In the `room` object, after `"capacity": …`, add `"edition": r.edition, "ruleset": r.ruleset, "league": r.league, "league_game_version": r.league_game_version,`.
- In the `match` object, after `"hold_reason": m.hold_reason, "seat": m.seat,`, add `"edition": m.edition, "ruleset": m.ruleset, "league": m.league,`.

**`online-mm.js`:** in the room card (the block with `const kind = kindName(r.seed_kind);`). A league room has no kind switch, because the server refuses one, and its title says what it is.

```js
  const leagueGame = r.league && typeof r.league === 'object' ? r.league : null;
  const leagueRoom = r.edition === 'league';
  const kindControl = r.you_host === true && !leagueRoom
```

(the rest of the `kindControl` expression is unchanged), and replace the title line with:

```js
    el('div', { class: 'on-card-h' },
      el('h2', { text: leagueGame ? `League game · Week ${leagueGame.week ?? '?'}` : leagueRoom ? 'League friendly' : `Your room · ${kind}` }),
      leagueGame ? el('span', { text: 'League rules · Best of 3' })
        : leagueRoom ? el('span', { text: r.ruleset === 'raw' ? 'Raw game' : 'League settings' })
        : r.mode === 'ranked' ? el('span', { text: 'Ranked' }) : null),
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-ui 2>&1 | tail -8`, then `node Tools/check-online-mm.mjs` and `node --test crates/reborn-ui/ui/online-model.test.mjs`
Expected: everything passes, including the existing `match_seed_kind` tests and every `netplay_open` source-scan test in `main.rs` (`grep -n "pub async fn netplay_open" crates/reborn-ui/src/main.rs` lists them). If a scan test pins the old `netplay_open` parameter list or `build:GameBuild::Offline`, update its expected string to the new one; say which in the report.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/src crates/reborn-ui/ui/online-mm.js
git commit -m "netplay: league rooms boot the league disc with league/raw seeds; joins carry the league build; league room card

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: The League edition commands (reborn-ui)

**Files:**
- Modify: `crates/reborn-ui/src/league_edition.rs` (commands)
- Modify: `crates/reborn-ui/src/main.rs` (`launch` takes `edition`; register five commands)

**Interfaces:**
- Consumes:
  - Task 1 (`ComponentId::League*`, `read_league_info`)
  - Task 4 (`request_download`, `LeagueSource`)
  - Task 5 (`open_room`, `host_friendly`, `RoomOpen`)
  - Task 7 (`ready_league_image`, `installed_league_version`, `sync_active_judged_for`)
  - `content_updates::{verifier, image_hook, load_settings, origin_url}`
  - `content::pipeline::{Updater, CheckRequest, Depth, ComponentState}`
  - 3a Task 1's `league_home` cache
- Produces (exactly the contracts file):
  - `league_edition_status() -> {installed, version, latest, update_available, size_bytes, game_id}`
  - `league_edition_install()`: emits `league-install-progress` `{phase, done, total}` and resolves `{version}`.
  - `league_play_offline() -> null`
  - `league_host_friendly(ruleset) -> {action:"created", room}`
  - `league_room_open() -> {action:"created"|"join"|"in_series", room?, code?, match_id?}`
  - Rejections are `LeagueCommandError { kind, message, required? }`. `kind` is `"league_update_required"` (with `required`), `"not_member"`, `"not_installed"`, `"rate_limited"` or `"unavailable"`.

- [ ] **Step 1: Write the failing tests** (bottom of `crates/reborn-ui/src/league_edition.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use reborn_core::league_edition::{LeagueError, RoomOpen};

    #[test]
    fn room_answers_become_the_contracts_shapes() {
        assert_eq!(room_view(RoomOpen::Created(serde_json::json!({"code":"K"}))).unwrap(), serde_json::json!({"action":"created","room":{"code":"K"}}));
        assert_eq!(room_view(RoomOpen::Join("K7".into())).unwrap(), serde_json::json!({"action":"join","code":"K7"}));
        assert_eq!(room_view(RoomOpen::InSeries(Some("m".into()))).unwrap(), serde_json::json!({"action":"in_series","match_id":"m"}));
        let up = room_view(RoomOpen::UpdateRequired("2026.10.09".into())).unwrap_err();
        assert_eq!((up.kind, up.required.as_deref()), ("league_update_required", Some("2026.10.09")));
        let msg = |why: &str| room_view(RoomOpen::Refused(why.into())).unwrap_err().message;
        assert_eq!(msg("opponent_unlinked"), OPPONENT_UNLINKED);
        assert_eq!(msg("no_game"), NO_GAME_NOW);
        assert_eq!(msg("already_final"), ALREADY_FINAL);
        assert_eq!(msg("not_member"), NOT_A_MEMBER);
        assert_eq!(msg("You are already in a lobby."), "You are already in a lobby.");
        assert_eq!(msg("https://x leaked"), ROOM_FAILED, "an unknown reason is never shown");
    }

    #[test]
    fn backend_failures_are_sentences() {
        assert_eq!(league_error(LeagueError::NotMember).kind, "not_member");
        assert_eq!(league_error(LeagueError::RateLimited).kind, "rate_limited");
        for e in [LeagueError::SignedOut, LeagueError::Unavailable, LeagueError::Unreachable, LeagueError::BadReply, LeagueError::UnknownVersion] {
            let c = league_error(e.clone());
            assert!(!c.message.contains("http") && !c.message.contains("40") && !c.message.contains("50"), "{e:?}: {}", c.message);
        }
    }

    #[test]
    fn status_reads_installed_and_offered_versions() {
        assert_eq!(status_view(Some("2026.10.08"), Some("2026.10.08"), 0, true),
            serde_json::json!({"installed":true,"version":"2026.10.08","latest":"2026.10.08","update_available":false,"size_bytes":0,"game_id":reborn_core::league_edition::LEAGUE_GAME_ID}));
        assert_eq!(status_view(Some("2026.10.08"), Some("2026.10.09"), 700, true)["update_available"], true);
        assert_eq!(status_view(None, Some("2026.10.09"), 6_000_000_000, false)["installed"], false);
        assert_eq!(status_view(Some("2026.10.08"), None, 0, true)["update_available"], false, "offline: no news is not an update");
        assert_eq!(status_view(Some("2026.10.08"), Some("2026.10.08"), 0, false)["installed"], false, "a damaged league.json is not installed");
    }

    #[test]
    fn only_league_or_raw_friendlies() {
        assert!(friendly_ruleset("league").is_ok());
        assert!(friendly_ruleset("raw").is_ok());
        assert_eq!(friendly_ruleset("ranked").unwrap_err().kind, "unavailable");
    }

    #[test]
    fn the_five_commands_are_registered() {
        let main = include_str!("main.rs");
        for c in ["league_edition_status", "league_edition_install", "league_play_offline", "league_host_friendly", "league_room_open"] {
            assert!(main.contains(&format!("league_edition::{c},")), "{c} is not in generate_handler!");
        }
    }

    #[test]
    fn offline_league_play_boots_the_league_build() {
        let main = include_str!("main.rs");
        let body = &main[main.find("async fn launch(").unwrap()..];
        assert!(body.contains("edition: Option<String>"));
        assert!(body.contains("crate::league_edition::ready_league_image(&p)?"));
        assert!(body.contains("GameBuild::League"));
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-ui league_edition 2>&1 | tail -15`
Expected: compile errors (`room_view`, `league_error`, `status_view`, `friendly_ruleset` not found).

- [ ] **Step 3: Implement**

**`main.rs` `launch`:** add `edition: Option<String>,` as the last parameter. After `find_runtime_and_game_for(...)` returns `(found, game_path)`, replace:

```rust
        let standing = may_play_before_boot(&p, &game_path)?;
```

with:

```rust
        // The League tab's Play offline boots the league build (spec v2 §4): its own disc, its own game ID, never
        // judged against the retail patch chain (it is not on it). Retail is exactly what it was.
        let league = edition.as_deref() == Some("league");
        let (game_path, standing, build) = if league {
            (crate::league_edition::ready_league_image(&p)?, reborn_core::game_identity::ImageStanding::Reborn, GameBuild::League)
        } else {
            let standing = may_play_before_boot(&p, &game_path)?;
            (game_path, standing, GameBuild::Offline)
        };
```

and in the `LaunchRequest` literal use `game_path,`, `textures: standing,` and `build,` in place of `build: GameBuild::Offline,`. The Play page's `call('launch', {...})` sends no `edition`, so it stays retail.

Register in `generate_handler!`:

```rust
            league_edition::league_edition_status,
            league_edition::league_edition_install,
            league_edition::league_play_offline,
            league_edition::league_host_friendly,
            league_edition::league_room_open,
```

**`league_edition.rs`:** append below `ready_league_image`.

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use reborn_core::content::pipeline::{CheckRequest, ComponentState, Depth, Updater};
use reborn_core::inventory::ComponentId;
use reborn_core::league_edition::{LeagueError, LeagueSource, RoomOpen};
use reborn_core::origin::Origin;
use tauri::Emitter;

const COMPONENTS: [ComponentId; 2] = [ComponentId::LeagueGame, ComponentId::LeagueContent];

const SIGN_IN_FIRST: &str = "Sign in with Discord on the Play page first.";
const BUSY: &str = "The league server is busy. Try again in a minute.";
const UNAVAILABLE: &str = "The league server couldn't be reached. Try again in a minute.";
const NOT_PUBLISHED: &str = "League files aren't available yet. Check back after the commissioners publish them.";
const INSTALL_FAILED: &str = "The league files didn't finish downloading. Press Install again to resume where it stopped.";
pub(crate) const NO_GAME_NOW: &str = "There's no league game for you this week.";
pub(crate) const ALREADY_FINAL: &str = "This week's series is already final.";
pub(crate) const OPPONENT_UNLINKED: &str = "Your opponent hasn't linked Discord on the league site yet.";
pub(crate) const ROOM_FAILED: &str = "The league game couldn't be opened. Try again.";
/// Room sentences the matchmaking server already words for players.
const PASS_THROUGH: [&str; 2] = ["You are already in a lobby.", "You are already in a match."];

/// `{ kind, message }` like every League command, plus `required` for a version refusal (contracts file, rulings).
#[derive(Debug, Clone, serde::Serialize)]
pub struct LeagueCommandError {
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<String>,
}

fn err(kind: &'static str, message: &str) -> LeagueCommandError {
    LeagueCommandError { kind, message: message.to_string(), required: None }
}

fn update_required(required: String) -> LeagueCommandError {
    LeagueCommandError {
        kind: "league_update_required",
        message: reborn_core::matchmaking::LEAGUE_UPDATE_SENTENCE.to_string(),
        required: Some(required),
    }
}

pub(crate) fn league_error(e: LeagueError) -> LeagueCommandError {
    match e {
        LeagueError::NotMember => err("not_member", NOT_A_MEMBER),
        LeagueError::RateLimited => err("rate_limited", BUSY),
        LeagueError::SignedOut => err("unavailable", SIGN_IN_FIRST),
        LeagueError::UnknownVersion => err("unavailable", NOT_PUBLISHED),
        LeagueError::Unavailable | LeagueError::Unreachable | LeagueError::BadReply => err("unavailable", UNAVAILABLE),
    }
}

pub(crate) fn room_view(open: RoomOpen) -> Result<serde_json::Value, LeagueCommandError> {
    match open {
        RoomOpen::Created(room) => Ok(serde_json::json!({ "action": "created", "room": room })),
        RoomOpen::Join(code) => Ok(serde_json::json!({ "action": "join", "code": code })),
        RoomOpen::InSeries(match_id) => Ok(serde_json::json!({ "action": "in_series", "match_id": match_id })),
        RoomOpen::UpdateRequired(required) => Err(update_required(required)),
        RoomOpen::Refused(why) => Err(err(
            if why == "not_member" { "not_member" } else { "unavailable" },
            match why.as_str() {
                "not_member" => NOT_A_MEMBER,
                "no_game" => NO_GAME_NOW,
                "already_final" => ALREADY_FINAL,
                "opponent_unlinked" => OPPONENT_UNLINKED,
                s if PASS_THROUGH.contains(&s) => s,
                _ => ROOM_FAILED,
            },
        )),
    }
}

pub(crate) fn friendly_ruleset(ruleset: &str) -> Result<&'static str, LeagueCommandError> {
    match ruleset.trim() {
        "league" => Ok("league"),
        "raw" => Ok("raw"),
        _ => Err(err("unavailable", ROOM_FAILED)),
    }
}

/// `installed` needs both the pointer and a `league.json` this launcher accepts, so a damaged or foreign install shows
/// Install (which repairs it), never Play.
pub(crate) fn status_view(installed: Option<&str>, latest: Option<&str>, size_bytes: u64, info_ok: bool) -> serde_json::Value {
    let is_installed = installed.is_some() && info_ok;
    serde_json::json!({
        "installed": is_installed,
        "version": installed,
        "latest": latest,
        "update_available": is_installed && latest.is_some_and(|l| Some(l) != installed),
        "size_bytes": size_bytes,
        "game_id": reborn_core::league_edition::LEAGUE_GAME_ID,
    })
}

fn bearer(p: &Platform) -> Result<(String, String), LeagueCommandError> {
    let token = reborn_core::identity::token(p).ok().flatten().filter(|t| !t.trim().is_empty())
        .ok_or_else(|| err("unavailable", SIGN_IN_FIRST))?;
    let origin = reborn_core::identity::origin(p).map_err(|_| err("unavailable", UNAVAILABLE))?;
    Ok((token, origin))
}

/// Install state, and the newest version the signed release offers (network; absent offline).
#[tauri::command]
pub async fn league_edition_status() -> Result<serde_json::Value, LeagueCommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        let data = p.paths.data_root().map_err(|_| err("unavailable", UNAVAILABLE))?;
        let installed = installed_league_version();
        let info_ok = le::read_league_info(&data).is_ok();
        let (mut latest, mut size) = (None, 0u64);
        if let (Ok(settings), Ok(verifier)) = (crate::content_updates::load_settings(&p), crate::content_updates::verifier()) {
            let origin = Origin::new(&crate::content_updates::origin_url(&settings));
            let updater = Updater::new(&p, &data, &origin, &verifier);
            if let Ok(check) = updater.check(&CheckRequest { components: &COMPONENTS, depth: Depth::Quick }) {
                if let Some(game) = check.component(ComponentId::LeagueGame) {
                    latest = match &game.state {
                        ComponentState::UpToDate { version, .. } => Some(version.clone()),
                        ComponentState::Available(plan) => Some(plan.version().to_string()),
                        _ => None,
                    };
                }
                size = check.download_bytes();
            }
        }
        Ok(status_view(installed.as_deref(), latest.as_deref(), size, info_ok))
    })
    .await
    .map_err(|_| err("unavailable", UNAVAILABLE))?
}

/// Download (members-only links), verify, promote, then compose the league texture tree. Resumes a previous attempt.
#[tauri::command]
pub async fn league_edition_install(app: tauri::AppHandle) -> Result<serde_json::Value, LeagueCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        if !is_member(&p) {
            return Err(err("not_member", NOT_A_MEMBER));
        }
        let (token, api) = bearer(&p)?;
        let data = p.paths.data_root().map_err(|_| err("unavailable", UNAVAILABLE))?;
        let settings = crate::content_updates::load_settings(&p).map_err(|_| err("unavailable", UNAVAILABLE))?;
        let verifier = crate::content_updates::verifier().map_err(|_| err("unavailable", UNAVAILABLE))?;
        let origin = Origin::new(&crate::content_updates::origin_url(&settings));
        let say = |phase: &str, done: u64, total: u64| {
            let _ = app.emit("league-install-progress", serde_json::json!({ "phase": phase, "done": done, "total": total }));
        };

        // 1. What the signed release offers, from the public origin.
        let probe = Updater::new(&p, &data, &origin, &verifier);
        let check = probe.check(&CheckRequest { components: &COMPONENTS, depth: Depth::Verify })
            .map_err(|_| err("unavailable", UNAVAILABLE))?;
        if !check.available().any(|_| true) && le::read_league_info(&data).is_ok() {
            let version = installed_league_version().unwrap_or_default();
            return Ok(serde_json::json!({ "version": version }));
        }
        if check.component(ComponentId::LeagueGame).map(|c| matches!(c.state, ComponentState::NotCarried)).unwrap_or(true) {
            return Err(err("unavailable", NOT_PUBLISHED));
        }

        // 2. One signed link per blob, from the backend (members only), for each component that needs fetching.
        let http = crate::http_client();
        let links = |check: &reborn_core::content::pipeline::ContentCheck| -> Result<Vec<le::GatedFile>, LeagueCommandError> {
            let mut all = Vec::new();
            for (cc, plan) in check.available() {
                all.extend(le::request_download(&http, &api, &token, cc.component.as_str(), plan.version()).map_err(league_error)?);
            }
            Ok(all)
        };
        let total = check.download_bytes();
        let done = Arc::new(AtomicU64::new(0));
        let tick = {
            let done = done.clone();
            let app = app.clone();
            Arc::new(move |n: u64| {
                let now = done.fetch_add(n, Ordering::Relaxed) + n;
                let _ = app.emit("league-install-progress", serde_json::json!({ "phase": "download", "done": now, "total": total }));
            }) as Arc<dyn Fn(u64) + Send + Sync>
        };
        let mut source = LeagueSource::new(origin.clone(), &links(&check)?).with_progress(tick);

        // 3. Fetch; links last 6 h, so an incomplete fetch asks for fresh links once and resumes (.part files kept).
        let mut attempt = 0;
        let (updater_check, fetched) = loop {
            let updater = Updater::new(&p, &data, &source, &verifier);
            let check = updater.check(&CheckRequest { components: &COMPONENTS, depth: Depth::Verify })
                .map_err(|_| err("unavailable", UNAVAILABLE))?;
            let outcome = updater.fetch(&check).map_err(|_| err("unavailable", INSTALL_FAILED))?;
            if outcome.all_staged() || attempt == 1 {
                break (check, outcome);
            }
            attempt += 1;
            source.refresh(&links(&check)?);
        };
        if !fetched.all_staged() {
            return Err(err("unavailable", INSTALL_FAILED));
        }

        // 4. Verify and swap in (the pipeline hashes everything before the swap).
        say("verify", 0, 1);
        let updater = Updater::new(&p, &data, &source, &verifier);
        let promoted = updater
            .promote(&updater_check, &crate::content_updates::image_hook(&p, &settings))
            .map_err(|_| err("unavailable", INSTALL_FAILED))?;
        if !promoted.all_applied() {
            return Err(err("unavailable", INSTALL_FAILED));
        }
        say("verify", 1, 1);

        // 5. The installed build must be the one this launcher boots, then compose its textures + the shared mods.
        match le::read_league_info(&data) {
            Ok(_) => {}
            Err(LeagueInfoError::WrongGameId(_)) => return Err(err("unavailable", WRONG_BUILD)),
            Err(_) => return Err(err("unavailable", DAMAGED)),
        }
        say("compose", 0, 1);
        if let Err(e) = crate::sync_active_judged_for(&p, None, Some(reborn_core::game_identity::ImageStanding::Reborn), reborn_core::runtime::GameBuild::League) {
            // Not fatal: every league launch composes again before boot.
            eprintln!("league: compose after install did not finish: {e}");
        }
        say("compose", 1, 1);
        Ok(serde_json::json!({ "version": installed_league_version().unwrap_or_default() }))
    })
    .await
    .map_err(|_| err("unavailable", INSTALL_FAILED))?
}

/// Boot the league disc, raw, offline. Resolves `null` once the game has run; a refusal is a sentence.
#[tauri::command]
pub async fn league_play_offline() -> Result<serde_json::Value, LeagueCommandError> {
    // The League page carries no graphics or window choices of its own: the ones the Play page saved are used
    // (mm_launch_prefs), else Balanced, fullscreen.
    let (runtime, game, preset) = crate::online_mm::launch_prefs();
    let preset = if preset.is_empty() { "balanced".to_string() } else { preset };
    let result = crate::launch(runtime, game, preset, true, None, false, Some(false), Some(true), Some("league".into()))
        .await
        .map_err(|message| LeagueCommandError { kind: "unavailable", message, required: None })?;
    if result.started { Ok(serde_json::Value::Null) } else { Err(err("unavailable", &result.message)) }
}

#[tauri::command]
pub async fn league_host_friendly(ruleset: String) -> Result<serde_json::Value, LeagueCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let ruleset = friendly_ruleset(&ruleset)?;
        ready_league_image(&p).map_err(|m| err(if m == NOT_A_MEMBER { "not_member" } else { "not_installed" }, &m))?;
        let (token, api) = bearer(&p)?;
        let version = installed_league_version();
        room_view(le::host_friendly(&crate::http_client(), &api, &token, ruleset, version.as_deref()).map_err(league_error)?)
    })
    .await
    .map_err(|_| err("unavailable", ROOM_FAILED))?
}

#[tauri::command]
pub async fn league_room_open() -> Result<serde_json::Value, LeagueCommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        ready_league_image(&p).map_err(|m| err(if m == NOT_A_MEMBER { "not_member" } else { "not_installed" }, &m))?;
        let (token, api) = bearer(&p)?;
        let version = installed_league_version();
        room_view(le::open_room(&crate::http_client(), &api, &token, version.as_deref()).map_err(league_error)?)
    })
    .await
    .map_err(|_| err("unavailable", ROOM_FAILED))?
}
```

Notes for the implementer:
- **`launch_prefs`:** `crate::online_mm::launch_prefs()` does not exist yet. Add it next to `mm_launch_prefs`:

  ```rust
  pub(crate) fn launch_prefs() -> (Option<String>, Option<String>, String) {
      lock(&LAUNCH).clone()
  }
  ```

- **`launch` signature:** `crate::launch` is the `#[tauri::command] async fn launch` in `main.rs`, and its argument order must be exactly what that function declares after this task's change. Check `LaunchResult`'s field names (`started`, `message`) against the struct, and adapt the two lines if they differ.
- **`check.download_bytes()`, `check.component(..)`, `check.available()` and `ComponentState::NotCarried`** are the pipeline's existing API (`content/pipeline.rs`). Use them as named.
- **`tauri::Emitter`** is the trait `setup_tools.rs` uses for `app.emit`. Import it the same way that file does.

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-ui 2>&1 | tail -8`, then `node Tools/check-page-call-args.mjs` and `node Tools/check-play-page.mjs`
Expected: all pass. The Play page still calls `launch` without `edition`.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/src
git commit -m "ui: league edition commands (status, install with members-only links, offline, friendly, league room)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: The Mods page says mods reach the league build too

**Files:**
- Modify: `crates/reborn-ui/ui/mods.html`
- Modify: `crates/reborn-ui/ui/mods.js`

**Interfaces:**
- Consumes: `league_edition_status` (Task 8).

- [ ] **Step 1: Write the failing check**

Read `Tools/check-mods-page.mjs` to see how it stubs commands and inspects the page. Add a case that does both of these:
- stubs `league_edition_status` to `{installed:true,...}` and asserts that `#league-mods-note` is visible with the sentence below;
- stubs it to `{installed:false,...}` and asserts the note is hidden.

Follow that checker's existing style exactly. If it cannot express this, put a `node --test` unit test beside `mods.js` for a pure helper `leagueNoteVisible(status)` instead, and say which in the report.

- [ ] **Step 2: Run it and watch it fail**

Run: `node Tools/check-mods-page.mjs`
Expected: the new case fails.

- [ ] **Step 3: Implement**

`mods.html`: directly under the page's main heading block, add:

```html
<p class="note" id="league-mods-note" hidden>Mods apply to the league build too. A mod made for Reborn may not match the league's teams or fields.</p>
```

`mods.js`: on load, after the page's first render, add:

```js
// One shared mod list (spec 2026-10-08 v2): a member with the league build installed is told mods reach it too.
quietCall('league_edition_status').then((s) => {
  const note = document.getElementById('league-mods-note');
  if (note) note.hidden = !(s && s.installed === true);
}).catch(() => {});
```

(Import `quietCall` from `./shell.js` if `mods.js` does not already. It is the same helper `online-mm.js` uses.)

- [ ] **Step 4: Run the checks**

Run: `node Tools/check-mods-page.mjs`, then `node Tools/check-theme.mjs`
Expected: both pass.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/mods.html crates/reborn-ui/ui/mods.js Tools/check-mods-page.mjs
git commit -m "mods: note that mods reach the league build too

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Manual check: offline boot with a stand-in league disc (no release)

Nothing is published or uploaded here. This proves the parts that unit tests cannot reach:
- the league image boots;
- textures are composed under the league ID;
- a retail mod appears in the league game;
- the image is untouched after the session.

- [ ] **Step 1: Build**

Run: `CARGO_BUILD_JOBS=1 cargo build -p reborn-ui`
Expected: clean build.

- [ ] **Step 2: Make a stand-in league build in a scratch data root**

Use a beta-sandbox data root (`Tools/beta-sandbox.ps1` prints how), never the real `%APPDATA%\NFL Street Reborn`. In PowerShell, with `$data` = that sandbox's data root and `$retail` = its booted retail image:

```powershell
$league = Join-Path $data 'library\league'
New-Item -ItemType Directory -Force "$league\game", "$league\textures\LGXXXX" | Out-Null
Copy-Item $retail "$league\game\league.iso"
# Stand-in disc ID: the first six bytes become the placeholder LEAGUE_GAME_ID.
$fs = [System.IO.File]::Open("$league\game\league.iso", 'Open', 'ReadWrite'); $fs.Write([Text.Encoding]::ASCII.GetBytes('LGXXXX'), 0, 6); $fs.Close()
# A few textures so the composed tree is non-empty.
Get-ChildItem (Join-Path $data 'library\textures\GN7E69') -File | Select-Object -First 20 | Copy-Item -Destination "$league\textures\LGXXXX"
'{"schema":"nsr.league-game.v1","game_id":"LGXXXX","iso":"game/league.iso","textures":"textures/LGXXXX"}' | Set-Content -Encoding ascii -NoNewline "$league\league.json"
(Get-FileHash "$league\game\league.iso").Hash
```

Record the hash. If Dolphin refuses the patched copy because it is an NKIT image (memory `project_reborn_netplay_nkit_warning_block.md`), convert the copy to a plain ISO with the bundled DolphinTool first (`DolphinTool convert -i <copy> -o <out.iso> -f iso`), patch that, and say so in the report.

- [ ] **Step 3: Sign in as a member and play offline**

1. Start the sandbox launcher, sign in with Adam's Discord, open League (3a) so `league.json` caches `member: true`, and turn on one retail mod (e.g. a ball) on the Mods page.
2. **If 3a's page is not merged yet:** from the devtools console run `await window.__TAURI__.core.invoke('league_play_offline')`.
3. Expected:
   - Dolphin boots the stand-in disc.
   - `<user>\Load\Textures\LGXXXX\composed.json` exists and lists the mod.
   - `<user>\GameSettings\LGXXXX.ini` holds the required settings (`MEM1Size`, `RAMOverrideEnable`, `CacheHiresTextures = False`).
   - The Mods page shows the league note.
4. Quit the game, then rerun `(Get-FileHash "$league\game\league.iso").Hash`.
   Expected: **identical** to Step 2 (Review Focus 1).

- [ ] **Step 4: Refusals**

1. In `league.json`, change `game_id` to `ZZZZ99`, then run `league_play_offline`.
   Expected: rejected with the `WRONG_BUILD` sentence. Restore the file.
2. Delete the sandbox's `config\league.json` (the membership cache), then run `league_play_offline`.
   Expected: rejected with `NOT_A_MEMBER`.

- [ ] **Step 5: Record**

Write the observations (hashes before and after, the composed file list head, the GameSettings excerpt, both refusal sentences, any NKIT conversion) into `docs/superpowers/plans/2026-10-08-league-tab-3b-manual-check.md` and commit it. Do not build an installer and do not publish anything.

```bash
git add docs/superpowers/plans/2026-10-08-league-tab-3b-manual-check.md
git commit -m "3b: manual offline check with a stand-in league disc

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Self-review

- **Spec §4 coverage:**

  | Spec item | Where |
  |---|---|
  | Components and members-only | Task 1 |
  | Download gate | Tasks 4 and 8 |
  | Game ID constant and release gate | Task 1 |
  | Install layout | Tasks 1 and 2 |
  | Controller textures and JSGME | Task 2 (prompt packs are mod packages, `controller.rs::prompt_pack`) |
  | Settings layers | Already per `req.build.game_id()` in `Session` (verified there); Task 10 checks the file |
  | Saves | Same memory card, new game ID; nothing to do |
  | Launch modes | Offline: Task 8. Friendly and league game: Tasks 5, 7, 8 |
  | Netplay version gate | Tasks 6, 7, 8 |
  | Install/update UX | Commands in Task 8; drawn by 3a |
  | Membership lost | `ready_league_image` + install gate (Tasks 7, 8) |
  | Old Task 3 (seed, `intent.league`/`fixture_id`, room card) | Tasks 6 and 7 |

- **Contracts:**
  - The command names, argument and result shapes, the event name and payload, and the `league_update_required` kind with `required` all match the contracts file.
  - Every room call carries `league_game_version` (Tasks 5, 6, 7).
  - The two commands never navigate.
  - Plan 4's `ComponentId`, `DataDir`, `members_only` and layout are copied identically. The only difference is the anchor (`GameFull` here, `GameImport` on the deck line).
- **Placeholders:** two places name a fallback for a helper whose exact signature could differ: the `lay_package` test helper and the `Platform` test constructor in Task 2. Both come with exact instructions. There is no TODO and no TBD.
- **Names across tasks:** `LEAGUE_GAME_ID`, `league_seed_path`, `ready_league_image`, `installed_league_version`, `sync_active_judged_for`, `seed_for_match`, `edition_of`, `current_match_league`, `join_error` and `LEAGUE_UPDATE_SENTENCE` are each defined once and used with the same signature.
