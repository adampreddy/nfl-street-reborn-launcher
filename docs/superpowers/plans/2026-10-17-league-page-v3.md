# League Page v3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the League tab page with the approved layout F (page heading with your team card, a slim matchup banner, then Standings · Week N · Week N's best), with every team, game and player linking to its league-site page, and add `week_best` to the league site's team_home answer.

**Architecture:** `league-model.js` stays pure (no DOM): it turns the league answer into banner, game and week-best view models. `league.js` draws them with `el()` and opens links through the `open_website` command. The site adds one field; the backend passes it through unchanged, since `mergeLeague` spreads the site answer.

**Tech Stack:** Vanilla ES modules, `node --test` (`crates/reborn-ui/ui/*.test.mjs`), Playwright page checks (`Tools/`, Edge channel), PHP 8 + MySQL (league site, tests in `C:\Users\Adam\nsr_local\tests`).

**Spec:** `docs/superpowers/specs/2026-10-17-league-page-v3-design.md`. Approved mockup: `.superpowers/brainstorm/42797-1792231954/content/league-f-v5.html`.

## Global Constraints

- Branch `feat/league-tab-3041` in `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league`. LF line endings. Build with `CARGO_BUILD_JOBS=1`.
- Colours, radii and motion come from `theme.css` tokens only (`node Tools/check-theme-tokens.mjs` must pass). No team-colour washes, coloured row backgrounds or side bars. Your team is bold with a 6px `--red` dot after the name.
- Copy: the subtitle reads exactly `Reborn Season 4 · Week 3`, built from `league.name` + ` · ` + `league.week_label`. The strings "This week", "Your game" and "Catch-up game" appear nowhere on the page. The only marker for your game in the Week card is `You`.
- Links are built only from numeric ids, under `https://nflstreetreborn.com/league/`, with `league_id = league.id`. They open via `call('open_website', { url })`.
- Site changes go to the live league site only with Adam's explicit OK (Task 5). Never print INGEST_TOKEN, LEAGUE_SITE_TOKEN or R2 keys.

## Review Focus

1. **Long team names** ("Pinky's Records", or a 30-character name) in all three columns and in the banner. They ellipsize in one line, and the grid never scrolls sideways at 960px (Task 4, `check-league-page` small-window state).
2. **A week where nothing is final yet** (Week 1 now). Week's best falls back to the previous week. If there is none, it reads "No games played yet", and the Week card shows all games with empty status (Task 1 PHP test, Task 2 `weekBest` test).
3. **A catch-up game.** When `matchup.week` is less than `league.week`, the banner shows "Week 2 game" over VS. The heading keeps "Week 3". The You marker appears only if that fixture is in `week_games` (Task 2 `bannerState` and `weekGames` tests).
4. **Stale data.** Room buttons are hidden, and one `.partial` line reads "Can't reach the league right now", never two banners (Task 2 `bannerState` stale test, Task 4 `league-stale` state).
5. **A logo that never arrives**, or a team with no logo. The letter fallback has no colour wash, and the layout height is unchanged (Task 4, a state with `league_logo` answering null for one team).

---

### Task 1: League site sends `week_best`

**Files:**
- Modify: `C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php` (`nsr_th_page`, around line 151, and its return array)
- Modify: `C:\Users\Adam\nsr_local\tests\th_fixture.php` (stat_events rows with week 1 and week 2 yards)
- Test: `C:\Users\Adam\nsr_local\tests\team_home_test.php`

**Interfaces:**
- Produces, in the team_home answer: `week_best: {week: int, groups: [{label: "Passing"|"Rushing"|"Receiving", rows: [{poid: int, name: string, team_id: int, line: string}]}]}`, or `week_best: null` when no week up to the current one has a stat event.

- [ ] **Step 1: Write the failing checks** in `team_home_test.php`, after the existing A checks:

```php
$wb = $a['week_best'] ?? null;
check('week_best present', is_array($wb));
check('week_best is the latest week with stats', ($wb['week'] ?? null) === 1);
check('three groups in order', array_column($wb['groups'] ?? [], 'label') === ['Passing', 'Rushing', 'Receiving']);
check('top 3 per group at most', max(array_map(fn($g) => count($g['rows']), $wb['groups'])) <= 3);
check('passing leader line', ($wb['groups'][0]['rows'][0]['line'] ?? '') === '318 yds · 4 TD');
check('row carries poid and team_id', is_int($wb['groups'][0]['rows'][0]['poid'] ?? null) && is_int($wb['groups'][0]['rows'][0]['team_id'] ?? null));
```

Seed `th_fixture.php` so that a week-1 fixture's QB has `pass_yds` 318 and `pass_td` 4, and the league's current week is 2 with no week-2 stats. That exercises the fallback.

- [ ] **Step 2: Run, expect FAIL** — `php C:\Users\Adam\nsr_local\tests\team_home_test.php` → `FAIL week_best present`.

- [ ] **Step 3: Implement `nsr_th_week_best(PDO $pdo, int $lid, int $week, array $names): ?array`** in `nsr_team_home.php`, and add `'week_best' => nsr_th_week_best($pdo, $lid, $week, $names)` to `nsr_th_page`'s return.
  - Walk from `$week` down to 1. Use the first week where `nsr_player_stat_rows($pdo, $lid, $w)` is non-empty.
  - Rank each group with `nsr_rank_rows($rows, $key, 3)`: `pass_yds`, `rush_yds`, `rec_yds`.
  - Build `line` as `number_format(yds) . ' yds · ' . td . ' TD'`, with td from `pass_td` / `rush_td` / `rec_td`, defaulting to 0.
  - Totals count every game of the week, including games inside a series still being played. `nsr_player_stat_rows` sums `stat_events`, so this already holds.

- [ ] **Step 4: Run, expect PASS** — the same command. Every line PASS, exit 0. Also run `php C:\Users\Adam\nsr_local\tests\team_home_http_test.php` and expect exit 0.

- [ ] **Step 5: Commit** in the site repo (`C:\Users\Adam\nsr_local`): `git add public_html/league/_inc/nsr_team_home.php tests/th_fixture.php tests/team_home_test.php` → `git commit -m "team_home: week_best (top 3 passing/rushing/receiving of the latest week with stats)"`. Do NOT upload to the live site (Task 5).

### Task 2: League model for layout F

**Files:**
- Modify: `crates/reborn-ui/ui/league-model.js`
- Test: `crates/reborn-ui/ui/league-model.test.mjs`

**Interfaces:**
- Consumes: the league answer (`league`, `team`, `matchup`, `week_games`, `standings`, `week_best`) and `league_edition_status` (shape in contracts).
- Produces:
  - `seasonLine(home) -> string`: "Reborn Season 4 · Week 3". It joins `league.name` and `league.week_label` (or `Week ${league.week}`) with ` · `, and drops missing parts.
  - `teamCardLine(home) -> string`: "2-0 · 2nd" (record · `ordinal(standing)`).
  - `bannerState(home, { stale, edition, progress }) -> { kind, mid, scores, text, tone, actions }`:
    - `mid` is `'VS' | 'LIVE' | 'FINAL' | 'Week N game'`.
    - `scores` is `[you, opp] | null`.
    - `tone` is `'' | 'ok' | 'bad'`.
    - `actions` is `[{ key, label, primary, disabled }]`, where `key` is one of `install`, `offline`, `friendly`, `play`, `join`, `open`, `cancel`, `back`.
  - `weekGames(home) -> [{ fixture_id, mine, status, marker, away: Side, home: Side }]`, with `marker` one of `'Final' | 'You' | 'Live' | ''`.
  - `standingsRows(home)`: unchanged.
  - `weekBest(home) -> { title, groups: [{ label, rows: [{ poid, name, team_id, line, top }] }] } | { title, empty: 'No games played yet' }`.
  - `siteUrl(kind, home, id?) -> string | null`, with `kind` one of `team | game | player | standings | schedule | leaders`.
- Removes: `weekTitle`, `scoreCells`, `recentRows`, `leaderCols`, `rosterRows`, `friendlyRuleset` and their tests. `cardState` can stay as `bannerState`'s helper.

- [ ] **Step 1: Write the failing tests** in `league-model.test.mjs`, using the real 10-17 answer shape (`Tools/fixtures/league-home.json`, extended in Task 4):

```js
test('seasonLine and teamCardLine', () => {
  assert.equal(seasonLine({ league: { name: 'Reborn Season 4', week: 3, week_label: 'Week 3' } }), 'Reborn Season 4 · Week 3');
  assert.equal(seasonLine({ league: { name: 'Reborn Season 4', week: 3 } }), 'Reborn Season 4 · Week 3');
  assert.equal(teamCardLine({ team: { record: '2-0', standing: 2 } }), '2-0 · 2nd');
});
test('bannerState: before the game', () => {
  const s = bannerState(home(), { edition: READY });
  assert.equal(s.mid, 'VS');
  assert.deepEqual(s.actions.map((a) => [a.key, a.label, a.primary]), [['offline', 'Play offline', false], ['friendly', 'Friendly', false], ['play', 'Play Wolves Reign', true]]);
});
test('bannerState: waiting / join / lobby / live / between games / final / forfeit / bye / unscheduled / unlinked', () => {
  // waiting: text 'Waiting for SNIPER to join', actions [cancel]
  // join:    text 'SNIPER opened the room', actions [join 'Join' primary]
  // lobby:   text 'SNIPER is in the room', actions [open 'Open room' primary]
  // live:    mid 'LIVE', scores [14,7] from matchup.score, text 'Game 2 of 3 · you lead 1-0', actions [back 'Back to the game' primary]
  // between: series {home:1,away:0}, status in_progress, no room: scores [1,0], text 'Game 2 next · you lead 1-0', play primary
  // final:   mid 'FINAL', scores [2,1] (series), text 'You won the series 2-1', tone 'ok', actions [offline, friendly]
  // lost:    text 'You lost the series 1-2', tone 'bad'
  // forfeit: text 'You won by forfeit'
  // bye:     kind 'bye', text 'No game this week'; unscheduled: text 'No schedule yet'
  // unlinked: play disabled, text "SNIPER hasn't linked Discord on the league site"
});
test('bannerState: league files missing or behind take the action slot, except live and waiting', () => {
  // installed:false -> actions [install 'Install league files (1.4 GB)' primary], others absent
  // update_available -> 'Update league files (1.4 GB)'; live with update_available keeps [back]
});
test('bannerState: catch-up game and stale data', () => {
  // matchup.week 2 < league.week 3 -> mid 'Week 2 game'
  // stale + room -> no join/cancel/open actions; kind 'stale'
});
test('weekGames: finals first, then mine, then the rest; markers', () => {
  // week_games order [mine sched, A final, B sched, C final] -> [A, C, mine, B]; markers ['Final','Final','You','']
  // live game -> marker 'Live'; mine only when fixture_id === matchup.fixture_id
});
test('weekBest: groups, top flag, empty, title by week', () => {
  // week_best {week:3,...} -> title "Week 3's best", rows[0].top === true, others false, at most 3 rows
  // week_best null or missing -> { title: "Week 3's best", empty: 'No games played yet' } (week from league.week)
  // week_best.week 2 while league.week 3 -> title "Week 2's best"
});
test('siteUrl: only numeric ids, under the league site', () => {
  const h = { league: { id: 36, week: 3 } };
  assert.equal(siteUrl('team', h, 493), 'https://nflstreetreborn.com/league/team.php?league_id=36&team_id=493');
  assert.equal(siteUrl('game', h, 2614), 'https://nflstreetreborn.com/league/stats/box.php?league_id=36&game_id=2614');
  assert.equal(siteUrl('player', h, 12), 'https://nflstreetreborn.com/league/stats/player.php?league_id=36&poid=12');
  assert.equal(siteUrl('standings', h), 'https://nflstreetreborn.com/league/standings.php?league_id=36');
  assert.equal(siteUrl('schedule', h), 'https://nflstreetreborn.com/league/schedule.php?league_id=36&week=3');
  assert.equal(siteUrl('leaders', h), 'https://nflstreetreborn.com/league/stats/leaders.php?league_id=36');
  assert.equal(siteUrl('team', h, '493&x=1'), null);
  assert.equal(siteUrl('team', { league: {} }, 493), null);
});
```

Write each commented case as real assertions with those exact strings. The owner name in the copy is `matchup.opponent.owner_display` ("SNIPER"); the opponent team name in the Play label is `matchup.opponent.name`.

- [ ] **Step 2: Run, expect FAIL** — `node --test crates/reborn-ui/ui/league-model.test.mjs` → failures naming `seasonLine` and the others as not exported.

- [ ] **Step 3: Implement** the Produces functions in `league-model.js`. Keep everything pure.
  - `bannerState` builds on the existing `cardState` kinds and `installState` / `playGate`. The friendly action is `{ key: 'friendly', label: 'Friendly' }`; Task 3 adds the ▾.
  - `Side` = `{ team_id, name, record, score }`.

- [ ] **Step 4: Run, expect PASS** — the same command. All pass, plus `node --test crates/reborn-ui/ui/league-nav.test.mjs`.

- [ ] **Step 5: Commit** — `git commit -m "league model: layout F (banner states, week games order, week's best, site links)"`.

### Task 3: League page draws layout F

**Files:**
- Modify: `crates/reborn-ui/ui/league.js` (replace `teamHeader`, `scoreboard`, `thisWeek`, `playPanel`, `standings`, `recent`, `leaders`, `roster`, `view`)
- Rewrite: `crates/reborn-ui/ui/pages/league.css`
- Modify: `crates/reborn-ui/ui/DESIGN.md` (League row in the page table, and `--league-cell-w` removed if unused)

**Interfaces:**
- Consumes: everything Task 2 produces; `league_logo(teamId)` and `logoTeam`, already in league.js; `openMenu(anchor, { label, options, current, onPick })` from `menu.js`.
- Produces, in the DOM, for Task 4's checks:
  - `.lg-head` (the `h1` "League", `.purpose` = `seasonLine`), and `.lg-teamcard` (`data-key="league-team-site"`);
  - `.lg-banner` (actions as buttons with `data-key="league-<action key>"`, plus `data-key="league-friendly-menu"` for the ▾);
  - `.lg-cols` holding `#lg-standings`, `#lg-week` and `#lg-best`;
  - every linked row is a `<button type="button" class="lg-link" data-url="<siteUrl>" data-key="league-link-<n>">`.

- [ ] **Step 1: Draw the heading and team card.**
  - `h1` "League", with `p.purpose` = `seasonLine(home)`.
  - On the right, a `.glass.lg-teamcard`: logo 40px, name, `teamCardLine`, and "View on site ›" → `siteUrl('team', home, team.id)`.

- [ ] **Step 2: Draw the banner** from `bannerState`.
  - Your logo (52px), name and "OWNER · record"; then `mid`; then the opponent. `scores` sit beside each team when they're not null.
  - Behind it, `.lg-banner-bg` holds both logos, blurred, opacity .16. Use no new colour token: `filter` and `opacity` only.
  - Actions on the right. Clicking a team in the banner opens its team page.
  - Map action keys to the existing handlers:

    | Action key | Handler |
    |---|---|
    | play | `play()` |
    | join | `join(room)` |
    | open, back | `go('online.html')` |
    | cancel | `cancel(room)` |
    | offline | `offline()` |
    | friendly | `friendly('league')` |
    | install | `install()` |

  - The ▾ next to Friendly opens `openMenu` with "League rules" (current) and "Standard rules". Picking Standard calls `friendly('raw')` once; nothing is stored.
  - Delete `FRIENDLY_KEY`, `leagueSettingsOn` and the switch.
  - Controller setup stays: the existing `.partial` box, right under the banner.

- [ ] **Step 3: Draw the three columns.**
  - `#lg-standings`: heading "Standings ›" linked to `siteUrl('standings')`, aside "W-L". Rows show rank, logo, name (with a bold dot when `mine`) and record; each row is an `.lg-link` to that team.
  - `#lg-week`: heading "Week N ›" linked to `siteUrl('schedule')`, aside "N of M final". Rows come from `weekGames`; each row is an `.lg-link` to `siteUrl('game', fixture_id)`, with the marker on the right.
  - `#lg-best`: heading "Week N's best", aside "All stats ›" linked to `siteUrl('leaders')`. Groups come from `weekBest`, or the `empty` line. Each row is an `.lg-link` to `siteUrl('player', poid)`, and `top` rows are bold.
  - One click handler on `#screen` reads `.lg-link[data-url]` and calls `call('open_website', { url })`.

- [ ] **Step 4: Write `pages/league.css`** to the spec's sizes:
  - Grid `.lg-cols{grid-template-columns:minmax(0,1.1fr) minmax(0,1fr) minmax(0,1.1fr);gap:16px;align-items:stretch}`. Cards are flex columns, and week rows and best groups get `flex:1`.
  - Rows are 36px; names ellipsize.
  - Under 1100px: two columns, with `#lg-best` spanning both and its groups in 3 columns. The banner wraps, and the primary button gets `flex:1`.
  - `#main` has no `min-height` stretch.

- [ ] **Step 5: Run the old harness checks, expect failures only in the League states that Task 4 replaces** — `node Tools/check-glass-pages.mjs` and `node Tools/check-shell-pages.mjs`. Note which League states fail, and confirm nothing outside League fails.

- [ ] **Step 6: Commit** — `git commit -m "League page: layout F (heading + team card, banner, standings / week / week's best, site links)"`.

### Task 4: Preview states and the League page check

**Files:**
- Modify: `Tools/fixtures/league-home.json`: replace it with the real 10-17 Season 4 answer from `C:\Reborn Rehersal\real-team-home-adam.json`, plus a `week_best` and Week 3 sample scores. Keep `logo_url`s; logos come from `league_logo`.
- Modify: `Tools/lib/page-states.mjs` (`leagueStates()`; `__byArg` already exists)
- Create: `Tools/check-league-page.mjs`
- Delete (untracked): `Tools/shot-league.mjs`, `Tools/shot-league-real.mjs`

**Interfaces:**
- Consumes: Task 3's DOM hooks (`.lg-head`, `.lg-banner`, `#lg-standings`, `#lg-week`, `#lg-best`, `.lg-link[data-url]`).

- [ ] **Step 1: Write the states:**
  - `league` (before the game);
  - `league-waiting`, `league-join`, `league-lobby`, `league-live`, `league-between`, `league-final`, `league-forfeit`, `league-bye`, `league-unscheduled`, `league-unlinked`, `league-stale`, `league-catchup`;
  - `league-not-installed`, `league-update`, `league-installing`;
  - `league-no-best` (`week_best` null), `league-no-logo` (`league_logo` answers null for team 484);
  - `league-small` (960×760).

  `league_logo` answers come from `{ __byArg: 'teamId', values }`, with small PNG data URLs generated in the fixture (not the real 300 KB logos).

- [ ] **Step 2: Write `Tools/check-league-page.mjs`** in the style of `check-online-page.mjs`. Run every League state at 1280×800 (and `league-small` at 960). Fail on any of these:
  - a page error;
  - the text "This week", "Your game" or "Catch-up game" anywhere in `#screen`;
  - a `.lg-roster` element, or text "Roster";
  - `.purpose` not equal to `Reborn Season 4 · Week 3` (in `league`);
  - the bottoms of the three `.lg-cols` cards differing by more than 2px;
  - `scrollWidth > clientWidth` on `#main`;
  - Week card row order not finals → mine → rest, or the `You` marker count not exactly 1 (in `league`);
  - more than 3 rows in any `#lg-best` group;
  - clicking the first `#lg-standings .lg-link` not calling `open_website` with `https://nflstreetreborn.com/league/team.php?league_id=36&team_id=<that row's team>`. Record calls by wrapping the harness's `open_website` answer.

  Save screenshots to `target/league-page/`.

- [ ] **Step 3: Run** — `node Tools/check-league-page.mjs` → `PASS` and the screenshots. Look at `league.png`, `league-small.png` and `league-live.png` against the mockup `league-f-v5.html`.

- [ ] **Step 4: Run everything** — `node Tools/check-theme-tokens.mjs`, `node Tools/check-page-call-args.mjs`, `node Tools/check-shell-pages.mjs`, `node Tools/check-glass-pages.mjs`, and `for t in crates/reborn-ui/ui/*.test.mjs; do node --test "$t"; done`. All pass.

- [ ] **Step 5: Commit** — `git commit -m "League page check: every layout F state, columns end on one line, links open the site"`.

### Task 5: Rehearsal build, then the site deploy (Adam's OK)

**Files:** none new.

- [ ] **Step 1: Build the rehearsal launcher.**
  - Add the TEST key to `crates/reborn-core/src/signing.rs` (`d4d59af6...`; see `C:\Users\Adam\Reborn Rehearsal\README.md`).
  - Run `CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=".../NFL Street Reborn Launcher league/target" cargo build --release -p reborn-ui`.
  - Then `git checkout -- crates/reborn-core/src/signing.rs`, and confirm `git status` is clean.
  - Copy `target/release/reborn-launcher.exe` over `%APPDATA%\NFL Street Reborn Beta\app\reborn-launcher.exe`. The backup `.bak_pre_league_rehearsal` already exists.

- [ ] **Step 2: Ask Adam to open the beta League tab.** It runs on real data: the rehearsal backend reads the real site. Fix what he reports.

- [ ] **Step 3: With Adam's explicit OK only, upload the site file.**
  - Upload `nsr_team_home.php` to the live site (WinSCP session `edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com`, root `/jacobg44.sg-host.com/public_html/league/_inc/`).
  - Save the live copy first to `C:\Users\Adam\nsr_local\stage_league_tab\live\nsr_team_home.v_before_week_best.php`.
  - Read it back.
  - Then restart the rehearsal backend and confirm `GET /v1/league/me` carries `week_best`.
