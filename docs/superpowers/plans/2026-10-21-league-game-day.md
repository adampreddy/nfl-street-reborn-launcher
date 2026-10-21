# League Game Day Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Play `<opponent>` keeps you on the League tab. The tab shows both lineups with portraits and season lines, the tale of the tape, and the launcher's room card and chat, then the match state.

**Architecture:**
- **Site:** `team_home` grows season totals, team totals and the opponent's lineup, plus a token-only `portrait` action.
- **Backend:** passes the fields through and proxies portraits by `poid`, exactly like team logos.
- **Launcher:**
  - a `league_portrait` command (twin of `league_logo`);
  - a pure model for lines and the tape;
  - the room card and chat split out of `online-mm.js` into a shared module;
  - a game-day view inside `league.js`, driven by the same `mm_state` snapshot Online uses.

**Tech Stack:** PHP 8 (league site, MySQL), Node/Express plus `node:test` (backend, Postgres tests), Rust/Tauri plus plain ES modules (launcher), Playwright in Edge (page checks).

**Spec:** `docs/superpowers/specs/2026-10-21-league-game-day-design.md` (mockup `.superpowers/brainstorm/81379-1792559195/content/gameday-v4.html`)

## Global Constraints

- **Copy, word for word:**
  - `‹ League`;
  - `Week N · <away> at <home>`;
  - `Tale of the tape`, with `Season` as its label;
  - `No games yet`;
  - `No stats kept` (OL only);
  - `Your room`, with `League rules · Best of 3`;
  - `Copy code`, `Copy invite link`, `Start`, `I'm ready`, `Ready`, `Leave room`, `you`, `Not ready`;
  - `Lineup not available`;
  - `League site unreachable, showing saved data`;
  - `Starting…`, `Game N of 3`, `Final`.
- **Tape rows in order:** Points, Points allowed, Total yards, Passing yards, Rushing yards, Turnovers, Sacks, Defensive TDs. Lower is better for Points allowed and Turnovers.
- **Stat line:** season totals. Only the first group the player has (passing, then rushing, then receiving, then defence). Only non-zero parts, joined with ` · `.
  - passing `X yds · N TD · N INT`;
  - rushing `X yds · N TD`;
  - receiving `X yds · N TD`;
  - defence `N sk · N INT · N tkl`.
- **Week 1** (neither team has `team_season.gp > 0`): every stat line is blank, and the tape shows only the records and `No games yet`.
- **No Hold up in the game-day view.** After `I'm ready`, the button is a green `Ready` with a drawn SVG check and sends nothing. Online keeps its own behaviour for other rooms.
- **Only fixture rooms** (`room.league.fixture_id` set) get the view. Friendlies stay on Online.
- **Styling:**
  - every colour is a `theme.css` token, except team colours from `team.color`, which are used only for that team's wash, portrait backdrop and tape bar;
  - Inter only;
  - the launcher's `.btn` / `.card` / glass classes.
- **Layout:** 1280x800 with no page scroll. 800x560 stacks and the page scrolls. Never a sideways scroll.
- **Source files:** LF line endings (`.rs`, `.js`, `.css`, `.md`). The launcher builds with `CARGO_BUILD_JOBS=1`.
- **Never:**
  - upload site files live, or deploy the backend, without Adam's OK;
  - touch the production backend env;
  - commit the TEST key in `signing.rs`.

## Review Focus

1. **A player in the roster with no stat rows at all** (a benchwarmer, a draftee in Week 3): blank line, never `0 yds`. Test in Task 4 (`statLine` of an all-zero season returns `''`).
2. **One team has played and the other has not** (a bye or forfeit in Week 2): the tape shows numbers, not `No games yet`; 0 is a real value. Test in Task 4 (`tape` with `gp` 2 vs 0 returns rows).
3. **The opponent hosts and you join after the room exists**, or the launcher restarts mid-room: the League tab must open straight into game day from `mm_state` alone. Test in Task 7 (page state `league-gameday-rejoin`).
4. **The portrait proxy asked for a poid that never appeared in a roster:** 404, no site call. Test in Task 2.
5. **Long names** ("Foyesade Oluokun", "Derek Stingley Jr.") on the mirrored side at 1280 px: ellipsis, never a wrap or overlap. Test in Task 7 (no row taller than 48 px, no text overflow).

---

### Task 1: Site: season totals, team totals, opponent lineup, portrait action

**Files:**
- Modify: `C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php` (`nsr_th_page` roster block around lines 255-265, `nsr_team_home` matchup block around lines 336-352)
- Modify: `C:\Users\Adam\nsr_local\public_html\league\api\bot.php` (new `portrait` action beside `team_home` at line ~879)
- Test: `C:\Users\Adam\nsr_local\tests\team_home_test.php`, `C:\Users\Adam\nsr_local\tests\th_fixture.php`

The site is not under git. Back up the two files to `nsr_local\stage_league_tab\pre_game_day\` before editing.

**Interfaces:**
- **Produces, every roster entry** (own `roster[]` and `matchup.opponent.roster[]`):
  - `{poid, name, position, number, has_portrait: bool, season: {gp, pass_yds, pass_td, thrown_int, rush_yds, rush_td, rec_yds, rec_td, tackle, sack, int, def_td}}`;
  - every `season` key is an int; 0 when absent.
- **Produces** `team_season` and `matchup.opponent.team_season`: `{gp, pts, pts_allowed, yds, pass_yds, rush_yds, turnovers, sacks, def_td}`, all ints.
- **Produces** a `bot.php` action `portrait`: POST `{action:'portrait', poid, league_id?}` returns `{ok:true, png:'<base64>'}` or `fail('no portrait', 404)`.

Steps:

- [ ] **Step 1: Failing tests** in `team_home_test.php`, using the existing fixture style. Seed `th_fixture.php`:
  - stat_events for the own team, two completed series;
  - a `league_players` row (`source='rdatabase'`) with a `player_portrait` PNG for one poid.

  Assert:
  - `$h['roster'][0]['season']['pass_yds'] === <seeded sum>`, and an unseeded player's `season` is all zeros;
  - `$h['team_season']['pts'] === <sum of own points over complete series>`, `pts_allowed` likewise, `turnovers === thrown_int + fumble_lost`;
  - `$h['matchup']['opponent']['roster']` has the opponent's 7, in QB/RB/WR/OL/DL/LB/DB order;
  - `has_portrait` is true only for the seeded poid;
  - the `portrait` action over the HTTP harness (TH_URL :8799), with the bearer, returns base64 that decodes to the seeded PNG;
  - a poid not in the league is 404;
  - no bearer is 401.
- [ ] **Step 2: Run, expect FAIL.** `php C:\Users\Adam\nsr_local\tests\team_home_test.php` names the new checks.
- [ ] **Step 3: Implement in `nsr_team_home.php`:**
  - `nsr_th_lineup(PDO $pdo, int $lid, int $teamId, array $names, array $pos, array $num, array $statRows, array $hasPortrait): array`, the existing roster loop moved here, adding `season` from `nsr_player_stat_rows($pdo,$lid)[$poid]` and `has_portrait`;
  - `nsr_th_team_season(int $teamId, array $teamRows, array $series): array`:
    - `gp`, `yds = pass_yds + rush_yds`, `sacks = sack`, `turnovers = thrown_int + fumble_lost` and `def_td` from `nsr_team_stat_rows($pdo,$lid)[$teamId]`;
    - `pts` / `pts_allowed` summed from `nsr_series($pdo,$lid)` rows where `complete` and the team is home or away (`home_points` / `away_points`);
  - `nsr_th_portrait_poids(PDO $pdo, int $lid): array` returns `[poid => true]` from `league_players lp JOIN player_portrait pp ON pp.player_id = lp.source_ref WHERE lp.league_id=? AND lp.source='rdatabase' AND lp.active=1 AND pp.source <> ''`. Wrap it in `try` and return `[]` when the tables are missing.

  Call `nsr_th_lineup` for the own team in `nsr_th_page`, and for the opponent in the matchup block. Add the `team_season` keys.
- [ ] **Step 4: Implement the `portrait` action in `bot.php`.** It uses the same bearer check as `team_home`, and `$leagueId` the same way (`league_id` or current). Look up `player_portrait.source` through the same JOIN for that poid, then `out(['ok'=>true,'png'=>base64_encode($png)])`. Otherwise `fail('no portrait', 404)`. Only rows with `pp.source` starting with the PNG magic are served.
- [ ] **Step 5: Run, expect `ALL PASS`.** `php C:\Users\Adam\nsr_local\tests\team_home_test.php`, plus `php -l` on both files.
- [ ] **Step 6: Record.** Write `nsr_local\stage_league_tab\pre_game_day\CHANGES.txt`, listing the two files and their sha256 for the later live upload. The upload itself waits for Adam's OK.

### Task 2: Backend: pass-through and portrait proxy

**Files** (`C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend`, on a branch `feat/league-game-day` off `main`):
- Modify: `src/league/merge.js`, `src/league/site.js`, `src/league/router.js`
- Test: `test/league_merge.test.js`, new `test/league_portrait.test.js`

**Interfaces:**
- **Consumes:** Task 1 shapes.
- **Produces, in `/v1/league/me`:** each roster entry gains `portrait_url: string|null` = `${publicOrigin}/live-assets/league/portrait/<poid>.png` when `has_portrait`, else null. `season` and `team_season` are passed through unchanged.
- **Produces** `site.portraitAllowed(poid): boolean` (poids seen in any team_home roster) and `site.portrait(poid): Promise<Buffer|null>` (POST `action:'portrait'`, base64-decoded, SiteUnavailable on 5xx or timeout).
- **Produces** route `GET /live-assets/league/portrait/:file`, where `:file` is `<poid>.png`.

Steps:

- [ ] **Step 1: Failing tests.**
  - In `league_merge.test.js`:
    - `mergeLeague` keeps `season` / `team_season` deep-equal;
    - it sets `portrait_url` to the backend URL for `has_portrait:true`, and null otherwise, for both rosters.
  - In `league_portrait.test.js`, mirroring `league_logo.test.js`:
    - a poid seen in a roster returns `200 image/png` with the decoded bytes;
    - a second request within the TTL makes no second site call;
    - an unseen poid is 404 with zero site calls;
    - a non-PNG answer is 404;
    - a body over `LOGO_MAX_BYTES` is 404;
    - the rate limit gives 429 past `logoPerMinute`.
- [ ] **Step 2: Run, expect FAIL.** `npm test -- test/league_portrait.test.js test/league_merge.test.js`.
- [ ] **Step 3: Implement.**
  - `site.js`: collect roster poids in `post()` beside `logoPairs`.
  - `merge.js`: map both rosters.
  - `router.js`: a portrait route sharing the logo route's cache, miss cache, size cap, `IMAGE_TYPES` check and `logoLimit`. The cache key is `portrait:<poid>`.
- [ ] **Step 4: Run the full suite, expect all pass with 0 fail.** `npm test` (`.env` with `DATABASE_URL_TEST`).
- [ ] **Step 5: Commit** `league: game day, season lines, team totals and portraits through the backend`.

### Task 3: Launcher: `league_portrait` command

**Files:**
- Modify: `crates/reborn-ui/src/league_tab.rs` (around `league_logo`, line 176)
- Modify: `crates/reborn-ui/src/main.rs:6753` (register the command)
- Modify: the bridge command allow-list, if one exists (`grep -rn "league_logo" crates/reborn-ui/ui Tools`); add `league_portrait` beside every `league_logo` entry.

**Interfaces:**
- **Produces:** `#[tauri::command] pub async fn league_portrait(poid: u64) -> Option<String>`, which returns a `data:image/png;base64,` URL or None.
- **Shared:** `fn cached_png(dir: &str, name: &str, path: &str) -> Option<String>`, a private helper both commands call. `dir` is `league-logos` or `league-portraits`, `path` is `/live-assets/league/...`, and it uses the same `LOGO_MAX_AGE` (1 day), `LOGO_MAX_BYTES` and PNG check.

Steps:

- [ ] **Step 1: Failing unit test** `cached_png_names_and_rejects`:
  - the cache file path is `<cache_root>/league-portraits/<poid>.png`;
  - a 0-byte or non-PNG body yields None (with an injected fetch).

  Keep the injection minimal: split out `fn cached_png_with(fetch: impl Fn(&str, &Path) -> Result<u64>, ...)`.
- [ ] **Step 2: Run, expect FAIL.** `cargo test -p reborn-ui league_tab` (`CARGO_BUILD_JOBS=1`).
- [ ] **Step 3: Implement.** Move `league_logo`'s body into `cached_png`, with `league_logo` and `league_portrait` both one-liners over it.
- [ ] **Step 4: Run, expect PASS.** `cargo test -p reborn-ui league_tab`, and confirm `file crates/reborn-ui/src/league_tab.rs` reports no CRLF.
- [ ] **Step 5: Commit** `league: league_portrait, the logo cache shared`.

### Task 4: Launcher: game-day model

**Files:**
- Create: `crates/reborn-ui/ui/league-gameday-model.js`
- Test: `crates/reborn-ui/ui/league-gameday.test.mjs` (run like `ui/league-layout.test.mjs`)

**Interfaces:**
- **Produces:**
  - `statLine(season, position)` returns `string` (`''`, `'No stats kept'`, or the line);
  - `lineup(roster)` returns `[{poid, name, pos, number, portraitUrl, line}]`, ordered;
  - `tape(mySeason, oppSeason)` returns `null` (Week 1) or `[{label, mine, theirs, mineBetter, theirsBetter, mineWidth, theirsWidth}]`;
  - `gameDay(home, snap)` returns `null` when the room is not this fixture, else `{week, title, me:{id,name,owner,record,color,lineup}, opp:{...}|null, tape, oppMissing:boolean, stale:boolean, room, match}`.
  - The `title` is `Week N · <away> at <home>`, built from `home.matchup.home`.
- **Consumes:** `home` (`/v1/league/me` with the Task 2 fields), and `snap` (the `mm_state.snapshot`).

Steps:

- [ ] **Step 1: Failing tests:**
  - `statLine({pass_yds:482,pass_td:5,thrown_int:1,rush_yds:30}, 'QB') === '482 yds · 5 TD · 1 INT'`;
  - `statLine({rush_yds:242,rush_td:3},'RB') === '242 yds · 3 TD'`;
  - `statLine({sack:3,tackle:8},'DL') === '3 sk · 8 tkl'`;
  - `statLine({int:1,tackle:10},'DB') === '1 INT · 10 tkl'`;
  - `statLine(zeros,'OL') === 'No stats kept'`;
  - `statLine(zeros,'DB') === ''`;
  - `lineup` orders `[DB,QB,OL,WR,RB,LB,DL]` as QB, RB, WR, OL, DL, LB, DB (ties by name);
  - `tape({gp:0,...},{gp:0,...}) === null`;
  - `tape({gp:2,...},{gp:0,...})` has 8 rows;
  - the row labels equal the Global Constraints list;
  - Points allowed 45 vs 48 gives `mineBetter === true`;
  - Turnovers 3 vs 5 gives `mineBetter === true`;
  - Points 63 vs 54 gives `mineBetter === true` with `mineWidth === 100`;
  - a tie gives neither better;
  - `gameDay` is null when `snap.room.league?.fixture_id !== home.matchup.fixture_id`;
  - `oppMissing` is true when `home.matchup.opponent.roster` is absent;
  - Week 1 gives a blank line for every player.
- [ ] **Step 2: Run, expect FAIL.** `node --test crates/reborn-ui/ui/league-gameday.test.mjs`.
- [ ] **Step 3: Implement**, with pure functions, no DOM and no imports beyond plain JS.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit** `league: game day model (lines, lineup order, tale of the tape)`.

### Task 5: Shared room card and chat

**Files:**
- Create: `crates/reborn-ui/ui/room-card.js`
- Modify: `crates/reborn-ui/ui/online-mm.js` (`chatBox` 269-325, `settleChat` 326-340, `drawRoom` 624-693 move out and are imported back)

**Interfaces:**
- **Produces:**
  - `export function chatBox(key, messages, send)` and `export function settleChat(root)`, moved verbatim; they take the root to search instead of the module-level `section`;
  - `export function drawRoomCard(r, chat, { act, title, sub, holdUp = true, kindControl = null })`, which returns `[roomSection, chatSection]`:
    - `holdUp:false` renders the ready button as `class="btn done"`, an SVG check plus `Ready`, with no onclick once ready;
    - the footer row holds Start, ready and Leave room plus the note, as in commit 351b084.
- `online-mm.js`'s `drawRoom(r, chat)` becomes the kind switch plus `drawRoomCard(...)` with `holdUp:true`, wrapped in `div.on-room` exactly as now.

Steps:

- [ ] **Step 1: Run the baseline.** `node Tools/check-online-page.mjs` gives `PASS (69 state(s))`, and `node Tools/check-chat-scroll.mjs` passes.
- [ ] **Step 2: Move the code.** Online's DOM must stay identical, with the same classes, `data-key`s and text.
- [ ] **Step 3: Re-run both checks.** Expect the same PASS lines. `node Tools/shot-room.mjs` screenshots match the 351b084 look by eye.
- [ ] **Step 4: Commit** `online: the room card and chat are a shared module`.

### Task 6: League tab: the game-day view

**Files:**
- Create: `crates/reborn-ui/ui/league-gameday.js` (render only)
- Modify: `crates/reborn-ui/ui/league.js` (`play` 172, `join` 178, `view` 317, the poller), `crates/reborn-ui/ui/pages/league.css`

**Interfaces:**
- **Consumes:** Task 4 `gameDay`, Task 5 `drawRoomCard` / `chatBox` / `settleChat`, Task 3 `league_portrait`, and `league.js`'s existing `logoFor`, `act` and `call`.
- **Produces:** `export function drawGameDay(model, { el, act, logoFor, portraitFor, onBack })`, which returns the view's root node (`section.gd`).

**Behaviour:**
- **`league.js`:**
  - `play()` and `join()` stop calling `go('online.html')`. After the room opens or is joined, they set `state.gameDay = true` and redraw.
  - While `state.gameDay` is set, or `mm_state.snapshot.room.league.fixture_id === home.matchup.fixture_id`, the page polls `mm_state` every 1000 ms (its own interval, cleared on leaving) and draws `drawGameDay`.
  - `‹ League` sets `state.gameDay = false`, an in-memory "shown home" flag that is cleared when the room changes, and redraws the home. The room is untouched.
  - Leave room calls `mm_room_action` leave, then the home.
- **Match:**
  - When `snap.match` belongs to this fixture (`match.league?.fixture_id`), the room row becomes `div.gd-match`, showing the series score `X-Y`.
  - The words are `Starting…` while `relay_state` is not running or intermission; `Game N of 3` while running, where N = `series[0] + series[1] + 1`; and `Final` once `relay_state === 'complete'`, followed by `Back to League`.
  - `Accept` shows when `relay_state === 'pairing' && !your_accepted`, sending `mm_match_action accept true`.
  - `Leave match` is a quiet button.
  - The held notice renders with the existing `heldNotice` text from `online-model.js`.
- **Controller:** focus order is the room footer (Start, ready, Leave room), then chat, then `‹ League`. A pad user who opens the view lands on Start (host) or the ready button (guest), the way Online lands on its primary.
- **Portraits:** asked once per poid per page visit, like `logoFor`. A missing portrait falls back to the team logo.
- **CSS:**
  - the classes `gd`, `gd-board`, `gd-side`, `gd-side.r`, `gd-pl`, `gd-face`, `gd-tape`, `gd-st`, `gd-room`, `gd-match`, with values from the mockup's `<style>`, every colour a token or `--tc`;
  - the board grid is `minmax(0,1fr) 312px minmax(0,1fr)`;
  - rows use `flex:1; min-height:46px`;
  - under 1000 px, one column: own team, opponent, tape, room, chat.

Steps:

- [ ] **Step 1: Write `league-gameday.js` and the CSS.** Then wire `league.js`.
- [ ] **Step 2: Run the existing checks.** `node --test crates/reborn-ui/ui/*.test.mjs`, `node Tools/check-league-page.mjs`, `node Tools/check-theme-tokens.mjs`, `node Tools/check-shell-pages.mjs` (or whichever names exist: `ls Tools/check-*league* Tools/check-shell*`). All PASS; update `check-league-page`'s expectation that Play navigates to `online.html`.
- [ ] **Step 3: Commit** `league: game day, the league game room stays on the League tab`.

### Task 7: Page states and the game-day check

**Files:**
- Modify: `Tools/lib/page-states.mjs` (league states, beside `league-*` around line 281)
- Create: `Tools/check-league-gameday.mjs`, `Tools/fixtures/league-gameday-s4.json` (home plus snapshot, fake Discord ids, the real S4 names from the mockup, a `has_portrait` mix)

**Interfaces:** consumes Tasks 4 and 6.

**States:**
- `league-gameday-week1`;
- `league-gameday-season` (gp 2, sample totals from the mockup);
- `league-gameday-ready` (both ready, host);
- `league-gameday-guest`;
- `league-gameday-rejoin` (page load with the room already open, no click);
- `league-gameday-opp-missing`;
- `league-gameday-stale`;
- `league-gameday-match` (running, series 1-0);
- `league-gameday-final`;
- `league-gameday-small` (800x560).

**The check asserts, per state:**
- no page error;
- no sideways scroll;
- at 1280x800 `#main` does not scroll vertically;
- no element with text `Hold up`;
- Start is disabled unless both are ready;
- week1 has every `.gd-pl .ln` empty and the tape text equals `Tale of the tape` + records + `No games yet`;
- season has the 8 labels in order;
- rejoin shows `.gd` without a click;
- opp-missing shows `Lineup not available`;
- stale shows the stale line;
- match shows `Game 2 of 3`;
- final shows `Final` and `Back to League`, and no `Rematch`;
- small stacks the sides vertically;
- no `.gd-pl` is taller than 48 px at 1280;
- name cells have `scrollWidth <= clientWidth + 1`.

Screenshots go to `target/league-gameday/`.

Steps:

- [ ] **Step 1: Add the fixture, the states and the check.** Run `node Tools/check-league-gameday.mjs`; expect FAIL lines until Task 6 is complete, then all `ok`.
- [ ] **Step 2: Fix in Task 6's files** whatever the check finds, then re-run; expect `check-league-gameday: PASS (10 state(s))`.
- [ ] **Step 3: Look at the screenshots** for week1, season and small against `gameday-v4.html`, and fix visible drift once.
- [ ] **Step 4: Commit** `tools: league game day states and check`.

### Task 8: Rehearsal: see it on the beta

**Files:** `C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-league` (the rehearsal backend's worktree) and `C:\Users\Adam\Reborn Rehearsal\backend\rehearsal.mjs`.

- [ ] **Step 1: Bring the rehearsal backend up to date.** Merge `feat/league-game-day` into `reborn-backend-league` (or point `run.sh`'s `BACKEND_DIR` at a game-day worktree).
- [ ] **Step 2: Fill in the rehearsal's own site data.**
  - The live site lacks Task 1 until Adam OKs the upload, so `rehearsal.mjs` adds `season`, `team_season`, `has_portrait` and the opponent `roster` to the real `team_home` answer when they are missing, using the fixture values from Task 7.
  - It serves portraits for the 14 S4 starters from the Downloads portrait PNGs, mapped by PSXP as in the mockup.
  - This is clearly marked as a stand-in.
- [ ] **Step 3: Build the beta.** Add the TEST key to `signing.rs`, run `CARGO_BUILD_JOBS=1 cargo build --release -p reborn-ui`, then `git checkout -- crates/reborn-core/src/signing.rs` and verify it is clean. Copy the build to `%APPDATA%\NFL Street Reborn Beta\app\reborn-launcher.exe` only if no launcher is running. Restart the rehearsal servers (Postgres first).
- [ ] **Step 4: Tell Adam** to open League → Play Wolves Reign.
