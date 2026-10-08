# League Tab — Design

Date: 2026-10-08
Status: approved in brainstorming, awaiting spec review

## Goal

A launcher tab that only league members can see. It is the league website's
homepage seen from *your team*: the week's scoreboard, your matchup, standings,
results, leaders and roster, plus a one-click way to play this week's league
game against your opponent. It must look native to the glass launcher.

## Decisions (from brainstorming)

| Topic | Decision |
|---|---|
| Who gets access | League site is the source of truth: `league_owners.discord_user_id` -> `owner_teams` -> team. Commissioners manage it in the site admin. No launcher release needed to add/remove members. |
| Scope | Read-only league data + "play this week's game". No roster moves, trades, or score reporting. |
| Data path | Approach 1: launcher -> Reborn backend -> league site (server-to-server). No secret in the launcher. |
| League game | Private league room, locked to the two Discord IDs, tagged with the fixture. First click hosts, second click joins. |
| Rules | Ranked settings (Hard + GB Off, `ranked.s01` seed), **best of 3** via the existing series system. |
| Result recording on site | Out of scope. Results are tagged and forwarded; the site consuming them is a later feature. |

## Architecture

```
Launcher (league.html/js) --Rust cmd--> reborn-core backend.rs
   GET  /v1/league/me      (bearer)          \
   POST /v1/league/room    (bearer)           > Reborn backend (Railway, Express/PG)
   POST /v1/league/room/:id/cancel            /
                                               | POST api/bot.php action=team_home
                                               |   (INGEST_TOKEN, server-to-server)
                                               v
                                         League site (PHP/MySQL, live)
```

Three codebases change:

1. **League site** (live `public_html/league`, SiteGround; pull live files first — local replica is stale for Season 4).
2. **Reborn backend** (`Reborn Backend`, new branch `feat/league-tab`).
3. **Launcher** (new branch `feat/league-tab` off the current stable line, 3.0.35).

## 1. League site: `team_home` action

Added to `api/bot.php` (existing bearer `INGEST_TOKEN` check).

Request: `{"action":"team_home","discord_user_id":"<snowflake>"}`

Resolution:
- Find `league_owners` row by `discord_user_id`; join `owner_teams` -> teams.
- Restrict to the **current live league**: newest league not in `_inc/nsr_hidden.php`, same rule `index.php` uses. If the owner has teams in several active leagues, newest league wins (one team per person in v1).
- No match -> `{"member":false}`.

Response (member):

```json
{
  "member": true,
  "league":   {"id":36,"name":"...","season":4,"week":6},
  "team":     {"id":485,"name":"...","abbr":"...","logo_url":"...","color":"#...","record":"4-1","standing":2},
  "matchup":  {"fixture_id":1234,"week":6,"home":true,"status":"scheduled|final|bye","score":null,"when":"...",
               "opponent":{"team_id":486,"name":"...","abbr":"...","logo_url":"...","record":"3-2",
                           "owner_display":"...","discord_user_id":"<snowflake>"}},
  "week_games": [{"fixture_id":1234,"home":{"team_id":..,"abbr":"..","logo_url":"..","record":".."},
                  "away":{...},"status":"scheduled|final","score":{"home":28,"away":21},"when":"..."}],
  "standings": [{"rank":1,"team_id":..,"name":"..","abbr":"..","record":"5-0"}],
  "recent":    [{"fixture_id":..,"week":5,"opponent_abbr":"..","home":false,"result":"W","score":"28-21"}],
  "leaders":   {"pass":[{"name":"..","value":1204}],"rush":[..],"rec":[..]},
  "roster":    [{"name":"..","position":"..","number":..}],
  "site_url":  "https://nflstreetreborn.com/league/team.php?league_id=36&team_id=485"
}
```

Queries reuse the ones `index.php` / `team.php` already run (this week's games, standings, leaders, recent results) — extract into `_inc/nsr_team_home.php` rather than duplicate SQL.

Data: every Season 4 owner already has `discord_user_id` mapped (confirmed by Adam 10-08). The site deploy step still runs a one-line count as a sanity check; no backfill is planned.

## 2. Backend

### `GET /v1/league/me` (requirePlayer)
- Uses `req.player.discord_user_id`; calls site `team_home`.
- Per-player cache 60 s.
- Merges live state into `week_games` / `matchup`: any live game or open league room whose tag `league.fixture_id` matches gets `status:"live"`, live score (from `/v1/live` RAM score) and series score.
- Adds `room` to `matchup` if an open league room exists for the fixture: `{room_id, host_discord_user_id, state}`.
- Rewrites logo URLs to `/live-assets/league/<team_id>.png` (see Logos).
- Errors: 401 signed out; `{member:false}` for non-members; 503 if the site is unreachable.

### `POST /v1/league/room` (requirePlayer)
- Body: none. The backend re-fetches the caller's fixture from the site (never trusts the client).
- Rejects if: not a member, no fixture this week (bye), fixture already final.
- Idempotent per `fixture_id`: if an open room exists, return it (caller becomes the joiner); otherwise create a private room:
  - `allowed_discord_ids = [caller, opponent]`
  - `tag.league = {league_id, fixture_id, week}`
  - ruleset = Ranked settings (`ranked.s01` seed), `best_of = 3` through the existing series machinery (`match_id`).
- Concurrency: unique constraint on open league room per `fixture_id` (migration), so two simultaneous clicks produce one room.
- Notifies the opponent through the live bot (one ping: "@host is waiting for your Week N league game"). Failure to ping never fails the request.

### `POST /v1/league/room/:id/cancel`
- Host only, only before game 1 starts.

### Room rules
- Hidden from the public room list (existing private-room behaviour).
- Join endpoint refuses any Discord ID not in `allowed_discord_ids`.
- Expires after 30 min with nobody in it.
- Stays open across the best-of-3 games.

### Result tagging
- League games launch with `leagueMatch=true` + `fixture_id`; uploads carry `intent.league` + fixture + series result. Existing `forward.js` sends them to `LEAGUE_FORWARD_URL`. No site-side recording in this project.

### Logos
- Launcher CSP allows images only from `api.nflstreetreborn.com/live-assets/` and Discord avatars. Backend serves `/live-assets/league/<team_id>.png`, fetched from the site's logo URL and cached (24 h). CSP is not widened.

### Config
- `LEAGUE_SITE_URL` (bot.php endpoint), reuse `INGEST_TOKEN` (new env var `LEAGUE_SITE_TOKEN` holding the same value).

## 3. Launcher

### Identity / gating
- `reborn-core`: parse `discord_user_id` from `/v1/me` / poll reply into `PlayerFacts`; add `league_member: bool` to `identity.json`.
- New commands: `league_home()` -> `/v1/league/me`, `league_room_open()`, `league_room_cancel(id)`, `league_room_join(id)` (reuses existing room-join/launch path with `leagueMatch=true` + fixture).
- Membership refreshed on startup and after sign-in. Last known good payload cached on disk so a 503 still renders.

### Nav
- `shell.js` injects `<a href="league.html">League</a>` (shield icon) between Online and Stats when `league_member` is true. The 12 hard-coded navs are not edited. `markCurrent()` and the gamepad nav in `app-shell.js` pick it up because they read the DOM.
- `league.html` re-checks on load; non-members are redirected to `index.html`.

### Page layout (`league.html`, `league.js`, `pages/league.css`)

```
┌ team header: logo · TEAM NAME · Season 4 · Week 6 · 4-1 · 2nd · [View on site ›] ┐
├ WEEK SCOREBOARD (horizontal strip, ‹ ›) ───────────────────────────────────────┤
│ cells: logos, abbr, records, FINAL score / LIVE score ● / day-time or TBD       │
│ your game: red left edge, scrolled into view first, click -> This Week card     │
├───────────────────────────────────────────┬─────────────────────────────────────┤
│ THIS WEEK  (primary)                       │ STANDINGS (your row highlighted)   │
│ you vs opponent, records, opponent         │                                     │
│ avatar + online dot, series score          │                                     │
│ [ Play league game ] state machine         │                                     │
├───────────────────────────────────────────┤                                     │
│ RECENT RESULTS                             │                                     │
├───────────────────────────────────────────┤                                     │
│ TEAM LEADERS  Pass | Rush | Rec            │                                     │
├───────────────────────────────────────────┴─────────────────────────────────────┤
│ ROSTER (collapsed)                                                              │
└─────────────────────────────────────────────────────────────────────────────────┘
```

Style: existing `theme.css` glass tokens; panel treatment borrowed from the Madden reference — uppercase panel titles, "View on site ›" link top-right, dense dark rows, team-colour accent stripe on cards. Follows `ui/DESIGN.md`.

### This Week card states
| State | Shows |
|---|---|
| no fixture / bye | "No game this week", no button |
| scheduled, no room | **Play league game** |
| you host, waiting | **Waiting for @opponent…** + Cancel |
| opponent hosts | **Join @opponent's game** |
| in series | "LIVE · YOU lead 1-0" + Rejoin if dropped |
| final | final score, no button |

### Refresh
- Poll `league_home` every 60 s while the tab is visible; also react to the existing presence/room push for instant room state.

### "View on site"
- Uses `open_website` (already allows `https://nflstreetreborn.com/`).

### Gamepad
- Scoreboard: left/right scroll. Card button gets initial focus.

### Error/empty states
- Skeleton while loading; cached data + "League data unavailable" banner on 503; signed-out -> tab hidden.

## Security

- Site token only on the backend.
- Membership decided server-side on every call; the launcher flag only controls whether the nav link is drawn.
- Room creation derives the fixture and opponent server-side.
- Only the opponent's Discord ID reaches the client.

## Testing

- Site: `team_home` against a local MariaDB copy of the live Season 4 data — member, non-member, bye week, final week.
- Backend: unit tests with a stubbed site — gating, cache, 503 path, room idempotency (parallel creates -> one room), join refusal for a third Discord ID, cancel rules, expiry, live-score merge.
- Launcher: `reborn-core` tests for `discord_user_id` parse + `league_member` persistence; UI states rendered from fixture JSON; nav injection on/off.
- End-to-end: two accounts on two PCs (or two local Dolphins) play a league best-of-3 from the tab; verify room lock, series score on card + scoreboard, tagged upload forwarded.

## Out of scope

- Site automatically recording league results from forwarded uploads.
- Roster moves, trades, draft from the launcher.
- Multiple teams/leagues per person.
- Custom league seed (uses Ranked settings).

## Rollout

1. Site action deployed (pull live first, WinSCP single files).
2. Backend deployed (dark: no launcher calls it yet).
3. Launcher shipped to beta channel; league members test; then stable.
