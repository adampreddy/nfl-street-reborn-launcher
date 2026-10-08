# League Tab, Plan 3 of 3: Launcher — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a members-only League tab to the launcher: the week's scoreboard strip, your team's This Week card (play / join / waiting / live / final / bye), standings, recent results, team leaders and roster, with a one-click league best-of-3 room that hands off to the existing Online room flow.

**Architecture:** A new `reborn-core::league_home` client calls the backend's `/v1/league/*` endpoints (plans 1 and 2) with the player's bearer token and keeps the last good answer in `league.json`. Four Tauri commands in a new `reborn-ui/src/league_tab.rs` expose it. The page (`league.html` + `league.js` + pure `league-model.js` + `pages/league.css`) reuses the Stats v2 card components. `shell.js` injects the nav link only when the cached answer says "member". Joining a league room reuses the existing private-room path (`mm_room_join` by code), then sends the player to `online.html`, where the existing room → match → Dolphin flow runs unchanged. That flow now carries `league_match = true` when the room is a league room.

**Tech Stack:** Rust (reborn-core, reborn-ui / Tauri 2), vanilla ES modules, `node --test`, the theme checkers under `Tools/`.

**Spec:** `docs/superpowers/specs/2026-10-08-league-tab-design.md` (section 3 "Launcher" is this plan). Plans 1 (league site) and 2 (backend) are written in parallel; this plan depends only on the backend contract below.

**Worktree / branch:** `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league`, branch `feat/league-tab` (base 3.0.35 @ d37500a).

## Global Constraints

- Build/test Rust with `CARGO_BUILD_JOBS=1` (memory: parallel builds have run this PC out of memory).
- Every new or edited `.rs`/`.js`/`.html`/`.css` file is **LF**, not CRLF. Source-scanning tests break on CRLF (memory `project_reborn_launcher_crlf_breaks_tests.md`). Check with `git diff --stat` + `file <path>` before each commit.
- The core module is **`league_home.rs`**, never `league.rs`: `reborn-core/src/lib.rs` has a test asserting the retired `league.rs` uploader and `pub mod league;` stay gone.
- CSP stays as is (`crates/reborn-ui/tauri.conf.json`). Images only through `assetUrl()` (`/live-assets/...png` on `https://api.nflstreetreborn.com`) and `avatarSrc()` (Discord CDN). League logos arrive as `/live-assets/league/<team_id>.png` (backend plan).
- Server text is drawn with `el(..., { text })` only, never `innerHTML` (DESIGN.md §10).
- Page CSS holds layout only: no colour literals, no weight above 700, no new durations, no glows (DESIGN.md §8, §10). Any new token goes in `theme.css` and DESIGN.md §1 in the same commit.
- No player ever sees a status code, URL or raw error: commands return `{ kind, message }` with fixed sentences (same pattern as `live_view.rs`).
- League rules are the server's (Ranked settings, best of 3). The launcher never sends a mode, seed or best-of for a league room.
- No release, version bump or publish in this plan. The last task builds a beta installer locally and stops.

## Backend contract this plan assumes (plan 2 must match; deviations break Task 4 tests)

The body is **plan 1's site contract verbatim** (`docs/superpowers/plans/2026-10-08-league-tab-1-site.md` § "JSON contract", commit 6c37c29), passed through by the backend with four additions (marked ➕ below).

`GET /v1/league/me` (bearer) → 200:
```json
{ "ok": true, "member": false }
```
or (site contract plus ➕ backend fields):
```json
{
  "ok": true,
  "member": true,
  "league":  { "id": 36, "name": "Reborn Season 4", "season": 4, "week": 6, "week_label": "Week 6" },
  "team":    { "id": 493, "name": "Subway Surfers", "abbr": "ADAM", "logo_url": "/live-assets/league/493.png", "color": "#00A3FF", "record": "4-1", "standing": 2 },
  "matchup": { "fixture_id": 1234, "week": 6, "home": true, "status": "scheduled",
               "score": null, "series": null, "winner_team_id": null, "when": null,
               "opponent": { "team_id": 486, "name": "Football Furies", "abbr": "BLONDE", "logo_url": null, "color": "#FF0000", "record": "3-2",
                             "owner_display": "BLONDE", "discord_user_id": "480510938571735040",
                             "avatar_url": "https://cdn.discordapp.com/avatars/…", "online": true },
               "room": null },
  "week_games": [ { "fixture_id": 1234, "home": { "team_id": 493, "name": "…", "abbr": "…", "logo_url": null, "color": "#…", "record": "4-1" },
                    "away": { … }, "status": "scheduled|in_progress|live|final", "forfeit": false,
                    "score": { "home": 49, "away": 21 } | null, "series": { "home": 2, "away": 0 } | null,
                    "winner_team_id": 493 | null, "when": null } ],
  "standings": [ { "rank": 1, "team_id": 482, "name": "…", "abbr": "…", "logo_url": null, "record": "5-0" } ],
  "recent":    [ { "fixture_id": 1200, "week": 5, "opponent_abbr": "JOCK", "home": false, "result": "W" | null, "score": "28-21" | null, "series": "2-1" | null } ],
  "leaders":   { "pass": [ { "poid": 12, "name": "…", "value": 14 } ], "rush": [], "rec": [] },
  "leaders_unit": "TD",
  "roster":    [ { "poid": 12, "name": "…", "position": "QB", "number": 7 } ],
  "site_url":  "https://nflstreetreborn.com/league/team.php?league_id=36&team_id=493"
}
```
- Site rules the UI relies on:
  - `matchup.status` ∈ `scheduled | in_progress | final | bye`. On a bye, `fixture_id`, `home`, `score`, `series`, `winner_team_id` and `opponent` are `null`.
  - `in_progress` means a best-of-3 with game 1 recorded. The next game can still be played from the card.
  - `when` is **always null**: the UI never shows a time. A scheduled game reads "This week" (`week_label` in the header).
  - `series` and `score` are `{home, away}`. The launcher turns them into "you / opponent" with `matchup.home`.
  - `leaders` values are **touchdowns**, so the unit comes from `leaders_unit` ("14 TD").
  - `recent.result` can be `null` (a forfeit with no recorded winner), and `recent.score` can be `null`.
  - `logo_url` is site-absolute or `null`. Only `/live-assets/...png` reaches the screen (`assetUrl`); anything else draws the abbreviation badge.
  - `color` is a `#RRGGBB` hex, used as the team-colour accent stripe through the per-element `--tc` property (DESIGN.md §1, "Set per element").
  - `opponent.discord_user_id` may be `null` (an owner who has not linked Discord). The card then says "Opponent hasn't linked Discord" and offers no button.
- ➕ Backend changes and additions (**plan 2 § "Launcher-facing contract", commit 3b60379, is authoritative**):
  - ➕ `status` may also be `"live"` (a league match for that fixture is running). A site `final` with a league room still open becomes `"in_progress"`.
  - ➕ `match_id` (string|null) on `matchup` and every `week_games[]`. While live, `series {home,away}` and `score {home,away}` are the live values (`score` null until a play arrives).
  - ➕ `matchup.room`: `null`, or `{ room_id, code, host_discord_user_id, members, expires_at, state }`.
    - `state` ∈ `waiting` (host alone) | `lobby` (both in) | `in_series` (match running; `room_id` and `code` null).
    - `members` is a **count**.
    - There is no `you_host`. The launcher works it out without knowing its own Discord ID: **the opponent hosts iff `host_discord_user_id === matchup.opponent.discord_user_id`**, otherwise you host.
  - ➕ `matchup.opponent.online` is a boolean, false when the opponent has no Reborn account. `avatar_url` is not sent, so the card draws a letter avatar.
  - ➕ Every non-null `logo_url` becomes `https://api.nflstreetreborn.com/live-assets/league/<team_id>.png`, which `assetUrl()` accepts as is. Null stays null, so badges are expected for Season 4 today.
- Errors:
  - 401 `{ok:false,error:"Signed out. …"}`.
  - 503 `{ok:false,error:"league_unavailable"}` when the site is down. **Never `member:false` on an outage**, so a 503 must not drop the tab.
  - 429 rate limit.

`POST /v1/league/room` (bearer, **no body**; the client sends `{}` because `post_json` needs bytes):

| Reply | Launcher does |
|---|---|
| 200 `{ok:true, action:"created", room:<roomView>}` (also when the caller is already in it) | the caller hosts: go to `online.html` (the room is already this player's mm room) |
| 200 `{ok:true, action:"join", code:"K7QP2M"}` | existing `mm_room_join(code)` → `POST /v1/mm/rooms/:code/join` with `{}`, then `online.html` |
| 409 `{error:"in_series", match_id}` | go to `online.html` (the match is already this player's; the Online page owns rejoin) |
| 403 `not_member` · 409 `no_game` · `already_final` · `opponent_unlinked` | fixed launcher sentence (Task 2) |
| 409 `You are already in a lobby.` / `You are already in a match.` | that exact sentence (both are already player sentences in the mm flow) |
| 503 `league_unavailable` · 429 · anything else | fixed launcher sentence |

**Cancel** has no league route. The host calls the existing `mm_room_action({ roomId, action: 'leave' })` → `POST /v1/mm/rooms/:id/leave`; a host leaving closes the lobby. Ready, field, chat and start are the existing room routes. `kind` is refused on a league room (409 `A league game's type is fixed.`).

Matchmaking snapshot (the existing long poll): `room.league` and `match.league` are `{league_id, fixture_id, week}` or absent. **`match.ruleset` is `"ranked"` for league (and ranked) matches.** League matches are `mode: "casual"`, `best_of: 3`. Today the launcher picks `ranked.s01` only when `mode == "ranked"` (`dolphin_settings.rs:314`), so Task 3 makes it read `ruleset` first. League uploads must carry `intent.league = true` plus the fixture, or `forward.js` drops them (Task 3).

**The integration seam:** joining is the existing `mm_room_join(code)` command (`online_mm.rs:921` → `Client::join_room`). The one call site is `joinLeagueRoom()` in `crates/reborn-ui/ui/league.js`, marked `// INTEGRATION SEAM`. Before Task 6, confirm that `Client::join_room` posts to `/v1/mm/rooms/:code/join` (plan 2's route). If it posts elsewhere, fix it in `matchmaking.rs` with a test, not in the page.

**Deviations from the spec (on purpose, flag in review):**
1. The spec says to parse `discord_user_id` into `PlayerFacts` and store `league_member` in `identity.json`. This plan does neither:
   - The host test compares the room host with the opponent's Discord ID, which is in the payload. So the launcher never needs its own ID, and identity files written before this change would lack it anyway.
   - Membership plus the last payload live in their own `league.json`, removed on sign-out with the identity file.
2. The spec calls the shared league-room button "Rejoin if dropped". Here it is labelled "Open room", and it opens the Online page, which already owns rejoin/relay.
3. The spec's `/v1/league/room/:id/cancel` does not exist (plan 2). Cancel is the existing room leave.

## Review Focus

1. **A non-member opens `league.html` directly** (the address bar is not reachable, but a stale nav link from a cached member answer is). Expected: redirected to `index.html` as soon as `league_home` says `member: false`, and the cache flips so the link disappears on the next page. → Task 4 `pageMode` test + Task 1 `a_non_member_answer_clears_the_cached_team`.
2. **Signed out, or the token expired (401) while the cache says member.** Expected: no stale team shown, the tab disappears, the cache resets. → Task 1 `a_401_is_never_covered_by_the_cache`.
3. **Both players press Play at the same instant.** Expected: one room. The second caller gets `action:"join"` and is joined as the guest; neither sees an error. → Task 1 `open_room_*` tests + Task 4 `cardState` host/guest tests + Task 6 `play()`.
4. **The league site is down (503) for a member.** Expected: last answer drawn with the "League data unavailable" banner; the tab never disappears on an outage. → Task 1 `a_failure_falls_back_to_the_cached_member`, Task 4 `bannerLine`.
5. **A league match boots the ranked seed and uploads as a league game.** A wrong seed is a different game; a missing `intent.league` means the result never reaches the site. → Task 3 `ruleset_ranked_boots_the_ranked_seed`, `a_league_match_is_launched_as_one`, `a_league_upload_names_its_fixture`.

---

## File map

| File | Status | Responsibility |
|---|---|---|
| `crates/reborn-core/src/league_home.rs` | Create | HTTP client for `/v1/league/*`, `league.json` cache, `resolve()` fallback rule |
| `crates/reborn-core/src/lib.rs` | Modify | `pub mod league_home;` |
| `crates/reborn-core/src/live_view.rs` | Modify | `status_error` → `pub(crate)` (reused, not copied) |
| `crates/reborn-core/src/identity.rs` | Modify | `clear_token` also removes `league.json` |
| `crates/reborn-core/src/matchmaking.rs` | Modify | `Room.league`, `MatchView.league` (`Option<Value>`), `MatchView.ruleset` |
| `crates/reborn-core/src/supervisor.rs`, `backend.rs` | Modify | `SessionIntent.league_fixture_id`; upload `intent.fixture_id` |
| `crates/reborn-ui/src/online_mm.rs` | Modify | `current_match_league()`; seed from `ruleset`; snapshot JSON exposes `room.league`, `match.league` |
| `crates/reborn-ui/src/dolphin_settings.rs` | Modify | `match_seed_kind` honours `ruleset`; netplay launch sets `league_match` + fixture |
| `crates/reborn-ui/src/league_tab.rs` | Create | Tauri commands `league_member`, `league_home`, `league_room_open` |
| `crates/reborn-ui/src/main.rs` | Modify | `mod league_tab;` + register the three commands |
| `crates/reborn-ui/ui/league-model.js` | Create | pure view model (card state, scoreboard cells, rows) |
| `crates/reborn-ui/ui/league-model.test.mjs` | Create | node tests for the model |
| `Tools/fixtures/league-home.json` | Create | member payload fixture (tests + page checker) |
| `crates/reborn-ui/ui/shell.js` | Modify | `leagueNav()` injection before `markCurrent`/nav marker |
| `crates/reborn-ui/ui/league-nav.test.mjs` | Create | nav injection test with a stub DOM |
| `crates/reborn-ui/ui/league.html` | Create | page shell (nav includes League, current) |
| `crates/reborn-ui/ui/league.js` | Create | load/poll/draw, Play/Join/Cancel actions |
| `crates/reborn-ui/ui/pages/league.css` | Create | layout: team header, scoreboard strip, This Week card |
| `crates/reborn-ui/ui/theme.css` | Modify | 2 tokens: `--league-cell-w`, `--mine-edge` |
| `crates/reborn-ui/ui/online-mm.js` | Modify | league room: hide the kind switch, title "League · Week N" |
| `crates/reborn-ui/ui/DESIGN.md` | Modify | §1 tokens, §5 sidebar, §8 page table, §12 test table |
| `Tools/check-theme.mjs`, `Tools/check-shell-pages.mjs` | Modify | register the page |

---

### Task 1: Core client `league_home` (fetch, room, cache, fallback)

**Files:**
- Create: `crates/reborn-core/src/league_home.rs`
- Modify: `crates/reborn-core/src/lib.rs` (after `pub mod live_view;` at line 898)
- Modify: `crates/reborn-core/src/live_view.rs:55` (`fn status_error` → `pub(crate) fn status_error`)
- Modify: `crates/reborn-core/src/identity.rs:349` (`clear_token`)

**Interfaces:**
- Produces:
  - `league_home::fetch_home(http: &dyn Http, origin: &str, token: &str) -> Result<Value, LiveError>`
  - `league_home::RoomOpen { Created(Value /* roomView */), Join(String /* code */), InSeries(Option<String> /* match_id */), Refused(String /* server `error` */) }`
  - `league_home::open_room(http, origin, token) -> Result<RoomOpen, LiveError>`
  - `league_home::LeagueCache { member: bool, home: Option<Value>, fetched_at_ms: u64 }`
  - `league_home::cache_path(&Platform) -> platform::Result<PathBuf>`, `load_cache(&Path) -> LeagueCache`, `save_cache(&Path, &LeagueCache)`
  - `league_home::resolve(fetched: Result<Value, LiveError>, cache: &LeagueCache, now_ms: u64) -> (Result<Value, LiveError>, Option<LeagueCache>)`. The second element is the cache to write, or `None` to leave it alone.

- [ ] **Step 1: Write the failing tests** (bottom of the new `league_home.rs`; create the file with only the test module and `use super::*;` so it fails to compile)

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

    const ROOM_URL: &str = "https://api.test/v1/league/room";

    #[test]
    fn open_room_posts_an_empty_body_with_the_bearer() {
        let http = FakeHttp::default().reply(ROOM_URL, 200, r#"{"ok":true,"action":"created","room":{"room_id":"r1","code":"K7QP2M"}}"#);
        assert_eq!(open_room(&http, ORIGIN, "tok").unwrap(), RoomOpen::Created(serde_json::json!({"room_id":"r1","code":"K7QP2M"})));
        let posts = http.posts.lock().unwrap();
        assert_eq!((posts[0].0.as_str(), posts[0].2.as_deref()), (ROOM_URL, Some("tok")));
        assert_eq!(posts[0].1, b"{}".to_vec(), "the client never names the fixture: the server derives it");
    }

    #[test]
    fn the_second_player_is_told_to_join_by_code() {
        let http = FakeHttp::default().reply(ROOM_URL, 200, r#"{"ok":true,"action":"join","code":"K7QP2M"}"#);
        assert_eq!(open_room(&http, ORIGIN, "tok").unwrap(), RoomOpen::Join("K7QP2M".into()));
        let no_code = FakeHttp::default().reply(ROOM_URL, 200, r#"{"ok":true,"action":"join"}"#);
        assert_eq!(open_room(&no_code, ORIGIN, "tok").unwrap_err(), LiveError::BadReply);
        let odd = FakeHttp::default().reply(ROOM_URL, 200, r#"{"ok":true,"action":"teleport"}"#);
        assert_eq!(open_room(&odd, ORIGIN, "tok").unwrap_err(), LiveError::BadReply);
    }

    #[test]
    fn a_running_series_is_not_an_error() {
        let http = FakeHttp::default().reply(ROOM_URL, 409, r#"{"ok":false,"error":"in_series","match_id":"m-1"}"#);
        assert_eq!(open_room(&http, ORIGIN, "tok").unwrap(), RoomOpen::InSeries(Some("m-1".into())));
    }

    #[test]
    fn refusals_carry_the_servers_reason() {
        for (status, body, why) in [
            (403u16, r#"{"ok":false,"error":"not_member"}"#, "not_member"),
            (409, r#"{"ok":false,"error":"opponent_unlinked"}"#, "opponent_unlinked"),
            (409, r#"{"ok":false,"error":"You are already in a lobby."}"#, "You are already in a lobby."),
        ] {
            let http = FakeHttp::default().reply(ROOM_URL, status, body);
            assert_eq!(open_room(&http, ORIGIN, "tok").unwrap(), RoomOpen::Refused(why.into()), "{body}");
        }
    }

    #[test]
    fn other_failures_stay_errors() {
        let not_json = FakeHttp::default().reply(ROOM_URL, 409, "conflict");
        assert_eq!(open_room(&not_json, ORIGIN, "tok").unwrap_err(), LiveError::Server(409));
        let down = FakeHttp::default().reply(ROOM_URL, 503, r#"{"ok":false,"error":"league_unavailable"}"#);
        assert_eq!(open_room(&down, ORIGIN, "tok").unwrap_err(), LiveError::Server(503));
        assert_eq!(open_room(&FakeHttp::default(), ORIGIN, "tok").unwrap_err(), LiveError::Unreachable);
    }
}
```

Add `pub mod league_home;` to `lib.rs` right after `pub mod live_view;`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core league_home`
Expected: compile errors (`fetch_home`, `resolve`, `LeagueCache`, ... not found).

- [ ] **Step 3: Write the implementation** (top of `league_home.rs`, above the tests)

```rust
//! The League tab's client: `GET /v1/league/me` (the signed-in member's team home, or `{ member: false }`) and
//! `POST /v1/league/room` (open this week's league room, or be told the code of the one the opponent opened).
//! Joining and cancelling are the existing matchmaking room routes (`matchmaking.rs`). Values pass through as [`serde_json::Value`] for the same reason as
//! `live_view`: the backend can add a field without a launcher release. Errors are [`LiveError`], so nothing a
//! screen shows can carry a URL, a body or a token.
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

fn parse(reply: crate::net::HttpResponse) -> Result<Value, LiveError> {
    if !reply.is_success() {
        return Err(status_error(reply.status));
    }
    serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)
}

/// `GET {origin}/v1/league/me`. A reply without a boolean `member` is not this endpoint's answer.
pub fn fetch_home(http: &dyn Http, origin: &str, token: &str) -> Result<Value, LiveError> {
    let reply = http.get_text_as(&url(origin, "/v1/league/me"), Some(token)).map_err(|_| LiveError::Unreachable)?;
    let v = parse(reply)?;
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

/// What `POST /v1/league/room` decided (plan 2): this player hosts the room (`Created`), the opponent's room is open
/// (`Join` with its code), the fixture's match is already running (`InSeries`), or a stated refusal (`Refused` with
/// the server's `error`, a code such as `opponent_unlinked` or a room sentence such as "You are already in a lobby.";
/// league_tab.rs decides what a player reads).
#[derive(Debug, Clone, PartialEq)]
pub enum RoomOpen {
    Created(Value),
    Join(String),
    InSeries(Option<String>),
    Refused(String),
}

/// `POST {origin}/v1/league/room`. The body is empty: the server works out the fixture and the opponent from the
/// bearer, never from anything this launcher says.
pub fn open_room(http: &dyn Http, origin: &str, token: &str) -> Result<RoomOpen, LiveError> {
    let reply = http.post_json(&url(origin, "/v1/league/room"), b"{}", Some(token)).map_err(|_| LiveError::Unreachable)?;
    if matches!(reply.status, 403 | 409) {
        let body: Value = serde_json::from_str(&reply.body).map_err(|_| status_error(reply.status))?;
        let why = body.get("error").and_then(Value::as_str).ok_or(status_error(reply.status))?;
        return Ok(if why == "in_series" {
            RoomOpen::InSeries(body.get("match_id").and_then(Value::as_str).map(str::to_string))
        } else {
            RoomOpen::Refused(why.to_string())
        });
    }
    let body = parse(reply)?;
    match body.get("action").and_then(Value::as_str) {
        Some("created") => body.get("room").cloned().map(RoomOpen::Created).ok_or(LiveError::BadReply),
        Some("join") => body.get("code").and_then(Value::as_str).filter(|c| !c.is_empty()).map(|c| RoomOpen::Join(c.to_string())).ok_or(LiveError::BadReply),
        _ => Err(LiveError::BadReply),
    }
}
```

In `live_view.rs:55` change `fn status_error(` to `pub(crate) fn status_error(`.

In `identity.rs` `clear_token`, insert immediately before `let path = identity_path(platform).map_err(...)` (line 349):

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

- [ ] **Step 4: Run the tests to verify they pass**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core league_home signing_out_forgets_the_league_tab the_retired_league_uploader`
Expected: all PASS (the last one proves the module name did not trip the retired-uploader guard).

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/league_home.rs crates/reborn-core/src/lib.rs crates/reborn-core/src/live_view.rs crates/reborn-core/src/identity.rs
git commit -m "core: league_home client (/v1/league/me, room open/cancel, league.json cache)"
```

---

### Task 2: Tauri commands `league_tab.rs`

**Files:**
- Create: `crates/reborn-ui/src/league_tab.rs`
- Modify: `crates/reborn-ui/src/main.rs`: add `mod league_tab;` beside `mod live_view;`, and add the three commands to `tauri::generate_handler![...]` (line ~6551) after `live_view::stats_dashboard,`

**Interfaces:**
- Consumes: Task 1's `league_home::{fetch_home, resolve, load_cache, save_cache, cache_path, open_room, RoomOpen}`, the existing `identity::{status, token, origin}` and `crate::http_client()`.
- Produces (JS `call()` names → result):
  - `league_member` → `bool`. Cache only, no network.
  - `league_home` → `{ signed_in: false }` | `{ signed_in: true, ok: true, member: false }` | `{ signed_in: true, member: true, stale: bool, ...payload }`. Rejects `{ kind, message }`.
  - `league_room_open` → `{ action: "created", room }` | `{ action: "join", code }` | `{ action: "in_series", match_id }`. Rejects `{ kind, message }`.
- Cancel is NOT a command here: the page calls the existing `mm_room_action({ roomId, action: 'leave' })`.

- [ ] **Step 1: Write the failing tests** (test module at the bottom of the new `league_tab.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use reborn_core::league_home::RoomOpen;

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
    fn room_answers_become_page_actions() {
        assert_eq!(room_view(RoomOpen::Created(serde_json::json!({"code":"K"}))).unwrap(), serde_json::json!({"action":"created","room":{"code":"K"}}));
        assert_eq!(room_view(RoomOpen::Join("K7QP2M".into())).unwrap(), serde_json::json!({"action":"join","code":"K7QP2M"}));
        assert_eq!(room_view(RoomOpen::InSeries(Some("m".into()))).unwrap(), serde_json::json!({"action":"in_series","match_id":"m"}));
    }

    #[test]
    fn room_refusals_say_why_in_our_words() {
        let msg = |why: &str| room_view(RoomOpen::Refused(why.into())).unwrap_err().message;
        assert_eq!(msg("not_member"), NOT_YOUR_GAME);
        assert_eq!(msg("no_game"), NO_GAME_NOW);
        assert_eq!(msg("already_final"), ALREADY_FINAL);
        assert_eq!(msg("opponent_unlinked"), OPPONENT_UNLINKED);
        // The two room sentences the matchmaking flow already shows players pass through as they are.
        assert_eq!(msg("You are already in a lobby."), "You are already in a lobby.");
        assert_eq!(msg("You are already in a match."), "You are already in a match.");
        // Anything else is never shown raw.
        assert_eq!(msg("<b>stack trace</b>"), ROOM_FAILED);
    }

    #[test]
    fn room_transport_failures_are_fixed_sentences() {
        assert_eq!(room_error(LiveError::Server(401)).message, SIGN_IN_FIRST);
        assert_eq!(room_error(LiveError::Server(503)).message, ROOM_FAILED);
        assert_eq!(room_error(LiveError::RateLimited).kind, "rate_limited");
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
Expected: compile errors (`home_error`, `room_view`, constants not found).

- [ ] **Step 3: Write the implementation** (top of `league_tab.rs`)

```rust
//! The League tab's commands (spec 2026-10-08 §3, contract = plan 2): whether to draw the tab (`league_member`, from
//! the cache only), the team home (`league_home`), and this week's league room (`league_room_open`). Joining and
//! cancelling are NOT here: the page calls the existing `mm_room_join` (by code) and `mm_room_action` (`leave`), so a
//! league room runs through the same room -> match -> Dolphin path as every private room.
//!
//! Every failure is `{ kind, message }` (`LiveCommandError`), with `message` one of the fixed sentences below.

use reborn_core::identity;
use reborn_core::league_home::{self, LeagueCache, RoomOpen};
use reborn_core::live_view::LiveError;
use reborn_core::platform::Platform;

use crate::live_view::LiveCommandError;

const NOT_LOADED: &str = "League data couldn't be loaded right now.";
const BUSY: &str = "The league server is busy. Trying again in a minute.";
const NO_GAME_NOW: &str = "There's no league game for you this week.";
const ALREADY_FINAL: &str = "This week's series is already final.";
const OPPONENT_UNLINKED: &str = "Your opponent hasn't linked Discord on the league site yet.";
const NOT_YOUR_GAME: &str = "This league game belongs to the two teams playing it.";
const SIGN_IN_FIRST: &str = "Sign in with Discord on the Play page first.";
const ROOM_FAILED: &str = "The league game couldn't be opened. Try again.";
/// Room sentences the matchmaking server already words for players (plan 2: "existing room sentences").
const PASS_THROUGH: [&str; 2] = ["You are already in a lobby.", "You are already in a match."];

fn err(kind: &'static str, message: &str) -> LiveCommandError {
    LiveCommandError { kind, message: message.to_string() }
}

fn home_error(e: LiveError) -> LiveCommandError {
    match e {
        LiveError::RateLimited => err("rate_limited", BUSY),
        _ => err("unavailable", NOT_LOADED),
    }
}

fn room_error(e: LiveError) -> LiveCommandError {
    match e {
        LiveError::RateLimited => err("rate_limited", BUSY),
        LiveError::Server(401) => err("unavailable", SIGN_IN_FIRST),
        _ => err("unavailable", ROOM_FAILED),
    }
}

fn room_view(open: RoomOpen) -> Result<serde_json::Value, LiveCommandError> {
    match open {
        RoomOpen::Created(room) => Ok(serde_json::json!({ "action": "created", "room": room })),
        RoomOpen::Join(code) => Ok(serde_json::json!({ "action": "join", "code": code })),
        RoomOpen::InSeries(match_id) => Ok(serde_json::json!({ "action": "in_series", "match_id": match_id })),
        RoomOpen::Refused(why) => Err(err("unavailable", match why.as_str() {
            "not_member" => NOT_YOUR_GAME,
            "no_game" => NO_GAME_NOW,
            "already_final" => ALREADY_FINAL,
            "opponent_unlinked" => OPPONENT_UNLINKED,
            s if PASS_THROUGH.contains(&s) => s,
            _ => ROOM_FAILED,
        })),
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

#[tauri::command]
pub async fn league_room_open() -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        let Some(token) = bearer(&p) else { return Err(err("unavailable", SIGN_IN_FIRST)) };
        let origin = identity::origin(&p).map_err(|_| err("unavailable", ROOM_FAILED))?;
        room_view(league_home::open_room(&crate::http_client(), &origin, &token).map_err(room_error)?)
    })
    .await
    .map_err(|_| err("unavailable", ROOM_FAILED))?
}
```

`LiveCommandError`'s fields are already `pub` (`live_view.rs:25-28`), and `main.rs` declares `mod live_view;`, so `crate::live_view::LiveCommandError` resolves.

Register in `main.rs`'s `generate_handler!` list, right after `live_view::stats_dashboard,`:

```rust
            league_tab::league_member,
            league_tab::league_home,
            league_tab::league_room_open,
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-ui league_tab nothing_the_online_page_can_call`
Expected: PASS. The second filter is `main.rs`'s existing raw-error guard; it must stay green.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/src/league_tab.rs crates/reborn-ui/src/main.rs
git commit -m "ui: league_member / league_home / league_room_open commands"
```

---

### Task 3: League matches boot the ranked seed and upload as league games

Plan 2: a league match is `mode: "casual"`, `best_of: 3`, `ruleset: "ranked"`, with `match.league = {league_id, fixture_id, week}`. Two launcher changes are required, or the feature is broken end to end:
- **Seed.** `match_seed_kind` keys on `mode == "ranked"` (`dolphin_settings.rs:314`), so today a league match would boot `regular.s01` (the wrong settings) and the server's seed comparison would hold it. It must key on `ruleset` first.
- **Upload intent.** `forward.js` forwards only uploads with `intent.league = true`, and plan 2 wants the fixture beside it.

**Files:**
- Modify: `crates/reborn-core/src/matchmaking.rs:113-128` (`Room`), `:160-196` (`MatchView`)
- Modify: `crates/reborn-core/src/supervisor.rs:164-171` (`SessionIntent`) and its literal at `:837`
- Modify: `crates/reborn-core/src/backend.rs:18-27` (`SessionFacts`), `:29-49` (`session_facts`), `:81` (envelope), test literals at `:384`, `:402`, `:412`
- Modify: `crates/reborn-core/src/session.rs:1287` (literal: add `league_fixture_id: None`)
- Modify: `crates/reborn-ui/src/main.rs:5276` (literal: add `league_fixture_id: None`)
- Modify: `crates/reborn-ui/src/online_mm.rs` (after `current_match_id()` at :801; call sites `:603`, `:626`; `snapshot_json` `:841-876`)
- Modify: `crates/reborn-ui/src/dolphin_settings.rs:422,451` (netplay launch)
- Modify: `crates/reborn-ui/ui/online-mm.js:650-659` (room card)

**Interfaces:**
- Consumes: snapshot `room.league`, `match.league`, `match.ruleset` (plan 2).
- Produces:
  - `MatchView.ruleset: Option<String>`, `MatchView.league` and `Room.league: Option<serde_json::Value>`
  - `SessionIntent.league_fixture_id: Option<u64>`, `SessionFacts.league_fixture_id: Option<u64>`
  - `online_mm::seed_mode(m: &MatchView) -> &str` (`"ranked"` when `ruleset == "ranked"`, else `m.mode`)
  - `online_mm::current_match_league() -> Option<u64>` (the running match's `league.fixture_id`)
  - upload envelope `intent.fixture_id` (number) when the session was a league match with a fixture

Run `grep -rn "SessionIntent {" crates/` and `grep -rn "SessionFacts {" crates/` before Step 3. Every struct literal it prints needs the new field, and the list above is what exists at d37500a.

- [ ] **Step 1: Write the failing tests**

`matchmaking.rs` test module:

```rust
    #[test]
    fn a_league_room_and_match_keep_their_tag_and_ruleset() {
        let s: Snapshot = serde_json::from_str(r#"{"room":{"id":"r","code":"C","league":{"league_id":36,"fixture_id":1234,"week":6}},
            "match":{"id":"m","mode":"casual","ruleset":"ranked","league":{"league_id":36,"fixture_id":1234,"week":6}}}"#).unwrap();
        assert_eq!(s.room.unwrap().league.unwrap()["fixture_id"], 1234);
        let m = s.matched.unwrap();
        assert_eq!((m.ruleset.as_deref(), m.league.unwrap()["week"].as_u64()), (Some("ranked"), Some(6)));
        let plain: Snapshot = serde_json::from_str(r#"{"room":{"id":"r"},"match":{"id":"m"}}"#).unwrap();
        assert!(plain.room.unwrap().league.is_none());
        let pm = plain.matched.unwrap();
        assert!(pm.league.is_none() && pm.ruleset.is_none());
    }
```

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

`online_mm.rs` test module:

```rust
    #[test]
    fn ruleset_ranked_boots_the_ranked_seed() {
        let m: reborn_core::matchmaking::MatchView = serde_json::from_str(r#"{"id":"m","mode":"casual","ruleset":"ranked","seed_kind":"regular"}"#).unwrap();
        assert_eq!(seed_mode(&m), "ranked");
        assert_eq!(crate::dolphin_settings::match_seed_kind(m.seed_kind.as_deref(), Some(seed_mode(&m))), "ranked");
        let casual: reborn_core::matchmaking::MatchView = serde_json::from_str(r#"{"id":"m","mode":"casual","seed_kind":"regular"}"#).unwrap();
        assert_eq!(crate::dolphin_settings::match_seed_kind(casual.seed_kind.as_deref(), Some(seed_mode(&casual))), "regular");
        let ranked: reborn_core::matchmaking::MatchView = serde_json::from_str(r#"{"id":"m","mode":"ranked"}"#).unwrap();
        assert_eq!(seed_mode(&ranked), "ranked", "an older server without ruleset still boots ranked by mode");
    }

    #[test]
    fn every_seed_choice_reads_the_ruleset() {
        let src = include_str!("online_mm.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        assert!(!production.contains("Some(&m.mode)"), "a seed is still picked from mode alone");
    }

    #[test]
    fn a_league_match_is_launched_as_one() {
        let snap: reborn_core::matchmaking::Snapshot = serde_json::from_str(
            r#"{"match":{"id":"m1","relay_state":"launching","league":{"league_id":36,"fixture_id":1234,"week":6}}}"#).unwrap();
        lock(&STATE).snapshot = Some(snap);
        assert_eq!(current_match_league(), Some(1234));
        let snap: reborn_core::matchmaking::Snapshot = serde_json::from_str(r#"{"match":{"id":"m1","relay_state":"launching"}}"#).unwrap();
        lock(&STATE).snapshot = Some(snap);
        assert_eq!(current_match_league(), None);
        lock(&STATE).snapshot = None;
        assert_eq!(current_match_league(), None);
    }
```

(If the existing `online_mm` tests put snapshots into `STATE` through a helper, use that helper instead of `lock(&STATE).snapshot = ...`.)

`dolphin_settings.rs` test module:

```rust
    #[test]
    fn a_netplay_launch_records_whether_it_is_a_league_match() {
        let src = include_str!("dolphin_settings.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("the test module moved")];
        assert!(production.contains("let league=crate::online_mm::current_match_league();"));
        assert!(production.contains("league_match:league.is_some()"));
        assert!(production.contains("league_fixture_id:league"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core a_league_room_and_match a_league_upload session_facts_carry` and `CARGO_BUILD_JOBS=1 cargo test -p reborn-ui ruleset_ranked every_seed_choice a_league_match_is_launched a_netplay_launch_records`
Expected: compile errors (no field `league` / `ruleset` / `league_fixture_id`; no `seed_mode` / `current_match_league`).

- [ ] **Step 3: Write the implementation**

`matchmaking.rs`, last field of `Room` (after `capacity`):

```rust
    /// `{ league_id, fixture_id, week }` when this is a league room (spec 2026-10-08), else `None`. The server owns
    /// what it means; the launcher only shows it.
    pub league: Option<serde_json::Value>,
```

Last fields of `MatchView` (after `party`):

```rust
    /// The rules this match plays by: `"ranked"` for ranked and league matches (a league match is mode casual, best of
    /// 3, ranked rules), `"casual"` otherwise. `None` from an older server, which then means "by mode".
    pub ruleset: Option<String>,
    /// `{ league_id, fixture_id, week }` for a league match, else `None`.
    pub league: Option<serde_json::Value>,
```

`supervisor.rs` `SessionIntent`, after `match_id`:

```rust
    /// The league fixture this session played, when it was a league match (spec 2026-10-08). Goes up as
    /// `intent.fixture_id` beside `intent.league`. Absent for everything else and every record an older build wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub league_fixture_id: Option<u64>,
```

Add `league_fixture_id: None` to the `SessionIntent { ... }` literals at `supervisor.rs:837`, `session.rs:1287`, `main.rs:5276` and `backend.rs:402`.

`backend.rs` `SessionFacts`, after `match_id`:

```rust
    /// The league fixture, from the intent, when this was a league match.
    pub league_fixture_id: Option<u64>,
```

In `session_facts`, after `match_id: ...`:

```rust
                league_fixture_id: r.intent.as_ref().and_then(|i| i.league_fixture_id),
```

Add `league_fixture_id: None` to the test literals at `backend.rs:384` and `:412`.

In `envelope`, right after the `let mut env = serde_json::json!({ ... });` statement:

```rust
    // A league game names its fixture beside `intent.league` (plan 2: forward.js reads both). Only for a league game.
    if let Some(fixture) = facts.filter(|f| f.league_match).and_then(|f| f.league_fixture_id) {
        env["intent"]["fixture_id"] = serde_json::Value::from(fixture);
    }
```

`online_mm.rs`, right after `current_match_id()`:

```rust
/// The mode a match's seed is chosen by: `ranked` when the server says the match plays by ranked rules (a league
/// match is mode casual with `ruleset: "ranked"`, plan 2), else the match's mode. An older server without `ruleset`
/// falls through to its mode, so ranked still boots ranked.
pub(crate) fn seed_mode(m: &reborn_core::matchmaking::MatchView) -> &str {
    if m.ruleset.as_deref().map(str::trim) == Some("ranked") { "ranked" } else { &m.mode }
}

/// The running match's league fixture, when it is a league match: its results then go up with
/// `intent.league = true` and the fixture, which `forward.js` needs to send them to the league site.
pub fn current_match_league() -> Option<u64> {
    let s = lock(&STATE);
    s.snapshot.as_ref()?.matched.as_ref()?.league.as_ref()?.get("fixture_id")?.as_u64()
}
```

At both seed call sites (`online_mm.rs:603` and `:626`), replace `Some(&m.mode)` with `Some(seed_mode(m))`.

In `snapshot_json`, add `"league": r.league,` to the `room` object (after `"capacity": ...`) and `"league": m.league, "ruleset": m.ruleset,` to the `match` object (after `"hold_reason": m.hold_reason, "seat": m.seat,`).

`dolphin_settings.rs`, netplay launch (keep this file's compact style; the source-scan test matches it exactly):
- Just before `let req=LaunchRequest{...}` at line 422, add `let league=crate::online_mm::current_match_league();`.
- In that `LaunchRequest`, replace `league_match:false` with `league_match:league.is_some()`.
- At line 451, replace `Some(SessionIntent{league_match:req.league_match,match_id:session_match.clone()})` with `Some(SessionIntent{league_match:req.league_match,match_id:session_match.clone(),league_fixture_id:league})`.

`online-mm.js`, room card. A league room has no kind switch (the server refuses it) and says where it comes from:

```js
  const league = r.league && typeof r.league === 'object' ? r.league : null;
  const kindControl = r.you_host === true && !league
```

Replace the room card's title line with:

```js
    el('div', { class: 'on-card-h' }, el('h2', { text: league ? `League game · Week ${league.week ?? '?'}` : `Your room · ${kind}` }),
      league ? el('span', { text: 'Ranked rules · Best of 3' }) : r.mode === 'ranked' ? el('span', { text: 'Ranked' }) : null),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `CARGO_BUILD_JOBS=1 cargo test -p reborn-core`, then `CARGO_BUILD_JOBS=1 cargo test -p reborn-ui`, then `node --test crates/reborn-ui/ui/online-model.test.mjs`
Expected: all PASS, including the existing `match_seed_kind` tests at `dolphin_settings.rs:205-209`, which are unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/matchmaking.rs crates/reborn-core/src/supervisor.rs crates/reborn-core/src/backend.rs crates/reborn-core/src/session.rs crates/reborn-ui/src/main.rs crates/reborn-ui/src/online_mm.rs crates/reborn-ui/src/dolphin_settings.rs crates/reborn-ui/ui/online-mm.js
git commit -m "online: league matches boot ranked.s01 (ruleset) and upload intent.league + fixture"
```

---

### Task 4: Page model `league-model.js` + fixture

**Files:**
- Create: `Tools/fixtures/league-home.json`
- Create: `crates/reborn-ui/ui/league-model.js`
- Test: `crates/reborn-ui/ui/league-model.test.mjs`

**Interfaces:**
- Consumes: the `league_home` answer (Task 2 shape = the contract above + `signed_in` + `stale`).
- Produces (all pure, no DOM):
  - `pageMode(home) -> 'signed_out' | 'redirect' | 'member'`
  - `headerLine(home) -> string`
  - `bannerLine(home) -> string | null`
  - `teamColor(team) -> string | null`: a `#RRGGBB` or null, the only colour that reaches `--tc`
  - `cardState(home) -> { kind: 'bye'|'final'|'live'|'waiting'|'lobby'|'join'|'play'|'unlinked', line, action, cancel, room }`
  - `scoreCells(home) -> [{ key, mine, status, statusText, away: Side, home: Side }]`, where `Side = { abbrev, name, record, logo_url, color, score, winner }`
  - `standingsRows(home) -> [{ rank, name, abbrev, record, mine }]`
  - `recentRows(home) -> [{ result: 'W'|'L'|null, text }]`
  - `leaderCols(home) -> [{ label, rows: [{ name, value }] }]`
  - `rosterRows(home) -> [{ key, number, name, position }]`
  - `ordinal(n) -> string`

- [ ] **Step 1: Write the fixture** `Tools/fixtures/league-home.json`. It is the site contract's member example, filled in, plus the backend's `room`, `avatar_url` and `online`.

```json
{
  "ok": true,
  "signed_in": true,
  "member": true,
  "stale": false,
  "league": { "id": 36, "name": "Reborn Season 4", "season": 4, "week": 6, "week_label": "Week 6" },
  "team": { "id": 493, "name": "Subway Surfers", "abbr": "ADAM", "logo_url": "/live-assets/league/493.png", "color": "#00A3FF", "record": "4-1", "standing": 2 },
  "matchup": {
    "fixture_id": 1234, "week": 6, "home": true, "status": "scheduled",
    "score": null, "series": null, "winner_team_id": null, "when": null,
    "opponent": { "team_id": 486, "name": "Football Furies", "abbr": "BLONDE", "logo_url": null, "color": "#FF0000", "record": "3-2",
                  "owner_display": "BLONDE", "discord_user_id": "480510938571735040",
                  "avatar_url": "https://cdn.discordapp.com/avatars/1/a.png", "online": true },
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
      "status": "in_progress", "forfeit": false, "score": { "home": 14, "away": 21 }, "series": { "home": 0, "away": 1 }, "winner_team_id": null, "when": null },
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
    { "fixture_id": 1200, "week": 5, "opponent_abbr": "JOCK", "home": false, "result": "W", "score": "28-21", "series": "2-1" },
    { "fixture_id": 1190, "week": 4, "opponent_abbr": "GHO", "home": true, "result": "L", "score": null, "series": "0-2" },
    { "fixture_id": 1180, "week": 3, "opponent_abbr": "RAIL", "home": true, "result": null, "score": null, "series": null }
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

- [ ] **Step 2: Write the failing tests** `crates/reborn-ui/ui/league-model.test.mjs`

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pageMode, headerLine, bannerLine, teamColor, cardState, scoreCells, standingsRows, recentRows, leaderCols, rosterRows, ordinal } from './league-model.js';

const FIX = JSON.parse(readFileSync(new URL('../../../Tools/fixtures/league-home.json', import.meta.url), 'utf8'));
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

test('cardState: play, waiting, join, final, bye', () => {
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

test('cardState: in_progress keeps the series score and still plays game 2', () => {
  const s = cardState(withMatchup({ status: 'in_progress', series: { home: 1, away: 0 } }));
  assert.deepEqual([s.kind, s.line, s.action], ['play', 'Series · You lead 1-0', 'Play league game']);
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
  // The server refuses a room for an unlinked opponent (409 opponent_unlinked), so no room can exist here.
});

test('cardState: a room with no code is no room; names fall back', () => {
  assert.equal(cardState(withMatchup({ room: { room_id: 'r1', host_discord_user_id: OPP, state: 'waiting' } })).kind, 'play');
  const h = withMatchup({ room: THEIRS });
  h.matchup.opponent.owner_display = '';
  assert.equal(cardState(h).action, "Join @Football Furies's game");
});

test('scoreCells: mine first, never a time, series and forfeit lines, winners', () => {
  const cells = scoreCells(FIX);
  assert.deepEqual(cells.map((c) => c.key), [1234, 1230, 1231, 1232]);
  const [mine, fin, prog, ff] = cells;
  assert.deepEqual([mine.mine, mine.status, mine.statusText, mine.home.score], [true, 'scheduled', 'This week', null]);
  assert.deepEqual([fin.status, fin.statusText, fin.home.score, fin.away.score, fin.home.winner, fin.away.winner], ['final', 'FINAL · 0-2', 49, 21, true, false]);
  assert.deepEqual([prog.status, prog.statusText, prog.home.winner, prog.away.winner], ['in_progress', 'SERIES · 1-0', false, false]);
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
  ]);
  assert.deepEqual(leaderCols(FIX).map((c) => [c.label, c.rows[0]?.name ?? null, c.rows[0]?.value ?? null]),
    [['Passing TD', 'Michael Vick', '14 TD'], ['Rushing TD', 'Barry Sanders', '1,204 TD'], ['Receiving TD', null, null]]);
  const yards = clone(); yards.leaders_unit = undefined;
  assert.equal(leaderCols(yards)[0].rows[0].value, '14', 'no unit from the server: the bare number');
  assert.deepEqual(rosterRows(FIX)[0], { key: 12, number: '7', name: 'Michael Vick', position: 'QB' });
  assert.deepEqual([recentRows({}), leaderCols({}).map((c) => c.rows.length), rosterRows({})], [[], [0, 0, 0], []]);
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `node --test crates/reborn-ui/ui/league-model.test.mjs`
Expected: FAIL, `Cannot find module './league-model.js'`.

- [ ] **Step 4: Write the implementation** `crates/reborn-ui/ui/league-model.js`

```js
// The League tab's words and rows (spec 2026-10-08 §3; data = plan 1's site contract + plan 2's room/live fields),
// pure: no DOM, no calls. league.js draws what this returns. The site stores no kickoff time (`when` is always null),
// so nothing here ever prints a time. Series and scores arrive as {home, away}; `matchup.home` turns them into you/opp.

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
  const s = mine(m.series, m.home);
  if (m.winner_team_id === null || m.winner_team_id === undefined) return 'Final';
  const won = m.winner_team_id === teamId;
  return `Final · ${won ? 'Won' : 'Lost'} the series${s ? ` ${s[0]}-${s[1]}` : ''}`;
}

const join = (...parts) => parts.filter(Boolean).join(' · ');

/** The This Week card. A room only counts when it has a code to join. */
export function cardState(home) {
  const m = home?.matchup;
  const base = { line: '', action: null, cancel: false, room: null };
  if (!m || m.status === 'bye' || !m.opponent) return { ...base, kind: 'bye', line: 'No game this week' };
  if (m.status === 'final') return { ...base, kind: 'final', line: finalLine(m, home?.team?.id) };
  const s = mine(m.series, m.home);
  const who = oppName(m);
  if (m.status === 'live' || m.room?.state === 'in_series') return { ...base, kind: 'live', line: s ? `LIVE · ${standing(...s)}` : 'LIVE', action: 'Open room', room: m.room || null };
  const series = m.status === 'in_progress' && s ? `Series · ${standing(...s)}` : '';
  // A room only counts with a code to join. Plan 2 sends no `you_host`: the opponent hosts exactly when the room's host
  // is the opponent's Discord ID, which the payload carries; otherwise this player opened it.
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

function side(t, score, won) {
  return { abbrev: str(t?.abbr), name: str(t?.name) || str(t?.abbr), record: str(t?.record), logo_url: str(t?.logo_url) || null,
    color: teamColor(t), score, winner: won };
}

/** The week scoreboard strip: your game first, then the server's order. Rows read away over home, so every pair is
 *  printed away-home. A row with no fixture or teams is dropped. */
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
git add Tools/fixtures/league-home.json crates/reborn-ui/ui/league-model.js crates/reborn-ui/ui/league-model.test.mjs
git commit -m "ui: league page model + fixture (site contract: in_progress, forfeits, TD leaders, unlinked opponent)"
```

---

### Task 5: Nav injection for members only

**Files:**
- Modify: `crates/reborn-ui/ui/shell.js` (new export + one call inside the existing `if (bridge) { ... }` startup block, lines 170-177)
- Test: `crates/reborn-ui/ui/league-nav.test.mjs`

**Interfaces:**
- Produces: `export function addLeagueLink(doc) -> boolean` (true when it inserted). The startup block calls it only when `league_member` answers `true`.

- [ ] **Step 1: Write the failing test** `crates/reborn-ui/ui/league-nav.test.mjs`

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';

// shell.js runs a startup block when `window.__TAURI__` exists; the test has none, so importing it is side-effect free
// apart from the look.js import, which fails quietly (caught) outside a page.
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

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test crates/reborn-ui/ui/league-nav.test.mjs`
Expected: FAIL, `addLeagueLink is not a function` (or not exported).

- [ ] **Step 3: Write the implementation** in `shell.js`, above `// Disk preferences are authoritative ...` (line 168):

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

Then inside the existing `if (bridge) { ... }` block, after the `try { ... settings_paths ... } catch` and **before** the closing `}` (so it runs before any page's `markCurrent()` and before `look.js` mounts the nav marker):

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

### Task 6: The League page

**Files:**
- Create: `crates/reborn-ui/ui/league.html`
- Create: `crates/reborn-ui/ui/league.js`
- Create: `crates/reborn-ui/ui/pages/league.css`
- Modify: `crates/reborn-ui/ui/theme.css` (2 tokens in `:root`)
- Modify: `Tools/check-theme.mjs:6,9-12` (`PAGES` + `THEMED`)
- Modify: `Tools/check-shell-pages.mjs:161-167` (`PAGES`)
- Modify: `crates/reborn-ui/ui/DESIGN.md` (§1 table, §5 sidebar line, §8 page table, §12 test table)

**Interfaces:**
- Consumes: commands `league_home`, `league_room_open` (Task 2), `mm_room_join` and `mm_room_action` (existing, `online_mm.rs:921,938`), `open_website` (existing); model functions (Task 4); `card`, `cols` from `stats-cards.js`; `teamLogo`, `avatar` from `match-rows.js`; `makePoller` from `live-common.js`; `createSignIn` from `sign-in.js`.

- [ ] **Step 1: Register the page in the checkers first (the failing check)**

`Tools/check-theme.mjs`: add `"league"` to `PAGES` (keep alphabetical: after `"index"`... the list is not sorted; append it) and `league: "pages/league.css",` to `THEMED`.
`Tools/check-shell-pages.mjs`: add `["crates/reborn-ui/ui/league.js", "League"],` to `PAGES`.

Run: `node Tools/check-theme.mjs`
Expected: FAIL, `ENOENT ... league.html`.

- [ ] **Step 2: Write `league.html`** (LF line endings). It is a copy of `stats.html`'s shell with the League link in the nav (current), its own heading, and `league.js`:

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
    <!-- Copy lines 15-17 of stats.html verbatim (brand, Play, Mods, Music, Controllers, Online). -->
    <a href="league.html" class="current" aria-current="page"><svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 4 6v6c0 4.4 3.4 8.3 8 9 4.6-.7 8-4.6 8-9V6l-8-3Z"/><path d="m9 12 2 2 4-4"/></svg><span>League</span></a>
    <!-- Then stats.html line 18 WITHOUT class="current" aria-current="page", then line 19 (nav-bottom) verbatim. -->
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

(The two HTML comments are instructions to the implementer: replace each with the exact lines from `stats.html` named in it, and do not ship the comments.)

- [ ] **Step 3: Write `league.js`**

```js
// The League tab (spec 2026-10-08 §3): the league site's homepage for your team. Members only: league_home answers
// member:false for anyone else, and this page sends them to Play. Everything is server data, drawn with el()'s
// `text` only; images only through teamLogo()/avatar() (assetUrl / avatarSrc, the two prefixes the CSP allows).
// Playing this week's game opens (or finds) the league room on the server, joins it by code through the existing
// private-room path, and hands over to the Online page, which runs the room -> match -> Dolphin flow it always has.
import { call, el, markCurrent, quietCall, reason, wireCoreFold } from './shell.js';
import { makePoller } from './live-common.js';
import { card, cols } from './stats-cards.js';
import { avatar, teamLogo } from './match-rows.js';
import { createSignIn } from './sign-in.js';
import { bannerLine, cardState, headerLine, leaderCols, pageMode, recentRows, rosterRows, scoreCells, standingsRows, teamColor } from './league-model.js';

const REFRESH_MS = 60000;
const screen = document.getElementById('screen');
const state = { home: null, error: null, busy: false, actionError: null };
const signIn = createSignIn({ onChange: () => draw(), onDone: () => poller.start() });
const logoTeam = (t) => ({ name: t?.name || t?.abbrev || t?.abbr, abbrev: t?.abbrev || t?.abbr, logo_url: t?.logo_url });
// The team-colour accent stripe: the per-element `--tc` (DESIGN.md §1), set through the CSSOM, only from an exact
// #RRGGBB (teamColor / the model's `color`). Without one the stylesheet falls back to --team-fallback.
const tinted = (node, color) => { if (color) node.style.setProperty('--tc', color); return node; };

async function load() {
  try {
    const home = await quietCall('league_home');
    state.home = home;
    state.error = null;
    if (pageMode(home) === 'redirect') { location.replace('index.html'); return; }
  } catch (e) {
    state.error = reason(e);
    draw();
    throw e; // makePoller backs off (and waits a full minute after a rate limit)
  }
  draw();
}
const poller = makePoller(load, REFRESH_MS);

// INTEGRATION SEAM (plan 2): a league room is joined like any private room, by its code. If the backend ever needs a
// league-specific join, it changes here and nowhere else.
async function joinLeagueRoom(code) {
  await call('mm_room_join', { code });
}

async function act(fn) {
  if (state.busy) return;
  state.busy = true; state.actionError = null; draw();
  try { await fn(); }
  catch (e) { state.actionError = reason(e); }
  finally { state.busy = false; draw(); }
}

// league_room_open (plan 2): `created` = this player hosts and is already in the room; `join` = the opponent's room is
// open, join it by code; `in_series` = the fixture's match is already running and is this player's. In every case the
// Online page takes over (room, ready, relay, rejoin).
const play = () => act(async () => {
  const r = await call('league_room_open');
  if (r.action === 'join') await joinLeagueRoom(r.code);
  location.href = 'online.html';
});
const join = (room) => act(async () => { await joinLeagueRoom(room.code); location.href = 'online.html'; });
// Plan 2 has no league cancel route: the host leaves the room (`POST /v1/mm/rooms/:id/leave`), which closes it.
const cancel = (room) => act(async () => { await call('mm_room_action', { roomId: room.room_id, action: 'leave' }); await load(); });

function teamHeader(home) {
  const t = home.team || {};
  const site = typeof home.site_url === 'string' && home.site_url.startsWith('https://nflstreetreborn.com/')
    ? el('button', { type: 'button', class: 'btn quiet sm', text: 'View on site ›', onclick: () => call('open_website', { url: home.site_url }).catch(() => {}) })
    : null;
  return tinted(el('header', { class: 'glass lg-head' },
    teamLogo(logoTeam(t), 'lg-head-logo'),
    el('div', { class: 'lg-head-text' }, el('span', { class: 'eyebrow', text: home.league?.name || 'League' }), el('h1', { text: t.name || 'Your team' }),
      el('p', { class: 'lg-head-line num', text: headerLine(home) })),
    site), teamColor(t));
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
  const m = home.matchup || {};
  const opp = m.opponent || {};
  const vsTeam = (t) => tinted(el('div', { class: 'lg-vs-team' }, teamLogo(logoTeam(t), 'lg-vs-logo'), el('b', { text: t?.abbr || '' }), el('span', { class: 'num', text: t?.record || '' })), teamColor(t));
  const vs = s.kind === 'bye' ? null : el('div', { class: 'lg-vs' }, vsTeam(home.team), el('span', { class: 'lg-vs-at', text: m.home ? 'vs' : '@' }), vsTeam(opp));
  const owner = opp.owner_display ? el('div', { class: 'lg-owner' }, avatar(opp.avatar_url, opp.owner_display, 'sm'),
    el('span', { text: `@${opp.owner_display}` }), opp.online === true ? el('span', { class: 'presence-dot', 'aria-label': 'Online' }) : null) : null;
  const onClick = s.kind === 'play' ? play : s.kind === 'join' ? () => join(s.room) : (s.kind === 'waiting' || s.kind === 'lobby' || s.kind === 'live') ? () => { location.href = 'online.html'; } : null;
  const buttons = el('div', { class: 'lg-actions' },
    s.action ? el('button', { type: 'button', class: 'btn primary', 'data-key': 'league-primary', text: state.busy ? 'Working…' : s.action, disabled: state.busy, onclick: onClick }) : null,
    s.cancel ? el('button', { type: 'button', class: 'btn quiet', text: 'Cancel', disabled: state.busy, onclick: () => cancel(s.room) }) : null);
  return card('lg-week', `This week${home.league?.week_label ? ` · ${home.league.week_label}` : ''}`, [vs, owner, el('p', { class: `lg-line ${s.kind}`, text: s.line }), buttons,
    state.actionError ? el('p', { class: 'refusal', role: 'alert', text: state.actionError }) : null], { span: 8, cls: `lg-week ${s.kind}` });
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
    el('div', { class: 'vgrid lg-grid' }, thisWeek(home), standings(home), recent(home), leaders(home), roster(home)));
}

markCurrent();
wireCoreFold();
poller.start();
document.addEventListener('visibilitychange', () => (document.hidden ? poller.stop() : poller.start()));
```

Before writing, open `sign-in.js` and check the exact name of the method that returns the sign-in card. `stats.js` uses `createSignIn({ onChange, onDone })` and draws the card from it; use the same call `stats.js` uses, in place of `signIn.render()` if it differs. Likewise confirm that `card(id, title, body, { span, cls })` and `cols(node, template)` match `stats-cards.js:30,42` as quoted in the Interfaces block above.

- [ ] **Step 4: Write `pages/league.css` and the two tokens**

`theme.css` `:root`, next to the Stats tokens:

```css
  --league-cell-w: 168px;              /* one week-scoreboard cell */
  --mine-edge: inset 3px 0 0 var(--red); /* your game / your row: a red left edge, not a glow */
```

`pages/league.css` (layout only, tokens only):

```css
/* The League tab (spec 2026-10-08 §3). Layout only; every colour and size is a theme.css token. */
.page-league #screen { display: grid; gap: 16px; }

.page-league .lg-head { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 16px; padding: 16px 24px; border-radius: var(--r-lg);
  box-shadow: inset 4px 0 0 var(--tc, var(--team-fallback)), var(--sheen), var(--shadow); }
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
.page-league .lg-side { display: grid; grid-template-columns: 20px 1fr auto auto; align-items: center; gap: 8px; color: var(--fg-2); }
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
.page-league .lg-actions { display: flex; justify-content: center; gap: 8px; margin-top: 16px; }

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

@media (max-width: 1100px) { .page-league .lg-head { grid-template-columns: auto 1fr; } .page-league .lg-head .btn { grid-column: 1 / -1; justify-self: start; } }
```

If `--rule`, `--red-wash`, `--ok`, `--red-ink`, `--glass-*`, `--fg-*` or `--r-*` is missing from `theme.css` in this branch, stop and report it. They are all listed in DESIGN.md §1.

- [ ] **Step 5: Update DESIGN.md**
  - §1 Stats and Profile components table: add rows for `--league-cell-w` (one week-scoreboard cell, League) and `--mine-edge` (your game / your row: 3px red left edge, League).
  - §5 Sidebar line: `…Online, League (league members only, added by shell.js from league_member), Stats…`.
  - §8 page table: `| league.html | pages/league.css |`.
  - §12 test table: `| league-model.test.mjs | League's model: page mode, card states, scoreboard cells, rows |` and `| league-nav.test.mjs | the League link: members only, after Online, never twice |`.

- [ ] **Step 6: Run every check**

Run, from the repo root:
```bash
node Tools/check-theme.mjs
node Tools/check-theme-tokens.mjs
node Tools/check-shell-pages.mjs
node --test crates/reborn-ui/ui/*.test.mjs Tools/lib/*.test.mjs
CARGO_BUILD_JOBS=1 cargo test -p reborn-ui
```
Expected: all PASS. `check-shell-pages` must report "League draws when the core answers everything / … every field null / … rejects every call" and "League says what failed". If its `FULL` answers lack `league_home`, add `league_home: <the fixture JSON>` to that file's `FULL` map, the same way the other commands are listed there. With every field null, `pageMode` returns `redirect`. The checker's fake `location.replace` must not throw: if it does, guard the redirect with `typeof location?.replace === 'function'`.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/league.html crates/reborn-ui/ui/league.js crates/reborn-ui/ui/pages/league.css crates/reborn-ui/ui/theme.css crates/reborn-ui/ui/DESIGN.md Tools/check-theme.mjs Tools/check-shell-pages.mjs
git commit -m "ui: League page (team header, week scoreboard, This Week card, standings, results, leaders, roster)"
```

---

### Task 7: Run it, then build a beta installer (no release)

**Files:** none changed unless a bug turns up. Any fix goes back through the owning task's test first.

- [ ] **Step 1: Point a dev launcher at the backend.** The real backend endpoints come from plan 2. Until they are deployed, run plan 2's backend locally and set `backend_origin` in the launcher's settings file to it (Settings > Advanced, or `settings.json` `backend_origin`). Images still come from production, so league logos will show as abbreviation badges locally. That is expected.

- [ ] **Step 2: Run the launcher** (`CARGO_BUILD_JOBS=1 cargo run -p reborn-ui`) signed in as a Season 4 owner and check, with a screenshot of each:
  - The League link sits between Online and Stats on every page; the red nav marker lands on it on `league.html`.
  - Header, scoreboard (your game first with the red edge, LIVE dot on a live game, ‹ › and wheel scroll), This Week card, standings with your row marked, recent, leaders, roster expands.
  - Controller: from the sidebar, Right lands in the page; the Play button is reachable; Left/Right moves across scoreboard cells.
  - Window at 900 px wide: no horizontal page scroll.

- [ ] **Step 3: Two-account room check** (second PC or second Windows user, as in the series tests):
  - A presses Play → Online page shows "League game · Week N · Ranked rules · Best of 3", no Quick game/Pickup switch.
  - B's League card turns to "Join @A's game" within 60 s → Join → both in the room → ready → game boots on the ranked seed.
  - The game boots `ranked.s01` (check the Dolphin log for the boot state path), and the backend shows the upload with `intent.league = true` and `intent.fixture_id`.
  - A's Cancel before game 1 removes the room; B's card returns to Play.

- [ ] **Step 4: Non-member check.** Signed in as a non-owner: no League link anywhere. Then open `league.html` directly from the harness page or devtools: it redirects to Play. Sign out from a member account: the link is gone on the next page.

- [ ] **Step 5: Beta build note.** Build the installer the way beta builds are made today: per `project_reborn_launcher_status.md` and the BUILD SYNC LAW, from this worktree only, after `git status` is clean. **Do not publish, sign or bump the channel.** Write the installer path, SHA-256 and commit hash into the PR description for Adam. Merge order: plan 1 (site) and plan 2 (backend, dark) must be live before this ships to beta.

---

## Self-review

- **Spec coverage (§3 Launcher):**
  - Identity/gating → Tasks 1, 2, 5 (deviation documented).
  - Commands → Task 2.
  - Nav injection → Task 5.
  - Redirect of non-members → Task 4 `pageMode` + Task 6 `load`.
  - Layout incl. scoreboard → Tasks 4 and 6.
  - Card states table (bye, play, waiting, join, live, final) plus `lobby` and `unlinked` from plans 1 and 2 → Task 4.
  - 60 s poll + pause when hidden → Task 6.
  - "View on site" via `open_website` (allowlisted prefix `https://nflstreetreborn.com/`) → Task 6.
  - Gamepad → Task 7 check; existing `app-shell.js` reads the DOM.
  - Error/empty states (skeleton, cached + banner, signed out) → Tasks 1, 4, 6.
  - Result tagging (`league_match` + `intent.fixture_id`) and the Ranked-settings seed via `ruleset` → Task 3.
  - Logos via `/live-assets/league/` → `teamLogo`/`assetUrl`, no CSP change.
- **Placeholders:** the two HTML comments in Task 6 Step 2 are explicit copy instructions with line numbers, not TBDs. The `signIn.render()` name has an explicit verify step against `sign-in.js`.
- **Type consistency:**
  - `LeagueCache`, `resolve`, `fetch_home`, `open_room` and `RoomOpen` are used identically in Tasks 1 and 2. `room_view`'s `{action}` shapes match `play()` in Task 6.
  - The command names in Task 2 match the `call(...)` names in Tasks 5 and 6.
  - The model function names in the Task 4 interface match the Task 6 imports.
  - `matchup.room.{room_id, code, host_discord_user_id, state}` are consistent across the contract, Task 4 and Task 6. Cancel uses `room.room_id` with the existing `mm_room_action` `leave`.
- **Review Focus:** each of the five lines maps to a named test or check in its owning task.
