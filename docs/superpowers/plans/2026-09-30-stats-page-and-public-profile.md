# Stats Page v2 and Public Profile Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A private Stats dashboard in the launcher (Elo, your matches, your players), and a public, online-only player profile in the launcher and on the web at `online.nflstreetreborn.com/u/<player>`. **Matches come first:** every list is a list of matches, and every result counts matches.

**Architecture:** The backend owns every number. It loads a player's games once into plain "game records" (`src/profile/load.js`), groups them into matches (`src/profile/matches.js`), and pure modules turn those into each section (`src/profile/*.js`). Two routes serve them: a public, online-only profile and a signed-in dashboard that includes offline games. The launcher and the web page only draw what the server sends. The web page and the launcher view share the same data shape and the same section order.

**Tech Stack:**
- Backend: Node 22 ESM, Express 5, Postgres (`pg`), `node --test`.
- Launcher: Rust (Tauri 2, `reborn-core` with a `ureq` client), vanilla ES modules in `crates/reborn-ui/ui`, `node --test` for `*.test.mjs`.

**Spec:** `docs/superpowers/specs/2026-09-30-stats-page-and-public-profile-design.md` (launcher repo, commit 7b7492b). The approved mockup is `.superpowers/brainstorm/181729-1790747853/content/stats-layout-v17.html` in the launcher worktree. Open it in a browser. Its markup and CSS are the look to reproduce, with totals as one muted line (spec, "Look").

**Repos:**
- **Backend:** `C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-pbp`, branch `feat/play-by-play`. Tests: `npm test` (needs `DATABASE_URL_TEST`, already set in `.env`).
- **Launcher:** work in a NEW worktree off `release/3.0.27-beta-pbp`, never in the `rel-pbp` checkout itself. A global launcher redesign runs there in parallel (Adam, 09-30).
  - Create it once: `git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher rel-pbp" worktree add "../NFL Street Reborn Launcher stats-v2" -b feat/stats-profile-v2 release/3.0.27-beta-pbp`.
  - Every launcher path in this plan is relative to that new worktree. The spec and this plan stay in `rel-pbp`.
  - Tests: `cargo test -p reborn-core -p reborn-ui -j 2`. Never run a bare workspace build without `-j 2`: a run was killed for low memory.
  - UI tests: `node --test crates/reborn-ui/ui/<file>.test.mjs`.

## Global Constraints

- **Matches first (spec "Matches first").** Every list shows matches, and each match holds its games. Results count matches: record, win %, streak, recent form, head to head, by team, by queue and by game type. Stats count games: pts / gm, yds / gm, per game, wins vs losses, career tables, combos and records. A matchmade match is one `match` row. A game with no match row is a match of one game. In the result rows of Task 4, the field `games` counts matches, and the UI labels it "Matches".
- **The parallel launcher redesign is out of scope.** Build on today's look (DESIGN.md and the v17 mockup). Adam restyles these pages afterwards.
  - Never edit `launcher.css`, `shell.css`, `styles.css`, `product-ui.css`, `onboarding.css`, `finish.css`, `live-view.css`, `DESIGN.md`, `shell.js`, `app-shell.js`, or any other page's HTML or CSS.
  - `stats.html` keeps its markup as is. Only `stats.js` and `stats.css` change.
  - All new CSS is scoped to `.page-stats` or `.page-profile`.
  - If a shared file this plan must touch (`game.js`, `live-game-render.js`, `live.js`, `live-common.js`, `main.rs`) has redesign commits on `release/3.0.27-beta-pbp` by then, rebase onto them and keep their changes. Never revert or reformat them.
- The public profile counts **online games only**. It filters with `g.mode = 'online'` in SQL, whatever `show_offline` says. Offline games appear only in `GET /v1/me/dashboard` (spec §2, §3).
- The profile shows only `status = 'accepted'` games. The dashboard lists every non-void game. Records, streaks and head to head count only games with a result (`resultOf`).
- Per-player stats come only from `plays_status = 'verified'` games (spec §3).
- **Capture order vs play side order:** `participation.team_index` and `player_game_stat.team_index` are CAPTURE order. Play `offense_side`, `scoreAfter` and `drives` are PLAY SIDE order. Convert with `sideTeams(g).team` from `src/web/live-render.js`, where `team[side] === capture index`. Never compare a capture index to a side.
- Wording (spec, Look):
  - "Head to head", "Recent form", "Most played", "Best passer", "Best rusher", "Best receiver", "Best DL/LB", "Ballhawk".
  - Offline games show the label `Offline`.
  - "Unranked" when there are no ranked games. "No online games yet." for an empty profile. "None yet" for an empty role or table.
- Type: Inter only, and no Anton on these pages.
  - Page title 28/700, section 15/600, body 13/400.
  - Labels 11/600 uppercase with .08em letter spacing.
  - Numbers use `font-variant-numeric: tabular-nums` at 600–700.
- Colours come from DESIGN.md tokens only:
  - `--red #e0272e` for fills, `--red-ink #ff7a7f` for red text, `--teal #22c7b4` for positive values.
  - Panels are glass: `backdrop-filter: blur(24px)` over the translucent `--panel`.
- **Totals:** one muted line under a table, for example `Total  212/371 · 3,108 yds · 41 TD`. There is no banded or bold total row.
- Every player name carries a 16px team logo. A role card shows a 38px round headshot with a team badge, or the name's initial when there is no portrait.
- Server data reaches the DOM through `textContent` only, never `innerHTML`. Images are allowed only from `/live-assets/` (through `assetUrl`) and `https://cdn.discordapp.com/avatars/`.
- Accessibility:
  - Every control is a real `<button>` or `<a>` and reachable by Tab. `:focus-visible` is always drawn.
  - Every number has a visible label. Images carry `alt=""` when text beside them names the same thing.
- **Hosts:**
  - Pages use `https://online.nflstreetreborn.com` (`PAGE_ORIGIN`).
  - Data, sign-in and `/live-assets/` stay on `https://api.nflstreetreborn.com` forever.
- **Outward-facing steps need Adam's explicit go:** backend deploy, launcher publish, the DNS record for `online`, and `railway domain`. No task in this plan runs any of them. A launcher build that prints `online.` links must not ship before `online.` answers.
- Never HTTP-probe unpublished URLs. Never `git stash` without a tag. Never commit other sessions' uncommitted files: `git add` named paths only.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Rulings made while planning (spec gaps, decided here)

| # | Ruling | Why | Cost if wrong |
|---|---|---|---|
| R1 | Dashboard is `GET /v1/me/dashboard` and the team override is `POST /v1/me/plays-as`, not the spec's `/v1/players/me/*` and `PUT`. | The backend's existing "me" routes are `/v1/me/show_offline` and `/v1/me/region`, both POST. The launcher's `Http` trait has no PUT. | Two route names to change. |
| R2 | A career player is keyed by roster PGID (`p<pid>`) when the stored plays' roster knows it, else `n<name>\|<team>`. | `player_game_stat` has no pid. `rosterOf(plays)` has it by DIGP. | A player may split in two across old and new games. |
| R3 | Game type: a game is **pickup** when the newest play carrying `teams` has every side at `tgid -1`. It is **quick** when a side is a catalog team, by tgid or by capture name. Otherwise it is **unknown**. No runtime change. | `livegame.js` and `liveAssets.js` both record "-1 in Pick-Up mode". | If a real pickup capture lacks `teams`, those games show as unknown. Task 15 checks one real pickup game. |
| R4 | The Defense "TD" column is the capture's own `pick_six + fum_td` for that DIGP, from `game.players[].stats`. | Box-score lines have no defensive TD field. | None. |
| R5 | "Laterals & pitches" shows three tiles: **Pitches**, **Plays with a pitch**, **Yds on those plays**. | The decoder does not separate a lateral from a pitch. `summary.pitches` is the reliable list; `summary.laterals` is the fallback for older decoders. | A label change. |
| R6 | "3-and-outs" means your drives that ended without a first down or a touchdown. | NFL Street has no punts, and this is the closest honest meaning. | A label change. |
| R7 | A game's Elo change is the sum of that match's `rating_event.delta` for this player with `cause = 'accepted'`. It is shown on the match's LAST recorded game only. | Elo is rated per match (best of 3), not per game. | Display only. |
| R8 | Only the match log is paged (`?section=matches&before=&limit=`). Career tables and head to head are returned whole. The UI shows 5 rows and "All ›" expands in place. | A player uses at most a few hundred NFL players, so the payload stays small. | Payload size, fixable by paging later. |
| R9 | By team always has a **Pickup** row, even at 0 games, placed last. By queue always lists Ranked, Casual, Private room. By game type always lists Pickup and Quick game. | Adam: "make sure pickup shows in by team". | None. |
| R10 | Online status on the profile follows presence.js's own rule (`presence_hidden` false and seen within `ONLINE_WITHIN_MS`). It is omitted when presence is hidden. | Spec: "online status when the player shows it". | None. |
| R11 | `/u/:player` takes a UUID, or a display name that matches exactly one live player, case-insensitive. Anything else is a 404. | Two players can share a display name. | Names that collide get no short link. |

## Review Focus

1. **A two-human game where the capture order and the side order differ.** The player's stats, their team, their Elo and every play-derived number must be for the side they played. The test fixture must swap the order (Task 2 and Task 6 tests).
2. **An owner with `show_offline = true` and offline games.** No offline game may enter any public profile number, list or record. That includes head to head, records and career tables (Task 7 test).
3. **A player with only offline, unranked or unverified games.** The profile shows the header and "No online games yet.". Elo shows "Unranked". No role card is drawn from nothing, and nothing divides by zero (Task 4, Task 7 and Task 11 tests).
4. **Player-typed names.** Team and display names like `<img onerror>` or `</script>` must render as text on the web page and in the launcher (Task 8 and Task 13 tests).
5. **A game with plays but no roster, and an old capture with no `teams` on any play.** Records fall back to the name key and an unknown game type without throwing (Task 1 and Task 6 tests).

---

## File map

**Backend (`reborn-backend-pbp`)**
- Create `src/profile/classify.js`: game type, queue, and my side.
- Create `src/profile/matches.js`: group games into matches.
- Create `src/profile/teamGame.js`: one game's team numbers from its plays.
- Create `src/profile/aggregate.js`: game-level sections (summary, form, head to head, by team, queue and type, wins vs losses, most played team).
- Create `src/profile/players.js`: player-level sections (career tables, best players, combos, pitches, per game, records).
- Create `src/profile/load.js`: DB to game records, Elo, and the header.
- Create `src/profile/router.js`: `GET /v1/me/dashboard`, `GET /v1/players/:id/profile`, `POST /v1/me/plays-as`, and the cache.
- Create `migrations/009_plays_as.sql`.
- Create `src/web/profile.html`, `src/web/profile-page.js`, `src/web/profile.css`, `src/web/cover-color.js`.
- Modify `src/webRouter.js` (`/u/:player`, host redirect), `src/app.js` (mount), `src/pbp/avatar.js` (size argument), `src/server.js` (optional `PAGE_ORIGIN`, `API_HOSTS`).
- Tests:
  - `test/profile_classify.test.js`
  - `test/profile_matches.test.js`
  - `test/profile_teamgame.test.js`
  - `test/profile_aggregate.test.js`
  - `test/profile_players.test.js`
  - `test/profile_load.test.js`
  - `test/profile_routes.test.js`
  - `test/web_profile_page.test.js`
  - `test/web_host.test.js`
  - `test/cover_color.test.js`

**Launcher (new worktree `NFL Street Reborn Launcher stats-v2`, branch `feat/stats-profile-v2`)**
- Modify `crates/reborn-core/src/identity.rs`: add the `PAGE_ORIGIN` const.
- Modify `crates/reborn-ui/src/shell_tools.rs`: the allow-list covers `/live/`, `/games/` and `/u/` on both origins. This also fixes today's refused "Open on web" for `/games/`.
- Modify `crates/reborn-core/src/live_view.rs`: add `dashboard`, `player_profile` and `set_plays_as`.
- Modify `crates/reborn-ui/src/live_view.rs`: add the `stats_dashboard`, `player_profile` and `set_plays_as` commands.
- Modify `crates/reborn-ui/src/main.rs`: register the three commands.
- Modify `crates/reborn-ui/ui/live-common.js`: add `PAGE_ORIGIN`. `game.js` and `live.js` open pages through it.
- Create `crates/reborn-ui/ui/stats-model.js` and `stats-model.test.mjs`: the pure view models for both pages.
- Create `crates/reborn-ui/ui/cover-color.js` and `cover-color.test.mjs`.
- Rewrite `crates/reborn-ui/ui/stats.js`, `stats.css`, `stats.html`.
- Create `crates/reborn-ui/ui/profile.html`, `profile.js`, `profile.css`.
- Modify `crates/reborn-ui/ui/game.js`: add ‹ Previous / Next ›.

---

## Backend

### Task 1: Classify a game (game type, queue, my side)

**Files:**
- Create: `src/profile/classify.js`
- Test: `test/profile_classify.test.js`

**Interfaces:**
- Produces:
  - `gameTypeOf(plays: object[], teams: {name}[], assets) -> 'pickup'|'quick'|'unknown'`
  - `queueOf(mode: 'online'|'offline', match: {mode, source}|null) -> 'ranked'|'casual'|'private'|null`
  - `mySideOf(game: {players, teams}, teamIndex) -> 0|1|null`

- [ ] **Step 1: Write the failing test**

```js
// test/profile_classify.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { gameTypeOf, queueOf, mySideOf } from "../src/profile/classify.js";

const assets = {
  team: (id) => (id === 14 ? { tgid: 14, name: "Falcons" } : null),
  teamByName: (n) => (typeof n === "string" && n.toLowerCase() === "falcons" ? { tgid: 14, name: "Falcons" } : null),
};

test("gameTypeOf: -1 on every side of the newest play with teams is pickup", () => {
  const plays = [{ seq: 1, teams: [{ side: 0, tgid: 14 }, { side: 1, tgid: 3 }] }, { seq: 2, teams: [{ side: 0, tgid: -1 }, { side: 1, tgid: -1 }] }];
  assert.equal(gameTypeOf(plays, [], assets), "pickup");
});
test("gameTypeOf: a catalog tgid or a catalog capture name is quick", () => {
  assert.equal(gameTypeOf([{ seq: 1, teams: [{ side: 0, tgid: 14 }, { side: 1, tgid: -1 }] }], [], assets), "quick");
  assert.equal(gameTypeOf([], [{ name: "Falcons" }, { name: "Nobody" }], assets), "quick");
});
test("gameTypeOf: no teams anywhere and no catalog names is unknown; junk input never throws", () => {
  assert.equal(gameTypeOf([{ seq: 1 }], [{ name: "Team One" }, { name: "Team Two" }], assets), "unknown");
  assert.equal(gameTypeOf(null, null, assets), "unknown");
  assert.equal(gameTypeOf([{ seq: 1, teams: "x" }, null], undefined, assets), "unknown");
});
test("queueOf: offline has no queue; no match is a private room; room and invite are private", () => {
  assert.equal(queueOf("offline", null), null);
  assert.equal(queueOf("online", null), "private");
  assert.equal(queueOf("online", { mode: "ranked", source: "queue" }), "ranked");
  assert.equal(queueOf("online", { mode: "casual", source: "rematch" }), "casual");
  assert.equal(queueOf("online", { mode: "ranked", source: "room" }), "private");
  assert.equal(queueOf("online", { mode: "casual", source: "invite" }), "private");
});
test("mySideOf maps a capture index onto a play side, both orders", () => {
  const swapped = { teams: [{ name: "A" }, { name: "B" }], players: [{ side: 0, team_index: 1 }, { side: 1, team_index: 0 }] };
  assert.equal(mySideOf(swapped, 0), 1);
  assert.equal(mySideOf(swapped, 1), 0);
  const straight = { teams: [{ name: "A" }, { name: "B" }], players: [{ side: 0, team_index: 0 }] };
  assert.equal(mySideOf(straight, 0), 0);
  assert.equal(mySideOf(straight, null), null);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test test/profile_classify.test.js`
Expected: FAIL with `Cannot find module '../src/profile/classify.js'`.

- [ ] **Step 3: Write the implementation**

```js
// src/profile/classify.js
// What kind of game a record is: its game type (Pickup / Quick game, ruling R3), its online queue, and which play
// SIDE the player was on. Pure.
import { sideTeams } from "../web/live-render.js";

const bySeq = (plays) => (Array.isArray(plays) ? plays : []).filter((p) => p && Number.isInteger(p.seq)).sort((a, b) => a.seq - b.seq);

export function gameTypeOf(plays, teams, assets) {
  const withTeams = bySeq(plays).filter((p) => Array.isArray(p.teams));
  const last = withTeams[withTeams.length - 1];
  if (last) {
    const sides = last.teams.filter((t) => t && (t.side === 0 || t.side === 1));
    if (sides.length > 0 && sides.every((t) => t.tgid === -1)) return "pickup";
    if (sides.some((t) => assets.team(t.tgid))) return "quick";
  }
  for (const t of Array.isArray(teams) ? teams : []) if (assets.teamByName(t?.name)) return "quick";
  return "unknown";
}

export function queueOf(mode, match) {
  if (mode !== "online") return null;
  if (!match) return "private";
  if (match.source === "room" || match.source === "invite") return "private";
  return match.mode === "ranked" ? "ranked" : "casual";
}

export function mySideOf(game, teamIndex) {
  if (teamIndex !== 0 && teamIndex !== 1) return null;
  const { team } = sideTeams(game);
  if (team[0] === teamIndex) return 0;
  if (team[1] === teamIndex) return 1;
  return null;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test test/profile_classify.test.js`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add src/profile/classify.js test/profile_classify.test.js
git commit -m "profile: classify a game's type, queue and the player's side"
```

### Task 2: One game's team numbers from its plays

**Files:**
- Create: `src/profile/teamGame.js`
- Test: `test/profile_teamgame.test.js`

**Interfaces:**
- Consumes: `drives(plays)` (`src/pbp/drives.js`), `scoreAfter(plays)` (`src/pbp/livegame.js`), `safetyOf(play)` (`src/pbp/safety.js`).
- Produces: `teamGame(plays, side) -> TeamGame|null`, where TeamGame is:
  `{ points, drives, first_downs, turnovers, conv1:[made,att], conv2:[made,att], three_outs, longest_drive, lead_changes, biggest_lead, biggest_deficit, safeties, pitches, pitch_plays, pitch_yds, longest_td }`
  - `longest_td` is null when you scored none.

- [ ] **Step 1: Write the failing test**

```js
// test/profile_teamgame.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { teamGame } from "../src/profile/teamGame.js";

const S = (o) => ({ yards_known: true, yards: 0, first_down: false, touchdown: false, turnover: false, points: 0, ...o });
const plays = [
  { seq: 1, offense_side: 1, summary: S({ kind: "run", yards: 12, first_down: true }) },
  { seq: 2, offense_side: 1, summary: S({ kind: "pass", yards: 30, touchdown: true, points: 6, pitches: [{ from: 1, to: 2 }] }) },
  { seq: 3, offense_side: 1, conversion: true, summary: S({ kind: "conversion", text: "2-point try: X pass. Good.", conversion_good: true, points: 2 }) },
  { seq: 4, offense_side: 0, summary: S({ kind: "run", yards: 3 }) },
  { seq: 5, offense_side: 0, summary: S({ kind: "pass", yards: 44, touchdown: true, points: 6 }) },
  { seq: 6, offense_side: 0, conversion: true, summary: S({ kind: "conversion", text: "1-point try: Y run. No good.", conversion_good: false }) },
  { seq: 7, offense_side: 1, summary: S({ kind: "run", yards: 2 }) },
  { seq: 8, offense_side: 1, summary: S({ kind: "pass", yards: 0, turnover: true }) },
];

test("teamGame counts only the given side's offense, in play side order", () => {
  const me = teamGame(plays, 1);
  assert.equal(me.points, 8);
  assert.equal(me.drives, 2);
  assert.equal(me.first_downs, 1);
  assert.equal(me.turnovers, 1);
  assert.deepEqual(me.conv2, [1, 1]);
  assert.deepEqual(me.conv1, [0, 0]);
  assert.equal(me.three_outs, 1, "the second drive had no first down and no TD");
  assert.equal(me.longest_drive, 2);
  assert.equal(me.longest_td, 30);
  assert.equal(me.pitches, 1);
  assert.equal(me.pitch_plays, 1);
  assert.equal(me.pitch_yds, 30);
  assert.equal(me.biggest_lead, 8);
  const them = teamGame(plays, 0);
  assert.equal(them.points, 6);
  assert.deepEqual(them.conv1, [0, 1]);
  assert.equal(them.biggest_deficit, 8);
  assert.equal(them.longest_td, 44);
});

test("lead changes count flips of the leader, not ties", () => {
  const p = [
    { seq: 1, offense_side: 0, summary: S({ points: 6, touchdown: true }) },
    { seq: 2, offense_side: 1, summary: S({ points: 6, touchdown: true }) },
    { seq: 3, offense_side: 1, summary: S({ points: 6, touchdown: true }) },
    { seq: 4, offense_side: 0, summary: S({ points: 6, touchdown: true }) },
    { seq: 5, offense_side: 0, summary: S({ points: 6, touchdown: true }) },
  ];
  assert.equal(teamGame(p, 0).lead_changes, 2);
});

test("safeties scored by the side count, from the game's tsst stat", () => {
  const g5 = JSON.parse(readFileSync(new URL("./fixtures/pbp/g5-falcons-packers-safeties.json", import.meta.url), "utf8"));
  const total = teamGame(g5.plays, 0).safeties + teamGame(g5.plays, 1).safeties;
  assert.ok(total >= 2, "the fixture has two safeties");
});

test("no side or no plays: null, or zeros, never a throw", () => {
  assert.equal(teamGame(plays, null), null);
  assert.equal(teamGame(null, 0).points, 0);
  assert.equal(teamGame([{ seq: 1 }, null, { seq: "x" }], 0).drives, 0);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test test/profile_teamgame.test.js`
Expected: FAIL, module not found.

- [ ] **Step 3: Write the implementation**

```js
// src/profile/teamGame.js
// One side's team numbers for one verified game, from its stored plays (spec §3: never from team totals). Pure.
// `side` is a PLAY SIDE (0/1), not a capture index. Rulings R5 (pitches) and R6 (3-and-outs) define two of them.
import { drives } from "../pbp/drives.js";
import { scoreAfter } from "../pbp/livegame.js";
import { safetyOf } from "../pbp/safety.js";

const TRY_RE = /^([12])-point try/;
const isConv = (p) => p?.summary?.kind === "conversion" || p?.conversion === true;

export function teamGame(plays, side) {
  if (side !== 0 && side !== 1) return null;
  const list = (Array.isArray(plays) ? plays : []).filter((p) => p && Number.isInteger(p.seq)).sort((a, b) => a.seq - b.seq);
  const out = { points: 0, drives: 0, first_downs: 0, turnovers: 0, conv1: [0, 0], conv2: [0, 0], three_outs: 0, longest_drive: 0,
    lead_changes: 0, biggest_lead: 0, biggest_deficit: 0, safeties: 0, pitches: 0, pitch_plays: 0, pitch_yds: 0, longest_td: null };
  const firstDownSeqs = new Set();
  for (const p of list) {
    const s = p.summary ?? {};
    const saf = safetyOf(p);
    if (saf && saf.side === side) out.safeties += 1;
    if (p.offense_side !== side) continue;
    if (isConv(p)) {
      const m = TRY_RE.exec(typeof s.text === "string" ? s.text : "");
      if (m) { const c = m[1] === "1" ? out.conv1 : out.conv2; c[1] += 1; if (s.conversion_good === true) c[0] += 1; }
      continue;
    }
    if (s.first_down === true) { out.first_downs += 1; firstDownSeqs.add(p.seq); }
    if (s.turnover === true) out.turnovers += 1;
    const n = Array.isArray(s.pitches) ? s.pitches.length : Array.isArray(s.laterals) ? s.laterals.length : 0;
    if (n > 0) {
      out.pitches += n; out.pitch_plays += 1;
      if (s.yards_known === true && Number.isFinite(s.yards)) out.pitch_yds += s.yards;
    }
    if (s.touchdown === true && s.turnover !== true && s.yards_known === true && Number.isFinite(s.yards))
      out.longest_td = out.longest_td === null ? s.yards : Math.max(out.longest_td, s.yards);
  }
  for (const d of drives(list)) {
    if (d.side !== side) continue;
    out.drives += 1;
    out.longest_drive = Math.max(out.longest_drive, d.plays);
    let fd = false;
    for (const q of firstDownSeqs) if (q >= d.first_seq && q <= d.last_seq) { fd = true; break; }
    if (!fd && d.result !== "touchdown") out.three_outs += 1;
  }
  const score = scoreAfter(list);
  let leader = null;
  for (const p of list) {
    const [a, b] = score.get(p.seq);
    const mine = side === 0 ? a : b, theirs = side === 0 ? b : a;
    out.biggest_lead = Math.max(out.biggest_lead, mine - theirs);
    out.biggest_deficit = Math.max(out.biggest_deficit, theirs - mine);
    const now = mine > theirs ? "me" : theirs > mine ? "them" : null;
    if (now !== null && leader !== null && now !== leader) out.lead_changes += 1;
    if (now !== null) leader = now;
  }
  if (list.length) { const last = score.get(list[list.length - 1].seq); out.points = side === 0 ? last[0] : last[1]; }
  return out;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test test/profile_teamgame.test.js`
Expected: PASS, 4 tests. If the safeties assertion fails, open the fixture and count the `tsst` changes by `who`. Then correct the expected total to the fixture's real count, and keep the assertion exact.

- [ ] **Step 5: Commit**

```bash
git add src/profile/teamGame.js test/profile_teamgame.test.js
git commit -m "profile: one side's team numbers for a game, from its plays"
```

### Task 3: Group games into matches (matches first)

**Files:**
- Create: `src/profile/matches.js`
- Test: `test/profile_matches.test.js`

**Interfaces:**
- Consumes GameRecords (shape in Task 4). Each carries `match: { id, status, winner_seat, my_seat, best_of, game_number } | null`, which Task 6 loads.
- Produces:
  - `groupMatches(records: GameRecord[] newest first) -> MatchRecord[] newest first`.
  - `matchRow(m)` returns the MatchRecord without its heavy fields, with its `games` as `gameRow`s. `gameRow` is in `aggregate.js` (Task 4), so `matchRow` goes there too, in Task 4.

```
MatchRecord = {
  id: match id, or "g:<game id>" for a game with no match row,
  match_id: uuid|null, at: newest game's `at`, mode, queue, game_type, best_of: number,
  me, opp, opponent, opponent_kind,          // from the match's newest game
  games: GameRecord[]  (oldest first: game_number, then at),
  series: [won, lost] | null                  // null for a one-game match
  score: [points for, points against] | null  // summed over games with a score
  result: 'win'|'loss'|'tie'|null,
  elo_delta: number|null                      // sum of its games' elo_delta, null when none
}
```

- [ ] **Step 1: Write the failing test**

```js
// test/profile_matches.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { groupMatches } from "../src/profile/matches.js";

let n = 0;
const g = (o) => ({ id: `g${++n}`, at: `2026-09-30T0${n}:00:00Z`, mode: "online", queue: "ranked", game_type: "quick", status: "accepted",
  me: { name: "Falcons" }, opp: { name: "Eagles" }, opponent: { player_id: "p2", display_name: "Playz" }, opponent_kind: "human",
  score: [10, 7], result: "win", elo_delta: null, match: null, ...o });
const M = (id, game_number, extra = {}) => ({ id, status: "complete", winner_seat: 0, my_seat: 0, best_of: 3, game_number, ...extra });

test("a best of three is one match: series, summed points, winner from the match row, Elo once", () => {
  const rs = [g({ match: M("m1", 1), result: "win", score: [14, 10] }), g({ match: M("m1", 2), result: "loss", score: [7, 28] }),
    g({ match: M("m1", 3), result: "win", score: [21, 14], elo_delta: 16 })].reverse();   // newest first, like the loader
  const [m] = groupMatches(rs);
  assert.equal(m.id, "m1");
  assert.deepEqual(m.games.map((x) => x.match.game_number), [1, 2, 3]);
  assert.deepEqual(m.series, [2, 1]);
  assert.deepEqual(m.score, [42, 52]);
  assert.equal(m.result, "win", "the match row's winner seat decides, not the points");
  assert.equal(m.elo_delta, 16);
  assert.equal(m.best_of, 3);
});

test("games with no match row are one-game matches, in newest-first order with real matches", () => {
  const off = g({ mode: "offline", queue: null, at: "2026-09-30T09:00:00Z" });
  const old = g({ at: "2026-09-29T09:00:00Z" });
  const bo = g({ match: M("m2", 1, { best_of: 1 }), at: "2026-09-30T05:00:00Z" });
  const ms = groupMatches([off, bo, old]);
  assert.deepEqual(ms.map((m) => m.id), [`g:${off.id}`, "m2", `g:${old.id}`]);
  assert.equal(ms[0].series, null);
  assert.deepEqual(ms[0].score, [10, 7]);
  assert.equal(ms[0].result, "win");
});

test("no winner seat: the series decides; nothing decided: null; a lost seat is a loss", () => {
  const a = groupMatches([g({ match: M("m3", 2, { winner_seat: null, status: "abandoned" }), result: "loss" }),
    g({ match: M("m3", 1, { winner_seat: null, status: "abandoned" }), result: "loss" })]);
  assert.equal(a[0].result, "loss");
  const b = groupMatches([g({ match: M("m4", 1, { winner_seat: null, status: "running" }), result: null, score: null })]);
  assert.equal(b[0].result, null);
  assert.equal(b[0].score, null);
  const lostBySeat = groupMatches([g({ match: M("m5", 1, { winner_seat: 1, my_seat: 0 }), result: "win" })]);
  assert.equal(lostBySeat[0].result, "loss");
});

test("empty and junk input", () => {
  assert.deepEqual(groupMatches([]), []);
  assert.deepEqual(groupMatches(null), []);
  assert.deepEqual(groupMatches([null]), []);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test test/profile_matches.test.js`
Expected: FAIL, module not found.

- [ ] **Step 3: Write the implementation**

```js
// src/profile/matches.js
// Matches first (spec "Matches first"): a matchmade match is one entry holding its games; a game with no match row is
// a match of one game. Results come from the match row's winner seat, else from the games won and lost. Pure.
export function groupMatches(records) {
  const by = new Map();
  for (const r of Array.isArray(records) ? records : []) {
    if (!r) continue;
    const key = r.match?.id ?? `g:${r.id}`;
    if (!by.has(key)) by.set(key, []);
    by.get(key).push(r);
  }
  const out = [];
  for (const [key, list] of by) {
    const games = [...list].sort((a, b) => (a.match?.game_number ?? 0) - (b.match?.game_number ?? 0) || String(a.at).localeCompare(String(b.at)));
    const newest = [...list].sort((a, b) => String(b.at).localeCompare(String(a.at)))[0];
    const m = newest.match;
    const won = games.filter((x) => x.result === "win").length, lost = games.filter((x) => x.result === "loss").length;
    let result = null;
    if (m && (m.winner_seat === 0 || m.winner_seat === 1) && (m.my_seat === 0 || m.my_seat === 1)) result = m.winner_seat === m.my_seat ? "win" : "loss";
    else if (games.length === 1) result = games[0].result;
    else if (games.some((x) => x.result)) result = won > lost ? "win" : lost > won ? "loss" : "tie";
    const scored = games.filter((x) => Array.isArray(x.score));
    const deltas = games.map((x) => x.elo_delta).filter((d) => Number.isFinite(d));
    out.push({
      id: key, match_id: m?.id ?? null, at: newest.at, mode: newest.mode, queue: newest.queue, game_type: newest.game_type,
      best_of: m?.best_of ?? 1, me: newest.me, opp: newest.opp, opponent: newest.opponent, opponent_kind: newest.opponent_kind,
      games, series: games.length > 1 || (m?.best_of ?? 1) > 1 ? [won, lost] : null,
      score: scored.length ? [scored.reduce((a, x) => a + x.score[0], 0), scored.reduce((a, x) => a + x.score[1], 0)] : null,
      result, elo_delta: deltas.length ? deltas.reduce((a, d) => a + d, 0) : null,
    });
  }
  return out.sort((a, b) => String(b.at).localeCompare(String(a.at)));
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test test/profile_matches.test.js`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add src/profile/matches.js test/profile_matches.test.js
git commit -m "profile: group games into matches (matches first)"
```

### Task 4: Result sections by match, averages by game

**Files:**
- Create: `src/profile/aggregate.js`
- Test: `test/profile_aggregate.test.js`

**Interfaces:**
- Consumes **MatchRecords** from `groupMatches` (Task 3), newest first. Each MatchRecord holds its GameRecords in `games`. The GameRecord shape, produced by Task 6:

```
GameRecord = {
  id, at: ISO string, mode: 'online'|'offline', queue: 'ranked'|'casual'|'private'|null,
  game_type: 'pickup'|'quick'|'unknown', status, plays_status, verified: boolean,
  my_side: 0|1|null,
  me:  { name, abbrev, tgid, logo_url } ,  opp: { name, abbrev, tgid, logo_url },   // name may be null
  opponent: { player_id, display_name, avatar_url } | null,  opponent_kind: 'human'|'cpu'|'guest'|null,
  score: [mine, theirs] | null, result: 'win'|'loss'|'tie'|null, elo_delta: number|null,
  lines: Line[]  (my side only; [] unless verified),
  team: TeamGame|null (Task 2; null unless verified),
  plays: object[] ([] unless verified),  names: Map<digp, Line-identity>,
  match: { id, status, winner_seat, my_seat, best_of, game_number } | null   // Task 6; null for a game with no match row
}
Line = { key, digp, name, team, tgid, logo_url, portrait_url, stats, def_td }
```

- Produces:
  - `summary(records) -> { games, wins, losses, ties, win_pct, streak, pts_per_game, yds_per_game }`
    - `win_pct` is a number from 0 to 1 with 3 decimals, or null.
    - `streak` is like `"W2"`, or null.
  - `recentForm(records, n = 10) -> { results: ('W'|'L'|'T')[], pts_for, pts_against }`
  - `headToHead(records) -> H2H[]`
  - `byTeam(records) -> TeamRow[]` (Pickup row always present, last when empty)
  - `byQueue(records) -> QueueRow[]` (ranked, casual, private)
  - `byGameType(records) -> TypeRow[]` (pickup, quick)
  - `winsVsLosses(records) -> { wins: Split, losses: Split }`, where `Split = { games, pts, yds, to }` holds per-game averages or null.
  - `mostPlayedTeam(records) -> { name, abbrev, tgid, logo_url } | null`
  - `gameRow(record) -> the public row` (every field except `lines`, `plays`, `names`, `team`)
  - `matchRow(match) -> the match with its games as gameRows`

- [ ] **Step 1: Write the failing test**

```js
// test/profile_aggregate.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { summary, recentForm, headToHead, byTeam, byQueue, byGameType, winsVsLosses, mostPlayedTeam, gameRow, matchRow } from "../src/profile/aggregate.js";
import { groupMatches } from "../src/profile/matches.js";
const M = (rs) => groupMatches(rs);

const FAL = { name: "Falcons", abbrev: "ATL", tgid: 14, logo_url: "/live-assets/r/v/teams/14-256.png" };
const EAG = { name: "Eagles", abbrev: "PHI", tgid: 13, logo_url: "/live-assets/r/v/teams/13-256.png" };
const NONE = { name: null, abbrev: null, tgid: null, logo_url: null };
const playz = { player_id: "p2", display_name: "Playz", avatar_url: null };
let n = 0;
const rec = (o) => ({ id: `g${++n}`, at: new Date(Date.UTC(2026, 8, 30) - n * 3600e3).toISOString(), mode: "online", queue: "ranked", game_type: "quick",
  status: "accepted", plays_status: "verified", verified: true, my_side: 0, me: FAL, opp: EAG, opponent: playz, opponent_kind: "human",
  score: [21, 14], result: "win", elo_delta: null, lines: [], team: null, plays: [], names: new Map(), ...o });

const lines = (pass, rush) => [{ key: "p1", name: "QB", stats: { pass_yds: pass, rush_yds: 0 } }, { key: "p2", name: "RB", stats: { pass_yds: 0, rush_yds: rush } }];
const tg = (to) => ({ turnovers: to });

test("summary: record, win %, streak from the newest game, averages", () => {
  const rs = [rec({ result: "win", score: [30, 10], lines: lines(200, 100), team: tg(1) }), rec({ result: "win", score: [20, 13] }),
    rec({ result: "loss", score: [7, 28] }), rec({ result: null, score: null, status: "held" })];
  const s = summary(M(rs));
  assert.deepEqual([s.games, s.wins, s.losses, s.ties], [3, 2, 1, 0]);
  assert.equal(s.win_pct, 0.667);
  assert.equal(s.streak, "W2");
  assert.equal(s.pts_per_game, 19);
  assert.equal(s.yds_per_game, 300, "only verified games with lines count toward yards");
});

test("summary of nothing is zeros and nulls, never NaN", () => {
  const s = summary([]);
  assert.deepEqual(s, { games: 0, wins: 0, losses: 0, ties: 0, win_pct: null, streak: null, pts_per_game: null, yds_per_game: null });
});

test("recentForm takes the newest decided games", () => {
  const rs = [rec({ result: "loss", score: [3, 7] }), rec({ result: null, score: null }), rec({ result: "win", score: [10, 0] })];
  assert.deepEqual(recentForm(M(rs), 10), { results: ["L", "W"], pts_for: 13, pts_against: 7 });
});

test("headToHead groups humans only, with last meeting, biggest win and loss", () => {
  const rs = [rec({ result: "loss", score: [14, 36] }), rec({ result: "win", score: [33, 7] }), rec({ result: "win", score: [21, 20] }),
    rec({ opponent: null, opponent_kind: "cpu", result: "win", score: [50, 0] })];
  const h = headToHead(M(rs));
  assert.equal(h.length, 1);
  assert.equal(h[0].display_name, "Playz");
  assert.deepEqual([h[0].wins, h[0].losses, h[0].ties], [2, 1, 0]);
  assert.equal(h[0].last.result, "loss");
  assert.equal(h[0].biggest_win.margin, 26);
  assert.equal(h[0].biggest_loss.margin, 22);
});

test("byTeam: a Pickup row always exists, pickup games count there, not under a team name", () => {
  const rs = [rec({}), rec({ game_type: "pickup", me: NONE, result: "loss", score: [0, 6] })];
  const t = byTeam(M(rs));
  assert.equal(t.find((r) => r.key === "tgid:14").games, 1);
  const pickup = t.find((r) => r.key === "pickup");
  assert.equal(pickup.name, "Pickup");
  assert.equal(pickup.games, 1);
  const empty = byTeam(M([rec({})]));
  assert.equal(empty[empty.length - 1].key, "pickup");
  assert.equal(empty[empty.length - 1].games, 0);
});

test("byQueue and byGameType always list their rows, offline and unknown left out", () => {
  const rs = [rec({ queue: "casual" }), rec({ mode: "offline", queue: null, game_type: "pickup" }), rec({ game_type: "unknown" })];
  assert.deepEqual(byQueue(M(rs)).map((r) => [r.key, r.games]), [["ranked", 1], ["casual", 1], ["private", 0]]);
  assert.deepEqual(byGameType(M(rs)).map((r) => [r.key, r.games]), [["pickup", 1], ["quick", 1]]);
});

test("winsVsLosses averages points, yards and turnovers per split", () => {
  const rs = [rec({ result: "win", score: [30, 10], lines: lines(200, 100), team: tg(0) }), rec({ result: "loss", score: [10, 20], lines: lines(100, 50), team: tg(3) })];
  const w = winsVsLosses(M(rs));
  assert.deepEqual(w.wins, { games: 1, pts: 30, yds: 300, to: 0 });
  assert.deepEqual(w.losses, { games: 1, pts: 10, yds: 150, to: 3 });
  assert.deepEqual(winsVsLosses([]).wins, { games: 0, pts: null, yds: null, to: null });
});

test("mostPlayedTeam skips pickup and unnamed teams", () => {
  assert.equal(mostPlayedTeam(M([rec({}), rec({ me: EAG }), rec({ me: EAG }), rec({ game_type: "pickup", me: NONE })])).name, "Eagles");
  assert.equal(mostPlayedTeam(M([rec({ game_type: "pickup", me: NONE })])), null);
});

test("a 2–1 series is ONE match win; per-game averages use all three games", () => {
  const mm = (num, o) => rec({ match: { id: "m9", status: "complete", winner_seat: 0, my_seat: 0, best_of: 3, game_number: num }, ...o });
  const ms = M([mm(3, { result: "win", score: [21, 14] }), mm(2, { result: "loss", score: [7, 28] }), mm(1, { result: "win", score: [14, 10] })]);
  const s = summary(ms);
  assert.deepEqual([s.games, s.wins, s.losses], [1, 1, 0]);
  assert.equal(s.pts_per_game, 14);
  assert.deepEqual(recentForm(ms).results, ["W"]);
  assert.equal(headToHead(ms)[0].wins, 1);
  assert.equal(byQueue(ms)[0].games, 1);
  assert.equal(matchRow(ms[0]).games.length, 3);
  assert.equal("lines" in matchRow(ms[0]).games[0], false);
});

test("gameRow drops the heavy fields", () => {
  const r = gameRow(rec({ lines: [1], plays: [1] }));
  for (const k of ["lines", "plays", "names", "team"]) assert.equal(k in r, false);
  assert.equal(r.me.name, "Falcons");
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test test/profile_aggregate.test.js`
Expected: FAIL, module not found.

- [ ] **Step 3: Write the implementation**

```js
// src/profile/aggregate.js
// Game-level sections of the dashboard and the profile, from GameRecords (newest first). Pure.
const round1 = (x) => Math.round(Number((x * 10).toPrecision(12))) / 10;
const round3 = (x) => Math.round(Number((x * 1000).toPrecision(12))) / 1000;
const avg = (xs, r = round1) => (xs.length ? r(xs.reduce((a, b) => a + b, 0) / xs.length) : null);
const decided = (rs) => rs.filter((r) => r.result === "win" || r.result === "loss" || r.result === "tie");
const letter = (r) => (r === "win" ? "W" : r === "loss" ? "L" : "T");
// Results count MATCHES; per-game numbers count the games inside them (spec "Matches first").
const gamesOf = (ms) => ms.flatMap((m) => (Array.isArray(m.games) ? m.games : [m]));
const yardsOf = (r) => (r.verified && r.lines.length ? r.lines.reduce((a, l) => a + (l.stats?.pass_yds ?? 0) + (l.stats?.rush_yds ?? 0), 0) : null);
const tally = (rs) => {
  const d = decided(rs);
  const wins = d.filter((r) => r.result === "win").length, losses = d.filter((r) => r.result === "loss").length;
  const ties = d.length - wins - losses;
  return { games: d.length, wins, losses, ties, win_pct: d.length ? round3((wins + ties / 2) / d.length) : null,
    pts_per_game: avg(decided(gamesOf(d)).filter((r) => r.score).map((r) => r.score[0])) };
};

export function summary(records) {
  const t = tally(records);
  const d = decided(records);
  let streak = null;
  if (d.length) { const first = d[0].result; let k = 0; while (k < d.length && d[k].result === first) k += 1; streak = `${letter(first)}${k}`; }
  const yds = gamesOf(d).map(yardsOf).filter((y) => y !== null);
  return { games: t.games, wins: t.wins, losses: t.losses, ties: t.ties, win_pct: t.win_pct, streak,
    pts_per_game: t.pts_per_game, yds_per_game: avg(yds, Math.round) };
}

export function recentForm(records, n = 10) {
  const d = decided(records).slice(0, n);
  return { results: d.map((r) => letter(r.result)),
    pts_for: d.reduce((a, r) => a + (r.score?.[0] ?? 0), 0), pts_against: d.reduce((a, r) => a + (r.score?.[1] ?? 0), 0) };
}

export function headToHead(records) {
  const by = new Map();
  for (const r of decided(records)) {
    if (r.opponent_kind !== "human" || !r.opponent?.player_id) continue;
    const id = r.opponent.player_id;
    if (!by.has(id)) by.set(id, { player_id: id, display_name: r.opponent.display_name, avatar_url: r.opponent.avatar_url,
      wins: 0, losses: 0, ties: 0, pts_for: 0, pts_against: 0, last: null, biggest_win: null, biggest_loss: null });
    const h = by.get(id);
    h[r.result === "win" ? "wins" : r.result === "loss" ? "losses" : "ties"] += 1;
    if (r.score) { h.pts_for += r.score[0]; h.pts_against += r.score[1]; }
    const ref = { game_id: r.id, at: r.at, result: r.result, score: r.score, margin: r.score ? Math.abs(r.score[0] - r.score[1]) : null };
    if (!h.last) h.last = ref;   // records are newest first
    if (r.result === "win" && ref.margin !== null && (!h.biggest_win || ref.margin > h.biggest_win.margin)) h.biggest_win = ref;
    if (r.result === "loss" && ref.margin !== null && (!h.biggest_loss || ref.margin > h.biggest_loss.margin)) h.biggest_loss = ref;
  }
  return [...by.values()].sort((a, b) => (b.wins + b.losses + b.ties) - (a.wins + a.losses + a.ties) || String(b.last?.at).localeCompare(String(a.last?.at)));
}

const row = (key, name, team, rs) => ({ key, name, abbrev: team?.abbrev ?? null, tgid: team?.tgid ?? null, logo_url: team?.logo_url ?? null, ...tally(rs) });

export function byTeam(records) {
  const groups = new Map();
  const pickup = [];
  for (const r of records) {
    if (r.game_type === "pickup") { pickup.push(r); continue; }
    if (!r.me?.name) continue;
    const key = r.me.tgid !== null && r.me.tgid !== undefined ? `tgid:${r.me.tgid}` : `name:${r.me.name}`;
    if (!groups.has(key)) groups.set(key, { team: r.me, rs: [] });
    groups.get(key).rs.push(r);
  }
  const rows = [...groups.entries()].map(([key, g]) => row(key, g.team.name, g.team, g.rs)).sort((a, b) => b.games - a.games);
  const p = row("pickup", "Pickup", null, pickup);
  return p.games > 0 ? [...rows, p].sort((a, b) => b.games - a.games) : [...rows, p];
}

const QUEUES = [["ranked", "Ranked"], ["casual", "Casual"], ["private", "Private room"]];
const TYPES = [["pickup", "Pickup"], ["quick", "Quick game"]];
export const byQueue = (records) => QUEUES.map(([k, name]) => row(k, name, null, records.filter((r) => r.mode === "online" && r.queue === k)));
export const byGameType = (records) => TYPES.map(([k, name]) => row(k, name, null, records.filter((r) => r.game_type === k)));

export function winsVsLosses(records) {
  const split = (res) => {
    const rs = decided(gamesOf(records)).filter((r) => r.result === res);
    return { games: rs.length, pts: avg(rs.filter((r) => r.score).map((r) => r.score[0])),
      yds: avg(rs.map(yardsOf).filter((y) => y !== null), Math.round), to: avg(rs.filter((r) => r.team).map((r) => r.team.turnovers)) };
  };
  return { wins: split("win"), losses: split("loss") };
}

export function mostPlayedTeam(records) {
  const t = byTeam(records).find((r) => r.key !== "pickup" && r.games > 0);
  return t ? { name: t.name, abbrev: t.abbrev, tgid: t.tgid, logo_url: t.logo_url } : null;
}

export function gameRow(r) {
  const { lines, plays, names, team, ...rest } = r;
  return rest;
}

export function matchRow(m) {
  const { games, ...rest } = m;
  return { ...rest, games: (games ?? []).map(gameRow) };
}
```

`byTeam` counts ALL records in `tally` but only decided games in `games`. That is intended: a team row shows the record.

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test test/profile_aggregate.test.js`
Expected: PASS, 10 tests.

- [ ] **Step 5: Commit**

```bash
git add src/profile/aggregate.js test/profile_aggregate.test.js
git commit -m "profile: match-level results and game-level averages (record, form, head to head, by team/queue/type)"
```

### Task 5: Player-level sections

**Files:**
- Create: `src/profile/players.js`
- Test: `test/profile_players.test.js`

**Interfaces:**
- Consumes: `sumStats(list)` from `src/players.js` (already exported), plus the GameRecord and Line shapes (Task 4).
- Produces:
  - `careerTables(records) -> { passing: Table, rushing: Table, receiving: Table, defense: Table }`, where `Table = { rows: Row[], total: Row|null }`. Each Row has `{ key, name, team, tgid, logo_url, portrait_url, games, ...columns }`.
    - Passing columns: `cmp, att, yds, td, int, air, yac, ypa, rtg, long, sacked`.
    - Rushing columns: `att, yds, avg, td, long, btk, yac`.
    - Receiving columns: `rec, yds, avg, yac, td, long, drops`.
    - Defense columns: `tkl, tfl, sck, int, pbu, ff, fr, td`.
  - `bestPlayers(tables) -> { passer, rusher, receiver, dl_lb, ballhawk, most_played }`. Each value is `{ key, name, team, tgid, logo_url, portrait_url, value, unit }` or null.
  - `combos(records) -> { passer, receiver, rec, yds, td }[]`, sorted by yds, descending.
  - `pitches(records) -> { pitches, pitch_plays, pitch_yds }`
  - `perGame(records) -> { games, yds, pass, rush, first_downs, turnovers, conv1_pct, conv2_pct, pts_per_drive, three_outs, longest_drive, lead_changes, biggest_lead, safeties }`
    - Averages are per verified game.
    - `longest_drive` and `biggest_lead` are career maxima. `safeties` is a career total.
  - `careerRecords(records) -> Rec[]`, where `Rec = { key, label, value, game_id, at, name|null }`.

- [ ] **Step 1: Write the failing test**

```js
// test/profile_players.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { careerTables, bestPlayers, combos, pitches, perGame, careerRecords } from "../src/profile/players.js";

const L = (key, name, stats, def_td = 0) => ({ key, digp: 0, name, team: "Falcons", tgid: 14, logo_url: "/l.png", portrait_url: null, stats, def_td });
const Z = { pass_att: 0, pass_cmp: 0, pass_yds: 0, pass_td: 0, pass_int: 0, sacked: 0, rush_att: 0, rush_yds: 0, rush_td: 0, rec: 0, rec_yds: 0, rec_td: 0, rec_yac: 0,
  tackles: 0, tfl: 0, sacks: 0, def_int: 0, ff: 0, fr: 0, broken_tackles: 0, yds_after_contact: 0, drops: 0, pass_yac: 0, pass_breakups: 0,
  pitches_thrown: 0, pitches_caught: 0, pass_long: null, rush_long: null, rec_long: null };
const s = (o) => ({ ...Z, ...o });
const TG = { points: 21, drives: 5, first_downs: 8, turnovers: 1, conv1: [1, 2], conv2: [0, 1], three_outs: 2, longest_drive: 7, lead_changes: 1,
  biggest_lead: 14, biggest_deficit: 3, safeties: 1, pitches: 2, pitch_plays: 1, pitch_yds: 15, longest_td: 44 };
const g = (id, lines, extra = {}) => ({ id, at: `2026-09-2${id.length}T00:00:00Z`, verified: true, result: "win", score: [21, 14], lines, team: TG, plays: [], names: new Map(), my_side: 0, ...extra });

const games = [
  g("a", [L("p1", "Penix", s({ pass_att: 20, pass_cmp: 14, pass_yds: 240, pass_td: 3, pass_long: 44 })), L("p2", "Bijan", s({ rush_att: 10, rush_yds: 80, rush_long: 30, rec: 2, rec_yds: 20, rec_long: 12 })),
    L("p3", "Terrell", s({ tackles: 3, def_int: 2, pass_breakups: 1 }), 1)]),
  g("bb", [L("p1", "Penix", s({ pass_att: 10, pass_cmp: 5, pass_yds: 90, pass_int: 1, pass_long: 30 })), L("p4", "Bolton", s({ tackles: 6, sacks: 2 }))]),
];

test("careerTables sums per player across games, and the total is the sum of every line", () => {
  const t = careerTables(games);
  const penix = t.passing.rows.find((r) => r.key === "p1");
  assert.deepEqual([penix.games, penix.cmp, penix.att, penix.yds, penix.td, penix.int, penix.long], [2, 19, 30, 330, 3, 1, 44]);
  assert.equal(t.passing.total.yds, 330);
  assert.equal(t.rushing.rows.length, 1);
  assert.equal(t.receiving.rows[0].name, "Bijan");
  assert.equal(t.defense.rows.find((r) => r.key === "p3").td, 1);
  assert.equal(t.defense.total.tkl, 9);
  assert.equal(t.passing.rows[0].logo_url, "/l.png");
});

test("careerTables of nothing: empty rows and a null total", () => {
  const t = careerTables([]);
  assert.deepEqual(t.passing, { rows: [], total: null });
});

test("bestPlayers picks each role; ballhawk needs an interception", () => {
  const b = bestPlayers(careerTables(games));
  assert.equal(b.passer.name, "Penix"); assert.equal(b.passer.value, 330); assert.equal(b.passer.unit, "pass_yds");
  assert.equal(b.rusher.name, "Bijan");
  assert.equal(b.dl_lb.name, "Bolton"); assert.equal(b.dl_lb.value, 8);
  assert.equal(b.ballhawk.name, "Terrell");
  assert.equal(b.most_played.name, "Penix"); assert.equal(b.most_played.value, 2);
  const none = bestPlayers(careerTables([g("c", [L("p1", "Penix", s({ pass_att: 1, pass_cmp: 1, pass_yds: 5 }))])]));
  assert.equal(none.ballhawk, null);
  assert.equal(none.rusher, null);
});

test("combos read passer and receiver from my offense's completed passes", () => {
  const names = new Map([[1, L("p1", "Penix", Z)], [2, L("p5", "London", Z)]]);
  const plays = [
    { seq: 1, offense_side: 0, summary: { kind: "pass", passer: 1, receiver: 2, yards_known: true, yards: 20, touchdown: true, turnover: false } },
    { seq: 2, offense_side: 0, summary: { kind: "pass", passer: 1, receiver: 2, yards_known: true, yards: 10, touchdown: false, turnover: false } },
    { seq: 3, offense_side: 1, summary: { kind: "pass", passer: 1, receiver: 2, yards_known: true, yards: 99, touchdown: false, turnover: false } },
    { seq: 4, offense_side: 0, summary: { kind: "pass", passer: 1, receiver: -1, yards_known: true, yards: 0, turnover: false } },
  ];
  const c = combos([g("a", [], { plays, names })]);
  assert.equal(c.length, 1);
  assert.deepEqual([c[0].passer.name, c[0].receiver.name, c[0].rec, c[0].yds, c[0].td], ["Penix", "London", 2, 30, 1]);
});

test("pitches and perGame average over verified games", () => {
  assert.deepEqual(pitches(games), { pitches: 4, pitch_plays: 2, pitch_yds: 30 });
  const p = perGame(games);
  assert.equal(p.games, 2);
  assert.equal(p.first_downs, 8);
  assert.equal(p.conv1_pct, 0.5);
  assert.equal(p.pts_per_drive, 4.2);
  assert.equal(p.longest_drive, 7);
  assert.equal(p.safeties, 2);
  assert.equal(perGame([]).games, 0);
  assert.equal(perGame([]).conv1_pct, null);
});

test("careerRecords link each best to its game and leave out zeros", () => {
  const r = careerRecords(games);
  const passYds = r.find((x) => x.key === "pass_yds");
  assert.deepEqual([passYds.value, passYds.name, passYds.game_id], [240, "Penix", "a"]);
  assert.equal(r.find((x) => x.key === "longest_td").value, 44);
  assert.equal(r.find((x) => x.key === "rec_td"), undefined, "no receiving TD anywhere: no record");
  assert.equal(r.find((x) => x.key === "biggest_win").value, 7);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test test/profile_players.test.js`
Expected: FAIL, module not found.

- [ ] **Step 3: Write the implementation**

```js
// src/profile/players.js
// Player-level sections from GameRecords: career tables, role leaders, QB->receiver combos, pitches, per-game team
// numbers and single-game records. Only verified records carry lines/plays/team, so the others add nothing. Pure.
import { sumStats } from "../players.js";

const round1 = (x) => Math.round(Number((x * 10).toPrecision(12))) / 10;
const pct = (made, att) => (att ? Math.round((made / att) * 100) / 100 : null);
const ident = (l) => ({ key: l.key, name: l.name, team: l.team, tgid: l.tgid, logo_url: l.logo_url, portrait_url: l.portrait_url });

const PROJ = {
  passing: { keep: (s) => s.pass_att > 0, sort: "yds",
    cols: (s) => ({ cmp: s.pass_cmp, att: s.pass_att, yds: s.pass_yds, td: s.pass_td, int: s.pass_int, air: s.pass_air, yac: s.pass_yac,
      ypa: s.pass_ypa, rtg: s.pass_rtg, long: s.pass_long, sacked: s.sacked }) },
  rushing: { keep: (s) => s.rush_att > 0, sort: "yds",
    cols: (s) => ({ att: s.rush_att, yds: s.rush_yds, avg: s.rush_ypc, td: s.rush_td, long: s.rush_long, btk: s.broken_tackles, yac: s.yds_after_contact }) },
  receiving: { keep: (s) => s.rec > 0 || s.drops > 0, sort: "yds",
    cols: (s) => ({ rec: s.rec, yds: s.rec_yds, avg: s.rec_avg, yac: s.rec_yac, td: s.rec_td, long: s.rec_long, drops: s.drops }) },
  defense: { keep: (s, td) => s.tackles + s.tfl + s.sacks + s.def_int + s.pass_breakups + s.ff + s.fr + td > 0, sort: "tkl",
    cols: (s, td) => ({ tkl: s.tackles, tfl: s.tfl, sck: s.sacks, int: s.def_int, pbu: s.pass_breakups, ff: s.ff, fr: s.fr, td }) },
};

export function careerTables(records) {
  const by = new Map();
  const all = [];
  let allTd = 0;
  for (const r of records) {
    if (!r.verified) continue;
    for (const l of r.lines) {
      if (!by.has(l.key)) by.set(l.key, { id: ident(l), list: [], td: 0 });
      const p = by.get(l.key);
      p.list.push(l.stats); p.td += l.def_td ?? 0;
      all.push(l.stats); allTd += l.def_td ?? 0;
    }
  }
  const players = [...by.values()].map((p) => ({ ...p, sum: sumStats(p.list) }));
  const totalSum = all.length ? sumStats(all) : null;
  const out = {};
  for (const [name, P] of Object.entries(PROJ)) {
    const rows = players.filter((p) => P.keep(p.sum, p.td)).map((p) => ({ ...p.id, games: p.list.length, ...P.cols(p.sum, p.td) }))
      .sort((a, b) => (b[P.sort] ?? 0) - (a[P.sort] ?? 0) || a.name.localeCompare(b.name));
    out[name] = { rows, total: rows.length && totalSum ? { key: "total", name: "Total", ...P.cols(totalSum, allTd) } : null };
  }
  return out;
}

const top = (rows, score, unit) => {
  let best = null, bestV = 0;
  for (const r of rows) { const v = score(r); if (v > bestV) { best = r; bestV = v; } }
  if (!best) return null;
  const { key, name, team, tgid, logo_url, portrait_url } = best;
  return { key, name, team, tgid, logo_url, portrait_url, value: bestV, unit };
};

export function bestPlayers(t) {
  const everyone = new Map();
  for (const table of Object.values(t)) for (const r of table.rows) if (!everyone.has(r.key) || everyone.get(r.key).games < r.games) everyone.set(r.key, r);
  return {
    passer: top(t.passing.rows, (r) => r.yds, "pass_yds"),
    rusher: top(t.rushing.rows, (r) => r.yds, "rush_yds"),
    receiver: top(t.receiving.rows, (r) => r.yds, "rec_yds"),
    dl_lb: top(t.defense.rows, (r) => r.tkl + r.sck, "tkl_sck"),
    ballhawk: top(t.defense.rows, (r) => r.int, "int"),
    most_played: top([...everyone.values()], (r) => r.games, "games"),
  };
}

export function combos(records) {
  const by = new Map();
  for (const r of records) {
    if (!r.verified) continue;
    for (const p of r.plays) {
      const s = p?.summary;
      if (!s || p.offense_side !== r.my_side || s.kind !== "pass" || s.turnover === true) continue;
      // A completion only: when the play carries events, one of them must be the catch (code 6, boxscore.js).
      if (Array.isArray(p.events) && !p.events.some((e) => e?.code === 6)) continue;
      const a = r.names.get(s.passer), b = r.names.get(s.receiver);
      if (!a || !b) continue;
      const k = `${a.key}>${b.key}`;
      if (!by.has(k)) by.set(k, { passer: ident(a), receiver: ident(b), rec: 0, yds: 0, td: 0 });
      const c = by.get(k);
      c.rec += 1;
      if (s.yards_known === true && Number.isFinite(s.yards)) c.yds += s.yards;
      if (s.touchdown === true) c.td += 1;
    }
  }
  return [...by.values()].sort((x, y) => y.yds - x.yds || y.rec - x.rec);
}

const teams = (records) => records.filter((r) => r.verified && r.team);

export function pitches(records) {
  const out = { pitches: 0, pitch_plays: 0, pitch_yds: 0 };
  for (const r of teams(records)) { out.pitches += r.team.pitches; out.pitch_plays += r.team.pitch_plays; out.pitch_yds += r.team.pitch_yds; }
  return out;
}

export function perGame(records) {
  const rs = teams(records);
  const n = rs.length;
  const sum = (f) => rs.reduce((a, r) => a + f(r), 0);
  const per = (f) => (n ? round1(sum(f) / n) : null);
  const lineSum = (r, k) => r.lines.reduce((a, l) => a + (l.stats?.[k] ?? 0), 0);
  const drivesN = sum((r) => r.team.drives);
  return {
    games: n,
    yds: per((r) => lineSum(r, "pass_yds") + lineSum(r, "rush_yds")),
    pass: per((r) => lineSum(r, "pass_yds")),
    rush: per((r) => lineSum(r, "rush_yds")),
    first_downs: per((r) => r.team.first_downs),
    turnovers: per((r) => r.team.turnovers),
    conv1_pct: pct(sum((r) => r.team.conv1[0]), sum((r) => r.team.conv1[1])),
    conv2_pct: pct(sum((r) => r.team.conv2[0]), sum((r) => r.team.conv2[1])),
    pts_per_drive: drivesN ? round1(sum((r) => r.team.points) / drivesN) : null,
    three_outs: per((r) => r.team.three_outs),
    longest_drive: n ? Math.max(...rs.map((r) => r.team.longest_drive)) : null,
    lead_changes: per((r) => r.team.lead_changes),
    biggest_lead: n ? Math.max(...rs.map((r) => r.team.biggest_lead)) : null,
    safeties: sum((r) => r.team.safeties),
  };
}

const LINE_RECORDS = [
  ["pass_yds", "Passing yds"], ["pass_td", "Passing TD"], ["rush_yds", "Rushing yds"], ["rush_td", "Rushing TD"],
  ["rec", "Receptions"], ["rec_yds", "Receiving yds"], ["rec_td", "Receiving TD"],
  ["tackles", "Tackles"], ["sacks", "Sacks"], ["def_int", "Interceptions"], ["rush_long", "Longest run"],
];

export function careerRecords(records) {
  const out = [];
  const keep = (key, label, value, r, name = null) => {
    if (!Number.isFinite(value) || value <= 0) return;
    const cur = out.find((x) => x.key === key);
    if (!cur) out.push({ key, label, value, game_id: r.id, at: r.at, name });
    else if (value > cur.value) Object.assign(cur, { value, game_id: r.id, at: r.at, name });
  };
  for (const r of records) {
    if (r.verified) {
      for (const [k, label] of LINE_RECORDS) for (const l of r.lines) keep(k, label, l.stats?.[k], r, l.name);
      if (r.team) keep("longest_td", "Longest TD", r.team.longest_td, r);
    }
    if (r.score && r.result) keep("most_points", "Most points", r.score[0], r);
    if (r.result === "win" && r.score) keep("biggest_win", "Biggest win", r.score[0] - r.score[1], r);
    if (r.result === "win" && r.team) keep("biggest_comeback", "Biggest comeback", r.team.biggest_deficit, r);
  }
  const order = [...LINE_RECORDS.map(([k]) => k), "longest_td", "most_points", "biggest_win", "biggest_comeback"];
  return out.sort((a, b) => order.indexOf(a.key) - order.indexOf(b.key));
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test test/profile_players.test.js`
Expected: PASS, 6 tests. In the pitches test, each of the two games carries TG with `pitches: 2`, so the totals are 4, 2 and 30.

- [ ] **Step 5: Commit**

```bash
git add src/profile/players.js test/profile_players.test.js
git commit -m "profile: career tables, role leaders, combos, per-game numbers and records"
```

### Task 6: Load game records, Elo and the header from the database

**Files:**
- Create: `src/profile/load.js`
- Create: `migrations/009_plays_as.sql`
- Modify: `src/pbp/avatar.js` (add an optional `size` argument, default 64, so existing callers are unchanged)
- Test: `test/profile_load.test.js`

**Interfaces:**
- Consumes: Task 1 (`gameTypeOf`, `queueOf`, `mySideOf`), Task 2 (`teamGame`), `scoreOf`/`resultOf` (`src/players.js`), `teamsView`/`captureTeamNames` (`src/live.js`), `teamsOf`/`rosterOf` (`src/pbp/livegame.js`), `sideTeams` (`src/web/live-render.js`), `avatarUrl` (`src/pbp/avatar.js`), `liveAssets`.
- Produces:
  - `loadRecords(pool, playerId, { onlineOnly: boolean, acceptedOnly: boolean, assets }) -> GameRecord[]` (newest first)
  - `eloOf(pool, playerId) -> { rating|null, rank|null, games, last_delta|null, history: {at, rating, delta, match_id}[] }`
  - `headerOf(pool, playerId, { assets, auto }) -> { player_id, display_name, avatar_url, joined, plays_as, plays_as_auto, online|null } | null`
    - `auto` is `mostPlayedTeam`'s answer.
    - The result is null for a missing or deleted player.
  - `profileKey(pool, playerId, onlineOnly) -> string`: the cache key (Task 7).

- [ ] **Step 1: Write the migration**

First read `migrations/008_live_games_cap.sql`, and copy its header comment style and any `SET lock_timeout` line it uses.

```sql
-- 009: the team a player's public profile shows under "Plays as". NULL = the most-played team, worked out per request.
-- Rollback: ALTER TABLE player DROP COLUMN IF EXISTS plays_as_tgid;
ALTER TABLE player ADD COLUMN plays_as_tgid smallint;
```

- [ ] **Step 2: Write the failing test**

The test builds rows directly, the same way `test/players.test.js` does with `insertGame` and `participate`. Copy those two helpers into this test file, and extend `insertGame` to take `mode` and `players`.

```js
// test/profile_load.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { testPool } from "./helpers/db.js";
import { loadRecords, eloOf, headerOf } from "../src/profile/load.js";

const assets = {
  team: (id) => ({ 14: { tgid: 14, name: "Falcons", abbrev: "ATL", logo_url: "/live-assets/r/v/teams/14-256.png" }, 13: { tgid: 13, name: "Eagles", abbrev: "PHI", logo_url: "/live-assets/r/v/teams/13-256.png" } })[id] ?? null,
  teamByName: (n) => ({ falcons: assets.team(14), eagles: assets.team(13) })[String(n).toLowerCase()] ?? null,
  portraitUrl: (pid) => (pid === 777 ? "/live-assets/r/v/portraits/777-96.png" : null),
};
const hash = () => createHash("sha256").update(randomUUID()).digest("hex");
const insertGame = (pool, { mode = "online", teams, players = [], status = "accepted", plays_status = "verified", seen }) => pool.query(
  `INSERT INTO game (capture_hash, mode, teams, style, players, target_reached, status, plays_status, first_seen_at)
   VALUES ($1, $2, $3::jsonb, '[]'::jsonb, $4::jsonb, true, $5, $6, $7) RETURNING id`,
  [hash(), mode, JSON.stringify(teams), JSON.stringify(players), status, plays_status, seen]).then((r) => r.rows[0].id);
const participate = (pool, gameId, playerId, teamIndex, kind = "human") => pool.query(
  `INSERT INTO participation (game_id, player_id, kind, team_index, source) VALUES ($1, $2, $3, $4, 'admin')`, [gameId, playerId, kind, teamIndex]);

test("loadRecords: side mapping, lines for my side only, opponent, Elo on the match's last game, online filter", async () => {
  const t = await testPool();
  try {
    const me = (await t.pool.query(`INSERT INTO player (discord_user_id, display_name, show_offline) VALUES ('1', 'Adam', true) RETURNING id`)).rows[0].id;
    const them = (await t.pool.query(`INSERT INTO player (discord_user_id, display_name, avatar_hash) VALUES ('2', 'Playz', 'abc') RETURNING id`)).rows[0].id;
    // Capture order [Eagles, Falcons]; I am capture index 1 (Falcons) and play SIDE 0.
    const players = [{ name: "Penix", team: "Falcons", side: 0, team_index: 1, digp: 5, stats: { pick_six: 0, fum_td: 0 } },
      { name: "Terrell", team: "Falcons", side: 0, team_index: 1, digp: 6, stats: { pick_six: 1, fum_td: 0 } },
      { name: "Hurts", team: "Eagles", side: 1, team_index: 0, digp: 9, stats: {} }];
    const g1 = await insertGame(t.pool, { teams: [{ name: "Eagles", score: 14 }, { name: "Falcons", score: 21 }], players, seen: "2026-09-29T20:00:00Z" });
    await participate(t.pool, g1, me, 1); await participate(t.pool, g1, them, 0);
    await t.pool.query(`INSERT INTO play (game_id, seq, body) VALUES ($1, 1, $2)`, [g1, JSON.stringify({ seq: 1, offense_side: 0,
      teams: [{ side: 0, tgid: 14 }, { side: 1, tgid: 13 }], roster: [{ digp: 5, name: "Penix", side: 0, pid: 777 }],
      summary: { kind: "pass", passer: 5, receiver: 6, yards_known: true, yards: 21, touchdown: true, points: 6, turnover: false } })]);
    await t.pool.query(`INSERT INTO player_game_stat (game_id, digp, name, team_index, stats) VALUES ($1, 5, 'Penix', 1, $2), ($1, 6, 'Terrell', 1, $3), ($1, 9, 'Hurts', 0, $4)`,
      [g1, JSON.stringify({ pass_yds: 21 }), JSON.stringify({ tackles: 2 }), JSON.stringify({ pass_yds: 300 })]);
    const m = (await t.pool.query(`INSERT INTO match (mode, status, seat0_player, seat1_player, source) VALUES ('ranked', 'complete', $1, $2, 'queue') RETURNING id`, [me, them])).rows[0].id;
    await t.pool.query(`INSERT INTO match_game (match_id, game_number, game_id, winner_seat) VALUES ($1, 1, $2, 0)`, [m, g1]);
    await t.pool.query(`INSERT INTO rating_event (match_id, player_id, seat, rating_before, rating_after, delta, k_factor, expected, cause)
      VALUES ($1, $2, 0, 1000, 1016, 16, 32, 0.5, 'accepted')`, [m, me]);
    const off = await insertGame(t.pool, { mode: "offline", teams: [{ name: "Falcons", score: 7 }, { name: "Eagles", score: 0 }], plays_status: "absent", seen: "2026-09-29T21:00:00Z" });
    await participate(t.pool, off, me, 0); await participate(t.pool, off, null, 1, "cpu");

    const all = await loadRecords(t.pool, me, { onlineOnly: false, acceptedOnly: false, assets });
    assert.deepEqual(all.map((r) => r.id), [off, g1], "newest first");
    const online = await loadRecords(t.pool, me, { onlineOnly: true, acceptedOnly: true, assets });
    assert.deepEqual(online.map((r) => r.id), [g1], "show_offline never lets an offline game into the online-only load");
    const r = online[0];
    assert.equal(r.my_side, 0);
    assert.equal(r.me.name, "Falcons"); assert.equal(r.opp.name, "Eagles");
    assert.deepEqual(r.score, [21, 14]); assert.equal(r.result, "win");
    assert.equal(r.queue, "ranked"); assert.equal(r.game_type, "quick");
    assert.equal(r.elo_delta, 16);
    assert.deepEqual(r.match, { id: m, status: "complete", winner_seat: null, my_seat: 0, best_of: 1, game_number: 1 });
    assert.equal(r.opponent.display_name, "Playz"); assert.equal(r.opponent_kind, "human");
    assert.deepEqual(r.lines.map((l) => l.name).sort(), ["Penix", "Terrell"], "only my side's lines");
    assert.equal(r.lines.find((l) => l.name === "Penix").key, "p777");
    assert.equal(r.lines.find((l) => l.name === "Penix").portrait_url, "/live-assets/r/v/portraits/777-96.png");
    assert.equal(r.lines.find((l) => l.name === "Terrell").key, "nTerrell|Falcons", "no roster pid: the name key (R2)");
    assert.equal(r.lines.find((l) => l.name === "Terrell").def_td, 1);
    assert.equal(r.team.points, 6);
    assert.equal(r.names.get(6).name, "Terrell");
    const cpu = all[0];
    assert.equal(cpu.opponent, null); assert.equal(cpu.opponent_kind, "cpu"); assert.equal(cpu.queue, null);
    assert.deepEqual(cpu.lines, []); assert.equal(cpu.team, null); assert.equal(cpu.match, null);
  } finally { await t.close(); }
});

test("eloOf: unranked, then rating, rank by the ladder's order, and history", async () => {
  const t = await testPool();
  try {
    const a = (await t.pool.query(`INSERT INTO player (display_name) VALUES ('A') RETURNING id`)).rows[0].id;
    const b = (await t.pool.query(`INSERT INTO player (display_name) VALUES ('B') RETURNING id`)).rows[0].id;
    assert.deepEqual(await eloOf(t.pool, a), { rating: null, rank: null, games: 0, last_delta: null, history: [] });
    await t.pool.query(`INSERT INTO rating (player_id, mode, rating, games) VALUES ($1, 'ranked', 1100, 3), ($2, 'ranked', 1200, 4)`, [a, b]);
    const m = (await t.pool.query(`INSERT INTO match (mode, status, seat0_player, seat1_player) VALUES ('ranked', 'complete', $1, $2) RETURNING id`, [a, b])).rows[0].id;
    await t.pool.query(`INSERT INTO rating_event (match_id, player_id, seat, rating_before, rating_after, delta, k_factor, expected, cause, applied_at)
      VALUES ($1, $2, 0, 1116, 1100, -16, 32, 0.5, 'accepted', '2026-09-29T20:00:00Z')`, [m, a]);
    const e = await eloOf(t.pool, a);
    assert.deepEqual([e.rating, e.rank, e.games, e.last_delta], [1100, 2, 3, -16]);
    assert.deepEqual(e.history.map((h) => h.rating), [1100]);
  } finally { await t.close(); }
});

test("headerOf: plays_as override wins over auto; deleted is null", async () => {
  const t = await testPool();
  try {
    const a = (await t.pool.query(`INSERT INTO player (discord_user_id, display_name, avatar_hash) VALUES ('9', 'A', '0123456789abcdef0123456789abcdef') RETURNING id`)).rows[0].id;
    const auto = { name: "Falcons", abbrev: "ATL", tgid: 14, logo_url: "/x.png" };
    const h1 = await headerOf(t.pool, a, { assets, auto });
    assert.equal(h1.plays_as.tgid, 14); assert.equal(h1.plays_as_auto, true);
    assert.match(h1.avatar_url, /size=256$/);
    await t.pool.query(`UPDATE player SET plays_as_tgid = 13 WHERE id = $1`, [a]);
    const h2 = await headerOf(t.pool, a, { assets, auto });
    assert.equal(h2.plays_as.name, "Eagles"); assert.equal(h2.plays_as_auto, false);
    await t.pool.query(`UPDATE player SET deleted_at = now() WHERE id = $1`, [a]);
    assert.equal(await headerOf(t.pool, a, { assets, auto }), null);
  } finally { await t.close(); }
});
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `node --test test/profile_load.test.js`
Expected: FAIL, module not found.

- [ ] **Step 4: Write the implementation**

Change `avatarUrl` in `src/pbp/avatar.js` to `avatarUrl(discordUserId, hash, size = 64)`, and use `size` in the query string. Read the file first, and keep its existing validation.

```js
// src/profile/load.js
// Everything the profile and the dashboard read from the database, turned into GameRecords (aggregate.js documents the
// shape). One query for the games, one for every participant, one each for plays and box-score lines of the verified
// games, one for Elo deltas. `onlineOnly` is the public profile's privacy rule (spec §2): SQL, not a filter after.
import { scoreOf, resultOf } from "../players.js";
import { teamsView, captureTeamNames } from "../live.js";
import { teamsOf, rosterOf } from "../pbp/livegame.js";
import { sideTeams } from "../web/live-render.js";
import { avatarUrl } from "../pbp/avatar.js";
import { liveAssets } from "../liveAssets.js";
import { ONLINE_WITHIN_MS } from "../mm/presence.js";
import { gameTypeOf, queueOf, mySideOf } from "./classify.js";
import { teamGame } from "./teamGame.js";

const NO_TEAM = { name: null, abbrev: null, tgid: null, logo_url: null };
const small = (t) => (t && t.name ? { name: t.name, abbrev: t.abbrev ?? null, tgid: t.tgid ?? null, logo_url: t.logo_url ?? null } : NO_TEAM);

function sideViews(g, plays, assets) {
  const names = captureTeamNames(g.players);
  const { team } = sideTeams(g);
  for (const side of [0, 1]) if (names[side] === null && typeof g.teams?.[team[side]]?.name === "string") names[side] = g.teams[team[side]].name;
  const view = teamsView(plays.length ? teamsOf(plays) : null, assets, names);
  // A custom (non-catalog) team keeps its capture name without a logo.
  return [0, 1].map((s) => (view[s].name ? small(view[s]) : names[s] ? { ...NO_TEAM, name: names[s] } : NO_TEAM));
}

export async function loadRecords(pool, playerId, { onlineOnly, acceptedOnly, assets = liveAssets }) {
  const games = (await pool.query(
    `SELECT g.id, g.mode, g.teams, g.players, g.status, g.plays_status, g.first_seen_at, pa.team_index,
            m.id AS match_id, m.mode AS match_mode, m.source AS match_source, m.status AS match_status, m.winner_seat AS match_winner,
            m.best_of AS match_best_of, (m.seat0_player = $1) AS match_seat0_me, mg.game_number,
            (mg.game_number = (SELECT max(x.game_number) FROM match_game x WHERE x.match_id = m.id AND x.game_id IS NOT NULL)) AS match_last
       FROM participation pa JOIN game g ON g.id = pa.game_id
       LEFT JOIN match_game mg ON mg.game_id = g.id LEFT JOIN match m ON m.id = mg.match_id
      WHERE pa.player_id = $1 AND pa.kind = 'human' AND g.status <> 'void'
        AND ($2::boolean = false OR g.mode = 'online') AND ($3::boolean = false OR g.status = 'accepted')
      ORDER BY g.first_seen_at DESC`, [playerId, onlineOnly, acceptedOnly])).rows;
  if (!games.length) return [];
  const ids = games.map((g) => g.id);
  const verified = games.filter((g) => g.plays_status === "verified").map((g) => g.id);
  const parts = (await pool.query(
    `SELECT pa.game_id, pa.kind, pa.team_index, pa.player_id, p.display_name, p.avatar_hash, p.discord_user_id
       FROM participation pa LEFT JOIN player p ON p.id = pa.player_id WHERE pa.game_id = ANY($1::uuid[])`, [ids])).rows;
  const plays = verified.length ? (await pool.query(`SELECT game_id, body FROM play WHERE game_id = ANY($1::uuid[]) ORDER BY game_id, seq`, [verified])).rows : [];
  const stats = verified.length ? (await pool.query(`SELECT game_id, digp, name, team_index, stats FROM player_game_stat WHERE game_id = ANY($1::uuid[])`, [verified])).rows : [];
  const deltas = new Map((await pool.query(
    `SELECT match_id, sum(delta)::int AS d FROM rating_event WHERE player_id = $1 AND cause = 'accepted' GROUP BY match_id`, [playerId])).rows
    .map((r) => [r.match_id, r.d]));
  const group = (rows) => { const m = new Map(); for (const r of rows) { if (!m.has(r.game_id)) m.set(r.game_id, []); m.get(r.game_id).push(r); } return m; };
  const partsBy = group(parts), playsBy = group(plays), statsBy = group(stats);

  return games.map((g) => {
    const ti = g.team_index === 0 || g.team_index === 1 ? g.team_index : null;
    const isVerified = g.plays_status === "verified";
    const body = isVerified ? (playsBy.get(g.id) ?? []).map((r) => r.body) : [];
    const side = mySideOf(g, ti);
    const views = sideViews(g, body, assets);
    const s = scoreOf(g.teams);
    const opp = (partsBy.get(g.id) ?? []).find((p) => ti !== null && p.team_index === 1 - ti) ?? null;
    const pid = new Map(rosterOf(body).map((r) => [r.digp, r.pid]));
    const capBy = new Map((Array.isArray(g.players) ? g.players : []).filter((p) => Number.isInteger(p?.digp)).map((p) => [p.digp, p]));
    const identity = (row) => {
      const cap = capBy.get(row.digp);
      const teamName = cap?.team ?? g.teams?.[row.team_index]?.name ?? null;
      const t = assets.teamByName(teamName);
      const p = pid.get(row.digp) ?? null;
      return { key: p !== null ? `p${p}` : `n${row.name}|${teamName ?? ""}`, digp: row.digp, name: row.name, team: t?.name ?? teamName,
        tgid: t?.tgid ?? null, logo_url: t?.logo_url ?? null, portrait_url: assets.portraitUrl(p, row.name),
        def_td: (Number(cap?.stats?.pick_six) || 0) + (Number(cap?.stats?.fum_td) || 0) };
    };
    const rows = statsBy.get(g.id) ?? [];
    const names = new Map(rows.map((row) => [row.digp, identity(row)]));
    return {
      id: g.id, at: new Date(g.first_seen_at).toISOString(), mode: g.mode,
      queue: queueOf(g.mode, g.match_id ? { mode: g.match_mode, source: g.match_source } : null),
      game_type: gameTypeOf(body, g.teams, assets), status: g.status, plays_status: g.plays_status, verified: isVerified,
      my_side: side, me: side === null ? NO_TEAM : views[side], opp: side === null ? NO_TEAM : views[1 - side],
      opponent: opp && opp.kind === "human" && opp.player_id ? { player_id: opp.player_id, display_name: opp.display_name, avatar_url: avatarUrl(opp.discord_user_id, opp.avatar_hash) } : null,
      opponent_kind: opp?.kind ?? null,
      score: s && ti !== null && s[ti] !== null && s[1 - ti] !== null ? [s[ti], s[1 - ti]] : null,
      result: resultOf({ ...g, team_index: ti }),
      elo_delta: g.match_id && g.match_last && deltas.has(g.match_id) ? deltas.get(g.match_id) : null,
      lines: isVerified && ti !== null ? rows.filter((row) => row.team_index === ti).map((row) => ({ ...names.get(row.digp), stats: row.stats })) : [],
      team: isVerified ? teamGame(body, side) : null,
      plays: body, names,
      match: g.match_id ? { id: g.match_id, status: g.match_status, winner_seat: g.match_winner,
        my_seat: g.match_seat0_me === null ? null : g.match_seat0_me ? 0 : 1, best_of: g.match_best_of, game_number: g.game_number } : null,
    };
  });
}

export async function eloOf(pool, playerId) {
  const r = (await pool.query(`SELECT rating, games FROM rating WHERE player_id = $1 AND mode = 'ranked'`, [playerId])).rows[0];
  if (!r || r.games === 0) return { rating: null, rank: null, games: 0, last_delta: null, history: [] };
  const rank = (await pool.query(
    `SELECT 1 + count(*)::int AS rank FROM rating r2 JOIN player p ON p.id = r2.player_id
      WHERE r2.mode = 'ranked' AND p.deleted_at IS NULL AND (r2.rating > $2 OR (r2.rating = $2 AND r2.player_id < $1))`, [playerId, r.rating])).rows[0].rank;
  const history = (await pool.query(
    `SELECT re.applied_at AS at, re.rating_after AS rating, re.delta, re.match_id FROM rating_event re JOIN match m ON m.id = re.match_id
      WHERE re.player_id = $1 AND m.mode = 'ranked' ORDER BY re.applied_at, re.id`, [playerId])).rows
    .map((h) => ({ at: new Date(h.at).toISOString(), rating: h.rating, delta: h.delta, match_id: h.match_id }));
  return { rating: r.rating, rank, games: r.games, last_delta: history.length ? history[history.length - 1].delta : null, history };
}

export async function headerOf(pool, playerId, { assets = liveAssets, auto = null } = {}) {
  const p = (await pool.query(
    `SELECT id, display_name, avatar_hash, discord_user_id, created_at, plays_as_tgid, presence_hidden, last_seen_at, deleted_at FROM player WHERE id = $1`, [playerId])).rows[0];
  if (!p || p.deleted_at) return null;
  const chosen = p.plays_as_tgid !== null ? assets.team(p.plays_as_tgid) : null;
  return {
    player_id: p.id, display_name: p.display_name, avatar_url: avatarUrl(p.discord_user_id, p.avatar_hash, 256),
    joined: new Date(p.created_at).toISOString(),
    plays_as: chosen ? small(chosen) : auto, plays_as_auto: !chosen,
    online: p.presence_hidden ? null : Date.now() - new Date(p.last_seen_at).getTime() < ONLINE_WITHIN_MS,
  };
}

/** Changes whenever anything a profile shows could have changed (Task 7's cache key). */
export async function profileKey(pool, playerId, onlineOnly) {
  const r = (await pool.query(
    `SELECT count(*)::int AS n, max(g.first_seen_at) AS seen, max(g.resolved_at) AS resolved,
            (SELECT max(updated_at) FROM rating WHERE player_id = $1) AS rated,
            (SELECT plays_as_tgid FROM player WHERE id = $1) AS plays_as
       FROM participation pa JOIN game g ON g.id = pa.game_id
      WHERE pa.player_id = $1 AND pa.kind = 'human' AND ($2::boolean = false OR g.mode = 'online')`, [playerId, onlineOnly])).rows[0];
  return [playerId, onlineOnly, r.n, r.seen?.toISOString?.() ?? "", r.resolved?.toISOString?.() ?? "", r.rated?.toISOString?.() ?? "", r.plays_as ?? ""].join("|");
}
```

Before relying on `presence_hidden` and `last_seen_at` for online status, open `src/mm/presence.js` `onlinePlayers`. If it decides "online" from a different column, use that column instead, and change the test to match (ruling R10).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `node --test test/profile_load.test.js`, then `npm test`.
Expected: `profile_load` passes 3 tests. The full suite exits 0, because the `avatarUrl` default keeps every existing caller unchanged.

- [ ] **Step 6: Commit**

```bash
git add migrations/009_plays_as.sql src/profile/load.js src/pbp/avatar.js test/profile_load.test.js
git commit -m "profile: load game records, Elo and the header; plays_as column"
```

### Task 7: Routes (profile, dashboard, plays-as) with a cache

**Files:**
- Create: `src/profile/router.js`
- Modify: `src/app.js`: mount `profileRouter` immediately BEFORE `playersRouter`. `/v1/players/:id/profile` does not clash, but keep all player routes together.
- Test: `test/profile_routes.test.js`

**Interfaces:**
- Consumes: Tasks 3, 4, 5 and 6 (`groupMatches`, `matchRow`); `bearerPlayer` (`src/auth.js`); `UUID_RE` (`src/admin.js`).
- Produces the HTTP API the launcher (Task 10) and the web page (Task 8) read.
  - `GET /v1/players/:id/profile`, no auth, returns:
    `{ ok, header, elo, summary, recent_matches, best_players, per_game, career, combos, pitches, by_team, by_queue, by_game_type, wins_vs_losses, head_to_head, records, matches_total }`
    - `recent_matches` is the 10 newest `matchRow`s. Each holds its games as `gameRow`s.
  - `GET /v1/players/:id/profile?section=matches&limit=50&before=<ISO>` returns `{ ok, matches: matchRow[] }`. `before` compares against a match's `at`. `limit` must be a whole number from 1 to 200, else 400. `before` must be a date, else 400.
  - `GET /v1/me/dashboard` needs a bearer token, else 401 `{ ok:false, error:"Sign in first" }`. It returns:
    `{ ok, you: { player_id, display_name, elo, summary, plays_as }, matches: matchRow[] (200 newest), recent_form, head_to_head, best_players }`
  - `POST /v1/me/plays-as` takes `{ tgid: integer|null }` and needs a bearer. The answer is `{ ok, plays_as }`. An unknown tgid is 400 `{ ok:false, error:"No such team" }`.

- [ ] **Step 1: Write the failing test**

Build the app the way `test/players.test.js` does: `express()`, `app.use(profileRouter({ pool, clock, assets }))`, and `listen(0)`. Make the token with `newToken`/`hashToken` into `device_token`, copying lines 61–100 of `test/players.test.js` for the pattern. Seed with Task 6's `insertGame`/`participate` helpers, copied into this file.

```js
// test/profile_routes.test.js  (helpers copied from profile_load.test.js and players.test.js, not shown again here)
test("public profile is online-only even with show_offline on; dashboard includes offline", async () => {
  // Seed: player Adam with show_offline = true; one online accepted verified win vs Playz, one offline win vs CPU,
  // one online HELD game, and one ranked best of three vs Playz (a `match` row, source 'queue', winner_seat = Adam's
  // seat, three accepted online games in match_game: won, lost, won). Adam has a token.
  const prof = await (await fetch(`${base}/v1/players/${adam}/profile`)).json();
  assert.equal(prof.ok, true);
  assert.equal(prof.summary.games, 2, "two online matches: the single win and the best of three; offline and held left out");
  assert.equal(prof.recent_matches.length, 2);
  const bo3 = prof.recent_matches.find((m) => m.match_id === bo3MatchId);
  assert.deepEqual(bo3.series, [2, 1]);
  assert.equal(bo3.games.length, 3);
  assert.equal(prof.recent_matches.some((m) => m.mode === "offline"), false);
  assert.equal(JSON.stringify(prof).includes(offlineGameId), false, "the offline game id appears nowhere");
  assert.equal(prof.by_team.at(-1).key, "pickup");
  const dash = await (await fetch(`${base}/v1/me/dashboard`, { headers: { authorization: `Bearer ${token}` } })).json();
  assert.equal(dash.matches.length, 4, "single win, offline win, held game, and the best of three as one row");
  assert.equal(dash.you.summary.games, 3, "the held game has no result");
  assert.equal(dash.matches.find((m) => m.games[0].id === offlineGameId).mode, "offline");
});

test("profile paging, 404s and validation", async () => {
  assert.equal((await fetch(`${base}/v1/players/not-a-uuid/profile`)).status, 404);
  assert.equal((await fetch(`${base}/v1/players/${randomUUID()}/profile`)).status, 404);
  assert.equal((await fetch(`${base}/v1/players/${adam}/profile?section=matches&limit=0`)).status, 400);
  assert.equal((await fetch(`${base}/v1/players/${adam}/profile?section=matches&before=nope`)).status, 400);
  const page = await (await fetch(`${base}/v1/players/${adam}/profile?section=matches&limit=1`)).json();
  assert.equal(page.matches.length, 1);
  const next = await (await fetch(`${base}/v1/players/${adam}/profile?section=matches&limit=1&before=${encodeURIComponent(page.matches[0].at)}`)).json();
  assert.equal(next.matches.length, 1);
  const last = await (await fetch(`${base}/v1/players/${adam}/profile?section=matches&limit=1&before=${encodeURIComponent(next.matches[0].at)}`)).json();
  assert.equal(last.matches.length, 0);
});

test("a player with no online games: header, zeros, Unranked, no role cards", async () => {
  const prof = await (await fetch(`${base}/v1/players/${loner}/profile`)).json();
  assert.equal(prof.ok, true);
  assert.equal(prof.header.display_name, "Loner");
  assert.equal(prof.summary.games, 0);
  assert.equal(prof.elo.rating, null);
  assert.deepEqual(Object.values(prof.best_players), [null, null, null, null, null, null]);
  assert.equal(prof.career.passing.total, null);
});

test("dashboard needs a token; plays-as validates and sticks", async () => {
  assert.equal((await fetch(`${base}/v1/me/dashboard`)).status, 401);
  const bad = await fetch(`${base}/v1/me/plays-as`, { method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" }, body: JSON.stringify({ tgid: 9999 }) });
  assert.equal(bad.status, 400);
  const ok = await (await fetch(`${base}/v1/me/plays-as`, { method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" }, body: JSON.stringify({ tgid: 13 }) })).json();
  assert.equal(ok.plays_as.name, "Eagles");
  const prof = await (await fetch(`${base}/v1/players/${adam}/profile`)).json();
  assert.equal(prof.header.plays_as.name, "Eagles");
  assert.equal(prof.header.plays_as_auto, false);
  await fetch(`${base}/v1/me/plays-as`, { method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" }, body: JSON.stringify({ tgid: null }) });
  assert.equal((await (await fetch(`${base}/v1/players/${adam}/profile`)).json()).header.plays_as_auto, true);
});
```

Write the seeding in full inside the test file. Every name in these tests (`adam`, `loner`, `token`, `offlineGameId`, `bo3MatchId`, `base`) must be defined in that file. Give each game a distinct `first_seen_at`. Use the fake `assets` object from Task 6's test, so tgid 13 is the Eagles.

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test test/profile_routes.test.js`
Expected: FAIL, module not found.

- [ ] **Step 3: Write the implementation**

```js
// src/profile/router.js
// The public profile (online games only, spec §2), the signed-in dashboard (every game), and the "Plays as" choice.
// Aggregates are cached per player, keyed by profileKey, so a busy profile is computed once per new game.
import express from "express";
import { bearerPlayer } from "../auth.js";
import { UUID_RE } from "../admin.js";
import { liveAssets } from "../liveAssets.js";
import { loadRecords, eloOf, headerOf, profileKey } from "./load.js";
import { summary, recentForm, headToHead, byTeam, byQueue, byGameType, winsVsLosses, mostPlayedTeam, matchRow } from "./aggregate.js";
import { groupMatches } from "./matches.js";
import { careerTables, bestPlayers, combos, pitches, perGame, careerRecords } from "./players.js";

const CACHE_MAX = 200;

export function profileRouter({ pool, clock, assets = liveAssets }) {
  const r = express.Router();
  const cache = new Map();
  const cached = async (key, build) => {
    if (cache.has(key)) { const v = cache.get(key); cache.delete(key); cache.set(key, v); return v; }
    const v = await build();
    cache.set(key, v);
    if (cache.size > CACHE_MAX) cache.delete(cache.keys().next().value);
    return v;
  };
  const me = async (req) => (await bearerPlayer(pool, clock, req.headers.authorization))?.player ?? null;

  const buildProfile = async (id) => {
    const records = await loadRecords(pool, id, { onlineOnly: true, acceptedOnly: true, assets });
    const matches = groupMatches(records);
    const header = await headerOf(pool, id, { assets, auto: mostPlayedTeam(matches) });
    if (!header) return null;
    const career = careerTables(records);
    return {
      header, elo: await eloOf(pool, id), summary: summary(matches),
      recent_matches: matches.slice(0, 10).map(matchRow), matches_total: matches.length,
      best_players: bestPlayers(career), per_game: perGame(records), career, combos: combos(records), pitches: pitches(records),
      by_team: byTeam(matches), by_queue: byQueue(matches), by_game_type: byGameType(matches),
      wins_vs_losses: winsVsLosses(matches), head_to_head: headToHead(matches), records: careerRecords(records),
    };
  };

  r.get("/v1/players/:id/profile", async (req, res, next) => {
    try {
      const id = String(req.params.id).toLowerCase();
      if (!UUID_RE.test(id)) return res.status(404).json({ ok: false, error: "No such player" });
      if (req.query.section === "matches") {
        const limit = req.query.limit === undefined ? 50 : Number(req.query.limit);
        if (!Number.isInteger(limit) || limit < 1 || limit > 200) return res.status(400).json({ ok: false, error: "limit must be a whole number from 1 to 200" });
        const before = req.query.before === undefined ? null : new Date(String(req.query.before));
        if (before !== null && Number.isNaN(before.getTime())) return res.status(400).json({ ok: false, error: "before must be a date" });
        const all = await cached(`matches|${await profileKey(pool, id, true)}`,
          async () => groupMatches(await loadRecords(pool, id, { onlineOnly: true, acceptedOnly: true, assets })));
        const page = all.filter((m) => before === null || new Date(m.at) < before).slice(0, limit).map(matchRow);
        return res.json({ ok: true, matches: page });
      }
      const body = await cached(`profile|${await profileKey(pool, id, true)}`, () => buildProfile(id));
      if (!body) return res.status(404).json({ ok: false, error: "No such player" });
      res.json({ ok: true, ...body });
    } catch (e) { next(e); }
  });

  r.get("/v1/me/dashboard", async (req, res, next) => {
    try {
      const p = await me(req);
      if (!p) return res.status(401).json({ ok: false, error: "Sign in first" });
      const body = await cached(`dash|${await profileKey(pool, p.id, false)}`, async () => {
        const records = await loadRecords(pool, p.id, { onlineOnly: false, acceptedOnly: false, assets });
        const matches = groupMatches(records);
        const header = await headerOf(pool, p.id, { assets, auto: mostPlayedTeam(matches) });
        return {
          you: { player_id: p.id, display_name: header?.display_name ?? p.display_name, elo: await eloOf(pool, p.id), summary: summary(matches), plays_as: header?.plays_as ?? null },
          matches: matches.slice(0, 200).map(matchRow), recent_form: recentForm(matches), head_to_head: headToHead(matches),
          best_players: bestPlayers(careerTables(records)),
        };
      });
      res.json({ ok: true, ...body });
    } catch (e) { next(e); }
  });

  r.post("/v1/me/plays-as", express.json({ limit: "1kb" }), async (req, res, next) => {
    try {
      const p = await me(req);
      if (!p) return res.status(401).json({ ok: false, error: "Sign in first" });
      const tgid = req.body?.tgid ?? null;
      if (tgid !== null && (!Number.isInteger(tgid) || !assets.team(tgid))) return res.status(400).json({ ok: false, error: "No such team" });
      await pool.query(`UPDATE player SET plays_as_tgid = $2 WHERE id = $1`, [p.id, tgid]);
      const header = await headerOf(pool, p.id, { assets, auto: null });
      res.json({ ok: true, plays_as: header?.plays_as ?? null });
    } catch (e) { next(e); }
  });
  return r;
}
```

`app.js`: `import { profileRouter } from "./profile/router.js";`. Add `app.use(profileRouter({ pool, clock }));` on the line before `app.use(playersRouter(...))`. It sits after the `/v1` rate limiter, so the profile is rate limited like every other `/v1` route.

`bearerPlayer` returns `{ player }`. Check that `player` carries `id` and `display_name` by reading `src/auth.js`. If the name differs, use the real one.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `node --test test/profile_routes.test.js`, then `npm test`, then `echo exit=$?`.
Expected: profile_routes passes 4 tests, and the full suite prints `exit=0`.

- [ ] **Step 5: Commit**

```bash
git add src/profile/router.js src/app.js test/profile_routes.test.js
git commit -m "profile: public online-only profile and signed-in dashboard, listed by match; plays-as; per-player cache"
```

### Task 8: Web profile page at /u/:player, and the online host

**Files:**
- Create: `src/web/profile.html`, `src/web/profile-page.js`, `src/web/profile.css`, `src/web/cover-color.js`
- Modify: `src/webRouter.js`: the `/u/:player` route, the assets, and the host redirect.
- Modify: `src/server.js`: read the optional `PAGE_ORIGIN` and `API_HOSTS`, and pass them to `buildApp` env → `webRouter`.
- Modify: `src/app.js`: `app.use(webRouter({ pool, clock, pageOrigin: env.PAGE_ORIGIN, apiHosts: env.API_HOSTS }))`.
- Tests: `test/web_profile_page.test.js`, `test/web_host.test.js`, `test/cover_color.test.js`

**Interfaces:**
- Consumes: `GET /v1/players/:id/profile` (Task 7), same origin.
- Produces:
  - `GET /u/:player` serves an HTML page with the player id in `<meta name="nsr-player" content="<uuid>">`.
  - `dominantColors(data: Uint8ClampedArray|number[], step=4) -> [hex, hex] | null` (both repos use the same algorithm).
  - With `pageOrigin` set, a GET on an API host for `/u`, `/games` or `/live` answers 301 to `pageOrigin + originalUrl`.

- [ ] **Step 1: Write the failing tests**

```js
// test/cover_color.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { dominantColors } from "../src/web/cover-color.js";
const px = (n, rgba) => Array.from({ length: n }, () => rgba).flat();
test("one colour: that colour and a darker shade of it", () => {
  assert.deepEqual(dominantColors(px(64, [255, 0, 0, 255])), ["#ff0000", "#8c0000"]);
});
test("two distinct colours: the more common first", () => {
  const [a, b] = dominantColors([...px(48, [0, 0, 255, 255]), ...px(16, [255, 255, 0, 255])], 1);
  assert.equal(a, "#0000ff"); assert.equal(b, "#ffff00");
});
test("transparent or near-black only: null", () => {
  assert.equal(dominantColors(px(64, [255, 0, 0, 0])), null);
  assert.equal(dominantColors(px(64, [5, 5, 5, 255])), null);
  assert.equal(dominantColors([]), null);
});
```

```js
// test/web_host.test.js
// Builds only webRouter (no DB needed for the redirect itself) with a stub pool that answers the game/match lookups.
test("with PAGE_ORIGIN set, page routes on the api host 301 to the online host; /v1 and assets never do", async () => {
  // app.use(webRouter({ pool: stubPool, pageOrigin: "https://online.nflstreetreborn.com", apiHosts: "api.nflstreetreborn.com" }))
  // fetch with header Host: api.nflstreetreborn.com and redirect: "manual"
  // /u/<uuid>        -> 301, location https://online.nflstreetreborn.com/u/<uuid>
  // /games/<uuid>?x=1 -> 301, location keeps ?x=1
  // /live            -> 301
  // /live-assets/live.css -> 200 (no redirect)
  // Host: online.nflstreetreborn.com /u/<uuid> -> not a redirect
});
test("without PAGE_ORIGIN nothing redirects", async () => { /* same requests, all non-301 */ });
```

Write both host tests out fully. The stub pool is `{ query: async () => ({ rows: [{ status: "accepted", id: UUID }] }) }`. Node's `fetch` may refuse to override `Host`. If it does, use `http.request` with `headers: { host }`.

```js
// test/web_profile_page.test.js
test("/u/:uuid serves the page with the id in its meta slot, CSP, and 404s for unknown/deleted/void ids", ...);
test("/u/:name resolves an exact single display-name match case-insensitively; two players with that name is a 404", ...);
test("the page script draws player-typed names as text only", () => {
  const src = readFileSync(new URL("../src/web/profile-page.js", import.meta.url), "utf8");
  assert.equal(/innerHTML|outerHTML|insertAdjacentHTML|document\.write/.test(src), false);
});
```

Write the first two tests against a real `testPool()`, the same way `test/web_game_page.test.js` does. Read that file and copy its setup.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test test/cover_color.test.js test/web_host.test.js test/web_profile_page.test.js`
Expected: FAIL, modules and routes missing.

- [ ] **Step 3: Write `cover-color.js`**

```js
// src/web/cover-color.js  (the launcher's ui/cover-color.js is the same algorithm; keep them in step)
// The two colours a profile cover is drawn from, read from the avatar's pixels (RGBA). Colourful pixels outweigh grey
// ones; transparent and near-black pixels are skipped. The second colour is the most common one clearly different from
// the first, else a darker shade of the first. Null when nothing usable is left (the page then keeps its neutral cover).
const hex = (c) => `#${c.map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0")).join("")}`;
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

export function dominantColors(data, step = 4) {
  const bins = new Map();
  for (let i = 0; i + 3 < (data?.length ?? 0); i += 4 * step) {
    if (data[i + 3] < 128) continue;
    const r = data[i], g = data[i + 1], b = data[i + 2];
    const max = Math.max(r, g, b), min = Math.min(r, g, b);
    if (max < 24) continue;
    const key = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
    const w = 1 + (max - min) / 64;
    const bin = bins.get(key) ?? { n: 0, r: 0, g: 0, b: 0 };
    bin.n += w; bin.r += r * w; bin.g += g * w; bin.b += b * w;
    bins.set(key, bin);
  }
  const ranked = [...bins.values()].sort((a, b) => b.n - a.n).map((x) => [x.r / x.n, x.g / x.n, x.b / x.n]);
  if (!ranked.length) return null;
  const first = ranked[0];
  const second = ranked.find((c) => dist(c, first) > 96) ?? first.map((v) => v * 0.55);
  return [hex(first), hex(second)];
}
```

- [ ] **Step 4: Write the route, the redirect and the page**

In `webRouter.js`:
- Add `"profile.css"`, `"profile-page.js"` and `"cover-color.js"` to `ASSETS`.
- `profile-page.js` imports `./cover-color.js`. Version that import with its hash, the way `game-page.js` imports `live-render.js`, including the startup `throw` if the import is lost.
- Build `PROFILE_PAGE` from `profile.html` the same way `GAME_PAGE` is built, with a `__NSR_PLAYER__` slot.
- Add the route:

```js
  r.get("/u/:player", limit, async (req, res, next) => {
    try {
      const raw = String(req.params.player ?? "");
      let id = null;
      if (UUID_RE.test(raw.toLowerCase())) id = raw.toLowerCase();
      else if (raw.length >= 1 && raw.length <= 64) {
        const rows = (await pool.query(`SELECT id FROM player WHERE lower(display_name) = lower($1) AND deleted_at IS NULL LIMIT 2`, [raw])).rows;
        if (rows.length === 1) id = rows[0].id;
      }
      if (!id) return missing(res);
      const p = (await pool.query(`SELECT deleted_at FROM player WHERE id = $1`, [id])).rows[0];
      if (!p || p.deleted_at) return missing(res);
      secure(res);
      res.set("cache-control", "no-cache");
      res.status(200).type("html").send(PROFILE_PAGE.replace(PLAYER_SLOT, id));
    } catch (e) { next(e); }
  });
```

- Change the signature to `webRouter({ pool, clock, assets, pageOrigin = null, apiHosts = "" })`. The FIRST middleware in the router is:

```js
  const hosts = new Set(String(apiHosts).split(",").map((h) => h.trim().toLowerCase()).filter(Boolean));
  const PAGE_PATH = /^\/(u|games|live)(\/|$)/;
  if (pageOrigin) r.use((req, res, next) => {
    if ((req.method === "GET" || req.method === "HEAD") && hosts.has(String(req.hostname).toLowerCase()) && PAGE_PATH.test(req.path))
      return res.redirect(301, pageOrigin.replace(/\/+$/, "") + req.originalUrl);
    next();
  });
```

`PAGE_PATH` does not match `/live-assets/`, because the character after `live` must be `/` or the end. `/v1/*` is not on this router's page paths.

In `server.js`, read `PAGE_ORIGIN` and `API_HOSTS` with `process.env.X ?? null` or `?? ""`. They are optional, and `need()` must not be used for them. Leaving them unset in Railway keeps today's behaviour. They get set only after `online.` answers (Task 15).

**The page itself:**
- `profile.html` mirrors `game.html`: the same head, `live.css` + `teams.css` + `profile.css`, the meta slot, and one `<main id="profile">` holding a loading line.
- `profile-page.js`:
  1. reads the meta
  2. fetches `/v1/players/<id>/profile`
  3. builds the sections in spec §2 order with `document.createElement` and `textContent` only
  4. reads the avatar for the cover
- Section order and content follow the spec §2 list exactly: Elo over time, Recent matches, Best players, Per game, the four Career tables, Best QB → receiver, Laterals & pitches, By team, By mode (By queue and By game type), In wins vs losses, Head to head, Records.
- Labels, number formats and "All ›" behaviour (R8: expand in place from 5 rows) are the same as the launcher's `stats-model.js` (Task 11). Duplicate the few formatters (`fmtInt` with thousands separators, `fmtPct` as `.630`, `fmtElo` with a sign) at the top of the page script. The web page cannot import the launcher.
- A one-game match row links to `/games/<id>`. A longer match row is a `<button aria-expanded>` that shows its game rows, each linking to `/games/<id>`. The series (`2–1`) is its score. "All ›" under Recent matches fetches more with `?section=matches&before=`. A head-to-head name links to `/u/<player_id>`. A record links to its `/games/<id>`.
- **Cover:**

```js
  const img = new Image(); img.crossOrigin = "anonymous"; img.decoding = "async";
  img.onload = () => { try {
    const c = document.createElement("canvas"); c.width = c.height = 32; const x = c.getContext("2d");
    x.drawImage(img, 0, 0, 32, 32); const cols = dominantColors(x.getImageData(0, 0, 32, 32).data);
    if (cols) { cover.style.setProperty("--c1", cols[0]); cover.style.setProperty("--c2", cols[1]); cover.classList.add("has-colour"); }
  } catch { /* tainted canvas: keep the neutral cover */ } };
  if (header.avatar_url) img.src = header.avatar_url;
```

CSSOM `style.setProperty` is allowed under `style-src 'self'`. Style attributes in markup are not, so never write one.

- `profile.css` takes the look from the v17 mockup (`.youm`, `.gs`, `.tb`, `.pgrid` blocks) on the web page's dark background:
  - Put the `:root` tokens first: `--bg #0a0b0d`, `--panel rgba(19,21,25,.72)`, `--line rgba(255,255,255,.08)`, `--fg #f4f5f7`, `--muted #a4acb8`, `--red #e0272e`, `--red-ink #ff7a7f`, `--teal #22c7b4`.
  - The cover is `.cover { background: linear-gradient(135deg, #1b1e24, #0f1114) }` and `.cover.has-colour { background: linear-gradient(135deg, color-mix(in srgb, var(--c1) 55%, #0a0b0d), color-mix(in srgb, var(--c2) 45%, #0a0b0d)) }`.
  - Totals are `.total { color: var(--muted); font-size: 12px; padding: 8px 12px }`, with no background.
  - At under 720px the layout is one column with 16px gutters and no horizontal scroll. Wide tables scroll inside their own panel (`overflow-x: auto`).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `node --test test/cover_color.test.js test/web_host.test.js test/web_profile_page.test.js`, then `npm test; echo exit=$?`.
Expected: all pass, `exit=0`.

- [ ] **Step 6: Look at it**

Run the backend locally against the test DB, the way `README.md` describes, and seed one player with the Task 7 seeding. Then open `http://localhost:<port>/u/<id>` in a browser. Check each item:
- every section renders
- names with `<b>` show literally
- a player with no online games shows the header and "No online games yet."
- the phone width (390px) has no horizontal scroll

Save a screenshot to the SDD workspace.

- [ ] **Step 7: Commit**

```bash
git add src/web/profile.html src/web/profile-page.js src/web/profile.css src/web/cover-color.js src/webRouter.js src/server.js src/app.js test/cover_color.test.js test/web_host.test.js test/web_profile_page.test.js
git commit -m "web: public profile page at /u/:player; page routes move to PAGE_ORIGIN when set"
```

---

## Launcher

### Task 9: Page origin and the open-website allow-list

**Files:**
- Modify: `crates/reborn-core/src/identity.rs`: add `pub const PAGE_ORIGIN: &str = "https://online.nflstreetreborn.com";` next to `DEFAULT_ORIGIN`.
- Modify: `crates/reborn-ui/src/shell_tools.rs` (`allowed` and its doc comment).
- Modify: `crates/reborn-ui/ui/live-common.js`: add `export const PAGE_ORIGIN = 'https://online.nflstreetreborn.com';`.
- Modify: `crates/reborn-ui/ui/game.js` and `crates/reborn-ui/ui/live.js`: `open_website` URLs use `PAGE_ORIGIN`. `LIVE_ORIGIN` stays for `assetUrl`.
- Test: the tests module in `shell_tools.rs`.

**Interfaces:**
- Produces: `allowed(url)` accepts `https://nflstreetreborn.com/...`, and `{DEFAULT_ORIGIN|PAGE_ORIGIN}/{live|games|u}/<canonical uuid>`, and nothing else.

- [ ] **Step 1: Write the failing tests** (add them to `shell_tools.rs` `mod tests`)

```rust
    #[test]
    fn game_and_profile_pages_open_on_both_origins() {
        for origin in [reborn_core::identity::DEFAULT_ORIGIN, reborn_core::identity::PAGE_ORIGIN] {
            for kind in ["live", "games", "u"] {
                assert!(allowed(&format!("{origin}/{kind}/{LIVE_ID}")), "{origin}/{kind}");
                assert!(!allowed(&format!("{origin}/{kind}/{LIVE_ID}?x=1")));
                assert!(!allowed(&format!("{origin}/{kind}/../admin")));
            }
            assert!(!allowed(&format!("{origin}/v1/players/{LIVE_ID}/profile")));
            assert!(!allowed(&format!("{origin}/u/Adam")), "only an id, never a name, from the launcher");
        }
        assert!(!allowed(&format!("https://online.nflstreetreborn.com.evil.example/u/{LIVE_ID}")));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p reborn-ui -j 2 game_and_profile_pages_open_on_both_origins`
Expected: FAIL. `PAGE_ORIGIN` does not exist, and `/games/` is refused.

- [ ] **Step 3: Implement**

```rust
/// Whether `url` is one this launcher will hand to the shell: a page on the site, or exactly
/// `<origin>/<live|games|u>/<uuid>` on the backend origin or the page origin.
fn allowed(url: &str) -> bool {
    if url.starts_with(SITE) {
        return true;
    }
    use reborn_core::identity::{DEFAULT_ORIGIN, PAGE_ORIGIN};
    [DEFAULT_ORIGIN, PAGE_ORIGIN].iter().any(|origin| {
        ["live", "games", "u"].iter().any(|kind| {
            url.strip_prefix(&format!("{origin}/{kind}/"))
                .is_some_and(reborn_core::live_view::is_canonical_uuid)
        })
    })
}
```

Delete `live_prefix()` if nothing else uses it: `grep -n live_prefix crates/reborn-ui/src`. Keep the existing tests. The old `/live/` tests still pass. Update the doc comment above `allowed` to describe the three kinds and two origins.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p reborn-ui -j 2 shell_tools`
Expected: every `shell_tools` test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/identity.rs crates/reborn-ui/src/shell_tools.rs crates/reborn-ui/ui/live-common.js crates/reborn-ui/ui/game.js crates/reborn-ui/ui/live.js
git commit -m "launcher: pages open on online.nflstreetreborn.com; /games and /u allowed (Open on web for a game was refused)"
```

### Task 10: Core client and commands (dashboard, profile, plays-as)

**Files:**
- Modify: `crates/reborn-core/src/live_view.rs`: three functions and their tests.
- Modify: `crates/reborn-ui/src/live_view.rs`: three commands.
- Modify: `crates/reborn-ui/src/main.rs`: register them next to `live_view::stats_game`.

**Interfaces:**
- Produces (core):
  - `pub fn dashboard(http: &dyn Http, origin: &str, token: &str) -> Result<Value, LiveError>`: `GET /v1/me/dashboard`, the whole body.
  - `pub fn player_profile(http: &dyn Http, origin: &str, player_id: &str, games_before: Option<&str>) -> Result<Option<Value>, LiveError>`:
    - With `games_before = None`, it asks `GET /v1/players/{id}/profile`.
    - With `Some(b)`, it asks `...?section=matches&limit=50&before=<percent-encoded b>` (one more page of the match log).
    - A 404 is `Ok(None)`. A bad id is `Err(LiveError::BadId)` before any request.
  - `pub fn set_plays_as(http: &dyn Http, origin: &str, token: &str, tgid: Option<u16>) -> Result<Value, LiveError>`: `POST /v1/me/plays-as` with `{"tgid": n|null}`.
- Produces (commands, returning `LiveCommandError` on failure like `stats_my_games`):
  - `stats_dashboard() -> Value`: `{ signed_in: false }` when signed out, else the body with `signed_in: true`.
  - `player_profile(player_id: String, before: Option<String>) -> Option<Value>`
  - `set_plays_as(tgid: Option<u16>) -> Value`

- [ ] **Step 1: Write the failing core tests**

In `live_view.rs` `mod tests`, use the existing `FakeHttp` pattern: read how `player_games` is tested there, and copy it.

```rust
    #[test]
    fn dashboard_sends_the_token_and_returns_the_body() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/me/dashboard"), 200, r#"{"ok":true,"games":[]}"#);
        let v = dashboard(&http, ORIGIN, "tok").unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(http.last_bearer().as_deref(), Some("tok"));
    }

    #[test]
    fn player_profile_404_is_none_bad_id_is_refused_and_paging_builds_the_query() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/players/{ID}/profile"), 404, "{}");
        assert_eq!(player_profile(&http, ORIGIN, ID, None).unwrap(), None);
        assert_eq!(player_profile(&PanicHttp, ORIGIN, "../x", None), Err(LiveError::BadId));
        let url = format!("{ORIGIN}/v1/players/{ID}/profile?section=matches&limit=50&before=2026-09-30T01%3A02%3A03.000Z");
        let http = FakeHttp::default().reply(&url, 200, r#"{"ok":true,"games":[]}"#);
        assert!(player_profile(&http, ORIGIN, ID, Some("2026-09-30T01:02:03.000Z")).unwrap().is_some());
    }

    #[test]
    fn set_plays_as_posts_the_tgid_or_null() {
        let http = FakeHttp::default().reply(&format!("{ORIGIN}/v1/me/plays-as"), 200, r#"{"ok":true,"plays_as":null}"#);
        set_plays_as(&http, ORIGIN, "tok", None).unwrap();
        assert_eq!(http.last_body().as_deref(), Some(br#"{"tgid":null}"#.as_slice()));
    }
```

The fake's helper names (`last_bearer`, `last_body`, `PanicHttp`) must match what `net::fake` and this test module actually provide. Read them first. If a helper you need is missing, add it to `net::fake` in this task.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p reborn-core -j 2 live_view`
Expected: FAIL, the functions are missing.

- [ ] **Step 3: Implement the core functions**

```rust
/// `GET {origin}/v1/me/dashboard`: the signed-in player's Stats page (every game, offline included).
pub fn dashboard(http: &dyn Http, origin: &str, token: &str) -> Result<Value, LiveError> {
    get_json(http, &format!("{}/v1/me/dashboard", origin.trim_end_matches('/')), Some(token))
}

/// `GET {origin}/v1/players/{id}/profile`: a player's public, online-only profile, or with `games_before` one more
/// page of their match log. No token: the profile is the same for everyone.
pub fn player_profile(http: &dyn Http, origin: &str, player_id: &str, games_before: Option<&str>) -> Result<Option<Value>, LiveError> {
    if !is_canonical_uuid(player_id) {
        return Err(LiveError::BadId);
    }
    let mut url = format!("{}/v1/players/{player_id}/profile", origin.trim_end_matches('/'));
    if let Some(b) = games_before {
        url.push_str("?section=matches&limit=50&before=");
        url.push_str(&percent_encode(b));
    }
    get_json_opt(http, &url, None)
}

/// `POST {origin}/v1/me/plays-as`: the team the player's profile shows, or `None` for "most played".
pub fn set_plays_as(http: &dyn Http, origin: &str, token: &str, tgid: Option<u16>) -> Result<Value, LiveError> {
    let body = serde_json::to_vec(&serde_json::json!({ "tgid": tgid })).map_err(|_| LiveError::BadReply)?;
    let reply = http.post_json(&format!("{}/v1/me/plays-as", origin.trim_end_matches('/')), &body, Some(token))
        .map_err(|_| LiveError::Unreachable)?;
    if !reply.is_success() {
        return Err(status_error(reply.status));
    }
    serde_json::from_str(&reply.body).map_err(|_| LiveError::BadReply)
}
```

- [ ] **Step 4: Implement the commands**

Add these to `crates/reborn-ui/src/live_view.rs`, following `stats_my_games` exactly: `spawn_blocking`, `identity::status`/`token`/`origin`, `crate::http_client()`, and `LiveCommandError::from`.

```rust
/// The Stats page (dashboard). Signed out is `{ signed_in: false }`, a shape rather than an error.
#[tauri::command]
pub async fn stats_dashboard() -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let signed_in = identity::status(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?.is_some();
        let token = if signed_in { identity::token(&p).unwrap_or(None) } else { None };
        let Some(token) = token else { return Ok(serde_json::json!({ "signed_in": false })) };
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let mut v = reborn_core::live_view::dashboard(&crate::http_client(), &origin, &token).map_err(LiveCommandError::from)?;
        v["signed_in"] = serde_json::Value::Bool(true);
        Ok(v)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}
```

`player_profile(player_id, before)` has the same shape without the token, and returns `Option<Value>`. `set_plays_as(tgid)` needs the token. Signed out, it is `Err(LiveCommandError::unavailable("Sign in with Discord on the Play page first."))`. Register all three in `main.rs` next to `live_view::stats_game`.

`main.rs` has a test named `nothing_the_online_page_can_call_turns_a_core_error_into_raw_text`. It lists commands, so read it. If it enumerates commands, add the three new ones to its list.

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p reborn-core -p reborn-ui -j 2`
Expected: all pass. Before this change, check which tests fail on a clean tree. Record any failure that was there before as a baseline in the ledger, and don't count it against this task.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-core/src/live_view.rs crates/reborn-core/src/net.rs crates/reborn-ui/src/live_view.rs crates/reborn-ui/src/main.rs
git commit -m "launcher: dashboard, player profile and plays-as commands"
```

Add `net.rs` only if the fake was changed.

### Task 11: Pure view models for both pages

**Files:**
- Create: `crates/reborn-ui/ui/stats-model.js`
- Test: `crates/reborn-ui/ui/stats-model.test.mjs`

**Interfaces:**
- Consumes: the Task 7 JSON shapes.
- Produces:
  - `fmtInt(n) -> string`: `1284` becomes `"1,284"`, and null or NaN becomes `"—"`.
  - `fmtPct(p) -> string`: `0.63` becomes `".630"`, `1` becomes `"1.000"`, and null becomes `"—"`.
  - `fmtRecord({wins, losses, ties}) -> "14–9–1"` or `"14–9"`. Ties appear only when above 0, and the separator is an en dash.
  - `fmtElo(elo) -> { value: "1,184"|"Unranked", change: "▲ 16"|"▼ 18"|"", rank: "#7 on the ladder"|"" }`
  - `gameRowModel(row) -> { id, date: "Sep 26", time: "9:18 PM", me, opp, opponent: "Playz"|"CPU"|"Guest"|"—", avatar, score: "36–14"|"—", result: "W"|"L"|"T"|"", tag: "+16 Elo"|"−18 Elo"|"Offline"|"", tagKind: "up"|"down"|"offline"|"" }`
  - `matchRowModel(match) -> { ...gameRowModel fields for the match, score: "2–1" for a series else the game score, games: gameRowModel[] (each built with elo_delta null, so Elo shows once, on the match), expandable: games.length > 1, onlyGame: id of the single game or null }`
  - `roleCards(best) -> { role, label, name, team, logo, portrait, initial, stat }[6]`, in the order passer, rusher, receiver, dl_lb, ballhawk, most_played. The labels are "Best passer", "Best rusher", "Best receiver", "Best DL/LB", "Ballhawk", "Most played". A missing role gets `name: "None yet"`, and its stat is `""`.
  - `totalLine(table, kind) -> string`: `""` when `total` is null.
  - `youGrid(summary, playsAs) -> { label, value, logo? }[6]`: Record, Win %, Streak, Pts / gm, Yds / gm, Plays as.
  - `trendPoints(history, w=160, h=50) -> string`: SVG polyline points, `""` for fewer than 2 points.

- [ ] **Step 1: Write the failing test**

```js
// crates/reborn-ui/ui/stats-model.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fmtInt, fmtPct, fmtRecord, fmtElo, gameRowModel, matchRowModel, roleCards, totalLine, youGrid, trendPoints } from './stats-model.js';

test('number formats', () => {
  assert.equal(fmtInt(1284), '1,284'); assert.equal(fmtInt(null), '—'); assert.equal(fmtInt(NaN), '—');
  assert.equal(fmtPct(0.63), '.630'); assert.equal(fmtPct(1), '1.000'); assert.equal(fmtPct(null), '—');
  assert.equal(fmtRecord({ wins: 14, losses: 9, ties: 1 }), '14–9–1'); assert.equal(fmtRecord({ wins: 3, losses: 0, ties: 0 }), '3–0');
});

test('fmtElo: rated and unranked', () => {
  assert.deepEqual(fmtElo({ rating: 1184, last_delta: 16, rank: 7 }), { value: '1,184', change: '▲ 16', rank: '#7 on the ladder' });
  assert.deepEqual(fmtElo({ rating: 990, last_delta: -18, rank: 30 }).change, '▼ 18');
  assert.deepEqual(fmtElo({ rating: null }), { value: 'Unranked', change: '', rank: '' });
  assert.deepEqual(fmtElo(undefined), { value: 'Unranked', change: '', rank: '' });
});

test('gameRowModel: ranked Elo, offline tag, CPU', () => {
  const base = { id: 'g', at: '2026-09-27T01:18:00Z', mode: 'online', score: [36, 14], result: 'win', opponent: { display_name: 'Playz', avatar_url: 'https://cdn.discordapp.com/avatars/1/a.png?size=64' }, opponent_kind: 'human',
    me: { name: 'Falcons', logo_url: '/live-assets/r/v/teams/14-256.png' }, opp: { name: 'Eagles', logo_url: null } };
  const r = gameRowModel({ ...base, elo_delta: 16 });
  assert.equal(r.score, '36–14'); assert.equal(r.result, 'W'); assert.equal(r.tag, '+16 Elo'); assert.equal(r.tagKind, 'up'); assert.equal(r.opponent, 'Playz');
  assert.equal(gameRowModel({ ...base, elo_delta: -18 }).tag, '−18 Elo');
  const off = gameRowModel({ ...base, mode: 'offline', opponent: null, opponent_kind: 'cpu', elo_delta: null });
  assert.equal(off.tag, 'Offline'); assert.equal(off.tagKind, 'offline'); assert.equal(off.opponent, 'CPU');
  assert.equal(gameRowModel({ ...base, score: null, result: null }).score, '—');
});

test('matchRowModel: a series shows 2–1 and expands; one game shows its score and opens it', () => {
  const g1 = { id: 'a', at: '2026-09-27T01:00:00Z', mode: 'online', score: [14, 10], result: 'win', elo_delta: 16, opponent: { display_name: 'Playz' }, opponent_kind: 'human', me: {}, opp: {} };
  const m = matchRowModel({ id: 'm1', at: '2026-09-27T02:00:00Z', mode: 'online', series: [2, 1], score: [42, 52], result: 'win', elo_delta: 16,
    opponent: { display_name: 'Playz' }, opponent_kind: 'human', me: {}, opp: {}, games: [g1, { ...g1, id: 'b' }, { ...g1, id: 'c' }] });
  assert.equal(m.score, '2–1'); assert.equal(m.expandable, true); assert.equal(m.onlyGame, null);
  assert.equal(m.tag, '+16 Elo'); assert.equal(m.result, 'W');
  assert.equal(m.games.length, 3); assert.equal(m.games[0].tag, '', 'Elo shows once, on the match');
  const one = matchRowModel({ id: 'g:a', at: g1.at, mode: 'offline', series: null, score: [36, 14], result: 'win', elo_delta: null,
    opponent: null, opponent_kind: 'cpu', me: {}, opp: {}, games: [{ ...g1, mode: 'offline' }] });
  assert.equal(one.score, '36–14'); assert.equal(one.expandable, false); assert.equal(one.onlyGame, 'a'); assert.equal(one.tag, 'Offline');
});

test('roleCards: six cards in order, None yet when empty', () => {
  const c = roleCards({ passer: { name: 'Penix', team: 'Falcons', logo_url: '/l', portrait_url: null, value: 2104, unit: 'pass_yds' }, rusher: null, receiver: null, dl_lb: null, ballhawk: null, most_played: null });
  assert.deepEqual(c.map((x) => x.label), ['Best passer', 'Best rusher', 'Best receiver', 'Best DL/LB', 'Ballhawk', 'Most played']);
  assert.equal(c[0].stat, '2,104 pass yds'); assert.equal(c[0].initial, 'P');
  assert.equal(c[1].name, 'None yet'); assert.equal(c[1].stat, '');
});

test('totalLine is one plain line, empty without a total', () => {
  assert.equal(totalLine({ total: { cmp: 212, att: 371, yds: 3108, td: 41, int: 18, rtg: 104.2 } }, 'passing'), 'Total  212/371 · 3,108 yds · 41 TD · 18 INT · 104.2 rtg');
  assert.equal(totalLine({ total: null }, 'passing'), '');
});

test('youGrid and trendPoints', () => {
  const g = youGrid({ wins: 14, losses: 9, ties: 1, win_pct: 0.63, streak: 'W2', pts_per_game: 31.4, yds_per_game: 412 }, { abbrev: 'ATL', logo_url: '/l' });
  assert.deepEqual(g.map((x) => x.label), ['Record', 'Win %', 'Streak', 'Pts / gm', 'Yds / gm', 'Plays as']);
  assert.equal(g[5].value, 'ATL');
  assert.equal(youGrid({ wins: 0, losses: 0, ties: 0, win_pct: null, streak: null, pts_per_game: null, yds_per_game: null }, null)[2].value, '—');
  assert.equal(trendPoints([{ rating: 1000 }]), '');
  assert.equal(trendPoints([{ rating: 1000 }, { rating: 1100 }], 100, 50), '0,50 100,0');
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `node --test crates/reborn-ui/ui/stats-model.test.mjs`
Expected: FAIL, the module is missing.

- [ ] **Step 3: Implement**

Write `stats-model.js` with the exports above. The rules the tests pin:
- The minus sign in Elo tags is U+2212. Score and record separators are U+2013.
- Dates use `toLocaleDateString('en-US', { month: 'short', day: 'numeric' })`. Times use `toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })`.
- Units for role stats: `pass_yds` gives "pass yds", `rush_yds` gives "rush yds", `rec_yds` gives "rec yds", `tkl_sck` gives "tkl + sck", `int` gives "INT", and `games` gives "games".
- `totalLine`:
  - passing: `Total  {cmp}/{att} · {yds} yds · {td} TD · {int} INT · {rtg} rtg`
  - rushing: `Total  {att} att · {yds} yds · {avg} avg · {td} TD`
  - receiving: `Total  {rec} rec · {yds} yds · {td} TD`
  - defense: `Total  {tkl} tkl · {sck} sck · {int} INT · {pbu} PBU`
  - Counts go through `fmtInt`. `avg` and `rtg` are printed with one decimal as the server sent them (`104.2`), never rounded to an integer.
- `trendPoints` maps ratings to the given box: min at the bottom, max at the top, x evenly spaced, and y rounded to 1 decimal with trailing `.0` dropped. With equal ratings, y is `h/2`.

- [ ] **Step 4: Run to verify it passes**

Run: `node --test crates/reborn-ui/ui/stats-model.test.mjs`
Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/stats-model.js crates/reborn-ui/ui/stats-model.test.mjs
git commit -m "stats: pure view models for the dashboard and profile"
```

### Task 12: Stats page rewrite (the dashboard)

**Files:**
- Rewrite: `crates/reborn-ui/ui/stats.js`, `crates/reborn-ui/ui/stats.css`
- Do NOT modify `crates/reborn-ui/ui/stats.html`: the redesign owns page markup.
- Keep: `stats-rows.js`, and its `queueLine`/`unsentCount` for the "N games not uploaded yet" line.

**Interfaces:**
- Consumes: `stats_dashboard` (its `matches`), `stats_queue` and `identity_status` (existing) commands; Task 11 models; `assetUrl` and `avatarSrc` from `live-common.js`.
- Produces: the Stats page. A game row opens `game.html?game=<id>`. "Your profile ›" opens `profile.html?player=<you.player_id>`.

- [ ] **Step 1: Read what you are replacing**

Read the current `stats.js` (179 lines) and keep its load, refresh and generation logic. It keeps:
- the 30 s quiet refresh
- the newest-answer-wins `generation` guard
- the signed-out and empty states
- the diagnostic fold
- the queue line

Only the data source (`stats_dashboard` instead of `stats_my_games`) and the drawing change.

- [ ] **Step 2: Draw the layout from the v17 mockup**

Open `.superpowers/brainstorm/181729-1790747853/content/stats-layout-v17.html`, the first frame ("Stats page (yours)"). Reproduce its structure with `el()` from `shell.js`, with text only.
- **Left column:**
  - **Your matches:** the filter buttons All, Online and Offline (`aria-pressed`). Then 42px rows, 8 matches per page with the existing pager. Each row is a `<button>` holding date and time, my logo, "vs", opp logo, the opponent (avatar + name, or "CPU"), the score (the series `2–1` for a best of three), W/L/T, and the tag from `matchRowModel`.
    - A row with `onlyGame` opens `game.html?game=<onlyGame>`.
    - An `expandable` row toggles `aria-expanded`, and shows its game rows indented under it ("Game 1", "Game 2", …, each with its score and W/L). Each game row opens that game.
  - **Recent form:** W/L/T tiles (`aria-label="Win"` and so on), then `Points for 213 · against 150`.
  - **Head to head:** 5 rows (avatar, name, record, a win-share bar with `role="img"` and `aria-label="6 wins, 5 losses"`). "All ›" expands in place.
- **Right column:**
  - **You:** the Elo value, change and rank from `fmtElo`. A 160×50 SVG polyline from `trendPoints(you.elo.history)`, hidden when `""`. Then the `youGrid` 3×2 grid. The section link "Your profile ›" goes to `profile.html?player=<id>`.
  - **Your players:** six `roleCards`. A card shows a 38px round portrait (`assetUrl`), else the initial, a 16px team badge, the name, and the stat.
  - The buttons **Offense · Defense · Records** open `profile.html?player=<id>#career`, `#defense` and `#records`.
  - The line "N games not uploaded yet" from `queueLine`, as today.
- Signed out: the existing `SIGNED_OUT` sentence. No games: `EMPTY`.

- [ ] **Step 3: Write `stats.css`**

Scope everything to `.page-stats`, as today.
- Copy the v17 values: `.youm`, `.ytop`, `.ygrid`, `.pgrid`, `.row`, `.badge`, `.badge.off` (dashed border, muted text), `.res.W` (teal) and `.res.L` (`--red-ink`).
- Font weights follow the Global Constraints. Nothing heavier than 700, and body text at 400.
- Two columns at `grid-template-columns: minmax(0, 1.35fr) minmax(0, 1fr)` above 1100px, and one column below.

- [ ] **Step 4: Check in the real launcher**

Build and run the launcher UI in dev, the way the repo's README or `installer/windows/build.ps1 -LauncherOnly` describes, signed in as Adam. Check each item:
1. A ranked best of three is ONE row with its series and one "+N Elo", and it expands to three games. Offline games show "Offline".
2. Tab reaches every row, filter, card and link, with focus visible.
3. At 1280×800 nothing overflows.
4. Signed out shows the sign-in sentence.

Save screenshots to the SDD workspace.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/stats.js crates/reborn-ui/ui/stats.css
git commit -m "stats: the dashboard (Elo, your matches, your players, recent form, head to head)"
```

### Task 13: Native public profile view

**Files:**
- Create: `crates/reborn-ui/ui/profile.html`, `crates/reborn-ui/ui/profile.js`, `crates/reborn-ui/ui/profile.css`, `crates/reborn-ui/ui/cover-color.js`, `crates/reborn-ui/ui/cover-color.test.mjs`
- Modify: every place that shows an opponent's name, so it links to the profile:
  - `stats.js` game rows and head to head (Task 12)
  - `game.js` header seats
  - `live-game-render.js` `seatCard`
  - Each name becomes an `<a href="profile.html?player=<id>">` when the seat has a `player_id`.

**Interfaces:**
- Consumes: the `player_profile` and `set_plays_as` commands (Task 10); Task 11 models; `dominantColors` (the same algorithm as backend Task 8).
- Produces: `profile.html?player=<uuid>[#section]`.

- [ ] **Step 1: Write the cover-colour test and the module**

Copy `src/web/cover-color.js` from the backend verbatim to `ui/cover-color.js` as an ES module, and copy its three tests into `cover-color.test.mjs`, adjusting the import. Run `node --test crates/reborn-ui/ui/cover-color.test.mjs`. Expected: PASS, 3 tests.

- [ ] **Step 2: Build `profile.html`**

Copy `game.html` (shell, nav, backdrop, skip link, diagnostics fold). Set the title to "Profile", keep the Stats nav item current, and load `profile.css` and `profile.js`.

- [ ] **Step 3: Draw the profile from the v17 mockup**

Use frames 2 to 4 of `stats-layout-v17.html` ("Full profile 1 of 3" to "3 of 3"), in the spec §2 order, with `el()` and text only.
- **Header:**
  - The cover: neutral by default, and avatar colours through `style.setProperty('--c1'|'--c2')` after `dominantColors`, with the same `crossOrigin` and canvas code as Task 8.
  - The avatar, name, "Joined Sep 2026", "Plays as" with the team logo, and the online dot when `online === true`.
  - A **Plays as** `<select>` shown only when `header.player_id` equals the signed-in player's id (from `identity_status`). Its first option is "Most played" (value `""`), followed by every team in `by_team` except Pickup. On change it calls `set_plays_as`.
  - The main strip: Elo (`fmtElo`), Ranked # (`#7` or "—"), Record, Win % and Games.
  - Buttons: **Share profile** copies `PAGE_ORIGIN + '/u/' + id` with `navigator.clipboard.writeText` and shows "Link copied" for 2 s. **View on web** calls `open_website` with that URL.
- **Sections:** each is an `<section id="...">` with an `<h2>` and an "All ›" `<button aria-expanded>` that shows every row instead of 5 (R8). The ids let `#career`, `#defense` and `#records` from the Stats page scroll there.
- **Recent matches:** the rows use `matchRowModel`, and they expand and open exactly as on the Stats page. "All ›" loads more pages with `player_profile(id, before = last match's at)` until an empty page.
- **Tables:** the name cell has a 16px logo, then the name. Under each table goes `totalLine(...)` as one muted line. Column headers are `<th scope="col">`.
- **Empty states:** a player with no games shows the header plus "No online games yet." and no sections. Each empty table or card shows "None yet".
- **Hidden or deleted player** (`player_profile` returns null): use `notFound('stats.html', 'Back to Stats')` from `live-game-render.js`.

- [ ] **Step 4: Write `profile.css`**

Use the same tokens and weights as Task 12. The cover is 140px tall. `.cover.has-colour` uses the `color-mix` gradient from Task 8. Tables scroll inside their panel.

- [ ] **Step 5: Add a test that names stay text**

Add to `cover-color.test.mjs`, or a new `profile-static.test.mjs`:

```js
test('profile.js never builds HTML from strings', () => {
  const src = readFileSync(new URL('./profile.js', import.meta.url), 'utf8');
  assert.equal(/innerHTML|outerHTML|insertAdjacentHTML|document\.write/.test(src), false);
});
```

Run it. Expected: PASS.

- [ ] **Step 6: Check in the real launcher**

With the local backend from Task 8, or after deploy, use Adam's profile and a profile with no online games. Check every section, keyboard reach, the Share copy, the Plays as change sticking after a reload, and the neutral cover when the avatar can't be read. Save screenshots.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/profile.html crates/reborn-ui/ui/profile.js crates/reborn-ui/ui/profile.css crates/reborn-ui/ui/cover-color.js crates/reborn-ui/ui/cover-color.test.mjs crates/reborn-ui/ui/game.js crates/reborn-ui/ui/live-game-render.js crates/reborn-ui/ui/stats.js
git commit -m "profile: native public profile view; opponent names open it"
```

### Task 14: Game view ‹ Previous / Next ›

**Files:**
- Modify: `crates/reborn-ui/ui/game.js`, `crates/reborn-ui/ui/stats.js`, `crates/reborn-ui/ui/profile.js`
- Modify: `crates/reborn-ui/ui/stats-model.js` + test: add `neighbours(list, id) -> { prev: id|null, next: id|null }`

**Interfaces:**
- Produces: when a list page opens a game, it stores the ordered GAME ids of the matches it is showing in `sessionStorage['nsr.gameList']`. Matches go newest first, and each match's games are newest first within it, so "Previous" is always the older game. It is stored as JSON `{ ids: [...], label: 'Your matches'|'<name>'s matches' }`. Wrap the write in try/catch. `game.js` reads the key (also in try/catch) and draws ‹ Previous and Next › buttons, disabled at the ends. It draws nothing when the key is missing or the id is not in the list.

- [ ] **Step 1: Write the failing test**

```js
test('neighbours in a newest-first list', () => {
  assert.deepEqual(neighbours(['a', 'b', 'c'], 'b'), { prev: 'c', next: 'a' });
  assert.deepEqual(neighbours(['a', 'b'], 'a'), { prev: 'b', next: null });
  assert.deepEqual(neighbours(['a'], 'x'), { prev: null, next: null });
  assert.deepEqual(neighbours(null, 'a'), { prev: null, next: null });
});
```

"Previous" is the older game, and the list is newest first.

- [ ] **Step 2: Run to verify it fails, implement, and run to verify it passes**

Run: `node --test crates/reborn-ui/ui/stats-model.test.mjs`
Expected: FAIL, then PASS after adding:

```js
export function neighbours(list, id) {
  const ids = Array.isArray(list) ? list : [];
  const i = ids.indexOf(id);
  if (i === -1) return { prev: null, next: null };
  return { prev: ids[i + 1] ?? null, next: ids[i - 1] ?? null };
}
```

- [ ] **Step 3: Wire it up**

In `stats.js` and `profile.js`, write the key right before navigating to a game, with the ids of the list as currently filtered. In `game.js`, draw the two buttons above the header. They navigate to `game.html?game=<id>`, and Left/Right arrow keys do the same while focus is not in a text field.

- [ ] **Step 4: Commit**

```bash
git add crates/reborn-ui/ui/game.js crates/reborn-ui/ui/stats.js crates/reborn-ui/ui/profile.js crates/reborn-ui/ui/stats-model.js crates/reborn-ui/ui/stats-model.test.mjs
git commit -m "game view: step through the list you came from"
```

### Task 15: Whole-branch verification (no deploy)

**Files:** none new. This task only runs checks and writes the report.

- [ ] **Step 1: Backend suite**

Run in `reborn-backend-pbp`: `npm test; echo exit=$?`
Expected: `exit=0`. Record the pass count.

- [ ] **Step 2: Launcher suites**

Run in `rel-pbp`:

```bash
cargo test -p reborn-core -p reborn-ui -j 2
for f in crates/reborn-ui/ui/*.test.mjs; do node --test "$f" || echo "FAIL $f"; done
```

Expected: no failures beyond the baseline recorded in Task 10.

- [ ] **Step 3: CRLF guard**

Run: `git -C "<rel-pbp>" ls-files --eol crates | grep -E "w/crlf" | grep "\.rs$" || echo none`
Expected: `none`. CRLF in a `.rs` file breaks the source-scanning tests.

- [ ] **Step 4: Pickup data check (ruling R3)**

Look for a real pickup capture on this PC. Search the Reborn data folder's capture and results queue for a capture whose plays carry `"tgid":-1` on both sides:

```bash
grep -rl '"tgid":-1' "$LOCALAPPDATA/NFL Street Reborn" 2>/dev/null | head
```

- If one exists, run `gameTypeOf` on it and expect `"pickup"`.
- If none exists, write in the report that pickup detection is unconfirmed on a real game. Adam then plays one offline Pickup game on the new build, and it must show under Pickup in By team on his Stats page.

- [ ] **Step 5: Visual comparison**

Put the Task 12 and Task 13 screenshots side by side with `stats-layout-v17.html`. List every visible difference, then fix or rule on each. Pay particular attention to:
- text weight (Adam: "too bold")
- the totals line (one plain line)
- the Pickup row in By team
- the "Offline" tag
- a best of three shown as one match row
- no edits to shared launcher files: `git diff --stat release/3.0.27-beta-pbp...HEAD -- crates/reborn-ui/ui` lists only this plan's files

- [ ] **Step 6: Write the release checklist into the report**

Write this into the report, for Adam's go, and do NOT run any of it:
1. Deploy the backend: `npm test` exit 0, fast-forward `main`, `railway up --ci`. Migration 009 runs on boot.
2. Add a DNS `CNAME online` pointing at the Railway target, and `railway domain online.nflstreetreborn.com`. Then open `https://online.nflstreetreborn.com/live` in a browser once it resolves.
3. Set Railway env `PAGE_ORIGIN=https://online.nflstreetreborn.com` and `API_HOSTS=api.nflstreetreborn.com`, then redeploy. Check that an old `api.` `/live/<id>` link lands on `online.`.
4. Only after step 2 answers: publish the launcher beta that prints `online.` links.
