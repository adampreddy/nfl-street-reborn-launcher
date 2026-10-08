# League Tab — Plan 1 of 3: League Site `team_home` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the league site a server-to-server action, `team_home`, that turns a Discord ID into that member's club page as JSON (or `member:false`).

**Architecture:** One new read-only include, `_inc/nsr_team_home.php`, builds the payload from the helpers the site's own pages already use (`nsr_series`, `nsr_standings_rows`, `nsr_player_stat_rows`, `nsr_rosters`, ...). `api/bot.php` gains a `team_home` action behind its existing `INGEST_TOKEN` bearer check. Only the Reborn backend calls it (Plan 2).

**Tech Stack:** Plain PHP + PDO. Live DB is MySQL (SiteGround); local replica is MariaDB 12.3 at `127.0.0.1` / `nsr_league` / `root` / `nsrlocaldev`. PHP 8.3 CLI on this PC (`php`). WinSCP for deploys.

**Spec:** `docs/superpowers/specs/2026-10-08-league-tab-design.md` (section "1. League site: `team_home` action").

**Plans in this set:** 1 = site (this), 2 = Reborn backend, 3 = launcher. Plan 2 consumes the JSON contract below verbatim.

## Global Constraints

- Working tree for site code is the replica `C:\Users\Adam\nsr_local\public_html\league` (no git). The git copy `C:\Users\Adam\Documents\jacobg44.sg-host.com` is stale since 2026-07-28 and lacks `api/bot.php` — do not use it.
- Live is the authority: Task 1 refreshes every touched file from live before any edit. Never trust the replica for Season 4 ids (live league 36; the replica tops out at 34).
- PHP syntax must stay 7.4-compatible (live PHP version is unverified): no `match`, no `?->`, no named arguments, no `str_contains`/`str_starts_with`, no union types, no `readonly`.
- No DDL. This plan creates no tables and alters none.
- Deploy from **PowerShell only** (Bash hangs on winscp.com). WinSCP: `C:\Program Files (x86)\WinSCP\WinSCP.com`, stored session `edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com`; never pass `/ini=nul`. If the host does not resolve add `-rawsettings HostName=ftp.nflstreetreborn.com`. If SiteGround refuses/blocks, ask Adam to turn the VPN off.
- One `put` per file, then `get` it back and compare SHA-256. Never `synchronize` a tree. Never upload `_inc/config.php` or `_inc/ingest_token.php`.
- Live remote root: `/jacobg44.sg-host.com/public_html/league`.
- Secrets (the pulled live `ingest_token.php`, runner keys) live only in `C:\Users\Adam\nsr_local\deploy\secret\` and are never printed to the console or committed.
- Discord IDs are **strings** everywhere in JSON.
- JSON field names are exactly the contract below; Plan 2 depends on them.
- SiteGround caches 404s: any GET to a freshly uploaded URL carries a random `&nc=` parameter.

## JSON contract (what Plan 2 consumes)

Request: `POST /league/api/bot.php`, `Authorization: Bearer <INGEST_TOKEN>`, body `{"action":"team_home","discord_user_id":"<snowflake>"}`.

Errors: `401 {"ok":false,"error":"Unauthorized"}`; `400 {"ok":false,"error":"discord_user_id must be a Discord snowflake"}`; `405` for non-POST.

Not a member of the current league: `200 {"ok":true,"member":false}`.

Member:

```json
{
  "ok": true,
  "member": true,
  "schedule_published": true,
  "league":  {"id": 36, "name": "Reborn Season 4", "season": 4, "week": 6, "week_label": "Week 6"},
  "team":    {"id": 493, "name": "Subway Surfers", "abbr": "ADAM",
              "logo_url": "https://nflstreetreborn.com/league/uploads/...png", "color": "#00A3FF",
              "record": "4-1", "standing": 2},
  "matchup": {"fixture_id": 1234, "week": 6, "home": true,
              "status": "scheduled", "forfeit": false,
              "score": null, "series": null, "winner_team_id": null, "when": null,
              "opponent": {"team_id": 486, "name": "Football Furies", "abbr": "BLONDE",
                           "logo_url": null, "color": "#FF0000", "record": "3-2",
                           "owner_display": "BLONDE", "discord_user_id": "480510938571735040"}},
  "week_games": [
    {"fixture_id": 1234,
     "home": {"team_id": 493, "name": "...", "abbr": "...", "logo_url": null, "color": "#...", "record": "4-1"},
     "away": {"team_id": 486, "name": "...", "abbr": "...", "logo_url": null, "color": "#...", "record": "3-2"},
     "status": "final", "forfeit": false,
     "score": {"home": 49, "away": 21}, "series": {"home": 2, "away": 0},
     "winner_team_id": 493, "when": null}
  ],
  "standings": [{"rank": 1, "team_id": 482, "name": "...", "abbr": "...", "logo_url": null, "record": "5-0"}],
  "recent":    [{"fixture_id": 1200, "week": 5, "opponent_abbr": "JOCK", "home": false,
                 "result": "W", "score": "28-21", "series": "2-1", "forfeit": false}],
  "leaders":   {"pass": [{"poid": 12, "name": "...", "value": 14}], "rush": [], "rec": []},
  "leaders_unit": "TD",
  "roster":    [{"poid": 12, "name": "...", "position": "QB", "number": 7}],
  "site_url":  "https://nflstreetreborn.com/league/team.php?league_id=36&team_id=493"
}
```

Field rules:
- `status` ∈ `scheduled | in_progress | final` for games; `matchup.status` may also be `bye` (then `fixture_id`, `home`, `score`, `series`, `winner_team_id`, `opponent` are `null`).
- `score` = summed game points `{home, away}` once any game is played, else `null`. `series` = games won `{home, away}` once any game is played or a winner is recorded, else `null`.
- `when` is always `null`: the schema stores no kickoff time (`games.played_at` is dead). Kept so the client has one place to read it if it is ever added.
- `league.week` is the week the tab (scoreboard, `week_games`) is about (see `nsr_th_week`): the site's current week, advanced to the next scheduled week once every series in it is settled.
- `matchup` is chosen **per club**, not from `league.week`: the club's earliest series that is not complete in any week >= the site's current week (`nsr_current_week`); else its series in `league.week` (may be `final`); else a bye with `week` = `league.week`. `matchup.week` is that series' own week, so it can differ from `league.week` (one postponed fixture no longer freezes every other club's card).
- `forfeit` (bool) is on `week_games[]`, on `matchup` (`false` for a bye) and on each `recent[]` entry, all from `nsr_series()`'s `forfeit` (games.status = `forfeit`).
- `schedule_published` (bool, top level, members only): `true` when the current league has any fixture at all. When `false`, `matchup` is a bye only because no schedule exists yet, not because the club sits the week out.
- `matchup.opponent.discord_user_id` is `string|null` (null when the opponent's owner has no linked Discord id); `owner_display` is a string and may be `''` (club with no owner row). Should a club ever have several owner rows, a row with a Discord id wins (deterministic ORDER BY in `nsr_th_owners`).
- `winner_team_id` follows the site's `nsr_series_winner()`: mid-series (`in_progress`) it is the club currently leading the series, not a settled winner; read it as final only when `status` is `final`.
- `league.season` is the first number in `season_label`, falling back to the league `name` when the label is empty; `null` when neither has a number.
- `recent` is at most 5 entries, newest first (week desc, then fixture id desc).
- `logo_url` is absolute or `null` (Season 4 logos are still placeholders).
- `recent.result` is `W`/`L`, or `null` for a forfeit with no recorded winner; `recent.score`/`series` are from this team's side ("mine-theirs") or `null` when only the series winner was recorded.

### Deviations from the spec's sketch (and why)

| Spec | Here | Why |
|---|---|---|
| `{"member":false}` | `{"ok":true,"member":false}` | every bot.php reply carries `ok`; additive |
| `leaders` values in yards (`1204`) | touchdowns, plus `"leaders_unit":"TD"` | the schema has no yardage at all (`nsr_data.php:23-29`) |
| `matchup.status` `scheduled\|final\|bye` | adds `in_progress` | a best-of-3 with game 1 recorded is neither |
| (none) | `series`, `winner_team_id`, `forfeit`, `week_label`, `color` on sides, `poid` on players | the scoreboard needs series scores; playoff weeks have names; additive |
| (none) | `schedule_published`; `forfeit` on `matchup` and `recent[]` (final-review fix, 2026-10-08) | tell "no schedule" from a real bye; show forfeits on the card and results; additive |
| `roster[].number` | from pool field `PJEN` | verified: Barry Sanders 20, Ray Lewis 52, Randy Moss 84 |

## Review Focus

1. **One person, two owner rows** (DREAD and Sentence share one Discord ID) — must resolve to the single club in the current league, not error or pick nothing. Pinned in Task 2.
2. **Week finished, next week not started** — the site's `nsr_current_week` stays on the finished week; the tab must show next week's opponent. Pinned in Task 2.
3. **Owner only in an older season** — a past club must not make someone a member. Pinned in Task 2.
4. **Club with no logo** (all Season 4 clubs today) — `logo_url` must be `null`, never `"https://nflstreetreborn.com/"`. Pinned in Task 3.
5. **Series recorded by winner only** (no game scores) — `result` present, `score` `null`, no crash. Pinned in Task 3.

Known site semantics, not changed here: `nsr_series()` calls a fixture whose only recorded row is one complete game "complete" (single-game leagues). A best-of-3 game 1 written without a game-2 row would show `final` on the site and in this payload. Plan 2's live merge (backend room/series state) overrides `status` for fixtures with an open league room, which covers the tab.

## File map

| Path | Status | Responsibility |
|---|---|---|
| `nsr_local\public_html\league\_inc\nsr_team_home.php` | create | `nsr_team_home()` and its small helpers; read-only |
| `nsr_local\public_html\league\api\bot.php` | modify | `team_home` action, inserted just above the final `fail('Unknown action: ' ...)` |
| `nsr_local\tests\th_fixture.php` | create | throwaway league builder used by the tests (never deployed) |
| `nsr_local\tests\team_home_test.php` | create | CLI tests for `nsr_team_home()` (never deployed) |
| `nsr_local\tests\team_home_http_test.php` | create | CLI tests for the HTTP action against `php -S` (never deployed) |
| `nsr_local\deploy\stage_league_tab\` | create | pulled live copies, upload scripts, runner |

`nsr_local\tests\` sits outside `public_html`, so nothing in it can be uploaded by accident.

---

### Task 1: Refresh the replica from live

**Files:**
- Create: `C:\Users\Adam\nsr_local\deploy\stage_league_tab\get_live.txt`
- Create: `C:\Users\Adam\nsr_local\deploy\stage_league_tab\live\` (pulled files)
- Create: `C:\Users\Adam\nsr_local\deploy\stage_league_tab\replica-before\` (backup of replica files)
- Modify: replica copies of the pulled files

**Interfaces:**
- Consumes: nothing.
- Produces: a replica whose `api/bot.php`, `_inc/nsr_data.php`, `_inc/nsr_standings.php`, `_inc/nsr_hidden.php`, `_inc/nsr_owners.php` are byte-identical to live; `deploy\secret\ingest_token.live.php` holding live's token file; the fact that `_inc/nsr_team_home.php` does not exist live.

- [ ] **Step 1: Write the WinSCP get script**

`C:\Users\Adam\nsr_local\deploy\stage_league_tab\get_live.txt`:

```
option batch abort
option confirm off
open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com"
lcd "C:\Users\Adam\nsr_local\deploy\stage_league_tab\live"
cd /jacobg44.sg-host.com/public_html/league/api
get bot.php
cd /jacobg44.sg-host.com/public_html/league/_inc
get nsr_data.php
get nsr_standings.php
get nsr_hidden.php
get nsr_owners.php
lcd "C:\Users\Adam\nsr_local\deploy\secret"
get ingest_token.php ingest_token.live.php
option batch continue
ls nsr_team_home.php
exit
```

- [ ] **Step 2: Run it (PowerShell)**

```powershell
New-Item -ItemType Directory -Force C:\Users\Adam\nsr_local\deploy\stage_league_tab\live | Out-Null
& 'C:\Program Files (x86)\WinSCP\WinSCP.com' /script=C:\Users\Adam\nsr_local\deploy\stage_league_tab\get_live.txt /log=C:\Users\Adam\nsr_local\deploy\stage_league_tab\get_live.log
```

Expected: five files in `live\`, `ingest_token.live.php` in `deploy\secret\`, and the final `ls` reporting the file does not exist (`File or folder 'nsr_team_home.php' does not exist`). If `nsr_team_home.php` DOES exist live, stop and tell Adam — someone else is on this.

- [ ] **Step 3: Diff live against the replica**

```powershell
$r='C:\Users\Adam\nsr_local\public_html\league'; $l='C:\Users\Adam\nsr_local\deploy\stage_league_tab\live'
foreach ($p in @('api\bot.php','_inc\nsr_data.php','_inc\nsr_standings.php','_inc\nsr_hidden.php','_inc\nsr_owners.php')) {
  $a=(Get-FileHash "$r\$p").Hash; $b=(Get-FileHash "$l\$(Split-Path $p -Leaf)").Hash
  "{0,-24} {1}" -f $p, $(if ($a -eq $b) {'same'} else {'DIFFERS'})
}
```

For each `DIFFERS`, run `git diff --no-index "<replica>" "<live>"` and read it. Write one line per file into `deploy\stage_league_tab\diff-notes.txt` saying what changed. Specifically confirm in the live copies:
- `nsr_data.php` still defines `nsr_teams`, `nsr_series`, `nsr_series_winner`, `nsr_current_week`, `nsr_weeks`, `nsr_team_records`, `nsr_team_standing`, `nsr_player_names`, `nsr_player_positions`, `nsr_player_stat_rows`, `nsr_rank_rows`, `nsr_rosters`, `nsr_week_label` with the same parameter lists used in Task 2/3.
- `nsr_hidden.php` defines `NSR_HIDDEN_LEAGUES` and whether league 36 is in it (it must NOT be).
- `bot.php` still ends with `fail('Unknown action: ' . $action);` and has `out()`/`fail()` helpers.

```powershell
Select-String -Path C:\Users\Adam\nsr_local\deploy\stage_league_tab\live\nsr_data.php -Pattern '^function (nsr_teams|nsr_series|nsr_series_winner|nsr_current_week|nsr_weeks|nsr_team_records|nsr_team_standing|nsr_player_names|nsr_player_positions|nsr_player_stat_rows|nsr_rank_rows|nsr_rosters|nsr_week_label)\('
Select-String -Path C:\Users\Adam\nsr_local\deploy\stage_league_tab\live\nsr_hidden.php -Pattern '\d+,'
Select-String -Path C:\Users\Adam\nsr_local\deploy\stage_league_tab\live\bot.php -Pattern "fail\('Unknown action"
```

Expected: 13 function lines; hidden ids listed (no 36); one `Unknown action` line. If any signature changed, adapt Task 2/3 code to the live signature before continuing and note it in `diff-notes.txt`.

- [ ] **Step 4: Back up the replica copies, then make the replica match live**

```powershell
$r='C:\Users\Adam\nsr_local\public_html\league'; $s='C:\Users\Adam\nsr_local\deploy\stage_league_tab'
New-Item -ItemType Directory -Force "$s\replica-before\api","$s\replica-before\_inc" | Out-Null
Copy-Item "$r\api\bot.php" "$s\replica-before\api\"
foreach ($f in 'nsr_data.php','nsr_standings.php','nsr_hidden.php','nsr_owners.php') { Copy-Item "$r\_inc\$f" "$s\replica-before\_inc\" }
Copy-Item "$s\live\bot.php" "$r\api\bot.php" -Force
foreach ($f in 'nsr_data.php','nsr_standings.php','nsr_hidden.php','nsr_owners.php') { Copy-Item "$s\live\$f" "$r\_inc\$f" -Force }
foreach ($p in "$r\api\bot.php","$r\_inc\nsr_data.php","$r\_inc\nsr_standings.php","$r\_inc\nsr_hidden.php","$r\_inc\nsr_owners.php") { php -l $p }
```

Expected: `No syntax errors detected` five times. (If live `nsr_data.php` now `require`s an include the replica lacks, pull that file too with the same get-script pattern and repeat.)

- [ ] **Step 5: Commit the notes (plan worktree)**

The site has no git; the record of this task lives in the launcher worktree plan folder:

```powershell
Copy-Item C:\Users\Adam\nsr_local\deploy\stage_league_tab\diff-notes.txt "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league\docs\superpowers\plans\2026-10-08-league-tab-1-site.diff-notes.txt"
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" add docs/superpowers/plans/2026-10-08-league-tab-1-site.diff-notes.txt
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" commit -m "site plan: live-vs-replica diff notes before team_home"
```

---

### Task 2: Membership, league and matchup (`nsr_team_home` core)

**Files:**
- Create: `C:\Users\Adam\nsr_local\tests\th_fixture.php`
- Create: `C:\Users\Adam\nsr_local\tests\team_home_test.php`
- Create: `C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php`

**Interfaces:**
- Consumes: live-identical `nsr_data.php` / `nsr_standings.php` / `nsr_hidden.php` from Task 1.
- Produces:
  - `nsr_team_home(PDO $pdo, string $discordUserId): array` — throws `InvalidArgumentException('discord_user_id must be a Discord snowflake')` on a non-snowflake; returns `['member' => false]` or the member array (keys `member, league, team, matchup` after this task; Task 3 adds the rest).
  - `nsr_th_current_league(PDO $pdo): int`
  - `nsr_th_week(PDO $pdo, int $leagueId): int`
  - `nsr_th_url(?string $path): ?string`
  - `nsr_th_side(array $team, array $records): array` → `{team_id,name,abbr,logo_url,color,record}`
  - `nsr_th_status(array $series): string`, `nsr_th_score(array $series): ?array`, `nsr_th_series(array $series): ?array`
  - `const NSR_PUBLIC_ORIGIN = 'https://nflstreetreborn.com'`
  - Test helper `th_fixture(PDO $pdo): array` with keys `lid, t (A..D => team id), did (A..F => discord id), g (wk1ab, wk1cd, wk2ba => game id), poid (1..4)`.

- [ ] **Step 1: Write the fixture builder**

`C:\Users\Adam\nsr_local\tests\th_fixture.php`:

```php
<?php
declare(strict_types=1);

/**
 * A throwaway league for the team_home tests. Call inside a transaction and
 * roll back afterwards; nothing here is meant to survive.
 *
 *   week 1  A (home) beat B 2-0 (21-14, 28-7)
 *           C vs D decided by series row only: D won, no game scores
 *   week 2  B (home) vs A, nothing played yet
 *           C and D have a bye
 *   owners  A..D own the four clubs; E owns a club only in an OLDER league;
 *           F is a second owner row sharing A's Discord id with no club.
 */
function th_fixture(PDO $pdo): array {
    $pool = (int)$pdo->query(
        "SELECT version_id FROM pool_csv_rows GROUP BY version_id ORDER BY version_id DESC LIMIT 1")->fetchColumn();
    $older = (int)$pdo->query("SELECT MAX(id) FROM leagues")->fetchColumn();

    $pdo->prepare("INSERT INTO leagues (name, season_label, pool_version_id, weeks_regular_season)
                   VALUES ('TH Test League', 'Season 9', ?, 2)")->execute([$pool]);
    $lid = (int)$pdo->lastInsertId();

    $t = [];
    $teams = ['A' => ['Team A', '#112233', "/league/uploads/league_$lid/teams/a.png"],
              'B' => ['Señor B', '#445566', null],
              'C' => ['Team C', '#778899', null],
              'D' => ['Team D', '#AABBCC', null]];
    $insT = $pdo->prepare("INSERT INTO teams (league_id, name, abbrev, primary_color, logo_path) VALUES (?,?,?,?,?)");
    foreach ($teams as $k => [$name, $color, $logo]) {
        $insT->execute([$lid, $name, "TH$k", $color, $logo]);
        $t[$k] = (int)$pdo->lastInsertId();
    }
    $pdo->prepare("INSERT INTO teams (league_id, name, abbrev) VALUES (?, 'TH Old Club', 'THOLD')")->execute([$older]);
    $oldTeam = (int)$pdo->lastInsertId();

    $did = ['A' => '900000000000000001', 'B' => '900000000000000002', 'C' => '900000000000000003',
            'D' => '900000000000000004', 'E' => '900000000000000005', 'F' => '900000000000000001'];
    $o = [];
    $insO = $pdo->prepare("INSERT INTO league_owners (handle, display_name, discord_user_id) VALUES (?,?,?)");
    foreach ($did as $k => $d) {
        $insO->execute(["THOWNER$k", $k === 'B' ? 'Bee' : null, $d]);
        $o[$k] = (int)$pdo->lastInsertId();
    }
    $insOT = $pdo->prepare("INSERT INTO owner_teams (owner_id, league_id, team_id) VALUES (?,?,?)");
    foreach (['A', 'B', 'C', 'D'] as $k) $insOT->execute([$o[$k], $lid, $t[$k]]);
    $insOT->execute([$o['E'], $older, $oldTeam]);

    $insG = $pdo->prepare("INSERT INTO games (league_id, week_num, week_no, series_game_no, home_team_id, away_team_id)
                           VALUES (?,?,?,1,?,?)");
    $g = [];
    $insG->execute([$lid, 1, 1, $t['A'], $t['B']]); $g['wk1ab'] = (int)$pdo->lastInsertId();
    $insG->execute([$lid, 1, 1, $t['C'], $t['D']]); $g['wk1cd'] = (int)$pdo->lastInsertId();
    $insG->execute([$lid, 2, 2, $t['B'], $t['A']]); $g['wk2ba'] = (int)$pdo->lastInsertId();

    $insS = $pdo->prepare("INSERT INTO game_series_scores
                             (league_id, game_id, series_game_no, home_score, away_score, winner_team_id, status)
                           VALUES (?,?,?,?,?,?,'complete')");
    $insS->execute([$lid, $g['wk1ab'], 1, 21, 14, null]);
    $insS->execute([$lid, $g['wk1ab'], 2, 28, 7, null]);
    $insS->execute([$lid, $g['wk1cd'], 0, null, null, $t['D']]);   // series row only

    $poids = array_map('intval', $pdo->query(
        "SELECT poid FROM pool_csv_rows WHERE version_id = $pool ORDER BY poid LIMIT 4")->fetchAll(PDO::FETCH_COLUMN));
    $insR = $pdo->prepare("INSERT INTO league_player_rights (league_id, poid, team_id, status) VALUES (?,?,?,'rostered')");
    foreach ([0, 1, 2] as $i) $insR->execute([$lid, $poids[$i], $t['A']]);
    $insR->execute([$lid, $poids[3], $t['B']]);

    $insE = $pdo->prepare("INSERT INTO stat_events (league_id, game_id, series_game_no, team_id, player_poid, stat_key, delta)
                           VALUES (?,?,?,?,?,?,?)");
    $insE->execute([$lid, $g['wk1ab'], 1, $t['A'], $poids[0], 'pass_td', 2]);
    $insE->execute([$lid, $g['wk1ab'], 2, $t['A'], $poids[0], 'pass_td', 1]);
    $insE->execute([$lid, $g['wk1ab'], 1, $t['A'], $poids[1], 'rush_td', 1]);
    $insE->execute([$lid, $g['wk1ab'], 1, $t['B'], $poids[3], 'rec_td', 5]);

    return ['lid' => $lid, 't' => $t, 'did' => $did, 'g' => $g,
            'poid' => [1 => $poids[0], 2 => $poids[1], 3 => $poids[2], 4 => $poids[3]]];
}
```

- [ ] **Step 2: Write the failing tests**

`C:\Users\Adam\nsr_local\tests\team_home_test.php`:

```php
<?php
declare(strict_types=1);

/* php C:\Users\Adam\nsr_local\tests\team_home_test.php
   Builds a throwaway league in a transaction, checks nsr_team_home() against it,
   rolls back. Exit 0 = every check passed. */

require_once __DIR__ . '/../public_html/league/_inc/config.php';
require_once __DIR__ . '/../public_html/league/_inc/db.php';
require_once __DIR__ . '/../public_html/league/_inc/nsr_team_home.php';
require_once __DIR__ . '/th_fixture.php';

$fails = 0;
function check(string $what, bool $ok): void {
    global $fails;
    echo ($ok ? 'PASS ' : 'FAIL ') . $what . "\n";
    if (!$ok) $fails++;
}

$pdo = db();
$pdo->beginTransaction();
try {
    $fx = th_fixture($pdo);
    $t = $fx['t']; $did = $fx['did']; $g = $fx['g']; $lid = $fx['lid'];

    /* ---- membership ---- */
    $threw = false;
    try { nsr_team_home($pdo, 'abc'); } catch (InvalidArgumentException $e) { $threw = true; }
    check('non-snowflake id is refused', $threw);
    check('unknown id is not a member', nsr_team_home($pdo, '900000000000000099') === ['member' => false]);
    check('owner of an older season only is not a member', nsr_team_home($pdo, $did['E']) === ['member' => false]);
    check('current league is the fixture league', nsr_th_current_league($pdo) === $lid);

    /* ---- A: league, team, matchup ---- */
    $a = nsr_team_home($pdo, $did['A']);   // F shares this id and owns nothing: A must still resolve
    check('A is a member', ($a['member'] ?? null) === true);
    check('league id', ($a['league']['id'] ?? null) === $lid);
    check('league season parsed from season_label', ($a['league']['season'] ?? null) === 9);
    check('finished week 1 advances the tab to week 2', ($a['league']['week'] ?? null) === 2);
    check('week label', ($a['league']['week_label'] ?? '') !== '');
    check('team id', ($a['team']['id'] ?? null) === $t['A']);
    check('team abbr', ($a['team']['abbr'] ?? null) === 'THA');
    check('team logo absolute',
          ($a['team']['logo_url'] ?? null) === "https://nflstreetreborn.com/league/uploads/league_$lid/teams/a.png");
    check('team color', ($a['team']['color'] ?? null) === '#112233');
    check('team record', ($a['team']['record'] ?? null) === '1-0');
    check('team standing is 1 or 2', in_array($a['team']['standing'] ?? 0, [1, 2], true));

    $m = $a['matchup'] ?? [];
    check('matchup fixture is week 2 B-A', ($m['fixture_id'] ?? null) === $g['wk2ba']);
    check('matchup week', ($m['week'] ?? null) === 2);
    check('A is away', ($m['home'] ?? null) === false);
    check('matchup scheduled', ($m['status'] ?? null) === 'scheduled');
    check('scheduled has no score', array_key_exists('score', $m) && $m['score'] === null);
    check('when is null', array_key_exists('when', $m) && $m['when'] === null);
    check('opponent is B', ($m['opponent']['team_id'] ?? null) === $t['B']);
    check('opponent owner display', ($m['opponent']['owner_display'] ?? null) === 'Bee');
    check('opponent discord id is a string', ($m['opponent']['discord_user_id'] ?? null) === $did['B']);
    check('opponent record', ($m['opponent']['record'] ?? null) === '0-1');

    /* ---- C: bye ---- */
    $c = nsr_team_home($pdo, $did['C']);
    check('C has a bye', ($c['matchup']['status'] ?? null) === 'bye');
    check('bye has no opponent', array_key_exists('opponent', $c['matchup']) && $c['matchup']['opponent'] === null);
    check('bye has no fixture', $c['matchup']['fixture_id'] === null);

    // --- payload checks (Task 3) ---

    /* ---- game 1 of week 2 played: B won 14-10, game 2 still to come ----
       The game-2 row matters: nsr_series() treats a fixture whose ONLY row is one
       complete game as settled (single-game leagues), so a best-of-3 in progress
       needs its next game's row present, as the site's ingest writes it. */
    $pdo->prepare("INSERT INTO game_series_scores (league_id, game_id, series_game_no, home_score, away_score, status)
                   VALUES (?,?,1,14,10,'complete'), (?,?,2,NULL,NULL,'scheduled')")
        ->execute([$lid, $g['wk2ba'], $lid, $g['wk2ba']]);
    $a2 = nsr_team_home($pdo, $did['A']);
    check('one game in: in_progress', ($a2['matchup']['status'] ?? null) === 'in_progress');
    check('series 1-0 to home', ($a2['matchup']['series'] ?? null) === ['home' => 1, 'away' => 0]);
    check('score 14-10', ($a2['matchup']['score'] ?? null) === ['home' => 14, 'away' => 10]);
    check('still week 2', ($a2['league']['week'] ?? null) === 2);

    check('payload encodes as JSON', json_encode($a2, JSON_THROW_ON_ERROR) !== '');
} finally {
    $pdo->rollBack();
}
echo $fails ? "\n$fails FAILED\n" : "\nALL PASS\n";
exit($fails ? 1 : 0);
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `php C:\Users\Adam\nsr_local\tests\team_home_test.php`
Expected: fatal `Failed opening required '.../_inc/nsr_team_home.php'`.

- [ ] **Step 4: Write the core implementation**

`C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php`:

```php
<?php
declare(strict_types=1);

/**
 * One club's home page as data, for the Reborn launcher's League tab.
 *
 * The caller is the Reborn backend, never the launcher: it knows who is signed
 * in and sends that person's Discord id. The id is matched to league_owners and
 * from there to the club that person runs in the CURRENT league - the newest one
 * not hidden. A club in an older season does not count, so a former member gets
 * {member:false} rather than a finished season dressed up as this week.
 *
 * Read-only. Every figure comes from the helpers the site's own pages use, so the
 * tab and the website cannot disagree about a record or a table.
 */

require_once __DIR__ . '/nsr_data.php';
require_once __DIR__ . '/nsr_standings.php';

/** Where the site is served, for absolute logo and page links. */
const NSR_PUBLIC_ORIGIN = 'https://nflstreetreborn.com';

/** A site path as an absolute URL; null when there is no path at all. */
function nsr_th_url(?string $path): ?string {
    $p = trim((string)$path);
    if ($p === '') return null;
    if (preg_match('~^https?://~i', $p)) return $p;
    return NSR_PUBLIC_ORIGIN . '/' . ltrim($p, '/');
}

/** The league the tab is about: the newest one not hidden. 0 when there is none. */
function nsr_th_current_league(PDO $pdo): int {
    $ids = array_map('intval', $pdo->query('SELECT id FROM leagues ORDER BY id DESC')->fetchAll(PDO::FETCH_COLUMN));
    foreach ($ids as $id) {
        if (!in_array($id, NSR_HIDDEN_LEAGUES, true)) return $id;
    }
    return 0;
}

/**
 * The week the tab is about.
 *
 * The site's own current week stays on a week once it is fully played - right
 * for a masthead showing last week's finals, wrong for "who do I play this
 * week". So when every series in that week is settled, move on to the next week
 * that has fixtures. A finished season stays on its last week.
 */
function nsr_th_week(PDO $pdo, int $leagueId): int {
    $w = nsr_current_week($pdo, $leagueId);
    foreach (nsr_series($pdo, $leagueId, $w) as $s) {
        if (!$s['complete']) return $w;
    }
    foreach (nsr_weeks($pdo, $leagueId) as $n) {
        if ($n > $w) return $n;
    }
    return $w;
}

/** One side of a fixture, as the scoreboard draws it. */
function nsr_th_side(array $t, array $records): array {
    return [
        'team_id'  => (int)$t['id'],
        'name'     => (string)$t['name'],
        'abbr'     => (string)($t['abbrev'] ?? ''),
        'logo_url' => nsr_th_url($t['logo_path'] ?? null),
        'color'    => (string)($t['primary_color'] ?? ''),
        'record'   => $records[(int)$t['id']] ?? '0-0',
    ];
}

function nsr_th_status(array $s): string {
    if (!empty($s['complete'])) return 'final';
    if ((int)$s['played'] > 0)  return 'in_progress';
    return 'scheduled';
}

/** Summed game points, once anything has been played. */
function nsr_th_score(array $s): ?array {
    if ((int)$s['played'] === 0) return null;
    return ['home' => (int)$s['home_points'], 'away' => (int)$s['away_points']];
}

/** Games won in the series, once anything has been played or a winner recorded. */
function nsr_th_series(array $s): ?array {
    if ((int)$s['played'] === 0 && nsr_series_winner($s) === null) return null;
    return ['home' => (int)$s['home_wins'], 'away' => (int)$s['away_wins']];
}

/** team_id => owner display name and Discord id, for one league. */
function nsr_th_owners(PDO $pdo, int $leagueId): array {
    $st = $pdo->prepare(
        "SELECT ot.team_id, o.handle, o.display_name, o.discord_user_id
           FROM owner_teams ot JOIN league_owners o ON o.id = ot.owner_id
          WHERE ot.league_id = ?");
    $st->execute([$leagueId]);
    $out = [];
    foreach ($st->fetchAll(PDO::FETCH_ASSOC) as $r) {
        $d = trim((string)($r['discord_user_id'] ?? ''));
        $out[(int)$r['team_id']] = [
            'display' => (string)($r['display_name'] ?: $r['handle']),
            'discord' => $d !== '' ? $d : null,
        ];
    }
    return $out;
}

function nsr_team_home(PDO $pdo, string $discordUserId): array {
    $did = trim($discordUserId);
    if (!preg_match('/^\d{15,22}$/', $did)) {
        throw new InvalidArgumentException('discord_user_id must be a Discord snowflake');
    }

    $lid = nsr_th_current_league($pdo);
    if ($lid <= 0) return ['member' => false];

    /* Several owner rows can share one Discord id (one person, two handles over
       the years). Any of them that holds a club in this league makes the person a
       member; lowest team id wins if somehow more than one does. */
    $st = $pdo->prepare(
        "SELECT ot.team_id
           FROM league_owners o JOIN owner_teams ot ON ot.owner_id = o.id
          WHERE o.discord_user_id = ? AND ot.league_id = ?
          ORDER BY ot.team_id LIMIT 1");
    $st->execute([$did, $lid]);
    $teamId = (int)($st->fetchColumn() ?: 0);

    $teams = nsr_teams($pdo, $lid);
    if ($teamId <= 0 || !isset($teams[$teamId])) return ['member' => false];
    $me = $teams[$teamId];

    $L = $pdo->prepare('SELECT name, season_label FROM leagues WHERE id = ?');
    $L->execute([$lid]);
    $lg = $L->fetch(PDO::FETCH_ASSOC) ?: ['name' => '', 'season_label' => null];
    $season = preg_match('/(\d+)/', (string)($lg['season_label'] ?? $lg['name']), $sm) ? (int)$sm[1] : null;

    $week    = nsr_th_week($pdo, $lid);
    $records = nsr_team_records($pdo, $lid);
    $rankOf  = nsr_team_standing($pdo, $lid);
    $owners  = nsr_th_owners($pdo, $lid);

    $mine = null;
    foreach (nsr_series($pdo, $lid, $week) as $s) {
        if ((int)$s['home_team_id'] === $teamId || (int)$s['away_team_id'] === $teamId) { $mine = $s; break; }
    }

    if ($mine === null) {
        $matchup = ['fixture_id' => null, 'week' => $week, 'home' => null, 'status' => 'bye',
                    'score' => null, 'series' => null, 'winner_team_id' => null, 'when' => null,
                    'opponent' => null];
    } else {
        $isHome = (int)$mine['home_team_id'] === $teamId;
        $oppId  = $isHome ? (int)$mine['away_team_id'] : (int)$mine['home_team_id'];
        $opp    = $teams[$oppId] ?? ['id' => $oppId, 'name' => '', 'abbrev' => ''];
        $matchup = [
            'fixture_id'     => (int)$mine['game_id'],
            'week'           => $week,
            'home'           => $isHome,
            'status'         => nsr_th_status($mine),
            'score'          => nsr_th_score($mine),
            'series'         => nsr_th_series($mine),
            'winner_team_id' => nsr_series_winner($mine),
            'when'           => null,
            'opponent'       => nsr_th_side($opp, $records) + [
                'owner_display'   => $owners[$oppId]['display'] ?? '',
                'discord_user_id' => $owners[$oppId]['discord'] ?? null,
            ],
        ];
    }

    return [
        'member' => true,
        'league' => [
            'id'         => $lid,
            'name'       => (string)$lg['name'],
            'season'     => $season,
            'week'       => $week,
            'week_label' => nsr_week_label($lid, $week),
        ],
        'team' => [
            'id'       => $teamId,
            'name'     => (string)$me['name'],
            'abbr'     => (string)($me['abbrev'] ?? ''),
            'logo_url' => nsr_th_url($me['logo_path'] ?? null),
            'color'    => (string)($me['primary_color'] ?? ''),
            'record'   => $records[$teamId] ?? '0-0',
            'standing' => $rankOf[$teamId] ?? null,
        ],
        'matchup' => $matchup,
    ];
}
```

- [ ] **Step 5: Run the tests**

Run: `php -l C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php; php C:\Users\Adam\nsr_local\tests\team_home_test.php`
Expected: `No syntax errors detected`, every line `PASS`, `ALL PASS`, exit 0.

If `team record` fails with `0-0`: the live `nsr_standings_rows` excludes weeks beyond `weeks_regular_season` or a playoff config; print `nsr_standings_rows($pdo,$lid)['rows']` and fix the fixture (not the code) so week 1 is a regular-season week.

- [ ] **Step 6: Record**

The site has no git. Copy the new files into the plan worktree as an audit trail and commit:

```powershell
$w="C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league\docs\superpowers\plans\site-src"
New-Item -ItemType Directory -Force "$w\_inc","$w\tests" | Out-Null
Copy-Item C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php "$w\_inc\"
Copy-Item C:\Users\Adam\nsr_local\tests\th_fixture.php,C:\Users\Adam\nsr_local\tests\team_home_test.php "$w\tests\"
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" add docs/superpowers/plans/site-src
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" commit -m "site: nsr_team_home core (membership, league, matchup) + tests"
```

---

### Task 3: Scoreboard, standings, recent, leaders, roster

**Files:**
- Modify: `C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php` (the `return` of `nsr_team_home`)
- Modify: `C:\Users\Adam\nsr_local\tests\team_home_test.php` (replace the `// --- payload checks (Task 3) ---` marker)

**Interfaces:**
- Consumes: Task 2's helpers (`nsr_th_side`, `nsr_th_status`, `nsr_th_score`, `nsr_th_series`, `nsr_th_url`) and locals `$lid, $teamId, $teams, $records, $week`.
- Produces: member payload keys `week_games, standings, recent, leaders, leaders_unit, roster, site_url` exactly as in the contract.

- [ ] **Step 1: Write the failing tests**

In `team_home_test.php`, replace the line `    // --- payload checks (Task 3) ---` with:

```php
    /* ---- the week's scoreboard ---- */
    $wg = $a['week_games'] ?? [];
    check('one fixture in week 2', count($wg) === 1);
    check('scoreboard fixture id', ($wg[0]['fixture_id'] ?? null) === $g['wk2ba']);
    check('scoreboard home is B', ($wg[0]['home']['team_id'] ?? null) === $t['B']);
    check('scoreboard status', ($wg[0]['status'] ?? null) === 'scheduled');
    check('scoreboard forfeit flag', ($wg[0]['forfeit'] ?? null) === false);
    check('club with no logo has a null logo_url', array_key_exists('logo_url', $wg[0]['home']) && $wg[0]['home']['logo_url'] === null);
    check('non-ASCII club name survives', ($wg[0]['home']['name'] ?? '') === 'Señor B');

    /* ---- standings ---- */
    $sd = $a['standings'] ?? [];
    check('four clubs in the table', count($sd) === 4);
    check('ranks run 1..4', array_column($sd, 'rank') === [1, 2, 3, 4]);
    $rowA = array_values(array_filter($sd, function ($r) use ($t) { return $r['team_id'] === $t['A']; }))[0] ?? [];
    check('A in table at 1-0', ($rowA['record'] ?? null) === '1-0');

    /* ---- recent results ---- */
    $rc = $a['recent'] ?? [];
    check('A has one result', count($rc) === 1);
    check('recent fixture', ($rc[0]['fixture_id'] ?? null) === $g['wk1ab']);
    check('recent opponent', ($rc[0]['opponent_abbr'] ?? null) === 'THB');
    check('recent home flag', ($rc[0]['home'] ?? null) === true);
    check('recent W', ($rc[0]['result'] ?? null) === 'W');
    check('recent points from A side', ($rc[0]['score'] ?? null) === '49-21');
    check('recent series from A side', ($rc[0]['series'] ?? null) === '2-0');

    $d = nsr_team_home($pdo, $did['D']);
    $rd = $d['recent'][0] ?? [];
    check('winner-only series: D result W', ($rd['result'] ?? null) === 'W');
    check('winner-only series: no score', array_key_exists('score', $rd) && $rd['score'] === null);

    /* ---- leaders: A's own players, touchdowns ---- */
    $ld = $a['leaders'] ?? [];
    check('leaders unit', ($a['leaders_unit'] ?? null) === 'TD');
    check('pass leader is poid1 with 3', ($ld['pass'][0]['poid'] ?? null) === $fx['poid'][1] && ($ld['pass'][0]['value'] ?? null) === 3);
    check('pass leader has a name', ($ld['pass'][0]['name'] ?? '') !== '');
    check('rush leader value 1', ($ld['rush'][0]['value'] ?? null) === 1);
    check('B receiver not in A leaders', ($ld['rec'] ?? null) === []);

    /* ---- roster ---- */
    $ro = $a['roster'] ?? [];
    check('three rostered', count($ro) === 3);
    check('roster names', count(array_filter($ro, function ($p) { return $p['name'] !== ''; })) === 3);
    check('roster numbers are int or null', count(array_filter($ro, function ($p) {
        return $p['number'] === null || is_int($p['number']); })) === 3);
    check('roster has position key', array_key_exists('position', $ro[0] ?? []));

    check('site url', ($a['site_url'] ?? '') === "https://nflstreetreborn.com/league/team.php?league_id=$lid&team_id={$t['A']}");
```

- [ ] **Step 2: Run to see the new checks fail**

Run: `php C:\Users\Adam\nsr_local\tests\team_home_test.php`
Expected: Task 2 checks PASS; the new checks FAIL (`one fixture in week 2`, `four clubs in the table`, ...); exit 1.

- [ ] **Step 3: Implement**

In `nsr_team_home.php`, add above `function nsr_team_home`:

```php
/** poid => jersey number (pool field PJEN), for the league's pinned pool. */
function nsr_th_numbers(PDO $pdo, int $leagueId): array {
    $st = $pdo->prepare("SELECT pool_version_id FROM leagues WHERE id = ?");
    $st->execute([$leagueId]);
    $vid = (int)$st->fetchColumn();
    if ($vid <= 0) return [];
    $q = $pdo->prepare("SELECT poid, JSON_UNQUOTE(JSON_EXTRACT(row_json, '$.PJEN')) AS n
                          FROM pool_csv_rows WHERE version_id = ?");
    $q->execute([$vid]);
    $out = [];
    foreach ($q->fetchAll(PDO::FETCH_ASSOC) as $r) {
        if ($r['n'] !== null && $r['n'] !== '' && is_numeric($r['n'])) $out[(int)$r['poid']] = (int)$r['n'];
    }
    return $out;
}

/** The rest of the page: scoreboard, table, results, leaders, roster. */
function nsr_th_page(PDO $pdo, int $lid, int $teamId, array $teams, array $records, int $week): array {
    $weekGames = [];
    foreach (nsr_series($pdo, $lid, $week) as $s) {
        $h = $teams[$s['home_team_id']] ?? null;
        $a = $teams[$s['away_team_id']] ?? null;
        if (!$h || !$a) continue;
        $weekGames[] = [
            'fixture_id'     => (int)$s['game_id'],
            'home'           => nsr_th_side($h, $records),
            'away'           => nsr_th_side($a, $records),
            'status'         => nsr_th_status($s),
            'forfeit'        => !empty($s['forfeit']),
            'score'          => nsr_th_score($s),
            'series'         => nsr_th_series($s),
            'winner_team_id' => nsr_series_winner($s),
            'when'           => null,
        ];
    }

    $standings = [];
    foreach (nsr_standings_rows($pdo, $lid)['rows'] as $i => $r) {
        $t = $teams[(int)$r['id']] ?? null;
        if (!$t) continue;
        $standings[] = [
            'rank'     => $i + 1,
            'team_id'  => (int)$t['id'],
            'name'     => (string)$t['name'],
            'abbr'     => (string)($t['abbrev'] ?? ''),
            'logo_url' => nsr_th_url($t['logo_path'] ?? null),
            'record'   => (int)$r['w'] . '-' . (int)$r['l'],
        ];
    }

    $done = array_values(array_filter(nsr_series($pdo, $lid), function ($s) use ($teamId) {
        return $s['complete']
            && ((int)$s['home_team_id'] === $teamId || (int)$s['away_team_id'] === $teamId);
    }));
    usort($done, function ($x, $y) {
        return [$y['week_num'], $y['game_id']] <=> [$x['week_num'], $x['game_id']];
    });
    $recent = [];
    foreach (array_slice($done, 0, 5) as $s) {
        $isHome = (int)$s['home_team_id'] === $teamId;
        $oppId  = $isHome ? (int)$s['away_team_id'] : (int)$s['home_team_id'];
        $won    = nsr_series_winner($s);
        $played = (int)$s['played'] > 0;
        $recent[] = [
            'fixture_id'    => (int)$s['game_id'],
            'week'          => (int)$s['week_num'],
            'opponent_abbr' => (string)($teams[$oppId]['abbrev'] ?? ''),
            'home'          => $isHome,
            'result'        => $won === null ? null : ($won === $teamId ? 'W' : 'L'),
            'score'         => $played
                ? ($isHome ? $s['home_points'] . '-' . $s['away_points'] : $s['away_points'] . '-' . $s['home_points'])
                : null,
            'series'        => $played
                ? ($isHome ? $s['home_wins'] . '-' . $s['away_wins'] : $s['away_wins'] . '-' . $s['home_wins'])
                : null,
        ];
    }

    $names = nsr_player_names($pdo, $lid);
    $rows  = array_filter(nsr_player_stat_rows($pdo, $lid), function ($r) use ($teamId) {
        return (int)$r['team_id'] === $teamId;
    });
    $leaders = [];
    foreach (['pass' => 'pass_td', 'rush' => 'rush_td', 'rec' => 'rec_td'] as $label => $key) {
        $leaders[$label] = [];
        foreach (nsr_rank_rows($rows, $key, 3) as $e) {
            $p = (int)$e['row']['poid'];
            $leaders[$label][] = ['poid' => $p, 'name' => (string)($names[$p] ?? ('Player #' . $p)),
                                  'value' => (int)$e['row'][$key]];
        }
    }

    $pos = nsr_player_positions($pdo, $lid);
    $num = nsr_th_numbers($pdo, $lid);
    $roster = [];
    foreach ((nsr_rosters($pdo, $lid)[$teamId] ?? []) as $p) {
        $roster[] = ['poid' => $p, 'name' => (string)($names[$p] ?? ('Player #' . $p)),
                     'position' => $pos[$p] ?? null, 'number' => $num[$p] ?? null];
    }
    $order = array_flip(['QB', 'RB', 'WR', 'OL', 'DL', 'LB', 'DB']);
    usort($roster, function ($x, $y) use ($order) {
        return [$order[$x['position']] ?? 9, $x['name']] <=> [$order[$y['position']] ?? 9, $y['name']];
    });

    return [
        'week_games'   => $weekGames,
        'standings'    => $standings,
        'recent'       => $recent,
        'leaders'      => $leaders,
        'leaders_unit' => 'TD',
        'roster'       => $roster,
        'site_url'     => NSR_PUBLIC_ORIGIN . '/league/team.php?league_id=' . $lid . '&team_id=' . $teamId,
    ];
}
```

Then change the end of `nsr_team_home` from `        'matchup' => $matchup,\n    ];` to:

```php
        'matchup' => $matchup,
    ] + nsr_th_page($pdo, $lid, $teamId, $teams, $records, $week);
```

- [ ] **Step 4: Run the tests**

Run: `php -l C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php; php C:\Users\Adam\nsr_local\tests\team_home_test.php`
Expected: `ALL PASS`, exit 0.

- [ ] **Step 5: Record**

```powershell
$w="C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league\docs\superpowers\plans\site-src"
Copy-Item C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php "$w\_inc\" -Force
Copy-Item C:\Users\Adam\nsr_local\tests\team_home_test.php "$w\tests\" -Force
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" add docs/superpowers/plans/site-src
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" commit -m "site: team_home scoreboard, standings, recent, leaders, roster"
```

---

### Task 4: `team_home` action in `api/bot.php`

**Files:**
- Modify: `C:\Users\Adam\nsr_local\public_html\league\api\bot.php` (insert above `fail('Unknown action: ' . $action);`; also extend the header comment)
- Create: `C:\Users\Adam\nsr_local\tests\team_home_http_test.php`

**Interfaces:**
- Consumes: `nsr_team_home()` (Tasks 2-3); bot.php's `$in`, `$pdo`, `$action`, `out()`, `fail()`.
- Produces: the HTTP contract at the top of this plan.

- [ ] **Step 1: Write the failing HTTP test**

`C:\Users\Adam\nsr_local\tests\team_home_http_test.php`:

```php
<?php
declare(strict_types=1);

/* Start the site first (separate PowerShell):
     php -S 127.0.0.1:8765 -t C:\Users\Adam\nsr_local\public_html
   then: php C:\Users\Adam\nsr_local\tests\team_home_http_test.php
   Uses the LOCAL replica token (_inc/ingest_token.php), never the live one. */

require_once __DIR__ . '/../public_html/league/_inc/ingest_token.php';

$url = 'http://127.0.0.1:8765/league/api/bot.php';
$fails = 0;
function check(string $what, bool $ok): void {
    global $fails;
    echo ($ok ? 'PASS ' : 'FAIL ') . $what . "\n";
    if (!$ok) $fails++;
}
/** [status, decoded body] */
function call(string $url, string $method, ?string $token, $body): array {
    $h = "Content-Type: application/json\r\n";
    if ($token !== null) $h .= "Authorization: Bearer $token\r\n";
    $ctx = stream_context_create(['http' => ['method' => $method, 'header' => $h,
        'content' => $body === null ? '' : json_encode($body), 'ignore_errors' => true]]);
    $raw = file_get_contents($url, false, $ctx);
    preg_match('~^HTTP/\S+\s+(\d+)~', $http_response_header[0] ?? '', $m);
    return [(int)($m[1] ?? 0), json_decode((string)$raw, true)];
}

[$s, $b] = call($url, 'POST', null, ['action' => 'team_home', 'discord_user_id' => '900000000000000099']);
check('no token -> 401', $s === 401);

[$s, $b] = call($url, 'GET', INGEST_TOKEN, null);
check('GET -> 405', $s === 405);

[$s, $b] = call($url, 'POST', INGEST_TOKEN, ['action' => 'team_home', 'discord_user_id' => 'abc']);
check('bad id -> 400', $s === 400 && ($b['ok'] ?? null) === false);

[$s, $b] = call($url, 'POST', INGEST_TOKEN, ['action' => 'team_home']);
check('missing id -> 400', $s === 400);

[$s, $b] = call($url, 'POST', INGEST_TOKEN, ['action' => 'team_home', 'discord_user_id' => '900000000000000099']);
check('unknown id -> 200 member:false', $s === 200 && $b === ['ok' => true, 'member' => false]);

/* A real local owner (ADAM). Locally his newest club may not be in the newest
   league, so only the shape is checked. */
[$s, $b] = call($url, 'POST', INGEST_TOKEN, ['action' => 'team_home', 'discord_user_id' => '1132855005523746906']);
check('real owner -> 200 ok with member flag', $s === 200 && ($b['ok'] ?? null) === true && is_bool($b['member'] ?? null));
if (!empty($b['member'])) {
    foreach (['league', 'team', 'matchup', 'week_games', 'standings', 'recent', 'leaders', 'leaders_unit', 'roster', 'site_url'] as $k)
        check("member payload has $k", array_key_exists($k, $b));
}

echo $fails ? "\n$fails FAILED\n" : "\nALL PASS\n";
exit($fails ? 1 : 0);
```

- [ ] **Step 2: Run to see it fail**

```powershell
Start-Process php -ArgumentList '-S','127.0.0.1:8765','-t','C:\Users\Adam\nsr_local\public_html' -WindowStyle Hidden -PassThru | Tee-Object -Variable srv | Out-Null
php C:\Users\Adam\nsr_local\tests\team_home_http_test.php
```

Expected: `no token -> 401` and `GET -> 405` PASS; `bad id -> 400` FAIL (current bot.php answers `400 Unknown action` — status matches but keep reading), `unknown id -> 200 member:false` FAIL; exit 1.

- [ ] **Step 3: Add the action**

In `bot.php`, insert directly above the line `fail('Unknown action: ' . $action);`:

```php
/* -------------------------------------------------------------- team_home -- */
/**
 * One member's club, for the Reborn launcher's League tab.
 *
 * Called by the Reborn backend, never by the launcher: the backend knows who is
 * signed in and passes that person's Discord id. Someone with no club in the
 * current league gets member:false. See _inc/nsr_team_home.php for the shape.
 */
if ($action === 'team_home') {
    require_once __DIR__ . '/../_inc/nsr_team_home.php';
    try {
        $home = nsr_team_home($pdo, (string)($in['discord_user_id'] ?? ''));
    } catch (InvalidArgumentException $e) {
        fail($e->getMessage());
    }
    /* A player name with a stray byte in the pool would make json_encode return
       false and the backend read an empty body; substitute instead. */
    echo json_encode(['ok' => true] + $home, JSON_INVALID_UTF8_SUBSTITUTE);
    exit;
}

```

And add to the header comment's action list (after the `stat` entry):

```php
 *   {"action":"team_home","discord_user_id":"..."}
 *       -> that person's club in the current league, for the launcher's League
 *          tab (via the Reborn backend), or {"ok":true,"member":false}
```

- [ ] **Step 4: Run the tests**

```powershell
php -l C:\Users\Adam\nsr_local\public_html\league\api\bot.php
php C:\Users\Adam\nsr_local\tests\team_home_http_test.php
php C:\Users\Adam\nsr_local\tests\team_home_test.php
Stop-Process -Id $srv.Id
```

Expected: no syntax errors; both suites `ALL PASS`.

- [ ] **Step 5: Record**

```powershell
$w="C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league\docs\superpowers\plans\site-src"
New-Item -ItemType Directory -Force "$w\api" | Out-Null
Copy-Item C:\Users\Adam\nsr_local\public_html\league\api\bot.php "$w\api\" -Force
Copy-Item C:\Users\Adam\nsr_local\tests\team_home_http_test.php "$w\tests\" -Force
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" add docs/superpowers/plans/site-src
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" commit -m "site: bot.php team_home action + HTTP tests"
```

---

### Task 5: Deploy to live and verify

**Files:**
- Create: `C:\Users\Adam\nsr_local\deploy\stage_league_tab\__lt_owner_count.php` (runner, deleted from live in this task)
- Create: `C:\Users\Adam\nsr_local\deploy\stage_league_tab\up_inc.txt`, `up_runner.txt`, `up_bot.txt`, `verify.txt`, `dele_runner.txt`
- Create: `C:\Users\Adam\nsr_local\deploy\secret\lt_runner_key.txt`

**Interfaces:**
- Consumes: the tested `_inc/nsr_team_home.php` and `api/bot.php` from Tasks 2-4; live's token in `deploy\secret\ingest_token.live.php` (Task 1).
- Produces: `team_home` live at `https://nflstreetreborn.com/league/api/bot.php`; the count of current-league owners missing a Discord ID; a rollback copy `deploy\stage_league_tab\live\bot.php`.

- [ ] **Step 1: Re-check live bot.php has not moved since Task 1**

```powershell
$s='C:\Users\Adam\nsr_local\deploy\stage_league_tab'
New-Item -ItemType Directory -Force "$s\recheck" | Out-Null
@"
option batch abort
option confirm off
open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com"
lcd "$s\recheck"
cd /jacobg44.sg-host.com/public_html/league/api
get bot.php
exit
"@ | Set-Content -Encoding ascii "$s\recheck.txt"
& 'C:\Program Files (x86)\WinSCP\WinSCP.com' /script="$s\recheck.txt"
(Get-FileHash "$s\recheck\bot.php").Hash -eq (Get-FileHash "$s\live\bot.php").Hash
```

Expected: `True`. If `False`, someone changed live bot.php: diff it, merge the `team_home` block onto the new live copy, re-run Task 4's tests, and only then continue.

- [ ] **Step 2: Upload the include (inert: nothing requires it yet) and verify**

```powershell
$s='C:\Users\Adam\nsr_local\deploy\stage_league_tab'
@"
option batch abort
option confirm off
open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com"
lcd "C:\Users\Adam\nsr_local\public_html\league\_inc"
cd /jacobg44.sg-host.com/public_html/league/_inc
put nsr_team_home.php
lcd "$s\recheck"
get nsr_team_home.php
exit
"@ | Set-Content -Encoding ascii "$s\up_inc.txt"
& 'C:\Program Files (x86)\WinSCP\WinSCP.com' /script="$s\up_inc.txt"
(Get-FileHash C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php).Hash -eq (Get-FileHash "$s\recheck\nsr_team_home.php").Hash
```

Expected: `True`.

- [ ] **Step 3: Write and run the owner-count runner**

```powershell
$key = -join ((1..32) | ForEach-Object { '{0:x}' -f (Get-Random -Maximum 16) })
Set-Content -Encoding ascii C:\Users\Adam\nsr_local\deploy\secret\lt_runner_key.txt $key
@"
<?php
declare(strict_types=1);
/* One-shot: how many clubs in the current league have an owner with no Discord id.
   Key-gated, 404 otherwise. DELETE FROM LIVE after use. */
if (!hash_equals('$key', (string)(`$_GET['key'] ?? ''))) { http_response_code(404); exit; }
require_once __DIR__ . '/_inc/config.php';
require_once __DIR__ . '/_inc/db.php';
require_once __DIR__ . '/_inc/nsr_team_home.php';
header('Content-Type: application/json');
`$pdo = db();
`$lid = nsr_th_current_league(`$pdo);
`$st = `$pdo->prepare("SELECT t.id, t.name, o.handle, o.discord_user_id
                        FROM teams t
                        LEFT JOIN owner_teams ot ON ot.league_id = t.league_id AND ot.team_id = t.id
                        LEFT JOIN league_owners o ON o.id = ot.owner_id
                       WHERE t.league_id = ? ORDER BY t.id");
`$st->execute([`$lid]);
`$rows = `$st->fetchAll(PDO::FETCH_ASSOC);
`$missing = array_values(array_filter(`$rows, function (`$r) { return trim((string)`$r['discord_user_id']) === ''; }));
echo json_encode(['league_id' => `$lid, 'clubs' => count(`$rows), 'missing_discord' => count(`$missing),
                  'missing' => array_map(function (`$r) { return `$r['name'] . ' / ' . (`$r['handle'] ?? 'no owner'); }, `$missing)]);
"@ | Set-Content -Encoding ascii C:\Users\Adam\nsr_local\deploy\stage_league_tab\__lt_owner_count.php
php -l C:\Users\Adam\nsr_local\deploy\stage_league_tab\__lt_owner_count.php
```

Upload, call, delete (keep these three in one PowerShell step so the runner never lingers):

```powershell
$s='C:\Users\Adam\nsr_local\deploy\stage_league_tab'; $key = Get-Content C:\Users\Adam\nsr_local\deploy\secret\lt_runner_key.txt
@"
option batch abort
option confirm off
open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com"
lcd "$s"
cd /jacobg44.sg-host.com/public_html/league
put __lt_owner_count.php
exit
"@ | Set-Content -Encoding ascii "$s\up_runner.txt"
@"
option batch abort
option confirm off
open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com"
cd /jacobg44.sg-host.com/public_html/league
call DELE __lt_owner_count.php
exit
"@ | Set-Content -Encoding ascii "$s\dele_runner.txt"
& 'C:\Program Files (x86)\WinSCP\WinSCP.com' /script="$s\up_runner.txt"
try {
  $nc = Get-Random
  Invoke-RestMethod "https://nflstreetreborn.com/league/__lt_owner_count.php?key=$key&nc=$nc" | ConvertTo-Json -Depth 4
} finally {
  & 'C:\Program Files (x86)\WinSCP\WinSCP.com' /script="$s\dele_runner.txt"
}
```

Expected: `league_id` = 36, `clubs` = 12, `missing_discord` = 0 (Adam confirmed every Season 4 owner is mapped). If `league_id` is not 36, STOP: a newer non-hidden league exists live and the tab would point at it — ask Adam before uploading bot.php. If `missing_discord` > 0, report the listed clubs to Adam; not a blocker for the upload.

Confirm the DELE worked: `Invoke-WebRequest "https://nflstreetreborn.com/league/__lt_owner_count.php?key=$key&nc=$(Get-Random)" -SkipHttpErrorCheck` must give `404` (PowerShell 5.1: wrap in try/catch and read `$_.Exception.Response.StatusCode`).

- [ ] **Step 4: Upload bot.php and verify the bytes**

```powershell
$s='C:\Users\Adam\nsr_local\deploy\stage_league_tab'
@"
option batch abort
option confirm off
open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com"
lcd "C:\Users\Adam\nsr_local\public_html\league\api"
cd /jacobg44.sg-host.com/public_html/league/api
put bot.php
lcd "$s\recheck"
get bot.php bot.uploaded.php
exit
"@ | Set-Content -Encoding ascii "$s\up_bot.txt"
& 'C:\Program Files (x86)\WinSCP\WinSCP.com' /script="$s\up_bot.txt"
(Get-FileHash C:\Users\Adam\nsr_local\public_html\league\api\bot.php).Hash -eq (Get-FileHash "$s\recheck\bot.uploaded.php").Hash
```

Expected: `True`.

- [ ] **Step 5: Call the live action**

The token is read from the pulled live file and never printed:

```powershell
$src = Get-Content -Raw C:\Users\Adam\nsr_local\deploy\secret\ingest_token.live.php
if ($src -notmatch "define\(\s*'INGEST_TOKEN'\s*,\s*'([^']+)'") { throw 'token not found' }
$tok = $Matches[1]
$u = 'https://nflstreetreborn.com/league/api/bot.php'
function Call($body, $auth) {
  $h = @{}; if ($auth) { $h['Authorization'] = "Bearer $auth" }
  try { $r = Invoke-WebRequest $u -Method Post -Headers $h -ContentType 'application/json' -Body ($body | ConvertTo-Json) -UseBasicParsing
        "$($r.StatusCode) $($r.Content.Substring(0, [Math]::Min(400, $r.Content.Length)))" }
  catch { "$([int]$_.Exception.Response.StatusCode) (error)" }
}
Call @{action='team_home'; discord_user_id='1132855005523746906'} $null
Call @{action='team_home'; discord_user_id='900000000000000099'} $tok
Call @{action='team_home'; discord_user_id='1132855005523746906'} $tok
```

Expected, in order:
1. `401 (error)`
2. `200 {"ok":true,"member":false}`
3. `200 {"ok":true,"member":true,"league":{"id":36,"name":"Reborn Season 4",...` with `"team":{"id":493,"name":"Subway Surfers"` (Adam's club per the Season 4 draft order).

Also spot-check one other existing action still works (regression): `Call @{action='stat_schema'} $tok` → `200 {"ok":true,"metrics":...`.

- [ ] **Step 6: Rollback recipe (only if Step 5 shows a regression)**

```
open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com"
lcd "C:\Users\Adam\nsr_local\deploy\stage_league_tab\live"
cd /jacobg44.sg-host.com/public_html/league/api
put bot.php
```

`_inc/nsr_team_home.php` can stay: nothing else requires it.

- [ ] **Step 7: Record the deploy**

Append to `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league\docs\superpowers\plans\2026-10-08-league-tab-1-site.diff-notes.txt`: the date/time, the SHA-256 of the uploaded `bot.php` and `nsr_team_home.php`, the runner's `missing_discord` result, and the three Step 5 status lines (no token values). Commit:

```powershell
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" add docs/superpowers/plans
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" commit -m "site: team_home deployed live (record)"
```

---

## Self-review (done while writing)

- Spec coverage: `team_home` action on bot.php with `INGEST_TOKEN` (Task 4); resolution owners → owner_teams → current non-hidden league, newest wins, one team (Task 2); member:false (Task 2); every payload block (Tasks 2-3); queries reused from the site helpers rather than duplicated (`_inc/nsr_team_home.php`); pre-flight count as a sanity check (Task 5 Step 3); pull live first (Task 1); WinSCP single files (Task 5).
- Placeholders: none; the runner key is generated at run time by the shown script.
- Names consistent across tasks: `nsr_team_home`, `nsr_th_current_league`, `nsr_th_week`, `nsr_th_side`, `nsr_th_status`, `nsr_th_score`, `nsr_th_series`, `nsr_th_owners`, `nsr_th_numbers`, `nsr_th_page`, `NSR_PUBLIC_ORIGIN`, `th_fixture`.
- Review Focus items each have a check: shared Discord id (owner F), finished-week advance, older-season owner (E), null logo, winner-only series (C-D).
