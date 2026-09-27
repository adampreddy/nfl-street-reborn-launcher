# Stats Everywhere and the Stats Tab — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every game's play-by-play, box score and drives get a permanent page, and the launcher's Stats tab lists the signed-in player's games from the server and opens those pages.

**Architecture:** No new plumbing at game time (captures already carry plays; the server already grades them). Backend: three fields on the player games list, a `/games/:id` HTML page built on `live-render.js`, links from the live pages. Launcher: two read-only commands over the existing `live_view` HTTP path, a Stats page rewritten around "Your games", and a native `game.html` view that reuses the live view's renderer.

**Tech Stack:** Backend: Node 22 / Express 5 / Postgres, `node --test` (`npm test`). Launcher: Rust (reborn-core, reborn-ui/Tauri), plain ES modules for pages, `cargo test`, `node --test` for a page's pure module.

**Spec:** `docs/superpowers/specs/2026-09-26-stats-everywhere-and-stats-tab-design.md`

Two parts in one document, each shippable on its own: Part 1 (backend, Tasks 1–3) then Part 2 (launcher, Tasks 4–7). Part 2 needs Part 1 deployed.

## Global Constraints

- Backend repo: `C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-pbp`, branch `feat/play-by-play`. Deploy = fast-forward `main` in `C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend` then `railway up --ci`. **Only after `npm test` exits 0** (check `$?`, never a grep of the output).
- Launcher repo: `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher rel-pbp`, branch `release/3.0.27-beta-pbp`. Every `.rs`, `.js`, `.html`, `.css` written here must be LF-only (`grep -c $'\r' <file>` prints 0): CRLF breaks the source-scanning tests.
- Player-facing sentences: the spec's exact words. Signed out: `Sign in with Discord on the Play page to see your games.` Empty: `No games yet. Play one and it shows up here.` Queue: `N games not uploaded yet` (`1 game not uploaded yet`). Totals-only note on the page: `Totals only for this game. It was played before play-by-play existed (or its plays could not be checked), and older totals may be inaccurate.` Badges: `Verified`, `Totals only`, `Held for review`.
- Everything the backend returns is untrusted player-typed data: pages draw it with `textContent` only (the `el()` helpers), never `innerHTML`.
- The web page CSP (`webRouter.js` `PAGE_CSP`) forbids inline scripts: every page script is a versioned `/live-assets/<file>?v=<hash>` asset.
- Visibility never widens: `/games/:id` (HTML) exists for any non-void game, but its data comes from the existing `/v1/games/:id*` routes, which 404 a hidden game to a non-participant. The launcher's native view sends the player's token, so a player sees their own offline games.
- Old games: `plays_status` `absent`, `failed`, `no-human` are all "Totals only". Nothing re-grades them.

## Review Focus

1. A game whose `teams[i].score` is missing or not a number (an older capture) — the list must show `–` for that score and `result: null`, not NaN or a crash. Pinned in Task 1.
2. A player viewing another player's list — `result` must be `null` for every row even when `team_index` is set (it is the viewer's own record only). Pinned in Task 1.
3. A verified game with no drives (a game with one play, or one that never changed possession) — the page's Drives tab must show the empty message, not throw on `drives.length`. Pinned in Task 2.
4. Opening a game page while signed out in the launcher for a game that is public (online) — must still render; only hidden games 404. Pinned in Task 5.
5. A Stats list answer that arrives after the player signed out mid-request — the page must draw the signed-out state, not the stale rows. Pinned in Task 6.

---

## Part 1 — Backend

### Task 1: `GET /v1/players/:id/games` returns `score`, `plays_status`, `result`

**Files:**
- Modify: `src/players.js` — `playerGames()` (currently ~line 45) and the route at ~line 194.
- Test: `test/players.test.js`

**Interfaces:**
- Consumes: the `game` table's `teams` jsonb (`[{name, score}, ...]`, capture order), `plays_status`, `status`; `participation.team_index`.
- Produces: each row of `games` additionally has `score: [number|null, number|null] | null`, `plays_status: string`, `result: "win"|"loss"|"tie"|null`. `result` is non-null only when `viewerId === playerId`, `status === "accepted"`, `team_index` is 0 or 1 and both scores are finite numbers.

- [ ] **Step 1: Write the failing test**

Append to `test/players.test.js` (it already has a harness that inserts games and participations; use the same helpers the file uses for `GET /v1/players/:id/games` — read the existing test named for that route and copy its setup):

```js
test("the games list carries score, plays_status and the viewer's own result", async () => {
  // Setup as the existing games-list test: a player `me`, an accepted online game where me is team_index 1
  // with teams [{name:"Eagles",score:36},{name:"Falcons",score:6}] and plays_status 'verified';
  // a second accepted game where me is team_index 0 and teams [{name:"A",score:7},{name:"B",score:7}] with plays_status 'absent';
  // a third HELD game (status 'held') where me is team_index 0 with scores 10-3.
  const rows = (await get(`/v1/players/${me.id}/games`, me)).body.games; // `get` = the file's authenticated GET helper
  const byName = Object.fromEntries(rows.map((g) => [g.teams[0].name, g]));
  assert.deepEqual(byName.Eagles.score, [36, 6]);
  assert.equal(byName.Eagles.plays_status, "verified");
  assert.equal(byName.Eagles.result, "loss", "me was team 1 (Falcons, 6) against 36");
  assert.equal(byName.A.result, "tie");
  assert.equal(byName.A.plays_status, "absent");
  assert.equal(byName.Held.result, null, "a held game has no result"); // name the held game's team[0] "Held"
  // Another viewer sees the same rows with result null, and a missing score is null, never NaN.
  const other = await signIn(pool, "other-discord-id", "Other");
  const theirs = (await get(`/v1/players/${me.id}/games`, other)).body.games;
  assert.ok(theirs.every((g) => g.result === null));
  // A capture without scores: insert a fourth accepted game with teams [{name:"X"},{name:"Y"}] for me, then:
  const mine = (await get(`/v1/players/${me.id}/games`, me)).body.games.find((g) => g.teams[0].name === "X");
  assert.equal(mine.score, null);
  assert.equal(mine.result, null);
});
```

Adapt the setup lines to the file's real helpers (`signIn`, `insertGame`/`newGame`, `participate`, `get`) — copy them from the existing list test in the same file. Do not invent new helpers.

- [ ] **Step 2: Run it to verify it fails**

Run: `cd "C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-pbp" && node --env-file-if-exists=.env --test test/players.test.js`
Expected: the new test fails on `score` being `undefined`.

- [ ] **Step 3: Implement**

In `src/players.js`, change `playerGames`:

```js
export async function playerGames(pool, playerId, { viewerId = null, limit = 50, before = null } = {}) {
  const lim = Math.max(1, Math.min(200, Math.trunc(Number(limit)) || 50));
  const rows = (await pool.query(
    `SELECT g.id, g.mode, g.teams, g.status, g.hold_reason, g.captured_at, g.first_seen_at, g.plays_status, pa.team_index
       FROM participation pa JOIN game g ON g.id = pa.game_id JOIN player p ON p.id = pa.player_id
      WHERE pa.player_id = $1 AND g.status <> 'void' AND ${VISIBLE} AND ($3::timestamptz IS NULL OR g.first_seen_at < $3)
      ORDER BY g.first_seen_at DESC LIMIT $4`,
    [playerId, viewerId, before, lim],
  )).rows;
  const own = viewerId !== null && String(viewerId).toLowerCase() === String(playerId).toLowerCase();
  return rows.map((g) => ({ ...g, score: scoreOf(g.teams), result: own ? resultOf(g) : null }));
}

/** [home, away] from the capture's own team order; null when the capture carries no usable score. */
export function scoreOf(teams) {
  if (!Array.isArray(teams) || teams.length < 2) return null;
  const s = [0, 1].map((i) => (Number.isFinite(teams[i]?.score) ? teams[i].score : null));
  return s[0] === null && s[1] === null ? null : s;
}

/** The player's own outcome: only for an accepted game with a side and two finite scores. */
export function resultOf(g) {
  if (g.status !== "accepted") return null;
  const t = g.team_index;
  if (t !== 0 && t !== 1) return null;
  const s = scoreOf(g.teams);
  if (!s || s[0] === null || s[1] === null) return null;
  if (s[t] === s[1 - t]) return "tie";
  return s[t] > s[1 - t] ? "win" : "loss";
}
```

The `${VISIBLE}` fragment and the `p` join are the existing ones; leave them. `plays_status` is a plain column added to the SELECT.

- [ ] **Step 4: Run the file's tests**

Run: `node --env-file-if-exists=.env --test test/players.test.js`
Expected: all pass, including the new test.

- [ ] **Step 5: Commit**

```bash
git add src/players.js test/players.test.js
git commit -m "players: the games list carries score, plays_status and the viewer's own result"
```

### Task 2: `GET /games/:id` — the permanent game page

**Files:**
- Create: `src/web/game.html`, `src/web/game-page.js`
- Modify: `src/webRouter.js` (assets map, page constant, route), `src/web/live.css` (one section)
- Test: `test/web_game_page.test.js` (new), `test/web_page.test.js` (one renderer test)

**Interfaces:**
- Consumes: `/v1/games/:id` (`{ok, game:{id, mode, teams, team_totals, status, plays_status, plays_checks, captured_at, participants}}` — read the route at `players.js:215` to confirm the envelope key and copy it), `/v1/games/:id/plays` (`{plays_status, plays:[{body, situation}]}`), `/v1/games/:id/boxscore` (`{teams, lines}`), `/v1/games/:id/drives` (`{drives}`).
- Consumes from `live-render.js`: `renderHeader(snapshot, doc, {game})`, `renderPlays(game, doc, {})`, `renderBox(lines, doc, teams, {})`, `renderDrives(drives, seats, sides, doc)`.
- Produces: the page at `/games/<uuid>`; the snapshot-shaped object `game-page.js` builds, named here so Task 3 can link and Task 5 can mirror it: `{ match: { status: "complete" }, seats: [], games: [ { game_seq: 1, game_id, score, teams, sides: [], plays, box: { provisional: false, lines }, drives } ] }`.

- [ ] **Step 1: Write the failing route test**

`test/web_game_page.test.js`:

```js
// /games/:id is the permanent page for one game: the live page's renderer on a finished game.
import test from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { webRouter, PAGE_CSP } from "../src/webRouter.js";

function pool(status) {
  return { query: async (sql) => ({ rows: /FROM game/.test(sql) && status ? [{ status }] : [] }) };
}
async function serve(status) {
  const app = express();
  app.use(webRouter({ pool: pool(status) }));
  const server = await new Promise((r) => { const s = app.listen(0, "127.0.0.1", () => r(s)); });
  return { base: `http://127.0.0.1:${server.address().port}`, close: () => new Promise((r) => server.close(r)) };
}
const ID = "12345678-1234-4123-8123-123456789abc";

test("/games/:id serves the game page for an accepted game, with the versioned script and the id in the meta", async () => {
  const { base, close } = await serve("accepted");
  try {
    const res = await fetch(`${base}/games/${ID}`);
    assert.equal(res.status, 200);
    assert.equal(res.headers.get("content-security-policy"), PAGE_CSP);
    const html = await res.text();
    assert.match(html, new RegExp(`<meta name="nsr-game" content="${ID}">`));
    const m = html.match(/"\/live-assets\/game-page\.js\?v=([0-9a-f]{12})"/);
    assert.ok(m, "versioned page script");
    const js = await fetch(`${base}/live-assets/game-page.js?v=${m[1]}`);
    assert.equal(js.status, 200);
    assert.match(await js.text(), /\/v1\/games\/\$\{encodeURIComponent\(gameId\)\}\/plays/, "reads the plays route");
  } finally { await close(); }
});

test("/games/:id is a 404 page for a void game, an unknown game and a malformed id", async () => {
  for (const [status, id] of [["void", ID], [null, ID], ["accepted", "not-a-uuid"]]) {
    const { base, close } = await serve(status);
    try { assert.equal((await fetch(`${base}/games/${id}`)).status, 404, `${status} ${id}`); } finally { await close(); }
  }
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `node --env-file-if-exists=.env --test test/web_game_page.test.js`
Expected: FAIL — `/games/<id>` answers 404 (no route) for the accepted case.

- [ ] **Step 3: Create `src/web/game.html`**

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="nsr-game" content="__NSR_GAME__">
<title>Game - NFL Street Reborn</title>
<link rel="stylesheet" href="/live-assets/live.css">
<link rel="stylesheet" href="/live-assets/teams.css">
<script type="module" src="/live-assets/game-page.js"></script>
</head>
<body>
<header id="board" class="board"><p class="loading">Loading the game&hellip;</p></header>
<p id="note" class="note" role="status" aria-live="polite"></p>
<div class="tabs" role="tablist" aria-label="Game views">
  <button type="button" role="tab" id="tabbtn-plays" aria-controls="tab-plays" data-tab="plays" aria-selected="true">Plays</button>
  <button type="button" role="tab" id="tabbtn-box" aria-controls="tab-box" data-tab="box" aria-selected="false" tabindex="-1">Box score</button>
  <button type="button" role="tab" id="tabbtn-drives" aria-controls="tab-drives" data-tab="drives" aria-selected="false" tabindex="-1">Drives</button>
</div>
<main>
  <section id="tab-plays" class="panel" role="tabpanel" aria-labelledby="tabbtn-plays" tabindex="0"></section>
  <section id="tab-box" class="panel" role="tabpanel" aria-labelledby="tabbtn-box" tabindex="0" hidden></section>
  <section id="tab-drives" class="panel" role="tabpanel" aria-labelledby="tabbtn-drives" tabindex="0" hidden></section>
</main>
<footer><a class="note" href="/live">Live now</a><span class="brand">NFL Street Reborn</span></footer>
</body>
</html>
```

- [ ] **Step 4: Create `src/web/game-page.js`**

```js
// The permanent page for one game: /v1/games/<id> (teams, totals, status), then its plays, box score and drives,
// drawn by live-render.js exactly as the live page draws a recorded game. No stream, no polling: a finished game.
import { renderHeader, renderPlays, renderBox, renderDrives, gameSnapshot } from "./live-render.js";

const TOTALS_ONLY = "Totals only for this game. It was played before play-by-play existed (or its plays could not be checked), and older totals may be inaccurate.";
const NOT_FOUND = "That game couldn't be found.";
const TABS = ["plays", "box", "drives"];
const gameId = document.querySelector('meta[name="nsr-game"]')?.getAttribute("content") ?? "";
const $ = (id) => document.getElementById(id);
const board = $("board"), note = $("note");
const panels = { plays: $("tab-plays"), box: $("tab-box"), drives: $("tab-drives") };
let tab = "plays";

function el(tag, className, text) {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text !== undefined) n.textContent = text;
  return n;
}

async function getJson(url) {
  const res = await fetch(url, { headers: { accept: "application/json" } });
  return res.ok ? res.json() : null;
}

// The snapshot shape live-render.js reads is built by `gameSnapshot` there (Step 6), so it is testable under Node.

function showTab(name) {
  tab = name;
  for (const t of TABS) {
    panels[t].hidden = t !== name;
    const b = $(`tabbtn-${t}`);
    b.setAttribute("aria-selected", String(t === name));
    b.tabIndex = t === name ? 0 : -1;
  }
}

function draw(snap, verified) {
  const game = snap.games[0];
  board.replaceChildren(renderHeader(snap, document, { game }));
  if (!verified) {
    for (const p of Object.values(panels)) p.replaceChildren(el("p", "empty", TOTALS_ONLY));
    return;
  }
  panels.plays.replaceChildren(renderPlays(game, document, {}));
  panels.box.replaceChildren(el("p", "label", "Final box score"), renderBox(game.box.lines, document, game.teams.map((t) => t.name), {}));
  panels.drives.replaceChildren(renderDrives(game.drives ?? [], snap.seats, game.sides, document));
}

async function load() {
  const detail = await getJson(`/v1/games/${encodeURIComponent(gameId)}`);
  const game = detail?.game ?? detail; // the route's envelope; see players.js GET /v1/games/:id
  if (!detail?.ok || !game?.id) { board.replaceChildren(el("p", "empty", NOT_FOUND)); return; }
  const verified = game.plays_status === "verified";
  const [plays, box, drives] = verified
    ? await Promise.all([
      getJson(`/v1/games/${encodeURIComponent(gameId)}/plays`),
      getJson(`/v1/games/${encodeURIComponent(gameId)}/boxscore`),
      getJson(`/v1/games/${encodeURIComponent(gameId)}/drives`),
    ])
    : [null, null, null];
  draw(gameSnapshot(game, plays?.plays ?? [], box?.lines ?? [], drives?.drives ?? []), verified);
  note.textContent = verified ? "Final" : "";
}

for (const t of TABS) $(`tabbtn-${t}`).addEventListener("click", () => showTab(t));
document.querySelector(".tabs")?.addEventListener("keydown", (e) => {
  if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
  const i = TABS.indexOf(tab);
  const next = TABS[(i + (e.key === "ArrowRight" ? 1 : TABS.length - 1)) % TABS.length];
  showTab(next);
  $(`tabbtn-${next}`).focus();
});
load().catch(() => board.replaceChildren(el("p", "empty", NOT_FOUND)));
```

Before writing, read `players.js` `GET /v1/games/:id` (~line 215) and set the `detail?.game ?? detail` line to the real envelope (if the route answers `{ok:true, ...game}` flat, use `detail` alone). Read `renderHeader`'s `teamOf(game, side)` (live-render.js) to confirm it reads `game.teams[side]` by `side` and the `tgid`/`tmc` names; match them exactly.

- [ ] **Step 5: Wire the route in `src/webRouter.js`**

Add to the `ASSETS` map: `["game-page.js", { type: "text/javascript; charset=utf-8", body: read("game-page.js").replace('from "./live-render.js"', \`from "./live-render.js?v=${hashOf(render)}"\`) }],` (the same versioned import rewrite `live-page.js` gets; assert it took, as the page's line does).

After `INDEX`:

```js
const GAME_SLOT = "__NSR_GAME__";
const GAME_PAGE = ["live.css", "teams.css", "game-page.js"].reduce((html, file) => {
  const plain = `"/live-assets/${file}"`;
  if (!html.includes(plain)) throw new Error(`game.html lost its ${file} reference`);
  return html.replace(plain, `"${versioned(file)}"`);
}, read("game.html"));
if (!GAME_PAGE.includes(`content="${GAME_SLOT}"`)) throw new Error("game.html lost its nsr-game slot");
```

Route, next to `/live/:matchId`:

```js
  r.get("/games/:gameId", limit, async (req, res, next) => {
    try {
      const gid = String(req.params.gameId ?? "").toLowerCase();
      if (!UUID_RE.test(gid)) return missing(res);
      const g = (await pool.query(`SELECT status FROM game WHERE id = $1`, [gid])).rows[0];
      if (!g || g.status === "void") return missing(res);
      secure(res);
      res.set("cache-control", "no-cache");
      res.status(200).type("html").send(GAME_PAGE.replace(GAME_SLOT, gid));
    } catch (e) { next(e); }
  });
```

Add to `live.css` a `.footer a.note { color: inherit; }` rule if the footer link has no style (check by eye in `live.css`; skip if links are already styled).

- [ ] **Step 6: Move the snapshot builder into the renderer and test it there**

`game-page.js` reads `document` at import, so its snapshot builder is not testable under Node. Put it in `live-render.js` instead (pure, exported), and have `game-page.js` import it — replace `teamsOf` and `snapshotOf` in `game-page.js` with `import { gameSnapshot } from "./live-render.js"` and call `gameSnapshot(game, plays, lines, drives)`:

```js
// live-render.js
/** The snapshot shape this renderer reads, built from one FINISHED game (the /games/:id page and the launcher's
 *  native game view both build it): team identity from the plays' own `teams` when there are plays, else the
 *  capture's names with no logo; the capture's scores; one game, no seats, no sides. */
export function gameSnapshot(game, plays, lines, drives) {
  const fromPlays = (plays ?? []).find((p) => Array.isArray(p.body?.teams) && p.body.teams.length === 2)?.body.teams;
  const teams = [0, 1].map((side) => ({
    side,
    tgid: Number.isInteger(fromPlays?.[side]?.tgid) ? fromPlays[side].tgid : null,
    tmc: fromPlays?.[side]?.tmc ?? null,
    name: game.teams?.[side]?.name ?? `Side ${side + 1}`,
    abbrev: game.teams?.[side]?.abbrev ?? null,
  }));
  const score = [0, 1].map((i) => (Number.isFinite(game.teams?.[i]?.score) ? game.teams[i].score : 0));
  return {
    match: { status: "complete", best_of: 1 },
    seats: [],
    games: [{ game_seq: 1, game_id: game.id, score, teams, sides: [], plays: plays ?? [], box: { provisional: false, lines: lines ?? [] }, drives: drives ?? [] }],
  };
}
```

Append to `test/web_page.test.js` (it already imports `renderHeader`, `renderDrives` and has a `doc` factory — reuse them; add `gameSnapshot` to the import):

```js
test("gameSnapshot builds a header-renderable game from a capture with no plays, and empty drives render without throwing", () => {
  const game = { id: "g", teams: [{ name: "Eagles", score: 36 }, { name: "Falcons", score: 6 }], plays_status: "absent" };
  const snap = gameSnapshot(game, [], [], []);
  assert.deepEqual(snap.games[0].score, [36, 6]);
  assert.equal(snap.games[0].teams[1].name, "Falcons");
  assert.equal(snap.games[0].teams[1].tgid, null, "no plays: no logo, the name badge draws");
  const header = renderHeader(snap, doc(), { game: snap.games[0] });
  assert.ok(header.textContent.includes("Eagles") && header.textContent.includes("Falcons"));
  assert.doesNotThrow(() => renderDrives([], [], [], doc()));
  // With plays, team identity comes from the plays' own teams.
  const withPlays = gameSnapshot(game, [{ body: { teams: [{ tgid: 14, tmc: [1, 2, 3] }, { tgid: 8, tmc: [4, 5, 6] }] } }], [], []);
  assert.equal(withPlays.games[0].teams[0].tgid, 14);
});
```

- [ ] **Step 7: Run the tests**

Run: `node --env-file-if-exists=.env --test test/web_game_page.test.js test/web_page.test.js test/live_index.test.js`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/web/game.html src/web/game-page.js src/web/live-render.js src/webRouter.js src/web/live.css test/web_game_page.test.js test/web_page.test.js
git commit -m "web: /games/:id, the permanent page for one game, on the live page's renderer"
```

### Task 3: Live pages link finished games to their pages

**Files:**
- Modify: `src/web/live-render.js` (one exported helper), `src/web/live-page.js` (`drawPicker`), `src/web/live-index.js` (row meta)
- Test: `test/web_page.test.js`

**Interfaces:**
- Produces: `export function gamePageHref(game)` in `live-render.js` → `/games/<game_id>` when `game.game_id` is a canonical UUID, else `null`.

- [ ] **Step 1: Failing test** (append to `test/web_page.test.js`):

```js
test("a recorded game links to its permanent page; an unrecorded one does not", async () => {
  const { gamePageHref } = await import("../src/web/live-render.js");
  assert.equal(gamePageHref({ game_id: "12345678-1234-4123-8123-123456789abc" }), "/games/12345678-1234-4123-8123-123456789abc");
  assert.equal(gamePageHref({ game_id: null }), null);
  assert.equal(gamePageHref({ game_id: "../etc" }), null);
});
```

- [ ] **Step 2: Run to verify it fails** — `node --env-file-if-exists=.env --test test/web_page.test.js` → `gamePageHref is not a function`.

- [ ] **Step 3: Implement**

`live-render.js`:

```js
const GAME_ID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
/** The permanent page of a recorded game, or null while it has none. */
export function gamePageHref(game) {
  const id = typeof game?.game_id === "string" ? game.game_id.toLowerCase() : "";
  return GAME_ID_RE.test(id) ? `/games/${id}` : null;
}
```

`live-page.js` `drawPicker`: after each game button, when `gamePageHref(g)` is non-null, append `const a = el("a", "game-link", "Game page"); a.href = href;` inside the picker (`gamesNav.replaceChildren(...)` becomes a flat list of button-or-fragment; wrap button + link in a `span.game-pick`). Import `gamePageHref` at the top. Keep `gamesNav.hidden = games.length < 2` — with one recorded game the link must still be reachable, so change that line to `gamesNav.hidden = games.length < 2 && !games.some((g) => gamePageHref(g));`.

`live-index.js` `row(m)`: no `game_id` on the list entries, so nothing changes there; the index links to the match page, which links to game pages.

- [ ] **Step 4: Run** `node --env-file-if-exists=.env --test test/web_page.test.js` → pass.

- [ ] **Step 5: Commit**

```bash
git add src/web/live-render.js src/web/live-page.js test/web_page.test.js
git commit -m "live page: a recorded game links to its permanent page"
```

- [ ] **Step 6: Full suite, then deploy (Part 1 done)**

```bash
npm test > "$TEMP/npm_test.log" 2>&1; echo "exit $?"    # must print exit 0; read the ℹ lines from the log
cd "C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend" && git merge --ff-only feat/play-by-play && railway up --ci
curl -s -o /dev/null -w "%{http_code}\n" https://api.nflstreetreborn.com/games/a51767ab-0000-0000-0000-000000000000   # 404 (unknown), proves the route is up
```

Then open `https://api.nflstreetreborn.com/games/<a real accepted game id from /v1/players/0040ba1e-ee6f-47ea-bb8e-b2104247f36a/games>` in a browser and confirm the header and the three tabs draw.

---

## Part 2 — Launcher

### Task 4: `reborn_core::live_view` reads a player's games and one game's record

**Files:**
- Modify: `crates/reborn-core/src/live_view.rs`
- Test: same file's `mod tests` (it has an `Http` double; read the existing `list`/`snapshot` tests and reuse their `Script`/fake).

**Interfaces:**
- Consumes: `crate::net::Http` (`get(url, headers)` as the existing functions use it — read `list()` for the exact call), `LiveError`.
- Produces:
  - `pub fn player_games(http: &dyn Http, origin: &str, token: Option<&str>, player_id: &str, limit: u32, before: Option<&str>) -> Result<Value, LiveError>` → the JSON `games` array as returned by `/v1/players/:id/games` (Task 1 fields included). `player_id` must satisfy `is_canonical_uuid`, else `LiveError::BadId`. `token` → `Authorization: Bearer <token>` header when `Some`.
  - `pub fn game_record(http: &dyn Http, origin: &str, token: Option<&str>, game_id: &str) -> Result<Option<Value>, LiveError>` → `None` on 404; otherwise `json!({ "game": <detail>, "plays": <plays or []>, "lines": <box lines or []>, "drives": <drives or []> })`, reading `/plays`, `/boxscore`, `/drives` only when `game.plays_status == "verified"`.

- [ ] **Step 1: Failing tests** (append inside `mod tests`, using the file's existing HTTP double the way `snapshot` tests do):

```rust
    #[test]
    fn player_games_reads_the_list_with_the_bearer_token_and_refuses_a_bad_id() {
        let http = Script::new(vec![ok(200, r#"{"ok":true,"games":[{"id":"g1","score":[36,6],"result":"loss","plays_status":"verified"}]}"#)]);
        let v = player_games(&http, "https://api.test", Some("tok"), "12345678-1234-4123-8123-123456789abc", 50, None).unwrap();
        assert_eq!(v[0]["result"], "loss");
        let sent = http.requests();  // the double records (url, headers); adapt to its real accessor
        assert!(sent[0].0.ends_with("/v1/players/12345678-1234-4123-8123-123456789abc/games?limit=50"), "{}", sent[0].0);
        assert!(sent[0].1.iter().any(|(k, v)| k == "Authorization" && v == "Bearer tok"));
        assert!(matches!(player_games(&http, "https://api.test", None, "../x", 50, None), Err(LiveError::BadId)));
    }

    #[test]
    fn game_record_is_none_on_404_and_reads_plays_box_and_drives_only_for_a_verified_game() {
        let http = Script::new(vec![ok(404, r#"{"ok":false}"#)]);
        assert!(game_record(&http, "https://api.test", None, "12345678-1234-4123-8123-123456789abc").unwrap().is_none());
        let http = Script::new(vec![ok(200, r#"{"ok":true,"game":{"id":"g","plays_status":"absent","teams":[]}}"#)]);
        let v = game_record(&http, "https://api.test", None, "12345678-1234-4123-8123-123456789abc").unwrap().unwrap();
        assert_eq!(v["plays"].as_array().unwrap().len(), 0);
        assert_eq!(http.requests().len(), 1, "an unverified game reads only the detail");
        let http = Script::new(vec![
            ok(200, r#"{"ok":true,"game":{"id":"g","plays_status":"verified","teams":[]}}"#),
            ok(200, r#"{"ok":true,"plays":[{"body":{"seq":1}}]}"#),
            ok(200, r#"{"ok":true,"lines":[{"digp":1}]}"#),
            ok(200, r#"{"ok":true,"drives":[{"side":0}]}"#),
        ]);
        let v = game_record(&http, "https://api.test", None, "12345678-1234-4123-8123-123456789abc").unwrap().unwrap();
        assert_eq!(v["plays"][0]["body"]["seq"], 1);
        assert_eq!(v["lines"][0]["digp"], 1);
        assert_eq!(v["drives"][0]["side"], 0);
    }
```

Read the existing tests first and use their real double (`Script`, `ok`, the recorded-requests accessor). If the double does not record headers, extend it to (a `Vec<(String, Vec<(String,String)>)>`), keeping the old tests green. Match the `/v1/games/:id` envelope to what Task 1's route really returns (`game` key or flat) — the same decision as Task 2 Step 4.

- [ ] **Step 2: Run to verify they fail** — `cd "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher rel-pbp" && cargo test -p reborn-core live_view` → compile error (functions missing).

- [ ] **Step 3: Implement** (next to `snapshot`, same style; `get_json` is whatever helper `snapshot` uses — read it):

```rust
fn bearer(token: Option<&str>) -> Vec<(String, String)> {
    token.map(|t| vec![("Authorization".to_string(), format!("Bearer {t}"))]).unwrap_or_default()
}

/// `GET /v1/players/{id}/games`: the player's games, newest first, with the fields the Stats tab draws.
pub fn player_games(http: &dyn Http, origin: &str, token: Option<&str>, player_id: &str, limit: u32, before: Option<&str>) -> Result<Value, LiveError> {
    if !is_canonical_uuid(player_id) { return Err(LiveError::BadId); }
    let mut url = format!("{}/v1/players/{player_id}/games?limit={}", origin.trim_end_matches('/'), limit.clamp(1, 200));
    if let Some(b) = before { url.push_str("&before="); url.push_str(&percent_encode(b)); }
    let body = get_json(http, &url, &bearer(token))?;   // the same status handling `list` uses: 404/5xx -> LiveError
    Ok(body.get("games").cloned().unwrap_or(Value::Array(vec![])))
}

/// One finished game: its record, and its plays / box score lines / drives when they are verified.
pub fn game_record(http: &dyn Http, origin: &str, token: Option<&str>, game_id: &str) -> Result<Option<Value>, LiveError> {
    if !is_canonical_uuid(game_id) { return Err(LiveError::BadId); }
    let base = format!("{}/v1/games/{game_id}", origin.trim_end_matches('/'));
    let h = bearer(token);
    let detail = match get_json_opt(http, &base, &h)? { Some(d) => d, None => return Ok(None) };
    let game = detail.get("game").cloned().unwrap_or(detail.clone());
    let verified = game.get("plays_status").and_then(Value::as_str) == Some("verified");
    let part = |path: &str, key: &str| -> Result<Value, LiveError> {
        if !verified { return Ok(Value::Array(vec![])); }
        Ok(get_json(http, &format!("{base}/{path}"), &h)?.get(key).cloned().unwrap_or(Value::Array(vec![])))
    };
    Ok(Some(serde_json::json!({ "game": game, "plays": part("plays", "plays")?, "lines": part("boxscore", "lines")?, "drives": part("drives", "drives")? })))
}
```

`get_json_opt` = `get_json` that maps a 404 to `Ok(None)` — write it beside `get_json` if `snapshot` does not already have one (it maps 404 to `None`; reuse its code path). `percent_encode` for `before` (an ISO timestamp: only `:` and `+` need escaping; a tiny function replacing `:` with `%3A` and `+` with `%2B` is enough, with a unit test line: `assert_eq!(percent_encode("2026-09-27T00:00:00Z"), "2026-09-27T00%3A00%3A00Z")`).

- [ ] **Step 4: Run** `cargo test -p reborn-core live_view` → all pass. Check `grep -c $'\r' crates/reborn-core/src/live_view.rs` prints 0.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-core/src/live_view.rs
git commit -m "live_view: a player's games and one game's record, read with the player's token"
```

### Task 5: Launcher commands `stats_my_games` and `stats_game`

**Files:**
- Modify: `crates/reborn-ui/src/live_view.rs` (two commands beside `live_matches`), `crates/reborn-ui/src/main.rs` (`generate_handler![...]` at ~line 6179: add `live_view::stats_my_games, live_view::stats_game,`)
- Test: `crates/reborn-ui/src/live_view.rs` `mod tests`

**Interfaces:**
- Consumes: Task 4's `player_games` / `game_record`; `reborn_core::identity::{status, token, origin}` (`status(&p) -> Result<Option<IdentityFile>>` with `file.player.id`).
- Produces (Tauri):
  - `stats_my_games(limit: Option<u32>, before: Option<String>) -> Result<serde_json::Value, LiveCommandError>` → `{ "signed_in": bool, "player_id": string|null, "games": [...] }`. Signed out → `{ signed_in: false, player_id: null, games: [] }` (not an error: the page draws its signed-out state).
  - `stats_game(game_id: String) -> Result<Option<serde_json::Value>, LiveCommandError>` → Task 4's record or `None`; works signed out for public games (token `None`).

- [ ] **Step 1: Failing test** (the module's tests test `LiveCommandError` mapping today; add a pure helper test):

```rust
    #[test]
    fn my_games_view_is_the_signed_out_shape_without_an_identity() {
        let v = my_games_view(None, serde_json::Value::Array(vec![]));
        assert_eq!(v["signed_in"], false);
        assert!(v["player_id"].is_null());
        assert_eq!(v["games"].as_array().unwrap().len(), 0);
        let v = my_games_view(Some("p1"), serde_json::json!([{"id":"g"}]));
        assert_eq!(v["signed_in"], true);
        assert_eq!(v["player_id"], "p1");
        assert_eq!(v["games"][0]["id"], "g");
    }
```

- [ ] **Step 2: Run** `cargo test -p reborn-ui live_view` → compile error.

- [ ] **Step 3: Implement**

```rust
fn my_games_view(player_id: Option<&str>, games: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "signed_in": player_id.is_some(), "player_id": player_id, "games": games })
}

/// The signed-in player's games from the server (Stats tab). Signed out is a shape, not an error.
#[tauri::command]
pub async fn stats_my_games(limit: Option<u32>, before: Option<String>) -> Result<serde_json::Value, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let Some(file) = identity::status(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))? else {
            return Ok(my_games_view(None, serde_json::Value::Array(vec![])));
        };
        let token = identity::token(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let http = crate::http_client();
        let games = reborn_core::live_view::player_games(&http, &origin, token.as_deref(), &file.player.id, limit.unwrap_or(50), before.as_deref())
            .map_err(LiveCommandError::from)?;
        Ok(my_games_view(Some(&file.player.id), games))
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}

/// One game's record for the native game view, with the player's token when signed in.
#[tauri::command]
pub async fn stats_game(game_id: String) -> Result<Option<serde_json::Value>, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let token = identity::token(&p).unwrap_or(None);
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        let http = crate::http_client();
        reborn_core::live_view::game_record(&http, &origin, token.as_deref(), &game_id).map_err(LiveCommandError::from)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}
```

Register both in `main.rs`'s `generate_handler![...]` next to `live_view::live_match`. If `main.rs` has a test that lists every command name (search for `"live_matches"` in `main.rs` tests), add the two names there too.

- [ ] **Step 4: Run** `cargo test -p reborn-ui` → all pass (105 before this task). CRLF check on both files.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/src/live_view.rs crates/reborn-ui/src/main.rs
git commit -m "stats: commands for the signed-in player's games and one game's record"
```

### Task 6: The Stats tab, and the native game view

**Files:**
- Create: `crates/reborn-ui/ui/stats-rows.js` (pure), `crates/reborn-ui/ui/stats-rows.test.mjs`, `crates/reborn-ui/ui/game.html`, `crates/reborn-ui/ui/game.js`, `crates/reborn-ui/ui/live-game-render.js`
- Modify: `crates/reborn-ui/ui/stats.js` (rewrite), `crates/reborn-ui/ui/stats.css`, `crates/reborn-ui/ui/live.js` (import the moved renderers)
- Test: `stats-rows.test.mjs` (node), `cargo test -p reborn-ui` (source scans)

**Interfaces:**
- Consumes: `stats_my_games`, `stats_game`, `stats_queue` (`{ entries: [{ key, state, ... }] }` — `state` is the queue's `EntryState`; count entries whose `state` is not the finished one: read `EntryState` in `reborn-core/src/stats.rs` and the `stats_queue` command's JSON in `main.rs` ~line 4183 for the exact names), `identity_status` (`{signed_in, player_id, display_name}`), the live view's `LIVE_ORIGIN, assetUrl` from `live-common.js`.
- Produces: `stats-rows.js`: `export function rowModel(game)` → `{ id, date, time, home:{name, abbrev, tgid|null}, away:{...}, score:[a,b]|null, mine: 0|1|null, result:"W"|"L"|"T"|null, badge:"Verified"|"Totals only"|"Held for review", mode:"Online"|"Offline" }` — `score` in the capture's order; `mine` is `team_index`; `game.html?game=<id>` is the native view's URL.

- [ ] **Step 1: Failing test** `stats-rows.test.mjs` (pure module, no DOM):

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rowModel, queueLine } from './stats-rows.js';

test('rowModel turns a games-list entry into what a row draws', () => {
  const r = rowModel({ id: 'g1', mode: 'online', status: 'accepted', captured_at: '2026-09-27T01:41:00Z',
    teams: [{ name: 'Eagles', score: 36 }, { name: 'Falcons', score: 6 }], team_index: 1, score: [36, 6], result: 'loss', plays_status: 'verified' });
  assert.equal(r.id, 'g1');
  assert.equal(r.home.name, 'Eagles');
  assert.deepEqual(r.score, [36, 6]);
  assert.equal(r.mine, 1);
  assert.equal(r.result, 'L');
  assert.equal(r.badge, 'Verified');
  assert.equal(r.mode, 'Online');
});

test('rowModel: totals-only, held, and missing pieces', () => {
  assert.equal(rowModel({ id: 'a', mode: 'offline', status: 'accepted', teams: [{ name: 'A' }, { name: 'B' }], plays_status: 'absent' }).badge, 'Totals only');
  assert.equal(rowModel({ id: 'b', mode: 'offline', status: 'accepted', teams: [{ name: 'A' }, { name: 'B' }], plays_status: 'failed' }).badge, 'Totals only');
  const held = rowModel({ id: 'c', mode: 'online', status: 'held', hold_reason: 'left-before-start', teams: [{ name: 'A' }, { name: 'B' }], plays_status: 'absent' });
  assert.equal(held.badge, 'Held for review');
  assert.equal(held.result, null);
  const bare = rowModel({ id: 'd', mode: 'offline', status: 'accepted', teams: null, plays_status: 'absent' });
  assert.equal(bare.home.name, 'Side 1');
  assert.equal(bare.score, null);
  assert.equal(bare.mode, 'Offline');
});

test('queueLine counts what has not uploaded, in the spec\'s words', () => {
  assert.equal(queueLine([]), null);
  assert.equal(queueLine([{ state: 'pending' }]), '1 game not uploaded yet');
  assert.equal(queueLine([{ state: 'pending' }, { state: 'failed' }, { state: 'done' }]), '2 games not uploaded yet');
});
```

Set the `state` strings to the real `EntryState` names once read (`done`/`sent`/`uploaded` — whatever the finished state is); the rule is "count entries whose state is not the finished state".

- [ ] **Step 2: Run** `node --test crates/reborn-ui/ui/stats-rows.test.mjs` → fails (module missing).

- [ ] **Step 3: Write `stats-rows.js`**

```js
// The Stats tab's row model: pure, no DOM, so it is testable under node. Everything here is data the
// server returned (player-typed team names among it); the page draws it with textContent only.
const RESULT = { win: 'W', loss: 'L', tie: 'T' };
const FINISHED_STATES = new Set(['done']); // the queue's finished EntryState (read reborn-core/src/stats.rs)

function team(t, side) {
  return { name: t && t.name ? String(t.name) : `Side ${side + 1}`, abbrev: t && t.abbrev ? String(t.abbrev) : null, tgid: Number.isInteger(t?.tgid) ? t.tgid : null };
}

export function rowModel(game) {
  const teams = Array.isArray(game.teams) ? game.teams : [];
  const when = new Date(game.captured_at ?? game.first_seen_at ?? NaN);
  const ok = Number.isFinite(when.getTime());
  const score = Array.isArray(game.score) && game.score.length === 2 ? game.score : null;
  const badge = game.status === 'held' ? 'Held for review' : game.plays_status === 'verified' ? 'Verified' : 'Totals only';
  return {
    id: String(game.id ?? ''),
    date: ok ? when.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' }) : 'Date unavailable',
    time: ok ? when.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' }) : '',
    home: team(teams[0], 0), away: team(teams[1], 1),
    score,
    mine: game.team_index === 0 || game.team_index === 1 ? game.team_index : null,
    result: game.status === 'accepted' ? (RESULT[game.result] ?? null) : null,
    badge,
    mode: game.mode === 'online' ? 'Online' : 'Offline',
  };
}

export function queueLine(entries) {
  const n = (Array.isArray(entries) ? entries : []).filter((e) => !FINISHED_STATES.has(String(e?.state ?? ''))).length;
  if (n === 0) return null;
  return `${n} ${n === 1 ? 'game' : 'games'} not uploaded yet`;
}
```

- [ ] **Step 4: Run** the node test → passes.

- [ ] **Step 5: Move the live view's renderers into `live-game-render.js`**

Cut from `live.js` the functions `initialOf, newestGame, playsOf, seatScore, teamOf, sideLabel, gameStatus, withFallback, teamLogo, seatAvatar, face, teamName, seatCard, header, portraitsFor, starOf, playsTab, boxTab, drivesTab, notFound` (lines ~39–336) into `crates/reborn-ui/ui/live-game-render.js`, each with `export`, carrying the imports they use (`el` from `./shell.js`, `inkFor` from `./live-color.js`, `assetUrl, avatarSrc, sideOfSeats` from `./live-common.js`, and the module constants `DASH, BIG_YARDS, HEX_RE, NOT_FOUND` they reference). In `live.js` replace them with one import line: `import { newestGame, header, playsTab, boxTab, drivesTab, notFound } from './live-game-render.js';` (import exactly the names `draw()`/`poll()` still call — check with `grep -n` after the move). No behaviour change. Verify: `node --check crates/reborn-ui/ui/live.js crates/reborn-ui/ui/live-game-render.js`, and `cargo test -p reborn-ui` (source scans still pass).

- [ ] **Step 6: Create `game.html` and `game.js`** (the native game view)

`game.html`: copy `live.html`, change the title to `Game | NFL Street Reborn`, the script to `game.js`, keep the same `#screen`, tabs and footer. Read `live.html` first and keep its structure exactly; only the title and script differ.

`game.js`:

```js
// The native game view: one finished game from the server through the `stats_game` command (Rust does the
// request, with the player's token, so a player's own offline game opens). Opened from the Stats tab as
// game.html?game=<id>. Drawn by the same renderers as the live view (live-game-render.js).
import { bridge, call, el, quietCall, reason } from './shell.js';
import { LIVE_ORIGIN } from './live-common.js';
import { header, playsTab, boxTab, drivesTab, notFound } from './live-game-render.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const TOTALS_ONLY = 'Totals only for this game. It was played before play-by-play existed (or its plays could not be checked), and older totals may be inaccurate.';
const params = new URLSearchParams(location.search);
const gameId = (params.get('game') || '').toLowerCase();
const screen = document.getElementById('screen');
const state = { snap: null, error: null, tab: 'plays', missing: !UUID_RE.test(gameId) };

/** The snapshot shape the live renderers read, from one finished game (mirrors the backend's game-page.js). */
function snapshotOf(record) {
  const g = record.game, plays = record.plays || [];
  const fromPlays = plays.find((p) => Array.isArray(p.body?.teams) && p.body.teams.length === 2)?.body.teams;
  const teams = [0, 1].map((side) => ({ side, tgid: fromPlays?.[side]?.tgid ?? null, tmc: fromPlays?.[side]?.tmc ?? null,
    name: g.teams?.[side]?.name ?? `Side ${side + 1}`, abbrev: g.teams?.[side]?.abbrev ?? null }));
  const score = [0, 1].map((i) => (Number.isFinite(g.teams?.[i]?.score) ? g.teams[i].score : 0));
  return { match: { status: 'complete', best_of: 1 }, seats: [],
    games: [{ game_seq: 1, game_id: g.id, score, teams, sides: [], plays, box: { provisional: false, lines: record.lines || [] }, drives: record.drives || [] }] };
}

function draw() {
  if (state.missing || (!state.snap && state.error)) { screen.replaceChildren(notFound()); if (state.error) screen.append(el('p', { class: 'note', text: state.error })); return; }
  if (!state.snap) { screen.replaceChildren(el('p', { class: 'loading', text: 'Loading the game…' })); return; }
  const game = state.snap.games[0];
  const verified = state.snap.games[0].box.lines.length > 0 || game.plays.length > 0;
  const parts = [header(state.snap, game)];
  if (!verified) parts.push(el('p', { class: 'note', text: TOTALS_ONLY }));
  else parts.push(state.tab === 'plays' ? playsTab(state.snap, game) : state.tab === 'box' ? boxTab(state.snap, game) : drivesTab(state.snap, game));
  screen.replaceChildren(...parts);
}

async function load() {
  if (state.missing || !bridge) { draw(); return; }
  try {
    const record = await quietCall('stats_game', { gameId });
    if (!record) { state.missing = true; } else { state.snap = snapshotOf(record); state.error = null; }
  } catch (e) { state.error = reason(e); }
  draw();
}

document.querySelectorAll('[data-tab]').forEach((b) => b.addEventListener('click', () => { state.tab = b.dataset.tab; draw(); }));
document.getElementById('open-web')?.addEventListener('click', () => { call('open_website', { url: `${LIVE_ORIGIN}/games/${gameId}` }).catch((e) => { state.error = reason(e); draw(); }); });
load();
```

Read `live.js`'s `draw()` and its tab wiring (`[data-tab]` buttons, the "Watch on web" button id) and match `game.html`'s ids to them; if `header()` needs `snap.seats` entries for names, `seats: []` is what the live view already tolerates for CPU sides (confirm in `seatCard`).

- [ ] **Step 7: Rewrite `stats.js`**

Replace the file's content with the three-state page. Keep the imports the shell provides (`call, el, markCurrent, partial, reason, refusal, wireCoreFold` — drop what the new page does not use):

```js
// The Stats tab: the signed-in player's games from the server (stats_my_games), each opening the native game
// view (game.html?game=<id>). The local results queue is only counted ("N games not uploaded yet").
import { call, el, reason, wireCoreFold } from './shell.js';
import { rowModel, queueLine } from './stats-rows.js';
import { assetUrl } from './live-common.js';

const PAGE_SIZE = 6;
const SIGNED_OUT = 'Sign in with Discord on the Play page to see your games.';
const EMPTY = 'No games yet. Play one and it shows up here.';
const state = { signedIn: false, games: [], queue: [], filter: 'all', page: 0, error: null, loaded: false };
const screen = document.getElementById('screen');
let generation = 0;

async function load() {
  const mine = ++generation;
  const [games, queue] = await Promise.allSettled([call('stats_my_games', { limit: 200 }), call('stats_queue')]);
  if (mine !== generation) return; // a later load answered first (sign-out mid-request): keep the newest answer
  if (games.status === 'fulfilled') { state.signedIn = !!games.value.signed_in; state.games = games.value.games || []; state.error = null; }
  else { state.error = reason(games.reason); }
  state.queue = queue.status === 'fulfilled' ? (queue.value?.entries || []) : [];
  state.loaded = true;
  draw();
}

function teamCell(t, score, mine, side) {
  const cell = el('span', { class: `stats-team${mine === side ? ' stats-team-mine' : ''}` });
  const logo = t.tgid !== null ? el('img', { class: 'stats-logo', alt: '', src: assetUrl(`teams/${t.tgid}-64.png`) }) : el('span', { class: 'stats-abbrev', text: t.abbrev || t.name.slice(0, 3).toUpperCase() });
  if (logo.tagName === 'IMG') logo.addEventListener('error', () => logo.replaceWith(el('span', { class: 'stats-abbrev', text: t.abbrev || t.name.slice(0, 3).toUpperCase() })));
  cell.append(logo, el('span', { class: 'stats-team-name', text: t.name }), el('strong', { class: 'stats-team-score', text: score === null ? '–' : String(score) }));
  return cell;
}

function row(game) {
  const r = rowModel(game);
  const btn = el('button', { type: 'button', class: 'stats-game', 'aria-label': `${r.home.name} ${r.score ? r.score[0] : '–'}, ${r.away.name} ${r.score ? r.score[1] : '–'}. ${r.date}, ${r.mode}, ${r.badge}.`,
    onclick: () => { location.href = `game.html?game=${encodeURIComponent(r.id)}`; } });
  btn.append(
    el('span', { class: 'stats-game-date' }, el('span', { text: r.date }), el('small', { text: r.time })),
    el('span', { class: 'stats-matchup' }, teamCell(r.home, r.score ? r.score[0] : null, r.mine, 0), teamCell(r.away, r.score ? r.score[1] : null, r.mine, 1)),
    el('span', { class: `stats-result stats-result-${(r.result || 'none').toLowerCase()}`, text: r.result || '' }),
    el('span', { class: 'stats-game-mode', text: r.mode }),
    el('span', { class: `stats-badge stats-badge-${r.badge.replace(/\s+/g, '-').toLowerCase()}`, text: r.badge }),
  );
  return btn;
}

function filtered() {
  return state.games.filter((g) => state.filter === 'all' || (state.filter === 'online' ? g.mode === 'online' : g.mode !== 'online'));
}

function draw() {
  if (!state.loaded) { screen.replaceChildren(el('p', { class: 'loading', text: 'Loading…' })); return; }
  const parts = [];
  const q = queueLine(state.queue);
  if (!state.signedIn) {
    parts.push(el('p', { class: 'note', text: SIGNED_OUT }));
    if (q) parts.push(el('p', { class: 'note', text: `${q.replace(' not uploaded yet', '')} will upload when you sign in.` }));
    screen.replaceChildren(...parts); return;
  }
  if (state.error) parts.push(el('p', { class: 'refusal', role: 'alert', text: state.error }));
  if (q) parts.push(el('p', { class: 'note', text: q }));
  parts.push(el('div', { class: 'stats-filters', role: 'group', 'aria-label': 'Show' }, ...[['all', 'All'], ['online', 'Online'], ['offline', 'Offline']].map(([k, label]) =>
    el('button', { type: 'button', class: `stats-filter${state.filter === k ? ' current' : ''}`, 'aria-pressed': String(state.filter === k), text: label, onclick: () => { state.filter = k; state.page = 0; draw(); } }))));
  const list = filtered();
  if (!list.length) { parts.push(el('p', { class: 'note', text: state.games.length ? 'No games of that kind yet.' : EMPTY })); screen.replaceChildren(...parts); return; }
  const pages = Math.max(1, Math.ceil(list.length / PAGE_SIZE));
  state.page = Math.min(state.page, pages - 1);
  parts.push(el('h2', { text: 'Your games' }));
  parts.push(el('div', { class: 'stats-list' }, ...list.slice(state.page * PAGE_SIZE, (state.page + 1) * PAGE_SIZE).map(row)));
  if (pages > 1) parts.push(el('div', { class: 'stats-pager' },
    el('button', { type: 'button', text: 'Newer', disabled: state.page === 0, onclick: () => { state.page--; draw(); } }),
    el('span', { text: `Page ${state.page + 1} of ${pages}` }),
    el('button', { type: 'button', text: 'Older', disabled: state.page >= pages - 1, onclick: () => { state.page++; draw(); } })));
  screen.replaceChildren(...parts);
}

wireCoreFold?.();
draw();
load();
setInterval(load, 30000);
```

Check the `el()` signature in `shell.js` (attributes object with `text`, `class`, `onclick`; `disabled`) and `assetUrl`'s argument form in `live-common.js` (it may want `teams/<tgid>-64.png` or a full asset path — copy what `live.js`'s `teamLogo` passes). Remove the old game dialog, search box and mode inference entirely. Update `stats.css` for `.stats-list/.stats-game/.stats-team/.stats-logo/.stats-abbrev/.stats-result-w|l|t/.stats-badge-*/.stats-filters/.stats-pager` (dark, compact rows; a 24 px logo; W green, L red, T grey; badges as small pills).

- [ ] **Step 8: Verify**

```bash
node --check crates/reborn-ui/ui/stats.js crates/reborn-ui/ui/game.js crates/reborn-ui/ui/live.js crates/reborn-ui/ui/live-game-render.js
node --test crates/reborn-ui/ui/stats-rows.test.mjs
for f in crates/reborn-ui/ui/stats.js crates/reborn-ui/ui/stats.css crates/reborn-ui/ui/game.js crates/reborn-ui/ui/game.html crates/reborn-ui/ui/live-game-render.js crates/reborn-ui/ui/live.js crates/reborn-ui/ui/stats-rows.js; do grep -c $'\r' "$f"; done   # all 0
cargo test -p reborn-ui
```

Then run the launcher from the worktree (`cargo run -p reborn-ui` or the dev command the repo's README names) signed in as Adam, open Stats: rows appear for tonight's games with logos, scores, L/W, Verified; a row opens `game.html` with the header and three tabs; the Live view still works (Online → Live now → Watch).

- [ ] **Step 9: Commit**

```bash
git add crates/reborn-ui/ui/stats.js crates/reborn-ui/ui/stats.css crates/reborn-ui/ui/stats-rows.js crates/reborn-ui/ui/stats-rows.test.mjs crates/reborn-ui/ui/game.html crates/reborn-ui/ui/game.js crates/reborn-ui/ui/live-game-render.js crates/reborn-ui/ui/live.js
git commit -m "stats: the Stats tab is the player's games from the server, each opening a native game view"
```

### Task 7: Beta build, and Adam's offline confirmation

**Files:** none new. Uses `installer/windows/build.ps1` and the scratch publish scripts from this session (`publish_beta_3027b12.ps1` pattern: change the version).

- [ ] **Step 1: Full launcher suite** — `cargo test --workspace --no-fail-fast` → every crate `ok`.
- [ ] **Step 2: Build** — `powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/build.ps1 -Channel beta -Version 3.0.27-beta.13 -LauncherOnly` → `BUILD EXIT 0`, `dist\latest-beta.json` version 3.0.27-beta.13; `grep -a -c "3.0.27-beta.13" target/release/reborn-launcher.exe` = 1.
- [ ] **Step 3: Publish** with the beta publish script pattern (exe first, read back through `?v=`, then `latest-beta.json`), only after Adam says go.
- [ ] **Step 4: Adam's confirmation (the one live test for "stats everywhere")**: Adam plays one offline quick game on the beta launcher, signed in. Then:
  - `GET https://api.nflstreetreborn.com/v1/players/0040ba1e-ee6f-47ea-bb8e-b2104247f36a/games?limit=3` (with his token, or via the launcher's Stats tab) shows the game with `mode: "offline"`, `plays_status: "verified"`, `team_index` = the side his pad played, `result` set.
  - The Stats tab lists it with the Verified badge; opening it shows plays, box score and drives.
  - Record the game id and the outcome in memory (`project_reborn_stats_everywhere_confirmation`).
