# League Tab — Plan 2: Reborn Backend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the launcher one signed-in read (`GET /v1/league/me`) and one write (`POST /v1/league/room`) that turn the league site's `team_home` answer into a members-only team page with live series state, and open a best-of-3, Ranked-settings private room locked to the two players of this week's fixture.

**Architecture:** A small `src/league/` module: `site.js` (server-to-server client for the site's `team_home`, 60 s cache per Discord ID, logo source registry), `state.js` (open league rooms and running league matches for a set of fixtures), `merge.js` (pure: site payload + backend state -> launcher payload), `router.js` (the two routes plus the logo proxy). League rooms are ordinary `mm_room` rows with league columns and an `allowed_discord_ids` lock; joining, ready, start, leave and the whole match/series/relay path are the existing room machinery, with `createMatch` taught one new `source: "league"` that pins best-of-3.

**Tech Stack:** Node 22, ESM, Express 5, Postgres (`pg`), `node:test`; Railway service `reborn-api`.

**Spec:** `docs/superpowers/specs/2026-10-08-league-tab-design.md` (in the launcher repo, branch `feat/league-tab`). Site contract: Plan 1, `docs/superpowers/plans/2026-10-08-league-tab-1-site.md` § "JSON contract" — that section is authoritative for field names.

## Global Constraints

- Work in a new worktree `C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-league` on branch `feat/league-tab` cut from `Reborn Backend` `main` (2fdeefa or newer). Never commit in `Reborn Backend` itself (shared worktree law, memory `project_reborn_stable_301_launch.md`).
- Migrations are numbered files in `migrations/`; this plan adds `029_league_room.sql`. Before creating it run `ls migrations` on `main` AND on every `reborn-backend-*` worktree; if 029 is taken anywhere, take the next free number and use it everywhere this plan says 029.
- Every migration begins `SET LOCAL lock_timeout = '5s';` and documents a Rollback line (house style, see 028).
- Identity is never read from a body: every player route uses `requirePlayer` and `req.player` (rooms.js property 1).
- Lock order law (`src/mm/locks.js`): ROOM(2) -> MATCH(3) -> PLAYER(4). The fixture lock is a ROOM-class key `league-fixture:<id>` taken before any room/player key.
- League match rules: `mode = 'casual'` (no Elo), `source = 'league'`, `best_of = 3`, copied rules `casual_best_of = 3`. Ruleset name sent to the launcher: `"ranked"` (Hard + GB Off seed `ranked.s01`).
- League room TTL: 30 minutes (`LEAGUE_ROOM_TTL_MS = 30 * 60 * 1000`). Capacity 2. Seed kind `regular`, fixed.
- Site call: `POST ${LEAGUE_SITE_URL}` with `Authorization: Bearer ${LEAGUE_SITE_TOKEN}`, body `{"action":"team_home","discord_user_id":"<snowflake>"}`, 8 s timeout. Discord IDs are strings.
- Per-player cache of the site answer: 60 s. Writes (`POST /v1/league/room`) always fetch fresh.
- Logo proxy only fetches `https://nflstreetreborn.com/league/` URLs, only image content types, at most 1 MiB, cached 24 h.
- New env vars `LEAGUE_SITE_URL`, `LEAGUE_SITE_TOKEN` are optional (never through `need()` in server.js); unset = every `/v1/league/me` answers 503.
- Tests: `npm test` (needs `DATABASE_URL_TEST`, see README). One file: `node --env-file-if-exists=.env --test test/<file>.test.js`.
- No `.rs` work here; launcher is Plan 3.

## Launcher-facing contract (what Plan 3 consumes)

### `GET /v1/league/me` (bearer)

- 401 `{ok:false,error:"Signed out. Sign in from the launcher."}` (requirePlayer).
- 503 `{ok:false,error:"league_unavailable"}` when the site is unset/unreachable/answers non-200/non-JSON.
- 200 `{ok:true,member:false}` for non-members.
- 200 member: the Plan 1 site payload, with these changes:
  - every `logo_url` that was non-null becomes `${PUBLIC_ORIGIN}/live-assets/league/<team_id>.png` (team: `team.id`; others: `team_id`); null stays null.
  - `week_games[].status` and `matchup.status` may additionally be `"live"` (a league match for that fixture is running). An open league room for a fixture the site calls `final` turns it into `"in_progress"`.
  - `week_games[]` and `matchup` gain `match_id` (string or null) and, when live, `series {home,away}` and `score {home,away}` (current game's score; null until a play with ports arrives) override the site's.
  - `matchup.room`: `null`, or `{room_id, code, host_discord_user_id, members, expires_at, state}` where `state` ∈ `"waiting"` (host alone) | `"lobby"` (both in) | `"in_series"` (match running; `room_id`/`code` null, `match_id` set).
  - `matchup.opponent.online`: boolean from presence (`last_used_at` within 90 s), false if the opponent has no Reborn account.

### `POST /v1/league/room` (bearer, no body)

Re-reads the caller's fixture from the site (fresh). Replies:

| Case | Status | Body |
|---|---|---|
| not a member | 403 | `{ok:false,error:"not_member"}` |
| bye / no fixture | 409 | `{ok:false,error:"no_game"}` |
| site says final and nothing open | 409 | `{ok:false,error:"already_final"}` |
| opponent has no Discord ID on the site | 409 | `{ok:false,error:"opponent_unlinked"}` |
| a league match for this fixture is running | 409 | `{ok:false,error:"in_series",match_id}` |
| caller already in another lobby / seated | 409 | existing room sentences (`You are already in a lobby.` / `You are already in a match.`) |
| no open room -> created, caller is host | 200 | `{ok:true,action:"created",room:<roomView>}` |
| caller already in the open room | 200 | `{ok:true,action:"created",room:<roomView>}` |
| opponent's room is open | 200 | `{ok:true,action:"join",code:"K7QP2M"}` |
| site unreachable | 503 | `{ok:false,error:"league_unavailable"}` |

### Joining, readying, starting, cancelling = existing routes

- Join: `POST /v1/mm/rooms/:code/join` (body `{}`), same as any code room. A league room refuses everyone whose Discord ID is not in `allowed_discord_ids` with the room-not-found 404.
- Ready / field / chat / start: `POST /v1/mm/rooms/:id/ready|field|chat|start` unchanged. `kind` is refused on a league room (409 `A league game's type is fixed.`).
- Cancel: `POST /v1/mm/rooms/:id/leave` by the host (host leaving closes the lobby). No separate cancel route (spec's `/cancel` is this route).
- The match the room starts is seen through the existing long poll snapshot. `snapshot.room.league` and `snapshot.match.league` are `{league_id, fixture_id, week}` or null, and `snapshot.match.ruleset` is `"ranked"` for league and ranked matches, `"casual"` otherwise. The launcher boots `ranked.s01` when `ruleset == "ranked"`, launches with `leagueMatch=true` and `intent.league=true` on uploads so `forward.js` forwards them.

## Review Focus

1. **Both players click Play at the same instant** — exactly one room exists; one gets `created`, the other `join`. Pinned in Task 5 (holds the fixture key from outside, like `mm_rooms.test.js`).
2. **A stranger with the code** — joining or peeking a league room by code must look exactly like a bad code (404), never "full" or "not yours". Pinned in Task 4.
3. **Site marks the bo3 `final` after game 1** (Plan 1 known semantics) — while the league match is running or a room is open, the tab must not say final. Pinned in Task 6.
4. **A stale 'open' room past `expires_at`** (the prune tick runs late) must not block a new room via the unique index. Pinned in Task 5.
5. **Site down** — `/v1/league/me` must 503 (launcher shows cached data), never 500 and never `member:false` (which would hide the tab from a real member). Pinned in Task 2 and Task 6.

---

## File map

| File | Change | Responsibility |
|---|---|---|
| `migrations/029_league_room.sql` | create | league columns on `mm_room` and `match`, `allowed_discord_ids`, one-open-room-per-fixture index |
| `src/league/site.js` | create | site client: `teamHome(discordId, {fresh})`, cache, `logoSource(teamId)` |
| `src/league/state.js` | create | `leagueState(pool, clock, fixtureIds)` -> open rooms + running matches |
| `src/league/merge.js` | create | pure `mergeLeague(site, state, {publicOrigin, viewerDiscordId, opponentOnline})` |
| `src/league/room.js` | create | `openLeagueRoom(pool, clock, {player, site, newCode})` transaction |
| `src/league/router.js` | create | `GET /v1/league/me`, `POST /v1/league/room`, `GET /live-assets/league/:file` |
| `src/mm/rooms.js` | modify | export `assertNotSeated`; league columns in `lockedRoom`/`roomView`; allowed-list on join + peek; league start; kind refusal |
| `src/mm/match.js` | modify | `createMatch` `league` param + `source:"league"` rules; `MATCH_COLUMNS` |
| `src/mm/snapshot.js` | modify | `room.league`, `match.league`, `match.ruleset` |
| `src/botFeeds.js` | modify | `GET /v1/bot/league/rooms` feed for the opponent ping |
| `src/app.js` | modify | mount `leagueRouter` |
| `README.md` | modify | env vars + routes |
| `test/league_*.test.js` | create | tests per task |

---

### Task 0: Worktree

**Files:** none (git only)

- [ ] **Step 1: Create the worktree**

```powershell
cd "C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend"
git fetch --all 2>$null
git worktree add -b feat/league-tab "..\reborn-backend-league" main
cd "..\reborn-backend-league"
git log --oneline -1
```
Expected: the `main` head (2fdeefa or newer).

- [ ] **Step 2: Install and baseline**

```powershell
npm ci
npm test
```
Expected: all green (record the count). If anything fails on a clean `main`, stop and report it; do not start Task 1 on a red baseline.

- [ ] **Step 3: Pick the migration number**

```powershell
Get-ChildItem "C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend\migrations","C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-*\migrations" -Filter "02[89]*","03*" | Select-Object FullName
```
If `029_*` exists anywhere other than this branch, use the next free number in every later step.

---

### Task 1: Migration 029 — league rooms and league matches

**Files:**
- Create: `migrations/029_league_room.sql`
- Test: `test/league_schema.test.js`

**Interfaces:**
- Produces columns: `mm_room.league_id int`, `mm_room.league_fixture_id bigint`, `mm_room.league_week int`, `mm_room.league_seat0_home boolean`, `mm_room.allowed_discord_ids text[]`; same four `league_*` columns on `match`; unique partial index `mm_room_league_one_open`.

- [ ] **Step 1: Write the failing test**

```js
// test/league_schema.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";

let n = 0;
const player = async (pool) => (await pool.query(
  `INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`9${++n}0000000`, `P${n}`])).rows[0];
const room = (pool, clock, host, fixture, status = "open") => pool.query(
  `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, status, created_at, expires_at,
                        league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids)
   VALUES ($1, $2, 'casual', 'code', 3, $3, $4, $4, 36, $5, 6, true, ARRAY['1','2'])`,
  [`C${String(++n).padStart(5, "A")}`.slice(0, 6), host.id, status, clock.now(), fixture]);

test("029: one OPEN league room per fixture; closed and non-league rooms are not limited", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const a = await player(t.pool); const b = await player(t.pool);
    await room(t.pool, clock, a, 1234);
    await assert.rejects(room(t.pool, clock, b, 1234), (e) => e.code === "23505" && /mm_room_league_one_open/.test(e.constraint));
    await room(t.pool, clock, b, 1234, "closed");
    await room(t.pool, clock, b, 9999);
    const cols = (await t.pool.query(
      `SELECT table_name, column_name FROM information_schema.columns
        WHERE table_schema = $1 AND column_name LIKE 'league_%' ORDER BY 1, 2`, [t.schema])).rows
      .map((r) => `${r.table_name}.${r.column_name}`);
    assert.deepEqual(cols, [
      "match.league_fixture_id", "match.league_id", "match.league_seat0_home", "match.league_week",
      "mm_room.league_fixture_id", "mm_room.league_id", "mm_room.league_seat0_home", "mm_room.league_week",
    ]);
  } finally { await t.close(); }
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `node --env-file-if-exists=.env --test test/league_schema.test.js`
Expected: FAIL, `column "league_id" of relation "mm_room" does not exist`.

- [ ] **Step 3: Write the migration**

```sql
-- 029: league rooms (docs/superpowers/specs/2026-10-08-league-tab-design.md, launcher repo feat/league-tab).
--
-- * mm_room.league_*: the league fixture a room was opened for (league site ids; no FK, the site is another database).
--   league_seat0_home: true when the room's host (seat 0 of its match) owns the HOME team of the fixture.
-- * mm_room.allowed_discord_ids: the only Discord IDs that may join or even see the room by code. NULL = any code
--   holder (every non-league room).
-- * match.league_*: copied from the room at start, so the series and the launcher know which fixture it plays.
-- * mm_room_league_one_open: at most one OPEN room per fixture -- two players pressing Play at once get one room.
--
-- Rollback: DROP INDEX mm_room_league_one_open; DROP INDEX match_league_fixture;
-- ALTER TABLE mm_room DROP COLUMN league_id, DROP COLUMN league_fixture_id, DROP COLUMN league_week,
--   DROP COLUMN league_seat0_home, DROP COLUMN allowed_discord_ids;
-- ALTER TABLE match DROP COLUMN league_id, DROP COLUMN league_fixture_id, DROP COLUMN league_week,
--   DROP COLUMN league_seat0_home;
SET LOCAL lock_timeout = '5s';

ALTER TABLE mm_room
  ADD COLUMN IF NOT EXISTS league_id int,
  ADD COLUMN IF NOT EXISTS league_fixture_id bigint,
  ADD COLUMN IF NOT EXISTS league_week int,
  ADD COLUMN IF NOT EXISTS league_seat0_home boolean,
  ADD COLUMN IF NOT EXISTS allowed_discord_ids text[];

ALTER TABLE match
  ADD COLUMN IF NOT EXISTS league_id int,
  ADD COLUMN IF NOT EXISTS league_fixture_id bigint,
  ADD COLUMN IF NOT EXISTS league_week int,
  ADD COLUMN IF NOT EXISTS league_seat0_home boolean;

CREATE UNIQUE INDEX IF NOT EXISTS mm_room_league_one_open
  ON mm_room (league_fixture_id) WHERE status = 'open' AND league_fixture_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS match_league_fixture
  ON match (league_fixture_id) WHERE league_fixture_id IS NOT NULL;
```

- [ ] **Step 4: Run the test, then the whole suite**

Run: `node --env-file-if-exists=.env --test test/league_schema.test.js` → PASS. Then `npm test` → same count as baseline + 1, all green (the `matchmaking_schema` tests must not mind extra nullable columns; if one pins an exact column list, extend that list with the new columns).

- [ ] **Step 5: Commit**

```powershell
git add migrations/029_league_room.sql test/league_schema.test.js
git commit -m "league: migration 029 (league rooms, allowed_discord_ids, one open room per fixture)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Site client (`src/league/site.js`)

**Files:**
- Create: `src/league/site.js`
- Test: `test/league_site.test.js`

**Interfaces:**
- Produces: `leagueSite({ url, token, fetchImpl = fetch, clock, ttlMs = 60_000 })` -> `{ teamHome(discordId, { fresh = false } = {}) : Promise<object>, logoSource(teamId) : string|null }`; `class SiteUnavailable extends Error`; `TEAM_HOME_TTL_MS = 60_000`; `LOGO_ORIGIN = "https://nflstreetreborn.com/league/"`.
- `teamHome` resolves to the site's JSON (`{ok:true, member:...}`), or throws `SiteUnavailable`.

- [ ] **Step 1: Write the failing tests**

```js
// test/league_site.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { fixedClock, fakeFetch } from "./helpers/fakes.js";
import { leagueSite, SiteUnavailable } from "../src/league/site.js";

const URL_ = "https://nflstreetreborn.test/league/api/bot.php";
const member = {
  ok: true, member: true,
  league: { id: 36, name: "S4", season: 4, week: 6, week_label: "Week 6" },
  team: { id: 493, name: "Subway Surfers", abbr: "ADAM", logo_url: "https://nflstreetreborn.com/league/uploads/493.png", color: "#00A3FF", record: "4-1", standing: 2 },
  matchup: { fixture_id: 1234, week: 6, home: true, status: "scheduled", score: null, series: null, winner_team_id: null, when: null,
    opponent: { team_id: 486, name: "Furies", abbr: "BLONDE", logo_url: null, color: "#FF0000", record: "3-2", owner_display: "BLONDE", discord_user_id: "480510938571735040" } },
  week_games: [], standings: [], recent: [], leaders: { pass: [], rush: [], rec: [] }, leaders_unit: "TD", roster: [],
  site_url: "https://nflstreetreborn.com/league/team.php?league_id=36&team_id=493",
};

test("teamHome posts the action with the bearer and caches the answer for 60 s per Discord ID", async () => {
  const clock = fixedClock();
  const f = fakeFetch({ [URL_]: { body: JSON.stringify(member) } });
  const site = leagueSite({ url: URL_, token: "tok", fetchImpl: f, clock });
  assert.deepEqual(await site.teamHome("111"), member);
  const sent = f.calls[0];
  assert.equal(sent.init.method, "POST");
  assert.equal(sent.init.headers.Authorization, "Bearer tok");
  assert.deepEqual(JSON.parse(sent.init.body), { action: "team_home", discord_user_id: "111" });
  await site.teamHome("111");
  assert.equal(f.calls.length, 1, "second read inside 60 s is cached");
  clock.advance(60_000);
  await site.teamHome("111");
  assert.equal(f.calls.length, 2, "expired after 60 s");
  await site.teamHome("111", { fresh: true });
  assert.equal(f.calls.length, 3, "fresh always asks");
});

test("non-member is an answer, not an error", async () => {
  const site = leagueSite({ url: URL_, token: "tok", fetchImpl: fakeFetch({ [URL_]: { body: '{"ok":true,"member":false}' } }), clock: fixedClock() });
  assert.deepEqual(await site.teamHome("222"), { ok: true, member: false });
});

test("unset, unreachable, non-200, non-JSON and ok:false are all SiteUnavailable, and are never cached", async () => {
  const clock = fixedClock();
  await assert.rejects(leagueSite({ url: "", token: "", clock }).teamHome("1"), SiteUnavailable);
  await assert.rejects(leagueSite({ url: URL_, token: "t", fetchImpl: fakeFetch({}), clock }).teamHome("1"), SiteUnavailable);
  for (const r of [{ status: 500, body: "{}" }, { status: 200, body: "<html>" }, { status: 401, body: '{"ok":false,"error":"Unauthorized"}' }, { status: 200, body: '{"ok":true}' }]) {
    const f = fakeFetch({ [URL_]: r });
    const site = leagueSite({ url: URL_, token: "t", fetchImpl: f, clock });
    await assert.rejects(site.teamHome("1"), SiteUnavailable, JSON.stringify(r));
    await assert.rejects(site.teamHome("1"), SiteUnavailable);
    assert.equal(f.calls.length, 2, "a failure is not cached");
  }
});

test("logoSource remembers league-site logo URLs seen in answers, and nothing from another origin", async () => {
  const payload = structuredClone(member);
  payload.week_games = [{ fixture_id: 1, home: { team_id: 482, logo_url: "https://evil.example/x.png" }, away: { team_id: 483, logo_url: "https://nflstreetreborn.com/league/uploads/483.png" } }];
  const site = leagueSite({ url: URL_, token: "t", fetchImpl: fakeFetch({ [URL_]: { body: JSON.stringify(payload) } }), clock: fixedClock() });
  await site.teamHome("1");
  assert.equal(site.logoSource(493), "https://nflstreetreborn.com/league/uploads/493.png");
  assert.equal(site.logoSource(483), "https://nflstreetreborn.com/league/uploads/483.png");
  assert.equal(site.logoSource(482), null, "foreign origin never registered");
  assert.equal(site.logoSource(486), null, "null logo");
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_site.test.js`
Expected: FAIL, `Cannot find module '../src/league/site.js'`.

- [ ] **Step 3: Implement**

```js
// src/league/site.js
// The league site's `team_home` (Plan 1), server to server. The site's token lives here and nowhere on a player's
// machine (same rule as forward.js). A member answer is cached 60 s per Discord ID so tab switching never hammers the
// site; a failure is never cached, and is always SiteUnavailable -- never "not a member", which would hide the tab
// from a real member while the site is down.

export const TEAM_HOME_TTL_MS = 60_000;
export const SITE_TIMEOUT_MS = 8_000;
/** The only origin a league logo is ever fetched from (router.js logo proxy). */
export const LOGO_ORIGIN = "https://nflstreetreborn.com/league/";
const CACHE_MAX = 5_000;

export class SiteUnavailable extends Error {}

/** Every { team_id | id, logo_url } object in a team_home answer, as [teamId, url] pairs. */
function logoPairs(body) {
  const out = [];
  const add = (id, url) => { if (Number.isSafeInteger(id) && typeof url === "string") out.push([id, url]); };
  add(body?.team?.id, body?.team?.logo_url);
  add(body?.matchup?.opponent?.team_id, body?.matchup?.opponent?.logo_url);
  for (const g of Array.isArray(body?.week_games) ? body.week_games : []) { add(g?.home?.team_id, g?.home?.logo_url); add(g?.away?.team_id, g?.away?.logo_url); }
  for (const s of Array.isArray(body?.standings) ? body.standings : []) add(s?.team_id, s?.logo_url);
  return out;
}

export function leagueSite({ url, token, fetchImpl = fetch, clock, ttlMs = TEAM_HOME_TTL_MS, userAgent = "NFLStreetReborn-Backend/1.0" } = {}) {
  if (!clock || typeof clock.now !== "function") throw new Error("leagueSite needs the injected clock");
  const cache = new Map();   // discordId -> { at, value }
  const logos = new Map();   // teamId -> league-site URL

  async function teamHome(discordId, { fresh = false } = {}) {
    if (!url || !token) throw new SiteUnavailable("league site not configured");
    const id = String(discordId);
    const now = clock.now().getTime();
    const hit = cache.get(id);
    if (!fresh && hit && now - hit.at < ttlMs) return hit.value;
    let status = 0, text = "";
    try {
      const res = await fetchImpl(url, {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}`, "User-Agent": userAgent },
        body: JSON.stringify({ action: "team_home", discord_user_id: id }),
        signal: AbortSignal.timeout(SITE_TIMEOUT_MS),
      });
      status = res.status; text = await res.text();
    } catch (e) { throw new SiteUnavailable(`league site unreachable: ${e.message}`); }
    let body = null;
    try { body = JSON.parse(text); } catch { body = null; }
    if (status !== 200 || body?.ok !== true || typeof body.member !== "boolean") throw new SiteUnavailable(`league site answered ${status}`);
    for (const [teamId, logo] of logoPairs(body)) if (logo.startsWith(LOGO_ORIGIN)) logos.set(teamId, logo);
    cache.set(id, { at: now, value: body });
    if (cache.size > CACHE_MAX) for (const [k, v] of cache) if (now - v.at >= ttlMs) cache.delete(k);
    return body;
  }

  return { teamHome, logoSource: (teamId) => logos.get(Number(teamId)) ?? null };
}
```

- [ ] **Step 4: Run tests**

Run: `node --env-file-if-exists=.env --test test/league_site.test.js` → PASS (4 tests).

- [ ] **Step 5: Commit**

```powershell
git add src/league/site.js test/league_site.test.js
git commit -m "league: site client for team_home (60 s cache, SiteUnavailable, logo registry)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: League matches are best-of-3, Ranked-settings, and say so

**Files:**
- Modify: `src/mm/match.js` (`MATCH_COLUMNS` ~186, `createMatch` 388-469)
- Modify: `src/mm/snapshot.js` (room query ~226, `snapshot.room` ~252, match query ~281, `snapshot.match` ~335)
- Test: `test/league_match.test.js`

**Interfaces:**
- Consumes: Task 1 columns.
- Produces: `createMatch(client, clock, { ..., source: "league", league: { league_id, fixture_id, week, seat0_home } })`; exported `LEAGUE_BEST_OF = 3`; snapshot `room.league`, `match.league` (`{league_id, fixture_id, week}` | null), `match.ruleset` (`"ranked"`|`"casual"`).

- [ ] **Step 1: Write the failing test**

```js
// test/league_match.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { withTx } from "../src/db.js";
import { createMatch, LEAGUE_BEST_OF } from "../src/mm/match.js";
import { seriesShape } from "../src/mm/series.js";
import { snapshotFor } from "../src/mm/snapshot.js";

let n = 0;
const player = async (pool) => (await pool.query(
  `INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`7${++n}000000`, `P${n}`])).rows[0];

test("a league match is casual (no Elo), best of 3 whatever casual_best_of says, and carries its fixture", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    await t.pool.query(`UPDATE series_rules SET casual_best_of = NULL`);
    const a = await player(t.pool); const b = await player(t.pool);
    const { match } = await withTx(t.pool, (c) => createMatch(c, clock, {
      mode: "casual", seat0: a.id, seat1: b.id, source: "league",
      league: { league_id: 36, fixture_id: 1234, week: 6, seat0_home: false },
    }));
    assert.equal(LEAGUE_BEST_OF, 3);
    assert.equal(match.mode, "casual");
    assert.equal(Number(match.best_of), 3);
    assert.equal(Number(match.league_fixture_id), 1234);
    assert.equal(match.league_seat0_home, false);
    const shape = seriesShape(match);
    assert.equal(shape.ranked, false, "no rating, no ranked forfeit rules");
    assert.equal(shape.bestOf, 3); assert.equal(shape.need, 2);

    const snap = await withTx(t.pool, (c) => snapshotFor(c, clock, a.id));
    assert.deepEqual(snap.match.league, { league_id: 36, fixture_id: 1234, week: 6 });
    assert.equal(snap.match.ruleset, "ranked", "league games boot ranked.s01 (Hard + GB Off)");
  } finally { await t.close(); }
});

test("source league without a league object is a programming error; other sources say ruleset casual/ranked by mode", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const a = await player(t.pool); const b = await player(t.pool);
    await assert.rejects(withTx(t.pool, (c) => createMatch(c, clock, { seat0: a.id, seat1: b.id, source: "league" })), /league/);
    await withTx(t.pool, (c) => createMatch(c, clock, { mode: "casual", seat0: a.id, seat1: b.id, source: "room" }));
    const snap = await withTx(t.pool, (c) => snapshotFor(c, clock, a.id));
    assert.equal(snap.match.league, null);
    assert.equal(snap.match.ruleset, "casual");
  } finally { await t.close(); }
});
```

> If `snapshotFor`'s option shape differs from `(client, clock, playerId)` when you read snapshot.js:120, match its real signature here; do not change snapshot's signature.

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_match.test.js`
Expected: FAIL, `LEAGUE_BEST_OF` is not exported.

- [ ] **Step 3: Implement in match.js**

Add `"league_id", "league_fixture_id", "league_week", "league_seat0_home",` to the end of `MATCH_COLUMNS`.

Above `createMatch`, add:

```js
/** A league fixture is always a best-of-three (Adam 10-08: "it's best of 3"), whatever the casual rule says. */
export const LEAGUE_BEST_OF = 3;
```

Change the `createMatch` signature and the rules block:

```js
export async function createMatch(client, clock, {
  mode = "casual", bestOf = null, seat0, seat1, roomId = null, picks = [null, null],
  region = null, source = "queue", originSource = null, accepted = null, actor = ACTOR.SYSTEM,
  seedKind = "regular", extras = [], league = null,
} = {}) {
```

```js
  if (source === "league" && !(league && Number.isSafeInteger(Number(league.fixture_id)))) {
    throw new Error("createMatch: a league match needs league { league_id, fixture_id, week, seat0_home }");
  }
  const sandbox = mode !== "ranked" && PRIVATE_SOURCES.includes(originSource ?? source);
  const read = await readSeriesRules(client);
  // A league fixture is a fixed best of three: copied rules say so, so seriesShape reads it as a closed series.
  const rules = source === "league" ? { ...read, casual_best_of: LEAGUE_BEST_OF }
    : sandbox ? { ...read, casual_best_of: null } : read;
  const best = mode === "ranked" ? rules.ranked_best_of : rules.casual_best_of;
```

Extend the INSERT: add `league_id, league_fixture_id, league_week, league_seat0_home` to the column list, `$23,$24,$25,$26` to VALUES, and append to the params array:

```js
      league ? Number(league.league_id) : null, league ? Number(league.fixture_id) : null,
      league ? Number(league.week) : null, league ? league.seat0_home === true : null],
```

Add `league` to the `PAIRED` event payload: `...(league ? { league: { league_id: Number(league.league_id), fixture_id: Number(league.fixture_id), week: Number(league.week) } } : {})`.

- [ ] **Step 4: Implement in snapshot.js**

Room query: add `r.league_id, r.league_fixture_id, r.league_week,` to the SELECT. In `snapshot.room = {...}` add:

```js
        league: r.league_fixture_id === null || r.league_fixture_id === undefined ? null
          : { league_id: num(r.league_id), fixture_id: num(r.league_fixture_id), week: num(r.league_week) },
```

Match query: add `m.league_id, m.league_fixture_id, m.league_week,`. In `snapshot.match = {...}` add after `source`:

```js
        // The league fixture this series plays, or null. A league match boots the Ranked settings (ranked.s01).
        league: m.league_fixture_id === null || m.league_fixture_id === undefined ? null
          : { league_id: num(m.league_id), fixture_id: num(m.league_fixture_id), week: num(m.league_week) },
        ruleset: m.mode === "ranked" || (m.league_fixture_id !== null && m.league_fixture_id !== undefined) ? "ranked" : "casual",
```

- [ ] **Step 5: Run tests**

Run: `node --env-file-if-exists=.env --test test/league_match.test.js` → PASS. Then `npm test` → all green (snapshot shape tests that `deepEqual` the whole match/room object must be extended with `league: null` / `ruleset`; update those expectations, do not loosen them).

- [ ] **Step 6: Commit**

```powershell
git add src/mm/match.js src/mm/snapshot.js test/league_match.test.js test/*.test.js
git commit -m "league: createMatch source=league (bo3, casual, fixture) + snapshot league/ruleset`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Rooms honour the league lock and start league matches

**Files:**
- Modify: `src/mm/rooms.js` (`assertNotSeated` 265, `roomView` 291-328, `lockedRoom` 338-351, join 517-562, peek 570-584, kind 693-724, start 779-857)
- Test: `test/league_rooms.test.js`

**Interfaces:**
- Produces: `export async function assertNotSeated(...)` (now exported, unchanged body); `roomView(...).league`; join/peek refuse non-allowed Discord IDs with 404 `No such lobby.`; start passes `{ source: "league", league }` for league rooms.
- Consumes: Task 3 `createMatch` `league` param.

- [ ] **Step 1: Write the failing tests**

```js
// test/league_rooms.test.js
import test from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { newToken, hashToken } from "../src/tokens.js";
import { roomsRouter } from "../src/mm/rooms.js";

let n = 0;
async function signedIn(pool, clock) {
  const p = (await pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`5${++n}000000`, `P${n}`])).rows[0];
  const raw = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at, last_used_at) VALUES ($1, $2, 't', $3, $3)`, [p.id, hashToken(raw), clock.now()]);
  return { ...p, bearer: raw };
}
async function leagueRoom(pool, clock, host, allowed, code = "LGAME2") {
  const r = (await pool.query(
    `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, seed_kind, created_at, expires_at, capacity,
                          league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids)
     VALUES ($1, $2, 'casual', 'code', 3, 'regular', $3, $4, 2, 36, 1234, 6, true, $5) RETURNING *`,
    [code, host.id, clock.now(), new Date(clock.now().getTime() + 30 * 60_000), allowed])).rows[0];
  await pool.query(`INSERT INTO mm_room_member (room_id, player_id, ready, joined_at, active) VALUES ($1, $2, false, $3, true)`, [r.id, host.id, clock.now()]);
  return r;
}
async function serve(t, clock, opts = {}) {
  const app = express();
  app.use(roomsRouter({ pool: t.pool, clock, ...opts }));
  app.use((req, res) => res.status(404).json({ ok: false, error: "No such route" }));
  app.use((err, req, res, next) => res.status(500).json({ ok: false, error: String(err?.message ?? err) })); // eslint-disable-line no-unused-vars
  const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
  const base = `http://127.0.0.1:${server.address().port}`;
  const call = async (method, path, bearer, body) => {
    const res = await fetch(base + path, { method, headers: { authorization: `Bearer ${bearer}`, ...(body ? { "content-type": "application/json" } : {}) }, body: body ? JSON.stringify(body) : undefined });
    return { status: res.status, json: await res.json() };
  };
  return { call, close: () => new Promise((r) => { server.closeAllConnections?.(); server.close(r); }) };
}

test("a stranger holding a league room's code sees a bad code, on join AND on peek", async () => {
  const t = await testPool(); const clock = fixedClock(); const s = await serve(t, clock);
  try {
    const host = await signedIn(t.pool, clock); const opp = await signedIn(t.pool, clock); const stranger = await signedIn(t.pool, clock);
    await leagueRoom(t.pool, clock, host, [host.discord_user_id, opp.discord_user_id]);
    const peek = await s.call("GET", "/v1/rooms/LGAME2", stranger.bearer);
    assert.equal(peek.status, 404); assert.equal(peek.json.error, "No such lobby.");
    const join = await s.call("POST", "/v1/mm/rooms/LGAME2/join", stranger.bearer, {});
    assert.equal(join.status, 404); assert.equal(join.json.error, "No such lobby.");
    const ok = await s.call("POST", "/v1/mm/rooms/LGAME2/join", opp.bearer, {});
    assert.equal(ok.status, 200, JSON.stringify(ok.json));
    assert.deepEqual(ok.json.room.league, { league_id: 36, fixture_id: 1234, week: 6 });
    assert.equal(ok.json.room.best_of, 3);
  } finally { await s.close(); await t.close(); }
});

test("a league room's game type is fixed", async () => {
  const t = await testPool(); const clock = fixedClock(); const s = await serve(t, clock);
  try {
    const host = await signedIn(t.pool, clock); const opp = await signedIn(t.pool, clock);
    const r = await leagueRoom(t.pool, clock, host, [host.discord_user_id, opp.discord_user_id]);
    const k = await s.call("POST", `/v1/mm/rooms/${r.id}/kind`, host.bearer, { seed_kind: "pickup" });
    assert.equal(k.status, 409); assert.equal(k.json.error, "A league game's type is fixed.");
  } finally { await s.close(); await t.close(); }
});

test("starting a league room creates a source=league match carrying the fixture", async () => {
  const t = await testPool(); const clock = fixedClock();
  const calls = [];
  const createMatch = async (client, clk, args) => {
    calls.push(args);
    return (await client.query(
      `INSERT INTO match (mode, status, best_of, seat0_player, seat1_player, room_id, source, created_at) VALUES ('casual','pairing',3,$1,$2,$3,$4,$5) RETURNING id`,
      [args.seat0, args.seat1, args.roomId, args.source, clk.now()])).rows[0];
  };
  const s = await serve(t, clock, { createMatch });
  try {
    const host = await signedIn(t.pool, clock); const opp = await signedIn(t.pool, clock);
    const r = await leagueRoom(t.pool, clock, host, [host.discord_user_id, opp.discord_user_id]);
    await s.call("POST", "/v1/mm/rooms/LGAME2/join", opp.bearer, {});
    await s.call("POST", `/v1/mm/rooms/${r.id}/ready`, host.bearer, { ready: true });
    await s.call("POST", `/v1/mm/rooms/${r.id}/ready`, opp.bearer, { ready: true });
    const st = await s.call("POST", `/v1/mm/rooms/${r.id}/start`, host.bearer);
    assert.equal(st.status, 200, JSON.stringify(st.json));
    assert.equal(calls[0].source, "league");
    assert.equal(calls[0].mode, "casual");
    assert.deepEqual(calls[0].league, { league_id: 36, fixture_id: 1234, week: 6, seat0_home: true });
  } finally { await s.close(); await t.close(); }
});

test("an ordinary code room is unchanged: anyone with the code joins, league is null", async () => {
  const t = await testPool(); const clock = fixedClock(); const s = await serve(t, clock);
  try {
    const host = await signedIn(t.pool, clock); const guest = await signedIn(t.pool, clock);
    const made = await s.call("POST", "/v1/mm/rooms", host.bearer, {});
    assert.equal(made.json.room.league, null);
    const j = await s.call("POST", `/v1/mm/rooms/${made.json.room.code}/join`, guest.bearer, {});
    assert.equal(j.status, 200);
  } finally { await s.close(); await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_rooms.test.js`
Expected: FAIL (stranger join answers 200; `room.league` undefined).

- [ ] **Step 3: Implement in rooms.js**

1. `async function assertNotSeated(` -> `export async function assertNotSeated(`.
2. Add to `SAY`: `LEAGUE_KIND: "A league game's type is fixed.",`.
3. A shared helper under the "Small shared pieces" section:

```js
/** The league block of a room row, or null for every non-league room. */
export function leagueOf(r) {
  return r?.league_fixture_id === null || r?.league_fixture_id === undefined ? null
    : { league_id: num(r.league_id), fixture_id: num(r.league_fixture_id), week: num(r.league_week) };
}

/** A league room is visible and joinable only by its two Discord IDs; every other room by anyone with the code. */
function allowedIn(room, player) {
  return !Array.isArray(room.allowed_discord_ids) || room.allowed_discord_ids.includes(String(player?.discord_user_id ?? ""));
}
```

4. `roomView`: add `league_id, league_fixture_id, league_week` to its SELECT and `league: leagueOf(r),` to the returned object.
5. `lockedRoom`: add `league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids` to the second SELECT.
6. Join, right after `const room = await lockedRoom(c, { code });`:

```js
        // A league room answers a stranger exactly as a bad code does: no "full", no "not yours".
        if (!allowedIn(room, req.player)) throw refuse(404, SAY.NO_ROOM);
```

7. Peek: add `r.allowed_discord_ids` to the SELECT, then after `if (!row)`: `if (!allowedIn(row, req.player)) return res.status(404).json({ ok: false, error: SAY.NO_ROOM });`.
8. Kind, after the host check: `if (leagueOf(room)) throw refuse(409, SAY.LEAGUE_KIND);`.
9. Start, in the `makeMatch` call:

```js
          source: leagueOf(room) ? "league" : "room",
          ...(leagueOf(room) ? { league: { ...leagueOf(room), seat0_home: room.league_seat0_home === true } } : {}),
```

- [ ] **Step 4: Run tests**

Run: `node --env-file-if-exists=.env --test test/league_rooms.test.js` → PASS (4). `npm test` → green (`mm_rooms.test.js` `deepEqual`s on room views get `league: null`).

- [ ] **Step 5: Commit**

```powershell
git add src/mm/rooms.js test/league_rooms.test.js test/*.test.js
git commit -m "league: rooms enforce allowed_discord_ids on join+peek, fixed game type, start source=league`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Open a league room (`src/league/room.js` + `POST /v1/league/room`)

**Files:**
- Create: `src/league/room.js`, `src/league/router.js`
- Test: `test/league_room_open.test.js`

**Interfaces:**
- Consumes: `leagueSite().teamHome` (Task 2), `assertNotSeated`, `roomView`, `newRoomCode`, `ROOM_CODE_ATTEMPTS` (rooms.js), `lockRoom`, `appendEvent`/`EVENT_KINDS`/`actorPlayer`, `bump`, `withTx`.
- Produces: `openLeagueRoom(pool, clock, { player, home, newCode, ttlMs })` -> `{ status, body, channels }`; `leagueRouter({ pool, clock, site, hub, publicOrigin, newCode })` (express Router); `LEAGUE_ROOM_TTL_MS = 1_800_000`; `leagueFixtureKey(id) = "league-fixture:" + id`.

- [ ] **Step 1: Write the failing tests**

```js
// test/league_room_open.test.js
import test from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { newToken, hashToken } from "../src/tokens.js";
import { withTx } from "../src/db.js";
import { lockRoom } from "../src/mm/locks.js";
import { leagueRouter, leagueFixtureKey } from "../src/league/router.js";
import { LEAGUE_ROOM_TTL_MS } from "../src/league/room.js";

let n = 0;
async function signedIn(pool, clock) {
  const p = (await pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`4${++n}000000`, `P${n}`])).rows[0];
  const raw = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at, last_used_at) VALUES ($1, $2, 't', $3, $3)`, [p.id, hashToken(raw), clock.now()]);
  return { ...p, bearer: raw };
}
/** A fake site: each Discord ID -> its team_home answer. `calls` counts reads. */
function fakeSite(answers) {
  const site = { calls: 0, down: false,
    async teamHome(id) { site.calls++; if (site.down) { const { SiteUnavailable } = await import("../src/league/site.js"); throw new SiteUnavailable("down"); } return answers[id] ?? { ok: true, member: false }; },
    logoSource: () => null };
  return site;
}
const fixture = (me, opp, { home = true, status = "scheduled", oppDiscord = opp.discord_user_id } = {}) => ({
  ok: true, member: true, league: { id: 36, season: 4, week: 6, week_label: "Week 6" },
  team: { id: home ? 493 : 486, name: "Me", logo_url: null },
  matchup: { fixture_id: 1234, week: 6, home, status, score: null, series: null, winner_team_id: null, when: null,
    opponent: { team_id: home ? 486 : 493, name: "Them", logo_url: null, discord_user_id: oppDiscord } },
  week_games: [], standings: [], recent: [], leaders: { pass: [], rush: [], rec: [] }, leaders_unit: "TD", roster: [], site_url: "x",
});
let codeN = 0;
const codes = () => `LG${String(++codeN).padStart(4, "2")}`.slice(0, 6).replace(/[01IO]/g, "2");
async function serve(t, clock, site) {
  const app = express();
  app.use(leagueRouter({ pool: t.pool, clock, site, publicOrigin: "https://api.test", newCode: codes }));
  app.use((req, res) => res.status(404).json({ ok: false }));
  app.use((err, req, res, next) => res.status(500).json({ ok: false, error: String(err?.message ?? err) })); // eslint-disable-line no-unused-vars
  const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
  const base = `http://127.0.0.1:${server.address().port}`;
  const call = async (method, path, bearer) => { const r = await fetch(base + path, { method, headers: { authorization: `Bearer ${bearer}` } }); return { status: r.status, json: await r.json() }; };
  return { call, close: () => new Promise((r) => { server.closeAllConnections?.(); server.close(r); }) };
}

test("first click creates a locked bo3 room with a 30 min TTL; the opponent's click is told to join by code", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
  const site = fakeSite({ [a.discord_user_id]: fixture(a, b, { home: true }), [b.discord_user_id]: fixture(b, a, { home: false }) });
  const s = await serve(t, clock, site);
  try {
    const first = await s.call("POST", "/v1/league/room", a.bearer);
    assert.equal(first.status, 200, JSON.stringify(first.json));
    assert.equal(first.json.action, "created");
    const room = first.json.room;
    assert.equal(room.best_of, 3); assert.equal(room.seed_kind, "regular"); assert.equal(room.you_host, true);
    assert.deepEqual(room.league, { league_id: 36, fixture_id: 1234, week: 6 });
    assert.equal(new Date(room.expires_at).getTime() - clock.now().getTime(), LEAGUE_ROOM_TTL_MS);
    const row = (await t.pool.query(`SELECT allowed_discord_ids, league_seat0_home FROM mm_room WHERE id = $1`, [room.id])).rows[0];
    assert.deepEqual(row.allowed_discord_ids.sort(), [a.discord_user_id, b.discord_user_id].sort());
    assert.equal(row.league_seat0_home, true);

    const again = await s.call("POST", "/v1/league/room", a.bearer);
    assert.equal(again.json.action, "created"); assert.equal(again.json.room.id, room.id, "idempotent for the host");

    const second = await s.call("POST", "/v1/league/room", b.bearer);
    assert.deepEqual(second.json, { ok: true, action: "join", code: room.code });
  } finally { await s.close(); await t.close(); }
});

test("two clicks racing on the fixture produce ONE room", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
  const site = fakeSite({ [a.discord_user_id]: fixture(a, b, { home: true }), [b.discord_user_id]: fixture(b, a, { home: false }) });
  const s = await serve(t, clock, site);
  try {
    // Hold the fixture key from outside so both requests park on it, then release: deterministic, not lucky.
    let release; const held = new Promise((r) => { release = r; });
    const holder = withTx(t.pool, async (c) => { await lockRoom(c, leagueFixtureKey(1234)); await held; });
    await new Promise((r) => setTimeout(r, 100));
    const both = Promise.all([s.call("POST", "/v1/league/room", a.bearer), s.call("POST", "/v1/league/room", b.bearer)]);
    await new Promise((r) => setTimeout(r, 200));
    release(); await holder;
    const [x, y] = await both;
    assert.deepEqual([x.json.action, y.json.action].sort(), ["created", "join"]);
    const n = (await t.pool.query(`SELECT count(*)::int AS n FROM mm_room WHERE league_fixture_id = 1234`)).rows[0].n;
    assert.equal(n, 1);
  } finally { await s.close(); await t.close(); }
});

test("refusals: not a member, bye, final, opponent unlinked, site down", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
  const bye = fixture(a, b); bye.matchup = { status: "bye", fixture_id: null, home: null, opponent: null };
  const answers = {};
  const site = fakeSite(answers);
  const s = await serve(t, clock, site);
  try {
    const expect = async (answer, status, error) => {
      answers[a.discord_user_id] = answer;
      const r = await s.call("POST", "/v1/league/room", a.bearer);
      assert.equal(r.status, status, JSON.stringify(r.json)); assert.equal(r.json.error, error);
    };
    await expect({ ok: true, member: false }, 403, "not_member");
    await expect(bye, 409, "no_game");
    await expect(fixture(a, b, { status: "final" }), 409, "already_final");
    await expect(fixture(a, b, { oppDiscord: null }), 409, "opponent_unlinked");
    site.down = true;
    await expect(fixture(a, b), 503, "league_unavailable");
  } finally { await s.close(); await t.close(); }
});

test("an expired room still marked open is closed and replaced, not a unique-index error", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
  const site = fakeSite({ [a.discord_user_id]: fixture(a, b), [b.discord_user_id]: fixture(b, a, { home: false }) });
  const s = await serve(t, clock, site);
  try {
    const first = await s.call("POST", "/v1/league/room", a.bearer);
    clock.advance(LEAGUE_ROOM_TTL_MS + 1);
    const fresh = await s.call("POST", "/v1/league/room", b.bearer);
    assert.equal(fresh.status, 200, JSON.stringify(fresh.json));
    assert.equal(fresh.json.action, "created");
    assert.notEqual(fresh.json.room.id, first.json.room.id);
    const old = (await t.pool.query(`SELECT status FROM mm_room WHERE id = $1`, [first.json.room.id])).rows[0];
    assert.equal(old.status, "closed");
  } finally { await s.close(); await t.close(); }
});

test("a running league match for the fixture refuses a new room with in_series", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
  const site = fakeSite({ [a.discord_user_id]: fixture(a, b, { status: "final" }) });
  const s = await serve(t, clock, site);
  try {
    const m = (await t.pool.query(
      `INSERT INTO match (mode, status, best_of, seat0_player, seat1_player, source, created_at, league_id, league_fixture_id, league_week, league_seat0_home)
       VALUES ('casual', 'running', 3, $1, $2, 'league', $3, 36, 1234, 6, true) RETURNING id`, [a.id, b.id, clock.now()])).rows[0];
    const r = await s.call("POST", "/v1/league/room", a.bearer);
    assert.equal(r.status, 409); assert.equal(r.json.error, "in_series"); assert.equal(r.json.match_id, m.id);
  } finally { await s.close(); await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_room_open.test.js`
Expected: FAIL, `Cannot find module '../src/league/router.js'`.

- [ ] **Step 3: Implement `src/league/room.js`**

```js
// src/league/room.js
// Open (or find) this week's league room. One transaction under the FIXTURE key (ROOM class, taken before any room
// or player key: locks.js order law), so two players pressing Play at once get one room -- the unique index
// mm_room_league_one_open (029) is the backstop, the lock is what makes the loser a "join" instead of a 500.
import { lockRoom } from "../mm/locks.js";
import { bump } from "../mm/cursor.js";
import { appendEvent, EVENT_KINDS, actorPlayer, ACTOR } from "../mm/events.js";
import { assertNotSeated, roomView, newRoomCode, ROOM_CODE_ATTEMPTS } from "../mm/rooms.js";
import { withTx } from "../db.js";

export const LEAGUE_ROOM_TTL_MS = 30 * 60 * 1000;
export const leagueFixtureKey = (fixtureId) => `league-fixture:${Number(fixtureId)}`;
const LIVE_STATUSES = ["pairing", "lobby", "running"];

const answer = (status, body, channels = []) => ({ status, body, channels });

/**
 * @param {object} home  the caller's FRESH team_home answer (member, with a matchup)
 * @returns {Promise<{status:number, body:object, channels:string[]}>}
 */
export async function openLeagueRoom(pool, clock, { player, home, newCode = () => newRoomCode(), ttlMs = LEAGUE_ROOM_TTL_MS }) {
  const mu = home.matchup;
  const fixtureId = Number(mu.fixture_id);
  const me = String(player.discord_user_id);
  const opp = String(mu.opponent.discord_user_id);

  for (let attempt = 0; attempt < ROOM_CODE_ATTEMPTS; attempt++) {
    try {
      return await withTx(pool, async (c) => {
        const now = clock.now();
        await lockRoom(c, leagueFixtureKey(fixtureId));

        const live = (await c.query(
          `SELECT id FROM match WHERE league_fixture_id = $1 AND status = ANY($2::text[]) AND deleted_at IS NULL LIMIT 1`,
          [fixtureId, LIVE_STATUSES])).rows[0];
        if (live) return answer(409, { ok: false, error: "in_series", match_id: live.id });

        let open = (await c.query(
          `SELECT id, code, host_player, expires_at FROM mm_room WHERE league_fixture_id = $1 AND status = 'open'`, [fixtureId])).rows[0];
        if (open && open.expires_at <= now) {
          // The prune tick has not reached it yet; it is dead all the same, and must not hold the unique index.
          await lockRoom(c, open.id);
          await c.query(`UPDATE mm_room SET status = 'closed' WHERE id = $1`, [open.id]);
          await c.query(`UPDATE mm_room_member SET active = false, ready = false WHERE room_id = $1 AND active`, [open.id]);
          await appendEvent(c, { roomId: open.id, actor: ACTOR.SYSTEM, kind: EVENT_KINDS.ROOM_CLOSED, reason: "expired", at: now });
          open = null;
        }
        if (open) {
          if (open.host_player === player.id) return answer(200, { ok: true, action: "created", room: await roomView(c, open.id, player.id) });
          return answer(200, { ok: true, action: "join", code: open.code });
        }

        // The site's "final" only stands when nothing of ours is open or running (Plan 1: a bo3 can read final early).
        if (mu.status === "final") return answer(409, { ok: false, error: "already_final" });

        await assertNotSeated(c, player.id, now);
        const room = (await c.query(
          `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, seed_kind, created_at, expires_at, capacity,
                                league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids)
           VALUES ($1, $2, 'casual', 'code', 3, 'regular', $3, $4, 2, $5, $6, $7, $8, $9) RETURNING id`,
          [newCode(), player.id, now, new Date(now.getTime() + ttlMs), Number(home.league.id), fixtureId,
            Number(mu.week ?? home.league.week), mu.home === true, [me, opp]])).rows[0];
        await c.query(
          `INSERT INTO mm_room_member (room_id, player_id, ready, field_pick, joined_at, active, party) VALUES ($1, $2, false, NULL, $3, true, false)`,
          [room.id, player.id, now]);
        await appendEvent(c, {
          roomId: room.id, actor: actorPlayer(player.id), kind: EVENT_KINDS.ROOM_CREATED, at: now,
          payload: { mode: "casual", visibility: "code", best_of: 3, seed_kind: "regular", capacity: 2,
            league: { league_id: Number(home.league.id), fixture_id: fixtureId, week: Number(mu.week ?? home.league.week), opponent_discord_user_id: opp } },
        });
        const channels = await bump(c, [player.id]);
        return answer(200, { ok: true, action: "created", room: await roomView(c, room.id, player.id) }, channels);
      });
    } catch (e) {
      // A room-code collision is the one error worth another go (rooms.js does the same).
      if (!(e?.code === "23505" && String(e.constraint ?? "").includes("code"))) throw e;
    }
  }
  return answer(503, { ok: false, error: "Could not open a lobby just now. Try again." });
}
```

> Check `events.js` exports `ACTOR` (match.js imports it from there). If `appendEvent` needs `reason` inside `payload` instead of top-level, mirror how rooms.js leave writes `reason: hostLeft ? ...` (it is top-level there).

- [ ] **Step 4: Implement `src/league/router.js` (room route only for now)**

```js
// src/league/router.js
// The launcher's League tab (spec 2026-10-08): GET /v1/league/me, POST /v1/league/room, and the league logo proxy.
// Membership is decided by the league site on EVERY call; the launcher's own flag only decides whether to draw a nav link.
import express from "express";
import { requirePlayer } from "../auth.js";
import { SiteUnavailable } from "./site.js";
import { openLeagueRoom, leagueFixtureKey } from "./room.js";

export { leagueFixtureKey };

const unavailable = (res) => res.status(503).json({ ok: false, error: "league_unavailable" });

export function leagueRouter({ pool, clock, site, hub = null, publicOrigin = "", newCode = undefined, presence = undefined } = {}) {
  if (!pool || !clock || !site) throw new Error("leagueRouter needs pool, clock and site");
  const r = express.Router();
  const auth = requirePlayer(pool, clock);
  const wake = (channels) => { try { hub?.notifyAll?.(channels ?? []); } catch { /* after commit; never a 500 */ } };

  r.post("/v1/league/room", auth, async (req, res, next) => {
    try {
      let home;
      try { home = await site.teamHome(req.player.discord_user_id, { fresh: true }); }
      catch (e) { if (e instanceof SiteUnavailable) return unavailable(res); throw e; }
      if (!home.member) return res.status(403).json({ ok: false, error: "not_member" });
      const mu = home.matchup;
      if (!mu || mu.status === "bye" || !Number.isSafeInteger(Number(mu.fixture_id)) || mu.fixture_id === null) {
        return res.status(409).json({ ok: false, error: "no_game" });
      }
      if (!mu.opponent?.discord_user_id) return res.status(409).json({ ok: false, error: "opponent_unlinked" });
      const out = await openLeagueRoom(pool, clock, { player: req.player, home, ...(newCode ? { newCode } : {}) });
      wake(out.channels);
      res.status(out.status).json(out.body);
    } catch (e) {
      // assertNotSeated / one-active-room refusals carry status + a player sentence (rooms.js `refuse`).
      if (typeof e?.status === "number" && e.expose) return res.status(e.status).json({ ok: false, error: e.message });
      if (e?.code === "23505" && String(e.constraint ?? "").includes("one_active")) return res.status(409).json({ ok: false, error: "You are already in a lobby." });
      next(e);
    }
  });

  return r;
}
```

- [ ] **Step 5: Run tests**

Run: `node --env-file-if-exists=.env --test test/league_room_open.test.js` → PASS (5). Ablation (do it, then restore): delete the `lockRoom(c, leagueFixtureKey(...))` line; the race test must FAIL (a 500 from the unique index or two rooms). Restore, re-run green.

- [ ] **Step 6: Commit**

```powershell
git add src/league/room.js src/league/router.js test/league_room_open.test.js
git commit -m "league: POST /v1/league/room (fixture-locked, idempotent, created|join)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: `GET /v1/league/me` with live merge

**Files:**
- Create: `src/league/state.js`, `src/league/merge.js`
- Modify: `src/league/router.js`
- Test: `test/league_merge.test.js` (pure), `test/league_me.test.js` (HTTP + DB)

**Interfaces:**
- Consumes: `playerLive(pool, playerId)` from `src/live.js:554` (returns `{series:[mine,theirs], score:[mine,theirs]|null, ...}` from that player's side, or null).
- Produces:
  - `leagueState(pool, clock, fixtureIds)` -> `{ rooms: Map<fixtureId, {room_id, code, host_discord_user_id, members, expires_at}>, live: Map<fixtureId, {match_id, seat0_home, host_discord_user_id, series:[s0,s1], score:[s0,s1]|null}> }`
  - `mergeLeague(site, state, { publicOrigin, opponentOnline })` -> launcher payload (contract above).
  - `PRESENCE_WINDOW_MS = 90_000`.

- [ ] **Step 1: Write the failing pure tests**

```js
// test/league_merge.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { mergeLeague } from "../src/league/merge.js";

const base = () => ({
  ok: true, member: true, league: { id: 36, week: 6 },
  team: { id: 493, logo_url: "https://nflstreetreborn.com/league/uploads/493.png" },
  matchup: { fixture_id: 1234, week: 6, home: true, status: "final", score: { home: 36, away: 20 }, series: { home: 1, away: 0 }, winner_team_id: 493, when: null,
    opponent: { team_id: 486, logo_url: null, discord_user_id: "480" } },
  week_games: [
    { fixture_id: 1234, home: { team_id: 493, logo_url: "https://nflstreetreborn.com/league/uploads/493.png" }, away: { team_id: 486, logo_url: null }, status: "final", score: { home: 36, away: 20 }, series: { home: 1, away: 0 }, winner_team_id: 493 },
    { fixture_id: 1235, home: { team_id: 482, logo_url: null }, away: { team_id: 483, logo_url: null }, status: "scheduled", score: null, series: null, winner_team_id: null },
  ],
  standings: [{ team_id: 493, logo_url: "https://nflstreetreborn.com/league/uploads/493.png" }],
  recent: [], leaders: { pass: [], rush: [], rec: [] }, leaders_unit: "TD", roster: [], site_url: "x",
});
const empty = () => ({ rooms: new Map(), live: new Map() });
const opts = { publicOrigin: "https://api.nflstreetreborn.com", opponentOnline: false };

test("logos are rewritten to the proxy; null stays null; non-members pass through", () => {
  const out = mergeLeague(base(), empty(), opts);
  assert.equal(out.team.logo_url, "https://api.nflstreetreborn.com/live-assets/league/493.png");
  assert.equal(out.week_games[0].home.logo_url, "https://api.nflstreetreborn.com/live-assets/league/493.png");
  assert.equal(out.standings[0].logo_url, "https://api.nflstreetreborn.com/live-assets/league/493.png");
  assert.equal(out.matchup.opponent.logo_url, null);
  assert.equal(out.matchup.room, null); assert.equal(out.matchup.match_id, null); assert.equal(out.matchup.opponent.online, false);
  assert.deepEqual(mergeLeague({ ok: true, member: false }, empty(), opts), { ok: true, member: false });
});

test("a running league match makes the fixture LIVE with series and score mapped from seat to home/away", () => {
  const st = empty();
  // Host (seat 0) is the AWAY owner here: seat0_home false, so seat 0's numbers are the away side's.
  st.live.set(1234, { match_id: "m-1", seat0_home: false, host_discord_user_id: "480", series: [0, 1], score: [7, 14] });
  const out = mergeLeague(base(), st, opts);
  const g = out.week_games[0];
  assert.equal(g.status, "live"); assert.equal(g.match_id, "m-1");
  assert.deepEqual(g.series, { home: 1, away: 0 }); assert.deepEqual(g.score, { home: 14, away: 7 });
  assert.equal(out.matchup.status, "live", "the site's early 'final' after game 1 is overridden");
  assert.deepEqual(out.matchup.room, { room_id: null, code: null, host_discord_user_id: "480", members: 2, expires_at: null, state: "in_series" });
  assert.equal(out.week_games[1].status, "scheduled", "other fixtures untouched");
});

test("an open room: waiting or lobby; a site 'final' with an open room reads in_progress", () => {
  const st = empty();
  st.rooms.set(1234, { room_id: "r-1", code: "K7QP2M", host_discord_user_id: "111", members: 1, expires_at: "2027-01-01T00:00:00.000Z" });
  let out = mergeLeague(base(), st, opts);
  assert.equal(out.matchup.status, "in_progress");
  assert.equal(out.week_games[0].status, "in_progress");
  assert.deepEqual(out.matchup.room, { room_id: "r-1", code: "K7QP2M", host_discord_user_id: "111", members: 1, expires_at: "2027-01-01T00:00:00.000Z", state: "waiting" });
  st.rooms.get(1234).members = 2;
  out = mergeLeague(base(), st, opts);
  assert.equal(out.matchup.room.state, "lobby");
  const sched = base(); sched.matchup.status = "scheduled"; sched.week_games[0].status = "scheduled";
  assert.equal(mergeLeague(sched, st, opts).matchup.status, "scheduled", "an open room does not invent progress");
});

test("a live score of null (no ports yet) stays null; bye matchup passes through untouched", () => {
  const st = empty();
  st.live.set(1234, { match_id: "m-1", seat0_home: true, host_discord_user_id: "1", series: [0, 0], score: null });
  assert.equal(mergeLeague(base(), st, opts).week_games[0].score, null);
  const bye = base(); bye.matchup = { status: "bye", fixture_id: null, home: null, opponent: null, score: null, series: null, winner_team_id: null, when: null };
  const out = mergeLeague(bye, st, opts);
  assert.equal(out.matchup.status, "bye"); assert.equal(out.matchup.room, null);
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_merge.test.js` → FAIL (module missing).

- [ ] **Step 3: Implement `src/league/merge.js`**

```js
// src/league/merge.js
// Pure: the site's team_home answer + this server's league rooms and running league matches -> what the launcher
// draws. The site knows the schedule and recorded results; only this server knows a series is being played right now.
// Plan 1 known semantics: the site can call a best-of-3 "final" after game 1 -- an open room or a running match here
// always wins over that.

const logo = (origin, teamId, url) => (url && Number.isSafeInteger(Number(teamId)) ? `${origin}/live-assets/league/${Number(teamId)}.png` : null);
const side = (origin, s) => (s && typeof s === "object" ? { ...s, logo_url: logo(origin, s.team_id, s.logo_url) } : s);
/** [seat0, seat1] -> { home, away } */
const byHome = (pair, seat0Home) => (Array.isArray(pair) ? (seat0Home ? { home: pair[0], away: pair[1] } : { home: pair[1], away: pair[0] }) : null);

function overlay(game, fixtureId, state) {
  const live = state.live.get(fixtureId);
  if (live) {
    return { ...game, status: "live", match_id: live.match_id,
      series: byHome(live.series, live.seat0_home) ?? game.series, score: byHome(live.score, live.seat0_home) };
  }
  const room = state.rooms.get(fixtureId);
  if (room && game.status === "final") return { ...game, status: "in_progress", match_id: null };
  return { ...game, match_id: null };
}

export function mergeLeague(site, state, { publicOrigin, opponentOnline = false } = {}) {
  if (!site?.member) return { ok: true, member: false };
  const o = String(publicOrigin ?? "").replace(/\/+$/, "");
  const out = { ...site };
  out.team = site.team ? { ...site.team, logo_url: logo(o, site.team.id, site.team.logo_url) } : site.team;
  out.standings = (site.standings ?? []).map((s) => side(o, s));
  out.week_games = (site.week_games ?? []).map((g) => overlay({ ...g, home: side(o, g.home), away: side(o, g.away) }, Number(g.fixture_id), state));

  const mu = site.matchup;
  if (!mu || mu.status === "bye" || mu.fixture_id === null || mu.fixture_id === undefined) {
    out.matchup = mu ? { ...mu, room: null, match_id: null } : mu;
    return out;
  }
  const fid = Number(mu.fixture_id);
  const merged = overlay({ ...mu }, fid, state);
  const live = state.live.get(fid);
  const room = state.rooms.get(fid);
  merged.opponent = mu.opponent ? { ...side(o, mu.opponent), online: opponentOnline === true } : mu.opponent;
  merged.room = live
    ? { room_id: null, code: null, host_discord_user_id: live.host_discord_user_id, members: 2, expires_at: null, state: "in_series" }
    : room
      ? { room_id: room.room_id, code: room.code, host_discord_user_id: room.host_discord_user_id, members: room.members, expires_at: room.expires_at,
          state: room.members >= 2 ? "lobby" : "waiting" }
      : null;
  out.matchup = merged;
  return out;
}
```

- [ ] **Step 4: Run pure tests** → PASS (4).

- [ ] **Step 5: Write the failing HTTP test**

```js
// test/league_me.test.js
import test from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { newToken, hashToken } from "../src/tokens.js";
import { leagueRouter } from "../src/league/router.js";
import { SiteUnavailable } from "../src/league/site.js";

let n = 0;
async function signedIn(pool, clock, lastUsed = clock.now()) {
  const p = (await pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`3${++n}000000`, `P${n}`])).rows[0];
  const raw = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at, last_used_at) VALUES ($1, $2, 't', $3, $4)`, [p.id, hashToken(raw), clock.now(), lastUsed]);
  return { ...p, bearer: raw };
}
async function serve(t, clock, site) {
  const app = express();
  app.use(leagueRouter({ pool: t.pool, clock, site, publicOrigin: "https://api.test" }));
  app.use((err, req, res, next) => res.status(500).json({ ok: false, error: String(err?.message ?? err) })); // eslint-disable-line no-unused-vars
  const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
  const base = `http://127.0.0.1:${server.address().port}`;
  const get = async (bearer) => { const r = await fetch(`${base}/v1/league/me`, { headers: bearer ? { authorization: `Bearer ${bearer}` } : {} }); return { status: r.status, json: await r.json() }; };
  return { get, close: () => new Promise((r) => { server.closeAllConnections?.(); server.close(r); }) };
}

test("/v1/league/me: 401 signed out, member:false, 503 when the site is down (never member:false)", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock);
  const site = { down: false, async teamHome() { if (site.down) throw new SiteUnavailable("x"); return { ok: true, member: false }; }, logoSource: () => null };
  const s = await serve(t, clock, site);
  try {
    assert.equal((await s.get(null)).status, 401);
    assert.deepEqual((await s.get(a.bearer)).json, { ok: true, member: false });
    site.down = true;
    const down = await s.get(a.bearer);
    assert.equal(down.status, 503); assert.equal(down.json.error, "league_unavailable");
  } finally { await s.close(); await t.close(); }
});

test("/v1/league/me: open room for my fixture shows as waiting with its code; opponent online from token use", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock);
  const b = await signedIn(t.pool, clock, new Date(clock.now().getTime() - 30_000));
  const home = { ok: true, member: true, league: { id: 36, week: 6 }, team: { id: 493, logo_url: null },
    matchup: { fixture_id: 1234, week: 6, home: true, status: "scheduled", score: null, series: null, winner_team_id: null, when: null,
      opponent: { team_id: 486, logo_url: null, discord_user_id: b.discord_user_id } },
    week_games: [{ fixture_id: 1234, home: { team_id: 493, logo_url: null }, away: { team_id: 486, logo_url: null }, status: "scheduled", score: null, series: null }],
    standings: [], recent: [], leaders: { pass: [], rush: [], rec: [] }, leaders_unit: "TD", roster: [], site_url: "x" };
  const site = { async teamHome() { return home; }, logoSource: () => null };
  const s = await serve(t, clock, site);
  try {
    const r = (await t.pool.query(
      `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, created_at, expires_at, capacity, league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids)
       VALUES ('K7QP2M', $1, 'casual', 'code', 3, $2, $3, 2, 36, 1234, 6, true, $4) RETURNING id`,
      [a.id, clock.now(), new Date(clock.now().getTime() + 60_000), [a.discord_user_id, b.discord_user_id]])).rows[0];
    await t.pool.query(`INSERT INTO mm_room_member (room_id, player_id, joined_at, active) VALUES ($1, $2, $3, true)`, [r.id, a.id, clock.now()]);
    const me = await s.get(a.bearer);
    assert.equal(me.status, 200, JSON.stringify(me.json));
    assert.equal(me.json.matchup.room.state, "waiting");
    assert.equal(me.json.matchup.room.code, "K7QP2M");
    assert.equal(me.json.matchup.room.host_discord_user_id, a.discord_user_id);
    assert.equal(me.json.matchup.opponent.online, true);
  } finally { await s.close(); await t.close(); }
});
```

- [ ] **Step 6: Implement `src/league/state.js`**

```js
// src/league/state.js
// This server's half of a league week: open league rooms and live league series, for a set of fixtures.
import { playerLive } from "../live.js";

export const PRESENCE_WINDOW_MS = 90_000;

export async function leagueState(pool, clock, fixtureIds) {
  const ids = [...new Set(fixtureIds.map(Number).filter(Number.isSafeInteger))];
  const rooms = new Map(); const live = new Map();
  if (!ids.length) return { rooms, live };
  const roomRows = (await pool.query(
    `SELECT r.id, r.code, r.league_fixture_id, r.expires_at, hp.discord_user_id AS host_discord,
            (SELECT count(*) FROM mm_room_member m WHERE m.room_id = r.id AND m.active)::int AS members
       FROM mm_room r JOIN player hp ON hp.id = r.host_player
      WHERE r.status = 'open' AND r.expires_at > $2 AND r.league_fixture_id = ANY($1::bigint[])`,
    [ids, clock.now()])).rows;
  for (const r of roomRows) rooms.set(Number(r.league_fixture_id), {
    room_id: r.id, code: r.code, host_discord_user_id: r.host_discord, members: r.members, expires_at: r.expires_at.toISOString() });
  const matchRows = (await pool.query(
    `SELECT m.id, m.league_fixture_id, m.league_seat0_home, m.seat0_player, m.status, hp.discord_user_id AS host_discord
       FROM match m JOIN player hp ON hp.id = m.seat0_player
      WHERE m.league_fixture_id = ANY($1::bigint[]) AND m.status IN ('pairing','lobby','running') AND m.deleted_at IS NULL`,
    [ids])).rows;
  for (const m of matchRows) {
    // playerLive answers from seat 0's side: series [seat0, seat1], score [seat0, seat1] or null.
    const view = m.status === "running" ? await playerLive(pool, m.seat0_player) : null;
    live.set(Number(m.league_fixture_id), {
      match_id: m.id, seat0_home: m.league_seat0_home === true, host_discord_user_id: m.host_discord,
      series: view?.series ?? [0, 0], score: view?.score ?? null,
    });
  }
  return { rooms, live };
}

/** True when this Discord ID's player used a token inside the presence window (auth.js stamps last_used_at). */
export async function onlineByDiscord(pool, clock, discordId) {
  if (!discordId) return false;
  const row = (await pool.query(
    `SELECT max(t.last_used_at) AS at FROM device_token t JOIN player p ON p.id = t.player_id
      WHERE p.discord_user_id = $1 AND p.deleted_at IS NULL AND t.revoked_at IS NULL AND NOT COALESCE(p.presence_hidden, false)`,
    [String(discordId)])).rows[0];
  return !!row?.at && clock.now().getTime() - new Date(row.at).getTime() <= PRESENCE_WINDOW_MS;
}
```

> `player.presence_hidden` is read by live.js; if the column name differs on main, use what `src/mm/presence.js` uses for "hide my presence" and keep the hidden player reading offline.

- [ ] **Step 7: Add the route to `src/league/router.js`**

Imports: `import { leagueState, onlineByDiscord } from "./state.js";` and `import { mergeLeague } from "./merge.js";`. Inside `leagueRouter`, before `return r;`:

```js
  r.get("/v1/league/me", auth, async (req, res, next) => {
    try {
      let site_;
      try { site_ = await site.teamHome(req.player.discord_user_id); }
      catch (e) { if (e instanceof SiteUnavailable) return unavailable(res); throw e; }
      if (!site_.member) return res.json({ ok: true, member: false });
      const fixtures = [site_.matchup?.fixture_id, ...(site_.week_games ?? []).map((g) => g.fixture_id)].filter((x) => x !== null && x !== undefined);
      const state = await leagueState(pool, clock, fixtures);
      const opponentOnline = await onlineByDiscord(pool, clock, site_.matchup?.opponent?.discord_user_id);
      res.set("cache-control", "no-store");
      res.json(mergeLeague(site_, state, { publicOrigin, opponentOnline }));
    } catch (e) { next(e); }
  });
```

- [ ] **Step 8: Run tests** — `test/league_merge.test.js` and `test/league_me.test.js` → PASS.

- [ ] **Step 9: Commit**

```powershell
git add src/league/state.js src/league/merge.js src/league/router.js test/league_merge.test.js test/league_me.test.js
git commit -m "league: GET /v1/league/me (site + open rooms + live series merge, opponent online)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: League logo proxy

**Files:**
- Modify: `src/league/router.js`
- Test: `test/league_logo.test.js`

**Interfaces:**
- Consumes: `site.logoSource(teamId)` (Task 2), `LOGO_ORIGIN`.
- Produces: `GET /live-assets/league/:file` where `:file` is `<teamId>.png`; `LOGO_TTL_MS = 86_400_000`, `LOGO_MAX_BYTES = 1_048_576`; `leagueRouter({ ..., fetchImpl })`.

- [ ] **Step 1: Write the failing test**

```js
// test/league_logo.test.js
import test from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { fixedClock } from "./helpers/fakes.js";
import { leagueRouter, LOGO_TTL_MS } from "../src/league/router.js";

const PNG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 1, 2, 3]);
function imageFetch() {
  const calls = [];
  const f = async (url) => { calls.push(url); return { status: 200, ok: true, headers: new Headers({ "content-type": "image/png" }), arrayBuffer: async () => PNG.buffer.slice(PNG.byteOffset, PNG.byteOffset + PNG.length) }; };
  f.calls = calls; return f;
}
async function serve(site, fetchImpl, clock) {
  const app = express();
  app.use(leagueRouter({ pool: { query: async () => ({ rows: [] }) }, clock, site, publicOrigin: "https://api.test", fetchImpl }));
  app.use((req, res) => res.status(404).json({ ok: false }));
  const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
  const base = `http://127.0.0.1:${server.address().port}`;
  return { get: (p) => fetch(base + p), close: () => new Promise((r) => { server.closeAllConnections?.(); server.close(r); }) };
}

test("serves a known league logo, caches it 24 h, and 404s anything unknown or malformed", async () => {
  const clock = fixedClock(); const f = imageFetch();
  const site = { logoSource: (id) => (Number(id) === 493 ? "https://nflstreetreborn.com/league/uploads/493.png" : null) };
  const s = await serve(site, f, clock);
  try {
    const r = await s.get("/live-assets/league/493.png");
    assert.equal(r.status, 200); assert.equal(r.headers.get("content-type"), "image/png");
    assert.deepEqual(Buffer.from(await r.arrayBuffer()), PNG);
    await s.get("/live-assets/league/493.png");
    assert.equal(f.calls.length, 1, "cached");
    clock.advance(LOGO_TTL_MS);
    await s.get("/live-assets/league/493.png");
    assert.equal(f.calls.length, 2, "refetched after 24 h");
    for (const p of ["/live-assets/league/486.png", "/live-assets/league/abc.png", "/live-assets/league/493.jpg", "/live-assets/league/..%2F493.png"]) {
      assert.equal((await s.get(p)).status, 404, p);
    }
  } finally { await s.close(); }
});

test("refuses a non-image or oversized upstream answer", async () => {
  const clock = fixedClock();
  const site = { logoSource: () => "https://nflstreetreborn.com/league/uploads/x.png" };
  const html = async () => ({ status: 200, ok: true, headers: new Headers({ "content-type": "text/html" }), arrayBuffer: async () => new ArrayBuffer(4) });
  const big = async () => ({ status: 200, ok: true, headers: new Headers({ "content-type": "image/png" }), arrayBuffer: async () => new ArrayBuffer(1_048_577) });
  for (const f of [html, big]) {
    const s = await serve(site, f, clock);
    try { assert.equal((await s.get("/live-assets/league/1.png")).status, 404); } finally { await s.close(); }
  }
});
```

- [ ] **Step 2: Run to verify failure** → FAIL (`LOGO_TTL_MS` not exported / 404 for 493).

- [ ] **Step 3: Implement in `src/league/router.js`**

Exports near the top:

```js
import { LOGO_ORIGIN } from "./site.js";
export const LOGO_TTL_MS = 24 * 60 * 60 * 1000;
export const LOGO_MAX_BYTES = 1_048_576;
const IMAGE_TYPES = new Set(["image/png", "image/jpeg", "image/webp", "image/gif"]);
```

Add `fetchImpl = fetch` to the `leagueRouter` options, then before `return r;`:

```js
  // The launcher's CSP allows images only from api.nflstreetreborn.com/live-assets/, so league logos are served from
  // here. Only a team id the site has named, only the league site's own origin, only an image, at most 1 MiB.
  const logos = new Map(); // teamId -> { at, type, body }
  r.get("/live-assets/league/:file", async (req, res, next) => {
    try {
      const m = /^(\d{1,9})\.png$/.exec(String(req.params.file));
      if (!m) return res.status(404).json({ ok: false, error: "No such route" });
      const teamId = Number(m[1]);
      const now = clock.now().getTime();
      let hit = logos.get(teamId);
      if (!hit || now - hit.at >= LOGO_TTL_MS) {
        const src = site.logoSource(teamId);
        if (!src || !src.startsWith(LOGO_ORIGIN)) return res.status(404).json({ ok: false, error: "No such route" });
        let up;
        try { up = await fetchImpl(src, { redirect: "error", signal: AbortSignal.timeout(8000) }); } catch { up = null; }
        const type = String(up?.headers?.get("content-type") ?? "").split(";")[0].trim().toLowerCase();
        const body = up && up.status === 200 && IMAGE_TYPES.has(type) ? Buffer.from(await up.arrayBuffer()) : null;
        if (!body || body.length > LOGO_MAX_BYTES) return res.status(404).json({ ok: false, error: "No such route" });
        hit = { at: now, type, body };
        logos.set(teamId, hit);
      }
      res.set("content-type", hit.type);
      res.set("cache-control", "public, max-age=3600");
      res.send(hit.body);
    } catch (e) { next(e); }
  });
```

- [ ] **Step 4: Run tests** → PASS (2).

- [ ] **Step 5: Commit**

```powershell
git add src/league/router.js test/league_logo.test.js
git commit -m "league: /live-assets/league/<team>.png logo proxy (site origin only, image only, 1 MiB, 24 h)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Bot feed for the opponent ping

The live bot (Railway `reborn-live-bot`, separate repo) is pull-based (`src/botFeeds.js`). This task exposes the feed; the bot's DM/ping is a follow-up in the bot repo (see Open risks). Ping failure never affects the room.

**Files:**
- Modify: `src/botFeeds.js`
- Test: `test/bot_feeds_league.test.js`

**Interfaces:**
- Produces: `GET /v1/bot/league/rooms?since=<event id>` (bot token) -> `{ok:true, cursor, rooms:[{event_id, room_id, code, at, host:{discord_user_id, name}, opponent_discord_user_id, league_id, fixture_id, week}]}`.

- [ ] **Step 1: Write the failing test**

```js
// test/bot_feeds_league.test.js
import test from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { botFeedsRouter } from "../src/botFeeds.js";

test("league room feed lists room_created events that carry a league block, bot token only", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const p = (await t.pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ('111', 'Adam') RETURNING *`)).rows[0];
    const room = (await t.pool.query(
      `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, created_at, expires_at) VALUES ('K7QP2M', $1, 'casual', 'code', 3, $2, $2) RETURNING id`,
      [p.id, clock.now()])).rows[0];
    const at = new Date(clock.now().getTime() - 10_000);
    await t.pool.query(`INSERT INTO mm_event (room_id, actor, kind, payload, at) VALUES ($1, $2, 'room_created', $3, $4)`,
      [room.id, `player:${p.id}`, { league: { league_id: 36, fixture_id: 1234, week: 6, opponent_discord_user_id: "480" } }, at]);
    await t.pool.query(`INSERT INTO mm_event (room_id, actor, kind, payload, at) VALUES ($1, $2, 'room_created', $3, $4)`,
      [room.id, `player:${p.id}`, { mode: "casual" }, at]);
    const app = express();
    app.use(botFeedsRouter({ pool: t.pool, clock, isBot: (req) => req.headers.authorization === "Bearer bot" }));
    const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
    const base = `http://127.0.0.1:${server.address().port}`;
    try {
      assert.equal((await fetch(`${base}/v1/bot/league/rooms`)).status, 401);
      const r = await (await fetch(`${base}/v1/bot/league/rooms?since=0`, { headers: { authorization: "Bearer bot" } })).json();
      assert.equal(r.rooms.length, 1);
      assert.deepEqual({ ...r.rooms[0], event_id: undefined, at: undefined }, {
        event_id: undefined, at: undefined, room_id: room.id, code: "K7QP2M", host: { discord_user_id: "111", name: "Adam" },
        opponent_discord_user_id: "480", league_id: 36, fixture_id: 1234, week: 6 });
      assert.ok(r.cursor >= r.rooms[0].event_id);
    } finally { server.closeAllConnections?.(); server.close(); }
  } finally { await t.close(); }
});
```

> If `mm_event` requires more NOT NULL columns than the INSERT above gives (check `migrations/006_matchmaking_runtime.sql`), add them in the test; do not relax the table.

- [ ] **Step 2: Run to verify failure** → FAIL (404).

- [ ] **Step 3: Implement** in `botFeedsRouter`, after the queue events route:

```js
  // League rooms opened from the launcher's League tab (spec 2026-10-08): the bot pings the opponent. Same settled
  // cursor as the queue feed. Only room_created events whose payload carries a league block.
  r.get("/v1/bot/league/rooms", bot, async (req, res, next) => {
    try {
      const since = parseSince(req.query.since);
      if (since === null) return res.status(400).json({ ok: false, error: "since must be a whole number" });
      const rows = (await pool.query(
        `SELECT e.id, e.at, e.payload, r.id AS room_id, r.code, p.discord_user_id, p.display_name
           FROM mm_event e JOIN mm_room r ON r.id = e.room_id JOIN player p ON p.id = r.host_player
          WHERE e.id > $1 AND e.at <= $2 AND e.kind = 'room_created' AND e.payload ? 'league'
          ORDER BY e.id LIMIT $3`, [since, cutoff(), QUEUE_EVENTS_MAX])).rows;
      const top = (await pool.query(`SELECT max(id) AS id FROM mm_event WHERE id > $1 AND at <= $2`, [since, cutoff()])).rows[0].id;
      const cursor = rows.length === QUEUE_EVENTS_MAX ? Number(rows[rows.length - 1].id) : top === null ? since : Number(top);
      res.json({ ok: true, cursor, rooms: rows.map((e) => ({
        event_id: Number(e.id), at: iso(e.at), room_id: e.room_id, code: e.code,
        host: { discord_user_id: e.discord_user_id, name: e.display_name },
        opponent_discord_user_id: String(e.payload.league.opponent_discord_user_id ?? "") || null,
        league_id: Number(e.payload.league.league_id), fixture_id: Number(e.payload.league.fixture_id), week: Number(e.payload.league.week),
      })) });
    } catch (e) { next(e); }
  });
```

- [ ] **Step 4: Run test** → PASS.

- [ ] **Step 5: Commit**

```powershell
git add src/botFeeds.js test/bot_feeds_league.test.js
git commit -m "league: bot feed /v1/bot/league/rooms for the opponent ping`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Mount, env, README, full suite

**Files:**
- Modify: `src/app.js`, `src/server.js` (env passthrough only), `README.md`
- Test: `test/league_app.test.js`

- [ ] **Step 1: Write the failing test**

```js
// test/league_app.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { testPool } from "./helpers/db.js";
import { fixedClock, fakeDiscord } from "./helpers/fakes.js";
import { buildApp } from "../src/app.js";

test("app mounts the league routes; unset LEAGUE_SITE_URL answers 503, not 404 or 500", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const app = buildApp({ pool: t.pool, clock, discord: fakeDiscord(), env: { MM_ENABLED: "0", PUBLIC_ORIGIN: "https://api.test", STATE_SECRET: "x".repeat(32), ADMIN_TOKEN: "a".repeat(32) } });
    const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
    const base = `http://127.0.0.1:${server.address().port}`;
    try {
      assert.equal((await fetch(`${base}/v1/league/me`)).status, 401, "mounted and behind requirePlayer");
      assert.equal((await fetch(`${base}/live-assets/league/1.png`)).status, 404);
    } finally { server.closeAllConnections?.(); server.close(); }
  } finally { await t.close(); }
});
```

> Copy the exact `buildApp` env a passing `test/app.test.js` case uses if the one above is missing a required key.

- [ ] **Step 2: Run** → FAIL (`/v1/league/me` is 404).

- [ ] **Step 3: Mount in `src/app.js`**

Imports:

```js
import { leagueRouter } from "./league/router.js";
import { leagueSite } from "./league/site.js";
```

After `app.use(nicknameRouter(...))`:

```js
  // The launcher's League tab (spec 2026-10-08). Behind the global /v1 limiter like every player route. The site's
  // token never leaves this process. Unset LEAGUE_SITE_URL/TOKEN = every /v1/league/me is a 503 (tab shows cached data).
  const league = leagueSite({ url: env.LEAGUE_SITE_URL, token: env.LEAGUE_SITE_TOKEN, clock });
  app.use(leagueRouter({ pool, clock, site: league, hub: mm?.hub ?? null, publicOrigin: env.PUBLIC_ORIGIN ?? "" }));
```

> The logo route `/live-assets/league/:file` is not under `/v1`, so it is outside the global limiter; it only ever fetches upstream once per team per 24 h, which is its bound.

`src/server.js`: if env is built as an explicit object (lines ~11-35), add next to `LEAGUE_FORWARD_URL`:

```js
  // League tab (optional): the site's bot.php and its ingest token. Never through need().
  LEAGUE_SITE_URL: process.env.LEAGUE_SITE_URL ?? "",
  LEAGUE_SITE_TOKEN: process.env.LEAGUE_SITE_TOKEN ?? "",
```

README: under env vars add `LEAGUE_SITE_URL` (e.g. `https://nflstreetreborn.com/league/api/bot.php`) and `LEAGUE_SITE_TOKEN` (the site's `INGEST_TOKEN`); under routes add `GET /v1/league/me`, `POST /v1/league/room`, `GET /live-assets/league/<team>.png`, `GET /v1/bot/league/rooms`, each with one line from the contract.

- [ ] **Step 4: Full suite**

Run: `npm test`
Expected: baseline count + all new tests, 0 failures. Paste the summary line into the commit body.

- [ ] **Step 5: Commit**

```powershell
git add src/app.js src/server.js README.md test/league_app.test.js
git commit -m "league: mount /v1/league routes + logo proxy; LEAGUE_SITE_URL/TOKEN`n`n<npm test summary line>`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Deploy dark to Railway `reborn-api` and smoke test

Dark = no shipped launcher calls these routes yet. Prerequisite: Plan 1's `team_home` is live (Plan 1 Task 5).

- [ ] **Step 1: Ask Adam before deploying.** The deploy runs migration 029 on the live database and replaces the live API process. Get an explicit yes for this deploy.

- [ ] **Step 2: Set the env vars** (Railway MCP `set-variables` or the dashboard): `LEAGUE_SITE_URL=https://nflstreetreborn.com/league/api/bot.php`, `LEAGUE_SITE_TOKEN=<site INGEST_TOKEN, from nsr_local\deploy\secret, never printed>`.

- [ ] **Step 3: Deploy from the worktree**

```powershell
cd "C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-league"
git status --short   # must be empty: only committed code ships
railway up --ci --service reborn-api
```
Expected: build OK, `/health` green, logs show migration 029 applied.

- [ ] **Step 4: Smoke**

```powershell
$api = "https://api.nflstreetreborn.com"
curl.exe -s -o NUL -w "%{http_code}`n" "$api/v1/league/me"                     # 401
curl.exe -s -o NUL -w "%{http_code}`n" "$api/live-assets/league/999999.png"    # 404
# Signed-in check with Adam's own launcher token (read from the OS vault by Adam, never pasted into a log):
curl.exe -s -H "Authorization: Bearer $env:REBORN_TOKEN" "$api/v1/league/me"
```
Expected: 401, 404, and Adam's Season 4 team payload (`member:true`, his matchup, `week_games`), or `member:false` for a non-member account. A 503 means the site URL/token is wrong: fix the env, never leave it half-configured.

- [ ] **Step 5: Record**

Merge `feat/league-tab` into backend `main` only after Adam confirms (memory law: no shared-branch surprises). Update memory note for the league tab with the deploy commit and the smoke results.

---

## Self-review

- Spec coverage: `/v1/league/me` (T6), 60 s cache (T2), 503 path (T2/T6), `/v1/league/room` idempotent + race (T5), allowed IDs on join (T4), hidden from public list (rooms are code-only already; peek refusal T4), 30 min expiry (T5 TTL + stale-open replacement), bo3 Ranked settings (T3 `LEAGUE_BEST_OF` + `ruleset`), live merge with series score (T6), logo proxy 24 h (T7), bot ping (T8 feed; bot repo follow-up), result tagging (T3 `match.league` + existing `forward.js` once the launcher sends `intent.league`), deploy dark (T10). Cancel = existing leave (documented deviation).
- Plan 1 alignment: `{ok,member}` everywhere; `in_progress`/`final` override; `opponent.discord_user_id` null -> `opponent_unlinked`; `when` untouched; `color`, `week_label`, `poid`, `leaders_unit` pass through unmodified (merge spreads the site object).
- Names used across tasks: `leagueSite`, `SiteUnavailable`, `LOGO_ORIGIN`, `openLeagueRoom`, `LEAGUE_ROOM_TTL_MS`, `leagueFixtureKey`, `leagueRouter`, `leagueState`, `onlineByDiscord`, `mergeLeague`, `LEAGUE_BEST_OF`, `leagueOf`, `assertNotSeated` — each defined once, consistent.

## Open risks

1. **Bot ping needs a bot-repo change** (poll `/v1/bot/league/rooms`, DM or ping the opponent). Not in this plan; until it ships the opponent sees the room on their tab's 60 s refresh / long-poll wake only.
2. **Launcher must send `intent.league=true`** on league uploads or `forward.js` never forwards them (consent rule). Plan 3 owns that.
3. **`playerLive` score is from seat 0's side and needs a play with ports**; between games it reports `[0,0]`. Scoreboard shows series score then, which is correct.
4. **Migration number collision** with another in-flight backend branch (Task 0 Step 3 checks).
5. **Site `final` after game 1 with nothing open** (both players closed the launcher mid-series and the match ended/abandoned): `POST /v1/league/room` answers `already_final`. A commissioner fixes the site row; not automatable here.
