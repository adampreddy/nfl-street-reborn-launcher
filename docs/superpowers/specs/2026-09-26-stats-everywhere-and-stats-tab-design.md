# Stats everywhere, and a Stats tab that reads them — design

Date: 2026-09-26. Approved in conversation by Adam. Repos: the launcher (this repo) and the backend
(`Reborn Backend`, branch `feat/play-by-play`).

## Purpose

Every game anyone plays through the launcher — offline, a private room, matchmaking — ends up with the
full play-by-play, box score and drives on a permanent game page, and the launcher's Stats tab shows the
signed-in player's games from that record instead of the old local capture `.json` files.

What Adam said: "We need our very cool stats to be done for offline games and private netplays";
"The Stats UI is bad too and also needs to use our new stats instead of the old .jsons"; stats live on
the server, tied to the signed-in player; in an offline game the player's controller port is their side;
the Stats tab opens on the player's games; old capture-only games stay visible but are marked as
totals only, and "old stats may be incorrect".

## What already exists (measured 2026-09-26)

- The Dolphin fork's capture writer embeds the game's play lines as `plays: [...]` in every capture
  (`NflsStatTracker.cpp`, runtime 1.2.8 and later), for any game, online or offline.
- The launcher's results queue (`reborn_core::stats::StatsQueue::ingest_all`) collects every capture
  from the session's `GameStats` folder and uploads it to `POST /v1/results` when the player is signed in;
  a game played signed out waits in the queue.
- The server ingests `plays` from a capture (`src/pbp/ingest.js`): one copy is graded R1–R3 and stored as
  `plays_status = 'verified'` when they pass (`'failed'` otherwise, `'no-human'` for CPU-only games,
  `'absent'` when the capture carries no plays); a second copy of the same game is compared by hash (R4).
  Tonight's matchmaking games are `verified` with R1–R4 passing.
- The server attributes a game to the signed-in player by the lowest local pad (`reborn_match.local_pads[0]`
  → `participation.team_index`), which is exactly "your port is your side" for offline games.
- `GET /v1/games/:id`, `/plays`, `/boxscore`, `/drives` serve a game's record; `/boxscore` and `/drives`
  answer only from verified plays.
- The live match page (`/live/:matchId`, `src/web/live-page.js` + `live-render.js`) renders plays, box
  score and drives, and the launcher hosts it (Live now → Watch).

So "stats everywhere" needs no new plumbing at game time. One confirmation test is owed: an offline game
played on the beta runtime must land on the server as `verified` (see Testing).

## Not in scope

- A live page for offline games while they are played (approach 2 in the brainstorm).
- Career totals, leaderboards, and any change to the league site. A career strip on the Stats tab is a
  follow-up once games accumulate.
- The Online tab redesign (its own brainstorm).
- Changing how the old capture-only games were graded or attributed.

## 1. Data flow

1. A game ends. Dolphin writes the capture, plays included, into the session's `GameStats` folder.
2. The launcher's results queue picks the capture up at session end (today's `collect`), and uploads it
   with the player's token. Signed out: it waits; it uploads on the next sign-in.
3. The server records the game, grades the plays, and attributes the player's side from the lowest local
   pad (or the match seat for matchmaking). In a private room both launchers upload; the second copy is
   compared by hash (R4) and disagreement withdraws the box score, as it does for matchmaking today.
4. The Stats tab reads `GET /v1/players/:id/games` for the signed-in player, and a row opens the game's
   permanent page `GET /games/:id`, hosted inside the launcher.

Nothing streams during an offline game. Matchmaking keeps streaming live exactly as today; the capture's
plays are the second copy the server already reconciles.

## 2. Server

### 2a. `GET /v1/players/:id/games` grows three fields per game

- `score`: `[home, away]` from the capture's team totals, in the capture's team order (the order `teams`
  is in). `null` when the capture has no totals.
- `plays_status`: the game's `plays_status` as stored (`verified`, `failed`, `no-human`, `absent`).
- `result`: from the viewer's own side when the viewer is the requested player and the game has a
  `team_index` for them: `"win"`, `"loss"` or `"tie"`; otherwise `null`. A game with `status` other than
  `accepted` (held, void) has `result: null` regardless.

Everything already returned stays as it is. The visibility rules (`show_offline`, hidden players) do not
change: this route already decides what a viewer may see, and it keeps deciding.

### 2b. `GET /games/:id` — the permanent game page

An HTML page under `webRouter.js`, the same CSP, asset versioning and 404 page as `/live/:matchId`. It
renders one finished game with the live page's renderer (`live-render.js`): header with both teams (logos,
colours, wordmarks from the live assets), final score, then the Plays / Box score / Drives tabs.

- `plays_status = verified`: plays from `/v1/games/:id/plays`, box score from `/boxscore`, drives from
  `/drives`. The header says "Final".
- `plays_status = absent` (an old capture) or `failed` or `no-human`: the header and the team totals from
  `/v1/games/:id` only, one note in place of the tabs: "Totals only for this game. It was played before
  play-by-play existed (or its plays could not be checked), and older totals may be inaccurate."
- A game the viewer may not see (hidden offline game of another player, void game, unknown id): the same
  404 page as an unknown match.

The page needs no sign-in for a public game; the launcher hosts it with the viewer's token so the
player's own offline games (hidden from the public by `show_offline`) open for them. No new JSON route:
team identity for logos and colours comes from the play lines' own `teams` field (`tgid`, `tmc`, `tlgl`),
exactly as the live page derives it, and names and totals come from `/v1/games/:id`; a totals-only game
has no play lines, so its header uses the capture's team names and the abbreviation badge in place of a
logo.

### 2c. `/live` links finished games

On the `/live` index and the live match page, a recorded game (`game_id` set) links to `/games/<id>`.

## 3. Stats tab (launcher)

`crates/reborn-ui/ui/stats.js` + `stats.html` + `stats.css` are rewritten around three states:

- **Signed out:** one sentence — "Sign in with Discord on the Play page to see your games." — and the
  count of captures waiting in the queue, if any ("3 games will upload when you sign in.").
- **Signed in, no games:** "No games yet. Play one and it shows up here."
- **Signed in, games:** **Your games**, newest first, paged (the existing 6-per-page pattern is kept), with
  a filter row All / Online / Offline. One row per game: date and time; both teams with logos (the live
  assets), the score with the player's team first; W / L / T from `result`; mode; a badge from
  `plays_status`: **Verified** (`verified`), **Totals only** (`absent`, `failed`, `no-human`); held games
  read "Held for review". A row is a button that opens the game page.
- Above the list, when the results queue holds captures not yet uploaded: "N games not uploaded yet" with
  the queue's own reason when it has one (its existing wording; unchanged).

Opening a game: the launcher hosts `/games/<id>` the way Live now hosts `/live/<matchId>` (same native
view, same back control, same token handling). The host view's title is "<Team> vs <Team> · <date>".

Removed: the local-capture list, the "capture folder → mode" inference ("Unknown" mode), and the game
dialog that rendered the old `.json` fields. The queue status commands (`stats_queue`) stay, because the
"not uploaded yet" line and the signed-out count use them.

New launcher commands (Tauri): `stats_my_games({ limit, before, filter })` → the signed-in player's games
from 2a (the launcher knows its own player id from `identity_status`), and `stats_game_view(id)` → opens
the hosted game page. Both refuse with the existing player-facing error sentences when signed out or
offline.

## 4. Errors

- Server unreachable / rate limited: the list keeps its last good rows and shows one line above them, the
  same pattern as Live now (`live-now.js`). The line's words are the backend's own player sentence.
- A game whose plays failed grading is a row like any other, badged "Totals only"; opening it shows the
  totals page from 2b, never an empty tab.
- A capture that cannot upload keeps the queue's existing message; the Stats tab only counts it.
- A game page for an id the viewer may not see is the 404 page; the launcher's host view shows it as-is.

## 5. Testing

Backend (node:test, the existing harnesses):
- `players.test.js`: the three new fields — `score` from totals, `null` without; `result` win / loss / tie
  from the viewer's side, `null` for a non-viewer and for a held game.
- `live_index.test.js` / a new `game_page.test.js`: `/games/:id` 200 for an accepted game with the
  versioned assets; the totals-only note for `absent` and `failed`; 404 for unknown, void, and a hidden
  offline game of another player.
- `/live` index: a finished game links to `/games/<id>`.

Launcher:
- `stats.js` states with the bridge double used by the other pages: signed out (+ queue count), empty,
  rows (badge per `plays_status`, result letter, score order), error line over last good rows.
- The source-scanning tests that pin command registration get the two new commands.

End to end (once, by Adam): one offline quick game on the beta runtime, then `GET /v1/games/<id>` shows
`plays_status: verified` and `participation.team_index` matches his pad; then the same game opens from
the Stats tab. This is the confirmation that "stats everywhere" holds for offline play.

## Sequencing

1. Server 2a + 2b + 2c with tests; deploy.
2. Launcher Stats tab + commands with tests; beta launcher build.
3. Adam's offline-game confirmation.
