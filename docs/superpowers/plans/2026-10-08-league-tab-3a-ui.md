# League Tab, Plan 3a: Launcher League tab UI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a members-only League tab to the launcher. It shows the week scoreboard strip, your team's This Week card, standings, recent results, team leaders and roster. It also gets the League edition's Install/Update button and a play panel (Play offline, Host friendly, Play league game), all driven by Plan 3b's commands.

**Architecture:**
- **Core client.** A new `reborn-core::league_home` client calls `GET /v1/league/me` (Plan 2) with the player's bearer token and keeps the last good answer in `league.json`.
- **Commands.** Two Tauri commands in a new `reborn-ui/src/league_tab.rs` expose it: `league_member` and `league_home`.
- **Page.** `league.html` + `league.js` + the pure `league-model.js` + `pages/league.css`. It reuses the Stats v2 card components.
- **Nav.** `shell.js` injects the nav link only when the cached answer says "member".
- **What this plan consumes:** everything about the League edition (install, versions, offline boot, friendly and league rooms, seeds, upload intent) belongs to **Plan 3b**. This plan only calls 3b's commands and draws their state.
- **Joining a room** uses the existing private-room path (`mm_room_join` by code), then the existing Online page flow takes over.

**Tech Stack:** Rust (reborn-core, reborn-ui / Tauri 2), vanilla ES modules, `node --test`, the theme checkers under `Tools/`.

**Spec:** `docs/superpowers/specs/2026-10-08-league-tab-design.md` (v2): §3 Launcher, plus §4 "Install / update UX" and "Launch modes" for the UI side only.

**Binding contracts:** `docs/superpowers/plans/2026-10-08-league-contracts.md`. The data contract is Plan 1's final site contract (top of `2026-10-08-league-tab-1-site.md`) plus Plan 2's backend additions.

**Replaces:** `2026-10-08-league-tab-3-launcher.md` (deleted). Its Task 3 (seed from `ruleset`, `intent.league` + `intent.fixture_id`, the Online room card's league title) **moved to Plan 3b**, and its `league_room_open` command and `open_room` client also moved to 3b (contracts file lists `league_room_open` under 3b).

**Worktree / branch:** `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league`, branch `feat/league-tab` (base 3.0.35 @ d37500a).

**Order with 3b:** Tasks 1–4 here are independent of 3b. Task 5 (the page) calls 3b's commands, but its tests and checker run against stubs, so it can merge first. Task 6 (manual run) needs 3b merged and Plan 2 deployed.

## Global Constraints

- Build/test Rust with `CARGO_BUILD_JOBS=1` (parallel builds have run this PC out of memory).
- Every new or edited `.rs`/`.js`/`.html`/`.css` file is **LF**, not CRLF. Source-scanning tests break on CRLF. Check with `git diff --stat` + `file <path>` before each commit.
- The core module is **`league_home.rs`**, never `league.rs`: `reborn-core/src/lib.rs` has a test asserting the retired `league.rs` uploader and `pub mod league;` stay gone.
- CSP stays as is (`crates/reborn-ui/tauri.conf.json`). Images only through `assetUrl()` (`/live-assets/...png` on `https://api.nflstreetreborn.com`) and `avatarSrc()` (Discord CDN).
- Server text is drawn with `el(..., { text })` only, never `innerHTML` (DESIGN.md §10).
- Page CSS holds layout only: no colour literals, no weight above 700, no new durations, no glows (DESIGN.md §8, §10). Any new token goes in `theme.css` and DESIGN.md §1 in the same commit.
- No player ever sees a status code, URL or raw error: commands return `{ kind, message }` with fixed sentences (same pattern as `live_view.rs`).
- The launcher never sends a mode, seed, best-of or fixture for a league room; the server derives them.
- `winner_team_id` is the series **leader** mid-series (site semantics). It means "won" only when `status === "final"`.
- The Play and Online tabs stay retail-only: nothing in this plan changes them.
- All three play buttons (Play league game / Join, Play offline, Host friendly) are **disabled until the League edition is installed and current** (`installed && !update_available`).
- Host friendly's **League settings** toggle defaults **ON** (`ruleset: "league"`); off sends `ruleset: "raw"`.
- No release, version bump or publish in this plan.

## Contract this plan consumes

### `GET /v1/league/me` (bearer) — Plan 1 site contract + Plan 2 additions

```json
{ "ok": true, "member": false }
```
or the member payload: Plan 1's contract verbatim, with these fields this plan relies on:
- `schedule_published` (bool). When `false`, `matchup.status` is `"bye"` only because the league has no fixtures yet. The card says **"Schedule not published yet"**, not "No game this week".
- `matchup`: `{ fixture_id, week, home, status, forfeit, score, series, winner_team_id, when, opponent, room, match_id }`.
  - `status` ∈ `scheduled | in_progress | live | final | bye`. On a bye, `fixture_id`, `home`, `score`, `series`, `winner_team_id` and `opponent` are `null`, and `forfeit` is `false`.
  - `matchup.week` can differ from `league.week`: the site picks each team's **earliest unfinished** fixture, so a postponed game shows its own week. The card title then shows `Week N` from `matchup.week`.
  - `forfeit: true` with `status: "final"` → the card says "Final · Forfeit".
  - `opponent.discord_user_id` is a string or `null`; `opponent.owner_display` may be `""`.
  - Plan 2 adds `opponent.online` (bool), `match_id`, and `room`: `null` or `{ room_id, code, host_discord_user_id, members, expires_at, state: waiting|lobby|in_series }`. The opponent hosts iff `host_discord_user_id === opponent.discord_user_id`.
- `week_games[]` (with `forfeit`), `standings[]`, `recent[]` (with `forfeit`; `result` may be `null`), `leaders` + `leaders_unit: "TD"`, `roster[]`, `site_url`, `league.week_label`, team/opponent `color`.
- Plan 2 adds `edition_versions: { "league-game": "<latest>", "league-content": "<latest>" }`. This plan does not read it; 3b's `league_edition_status` does.
- Errors: 401 (signed out), 503 `league_unavailable` (outage, **never** `member:false`), 429.

### Plan 3b commands (consumed, NOT implemented here)

| Command | Args | Resolves | Rejects |
|---|---|---|---|
| `league_edition_status` | — | `{ installed, version, latest, update_available, size_bytes, game_id }` | `{ kind, message }` |
| `league_edition_install` | — | `{ version }`; emits `league-install-progress` `{ phase: "download"\|"verify"\|"compose", done, total }` while running | `{ kind, message }` |
| `league_play_offline` | — | `null` (Dolphin launched) | `{ kind, message }` |
| `league_host_friendly` | `{ ruleset: "league"\|"raw" }` | `{ action: "created", room }` | `{ kind, message }` |
| `league_room_open` | — | `{ action: "created", room }` \| `{ action: "join", code }` \| `{ action: "in_series", match_id }` | `{ kind, message }` |

A version mismatch from either the room open or the join (`409 league_version_mismatch`) must reach this page as a rejection with **`kind: "league_update_required"`** (3b maps it). The page then shows "Your league files are out of date. Update league files to play." and highlights the Update button. Joining is the existing `mm_room_join({ code })`. If its rejection carries the raw server code instead, the page's `isUpdateRequired(e)` also matches `message === "league_version_mismatch"` (see Task 3).

**Deviations from the spec (flag in review):**
1. The launcher never parses or stores its own Discord ID or `league_member` in `identity.json`. Membership and the last payload live in `league.json`, which is removed on sign-out. The host test compares the room host with the opponent's ID, which is in the payload.
2. The spec's "Rejoin if dropped" button is labelled "Open room" and opens the Online page, which owns rejoin and the relay.
3. Cancel is the existing room leave (`mm_room_action({ roomId, action: 'leave' })`), not a league route.

## Review Focus

1. **A non-member opens `league.html` directly** (a stale nav link from a cached member answer). Expected: redirected to `index.html` as soon as `league_home` says `member: false`, and the cache flips so the link goes. → Task 3 `pageMode` + Task 1 `a_non_member_answer_clears_the_cached_team`.
2. **Signed out, or the token expired (401) while the cache says member.** Expected: no stale team, the tab goes, the cache resets. → Task 1 `a_401_is_never_covered_by_the_cache`.
3. **League files missing or out of date while the opponent's room is open.** Expected: the Join button is visible but disabled, with "Update league files to play"; pressing Update and finishing re-enables it without a page reload. → Task 3 `playGate` tests + Task 5 install flow.
4. **The league site is down (503) for a member.** Expected: last answer drawn with the "League data unavailable" banner; the tab never disappears on an outage; the edition buttons still work (they don't depend on the site). → Task 1 `a_failure_falls_back_to_the_cached_member`, Task 3 `bannerLine`.
5. **Season with no schedule yet** (live today: Season 4 has no fixtures). Expected: "Schedule not published yet", no scoreboard strip, play panel still usable. → Task 3 `cardState` unscheduled test + `scoreCells` empty.

---

## File map

| File | Status | Responsibility |
|---|---|---|
| `crates/reborn-core/src/league_home.rs` | Create | HTTP client for `GET /v1/league/me`, `league.json` cache, `resolve()` fallback rule |
| `crates/reborn-core/src/lib.rs` | Modify | `pub mod league_home;` |
| `crates/reborn-core/src/live_view.rs` | Modify | `status_error` → `pub(crate)` (reused, not copied) |
| `crates/reborn-core/src/identity.rs` | Modify | `clear_token` also removes `league.json` |
| `crates/reborn-ui/src/league_tab.rs` | Create | Tauri commands `league_member`, `league_home` |
| `crates/reborn-ui/src/main.rs` | Modify | `mod league_tab;` + register the two commands |
| `crates/reborn-ui/ui/league-model.js` | Create | pure view model (card state, play gate, install button, scoreboard cells, rows) |
| `crates/reborn-ui/ui/league-model.test.mjs` | Create | node tests for the model |
| `Tools/fixtures/league-home.json` | Create | member payload fixture (tests + page checker) |
| `Tools/fixtures/league-edition-status.json` | Create | installed-and-current edition status fixture (page checker) |
| `crates/reborn-ui/ui/shell.js` | Modify | `addLeagueLink()` injection |
| `crates/reborn-ui/ui/league-nav.test.mjs` | Create | nav injection test with a stub DOM |
| `crates/reborn-ui/ui/league.html` | Create | page shell (nav includes League, current) |
| `crates/reborn-ui/ui/league.js` | Create | load/poll/draw; install/update; Play/Join/Cancel/Offline/Friendly actions |
| `crates/reborn-ui/ui/pages/league.css` | Create | layout |
| `crates/reborn-ui/ui/theme.css` | Modify | 2 tokens: `--league-cell-w`, `--mine-edge` |
| `crates/reborn-ui/ui/DESIGN.md` | Modify | §1 tokens, §5 sidebar, §8 page table, §12 test table |
| `Tools/check-theme.mjs`, `Tools/check-shell-pages.mjs` | Modify | register the page + stub 3b commands |

---

### Task 1: Core client `league_home` (fetch, cache, fallback)

**Files:**
- Create: `crates/reborn-core/src/league_home.rs`
- Modify: `crates/reborn-core/src/lib.rs` (after `pub mod live_view;`)
- Modify: `crates/reborn-core/src/live_view.rs` (`fn status_error` → `pub(crate) fn status_error`, around line 55)
- Modify: `crates/reborn-core/src/identity.rs` (`clear_token`, around line 349)

**Interfaces:**
- Produces:
  - `league_home::fetch_home(http: &dyn Http, origin: &str, token: &str) -> Result<Value, LiveError>`
  - `league_home::LeagueCache { member: bool, home: Option<Value>, fetched_at_ms: u64 }`
  - `league_home::cache_path(&Platform) -> platform::Result<PathBuf>`, `load_cache(&Path) -> LeagueCache`, `save_cache(&Path, &LeagueCache)`
  - `league_home::resolve(fetched: Result<Value, LiveError>, cache: &LeagueCache, now_ms: u64) -> (Result<Value, LiveError>, Option<LeagueCache>)`. The second element is the cache to write, or `None` to leave it.
- Plan 3b may add `open_room`/`RoomOpen` to this module or its own; this task does not.

Before Step 1, confirm the names used: `crate::net::fake::FakeHttp` (with `.reply(url, status, body)` and `last_bearer()`), `Http::get_text_as`, `HttpResponse::is_success()`, `LiveError::{Server, RateLimited, Unreachable, BadReply}`, `DataDir::Config`. `grep -n "pub fn reply\|fn last_bearer\|fn get_text_as\|enum LiveError\|DataDir::Config" crates/reborn-core/src/*.rs`. Adapt a name if it differs and note it in the report.

- [ ] **Step 1: Write the failing tests** (create `league_home.rs` with only the test module and `use super::*;`, so it fails to compile)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;

    const ORIGIN: &str = "https://api.test";
    const MEMBER: &str = r#"{"member":true,"team":{"id":485,"name":"Atlanta Falcons"},"matchup":null}"#;

    #[test]
    fn fetch_sends_the_bearer_and_returns_the_body() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/league/me"), 200, MEMBER);
        let v = fetch_home(&http, ORIGIN, "tok").unwrap();
        assert_eq!(v["team"]["id"], 485);
        assert_eq!(http.last_bearer().as_deref(), Some("tok"));
    }

    #[test]
    fn a_body_without_a_member_flag_is_a_bad_reply() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/league/me"), 200, r#"{"team":{}}"#);
        assert_eq!(fetch_home(&http, ORIGIN, "tok").unwrap_err(), LiveError::BadReply);
    }

    #[test]
    fn a_body_that_is_not_json_is_a_bad_reply() {
        // The league site can leak an HTML warning through the backend (display_errors); never trust a non-JSON 200.
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/league/me"), 200, "<br /><b>Warning</b>");
        assert_eq!(fetch_home(&http, ORIGIN, "tok").unwrap_err(), LiveError::BadReply);
    }

    #[test]
    fn statuses_map_like_every_other_client() {
        for (status, want) in [(401, LiveError::Server(401)), (429, LiveError::RateLimited), (503, LiveError::Server(503))] {
            let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/league/me"), status, "{}");
            assert_eq!(fetch_home(&http, ORIGIN, "tok").unwrap_err(), want, "{status}");
        }
        assert_eq!(fetch_home(&FakeHttp::default(), ORIGIN, "tok").unwrap_err(), LiveError::Unreachable);
    }

    #[test]
    fn a_fresh_answer_is_returned_unstale_and_cached() {
        let fresh: Value = serde_json::from_str(MEMBER).unwrap();
        let (out, next) = resolve(Ok(fresh), &LeagueCache::default(), 7);
        let out = out.unwrap();
        assert_eq!(out["stale"], false);
        let next = next.unwrap();
        assert!(next.member);
        assert_eq!(next.fetched_at_ms, 7);
        assert_eq!(next.home.unwrap()["team"]["id"], 485);
    }

    #[test]
    fn a_non_member_answer_clears_the_cached_team() {
        let cache = LeagueCache { member: true, home: Some(serde_json::json!({"member":true})), fetched_at_ms: 1 };
        let (out, next) = resolve(Ok(serde_json::json!({"member": false})), &cache, 9);
        assert_eq!(out.unwrap()["member"], false);
        assert_eq!(next.unwrap(), LeagueCache { member: false, home: None, fetched_at_ms: 9 });
    }

    #[test]
    fn a_failure_falls_back_to_the_cached_member() {
        let cache = LeagueCache { member: true, home: Some(serde_json::from_str(MEMBER).unwrap()), fetched_at_ms: 1 };
        let (out, next) = resolve(Err(LiveError::Server(503)), &cache, 9);
        let out = out.unwrap();
        assert_eq!(out["stale"], true);
        assert_eq!(out["fetched_at_ms"], 1);
        assert!(next.is_none(), "a failure never rewrites the cache");
    }

    #[test]
    fn a_failure_with_no_cached_member_is_the_error() {
        let (out, next) = resolve(Err(LiveError::Unreachable), &LeagueCache::default(), 9);
        assert_eq!(out.unwrap_err(), LiveError::Unreachable);
        assert!(next.is_none());
    }

    #[test]
    fn a_401_is_never_covered_by_the_cache() {
        let cache = LeagueCache { member: true, home: Some(serde_json::from_str(MEMBER).unwrap()), fetched_at_ms: 1 };
        let (out, next) = resolve(Err(LiveError::Server(401)), &cache, 9);
        assert_eq!(out.unwrap_err(), LiveError::Server(401));
        assert_eq!(next.unwrap(), LeagueCache::default(), "the session is over: forget the tab");
    }

    #[test]
    fn the_cache_round_trips_and_a_bad_file_is_not_a_member() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("league.json");
        assert_eq!(load_cache(&path), LeagueCache::default());
        let c = LeagueCache { member: true, home: Some(serde_json::json!({"member":true})), fetched_at_ms: 3 };
        save_cache(&path, &c);
        assert_eq!(load_cache(&path), c);
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(load_cache(&path), LeagueCache::default());
    }
}
```

Add `pub mod league_home;` to `lib.rs` right after `pub mod live_view;`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core league_home`
Expected: compile errors (`fetch_home`, `resolve`, `LeagueCache`, ... not found).

- [ ] **Step 3: Write the implementation** (top of `league_home.rs`, above the tests)

```rust
//! The League tab's data client: `GET /v1/league/me` (the signed-in member's team home, or `{ member: false }`).
//! Values pass through as [`serde_json::Value`] for the same reason as `live_view`: the backend can add a field without
//! a launcher release. Errors are [`LiveError`], so nothing a screen shows can carry a URL, a body or a token.
//! The League edition (install, rooms, seeds) is Plan 3b's, not this module's.
//!
//! Named `league_home`, not `league`: `lib.rs` keeps a test that the retired `league.rs` uploader stays gone.
//!
//! The last good answer is kept in `league.json` beside `identity.json`. The nav reads it without the network, so
//! the League link is there on the first frame, and a member whose league server is down still sees their team,
//! marked `stale`. Signing out removes it (`identity::clear_token`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::live_view::{status_error, LiveError};
use crate::net::Http;
use crate::platform::{DataDir, Platform};

const CACHE_FILE: &str = "league.json";

/// What `league.json` holds. The default is "not a member", the answer for no file, an unreadable one, and signed out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LeagueCache {
    pub member: bool,
    #[serde(default)]
    pub home: Option<Value>,
    #[serde(default)]
    pub fetched_at_ms: u64,
}

pub fn cache_path(platform: &Platform) -> crate::platform::Result<PathBuf> {
    Ok(platform.data_dir(DataDir::Config)?.join(CACHE_FILE))
}

pub fn load_cache(path: &Path) -> LeagueCache {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Best effort: a cache that cannot be written only costs the next first frame its League link.
pub fn save_cache(path: &Path, cache: &LeagueCache) {
    let Ok(bytes) = serde_json::to_vec(cache) else { return };
    if let Err(e) = std::fs::write(path, bytes) {
        tracing::warn!("league: the cache could not be written at {}: {e}", path.display());
    }
}

fn url(origin: &str, path: &str) -> String {
    format!("{}{}", origin.trim_end_matches('/'), path)
}

/// `GET {origin}/v1/league/me`. A reply that is not JSON, or has no boolean `member`, is not this endpoint's answer.
pub fn fetch_home(http: &dyn Http, origin: &str, token: &str) -> Result<Value, LiveError> {
    let reply = http.get_text_as(&url(origin, "/v1/league/me"), Some(token)).map_err(|_| LiveError::Unreachable)?;
    if !reply.is_success() {
        return Err(status_error(reply.status));
    }
    let v: Value = serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)?;
    if !v.get("member").is_some_and(Value::is_boolean) {
        return Err(LiveError::BadReply);
    }
    Ok(v)
}

/// The page's answer and the cache to write (`None`: leave it). A fresh answer is returned with `stale: false` and
/// cached. A failure falls back to a cached member's last answer, `stale: true`. A 401 never does: the session is
/// over, so the cache resets and the tab goes.
pub fn resolve(fetched: Result<Value, LiveError>, cache: &LeagueCache, now_ms: u64) -> (Result<Value, LiveError>, Option<LeagueCache>) {
    match fetched {
        Ok(mut v) => {
            let member = v["member"].as_bool().unwrap_or(false);
            let next = LeagueCache { member, home: member.then(|| v.clone()), fetched_at_ms: now_ms };
            v["stale"] = Value::Bool(false);
            (Ok(v), Some(next))
        }
        Err(LiveError::Server(401)) => (Err(LiveError::Server(401)), Some(LeagueCache::default())),
        Err(e) => match (&cache.home, cache.member) {
            (Some(home), true) => {
                let mut v = home.clone();
                v["stale"] = Value::Bool(true);
                v["fetched_at_ms"] = Value::from(cache.fetched_at_ms);
                (Ok(v), None)
            }
            _ => (Err(e), None),
        },
    }
}
```

In `live_view.rs`, change `fn status_error(` to `pub(crate) fn status_error(`.

In `identity.rs` `clear_token`, insert immediately before `let path = identity_path(platform).map_err(...)`:

```rust
    // The League tab's last answer belongs to this sign-in: the next player on this PC must not see the tab.
    if let Ok(league) = crate::league_home::cache_path(platform) {
        let _ = std::fs::remove_file(league);
    }
```

Add this test to `identity.rs`'s test module (a source scan, like the repo's other wiring tests):

```rust
    #[test]
    fn signing_out_forgets_the_league_tab() {
        let src = include_str!("identity.rs");
        let body = &src[src.find("pub fn clear_token").unwrap()..src.find("pub fn sign_out").unwrap()];
        assert!(body.contains("league_home::cache_path"), "clear_token no longer removes league.json");
    }
```

(If `pub fn sign_out` does not follow `clear_token`, slice to the next `pub fn` after `clear_token` instead.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core league_home signing_out_forgets_the_league_tab the_retired_league_uploader`
Expected: all PASS (the last filter proves the module name did not trip the retired-uploader guard; if no test matches that name, find the guard with `grep -n "league.rs" crates/reborn-core/src/lib.rs` and run it by its name).

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/league_home.rs crates/reborn-core/src/lib.rs crates/reborn-core/src/live_view.rs crates/reborn-core/src/identity.rs
git commit -m "core: league_home client (/v1/league/me + league.json cache)"
```

---

### Task 2: Tauri commands `league_member`, `league_home`

**Files:**
- Create: `crates/reborn-ui/src/league_tab.rs`
- Modify: `crates/reborn-ui/src/main.rs`: `mod league_tab;` beside `mod live_view;`, and the two commands in `tauri::generate_handler![...]` after `live_view::stats_dashboard,`

**Interfaces:**
- Consumes: Task 1's `league_home::{fetch_home, resolve, load_cache, save_cache, cache_path, LeagueCache}`, the existing `identity::{status, token, origin}`, `crate::http_client()`, `crate::live_view::LiveCommandError`.
- Produces (JS `call()` names → result):
  - `league_member` → `bool`. Cache only, no network.
  - `league_home` → `{ signed_in: false }` | `{ signed_in: true, ok: true, member: false }` | `{ signed_in: true, member: true, stale: bool, ...payload }`. Rejects `{ kind, message }`.
- 3b registers its own commands in its own module; this task registers only these two. If 3b has merged first and the `generate_handler!` list already has `league_edition::...` lines, add these two beside them.

- [ ] **Step 1: Write the failing tests** (test module at the bottom of the new `league_tab.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_home_failure_reads_as_a_league_sentence() {
        let cases = [
            (LiveError::RateLimited, "rate_limited", BUSY),
            (LiveError::Server(503), "unavailable", NOT_LOADED),
            (LiveError::Unreachable, "unavailable", NOT_LOADED),
            (LiveError::BadReply, "unavailable", NOT_LOADED),
        ];
        for (e, kind, message) in cases {
            let v = serde_json::to_value(home_error(e)).unwrap();
            assert_eq!((v["kind"].as_str().unwrap(), v["message"].as_str().unwrap()), (kind, message), "{e:?}");
        }
    }

    #[test]
    fn the_home_shape_marks_signed_in() {
        assert_eq!(signed_out(), serde_json::json!({ "signed_in": false }));
        let mut v = serde_json::json!({ "member": true });
        mark_signed_in(&mut v);
        assert_eq!(v["signed_in"], true);
    }

    /// The same rule live_view.rs holds itself to: no command hands a page an error's Display.
    #[test]
    fn no_command_here_hands_a_page_raw_error_text() {
        let src = include_str!("league_tab.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        let compact: String = production.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(!compact.contains("e.to_string()"));
    }
}
```

Add `mod league_tab;` to `main.rs` now, so the test target compiles the file.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-ui league_tab`
Expected: compile errors (`home_error`, `signed_out`, constants not found).

- [ ] **Step 3: Write the implementation** (top of `league_tab.rs`)

```rust
//! The League tab's data commands (spec 2026-10-08 §3): whether to draw the tab (`league_member`, from the cache only)
//! and the team home (`league_home`). The League edition's commands (install, offline, friendly, league room) are
//! Plan 3b's module; joining and cancelling a room are the existing `mm_room_join` / `mm_room_action`.
//!
//! Every failure is `{ kind, message }` (`LiveCommandError`), with `message` one of the fixed sentences below.

use reborn_core::identity;
use reborn_core::league_home::{self, LeagueCache};
use reborn_core::live_view::LiveError;
use reborn_core::platform::Platform;

use crate::live_view::LiveCommandError;

const NOT_LOADED: &str = "League data couldn't be loaded right now.";
const BUSY: &str = "The league server is busy. Trying again in a minute.";

fn err(kind: &'static str, message: &str) -> LiveCommandError {
    LiveCommandError { kind, message: message.to_string() }
}

fn home_error(e: LiveError) -> LiveCommandError {
    match e {
        LiveError::RateLimited => err("rate_limited", BUSY),
        _ => err("unavailable", NOT_LOADED),
    }
}

fn signed_out() -> serde_json::Value {
    serde_json::json!({ "signed_in": false })
}

fn mark_signed_in(v: &mut serde_json::Value) {
    v["signed_in"] = serde_json::Value::Bool(true);
}

/// The bearer, or `None` when signed out (no facts file, or no vault token).
fn bearer(p: &Platform) -> Option<String> {
    identity::status(p).ok().flatten()?;
    identity::token(p).ok().flatten().filter(|t| !t.trim().is_empty())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Whether the nav draws the League link: the cached answer only, no network, so every page's first frame has it.
#[tauri::command]
pub async fn league_member() -> bool {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        if bearer(&p).is_none() {
            return false;
        }
        league_home::cache_path(&p).map(|path| league_home::load_cache(&path).member).unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// `GET /v1/league/me`, with the cached fallback (`league_home::resolve`). Signed out is a shape, not an error. A 503
/// (`league_unavailable`) is an outage, never "not a member": `resolve` falls back to the cached member.
#[tauri::command]
pub async fn league_home() -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        let Some(token) = bearer(&p) else { return Ok(signed_out()) };
        let origin = identity::origin(&p).map_err(|_| err("unavailable", NOT_LOADED))?;
        let path = league_home::cache_path(&p).ok();
        let cache = path.as_deref().map(league_home::load_cache).unwrap_or_else(LeagueCache::default);
        let fetched = league_home::fetch_home(&crate::http_client(), &origin, &token);
        let (out, next) = league_home::resolve(fetched, &cache, now_ms());
        if let (Some(path), Some(next)) = (path.as_deref(), next) {
            league_home::save_cache(path, &next);
        }
        match out {
            Ok(mut v) => {
                mark_signed_in(&mut v);
                Ok(v)
            }
            Err(LiveError::Server(401)) => Ok(signed_out()),
            Err(e) => Err(home_error(e)),
        }
    })
    .await
    .map_err(|_| err("unavailable", NOT_LOADED))?
}
```

Before writing, confirm `identity::status`, `identity::token`, `identity::origin`, `crate::http_client()` and `LiveCommandError { kind: &'static str, message: String }` exist with these shapes (`grep -n "pub fn status\|pub fn token\|pub fn origin" crates/reborn-core/src/identity.rs`, `grep -n "fn http_client\|struct LiveCommandError" -A3 crates/reborn-ui/src/*.rs`). Adapt and note any difference.

Register in `main.rs`'s `generate_handler!` list, right after `live_view::stats_dashboard,`:

```rust
            league_tab::league_member,
            league_tab::league_home,
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-ui league_tab nothing_the_online_page_can_call`
Expected: PASS. The second filter is `main.rs`'s existing raw-error guard; it must stay green (if it has another name, find it with `grep -n "to_string()" crates/reborn-ui/src/main.rs | grep test` and run that).

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/src/league_tab.rs crates/reborn-ui/src/main.rs
git commit -m "ui: league_member / league_home commands"
```

---

### Task 3: Page model `league-model.js` + fixtures

**Files:**
- Create: `Tools/fixtures/league-home.json`
- Create: `Tools/fixtures/league-edition-status.json`
- Create: `crates/reborn-ui/ui/league-model.js`
- Test: `crates/reborn-ui/ui/league-model.test.mjs`

**Interfaces:**
- Consumes: the `league_home` answer (Task 2), 3b's `league_edition_status` answer and `league-install-progress` payload (shapes in "Contract this plan consumes").
- Produces (all pure, no DOM):
  - `pageMode(home) -> 'signed_out' | 'redirect' | 'member'`
  - `headerLine(home) -> string`, `bannerLine(home) -> string | null`, `ordinal(n) -> string`
  - `teamColor(team) -> string | null` (exact `#RRGGBB` or null)
  - `weekTitle(home) -> string` (the This Week card title)
  - `cardState(home) -> { kind: 'unscheduled'|'bye'|'final'|'live'|'waiting'|'lobby'|'join'|'play'|'unlinked', line, action, cancel, room }`
  - `installState(status, progress) -> { label, disabled, primary, percent } | null`
  - `playGate(status, progress) -> { ready: boolean, hint: string | null }`
  - `isUpdateRequired(err) -> boolean`
  - `friendlyRuleset(leagueSettingsOn: boolean) -> 'league' | 'raw'`
  - `scoreCells(home)`, `standingsRows(home)`, `recentRows(home)`, `leaderCols(home)`, `rosterRows(home)`

- [ ] **Step 1: Write the fixtures**

`Tools/fixtures/league-home.json` (the site contract's member example plus Plan 2's `room`, `online`, `match_id`, `edition_versions`):

```json
{
  "ok": true,
  "signed_in": true,
  "member": true,
  "stale": false,
  "schedule_published": true,
  "edition_versions": { "league-game": "2026.10.08", "league-content": "1.0.0" },
  "league": { "id": 36, "name": "Reborn Season 4", "season": 4, "week": 6, "week_label": "Week 6" },
  "team": { "id": 493, "name": "Subway Surfers", "abbr": "ADAM", "logo_url": "/live-assets/league/493.png", "color": "#00A3FF", "record": "4-1", "standing": 2 },
  "matchup": {
    "fixture_id": 1234, "week": 6, "home": true, "status": "scheduled", "forfeit": false,
    "score": null, "series": null, "winner_team_id": null, "when": null,
    "opponent": { "team_id": 486, "name": "Football Furies", "abbr": "BLONDE", "logo_url": null, "color": "#FF0000", "record": "3-2",
                  "owner_display": "BLONDE", "discord_user_id": "480510938571735040", "online": true },
    "room": null, "match_id": null
  },
  "week_games": [
    { "fixture_id": 1230,
      "home": { "team_id": 482, "name": "Gridiron Ghosts", "abbr": "GHO", "logo_url": null, "color": "#22AA44", "record": "5-0" },
      "away": { "team_id": 483, "name": "Jock Squad", "abbr": "JOCK", "logo_url": null, "color": "#AA2222", "record": "2-3" },
      "status": "final", "forfeit": false, "score": { "home": 49, "away": 21 }, "series": { "home": 2, "away": 0 }, "winner_team_id": 482, "when": null },
    { "fixture_id": 1231,
      "home": { "team_id": 484, "name": "Night Owls", "abbr": "OWL", "logo_url": null, "color": "#333333", "record": "2-3" },
      "away": { "team_id": 485, "name": "Sand Crabs", "abbr": "CRAB", "logo_url": null, "color": "#DDAA00", "record": "1-4" },
      "status": "in_progress", "forfeit": false, "score": { "home": 14, "away": 21 }, "series": { "home": 0, "away": 1 }, "winner_team_id": 485, "when": null },
    { "fixture_id": 1232,
      "home": { "team_id": 487, "name": "Rail Riders", "abbr": "RAIL", "logo_url": null, "color": "#5555FF", "record": "3-2" },
      "away": { "team_id": 488, "name": "Dune Kings", "abbr": "DUNE", "logo_url": null, "color": "#AA7700", "record": "2-3" },
      "status": "final", "forfeit": true, "score": null, "series": null, "winner_team_id": 487, "when": null },
    { "fixture_id": 1234,
      "home": { "team_id": 493, "name": "Subway Surfers", "abbr": "ADAM", "logo_url": "/live-assets/league/493.png", "color": "#00A3FF", "record": "4-1" },
      "away": { "team_id": 486, "name": "Football Furies", "abbr": "BLONDE", "logo_url": null, "color": "#FF0000", "record": "3-2" },
      "status": "scheduled", "forfeit": false, "score": null, "series": null, "winner_team_id": null, "when": null }
  ],
  "standings": [
    { "rank": 1, "team_id": 482, "name": "Gridiron Ghosts", "abbr": "GHO", "logo_url": null, "record": "5-0" },
    { "rank": 2, "team_id": 493, "name": "Subway Surfers", "abbr": "ADAM", "logo_url": null, "record": "4-1" },
    { "rank": 3, "team_id": 486, "name": "Football Furies", "abbr": "BLONDE", "logo_url": null, "record": "3-2" }
  ],
  "recent": [
    { "fixture_id": 1200, "week": 5, "opponent_abbr": "JOCK", "home": false, "result": "W", "score": "28-21", "series": "2-1", "forfeit": false },
    { "fixture_id": 1190, "week": 4, "opponent_abbr": "GHO", "home": true, "result": "L", "score": null, "series": "0-2", "forfeit": false },
    { "fixture_id": 1180, "week": 3, "opponent_abbr": "RAIL", "home": true, "result": null, "score": null, "series": null, "forfeit": true },
    { "fixture_id": 1170, "week": 2, "opponent_abbr": "DUNE", "home": false, "result": "W", "score": null, "series": null, "forfeit": true }
  ],
  "leaders": {
    "pass": [ { "poid": 12, "name": "Michael Vick", "value": 14 } ],
    "rush": [ { "poid": 20, "name": "Barry Sanders", "value": 1204 } ],
    "rec":  []
  },
  "leaders_unit": "TD",
  "roster": [ { "poid": 12, "name": "Michael Vick", "position": "QB", "number": 7 }, { "poid": 84, "name": "Randy Moss", "position": "WR", "number": 84 } ],
  "site_url": "https://nflstreetreborn.com/league/team.php?league_id=36&team_id=493"
}
```

`Tools/fixtures/league-edition-status.json`:

```json
{ "installed": true, "version": "2026.10.08", "latest": "2026.10.08", "update_available": false, "size_bytes": 1610612736, "game_id": "LGXXXX" }
```

- [ ] **Step 2: Write the failing tests** `crates/reborn-ui/ui/league-model.test.mjs`

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  pageMode, headerLine, bannerLine, teamColor, weekTitle, cardState, installState, playGate, isUpdateRequired, friendlyRuleset,
  scoreCells, standingsRows, recentRows, leaderCols, rosterRows, ordinal,
} from './league-model.js';

const FIX = JSON.parse(readFileSync(new URL('../../../Tools/fixtures/league-home.json', import.meta.url), 'utf8'));
const ED = JSON.parse(readFileSync(new URL('../../../Tools/fixtures/league-edition-status.json', import.meta.url), 'utf8'));
const clone = () => structuredClone(FIX);
const withMatchup = (patch) => { const h = clone(); h.matchup = { ...h.matchup, ...patch }; return h; };
const OPP = '480510938571735040';
const ROOM = { room_id: 'r1', code: 'K7QP2M', members: 1, expires_at: null, state: 'waiting' };
const MINE = { ...ROOM, host_discord_user_id: '111' };      // I host (the host is not the opponent)
const THEIRS = { ...ROOM, host_discord_user_id: OPP };     // the opponent hosts

test('pageMode: signed out, non-member redirects, member', () => {
  assert.equal(pageMode({ signed_in: false }), 'signed_out');
  assert.equal(pageMode({ ok: true, signed_in: true, member: false }), 'redirect');
  assert.equal(pageMode(null), 'redirect');
  assert.equal(pageMode(FIX), 'member');
});

test('ordinal, header line (week_label wins), banner only when stale', () => {
  assert.deepEqual([1, 2, 3, 4, 11, 12, 13, 21, 22].map(ordinal), ['1st', '2nd', '3rd', '4th', '11th', '12th', '13th', '21st', '22nd']);
  assert.equal(headerLine(FIX), 'Season 4 · Week 6 · 4-1 · 2nd');
  const playoff = clone(); playoff.league.week_label = 'Wild Card'; playoff.team.standing = null; playoff.league.season = null;
  assert.equal(headerLine(playoff), 'Wild Card · 4-1');
  assert.equal(bannerLine(FIX), null);
  assert.equal(bannerLine({ ...FIX, stale: true }), 'League data unavailable. Showing what was last loaded.');
});

test('teamColor: only #RRGGBB passes', () => {
  assert.equal(teamColor(FIX.team), '#00A3FF');
  for (const bad of ['red', '#FFF', 'url(x)', '#00A3FF;x:y', null, 7]) assert.equal(teamColor({ color: bad }), null, String(bad));
});

test('weekTitle: the league week, or the matchup week when a postponed game is still open', () => {
  assert.equal(weekTitle(FIX), 'This week · Week 6');
  const late = withMatchup({ week: 5 });
  assert.equal(weekTitle(late), 'Catch-up game · Week 5');
  const noLabel = clone(); noLabel.league.week_label = ''; assert.equal(weekTitle(noLabel), 'This week · Week 6');
});

test('cardState: play, waiting, join, lobby, final, bye', () => {
  const p = cardState(FIX);
  assert.deepEqual([p.kind, p.action, p.line], ['play', 'Play league game', '@BLONDE is online']);

  const waiting = cardState(withMatchup({ room: MINE }));
  assert.deepEqual([waiting.kind, waiting.line, waiting.action, waiting.cancel], ['waiting', 'Waiting for @BLONDE…', 'Open room', true]);

  const join = cardState(withMatchup({ room: THEIRS }));
  assert.deepEqual([join.kind, join.action, join.cancel, join.room.code], ['join', "Join @BLONDE's game", false, 'K7QP2M']);

  const hostLobby = cardState(withMatchup({ room: { ...MINE, state: 'lobby', members: 2 } }));
  assert.deepEqual([hostLobby.kind, hostLobby.line, hostLobby.action, hostLobby.cancel], ['lobby', '@BLONDE joined', 'Open room', true]);
  const guestLobby = cardState(withMatchup({ room: { ...THEIRS, state: 'lobby', members: 2 } }));
  assert.deepEqual([guestLobby.kind, guestLobby.line, guestLobby.action, guestLobby.cancel], ['lobby', "You're in @BLONDE's room", 'Open room', false]);

  const won = cardState(withMatchup({ status: 'final', series: { home: 2, away: 1 }, winner_team_id: 493 }));
  assert.deepEqual([won.kind, won.line, won.action], ['final', 'Final · Won the series 2-1', null]);
  const lost = cardState(withMatchup({ status: 'final', home: false, series: { home: 2, away: 0 }, winner_team_id: 486 }));
  assert.equal(lost.line, 'Final · Lost the series 0-2');
  assert.equal(cardState(withMatchup({ status: 'final', series: null, winner_team_id: null })).line, 'Final');

  assert.deepEqual([cardState(withMatchup({ status: 'bye', opponent: null })).kind, cardState(withMatchup({ status: 'bye' })).line], ['bye', 'No game this week']);
  const none = clone(); none.matchup = null;
  assert.deepEqual([cardState(none).kind, cardState(none).action], ['bye', null]);
});

test('cardState: forfeits read as forfeits', () => {
  const ffWon = cardState(withMatchup({ status: 'final', forfeit: true, series: null, score: null, winner_team_id: 493 }));
  assert.deepEqual([ffWon.kind, ffWon.line, ffWon.action], ['final', 'Final · Won by forfeit', null]);
  const ffLost = cardState(withMatchup({ status: 'final', forfeit: true, series: null, winner_team_id: 486 }));
  assert.equal(ffLost.line, 'Final · Lost by forfeit');
  assert.equal(cardState(withMatchup({ status: 'final', forfeit: true, series: null, winner_team_id: null })).line, 'Final · Forfeit');
});

test('cardState: no schedule yet is not a bye', () => {
  const h = clone(); h.schedule_published = false; h.matchup = { ...h.matchup, status: 'bye', fixture_id: null, opponent: null, home: null };
  const s = cardState(h);
  assert.deepEqual([s.kind, s.line, s.action], ['unscheduled', 'Schedule not published yet', null]);
  const old = clone(); delete old.schedule_published; old.matchup = { ...old.matchup, status: 'bye', opponent: null };
  assert.equal(cardState(old).kind, 'bye', 'an older backend without the flag still reads as a bye');
});

test('cardState: in_progress keeps the series score and still plays game 2', () => {
  const s = cardState(withMatchup({ status: 'in_progress', series: { home: 1, away: 0 }, winner_team_id: 493 }));
  assert.deepEqual([s.kind, s.line, s.action], ['play', 'Series · You lead 1-0', 'Play league game'], 'winner_team_id mid-series is only the leader');
  assert.equal(cardState(withMatchup({ status: 'in_progress', home: false, series: { home: 1, away: 0 } })).line, 'Series · You trail 0-1');
  const j = cardState(withMatchup({ status: 'in_progress', series: { home: 1, away: 1 }, room: THEIRS }));
  assert.deepEqual([j.kind, j.line], ['join', 'Series · Tied 1-1 · @BLONDE is waiting']);
});

test('cardState: live (backend merge) or a room already in its series', () => {
  const l = cardState(withMatchup({ status: 'live', series: { home: 1, away: 0 } }));
  assert.deepEqual([l.kind, l.line, l.action], ['live', 'LIVE · You lead 1-0', 'Open room']);
  assert.equal(cardState(withMatchup({ room: { room_id: null, code: null, host_discord_user_id: OPP, members: 2, expires_at: null, state: 'in_series' } })).kind, 'live');
  assert.equal(cardState(withMatchup({ status: 'live', series: null })).line, 'LIVE');
});

test('cardState: an opponent without Discord gets no button', () => {
  const h = clone(); h.matchup.opponent.discord_user_id = null;
  const s = cardState(h);
  assert.deepEqual([s.kind, s.line, s.action], ['unlinked', "Opponent hasn't linked Discord", null]);
});

test('cardState: a room with no code is no room; names fall back', () => {
  assert.equal(cardState(withMatchup({ room: { room_id: 'r1', host_discord_user_id: OPP, state: 'waiting' } })).kind, 'play');
  const h = withMatchup({ room: THEIRS });
  h.matchup.opponent.owner_display = '';
  assert.equal(cardState(h).action, "Join @Football Furies's game");
});

test('installState: checking, install, update, progress, current', () => {
  assert.deepEqual(installState(null, null), { label: 'Checking league files…', disabled: true, primary: false, percent: null });
  assert.deepEqual(installState({ ...ED, installed: false, version: null }, null), { label: 'Install league files (1.5 GB)', disabled: false, primary: true, percent: null });
  assert.deepEqual(installState({ ...ED, latest: '2026.10.09', update_available: true }, null), { label: 'Update league files (1.5 GB)', disabled: false, primary: true, percent: null });
  assert.deepEqual(installState(ED, { phase: 'download', done: 512, total: 2048 }), { label: 'Downloading league files… 25%', disabled: true, primary: false, percent: 25 });
  assert.deepEqual(installState(ED, { phase: 'verify', done: 0, total: 0 }), { label: 'Checking downloaded files…', disabled: true, primary: false, percent: null });
  assert.deepEqual(installState(ED, { phase: 'compose', done: 3, total: 4 }), { label: 'Setting up textures… 75%', disabled: true, primary: false, percent: 75 });
  assert.equal(installState(ED, null), null, 'installed and current: no button');
  assert.equal(installState({ ...ED, size_bytes: 0, installed: false }, null).label, 'Install league files');
});

test('playGate: buttons only when installed, current and idle', () => {
  assert.deepEqual(playGate(ED, null), { ready: true, hint: null });
  assert.deepEqual(playGate(null, null), { ready: false, hint: 'Checking league files…' });
  assert.deepEqual(playGate({ ...ED, installed: false }, null), { ready: false, hint: 'Install league files to play.' });
  assert.deepEqual(playGate({ ...ED, update_available: true }, null), { ready: false, hint: 'Update league files to play.' });
  assert.deepEqual(playGate(ED, { phase: 'download', done: 1, total: 2 }), { ready: false, hint: 'League files are installing…' });
});

test('isUpdateRequired and friendlyRuleset', () => {
  assert.equal(isUpdateRequired({ kind: 'league_update_required', message: 'x' }), true);
  assert.equal(isUpdateRequired({ kind: 'unavailable', message: 'league_version_mismatch' }), true, 'a raw code from the existing join path');
  assert.equal(isUpdateRequired('league_version_mismatch'), true);
  assert.equal(isUpdateRequired({ kind: 'unavailable', message: 'nope' }), false);
  assert.equal(isUpdateRequired(null), false);
  assert.equal(friendlyRuleset(true), 'league');
  assert.equal(friendlyRuleset(false), 'raw');
});

test('scoreCells: mine first, never a time, series and forfeit lines, winners only when final', () => {
  const cells = scoreCells(FIX);
  assert.deepEqual(cells.map((c) => c.key), [1234, 1230, 1231, 1232]);
  const [mine, fin, prog, ff] = cells;
  assert.deepEqual([mine.mine, mine.status, mine.statusText, mine.home.score], [true, 'scheduled', 'This week', null]);
  assert.deepEqual([fin.status, fin.statusText, fin.home.score, fin.away.score, fin.home.winner, fin.away.winner], ['final', 'FINAL · 0-2', 49, 21, true, false]);
  assert.deepEqual([prog.status, prog.statusText, prog.home.winner, prog.away.winner], ['in_progress', 'SERIES · 1-0', false, false], 'the leader is not a winner');
  assert.deepEqual([ff.statusText, ff.home.winner, ff.away.winner, ff.home.score], ['FORFEIT', true, false, null]);
  assert.equal(mine.home.color, '#00A3FF');
  assert.equal(fin.home.logo_url, null);
});

test('scoreCells: live text, junk rows dropped, no week_games is []', () => {
  const h = clone(); h.week_games[3].status = 'live'; h.week_games[3].series = { home: 1, away: 0 }; h.week_games.push(null, { fixture_id: 9 });
  const cells = scoreCells(h);
  assert.deepEqual([cells[0].status, cells[0].statusText], ['live', 'LIVE · 0-1']);
  assert.equal(cells.length, 4);
  assert.deepEqual(scoreCells({ member: true }), []);
});

test('rows: standings mark mine; recent with forfeits; TD leaders; roster', () => {
  assert.deepEqual(standingsRows(FIX).map((r) => [r.rank, r.abbrev, r.record, r.mine]), [[1, 'GHO', '5-0', false], [2, 'ADAM', '4-1', true], [3, 'BLONDE', '3-2', false]]);
  assert.deepEqual(recentRows(FIX), [
    { result: 'W', text: 'W 28-21 @ JOCK · Week 5' },
    { result: 'L', text: 'L 0-2 vs GHO · Week 4' },
    { result: null, text: 'Forfeit vs RAIL · Week 3' },
    { result: 'W', text: 'W by forfeit @ DUNE · Week 2' },
  ]);
  assert.deepEqual(leaderCols(FIX).map((c) => [c.label, c.rows[0]?.name ?? null, c.rows[0]?.value ?? null]),
    [['Passing TD', 'Michael Vick', '14 TD'], ['Rushing TD', 'Barry Sanders', '1,204 TD'], ['Receiving TD', null, null]]);
  const bare = clone(); bare.leaders_unit = undefined;
  assert.equal(leaderCols(bare)[0].rows[0].value, '14', 'no unit from the server: the bare number');
  assert.deepEqual(rosterRows(FIX)[0], { key: 12, number: '7', name: 'Michael Vick', position: 'QB' });
  assert.deepEqual([recentRows({}), leaderCols({}).map((c) => c.rows.length), rosterRows({})], [[], [0, 0, 0], []]);
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `node --test crates/reborn-ui/ui/league-model.test.mjs`
Expected: FAIL, `Cannot find module './league-model.js'`.

- [ ] **Step 4: Write the implementation** `crates/reborn-ui/ui/league-model.js`

```js
// The League tab's words and rows (spec 2026-10-08 v2 §3-4; data = Plan 1's site contract + Plan 2's room/live fields;
// edition state = Plan 3b's league_edition_status), pure: no DOM, no calls. league.js draws what this returns.
// The site stores no kickoff time (`when` is always null), so nothing here prints a time. Series and scores arrive as
// {home, away}; `matchup.home` turns them into you/opp. `winner_team_id` is the series LEADER until status is final.

const str = (v) => (typeof v === 'string' ? v : '');
const num = (v) => (Number.isFinite(v) ? v : null);
const arr = (v) => (Array.isArray(v) ? v : []);
const HEX = /^#[0-9A-Fa-f]{6}$/;

export function ordinal(n) {
  const v = n % 100;
  if (v >= 11 && v <= 13) return `${n}th`;
  return `${n}${({ 1: 'st', 2: 'nd', 3: 'rd' })[n % 10] || 'th'}`;
}

/** The only team colour that may reach a `--tc` property: an exact #RRGGBB. */
export function teamColor(team) {
  return typeof team?.color === 'string' && HEX.test(team.color) ? team.color : null;
}

export function pageMode(home) {
  if (home && home.signed_in === false) return 'signed_out';
  return home && home.member === true ? 'member' : 'redirect';
}

export function headerLine(home) {
  const parts = [];
  if (num(home?.league?.season) !== null) parts.push(`Season ${home.league.season}`);
  const week = str(home?.league?.week_label) || (num(home?.league?.week) !== null ? `Week ${home.league.week}` : '');
  if (week) parts.push(week);
  if (str(home?.team?.record)) parts.push(home.team.record);
  if (num(home?.team?.standing) !== null) parts.push(ordinal(home.team.standing));
  return parts.join(' · ');
}

export function bannerLine(home) {
  return home?.stale === true ? 'League data unavailable. Showing what was last loaded.' : null;
}

/** The This Week card's title. The site picks each team's earliest unfinished fixture, so a postponed game can sit in
 *  an earlier week than the league's: that is a catch-up game, titled by its own week. */
export function weekTitle(home) {
  const lw = num(home?.league?.week);
  const mw = num(home?.matchup?.week);
  if (mw !== null && lw !== null && mw < lw) return `Catch-up game · Week ${mw}`;
  const label = str(home?.league?.week_label) || (lw !== null ? `Week ${lw}` : '');
  return label ? `This week · ${label}` : 'This week';
}

function oppName(m) {
  return str(m?.opponent?.owner_display) || str(m?.opponent?.name) || 'your opponent';
}

/** {home, away} as [you, opp] from this team's side, or null. */
function mine(pair, home) {
  if (!pair || num(pair.home) === null || num(pair.away) === null) return null;
  return home ? [pair.home, pair.away] : [pair.away, pair.home];
}

function standing(you, opp) {
  if (you > opp) return `You lead ${you}-${opp}`;
  if (you < opp) return `You trail ${you}-${opp}`;
  return `Tied ${you}-${opp}`;
}

function finalLine(m, teamId) {
  const decided = m.winner_team_id !== null && m.winner_team_id !== undefined;
  const won = decided && m.winner_team_id === teamId;
  if (m.forfeit === true) return decided ? `Final · ${won ? 'Won' : 'Lost'} by forfeit` : 'Final · Forfeit';
  if (!decided) return 'Final';
  const s = mine(m.series, m.home);
  return `Final · ${won ? 'Won' : 'Lost'} the series${s ? ` ${s[0]}-${s[1]}` : ''}`;
}

const join = (...parts) => parts.filter(Boolean).join(' · ');

/** The This Week card. A room only counts when it has a code to join. */
export function cardState(home) {
  const m = home?.matchup;
  const base = { line: '', action: null, cancel: false, room: null };
  if (home?.schedule_published === false) return { ...base, kind: 'unscheduled', line: 'Schedule not published yet' };
  if (!m || m.status === 'bye' || !m.opponent) return { ...base, kind: 'bye', line: 'No game this week' };
  if (m.status === 'final') return { ...base, kind: 'final', line: finalLine(m, home?.team?.id) };
  const s = mine(m.series, m.home);
  const who = oppName(m);
  if (m.status === 'live' || m.room?.state === 'in_series') return { ...base, kind: 'live', line: s ? `LIVE · ${standing(...s)}` : 'LIVE', action: 'Open room', room: m.room || null };
  const series = m.status === 'in_progress' && s ? `Series · ${standing(...s)}` : '';
  // Plan 2 sends no `you_host`: the opponent hosts exactly when the room's host is the opponent's Discord ID.
  const room = m.room && str(m.room.code) ? m.room : null;
  if (room) {
    const oppId = str(m.opponent.discord_user_id);
    const theirs = oppId !== '' && room.host_discord_user_id === oppId;
    const lobby = room.state === 'lobby';
    if (theirs) return lobby ? { ...base, kind: 'lobby', line: join(series, `You're in @${who}'s room`), action: 'Open room', room }
      : { ...base, kind: 'join', line: join(series, `@${who} is waiting`), action: `Join @${who}'s game`, room };
    return { ...base, kind: lobby ? 'lobby' : 'waiting', line: join(series, lobby ? `@${who} joined` : `Waiting for @${who}…`), action: 'Open room', cancel: true, room };
  }
  if (!str(m.opponent.discord_user_id)) return { ...base, kind: 'unlinked', line: join(series, "Opponent hasn't linked Discord") };
  return { ...base, kind: 'play', line: join(series, m.opponent.online === true ? `@${who} is online` : ''), action: 'Play league game' };
}

function gb(bytes) {
  const n = num(bytes);
  return n && n > 0 ? ` (${(n / 1073741824).toFixed(1)} GB)` : '';
}

function pct(p) {
  return num(p?.total) && p.total > 0 && num(p?.done) !== null ? Math.max(0, Math.min(100, Math.round((p.done / p.total) * 100))) : null;
}

/** The header's Install/Update button, or null when the League edition is installed and current. `progress` is the
 *  latest `league-install-progress` payload while an install runs, else null. */
export function installState(status, progress) {
  const busy = (label, percent) => ({ label: percent === null ? label : `${label} ${percent}%`, disabled: true, primary: false, percent });
  if (progress && typeof progress === 'object') {
    const p = pct(progress);
    if (progress.phase === 'verify') return busy('Checking downloaded files…', null);
    if (progress.phase === 'compose') return busy('Setting up textures…', p);
    return busy('Downloading league files…', p);
  }
  if (!status || typeof status !== 'object') return { label: 'Checking league files…', disabled: true, primary: false, percent: null };
  if (status.installed !== true) return { label: `Install league files${gb(status.size_bytes)}`, disabled: false, primary: true, percent: null };
  if (status.update_available === true) return { label: `Update league files${gb(status.size_bytes)}`, disabled: false, primary: true, percent: null };
  return null;
}

/** Whether the three play buttons work: only with the League edition installed, current and not mid-install. */
export function playGate(status, progress) {
  if (progress && typeof progress === 'object') return { ready: false, hint: 'League files are installing…' };
  if (!status || typeof status !== 'object') return { ready: false, hint: 'Checking league files…' };
  if (status.installed !== true) return { ready: false, hint: 'Install league files to play.' };
  if (status.update_available === true) return { ready: false, hint: 'Update league files to play.' };
  return { ready: true, hint: null };
}

/** A room refused because the two players' league files differ (Plan 2 `league_version_mismatch`, mapped by 3b to
 *  kind `league_update_required`; the existing join path may pass the raw code through as the message). */
export function isUpdateRequired(e) {
  if (typeof e === 'string') return e === 'league_version_mismatch';
  if (!e || typeof e !== 'object') return false;
  return e.kind === 'league_update_required' || e.message === 'league_version_mismatch';
}

export function friendlyRuleset(leagueSettingsOn) {
  return leagueSettingsOn ? 'league' : 'raw';
}

function side(t, score, won) {
  return { abbrev: str(t?.abbr), name: str(t?.name) || str(t?.abbr), record: str(t?.record), logo_url: str(t?.logo_url) || null,
    color: teamColor(t), score, winner: won };
}

/** The week scoreboard strip: your game first, then the server's order. Pairs print away-home. A row with no fixture
 *  or teams is dropped. A winner is marked only on a final game (mid-series `winner_team_id` is the leader). */
export function scoreCells(home) {
  const mineId = home?.matchup?.fixture_id;
  const rows = arr(home?.week_games).filter((g) => g && g.fixture_id !== undefined && g.fixture_id !== null && g.home && g.away);
  rows.sort((a, b) => (b.fixture_id === mineId) - (a.fixture_id === mineId));
  return rows.map((g) => {
    const status = ['final', 'in_progress', 'live'].includes(g.status) ? g.status : 'scheduled';
    const ser = g.series && num(g.series.home) !== null && num(g.series.away) !== null ? `${g.series.away}-${g.series.home}` : '';
    const statusText = status === 'final' ? (g.forfeit === true ? 'FORFEIT' : ser ? `FINAL · ${ser}` : 'FINAL')
      : status === 'live' ? (ser ? `LIVE · ${ser}` : 'LIVE')
      : status === 'in_progress' ? (ser ? `SERIES · ${ser}` : 'SERIES')
      : 'This week';
    const decided = status === 'final' && g.winner_team_id !== null && g.winner_team_id !== undefined;
    const pts = (k) => (status === 'scheduled' ? null : num(g.score?.[k]));
    return {
      key: g.fixture_id, mine: g.fixture_id === mineId, status, statusText,
      away: side(g.away, pts('away'), decided && g.winner_team_id === g.away.team_id),
      home: side(g.home, pts('home'), decided && g.winner_team_id === g.home.team_id),
    };
  });
}

export function standingsRows(home) {
  const me = home?.team?.id;
  return arr(home?.standings).filter(Boolean).map((r) => ({
    rank: num(r.rank), name: str(r.name), abbrev: str(r.abbr), record: str(r.record), mine: r.team_id === me,
  }));
}

export function recentRows(home) {
  return arr(home?.recent).filter(Boolean).map((r) => {
    const where = `${r.home ? 'vs' : '@'} ${str(r.opponent_abbr)}${num(r.week) !== null ? ` · Week ${r.week}` : ''}`;
    if (r.result !== 'W' && r.result !== 'L') return { result: null, text: `Forfeit ${where}` };
    if (r.forfeit === true) return { result: r.result, text: `${r.result} by forfeit ${where}` };
    const line = str(r.score) || str(r.series);
    return { result: r.result, text: `${r.result}${line ? ` ${line}` : ''} ${where}` };
  });
}

const LEADER_COLS = [['pass', 'Passing'], ['rush', 'Rushing'], ['rec', 'Receiving']];
export function leaderCols(home) {
  const unit = str(home?.leaders_unit);
  return LEADER_COLS.map(([k, label]) => ({
    label: unit ? `${label} ${unit}` : label,
    rows: arr(home?.leaders?.[k]).filter(Boolean).slice(0, 3).map((p) => ({
      name: str(p.name),
      value: num(p.value) === null ? '—' : `${p.value.toLocaleString('en-US')}${unit ? ` ${unit}` : ''}`,
    })),
  }));
}

export function rosterRows(home) {
  return arr(home?.roster).filter(Boolean).map((p) => ({
    key: p.poid ?? null, number: num(p.number) === null ? '' : String(p.number), name: str(p.name), position: str(p.position),
  }));
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `node --test crates/reborn-ui/ui/league-model.test.mjs`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Tools/fixtures/league-home.json Tools/fixtures/league-edition-status.json crates/reborn-ui/ui/league-model.js crates/reborn-ui/ui/league-model.test.mjs
git commit -m "ui: league page model + fixtures (schedule, forfeits, catch-up week, install/play gate)"
```

---

### Task 4: Nav injection for members only

**Files:**
- Modify: `crates/reborn-ui/ui/shell.js` (new export + one call inside the existing `if (bridge) { ... }` startup block)
- Test: `crates/reborn-ui/ui/league-nav.test.mjs`

**Interfaces:**
- Produces: `export function addLeagueLink(doc) -> boolean` (true when it inserted). The startup block calls it only when `league_member` answers `true`.

- [ ] **Step 1: Write the failing test** `crates/reborn-ui/ui/league-nav.test.mjs`

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';

// shell.js runs a startup block only when `window.__TAURI__` exists; the test has none, so importing it is side-effect
// free apart from any look.js import, which fails quietly (caught) outside a page.
globalThis.window = globalThis.window || {};
const { addLeagueLink } = await import('./shell.js');

function fakeDoc(hrefs) {
  const links = hrefs.map((href) => ({ href, getAttribute: (k) => (k === 'href' ? href : null) }));
  const inserted = [];
  const nav = {
    querySelector: (sel) => {
      const m = sel.match(/a\[href="([^"]+)"\]/);
      const hit = m && links.find((l) => l.href === m[1]);
      if (!hit) return null;
      return { ...hit, insertAdjacentElement: (where, node) => { inserted.push([where, hit.href, node]); } };
    },
  };
  const doc = {
    querySelector: (sel) => (sel === '.shell-nav' ? nav : null),
    createElement: (tag) => ({ tag, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = v; }, append(...c) { this.children.push(...c); }, set innerHTML(_) { throw new Error('no innerHTML'); } }),
    createElementNS: (_ns, tag) => ({ tag, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = v; }, append(...c) { this.children.push(...c); } }),
  };
  return { doc, inserted };
}

test('adds League right after Online', () => {
  const { doc, inserted } = fakeDoc(['index.html', 'online.html', 'stats.html']);
  assert.equal(addLeagueLink(doc), true);
  assert.equal(inserted.length, 1);
  assert.deepEqual([inserted[0][0], inserted[0][1], inserted[0][2].attrs.href], ['afterend', 'online.html', 'league.html']);
});

test('never twice, and not on a page without the sidebar', () => {
  assert.equal(addLeagueLink(fakeDoc(['online.html', 'league.html']).doc), false);
  assert.equal(addLeagueLink(fakeDoc(['index.html']).doc), false, 'no Online link: onboarding and harness pages');
  assert.equal(addLeagueLink({ querySelector: () => null }), false);
});
```

If importing `shell.js` in node throws (a top-level `document` read, or a browser-only import), stub the missing global at the top of the test (`globalThis.document = { getElementById: () => null, addEventListener() {} }`) rather than changing `shell.js`'s top level, and note it.

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test crates/reborn-ui/ui/league-nav.test.mjs`
Expected: FAIL, `addLeagueLink` is not exported.

- [ ] **Step 3: Write the implementation** in `shell.js`, just above the startup `if (bridge) { ... }` block:

```js
// The League tab (spec 2026-10-08): members only, so it is not in any page's hard-coded nav. The startup block below
// adds it after Online when the core's cached answer (`league_member`, no network) says this player is in the league.
// league.html carries its own link, so the page is never without it. Built with createElement only.
const LEAGUE_ICON = ['M12 3 4 6v6c0 4.4 3.4 8.3 8 9 4.6-.7 8-4.6 8-9V6l-8-3Z', 'm9 12 2 2 4-4'];
export function addLeagueLink(doc = document) {
  const nav = doc && typeof doc.querySelector === 'function' ? doc.querySelector('.shell-nav') : null;
  if (!nav || nav.querySelector('a[href="league.html"]')) return false;
  const online = nav.querySelector('a[href="online.html"]');
  if (!online) return false;
  const a = doc.createElement('a');
  a.setAttribute('href', 'league.html');
  const ns = 'http://www.w3.org/2000/svg', svg = doc.createElementNS(ns, 'svg');
  for (const [k, v] of [['viewBox', '0 0 24 24'], ['aria-hidden', 'true'], ['fill', 'none'], ['stroke', 'currentColor'], ['stroke-width', '1.6'], ['stroke-linecap', 'round'], ['stroke-linejoin', 'round']]) svg.setAttribute(k, v);
  for (const d of LEAGUE_ICON) { const p = doc.createElementNS(ns, 'path'); p.setAttribute('d', d); svg.append(p); }
  const label = doc.createElement('span');
  label.textContent = 'League';
  a.append(svg, label);
  online.insertAdjacentElement('afterend', a);
  return true;
}
```

Inside the existing `if (bridge) { ... }` startup block, as its last statement (so it runs before any page's `markCurrent()` and before `look.js` mounts the nav marker; if the block is not `async`, wrap in an `(async () => { ... })()`):

```js
  try { if (await bridge.invoke("league_member") === true) addLeagueLink(document); } catch (_) { /* no tab is the safe answer */ }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `node --test crates/reborn-ui/ui/*.test.mjs Tools/lib/*.test.mjs`
Expected: all PASS (new and existing).

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/shell.js crates/reborn-ui/ui/league-nav.test.mjs
git commit -m "ui: League nav link for league members only (cached, no network)"
```

---

### Task 5: The League page (data, install/update, play panel)

**Files:**
- Create: `crates/reborn-ui/ui/league.html`
- Create: `crates/reborn-ui/ui/league.js`
- Create: `crates/reborn-ui/ui/pages/league.css`
- Modify: `crates/reborn-ui/ui/theme.css` (2 tokens in `:root`)
- Modify: `Tools/check-theme.mjs` (`PAGES` + `THEMED`)
- Modify: `Tools/check-shell-pages.mjs` (`PAGES` + the `FULL` answers map: stub 3b's commands)
- Modify: `crates/reborn-ui/ui/DESIGN.md` (§1 table, §5 sidebar line, §8 page table, §12 test table)

**Interfaces:**
- Consumes: Task 2 `league_home`; Task 3 model; existing `mm_room_join({ code })`, `mm_room_action({ roomId, action })`, `open_website({ url })`; existing `card`, `cols` (`stats-cards.js`), `teamLogo`, `avatar` (`match-rows.js`), `makePoller` (`live-common.js`), `createSignIn` (`sign-in.js`), `get`/`set` (`shell.js`); **3b:** `league_edition_status`, `league_edition_install` + `league-install-progress` event, `league_play_offline`, `league_host_friendly({ ruleset })`, `league_room_open`.
- 3b commands are stubbed in `check-shell-pages.mjs`'s answer map so this task's checks pass whether or not 3b has merged.

Before Step 3, check these existing names and adapt (note any difference in the report):
- `sign-in.js`: the method that returns the sign-in card (`stats.js` uses it; mirror `stats.js`).
- `stats-cards.js`: `card(id, title, body, { span, cls })` and `cols(node, template)`.
- `match-rows.js`: `teamLogo(team, cls)` and `avatar(url, name, size)`.
- `live-common.js`: `makePoller(fn, ms)` with `.start()` / `.stop()`.
- `shell.js`: `get(key)` / `set(key, value)` persist a string per viewer.
- How `mm_room_join`'s rejection carries a 409 body (`grep -n "fn mm_room_join" -A30 crates/reborn-ui/src/online_mm.rs`): if it passes the server `error` string as `message`, `isUpdateRequired` covers it; if it maps it to a generic sentence, note it as a 3b follow-up (3b must map `league_version_mismatch` to `kind: "league_update_required"` on that path).

- [ ] **Step 1: Register the page in the checkers first (the failing check)**

`Tools/check-theme.mjs`: append `"league"` to `PAGES` and `league: "pages/league.css",` to `THEMED`.
`Tools/check-shell-pages.mjs`: add `["crates/reborn-ui/ui/league.js", "League"],` to `PAGES`. In its `FULL` answers map add:

```js
  league_home: JSON.parse(readFileSync(new URL('./fixtures/league-home.json', import.meta.url), 'utf8')),
  league_member: true,
  // Plan 3b's commands, stubbed so this page checks without them:
  league_edition_status: JSON.parse(readFileSync(new URL('./fixtures/league-edition-status.json', import.meta.url), 'utf8')),
  league_edition_install: { version: '2026.10.08' },
  league_play_offline: null,
  league_host_friendly: { action: 'created', room: { room_id: 'r9', code: 'FRND01' } },
  league_room_open: { action: 'created', room: { room_id: 'r1', code: 'K7QP2M' } },
```

(Import `readFileSync` from `node:fs` at the top if the file does not already; match how the file loads other fixtures if it already does.)

Run: `node Tools/check-theme.mjs`
Expected: FAIL, `ENOENT ... league.html`.

- [ ] **Step 2: Write `league.html`** (LF). A copy of `stats.html`'s shell with the League link (current), its own title, and `league.js`:

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1">
  <title>League | NFL Street Reborn</title>
  <script src="look-boot.js"></script>
  <link rel="icon" href="art/cover.png">
  <link rel="stylesheet" href="theme.css"><link rel="stylesheet" href="pages/league.css">
</head>
<body class="shell themed ob page-league">
  <a class="skip-link" href="#main">Skip to content</a>
  <div class="backdrop" aria-hidden="true"></div>
  <header class="app-header"><span class="breadcrumb">NFL STREET REBORN</span><span id="bridge" class="pill">Connecting</span></header>
  <nav class="shell-nav" aria-label="Main navigation">
    <!-- COPY: stats.html's nav lines from the brand link through the Online link, verbatim. -->
    <a href="league.html" class="current" aria-current="page"><svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 4 6v6c0 4.4 3.4 8.3 8 9 4.6-.7 8-4.6 8-9V6l-8-3Z"/><path d="m9 12 2 2 4-4"/></svg><span>League</span></a>
    <!-- COPY: stats.html's Stats link WITHOUT class="current" aria-current="page", then the rest of its nav (Settings, Help, nav-bottom) verbatim. -->
  </nav>
  <main id="main" tabindex="-1">
    <section id="screen"><p class="loading">Loading…</p></section>
    <details class="core"><summary>Diagnostic details</summary><div class="out-head"><button id="clear" type="button">Clear log</button></div><pre id="out"></pre></details>
  </main>
  <footer class="input-footer"><span id="input-hints"></span><span>NFL STREET REBORN</span></footer>
  <script type="module" src="app-shell.js"></script><script type="module" src="league.js"></script>
</body>
</html>
```

The two `COPY:` comments are instructions: replace each with the exact lines from `stats.html` and do not ship the comments. Diff the result's nav against `stats.html`'s: the only difference must be the League link and the moved `current`.

- [ ] **Step 3: Write `league.js`**

```js
// The League tab (spec 2026-10-08 v2 §3-4): the league site's homepage for your team, plus the League edition's
// Install/Update button and play panel. Members only: league_home answers member:false for anyone else, and this
// page sends them to Play. Everything is server data, drawn with el()'s `text` only; images only through
// teamLogo()/avatar() (assetUrl / avatarSrc, the two prefixes the CSP allows).
// The League edition commands (league_edition_*, league_play_offline, league_host_friendly, league_room_open) are
// Plan 3b's. Rooms run through the existing private-room path and hand over to the Online page.
import { call, el, get, markCurrent, quietCall, reason, set, wireCoreFold } from './shell.js';
import { makePoller } from './live-common.js';
import { card, cols } from './stats-cards.js';
import { avatar, teamLogo } from './match-rows.js';
import { createSignIn } from './sign-in.js';
import {
  bannerLine, cardState, friendlyRuleset, headerLine, installState, isUpdateRequired, leaderCols, pageMode, playGate,
  recentRows, rosterRows, scoreCells, standingsRows, teamColor, weekTitle,
} from './league-model.js';

const REFRESH_MS = 60000;
const FRIENDLY_KEY = 'league-friendly-league-settings';
const screen = document.getElementById('screen');
const state = { home: null, error: null, busy: false, actionError: null, edition: null, progress: null, needsUpdate: false };
const signIn = createSignIn({ onChange: () => draw(), onDone: () => poller.start() });
const logoTeam = (t) => ({ name: t?.name || t?.abbrev || t?.abbr, abbrev: t?.abbrev || t?.abbr, logo_url: t?.logo_url });
const tinted = (node, color) => { if (color) node.style.setProperty('--tc', color); return node; };
const leagueSettingsOn = () => get(FRIENDLY_KEY) !== 'off';   // default ON

async function loadEdition() {
  try { state.edition = await quietCall('league_edition_status'); }
  catch (_) { state.edition = null; }
}

async function load() {
  try {
    const home = await quietCall('league_home');
    state.home = home;
    state.error = null;
    if (pageMode(home) === 'redirect') { if (typeof location?.replace === 'function') location.replace('index.html'); return; }
  } catch (e) {
    state.error = reason(e);
    draw();
    throw e; // makePoller backs off (and waits a full minute after a rate limit)
  }
  if (!state.progress) await loadEdition();
  draw();
}
const poller = makePoller(load, REFRESH_MS);

// INTEGRATION SEAM (Plan 2/3b): a league room is joined like any private room, by its code.
async function joinLeagueRoom(code) {
  await call('mm_room_join', { code });
}

async function act(fn) {
  if (state.busy) return;
  state.busy = true; state.actionError = null; draw();
  try { await fn(); }
  catch (e) {
    if (isUpdateRequired(e)) { state.needsUpdate = true; state.actionError = 'Your league files are out of date. Update league files to play.'; await loadEdition(); }
    else state.actionError = reason(e);
  }
  finally { state.busy = false; draw(); }
}

async function install() {
  if (state.progress) return;
  let unlisten = null;
  state.progress = { phase: 'download', done: 0, total: 0 }; state.actionError = null; draw();
  try {
    if (window.__TAURI__?.event?.listen) unlisten = await window.__TAURI__.event.listen('league-install-progress', (e) => { state.progress = e.payload; draw(); });
    await call('league_edition_install');
    state.needsUpdate = false;
  } catch (e) {
    state.actionError = reason(e);
  } finally {
    if (typeof unlisten === 'function') unlisten();
    state.progress = null;
    await loadEdition();
    draw();
  }
}

const play = () => act(async () => {
  const r = await call('league_room_open');
  if (r.action === 'join') await joinLeagueRoom(r.code);
  location.href = 'online.html';
});
const join = (room) => act(async () => { await joinLeagueRoom(room.code); location.href = 'online.html'; });
const cancel = (room) => act(async () => { await call('mm_room_action', { roomId: room.room_id, action: 'leave' }); await load(); });
const offline = () => act(async () => { await call('league_play_offline'); });
const friendly = () => act(async () => { await call('league_host_friendly', { ruleset: friendlyRuleset(leagueSettingsOn()) }); location.href = 'online.html'; });

function installButton() {
  const s = installState(state.edition, state.progress);
  if (!s) return state.edition?.version ? el('span', { class: 'lg-files muted', text: `League files ${state.edition.version}` }) : null;
  const btn = el('button', { type: 'button', class: `btn ${s.primary ? 'primary' : 'quiet'}${state.needsUpdate ? ' attention' : ''}`, 'data-key': 'league-install', text: s.label, disabled: s.disabled || state.busy, onclick: install });
  return el('div', { class: 'lg-install' }, btn, s.percent === null ? null : el('progress', { max: 100, value: s.percent, 'aria-label': s.label }));
}

function teamHeader(home) {
  const t = home.team || {};
  const site = typeof home.site_url === 'string' && home.site_url.startsWith('https://nflstreetreborn.com/')
    ? el('button', { type: 'button', class: 'btn quiet sm', text: 'View on site ›', onclick: () => call('open_website', { url: home.site_url }).catch(() => {}) })
    : null;
  return tinted(el('header', { class: 'glass lg-head' },
    teamLogo(logoTeam(t), 'lg-head-logo'),
    el('div', { class: 'lg-head-text' }, el('span', { class: 'eyebrow', text: home.league?.name || 'League' }), el('h1', { text: t.name || 'Your team' }),
      el('p', { class: 'lg-head-line num', text: headerLine(home) })),
    el('div', { class: 'lg-head-actions' }, installButton(), site)), teamColor(t));
}

function sideRow(s) {
  return tinted(el('div', { class: `lg-side${s.winner ? ' won' : ''}` }, el('span', { class: 'lg-chip', 'aria-hidden': 'true' }), teamLogo(logoTeam(s), 'lg-cell-logo'),
    el('b', { text: s.abbrev }), el('span', { class: 'lg-rec num', text: s.record }),
    el('span', { class: 'lg-pts num', text: s.score === null ? '' : String(s.score) })), s.color);
}

function scoreboard(home) {
  const cells = scoreCells(home);
  if (!cells.length) return null;
  const track = el('div', { class: 'lg-track', role: 'list' }, ...cells.map((c) => el('button', {
    type: 'button', role: 'listitem', class: `lg-cell${c.mine ? ' mine' : ''}`,
    'aria-label': `${c.away.abbrev} at ${c.home.abbrev}, ${c.statusText}`,
    onclick: c.mine ? () => document.getElementById('lg-week')?.scrollIntoView({ block: 'nearest' }) : null,
  }, sideRow(c.away), sideRow(c.home),
    el('span', { class: `lg-status ${c.status}` }, c.status === 'live' ? el('span', { class: 'live-dot', 'aria-hidden': 'true' }) : null, c.statusText))));
  const step = (dir) => () => track.scrollBy({ left: dir * track.clientWidth * 0.8, behavior: 'smooth' });
  track.addEventListener('wheel', (e) => { if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) { track.scrollLeft += e.deltaY; e.preventDefault(); } }, { passive: false });
  return el('section', { class: 'glass lg-board', 'aria-label': `Week ${home.league?.week ?? ''} scoreboard` },
    el('button', { type: 'button', class: 'btn quiet sm lg-step', 'aria-label': 'Earlier games', text: '‹', onclick: step(-1) }),
    track,
    el('button', { type: 'button', class: 'btn quiet sm lg-step', 'aria-label': 'Later games', text: '›', onclick: step(1) }));
}

function thisWeek(home) {
  const s = cardState(home);
  const gate = playGate(state.edition, state.progress);
  const m = home.matchup || {};
  const opp = m.opponent || {};
  const vsTeam = (t) => tinted(el('div', { class: 'lg-vs-team' }, teamLogo(logoTeam(t), 'lg-vs-logo'), el('b', { text: t?.abbr || '' }), el('span', { class: 'num', text: t?.record || '' })), teamColor(t));
  const vs = (s.kind === 'bye' || s.kind === 'unscheduled') ? null : el('div', { class: 'lg-vs' }, vsTeam(home.team), el('span', { class: 'lg-vs-at', text: m.home ? 'vs' : '@' }), vsTeam(opp));
  const owner = opp.owner_display ? el('div', { class: 'lg-owner' }, avatar(opp.avatar_url, opp.owner_display, 'sm'),
    el('span', { text: `@${opp.owner_display}` }), opp.online === true ? el('span', { class: 'presence-dot', 'aria-label': 'Online' }) : null) : null;
  // Opening a room that already exists (waiting / lobby / live) needs no league files; starting or joining one does.
  const needsFiles = s.kind === 'play' || s.kind === 'join';
  const onClick = s.kind === 'play' ? play : s.kind === 'join' ? () => join(s.room) : (s.kind === 'waiting' || s.kind === 'lobby' || s.kind === 'live') ? () => { location.href = 'online.html'; } : null;
  const blocked = state.busy || (needsFiles && !gate.ready);
  const buttons = el('div', { class: 'lg-actions' },
    s.action ? el('button', { type: 'button', class: 'btn primary', 'data-key': 'league-primary', text: state.busy ? 'Working…' : s.action, disabled: blocked, onclick: onClick }) : null,
    s.cancel ? el('button', { type: 'button', class: 'btn quiet', text: 'Cancel', disabled: state.busy, onclick: () => cancel(s.room) }) : null);
  const hint = s.action && needsFiles && !gate.ready ? el('p', { class: 'muted lg-hint', text: gate.hint }) : null;
  return card('lg-week', weekTitle(home), [vs, owner, el('p', { class: `lg-line ${s.kind}`, text: s.line }), buttons, hint,
    state.actionError ? el('p', { class: 'refusal', role: 'alert', text: state.actionError }) : null], { span: 8, cls: `lg-week ${s.kind}` });
}

function playPanel() {
  const gate = playGate(state.edition, state.progress);
  const toggle = el('input', { type: 'checkbox', id: 'lg-league-settings', checked: leagueSettingsOn(),
    onchange: (e) => { set(FRIENDLY_KEY, e.target.checked ? 'on' : 'off'); } });
  return card('lg-play', 'Play the league build', [
    el('div', { class: 'lg-actions' },
      el('button', { type: 'button', class: 'btn', 'data-key': 'league-offline', text: 'Play offline', disabled: state.busy || !gate.ready, onclick: offline }),
      el('button', { type: 'button', class: 'btn', 'data-key': 'league-friendly', text: 'Host friendly', disabled: state.busy || !gate.ready, onclick: friendly })),
    el('label', { class: 'lg-toggle', for: 'lg-league-settings' }, toggle, el('span', { text: 'League settings (Hard, GameBreakers off)' })),
    gate.ready ? null : el('p', { class: 'muted lg-hint', text: gate.hint }),
  ], { span: 4 });
}

function standings(home) {
  const rows = standingsRows(home);
  return card('lg-standings', 'Standings', rows.length ? el('ol', { class: 'lg-list' }, ...rows.map((r) =>
    el('li', { class: r.mine ? 'mine' : null }, el('span', { class: 'num', text: r.rank ?? '' }), el('span', { text: r.name || r.abbrev }), el('span', { class: 'num r', text: r.record })))) : el('p', { class: 'muted', text: 'No standings yet.' }), { span: 4 });
}

function recent(home) {
  const rows = recentRows(home);
  return card('lg-recent', 'Recent results', rows.length ? el('ul', { class: 'lg-list' }, ...rows.map((r) => el('li', {}, el('span', { class: r.result === 'W' ? 'w' : r.result === 'L' ? 'l' : 'muted', text: r.text })))) : el('p', { class: 'muted', text: 'No results yet.' }), { span: 6 });
}

function leaders(home) {
  return card('lg-leaders', 'Team leaders', cols(el('div', { class: 'lg-leaders' }, ...leaderCols(home).map((c) => el('div', {},
    el('span', { class: 'k-label', text: c.label }), ...c.rows.map((r) => el('div', { class: 'lg-leader' }, el('span', { text: r.name }), el('b', { class: 'num', text: r.value })))))), 'repeat(3, 1fr)'), { span: 6 });
}

function roster(home) {
  const rows = rosterRows(home);
  if (!rows.length) return null;
  return el('details', { class: 'glass lg-roster s12' }, el('summary', { text: `Roster · ${rows.length} players` }),
    el('ol', { class: 'lg-list' }, ...rows.map((r) => el('li', { 'data-poid': r.key ?? null }, el('span', { class: 'num', text: r.number }), el('span', { text: r.name }), el('span', { class: 'r', text: r.position })))));
}

function draw() {
  const home = state.home;
  if (!home) { screen.replaceChildren(state.error ? el('p', { class: 'refusal', role: 'alert', text: state.error }) : el('p', { class: 'loading', text: 'Loading…' })); return; }
  if (pageMode(home) === 'signed_out') { screen.replaceChildren(signIn.render()); return; }
  if (pageMode(home) !== 'member') return;
  const banner = bannerLine(home);
  screen.replaceChildren(
    teamHeader(home),
    banner ? el('p', { class: 'partial', role: 'status', text: banner }) : null,
    scoreboard(home),
    el('div', { class: 'vgrid lg-grid' }, thisWeek(home), standings(home), playPanel(), recent(home), leaders(home), roster(home)));
}

markCurrent();
wireCoreFold();
poller.start();
document.addEventListener('visibilitychange', () => (document.hidden ? poller.stop() : poller.start()));
```

Replace `signIn.render()` with the exact call `stats.js` uses if it differs. If `el()` does not set `checked` / `for` / `value` as properties or attributes correctly for `input`/`label`/`progress`, set them after creation (`toggle.checked = leagueSettingsOn()`), and note it.

- [ ] **Step 4: Write `pages/league.css` and the two tokens**

`theme.css` `:root`, next to the Stats tokens:

```css
  --league-cell-w: 168px;              /* one week-scoreboard cell */
  --mine-edge: inset 3px 0 0 var(--red); /* your game / your row: a red left edge, not a glow */
```

`pages/league.css` (layout only, tokens only):

```css
/* The League tab (spec 2026-10-08 v2 §3-4). Layout only; every colour and size is a theme.css token. */
.page-league #screen { display: grid; gap: 16px; }

.page-league .lg-head { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 16px; padding: 16px 24px; border-radius: var(--r-lg);
  box-shadow: inset 4px 0 0 var(--tc, var(--team-fallback)), var(--sheen), var(--shadow); }
.page-league .lg-head-actions { display: grid; justify-items: end; gap: 8px; }
.page-league .lg-install { display: grid; gap: 4px; justify-items: end; }
.page-league .lg-install progress { width: 200px; }
.page-league .lg-install .btn.attention { box-shadow: var(--mine-edge); }
.page-league .lg-files { font-size: 12px; }
.page-league .lg-chip { width: 3px; align-self: stretch; border-radius: 2px; background: var(--tc, var(--team-fallback)); }
.page-league .lg-vs-team { border-top: 3px solid var(--tc, var(--team-fallback)); padding-top: 8px; }
.page-league .lg-head-logo { width: 64px; height: 64px; }
.page-league .lg-head h1 { font-size: 30px; line-height: 1.1; font-weight: 700; margin: 0; }
.page-league .lg-head-line { color: var(--fg-2); margin: 4px 0 0; }

.page-league .lg-board { display: grid; grid-template-columns: auto 1fr auto; align-items: stretch; border-radius: var(--r-lg); padding: 8px; gap: 8px; }
.page-league .lg-track { display: grid; grid-auto-flow: column; grid-auto-columns: var(--league-cell-w); gap: 8px; overflow-x: auto; scroll-snap-type: x proximity; scrollbar-width: none; }
.page-league .lg-track::-webkit-scrollbar { display: none; }
.page-league .lg-cell { display: grid; gap: 4px; padding: 8px; text-align: left; border: 1px solid var(--stroke); border-radius: var(--r-sm); background: var(--glass-1); scroll-snap-align: start; }
.page-league .lg-cell.mine { box-shadow: var(--mine-edge); background: var(--glass-2); }
.page-league .lg-side { display: grid; grid-template-columns: 3px 20px 1fr auto auto; align-items: center; gap: 8px; color: var(--fg-2); }
.page-league .lg-side.won { color: var(--fg); font-weight: 700; }
.page-league .lg-cell-logo { width: 20px; height: 20px; }
.page-league .lg-rec { color: var(--fg-3); font-size: 12px; }
.page-league .lg-status { display: flex; align-items: center; gap: 8px; font-size: 10.5px; font-weight: 600; letter-spacing: .06em; text-transform: uppercase; color: var(--fg-3); }
.page-league .lg-status.live { color: var(--red-ink); }
.page-league .lg-step { align-self: center; }

.page-league .lg-vs { display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; gap: 16px; }
.page-league .lg-vs-team { display: grid; justify-items: center; gap: 8px; }
.page-league .lg-vs-logo { width: 72px; height: 72px; }
.page-league .lg-vs-at { color: var(--fg-3); font-weight: 600; }
.page-league .lg-owner { display: flex; align-items: center; justify-content: center; gap: 8px; margin-top: 16px; color: var(--fg-2); }
.page-league .lg-line { text-align: center; margin: 16px 0 0; color: var(--fg-2); }
.page-league .lg-line.live { color: var(--red-ink); font-weight: 600; }
.page-league .lg-actions { display: flex; justify-content: center; flex-wrap: wrap; gap: 8px; margin-top: 16px; }
.page-league .lg-hint { text-align: center; margin: 8px 0 0; }
.page-league .lg-toggle { display: flex; align-items: center; justify-content: center; gap: 8px; margin-top: 16px; color: var(--fg-2); }

.page-league .lg-list { list-style: none; margin: 0; padding: 0; display: grid; }
.page-league .lg-list li { display: grid; grid-template-columns: 32px 1fr auto; align-items: center; gap: 8px; min-height: 40px; padding: 0 16px; border-top: 1px solid var(--rule); }
.page-league .lg-list li.mine { box-shadow: var(--mine-edge); background: var(--red-wash); }
.page-league .lg-list .r { text-align: right; }
.page-league .lg-list .w { color: var(--ok); }
.page-league .lg-list .l { color: var(--red-ink); }

.page-league .lg-leaders { display: grid; gap: 16px; }
.page-league .lg-leader { display: flex; justify-content: space-between; gap: 8px; min-height: 32px; align-items: center; }

.page-league .lg-roster { border-radius: var(--r-lg); padding: 8px 0; }
.page-league .lg-roster summary { padding: 8px 16px; cursor: pointer; font-weight: 600; }

@media (max-width: 1100px) { .page-league .lg-head { grid-template-columns: auto 1fr; } .page-league .lg-head-actions { grid-column: 1 / -1; justify-items: start; } }
```

If `--rule`, `--red-wash`, `--ok`, `--red-ink`, `--team-fallback`, `--sheen`, `--shadow`, `--glass-*`, `--fg-*` or `--r-*` is missing from `theme.css` in this branch, stop and report it (they are listed in DESIGN.md §1).

- [ ] **Step 5: Update DESIGN.md**
  - §1 tokens table: rows for `--league-cell-w` (one week-scoreboard cell, League) and `--mine-edge` (your game / your row / the Update attention mark: 3px red left edge, League).
  - §5 sidebar line: `…Online, League (league members only, added by shell.js from league_member), Stats…`.
  - §8 page table: `| league.html | pages/league.css |`.
  - §12 test table: `| league-model.test.mjs | League's model: page mode, card states, install/play gate, scoreboard cells, rows |` and `| league-nav.test.mjs | the League link: members only, after Online, never twice |`.

- [ ] **Step 6: Run every check**

Run, from the repo root:
```bash
node Tools/check-theme.mjs
node Tools/check-theme-tokens.mjs
node Tools/check-shell-pages.mjs
node --test crates/reborn-ui/ui/*.test.mjs Tools/lib/*.test.mjs
CARGO_BUILD_JOBS=1 cargo test -p reborn-ui
```
Expected: all PASS. `check-shell-pages` must report the League page drawing with every command answered, with every field null, and with every call rejected. With every field null, `pageMode` is `redirect` and the guarded `location.replace` must not throw. With every call rejected, the page shows the refusal sentence and no button throws.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/league.html crates/reborn-ui/ui/league.js crates/reborn-ui/ui/pages/league.css crates/reborn-ui/ui/theme.css crates/reborn-ui/ui/DESIGN.md Tools/check-theme.mjs Tools/check-shell-pages.mjs
git commit -m "ui: League page (team header + install/update, week scoreboard, This Week card, play panel, standings, results, leaders, roster)"
```

---

### Task 6: Run it (needs Plan 3b merged and Plan 2 deployed), then a beta build note

**Files:** none unless a bug turns up; any fix goes back through the owning task's test first.

- [ ] **Step 1: Point a dev launcher at the backend.** Until Plan 2 is deployed, run it locally and set `backend_origin` in the launcher's settings file to it. Images still come from production, so league logos show as abbreviation badges locally. That is expected.

- [ ] **Step 2: Run the launcher** (`CARGO_BUILD_JOBS=1 cargo run -p reborn-ui`) signed in as a Season 4 owner and check, with a screenshot of each:
  - The League link sits between Online and Stats on every page; the red nav marker lands on it on `league.html`.
  - Header with **Install league files (X GB)** before install; progress shows Download → Checking → Setting up textures; afterwards the button is replaced by "League files <version>". All play buttons were disabled with "Install league files to play." until then.
  - With no schedule (Season 4 today): "Schedule not published yet", no scoreboard strip, play panel usable.
  - Once fixtures exist: scoreboard (your game first with the red edge, LIVE dot on a live game, ‹ › and wheel scroll), This Week card, standings with your row marked, recent (forfeits read as forfeits), leaders in TD, roster expands.
  - Controller: from the sidebar, Right lands in the page; the Install and Play buttons are reachable; Left/Right moves across scoreboard cells.
  - Window at 900 px wide: no horizontal page scroll.

- [ ] **Step 3: Play panel checks.**
  - Play offline boots the league build.
  - Host friendly with League settings ON → Online page shows the friendly room; game boots on the league seed. Toggle OFF (persists across reloads) → boots raw.

- [ ] **Step 4: Two-account league room check** (second PC or second Windows user):
  - A presses Play league game → Online page shows the league room.
  - B's card turns to "Join @A's game" within 60 s → Join → both in the room → ready → game boots on the league seed.
  - With B on an older `league-game` version, B's Join shows "Your league files are out of date. Update league files to play." and the Update button is marked; after updating, Join works.
  - A's Cancel before game 1 removes the room; B's card returns to Play.

- [ ] **Step 5: Non-member check.** Signed in as a non-owner: no League link anywhere; `league.html` opened directly redirects to Play. Sign out from a member account: the link is gone on the next page.

- [ ] **Step 6: Beta build note.** Build the installer the way beta builds are made today, from this worktree only, after `git status` is clean (BUILD SYNC LAW). **Do not publish, sign or bump the channel.** Write the installer path, SHA-256 and commit hash into the PR description for Adam. Ship order: Plan 1 (live), Plan 2 (backend, dark), Plan 4 (first `league-game` + `league-content` published, which needs Adam's `LEAGUE_GAME_ID`), Plan 3b, then this.

---

## Self-review

- **Spec coverage (v2 §3 + UI parts of §4):**
  - Gating / identity → Tasks 1, 2, 4 (deviation 1 documented). Redirect of non-members → Task 3 `pageMode` + Task 5 `load`.
  - Layout incl. scoreboard → Tasks 3, 5. Card states: bye, unscheduled, play, waiting, join, lobby, live, final (+forfeit), unlinked → Task 3.
  - Install/Update button with progress; play buttons disabled until installed and current → Task 3 `installState`/`playGate`, Task 5.
  - Play offline / Host friendly with League settings toggle default ON → Task 3 `friendlyRuleset`, Task 5 `playPanel`.
  - `league_version_mismatch` → "Update league files" → Task 3 `isUpdateRequired`, Task 5 `act`.
  - Catch-up week from per-team matchup → Task 3 `weekTitle`.
  - 60 s poll + pause when hidden, "View on site", gamepad, error/empty states → Tasks 1, 3, 5, 6.
  - Moved to Plan 3b: seed selection by ruleset/edition, `intent.league` + fixture, Online room card league title, `league_room_open`, all League edition commands.
- **Placeholders:** the `COPY:` comments in Task 5 Step 2 are explicit copy instructions, verified by a diff against `stats.html`. Name checks before Tasks 1, 2, 5 are explicit grep steps.
- **Type consistency:** `LeagueCache`, `resolve`, `fetch_home` match across Tasks 1–2. Command names in Task 2 and the 3b table match the `call(...)` names in Tasks 4–5 and the `check-shell-pages` stubs. Model names in Task 3's interface match Task 5's imports. `matchup.room.{room_id, code, host_discord_user_id, state}` are consistent across the contract, Task 3 and Task 5.
- **Review Focus:** each of the five lines maps to a named test or check in its owning task.
