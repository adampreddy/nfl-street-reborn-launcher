# League Tab — Plan 2 (v2): Reborn Backend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the launcher's League tab its server half: a members-only team page (`GET /v1/league/me`), a fixture-locked best-of-3 league room on the league seed (`POST /v1/league/room`), members-only friendlies on the League edition (`POST /v1/league/friendly`), members-only short-lived download links for the League edition's files (`POST /v1/league/download`), and edition/version gates on joining.

**Architecture:** A `src/league/` module: `site.js` (server-to-server client for the league site's `team_home`, 60 s cache, `LEAGUE_ID`, one retry on SiteGround's first-request 403), `release.js` (reads the signed public `release.json` + pinned component manifests for `league-game`/`league-content`), `presign.js` (AWS SigV4 query presign for the private R2 bucket), `state.js` + `merge.js` (live overlay), `room.js` (fixture room), `friendly.js` (friendly room), `router.js` (routes + logo proxy). League and friendly rooms are ordinary `mm_room` rows with new columns (`edition`, `ruleset`, `league_game_version`, `league_*`, `allowed_discord_ids`); join/ready/start/leave and the whole match/series/relay path are the existing room machinery, taught to gate `edition:"league"` rooms and to copy edition/ruleset/version onto the match.

**Tech Stack:** Node 22, ESM, Express 5, Postgres (`pg`), `node:test`, `node:crypto` (Ed25519 verify, HMAC-SHA256); Railway service `reborn-api`. League site: PHP 7.4-compatible, WinSCP deploy.

**Spec:** `docs/superpowers/specs/2026-10-08-league-tab-design.md` (v2) in the launcher repo, branch `feat/league-tab`. **Binding contracts:** `docs/superpowers/plans/2026-10-08-league-contracts.md`. Site JSON contract: top of `docs/superpowers/plans/2026-10-08-league-tab-1-site.md` (final: `schedule_published`, `matchup.forfeit`, nullable `opponent.discord_user_id`, `winner_team_id` = series leader mid-series, final only when `status=="final"`).

## Global Constraints

- Backend work happens in a new worktree `C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-league` on branch `feat/league-tab` cut from `Reborn Backend` `main` (062e1ff or newer). Never commit in `Reborn Backend` itself (shared-worktree law).
- Migration file `migrations/029_league_room.sql`. Before creating it, list `migrations/` on `main` AND every `reborn-backend-*` worktree; if 029 is taken anywhere, use the next free number everywhere this plan says 029. Every migration begins `SET LOCAL lock_timeout = '5s';` and carries a Rollback comment (house style, see 028).
- Identity is never read from a body: every player route uses `requirePlayer` and `req.player`.
- Lock order law (`src/mm/locks.js`): ROOM(2) -> MATCH(3) -> PLAYER(4). The fixture lock is a ROOM-class key `league-fixture:<id>` taken before any room/player key.
- Edition values `"retail" | "league"`. Ruleset values `"casual" | "ranked" | "league" | "raw"`. Seeds: `league` -> `league.s01` (from `league-content`), `ranked` -> `ranked.s01`, `raw` -> none, `casual` -> the launcher's existing casual behaviour.
- League game rules: `mode='casual'` (no Elo), `source='league'`, `best_of=3` (`casual_best_of` copied as 3), `edition='league'`, `ruleset='league'`, seed kind `regular`, capacity 2, TTL 30 min (`LEAGUE_ROOM_TTL_MS = 1_800_000`).
- Friendly rules: `mode='casual'`, `source='room'`, `best_of=1`, `edition='league'`, `ruleset` = caller's `"league"` (default) or `"raw"`, seed kind `regular`, capacity 2, visibility `code`, TTL = `ROOM_TTL_MS` (rooms.js, 2 h). Not fixture-tagged.
- Room tagging uses `matchup.week` (the per-team week from Plan 1's fix), never `league.week`.
- Site call: `POST ${LEAGUE_SITE_URL}`, `Authorization: Bearer ${LEAGUE_SITE_TOKEN}`, body `{"action":"team_home","discord_user_id":"<snowflake>"[,"league_id":<LEAGUE_ID>]}`, 8 s timeout, one immediate retry on HTTP 403 (SiteGround's first-request-after-idle block). Non-200, non-JSON, `ok!==true`, missing boolean `member` => `SiteUnavailable` => 503 `league_unavailable`. Never `member:false` on an outage. Discord IDs are strings.
- Per-player cache of the site answer: 60 s. Writes (`/v1/league/room`) fetch fresh. `/v1/league/friendly`, `/v1/league/download` and the join gate may use the cache.
- Logo proxy only fetches `https://nflstreetreborn.com/league/` URLs, image content types, <= 1 MiB, cached 24 h.
- Release/manifest: `${LEAGUE_ORIGIN_BASE}release.json` + `release.json.sig` (128 hex), verified with Ed25519 against `RELEASE_PUBLIC_KEYS` (comma-separated 64-hex keys; default = the launcher's compiled keys `eac9ecc7a5f4e35234a23aa9b11d22b2f2372ef25e9c2ea9e1aa7458acb88fe9,71a34c792493cf9fe0de67f3ff7e202b5eebdae1729382bb165f38ca2aa9457b`). Component manifests are pinned by sha256 in the release. Cached 5 min.
- Members-only blobs live in a PRIVATE R2 bucket (`LEAGUE_BLOB_BUCKET`, default `reborn-league`, no public custom domain) at the origin layout `blobs/by-hash/<sha[0..2]>/<sha>`. Download URLs are SigV4 query-presigned GETs on `https://${R2_ACCOUNT_ID}.r2.cloudflarestorage.com/<bucket>/<key>`, region `auto`, service `s3`, expiry `LEAGUE_DOWNLOAD_TTL_S = 21600` (6 h; a 1.5 GB ISO on a slow line must finish; the launcher re-asks on a 403).
- All new env vars are optional (never through `need()` in server.js): `LEAGUE_SITE_URL`, `LEAGUE_SITE_TOKEN`, `LEAGUE_ID`, `LEAGUE_ORIGIN_BASE` (default `https://files.nflstreetreborn.com/`), `RELEASE_PUBLIC_KEYS`, `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, `LEAGUE_BLOB_BUCKET`. Unset site => `/v1/league/*` 503; unset R2 => `/v1/league/download` 503.
- Secrets (site token, R2 keys) are never logged, printed or committed.
- Tests: `npm test` (needs `DATABASE_URL_TEST`, see README). One file: `node --env-file-if-exists=.env --test test/<file>.test.js`.
- Site PHP (Task 1): 7.4-compatible syntax (no `match`, `?->`, named args, `str_contains`/`str_starts_with`, union types, `readonly`); no DDL; test teardown `th_cleanup()` (never `rollBack()` — `get_playoff_config` runs DDL); deploy from PowerShell only, WinSCP stored session `edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com`, never `/ini=nul`, one `put` per file + `get` back + SHA-256 compare, rollback copies first.
- No bot feed / opponent ping in v1 (Adam declined).

## Launcher-facing contract (what Plans 3a/3b consume)

### `GET /v1/league/me` (bearer)

- 401 signed out (requirePlayer). 503 `{ok:false,error:"league_unavailable"}` when the site is unset/unreachable/odd. 200 `{ok:true,member:false}` for non-members.
- 200 member: the Plan 1 site payload, plus:
  - every non-null `logo_url` -> `${PUBLIC_ORIGIN}/live-assets/league/<team_id>.png`;
  - `week_games[].status` / `matchup.status` may be `"live"` (league match running); an open league room for a fixture the site calls `final` reads `"in_progress"`;
  - `week_games[]` and `matchup` gain `match_id` (string|null); when live, `series {home,away}` and `score {home,away}|null` override the site's;
  - `matchup.room`: `null` or `{room_id, code, host_discord_user_id, members, expires_at, state, league_game_version}` with `state` ∈ `"waiting" | "lobby" | "in_series"` (in_series: `room_id`/`code` null, `match_id` set);
  - `matchup.opponent.online`: boolean (`last_used_at` within 90 s, hidden presence reads false);
  - top-level `edition_versions: {"league-game": string|null, "league-content": string|null}` (latest published; null when the release cannot be read — never fails the call).

### `POST /v1/league/room` (bearer, body `{league_game_version}`)

The launcher always sends its installed version (contracts ruling). The launcher's `league_room_open()` maps 200 `created`/`join` and 409 `in_series` to `{action:"created"|"join"|"in_series", room?, code?, match_id?}`, and 409 `league_version_mismatch` to `{kind:"league_update_required", required}`.

Re-reads the caller's fixture from the site (fresh). The room's `league_game_version` is the body's string when given, else the latest published `league-game` version (null if unknown).

| Case | Status | Body |
|---|---|---|
| not a member | 403 | `{ok:false,error:"not_member"}` |
| bye / no fixture | 409 | `{ok:false,error:"no_game"}` |
| site says final and nothing open/running | 409 | `{ok:false,error:"already_final"}` |
| opponent has no Discord ID on the site | 409 | `{ok:false,error:"opponent_unlinked"}` |
| a league match for this fixture is running | 409 | `{ok:false,error:"in_series",match_id}` |
| caller already in another lobby / seated | 409 | existing sentences (`You are already in a lobby.` / `You are already in a match.`) |
| no open room -> created, caller is host | 200 | `{ok:true,action:"created",room:<roomView>}` |
| caller already hosts the open room | 200 | `{ok:true,action:"created",room:<roomView>}` |
| opponent's room is open, caller's version matches (or none sent) | 200 | `{ok:true,action:"join",code:"K7QP2M",league_game_version}` |
| opponent's room is open, caller sent a different version | 409 | `{ok:false,error:"league_version_mismatch",required:"<room version>"}` |
| site unreachable | 503 | `{ok:false,error:"league_unavailable"}` |

### `POST /v1/league/friendly` (bearer, body `{ruleset?: "league"|"raw", league_game_version?: string}`)

- 403 `not_member`; 400 `{ok:false,error:"ruleset must be league or raw."}`; 409 existing lobby/match sentences; 503 `league_unavailable`.
- 200 `{ok:true,action:"created",room:<roomView>}` — private code room, `edition:"league"`, `ruleset` as asked (default `"league"`), `best_of:1`, `league:null`, `league_game_version` = body string or latest published or null.

### `POST /v1/league/download` (bearer, body `{component, version}`)

- `component` ∈ `"league-game" | "league-content"`, else 400 `{ok:false,error:"component must be league-game or league-content."}`.
- 403 `not_member`; 404 `{ok:false,error:"unknown_version"}` when `version` is not the currently published one; 503 `league_unavailable` (site, release, or R2 not configured/unreachable).
- 200 `{ok:true, component, version, files:[{path, url, sha256, size}], expires_at}`. `url` is a presigned R2 GET valid for 6 h; Range requests work on it.

### Rooms (existing routes)

- `roomView` gains `edition` (`"retail"|"league"`), `ruleset` (string|null), `league_game_version` (string|null), `league` (`{league_id,fixture_id,week}`|null).
- Join: `POST /v1/mm/rooms/:code/join`, body `{league_game_version}` (plus the existing `party` flag); the field is ignored for retail rooms. For an `edition:"league"` room:
  - fixture rooms: Discord ID not in `allowed_discord_ids` -> 404 `No such lobby.`;
  - friendly rooms: caller not a league member -> 404 `No such lobby.`; site down -> 503 `league_unavailable`;
  - room has a `league_game_version` and the body's differs or is missing -> 409 `{ok:false,error:"league_version_mismatch",required:"<version>"}`.
- Peek `GET /v1/rooms/:code`: same membership/allowed gate (404), no version check; answer gains `edition`.
- `kind` on a fixture room -> 409 `A league game's type is fixed.`
- Leave/cancel: existing `POST /v1/mm/rooms/:id/leave` (host leaving closes it).
- Match snapshot: `snapshot.room.{league,edition,ruleset,league_game_version}`; `snapshot.match.league` (`{league_id,fixture_id,week}`|null), `snapshot.match.edition`, `snapshot.match.ruleset` (`match.ruleset` when set, else `"ranked"` for ranked mode, else `"casual"`), `snapshot.match.league_game_version`. The launcher boots the ISO/game ID from `edition` and the seed from `ruleset`, and sets `intent.league=true` + `intent.fixture_id` on uploads of league matches so `forward.js` forwards them.

## Review Focus

1. **Both players click Play at the same instant** — exactly one room; one `created`, one `join`. Pinned in Task 7 (fixture key held from outside).
2. **A stranger (or a non-member) with a league room's code** — join and peek must look exactly like a bad code (404). Fixture rooms pinned in Task 6; friendly rooms pinned in Task 6 (non-member) and Task 8.
3. **League files out of date on one side** — a joiner on another `league-game` version is refused with `league_version_mismatch` + `required` before taking a seat. Pinned in Task 6.
4. **A presigned URL that S3/R2 would reject** — a wrong canonical request silently produces 403s for every member. Pinned in Task 4 against AWS's published presign example.
5. **Site or release down** — `/v1/league/me` must 503 on site outage (never `member:false`) and must still answer (with `edition_versions` nulls) when only the release is unreadable; `/v1/league/download` 503s. Pinned in Tasks 3, 9, 10.

---

## File map

| File | Change | Responsibility |
|---|---|---|
| `C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php` | modify | optional explicit `league_id` |
| `C:\Users\Adam\nsr_local\public_html\league\api\bot.php` | modify | pass `league_id` through |
| `C:\Users\Adam\nsr_local\tests\team_home_test.php` | modify | league_id checks |
| `migrations/029_league_room.sql` | create | league/edition/ruleset/version columns on `mm_room` + `match`, one-open-room-per-fixture index |
| `src/league/site.js` | create | site client: `teamHome`, `isMember`, cache, 403 retry, `logoSource` |
| `src/league/release.js` | create | signed release + pinned component manifests, `versions()`, `component()` |
| `src/league/presign.js` | create | SigV4 query presign, `r2BlobUrl()` |
| `src/league/state.js` | create | open league rooms + running league matches per fixture; presence |
| `src/league/merge.js` | create | pure site + state -> launcher payload |
| `src/league/room.js` | create | `openLeagueRoom` (fixture-locked) |
| `src/league/friendly.js` | create | `openFriendlyRoom` |
| `src/league/router.js` | create | `/v1/league/me|room|friendly|download`, `/live-assets/league/:file` |
| `src/mm/match.js` | modify | `createMatch` league/edition/ruleset/version; `MATCH_COLUMNS` |
| `src/mm/snapshot.js` | modify | room/match league, edition, ruleset, version |
| `src/mm/rooms.js` | modify | export `assertNotSeated`; views + lockedRoom columns; edition gate on join/peek; kind refusal; start copies league/edition/ruleset/version; `leagueMember` option |
| `src/mm/index.js` | modify | pass `leagueMember` to `roomsRouter` |
| `src/app.js`, `src/server.js`, `README.md` | modify | wiring + env |
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
Expected: the `main` head (062e1ff or newer).

- [ ] **Step 2: Install and baseline**

```powershell
npm ci
npm test
```
Expected: all green; record the count. A red clean `main` stops the plan: report it.

- [ ] **Step 3: Pick the migration number**

```powershell
Get-ChildItem "C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend\migrations","C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-*\migrations" -Filter "02[5-9]*" | Select-Object FullName
Get-ChildItem "C:\Users\Adam\Desktop\Reborn Installer\reborn-backend-*\migrations" -Filter "03*" | Select-Object FullName
```
If `029_*` exists in any other worktree, use the next free number in every later step.

---

### Task 1: Site follow-up — `team_home` accepts an explicit `league_id`

The backend pins the league with `LEAGUE_ID` so creating a test or next-season league never flips every member's tab (Plan 1 final review I2). The site still falls back to "newest non-hidden" when `league_id` is absent, so the live behaviour does not change until the backend sends it.

**Files:**
- Modify: `C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php` (`nsr_team_home` signature + league pick, lines ~251-258)
- Modify: `C:\Users\Adam\nsr_local\public_html\league\api\bot.php` (team_home block, ~929-940)
- Modify: `C:\Users\Adam\nsr_local\tests\team_home_test.php` (the "no schedule at all" block before `} finally {`)
- Create: `C:\Users\Adam\nsr_local\deploy\stage_league_tab\v3\` (rollback copies + scripts)

**Interfaces:**
- Produces: `nsr_team_home(PDO $pdo, string $discordUserId, ?int $leagueId = null): array`. `$leagueId` given => that league (must exist, else `InvalidArgumentException('league_id is not a league')` => HTTP 400). bot.php reads `$in['league_id']` (positive int or absent; anything else => 400 `league_id must be a positive whole number`).

- [ ] **Step 1: Write the failing tests**

In `team_home_test.php`, directly above the closing `} finally {` (after the `empty season_label falls back to the league name` check), add:

```php
    /* ---- explicit league_id: the backend pins the league (LEAGUE_ID) ---- */
    // lid2 (the empty league) is now the newest non-hidden one, so the fallback picks it...
    check('fallback picks the newest league', nsr_th_current_league($pdo) === $fx['lid2']);
    check('fallback: A is not a member of the newest league', nsr_team_home($pdo, $did['A']) === ['member' => false]);
    // ...but an explicit league_id pins the fixture league whatever is newer.
    $p = nsr_team_home($pdo, $did['A'], $lid);
    check('pinned: A is a member', ($p['member'] ?? null) === true);
    check('pinned: league is the fixture league', ($p['league']['id'] ?? null) === $lid);
    check('pinned: A team', ($p['team']['id'] ?? null) === $t['A']);
    $threw = '';
    try { nsr_team_home($pdo, $did['A'], 999999999); } catch (InvalidArgumentException $e) { $threw = $e->getMessage(); }
    check('unknown league_id is refused, not member:false', $threw === 'league_id is not a league');
```

- [ ] **Step 2: Run to verify failure**

Run (PowerShell): `php C:\Users\Adam\nsr_local\tests\team_home_test.php`
Expected: the new `pinned:` checks FAIL (3rd argument ignored) and `unknown league_id is refused` FAILs; exit 1. Then confirm no fixture rows remain:

```powershell
php -r "require 'C:/Users/Adam/nsr_local/public_html/league/_inc/config.php'; require 'C:/Users/Adam/nsr_local/public_html/league/_inc/db.php'; echo db()->query(\"SELECT COUNT(*) FROM leagues WHERE name LIKE 'TH %'\")->fetchColumn(), PHP_EOL;"
```
Expected: `0`.

- [ ] **Step 3: Implement in `nsr_team_home.php`**

Replace:

```php
function nsr_team_home(PDO $pdo, string $discordUserId): array {
    $did = trim($discordUserId);
    if (!preg_match('/^\d{15,22}$/', $did)) {
        throw new InvalidArgumentException('discord_user_id must be a Discord snowflake');
    }

    $lid = nsr_th_current_league($pdo);
    if ($lid <= 0) return ['member' => false];
```

with:

```php
/**
 * @param int|null $leagueId  the league the caller pins (the Reborn backend's LEAGUE_ID), or null for the
 *                            newest non-hidden league. A pinned league that does not exist is a caller error,
 *                            never member:false: a misconfigured backend must not hide the tab from members.
 */
function nsr_team_home(PDO $pdo, string $discordUserId, ?int $leagueId = null): array {
    $did = trim($discordUserId);
    if (!preg_match('/^\d{15,22}$/', $did)) {
        throw new InvalidArgumentException('discord_user_id must be a Discord snowflake');
    }

    if ($leagueId !== null) {
        $q = $pdo->prepare('SELECT id FROM leagues WHERE id = ?');
        $q->execute([$leagueId]);
        $lid = (int)($q->fetchColumn() ?: 0);
        if ($lid <= 0) throw new InvalidArgumentException('league_id is not a league');
    } else {
        $lid = nsr_th_current_league($pdo);
    }
    if ($lid <= 0) return ['member' => false];
```

- [ ] **Step 4: Implement in `bot.php`**

In the `team_home` block, replace:

```php
        $home = nsr_team_home($pdo, (string)($in['discord_user_id'] ?? ''));
```

with:

```php
        $leagueId = null;
        if (array_key_exists('league_id', $in) && $in['league_id'] !== null) {
            if (!is_int($in['league_id']) && !(is_string($in['league_id']) && ctype_digit($in['league_id']))) {
                throw new InvalidArgumentException('league_id must be a positive whole number');
            }
            $leagueId = (int)$in['league_id'];
            if ($leagueId <= 0) throw new InvalidArgumentException('league_id must be a positive whole number');
        }
        $home = nsr_team_home($pdo, (string)($in['discord_user_id'] ?? ''), $leagueId);
```

and in the header comment change the team_home line to:

```php
 *   {"action":"team_home","discord_user_id":"...","league_id":36?}
```

- [ ] **Step 5: Run the tests**

```powershell
php -l C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php
php -l C:\Users\Adam\nsr_local\public_html\league\api\bot.php
php C:\Users\Adam\nsr_local\tests\team_home_test.php
$srv = Start-Process php -ArgumentList '-S','127.0.0.1:8766','-t','C:\Users\Adam\nsr_local\public_html' -WindowStyle Hidden -PassThru
$env:TH_URL = 'http://127.0.0.1:8766/league/api/bot.php'
php C:\Users\Adam\nsr_local\tests\team_home_http_test.php
Stop-Process -Id $srv.Id
```
Expected: no syntax errors; `ALL PASS` twice; then the leftover-row check from Step 2 prints `0`.

- [ ] **Step 6: Deploy the two files (existing site-action approval; rollback copies first)**

```powershell
$s='C:\Users\Adam\nsr_local\deploy\stage_league_tab\v3'
New-Item -ItemType Directory -Force "$s\before","$s\after" | Out-Null
@"
option batch abort
option confirm off
open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com"
lcd "$s\before"
cd /jacobg44.sg-host.com/public_html/league/_inc
get nsr_team_home.php
cd /jacobg44.sg-host.com/public_html/league/api
get bot.php
exit
"@ | Set-Content -Encoding ascii "$s\get_before.txt"
& 'C:\Program Files (x86)\WinSCP\WinSCP.com' /script="$s\get_before.txt"
# live must still be exactly what Plan 1 deployed:
(Get-FileHash "$s\before\nsr_team_home.php").Hash.StartsWith('F32959A3')
(Get-FileHash "$s\before\bot.php").Hash.StartsWith('752404AC')
```
Expected: `True` twice. If either is `False`, STOP: someone changed live since Plan 1; diff and report, do not upload.

```powershell
$s='C:\Users\Adam\nsr_local\deploy\stage_league_tab\v3'
@"
option batch abort
option confirm off
open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com"
lcd "C:\Users\Adam\nsr_local\public_html\league\_inc"
cd /jacobg44.sg-host.com/public_html/league/_inc
put nsr_team_home.php
lcd "C:\Users\Adam\nsr_local\public_html\league\api"
cd /jacobg44.sg-host.com/public_html/league/api
put bot.php
lcd "$s\after"
cd /jacobg44.sg-host.com/public_html/league/_inc
get nsr_team_home.php
cd /jacobg44.sg-host.com/public_html/league/api
get bot.php
exit
"@ | Set-Content -Encoding ascii "$s\up.txt"
& 'C:\Program Files (x86)\WinSCP\WinSCP.com' /script="$s\up.txt"
(Get-FileHash C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php).Hash -eq (Get-FileHash "$s\after\nsr_team_home.php").Hash
(Get-FileHash C:\Users\Adam\nsr_local\public_html\league\api\bot.php).Hash -eq (Get-FileHash "$s\after\bot.php").Hash
```
Expected: `True` twice. If the host does not resolve, add `-rawsettings HostName=ftp.nflstreetreborn.com`; if SiteGround refuses, ask Adam to turn the VPN off.

- [ ] **Step 7: Live checks (token read into a variable, never printed)**

```powershell
$src = Get-Content -Raw C:\Users\Adam\nsr_local\deploy\secret\ingest_token.live.php
if ($src -notmatch "define\(\s*'INGEST_TOKEN'\s*,\s*'([^']+)'") { throw 'token not found' }
$tok = $Matches[1]
$u = 'https://nflstreetreborn.com/league/api/bot.php'
function Call($body) {
  for ($i = 0; $i -lt 2; $i++) {
    try { $r = Invoke-WebRequest $u -Method Post -Headers @{Authorization="Bearer $tok"} -ContentType 'application/json' -Body ($body | ConvertTo-Json) -UseBasicParsing
          return "$($r.StatusCode) $($r.Content.Substring(0, [Math]::Min(160, $r.Content.Length)))" }
    catch { $c = [int]$_.Exception.Response.StatusCode; if ($c -ne 403 -or $i -eq 1) { return "$c (error)" } }
  }
}
Call @{action='team_home'; discord_user_id='1132855005523746906'}
Call @{action='team_home'; discord_user_id='1132855005523746906'; league_id=36}
Call @{action='team_home'; discord_user_id='1132855005523746906'; league_id=999999999}
Call @{action='stat_schema'}
```
Expected: `200 {"ok":true,"member":true,"league":{"id":36` twice; `400 (error)`; `200 {"ok":true,"metrics"`. On any regression put back `$s\before\*` with the same put/get/hash pattern and report.

- [ ] **Step 8: Record (launcher worktree)**

```powershell
$w="C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league\docs\superpowers\plans\site-src"
Copy-Item C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php "$w\_inc\" -Force
Copy-Item C:\Users\Adam\nsr_local\public_html\league\api\bot.php "$w\api\" -Force
Copy-Item C:\Users\Adam\nsr_local\tests\team_home_test.php "$w\tests\" -Force
Add-Content "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league\docs\superpowers\plans\2026-10-08-league-tab-1-site.diff-notes.txt" "$(Get-Date -Format s) Plan 2 Task 1: team_home league_id deployed; nsr_team_home.php $((Get-FileHash C:\Users\Adam\nsr_local\public_html\league\_inc\nsr_team_home.php).Hash) bot.php $((Get-FileHash C:\Users\Adam\nsr_local\public_html\league\api\bot.php).Hash); rollback copies stage_league_tab\v3\before"
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" add docs/superpowers/plans
git -C "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher league" commit -m "site: team_home accepts league_id (backend LEAGUE_ID pin), deployed`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Migration 029 — league, edition, ruleset and version columns

**Files:**
- Create: `migrations/029_league_room.sql`
- Test: `test/league_schema.test.js`

**Interfaces:**
- Produces columns on `mm_room` AND `match`: `league_id int`, `league_fixture_id bigint`, `league_week int`, `league_seat0_home boolean`, `edition text NOT NULL DEFAULT 'retail'` (check retail|league), `ruleset text` (check casual|ranked|league|raw), `league_game_version text`; `mm_room.allowed_discord_ids text[]`; unique partial index `mm_room_league_one_open`; index `match_league_fixture`.

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
const room = (pool, clock, host, fixture, { status = "open", edition = "league", ruleset = "league" } = {}) => pool.query(
  `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, status, created_at, expires_at,
                        league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids,
                        edition, ruleset, league_game_version)
   VALUES ($1, $2, 'casual', 'code', 3, $3, $4, $4, 36, $5, 6, true, ARRAY['1','2'], $6, $7, '2026.10.08')`,
  [`C${String(++n).padStart(5, "A")}`.slice(0, 6), host.id, status, clock.now(), fixture, edition, ruleset]);

test("029: one OPEN league room per fixture; closed and other fixtures are not limited", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const a = await player(t.pool); const b = await player(t.pool);
    await room(t.pool, clock, a, 1234);
    await assert.rejects(room(t.pool, clock, b, 1234), (e) => e.code === "23505" && /mm_room_league_one_open/.test(e.constraint));
    await room(t.pool, clock, b, 1234, { status: "closed" });
    await room(t.pool, clock, b, 9999);
  } finally { await t.close(); }
});

test("029: edition defaults to retail and only retail|league; ruleset only casual|ranked|league|raw", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const a = await player(t.pool);
    const plain = (await t.pool.query(
      `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, created_at, expires_at)
       VALUES ('PLAIN2', $1, 'casual', 'code', 1, $2, $2) RETURNING edition, ruleset, league_game_version`, [a.id, clock.now()])).rows[0];
    assert.deepEqual(plain, { edition: "retail", ruleset: null, league_game_version: null });
    await assert.rejects(room(t.pool, clock, a, 1, { edition: "beta" }), (e) => e.code === "23514");
    await assert.rejects(room(t.pool, clock, a, 2, { ruleset: "hard" }), (e) => e.code === "23514");
    const cols = (await t.pool.query(
      `SELECT table_name, column_name FROM information_schema.columns
        WHERE table_schema = $1 AND table_name IN ('match','mm_room')
          AND (column_name LIKE 'league_%' OR column_name IN ('edition','ruleset','allowed_discord_ids')) ORDER BY 1, 2`, [t.schema])).rows
      .map((r) => `${r.table_name}.${r.column_name}`);
    assert.deepEqual(cols, [
      "match.edition", "match.league_fixture_id", "match.league_game_version", "match.league_id", "match.league_seat0_home", "match.league_week", "match.ruleset",
      "mm_room.allowed_discord_ids", "mm_room.edition", "mm_room.league_fixture_id", "mm_room.league_game_version", "mm_room.league_id",
      "mm_room.league_seat0_home", "mm_room.league_week", "mm_room.ruleset",
    ]);
  } finally { await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_schema.test.js`
Expected: FAIL, `column "league_id" of relation "mm_room" does not exist`.

- [ ] **Step 3: Write the migration**

```sql
-- 029: league rooms and the League edition (spec docs/superpowers/specs/2026-10-08-league-tab-design.md v2,
-- launcher repo feat/league-tab).
--
-- * league_*: the league fixture a room was opened for / a match plays (league site ids; no FK, other database).
--   league_seat0_home: true when the room's host (seat 0 of its match) owns the fixture's HOME team.
-- * edition: which build of the game the room boots, 'retail' (every room before this) or 'league'.
-- * ruleset: which seed the launcher boots: casual | ranked (ranked.s01) | league (league.s01) | raw (none).
--   NULL = the pre-029 meaning (ranked mode -> ranked, else casual).
-- * league_game_version: the league-game component version both players must have (edition league only).
-- * mm_room.allowed_discord_ids: the only Discord IDs that may join or even see a fixture room by code.
--   NULL = no per-ID lock (every other room; friendlies are gated on league membership instead).
-- * mm_room_league_one_open: at most one OPEN room per fixture -- two players pressing Play at once get one room.
--
-- Rollback: DROP INDEX mm_room_league_one_open; DROP INDEX match_league_fixture;
-- ALTER TABLE mm_room DROP COLUMN league_id, DROP COLUMN league_fixture_id, DROP COLUMN league_week,
--   DROP COLUMN league_seat0_home, DROP COLUMN allowed_discord_ids, DROP COLUMN edition, DROP COLUMN ruleset,
--   DROP COLUMN league_game_version;
-- ALTER TABLE match DROP COLUMN league_id, DROP COLUMN league_fixture_id, DROP COLUMN league_week,
--   DROP COLUMN league_seat0_home, DROP COLUMN edition, DROP COLUMN ruleset, DROP COLUMN league_game_version;
SET LOCAL lock_timeout = '5s';

ALTER TABLE mm_room
  ADD COLUMN IF NOT EXISTS league_id int,
  ADD COLUMN IF NOT EXISTS league_fixture_id bigint,
  ADD COLUMN IF NOT EXISTS league_week int,
  ADD COLUMN IF NOT EXISTS league_seat0_home boolean,
  ADD COLUMN IF NOT EXISTS allowed_discord_ids text[],
  ADD COLUMN IF NOT EXISTS edition text NOT NULL DEFAULT 'retail',
  ADD COLUMN IF NOT EXISTS ruleset text,
  ADD COLUMN IF NOT EXISTS league_game_version text;

ALTER TABLE match
  ADD COLUMN IF NOT EXISTS league_id int,
  ADD COLUMN IF NOT EXISTS league_fixture_id bigint,
  ADD COLUMN IF NOT EXISTS league_week int,
  ADD COLUMN IF NOT EXISTS league_seat0_home boolean,
  ADD COLUMN IF NOT EXISTS edition text NOT NULL DEFAULT 'retail',
  ADD COLUMN IF NOT EXISTS ruleset text,
  ADD COLUMN IF NOT EXISTS league_game_version text;

ALTER TABLE mm_room ADD CONSTRAINT mm_room_edition_check CHECK (edition IN ('retail', 'league'));
ALTER TABLE mm_room ADD CONSTRAINT mm_room_ruleset_check CHECK (ruleset IS NULL OR ruleset IN ('casual', 'ranked', 'league', 'raw'));
ALTER TABLE match ADD CONSTRAINT match_edition_check CHECK (edition IN ('retail', 'league'));
ALTER TABLE match ADD CONSTRAINT match_ruleset_check CHECK (ruleset IS NULL OR ruleset IN ('casual', 'ranked', 'league', 'raw'));

CREATE UNIQUE INDEX IF NOT EXISTS mm_room_league_one_open
  ON mm_room (league_fixture_id) WHERE status = 'open' AND league_fixture_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS match_league_fixture
  ON match (league_fixture_id) WHERE league_fixture_id IS NOT NULL;
```

> If `src/db.js`'s migration runner re-runs files (no applied-migrations ledger), the `ADD CONSTRAINT` lines must be idempotent: wrap each in `DO $$ BEGIN ... EXCEPTION WHEN duplicate_object THEN NULL; END $$;`. Check how 028 is applied before choosing.

- [ ] **Step 4: Run the test, then the suite**

Run: `node --env-file-if-exists=.env --test test/league_schema.test.js` → PASS (2). `npm test` → baseline + 2, all green (a schema test that pins an exact column list gets the new columns appended; do not loosen it).

- [ ] **Step 5: Commit**

```powershell
git add migrations/029_league_room.sql test/league_schema.test.js
git commit -m "league: migration 029 (league/edition/ruleset/version columns, one open room per fixture)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Site client (`src/league/site.js`)

**Files:**
- Create: `src/league/site.js`
- Test: `test/league_site.test.js`

**Interfaces:**
- Produces: `leagueSite({ url, token, leagueId = null, fetchImpl = fetch, clock, ttlMs = 60_000 })` -> `{ teamHome(discordId, { fresh = false } = {}): Promise<object>, isMember(discordId): Promise<boolean>, logoSource(teamId): string|null }`; `class SiteUnavailable extends Error`; `TEAM_HOME_TTL_MS = 60_000`; `LOGO_ORIGIN = "https://nflstreetreborn.com/league/"`.

- [ ] **Step 1: Write the failing tests**

```js
// test/league_site.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { fixedClock, fakeFetch } from "./helpers/fakes.js";
import { leagueSite, SiteUnavailable } from "../src/league/site.js";

const URL_ = "https://nflstreetreborn.test/league/api/bot.php";
const member = {
  ok: true, member: true, schedule_published: true,
  league: { id: 36, name: "S4", season: 4, week: 6, week_label: "Week 6" },
  team: { id: 493, name: "Subway Surfers", abbr: "ADAM", logo_url: "https://nflstreetreborn.com/league/uploads/493.png", color: "#00A3FF", record: "4-1", standing: 2 },
  matchup: { fixture_id: 1234, week: 6, home: true, status: "scheduled", forfeit: false, score: null, series: null, winner_team_id: null, when: null,
    opponent: { team_id: 486, name: "Furies", abbr: "BLONDE", logo_url: null, color: "#FF0000", record: "3-2", owner_display: "BLONDE", discord_user_id: "480510938571735040" } },
  week_games: [], standings: [], recent: [], leaders: { pass: [], rush: [], rec: [] }, leaders_unit: "TD", roster: [],
  site_url: "https://nflstreetreborn.com/league/team.php?league_id=36&team_id=493",
};

test("teamHome posts the action (+ league_id when pinned) with the bearer and caches 60 s per Discord ID", async () => {
  const clock = fixedClock();
  const f = fakeFetch({ [URL_]: { body: JSON.stringify(member) } });
  const site = leagueSite({ url: URL_, token: "tok", leagueId: 36, fetchImpl: f, clock });
  assert.deepEqual(await site.teamHome("111"), member);
  const sent = f.calls[0];
  assert.equal(sent.init.method, "POST");
  assert.equal(sent.init.headers.Authorization, "Bearer tok");
  assert.deepEqual(JSON.parse(sent.init.body), { action: "team_home", discord_user_id: "111", league_id: 36 });
  await site.teamHome("111");
  assert.equal(f.calls.length, 1, "cached inside 60 s");
  clock.advance(60_000);
  await site.teamHome("111");
  assert.equal(f.calls.length, 2, "expired after 60 s");
  await site.teamHome("111", { fresh: true });
  assert.equal(f.calls.length, 3, "fresh always asks");
  assert.equal(await site.isMember("111"), true);
});

test("no league_id key at all when unpinned", async () => {
  const f = fakeFetch({ [URL_]: { body: '{"ok":true,"member":false}' } });
  const site = leagueSite({ url: URL_, token: "tok", fetchImpl: f, clock: fixedClock() });
  assert.deepEqual(await site.teamHome("222"), { ok: true, member: false });
  assert.deepEqual(JSON.parse(f.calls[0].init.body), { action: "team_home", discord_user_id: "222" });
  assert.equal(await site.isMember("222"), false);
});

test("SiteGround's first-request 403 is retried once; a second 403 is SiteUnavailable", async () => {
  let n = 0;
  const f = fakeFetch({ [URL_]: () => (++n === 1 ? { status: 403, body: "<html>blocked</html>" } : { body: JSON.stringify(member) }) });
  const site = leagueSite({ url: URL_, token: "t", fetchImpl: f, clock: fixedClock() });
  assert.equal((await site.teamHome("1")).member, true);
  assert.equal(f.calls.length, 2);
  const always = fakeFetch({ [URL_]: { status: 403, body: "<html>" } });
  await assert.rejects(leagueSite({ url: URL_, token: "t", fetchImpl: always, clock: fixedClock() }).teamHome("1"), SiteUnavailable);
  assert.equal(always.calls.length, 2, "exactly one retry");
});

test("unset, unreachable, non-200, non-JSON (PHP warning HTML) and ok:false are SiteUnavailable and never cached", async () => {
  const clock = fixedClock();
  await assert.rejects(leagueSite({ url: "", token: "", clock }).teamHome("1"), SiteUnavailable);
  await assert.rejects(leagueSite({ url: URL_, token: "t", fetchImpl: fakeFetch({}), clock }).teamHome("1"), SiteUnavailable);
  for (const r of [{ status: 500, body: "{}" }, { status: 200, body: "<br /><b>Deprecated</b>: ...{\"ok\":true}" }, { status: 400, body: '{"ok":false,"error":"league_id is not a league"}' }, { status: 200, body: '{"ok":true}' }]) {
    const f = fakeFetch({ [URL_]: r });
    const site = leagueSite({ url: URL_, token: "t", fetchImpl: f, clock });
    await assert.rejects(site.teamHome("1"), SiteUnavailable, JSON.stringify(r));
    await assert.rejects(site.teamHome("1"), SiteUnavailable);
    assert.equal(f.calls.length, 2, "a failure is not cached");
  }
  await assert.rejects(leagueSite({ url: URL_, token: "t", fetchImpl: fakeFetch({}), clock }).isMember("1"), SiteUnavailable);
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
//
// SiteGround answers the first request after a quiet spell with a 403 (seen live 10-08); one immediate retry clears
// it. A PHP warning printed into the body (display_errors) makes it non-JSON: that is SiteUnavailable too.

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

export function leagueSite({ url, token, leagueId = null, fetchImpl = fetch, clock, ttlMs = TEAM_HOME_TTL_MS, userAgent = "NFLStreetReborn-Backend/1.0" } = {}) {
  if (!clock || typeof clock.now !== "function") throw new Error("leagueSite needs the injected clock");
  const pinned = Number.isSafeInteger(Number(leagueId)) && Number(leagueId) > 0 ? Number(leagueId) : null;
  const cache = new Map();   // discordId -> { at, value }
  const logos = new Map();   // teamId -> league-site URL

  async function post(id) {
    const body = JSON.stringify({ action: "team_home", discord_user_id: id, ...(pinned ? { league_id: pinned } : {}) });
    const init = {
      method: "POST",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}`, "User-Agent": userAgent },
      body,
    };
    for (let attempt = 0; attempt < 2; attempt++) {
      let res;
      try { res = await fetchImpl(url, { ...init, signal: AbortSignal.timeout(SITE_TIMEOUT_MS) }); }
      catch (e) { throw new SiteUnavailable(`league site unreachable: ${e.message}`); }
      const text = await res.text();
      if (res.status === 403 && attempt === 0) continue;   // SiteGround's first-request block
      return { status: res.status, text };
    }
    return { status: 403, text: "" };
  }

  async function teamHome(discordId, { fresh = false } = {}) {
    if (!url || !token) throw new SiteUnavailable("league site not configured");
    const id = String(discordId);
    const now = clock.now().getTime();
    const hit = cache.get(id);
    if (!fresh && hit && now - hit.at < ttlMs) return hit.value;
    const { status, text } = await post(id);
    let body = null;
    try { body = JSON.parse(text); } catch { body = null; }
    if (status !== 200 || body?.ok !== true || typeof body.member !== "boolean") throw new SiteUnavailable(`league site answered ${status}`);
    for (const [teamId, logo] of logoPairs(body)) if (logo.startsWith(LOGO_ORIGIN)) logos.set(teamId, logo);
    cache.set(id, { at: now, value: body });
    if (cache.size > CACHE_MAX) for (const [k, v] of cache) if (now - v.at >= ttlMs) cache.delete(k);
    return body;
  }

  return {
    teamHome,
    /** Membership from the (cached) answer. Throws SiteUnavailable on an outage: callers decide what that means. */
    isMember: async (discordId) => (await teamHome(discordId)).member === true,
    logoSource: (teamId) => logos.get(Number(teamId)) ?? null,
  };
}
```

- [ ] **Step 4: Run tests**

Run: `node --env-file-if-exists=.env --test test/league_site.test.js` → PASS (5).

- [ ] **Step 5: Commit**

```powershell
git add src/league/site.js test/league_site.test.js
git commit -m "league: site client (LEAGUE_ID pin, 60 s cache, one 403 retry, SiteUnavailable, logo registry)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Release reader and R2 presigning

**Files:**
- Create: `src/league/release.js`, `src/league/presign.js`
- Test: `test/league_release.test.js`, `test/league_presign.test.js`

**Interfaces:**
- Produces (`presign.js`): `presignGet({ accessKeyId, secretAccessKey, region, host, path, expiresSec, now }) -> string` (https URL); `r2BlobUrl({ accountId, accessKeyId, secretAccessKey, bucket, sha256, expiresSec, now }) -> string`; `blobKey(sha256) -> "blobs/by-hash/<xx>/<sha>"`; `LEAGUE_DOWNLOAD_TTL_S = 21600`.
- Produces (`release.js`): `releaseReader({ base, publicKeysHex, fetchImpl = fetch, clock, ttlMs = 300_000 })` -> `{ component(name): Promise<{version, files:[{path, bytes, sha256}]}>, versions(): Promise<{"league-game": string|null, "league-content": string|null}> }`; `class ReleaseUnavailable extends Error`; `LEAGUE_COMPONENTS = ["league-game", "league-content"]`; `DEFAULT_RELEASE_KEYS` (the two compiled launcher keys).

- [ ] **Step 1: Write the failing presign test (AWS's published example)**

```js
// test/league_presign.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { presignGet, r2BlobUrl, blobKey } from "../src/league/presign.js";

// AWS S3 docs, "Authenticating Requests: Using Query Parameters (AWS Signature Version 4)", GET example.
test("presignGet reproduces AWS's published SigV4 query-presign example exactly", () => {
  const url = presignGet({
    accessKeyId: "AKIAIOSFODNN7EXAMPLE",
    secretAccessKey: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    region: "us-east-1",
    host: "examplebucket.s3.amazonaws.com",
    path: "/test.txt",
    expiresSec: 86400,
    now: new Date("2013-05-24T00:00:00Z"),
  });
  const u = new URL(url);
  assert.equal(u.origin + u.pathname, "https://examplebucket.s3.amazonaws.com/test.txt");
  assert.equal(u.searchParams.get("X-Amz-Algorithm"), "AWS4-HMAC-SHA256");
  assert.equal(u.searchParams.get("X-Amz-Credential"), "AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request");
  assert.equal(u.searchParams.get("X-Amz-Date"), "20130524T000000Z");
  assert.equal(u.searchParams.get("X-Amz-Expires"), "86400");
  assert.equal(u.searchParams.get("X-Amz-SignedHeaders"), "host");
  assert.equal(u.searchParams.get("X-Amz-Signature"), "aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404");
});

test("r2BlobUrl signs the origin layout key on the account's R2 endpoint, region auto", () => {
  const sha = "ab".repeat(32);
  assert.equal(blobKey(sha), `blobs/by-hash/ab/${sha}`);
  const u = new URL(r2BlobUrl({ accountId: "acct123", accessKeyId: "K", secretAccessKey: "S", bucket: "reborn-league", sha256: sha, expiresSec: 21600, now: new Date("2026-10-08T12:00:00Z") }));
  assert.equal(u.host, "acct123.r2.cloudflarestorage.com");
  assert.equal(u.pathname, `/reborn-league/blobs/by-hash/ab/${sha}`);
  assert.equal(u.searchParams.get("X-Amz-Credential"), "K/20261008/auto/s3/aws4_request");
  assert.equal(u.searchParams.get("X-Amz-Expires"), "21600");
  assert.match(u.searchParams.get("X-Amz-Signature"), /^[0-9a-f]{64}$/);
  assert.throws(() => blobKey("../etc"), /sha256/);
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_presign.test.js` → FAIL (module missing).

- [ ] **Step 3: Implement `src/league/presign.js`**

```js
// src/league/presign.js
// AWS Signature Version 4, query-string form, GET only: the one thing the League edition download needs from R2's
// S3 API. Hand-rolled (40 lines of node:crypto) rather than the AWS SDK, and pinned to AWS's own published example in
// test/league_presign.test.js -- a canonical-request slip here is a 403 for every member, so the vector is the guard.
// Only `host` is signed, so the launcher's `Range` header (resume) rides along unsigned, as S3 allows.
import { createHash, createHmac } from "node:crypto";

export const LEAGUE_DOWNLOAD_TTL_S = 6 * 60 * 60;

const hex = (buf) => buf.toString("hex");
const sha256 = (s) => hex(createHash("sha256").update(s, "utf8").digest());
const hmac = (key, s) => createHmac("sha256", key).update(s, "utf8").digest();
/** RFC 3986 encoding as SigV4 wants it: everything but unreserved characters. */
const enc = (s) => encodeURIComponent(s).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
const encPath = (p) => p.split("/").map(enc).join("/");

export function presignGet({ accessKeyId, secretAccessKey, region, host, path, expiresSec, now }) {
  const iso = now.toISOString().replace(/[-:]/g, "").replace(/\.\d{3}/, "");   // 20130524T000000Z
  const day = iso.slice(0, 8);
  const scope = `${day}/${region}/s3/aws4_request`;
  const params = {
    "X-Amz-Algorithm": "AWS4-HMAC-SHA256",
    "X-Amz-Credential": `${accessKeyId}/${scope}`,
    "X-Amz-Date": iso,
    "X-Amz-Expires": String(expiresSec),
    "X-Amz-SignedHeaders": "host",
  };
  const query = Object.keys(params).sort().map((k) => `${enc(k)}=${enc(params[k])}`).join("&");
  const canonicalPath = encPath(path);
  const canonical = ["GET", canonicalPath, query, `host:${host}`, "", "host", "UNSIGNED-PAYLOAD"].join("\n");
  const toSign = ["AWS4-HMAC-SHA256", iso, scope, sha256(canonical)].join("\n");
  const kDate = hmac(`AWS4${secretAccessKey}`, day);
  const kRegion = hmac(kDate, region);
  const kService = hmac(kRegion, "s3");
  const kSigning = hmac(kService, "aws4_request");
  const signature = hex(createHmac("sha256", kSigning).update(toSign, "utf8").digest());
  return `https://${host}${canonicalPath}?${query}&X-Amz-Signature=${signature}`;
}

/** The launcher's origin layout (origin.rs `blob_path`): two hex characters of fan-out, then the hash. */
export function blobKey(sha) {
  const s = String(sha ?? "").toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(s)) throw new Error("blobKey needs a sha256 hex string");
  return `blobs/by-hash/${s.slice(0, 2)}/${s}`;
}

export function r2BlobUrl({ accountId, accessKeyId, secretAccessKey, bucket, sha256: sha, expiresSec = LEAGUE_DOWNLOAD_TTL_S, now }) {
  return presignGet({
    accessKeyId, secretAccessKey, region: "auto",
    host: `${accountId}.r2.cloudflarestorage.com`,
    path: `/${bucket}/${blobKey(sha)}`,
    expiresSec, now,
  });
}
```

- [ ] **Step 4: Run presign tests** → PASS (2). If the AWS vector fails, print the canonical request and compare it with the AWS doc's canonical request line by line; do not change the expected signature.

- [ ] **Step 5: Write the failing release test**

```js
// test/league_release.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { fixedClock, fakeFetch } from "./helpers/fakes.js";
import { releaseReader, ReleaseUnavailable, DEFAULT_RELEASE_KEYS } from "../src/league/release.js";

const BASE = "https://files.test/";
const { publicKey, privateKey } = generateKeyPairSync("ed25519");
const pubHex = publicKey.export({ format: "der", type: "spki" }).subarray(-32).toString("hex");
const sha = (s) => createHash("sha256").update(s).digest("hex");

function origin({ tamper = false, pinWrong = false } = {}) {
  const game = JSON.stringify({ schema: "nsr.component.v1", component: "league-game", version: "2026.10.08", sequence: 1,
    install_root: "library/league", customizable: false, members_only: true,
    files: [{ path: "NSR-League.iso", bytes: 1460000000, sha256: "aa".repeat(32) }, { path: "textures/x.png", bytes: 10, sha256: "bb".repeat(32) }] });
  const content = JSON.stringify({ schema: "nsr.component.v1", component: "league-content", version: "1.0.0", sequence: 1,
    install_root: "library/league-content", customizable: false, members_only: true, files: [{ path: "league.s01", bytes: 5, sha256: "cc".repeat(32) }] });
  const release = JSON.stringify({ schema: "nsr.release.v1", version: "3.0.36", sequence: 60, components: {
    "league-game": { url: "components/league-game-2026.10.08.json", sha256: pinWrong ? "00".repeat(32) : sha(game) },
    "league-content": { url: "components/league-content-1.0.0.json", sha256: sha(content) },
  } });
  const sig = sign(null, Buffer.from(tamper ? release + " " : release), privateKey).toString("hex");
  return fakeFetch({
    [`${BASE}release.json`]: { body: release },
    [`${BASE}release.json.sig`]: { body: sig },
    [`${BASE}components/league-game-2026.10.08.json`]: { body: game },
    [`${BASE}components/league-content-1.0.0.json`]: { body: content },
  });
}

test("versions() and component() read the signed release and the pinned manifests; cached 5 min", async () => {
  const clock = fixedClock(); const f = origin();
  const r = releaseReader({ base: BASE, publicKeysHex: [pubHex], fetchImpl: f, clock });
  assert.deepEqual(await r.versions(), { "league-game": "2026.10.08", "league-content": "1.0.0" });
  const g = await r.component("league-game");
  assert.equal(g.version, "2026.10.08");
  assert.deepEqual(g.files.map((x) => x.path), ["NSR-League.iso", "textures/x.png"]);
  const calls = f.calls.length;
  await r.versions(); await r.component("league-game");
  assert.equal(f.calls.length, calls, "cached");
  clock.advance(300_000);
  await r.versions();
  assert.ok(f.calls.length > calls, "refetched after 5 min");
});

test("a bad signature, an unknown key, a pin mismatch or an unreachable origin are ReleaseUnavailable", async () => {
  const clock = fixedClock();
  await assert.rejects(releaseReader({ base: BASE, publicKeysHex: [pubHex], fetchImpl: origin({ tamper: true }), clock }).versions(), ReleaseUnavailable);
  await assert.rejects(releaseReader({ base: BASE, publicKeysHex: DEFAULT_RELEASE_KEYS, fetchImpl: origin(), clock }).versions(), ReleaseUnavailable);
  await assert.rejects(releaseReader({ base: BASE, publicKeysHex: [pubHex], fetchImpl: origin({ pinWrong: true }), clock }).component("league-game"), ReleaseUnavailable);
  await assert.rejects(releaseReader({ base: BASE, publicKeysHex: [pubHex], fetchImpl: fakeFetch({}), clock }).versions(), ReleaseUnavailable);
});

test("a component the release does not pin reads as version null, and component() refuses it", async () => {
  const clock = fixedClock();
  const f = fakeFetch({});
  const release = JSON.stringify({ schema: "nsr.release.v1", version: "3.0.35", sequence: 59, components: {} });
  f.calls.length = 0;
  const g = fakeFetch({ [`${BASE}release.json`]: { body: release }, [`${BASE}release.json.sig`]: { body: sign(null, Buffer.from(release), privateKey).toString("hex") } });
  const r = releaseReader({ base: BASE, publicKeysHex: [pubHex], fetchImpl: g, clock });
  assert.deepEqual(await r.versions(), { "league-game": null, "league-content": null });
  await assert.rejects(r.component("league-game"), ReleaseUnavailable);
  assert.equal(DEFAULT_RELEASE_KEYS.length, 2);
});
```

- [ ] **Step 6: Run to verify failure** → FAIL (module missing).

- [ ] **Step 7: Implement `src/league/release.js`**

```js
// src/league/release.js
// The League edition's published versions and file lists, read from the same signed origin the launcher reads
// (origin.rs): release.json + release.json.sig (Ed25519, 128 hex), component manifests pinned by sha256. The backend
// verifies the signature itself so a tampered public bucket can never make it sign download URLs for other files.
import { createHash, createPublicKey, verify } from "node:crypto";

export const LEAGUE_COMPONENTS = Object.freeze(["league-game", "league-content"]);
/** The launcher's compiled TRUSTED_KEYS (crates/reborn-core/src/signing.rs): everyday, rotation. */
export const DEFAULT_RELEASE_KEYS = Object.freeze([
  "eac9ecc7a5f4e35234a23aa9b11d22b2f2372ef25e9c2ea9e1aa7458acb88fe9",
  "71a34c792493cf9fe0de67f3ff7e202b5eebdae1729382bb165f38ca2aa9457b",
]);
export const RELEASE_TTL_MS = 5 * 60 * 1000;
const TIMEOUT_MS = 10_000;
const SPKI_ED25519 = Buffer.from("302a300506032b6570032100", "hex");

export class ReleaseUnavailable extends Error {}

const keyOf = (h) => createPublicKey({ key: Buffer.concat([SPKI_ED25519, Buffer.from(h, "hex")]), format: "der", type: "spki" });

export function releaseReader({ base, publicKeysHex = DEFAULT_RELEASE_KEYS, fetchImpl = fetch, clock, ttlMs = RELEASE_TTL_MS } = {}) {
  if (!clock || typeof clock.now !== "function") throw new Error("releaseReader needs the injected clock");
  const root = String(base ?? "").replace(/\/*$/, "/");
  const keys = publicKeysHex.filter((h) => /^[0-9a-f]{64}$/i.test(h)).map(keyOf);
  let cached = null;                 // { at, release }
  const manifests = new Map();       // pinned sha256 -> parsed component (immutable by hash)

  async function bytes(url) {
    let res;
    try { res = await fetchImpl(url, { signal: AbortSignal.timeout(TIMEOUT_MS) }); }
    catch (e) { throw new ReleaseUnavailable(`origin unreachable: ${e.message}`); }
    if (res.status !== 200) throw new ReleaseUnavailable(`origin answered ${res.status} for ${url}`);
    return Buffer.from(await res.text(), "utf8");
  }
  const join = (rel) => (/^https?:\/\//.test(rel) ? rel : root + String(rel).replace(/^\/+/, ""));

  async function release() {
    const now = clock.now().getTime();
    if (cached && now - cached.at < ttlMs) return cached.release;
    const body = await bytes(join("release.json"));
    const sigHex = (await bytes(join("release.json.sig"))).toString("utf8").trim();
    if (!/^[0-9a-f]{128}$/i.test(sigHex)) throw new ReleaseUnavailable("release.json.sig is not 128 hex characters");
    const sig = Buffer.from(sigHex, "hex");
    if (!keys.some((k) => verify(null, body, k, sig))) throw new ReleaseUnavailable("release.json signature did not verify");
    let parsed;
    try { parsed = JSON.parse(body.toString("utf8")); } catch { throw new ReleaseUnavailable("release.json is not JSON"); }
    if (parsed?.schema !== "nsr.release.v1") throw new ReleaseUnavailable("release.json has the wrong schema");
    cached = { at: now, release: parsed };
    return parsed;
  }

  async function component(name) {
    if (!LEAGUE_COMPONENTS.includes(name)) throw new ReleaseUnavailable(`not a league component: ${name}`);
    const pin = (await release()).components?.[name];
    if (!pin?.url || !/^[0-9a-f]{64}$/i.test(String(pin.sha256 ?? ""))) throw new ReleaseUnavailable(`${name} is not published`);
    const want = String(pin.sha256).toLowerCase();
    if (manifests.has(want)) return manifests.get(want);
    const raw = await bytes(join(pin.url));
    if (createHash("sha256").update(raw).digest("hex") !== want) throw new ReleaseUnavailable(`${name} manifest does not match its pin`);
    let c;
    try { c = JSON.parse(raw.toString("utf8")); } catch { throw new ReleaseUnavailable(`${name} manifest is not JSON`); }
    if (c?.schema !== "nsr.component.v1" || c.component !== name || !Array.isArray(c.files)) throw new ReleaseUnavailable(`${name} manifest is malformed`);
    const out = { version: String(c.version), files: c.files.map((f) => ({ path: String(f.path), bytes: Number(f.bytes), sha256: String(f.sha256).toLowerCase() })) };
    manifests.set(want, out);
    return out;
  }

  async function versions() {
    const comps = (await release()).components ?? {};
    const out = {};
    for (const name of LEAGUE_COMPONENTS) {
      if (!comps[name]) { out[name] = null; continue; }
      out[name] = (await component(name)).version;
    }
    return out;
  }

  return { component, versions };
}
```

- [ ] **Step 8: Run tests** → `test/league_release.test.js` PASS (3), `test/league_presign.test.js` PASS (2).

- [ ] **Step 9: Commit**

```powershell
git add src/league/release.js src/league/presign.js test/league_release.test.js test/league_presign.test.js
git commit -m "league: signed release reader for league components + SigV4 R2 presign (AWS vector)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Matches carry league, edition, ruleset and version

**Files:**
- Modify: `src/mm/match.js` (`MATCH_COLUMNS` ~186, `createMatch` ~388-469)
- Modify: `src/mm/snapshot.js` (room query ~226, `snapshot.room` ~252, match query ~281, `snapshot.match` ~335)
- Test: `test/league_match.test.js`

**Interfaces:**
- Consumes: Task 2 columns.
- Produces: `createMatch(client, clock, { ..., source: "league", league: { league_id, fixture_id, week, seat0_home }, edition = "retail", ruleset = null, leagueGameVersion = null })`; exported `LEAGUE_BEST_OF = 3`; snapshot `room.league|edition|ruleset|league_game_version`, `match.league|edition|ruleset|league_game_version`.

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

test("a league match: casual (no Elo), best of 3 whatever casual_best_of says, league edition, league seed, fixture", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    await t.pool.query(`UPDATE series_rules SET casual_best_of = NULL`);
    const a = await player(t.pool); const b = await player(t.pool);
    const { match } = await withTx(t.pool, (c) => createMatch(c, clock, {
      mode: "casual", seat0: a.id, seat1: b.id, source: "league",
      league: { league_id: 36, fixture_id: 1234, week: 6, seat0_home: false },
      edition: "league", ruleset: "league", leagueGameVersion: "2026.10.08",
    }));
    assert.equal(LEAGUE_BEST_OF, 3);
    assert.equal(match.mode, "casual");
    assert.equal(Number(match.best_of), 3);
    assert.equal(Number(match.league_fixture_id), 1234);
    assert.equal(match.league_seat0_home, false);
    assert.equal(match.edition, "league"); assert.equal(match.ruleset, "league"); assert.equal(match.league_game_version, "2026.10.08");
    const shape = seriesShape(match);
    assert.equal(shape.ranked, false); assert.equal(shape.bestOf, 3); assert.equal(shape.need, 2);

    const snap = await withTx(t.pool, (c) => snapshotFor(c, clock, a.id));
    assert.deepEqual(snap.match.league, { league_id: 36, fixture_id: 1234, week: 6 });
    assert.equal(snap.match.edition, "league");
    assert.equal(snap.match.ruleset, "league", "league games boot league.s01");
    assert.equal(snap.match.league_game_version, "2026.10.08");
  } finally { await t.close(); }
});

test("source league needs a league object and the league ruleset; other matches keep the old ruleset meaning", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const a = await player(t.pool); const b = await player(t.pool);
    await assert.rejects(withTx(t.pool, (c) => createMatch(c, clock, { seat0: a.id, seat1: b.id, source: "league" })), /league/);
    await assert.rejects(withTx(t.pool, (c) => createMatch(c, clock, { seat0: a.id, seat1: b.id, source: "league",
      league: { league_id: 36, fixture_id: 1, week: 1, seat0_home: true }, edition: "league", ruleset: "raw" })), /ruleset/);
    await withTx(t.pool, (c) => createMatch(c, clock, { mode: "casual", seat0: a.id, seat1: b.id, source: "room" }));
    const snap = await withTx(t.pool, (c) => snapshotFor(c, clock, a.id));
    assert.equal(snap.match.league, null);
    assert.equal(snap.match.edition, "retail");
    assert.equal(snap.match.ruleset, "casual");
    assert.equal(snap.match.league_game_version, null);
  } finally { await t.close(); }
});

test("a friendly on the league edition keeps its raw ruleset and is not a league fixture", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const a = await player(t.pool); const b = await player(t.pool);
    await withTx(t.pool, (c) => createMatch(c, clock, { mode: "casual", bestOf: 1, seat0: a.id, seat1: b.id, source: "room",
      edition: "league", ruleset: "raw", leagueGameVersion: "2026.10.08" }));
    const snap = await withTx(t.pool, (c) => snapshotFor(c, clock, b.id));
    assert.equal(snap.match.edition, "league"); assert.equal(snap.match.ruleset, "raw"); assert.equal(snap.match.league, null);
  } finally { await t.close(); }
});
```

> If `snapshotFor`'s signature differs from `(client, clock, playerId)` (snapshot.js ~120), match the real one here; do not change snapshot's signature.

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_match.test.js` → FAIL (`LEAGUE_BEST_OF` not exported).

- [ ] **Step 3: Implement in `match.js`**

Append `"league_id", "league_fixture_id", "league_week", "league_seat0_home", "edition", "ruleset", "league_game_version",` to `MATCH_COLUMNS`.

Above `createMatch`:

```js
/** A league fixture is always a best-of-three (Adam 10-08: "it's best of 3"), whatever the casual rule says. */
export const LEAGUE_BEST_OF = 3;
const EDITIONS = ["retail", "league"];
const RULESETS = ["casual", "ranked", "league", "raw"];
```

Signature:

```js
export async function createMatch(client, clock, {
  mode = "casual", bestOf = null, seat0, seat1, roomId = null, picks = [null, null],
  region = null, source = "queue", originSource = null, accepted = null, actor = ACTOR.SYSTEM,
  seedKind = "regular", extras = [], league = null, edition = "retail", ruleset = null, leagueGameVersion = null,
} = {}) {
```

Before the rules are read:

```js
  if (source === "league" && !(league && Number.isSafeInteger(Number(league.fixture_id)))) {
    throw new Error("createMatch: a league match needs league { league_id, fixture_id, week, seat0_home }");
  }
  if (source === "league" && ruleset !== "league") throw new Error("createMatch: a league match plays the league ruleset");
  if (!EDITIONS.includes(edition)) throw new Error(`createMatch: edition must be ${EDITIONS.join("|")}`);
  if (ruleset !== null && !RULESETS.includes(ruleset)) throw new Error(`createMatch: ruleset must be ${RULESETS.join("|")}`);
  const sandbox = mode !== "ranked" && PRIVATE_SOURCES.includes(originSource ?? source);
  const read = await readSeriesRules(client);
  // A league fixture is a fixed best of three: copied rules say so, so seriesShape reads it as a closed series.
  const rules = source === "league" ? { ...read, casual_best_of: LEAGUE_BEST_OF }
    : sandbox ? { ...read, casual_best_of: null } : read;
  const best = mode === "ranked" ? rules.ranked_best_of : rules.casual_best_of;
```

(replacing the existing `sandbox`/`rules`/`best` lines). Extend the INSERT: add `league_id, league_fixture_id, league_week, league_seat0_home, edition, ruleset, league_game_version` to the column list, the next seven `$N` placeholders to VALUES, and append to the params array:

```js
      league ? Number(league.league_id) : null, league ? Number(league.fixture_id) : null,
      league ? Number(league.week) : null, league ? league.seat0_home === true : null,
      edition, ruleset, leagueGameVersion === null || leagueGameVersion === undefined ? null : String(leagueGameVersion)],
```

Add to the `PAIRED` event payload: `edition, ruleset, ...(league ? { league: { league_id: Number(league.league_id), fixture_id: Number(league.fixture_id), week: Number(league.week) } } : {})`.

- [ ] **Step 4: Implement in `snapshot.js`**

Room query: add `r.league_id, r.league_fixture_id, r.league_week, r.edition, r.ruleset, r.league_game_version,`. In `snapshot.room = {...}` add:

```js
        league: r.league_fixture_id === null || r.league_fixture_id === undefined ? null
          : { league_id: num(r.league_id), fixture_id: num(r.league_fixture_id), week: num(r.league_week) },
        edition: r.edition ?? "retail",
        ruleset: r.ruleset ?? null,
        league_game_version: r.league_game_version ?? null,
```

Match query: add `m.league_id, m.league_fixture_id, m.league_week, m.edition, m.ruleset, m.league_game_version,`. In `snapshot.match = {...}` after `source`:

```js
        // The league fixture this series plays, or null. edition picks the ISO/game ID, ruleset the seed:
        // league -> league.s01, ranked -> ranked.s01, raw -> none, casual -> the launcher's casual default.
        league: m.league_fixture_id === null || m.league_fixture_id === undefined ? null
          : { league_id: num(m.league_id), fixture_id: num(m.league_fixture_id), week: num(m.league_week) },
        edition: m.edition ?? "retail",
        ruleset: m.ruleset ?? (m.mode === "ranked" ? "ranked" : "casual"),
        league_game_version: m.league_game_version ?? null,
```

- [ ] **Step 5: Run tests**

Run: `node --env-file-if-exists=.env --test test/league_match.test.js` → PASS (3). `npm test` → green (snapshot tests that `deepEqual` whole room/match objects get the new keys with their defaults; update the expectations, do not loosen them).

- [ ] **Step 6: Commit**

```powershell
git add src/mm/match.js src/mm/snapshot.js test/league_match.test.js test/*.test.js
git commit -m "league: createMatch league/edition/ruleset/version + snapshot fields`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Rooms gate the League edition and start league/friendly matches

**Files:**
- Modify: `src/mm/rooms.js` (`assertNotSeated` ~265, `roomView` ~291-328, `lockedRoom` ~338-351, `roomsRouter` options ~420, join ~517, peek ~570, kind, start)
- Modify: `src/mm/index.js` (`createMatchmaking` options -> `roomsRouter`)
- Test: `test/league_rooms.test.js`

**Interfaces:**
- Produces: `export async function assertNotSeated(...)` (unchanged body); `export function leagueOf(row)`; `roomView(...)` gains `league`, `edition`, `ruleset`, `league_game_version`; `roomsRouter({ ..., leagueMember = null })` where `leagueMember(discordId): Promise<boolean>` may throw (outage); `createMatchmaking({ pool, clock, env, leagueMember })`; join body `league_game_version`; 409 `{ok:false,error:"league_version_mismatch",required}`; 503 `{ok:false,error:"league_unavailable"}`.
- Consumes: Task 5 `createMatch` params.

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
async function room(pool, clock, host, { code = "LGAME2", allowed = null, fixture = 1234, ruleset = "league", bestOf = 3 } = {}) {
  const r = (await pool.query(
    `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, seed_kind, created_at, expires_at, capacity,
                          league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids,
                          edition, ruleset, league_game_version)
     VALUES ($1, $2, 'casual', 'code', $9, 'regular', $3, $4, 2, $5, $6, $7, $8, $10, 'league', $11, '2026.10.08') RETURNING *`,
    [code, host.id, clock.now(), new Date(clock.now().getTime() + 30 * 60_000),
      fixture ? 36 : null, fixture, fixture ? 6 : null, fixture ? true : null, bestOf, allowed, ruleset])).rows[0];
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
const V = { league_game_version: "2026.10.08" };

test("fixture room: a stranger with the code sees a bad code on join AND peek; the opponent joins", async () => {
  const t = await testPool(); const clock = fixedClock(); const s = await serve(t, clock);
  try {
    const host = await signedIn(t.pool, clock); const opp = await signedIn(t.pool, clock); const stranger = await signedIn(t.pool, clock);
    await room(t.pool, clock, host, { allowed: [host.discord_user_id, opp.discord_user_id] });
    const peek = await s.call("GET", "/v1/rooms/LGAME2", stranger.bearer);
    assert.equal(peek.status, 404); assert.equal(peek.json.error, "No such lobby.");
    const join = await s.call("POST", "/v1/mm/rooms/LGAME2/join", stranger.bearer, V);
    assert.equal(join.status, 404); assert.equal(join.json.error, "No such lobby.");
    const ok = await s.call("POST", "/v1/mm/rooms/LGAME2/join", opp.bearer, V);
    assert.equal(ok.status, 200, JSON.stringify(ok.json));
    assert.deepEqual(ok.json.room.league, { league_id: 36, fixture_id: 1234, week: 6 });
    assert.equal(ok.json.room.edition, "league"); assert.equal(ok.json.room.ruleset, "league");
    assert.equal(ok.json.room.league_game_version, "2026.10.08"); assert.equal(ok.json.room.best_of, 3);
  } finally { await s.close(); await t.close(); }
});

test("league files out of date: the joiner is told the required version and takes no seat", async () => {
  const t = await testPool(); const clock = fixedClock(); const s = await serve(t, clock);
  try {
    const host = await signedIn(t.pool, clock); const opp = await signedIn(t.pool, clock);
    const r = await room(t.pool, clock, host, { allowed: [host.discord_user_id, opp.discord_user_id] });
    for (const body of [{ league_game_version: "2026.09.01" }, {}]) {
      const j = await s.call("POST", "/v1/mm/rooms/LGAME2/join", opp.bearer, body);
      assert.equal(j.status, 409, JSON.stringify(j.json));
      assert.deepEqual(j.json, { ok: false, error: "league_version_mismatch", required: "2026.10.08" });
    }
    const seated = (await t.pool.query(`SELECT count(*)::int AS n FROM mm_room_member WHERE room_id = $1 AND active`, [r.id])).rows[0].n;
    assert.equal(seated, 1);
  } finally { await s.close(); await t.close(); }
});

test("friendly room: members join; a non-member sees a bad code; a site outage is 503 league_unavailable", async () => {
  const t = await testPool(); const clock = fixedClock();
  const members = new Set(); let down = false;
  const leagueMember = async (id) => { if (down) { const e = new Error("down"); e.name = "SiteUnavailable"; throw e; } return members.has(String(id)); };
  const s = await serve(t, clock, { leagueMember });
  try {
    const host = await signedIn(t.pool, clock); const mate = await signedIn(t.pool, clock); const outsider = await signedIn(t.pool, clock);
    members.add(host.discord_user_id); members.add(mate.discord_user_id);
    await room(t.pool, clock, host, { code: "FRND22", fixture: null, ruleset: "raw", bestOf: 1 });
    const peek = await s.call("GET", "/v1/rooms/FRND22", outsider.bearer);
    assert.equal(peek.status, 404);
    const no = await s.call("POST", "/v1/mm/rooms/FRND22/join", outsider.bearer, V);
    assert.equal(no.status, 404); assert.equal(no.json.error, "No such lobby.");
    down = true;
    const out = await s.call("POST", "/v1/mm/rooms/FRND22/join", mate.bearer, V);
    assert.equal(out.status, 503); assert.equal(out.json.error, "league_unavailable");
    down = false;
    const yes = await s.call("POST", "/v1/mm/rooms/FRND22/join", mate.bearer, V);
    assert.equal(yes.status, 200, JSON.stringify(yes.json));
    assert.equal(yes.json.room.league, null); assert.equal(yes.json.room.ruleset, "raw");
    const okPeek = await s.call("GET", "/v1/rooms/FRND22", mate.bearer);
    assert.equal(okPeek.status, 404, "full now (2/2): peek's existing rule");
  } finally { await s.close(); await t.close(); }
});

test("a friendly room with no leagueMember wired refuses everyone like a bad code", async () => {
  const t = await testPool(); const clock = fixedClock(); const s = await serve(t, clock);
  try {
    const host = await signedIn(t.pool, clock); const mate = await signedIn(t.pool, clock);
    await room(t.pool, clock, host, { code: "FRND33", fixture: null, ruleset: "league", bestOf: 1 });
    const j = await s.call("POST", "/v1/mm/rooms/FRND33/join", mate.bearer, V);
    assert.equal(j.status, 404);
  } finally { await s.close(); await t.close(); }
});

test("a fixture room's game type is fixed", async () => {
  const t = await testPool(); const clock = fixedClock(); const s = await serve(t, clock);
  try {
    const host = await signedIn(t.pool, clock); const opp = await signedIn(t.pool, clock);
    const r = await room(t.pool, clock, host, { allowed: [host.discord_user_id, opp.discord_user_id] });
    const k = await s.call("POST", `/v1/mm/rooms/${r.id}/kind`, host.bearer, { seed_kind: "pickup" });
    assert.equal(k.status, 409); assert.equal(k.json.error, "A league game's type is fixed.");
  } finally { await s.close(); await t.close(); }
});

test("starting copies league, edition, ruleset and version onto the match", async () => {
  const t = await testPool(); const clock = fixedClock();
  const calls = [];
  const createMatch = async (client, clk, args) => {
    calls.push(args);
    return (await client.query(
      `INSERT INTO match (mode, status, best_of, seat0_player, seat1_player, room_id, source, created_at) VALUES ('casual','pairing',3,$1,$2,$3,$4,$5) RETURNING id`,
      [args.seat0, args.seat1, args.roomId, args.source, clk.now()])).rows[0];
  };
  const members = new Set();
  const s = await serve(t, clock, { createMatch, leagueMember: async (id) => members.has(String(id)) });
  try {
    const host = await signedIn(t.pool, clock); const opp = await signedIn(t.pool, clock);
    const r = await room(t.pool, clock, host, { allowed: [host.discord_user_id, opp.discord_user_id] });
    await s.call("POST", "/v1/mm/rooms/LGAME2/join", opp.bearer, V);
    await s.call("POST", `/v1/mm/rooms/${r.id}/ready`, host.bearer, { ready: true });
    await s.call("POST", `/v1/mm/rooms/${r.id}/ready`, opp.bearer, { ready: true });
    const st = await s.call("POST", `/v1/mm/rooms/${r.id}/start`, host.bearer);
    assert.equal(st.status, 200, JSON.stringify(st.json));
    assert.equal(calls[0].source, "league"); assert.equal(calls[0].mode, "casual");
    assert.deepEqual(calls[0].league, { league_id: 36, fixture_id: 1234, week: 6, seat0_home: true });
    assert.equal(calls[0].edition, "league"); assert.equal(calls[0].ruleset, "league"); assert.equal(calls[0].leagueGameVersion, "2026.10.08");

    members.add(host.discord_user_id); members.add(opp.discord_user_id);
    const host2 = await signedIn(t.pool, clock); const mate2 = await signedIn(t.pool, clock);
    members.add(host2.discord_user_id); members.add(mate2.discord_user_id);
    const f = await room(t.pool, clock, host2, { code: "FRND44", fixture: null, ruleset: "raw", bestOf: 1 });
    await s.call("POST", "/v1/mm/rooms/FRND44/join", mate2.bearer, V);
    await s.call("POST", `/v1/mm/rooms/${f.id}/ready`, host2.bearer, { ready: true });
    await s.call("POST", `/v1/mm/rooms/${f.id}/ready`, mate2.bearer, { ready: true });
    await s.call("POST", `/v1/mm/rooms/${f.id}/start`, host2.bearer);
    assert.equal(calls[1].source, "room"); assert.equal(calls[1].league, undefined);
    assert.equal(calls[1].edition, "league"); assert.equal(calls[1].ruleset, "raw");
  } finally { await s.close(); await t.close(); }
});

test("an ordinary code room is unchanged: anyone with the code joins; edition retail, league null", async () => {
  const t = await testPool(); const clock = fixedClock(); const s = await serve(t, clock);
  try {
    const host = await signedIn(t.pool, clock); const guest = await signedIn(t.pool, clock);
    const made = await s.call("POST", "/v1/mm/rooms", host.bearer, {});
    assert.equal(made.json.room.league, null); assert.equal(made.json.room.edition, "retail");
    const j = await s.call("POST", `/v1/mm/rooms/${made.json.room.code}/join`, guest.bearer, {});
    assert.equal(j.status, 200);
  } finally { await s.close(); await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_rooms.test.js`
Expected: FAIL (stranger join 200; `room.league`/`edition` undefined).

- [ ] **Step 3: Implement in `rooms.js`**

1. `async function assertNotSeated(` -> `export async function assertNotSeated(`.
2. Add to `SAY`: `LEAGUE_KIND: "A league game's type is fixed.",`.
3. Under "Small shared pieces":

```js
/** The league fixture block of a room row, or null for every room that is not a league fixture. */
export function leagueOf(r) {
  return r?.league_fixture_id === null || r?.league_fixture_id === undefined ? null
    : { league_id: num(r.league_id), fixture_id: num(r.league_fixture_id), week: num(r.league_week) };
}

/** A fixture room is visible and joinable only by its two Discord IDs. */
function allowedIn(room, player) {
  return !Array.isArray(room.allowed_discord_ids) || room.allowed_discord_ids.includes(String(player?.discord_user_id ?? ""));
}
```

4. `roomsRouter` options: add `leagueMember = null`. Inside `roomsRouter`, before the routes:

```js
  /**
   * The League edition gate, BEFORE the join transaction (it may call the league site; no lock is held across a
   * network call). A fixture room answers by its allowed list, a friendly by league membership; every refusal is the
   * same 404 a bad code gets. `checkVersion` adds the league-files check for a seat-taking join.
   * Returns null to proceed, or { status, body }.
   */
  async function editionGate(code, player, body, { checkVersion }) {
    const row = (await pool.query(
      `SELECT edition, league_game_version, allowed_discord_ids FROM mm_room WHERE code = $1 AND status = 'open'`, [code])).rows[0];
    if (!row || row.edition !== "league") return null;
    const notFound = { status: 404, body: { ok: false, error: SAY.NO_ROOM } };
    if (Array.isArray(row.allowed_discord_ids)) {
      if (!allowedIn(row, player)) return notFound;
    } else {
      if (typeof leagueMember !== "function") return notFound;
      let member;
      try { member = await leagueMember(player.discord_user_id); }
      catch { return { status: 503, body: { ok: false, error: "league_unavailable" } }; }
      if (!member) return notFound;
    }
    if (checkVersion && row.league_game_version) {
      const mine = typeof body?.league_game_version === "string" ? body.league_game_version : null;
      if (mine !== row.league_game_version) {
        return { status: 409, body: { ok: false, error: "league_version_mismatch", required: row.league_game_version } };
      }
    }
    return null;
  }
```

5. `roomView`: add `league_id, league_fixture_id, league_week, edition, ruleset, league_game_version` to its SELECT and to the returned object:

```js
    league: leagueOf(r),
    edition: r.edition ?? "retail",
    ruleset: r.ruleset ?? null,
    league_game_version: r.league_game_version ?? null,
```

6. `lockedRoom`: add `league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids, edition, ruleset, league_game_version` to the second SELECT.
7. Join: right after `if (code === null) return ...`, before `withTx`:

```js
      const gate = await editionGate(code, req.player, req.body, { checkVersion: true });
      if (gate) return res.status(gate.status).json(gate.body);
```

and inside the transaction, right after `const room = await lockedRoom(c, { code });`:

```js
        // Re-checked under the lock: the allowed list is the authority for a fixture room.
        if (!allowedIn(room, req.player)) throw refuse(404, SAY.NO_ROOM);
```

8. Peek: after `if (code === null) ...`:

```js
      const gate = await editionGate(code, req.player, null, { checkVersion: false });
      if (gate) return res.status(gate.status).json(gate.body);
```

and add `edition: r.edition` to its SELECT (`r.edition`) and `edition: row.edition ?? "retail"` to its JSON answer.
9. Kind, after the host check: `if (leagueOf(room)) throw refuse(409, SAY.LEAGUE_KIND);`.
10. Start, in the `makeMatch` call arguments:

```js
          source: leagueOf(room) ? "league" : "room",
          ...(leagueOf(room) ? { league: { ...leagueOf(room), seat0_home: room.league_seat0_home === true } } : {}),
          edition: room.edition ?? "retail",
          ruleset: room.ruleset ?? null,
          leagueGameVersion: room.league_game_version ?? null,
```

- [ ] **Step 4: Implement in `src/mm/index.js`**

```js
export function createMatchmaking({ pool, clock, env = {}, leagueMember = null } = {}) {
```

and `roomsRouter({ ...shared, createMatch, leagueMember }),` in `routers`.

- [ ] **Step 5: Run tests**

Run: `node --env-file-if-exists=.env --test test/league_rooms.test.js` → PASS (7). `npm test` → green (`mm_rooms.test.js` `deepEqual`s on room views get `league: null, edition: "retail", ruleset: null, league_game_version: null`; peek answers get `edition: "retail"`).

- [ ] **Step 6: Commit**

```powershell
git add src/mm/rooms.js src/mm/index.js test/league_rooms.test.js test/*.test.js
git commit -m "league: rooms gate the League edition (allowed IDs, membership, version), fixed fixture type, start copies edition/ruleset`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Open a league fixture room (`src/league/room.js` + `POST /v1/league/room`)

**Files:**
- Create: `src/league/room.js`, `src/league/router.js`
- Test: `test/league_room_open.test.js`

**Interfaces:**
- Consumes: `site.teamHome` (Task 3), `release.versions` (Task 4), `assertNotSeated`, `roomView`, `newRoomCode`, `ROOM_CODE_ATTEMPTS` (rooms.js), `lockRoom`, `appendEvent`/`EVENT_KINDS`/`actorPlayer`/`ACTOR`, `bump`, `withTx`.
- Produces: `openLeagueRoom(pool, clock, { player, home, version, newCode, ttlMs })` -> `{ status, body, channels }`; `leagueRouter({ pool, clock, site, release = null, r2 = null, hub, publicOrigin, newCode, fetchImpl })`; `LEAGUE_ROOM_TTL_MS = 1_800_000`; `leagueFixtureKey(id)`; `latestGameVersion(release)` helper.

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
import { SiteUnavailable } from "../src/league/site.js";

let n = 0;
async function signedIn(pool, clock) {
  const p = (await pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`4${++n}000000`, `P${n}`])).rows[0];
  const raw = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at, last_used_at) VALUES ($1, $2, 't', $3, $3)`, [p.id, hashToken(raw), clock.now()]);
  return { ...p, bearer: raw };
}
function fakeSite(answers) {
  const site = { calls: 0, down: false,
    async teamHome(id) { site.calls++; if (site.down) throw new SiteUnavailable("down"); return answers[id] ?? { ok: true, member: false }; },
    async isMember(id) { return (await site.teamHome(id)).member === true; },
    logoSource: () => null };
  return site;
}
const fakeRelease = (v = "2026.10.08") => ({ versions: async () => ({ "league-game": v, "league-content": "1.0.0" }), component: async () => { throw new Error("unused"); } });
const fixture = (me, opp, { home = true, status = "scheduled", oppDiscord = opp.discord_user_id, week = 6 } = {}) => ({
  ok: true, member: true, schedule_published: true, league: { id: 36, season: 4, week: 7, week_label: "Week 7" },
  team: { id: home ? 493 : 486, name: "Me", logo_url: null },
  matchup: { fixture_id: 1234, week, home, status, forfeit: false, score: null, series: null, winner_team_id: null, when: null,
    opponent: { team_id: home ? 486 : 493, name: "Them", logo_url: null, discord_user_id: oppDiscord } },
  week_games: [], standings: [], recent: [], leaders: { pass: [], rush: [], rec: [] }, leaders_unit: "TD", roster: [], site_url: "x",
});
let codeN = 0;
const codes = () => `LG${String(++codeN).padStart(4, "2")}`.slice(0, 6).replace(/[01IO]/g, "2");
async function serve(t, clock, site, release = fakeRelease()) {
  const app = express();
  app.use(express.json());
  app.use(leagueRouter({ pool: t.pool, clock, site, release, publicOrigin: "https://api.test", newCode: codes }));
  app.use((req, res) => res.status(404).json({ ok: false }));
  app.use((err, req, res, next) => res.status(500).json({ ok: false, error: String(err?.message ?? err) })); // eslint-disable-line no-unused-vars
  const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
  const base = `http://127.0.0.1:${server.address().port}`;
  const call = async (method, path, bearer, body) => {
    const r = await fetch(base + path, { method, headers: { authorization: `Bearer ${bearer}`, ...(body ? { "content-type": "application/json" } : {}) }, body: body ? JSON.stringify(body) : undefined });
    return { status: r.status, json: await r.json() };
  };
  return { call, close: () => new Promise((r) => { server.closeAllConnections?.(); server.close(r); }) };
}

test("first click creates a locked bo3 league-seed room tagged with matchup.week; the opponent is told to join with the version", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
  const site = fakeSite({ [a.discord_user_id]: fixture(a, b, { home: true }), [b.discord_user_id]: fixture(b, a, { home: false }) });
  const s = await serve(t, clock, site);
  try {
    const first = await s.call("POST", "/v1/league/room", a.bearer, { league_game_version: "2026.10.08" });
    assert.equal(first.status, 200, JSON.stringify(first.json));
    assert.equal(first.json.action, "created");
    const room = first.json.room;
    assert.equal(room.best_of, 3); assert.equal(room.seed_kind, "regular"); assert.equal(room.you_host, true);
    assert.deepEqual(room.league, { league_id: 36, fixture_id: 1234, week: 6 }, "matchup.week (6), not league.week (7)");
    assert.equal(room.edition, "league"); assert.equal(room.ruleset, "league"); assert.equal(room.league_game_version, "2026.10.08");
    assert.equal(new Date(room.expires_at).getTime() - clock.now().getTime(), LEAGUE_ROOM_TTL_MS);
    const row = (await t.pool.query(`SELECT allowed_discord_ids, league_seat0_home FROM mm_room WHERE id = $1`, [room.id])).rows[0];
    assert.deepEqual(row.allowed_discord_ids.sort(), [a.discord_user_id, b.discord_user_id].sort());
    assert.equal(row.league_seat0_home, true);

    const again = await s.call("POST", "/v1/league/room", a.bearer);
    assert.equal(again.json.action, "created"); assert.equal(again.json.room.id, room.id, "idempotent for the host");

    const stale = await s.call("POST", "/v1/league/room", b.bearer, { league_game_version: "2026.09.01" });
    assert.equal(stale.status, 409);
    assert.deepEqual(stale.json, { ok: false, error: "league_version_mismatch", required: "2026.10.08" });

    const second = await s.call("POST", "/v1/league/room", b.bearer, { league_game_version: "2026.10.08" });
    assert.deepEqual(second.json, { ok: true, action: "join", code: room.code, league_game_version: "2026.10.08" });
  } finally { await s.close(); await t.close(); }
});

test("the host's installed version wins over the published one", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
  const s = await serve(t, clock, fakeSite({ [a.discord_user_id]: fixture(a, b) }));
  try {
    const r = await s.call("POST", "/v1/league/room", a.bearer, { league_game_version: "2026.10.01" });
    assert.equal(r.json.room.league_game_version, "2026.10.01");
  } finally { await s.close(); await t.close(); }
});

test("two clicks racing on the fixture produce ONE room", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
  const site = fakeSite({ [a.discord_user_id]: fixture(a, b, { home: true }), [b.discord_user_id]: fixture(b, a, { home: false }) });
  const s = await serve(t, clock, site);
  try {
    let release; const held = new Promise((r) => { release = r; });
    const holder = withTx(t.pool, async (c) => { await lockRoom(c, leagueFixtureKey(1234)); await held; });
    await new Promise((r) => setTimeout(r, 100));
    const both = Promise.all([s.call("POST", "/v1/league/room", a.bearer), s.call("POST", "/v1/league/room", b.bearer)]);
    await new Promise((r) => setTimeout(r, 200));
    release(); await holder;
    const [x, y] = await both;
    assert.deepEqual([x.json.action, y.json.action].sort(), ["created", "join"]);
    const cnt = (await t.pool.query(`SELECT count(*)::int AS n FROM mm_room WHERE league_fixture_id = 1234`)).rows[0].n;
    assert.equal(cnt, 1);
  } finally { await s.close(); await t.close(); }
});

test("refusals: not a member, bye, final, opponent unlinked, site down", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
  const bye = fixture(a, b); bye.matchup = { status: "bye", fixture_id: null, week: 7, home: null, forfeit: false, opponent: null };
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
  const s = await serve(t, clock, fakeSite({ [a.discord_user_id]: fixture(a, b, { status: "final" }) }));
  try {
    const m = (await t.pool.query(
      `INSERT INTO match (mode, status, best_of, seat0_player, seat1_player, source, created_at, league_id, league_fixture_id, league_week, league_seat0_home, edition, ruleset)
       VALUES ('casual', 'running', 3, $1, $2, 'league', $3, 36, 1234, 6, true, 'league', 'league') RETURNING id`, [a.id, b.id, clock.now()])).rows[0];
    const r = await s.call("POST", "/v1/league/room", a.bearer);
    assert.equal(r.status, 409); assert.equal(r.json.error, "in_series"); assert.equal(r.json.match_id, m.id);
  } finally { await s.close(); await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --env-file-if-exists=.env --test test/league_room_open.test.js` → FAIL (`Cannot find module '../src/league/router.js'`).

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
 * @param {object} home     the caller's FRESH team_home answer (member, with a matchup)
 * @param {string|null} version        the league-game version a NEW room requires (caller's installed, else latest)
 * @param {string|null} callerVersion  what the caller sent; checked against an already-open room before "join"
 */
export async function openLeagueRoom(pool, clock, { player, home, version = null, callerVersion = null, newCode = () => newRoomCode(), ttlMs = LEAGUE_ROOM_TTL_MS }) {
  const mu = home.matchup;
  const fixtureId = Number(mu.fixture_id);
  const week = Number(mu.week);   // the per-team week (Plan 1 fix), never league.week
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
          `SELECT id, code, host_player, expires_at, league_game_version FROM mm_room WHERE league_fixture_id = $1 AND status = 'open'`, [fixtureId])).rows[0];
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
          // Same check the join route makes, said here first so the launcher shows "Update league files" instead of
          // sending the player to a join that will refuse them.
          if (callerVersion && open.league_game_version && callerVersion !== open.league_game_version) {
            return answer(409, { ok: false, error: "league_version_mismatch", required: open.league_game_version });
          }
          return answer(200, { ok: true, action: "join", code: open.code, league_game_version: open.league_game_version ?? null });
        }

        // The site's "final" only stands when nothing of ours is open or running (Plan 1: status=="final" is the
        // only final; winner_team_id is the series leader mid-series and is never read here).
        if (mu.status === "final") return answer(409, { ok: false, error: "already_final" });

        await assertNotSeated(c, player.id, now);
        const room = (await c.query(
          `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, seed_kind, created_at, expires_at, capacity,
                                league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids,
                                edition, ruleset, league_game_version)
           VALUES ($1, $2, 'casual', 'code', 3, 'regular', $3, $4, 2, $5, $6, $7, $8, $9, 'league', 'league', $10) RETURNING id`,
          [newCode(), player.id, now, new Date(now.getTime() + ttlMs), Number(home.league.id), fixtureId,
            week, mu.home === true, [me, opp], version])).rows[0];
        await c.query(
          `INSERT INTO mm_room_member (room_id, player_id, ready, field_pick, joined_at, active, party) VALUES ($1, $2, false, NULL, $3, true, false)`,
          [room.id, player.id, now]);
        await appendEvent(c, {
          roomId: room.id, actor: actorPlayer(player.id), kind: EVENT_KINDS.ROOM_CREATED, at: now,
          payload: { mode: "casual", visibility: "code", best_of: 3, seed_kind: "regular", capacity: 2, edition: "league", ruleset: "league",
            league: { league_id: Number(home.league.id), fixture_id: fixtureId, week, opponent_discord_user_id: opp } },
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

> Check `appendEvent`'s argument shape against how rooms.js's leave writes `reason` (top-level there); mirror it.

- [ ] **Step 4: Implement `src/league/router.js` (room route; later tasks add the rest)**

```js
// src/league/router.js
// The launcher's League tab (spec 2026-10-08 v2): /v1/league/me|room|friendly|download and the league logo proxy.
// Membership is decided by the league site on EVERY call; the launcher's own flag only decides whether to draw a nav link.
import express from "express";
import { requirePlayer } from "../auth.js";
import { SiteUnavailable } from "./site.js";
import { openLeagueRoom, leagueFixtureKey } from "./room.js";

export { leagueFixtureKey };

const unavailable = (res) => res.status(503).json({ ok: false, error: "league_unavailable" });
const jsonBody = express.json({ limit: "4kb" });

/** The latest published league-game version, or null when the release cannot be read (never fails a request). */
export async function latestGameVersion(release) {
  try { return (await release?.versions?.())?.["league-game"] ?? null; } catch { return null; }
}
/** A version string from a body, or null. */
const bodyVersion = (b) => (typeof b?.league_game_version === "string" && b.league_game_version.length > 0 && b.league_game_version.length <= 64 ? b.league_game_version : null);

/** Map a rooms.js-style refusal (status + player sentence) or the one-active-room index to an answer. */
function refusal(res, e) {
  if (typeof e?.status === "number" && e.expose) return res.status(e.status).json({ ok: false, error: e.message });
  if (e?.code === "23505" && String(e.constraint ?? "").includes("one_active")) return res.status(409).json({ ok: false, error: "You are already in a lobby." });
  return null;
}

export function leagueRouter({ pool, clock, site, release = null, r2 = null, hub = null, publicOrigin = "", newCode = undefined, fetchImpl = fetch } = {}) {
  if (!pool || !clock || !site) throw new Error("leagueRouter needs pool, clock and site");
  const r = express.Router();
  const auth = requirePlayer(pool, clock);
  const wake = (channels) => { try { hub?.notifyAll?.(channels ?? []); } catch { /* after commit; never a 500 */ } };

  r.post("/v1/league/room", jsonBody, auth, async (req, res, next) => {
    try {
      let home;
      try { home = await site.teamHome(req.player.discord_user_id, { fresh: true }); }
      catch (e) { if (e instanceof SiteUnavailable) return unavailable(res); throw e; }
      if (!home.member) return res.status(403).json({ ok: false, error: "not_member" });
      const mu = home.matchup;
      if (!mu || mu.status === "bye" || mu.fixture_id === null || !Number.isSafeInteger(Number(mu.fixture_id))) {
        return res.status(409).json({ ok: false, error: "no_game" });
      }
      if (!mu.opponent?.discord_user_id) return res.status(409).json({ ok: false, error: "opponent_unlinked" });
      const callerVersion = bodyVersion(req.body);
      const version = callerVersion ?? await latestGameVersion(release);
      const out = await openLeagueRoom(pool, clock, { player: req.player, home, version, callerVersion, ...(newCode ? { newCode } : {}) });
      wake(out.channels);
      res.status(out.status).json(out.body);
    } catch (e) { if (!refusal(res, e)) next(e); }
  });

  return r;
}
```

- [ ] **Step 5: Run tests**

Run: `node --env-file-if-exists=.env --test test/league_room_open.test.js` → PASS (6). Ablation (then restore): delete the `lockRoom(c, leagueFixtureKey(...))` line; the race test must FAIL. Restore, re-run green.

- [ ] **Step 6: Commit**

```powershell
git add src/league/room.js src/league/router.js test/league_room_open.test.js
git commit -m "league: POST /v1/league/room (fixture-locked, league seed, bo3, version, created|join)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Friendlies (`src/league/friendly.js` + `POST /v1/league/friendly`)

**Files:**
- Create: `src/league/friendly.js`
- Modify: `src/league/router.js`
- Test: `test/league_friendly.test.js`

**Interfaces:**
- Consumes: `site.isMember`, `latestGameVersion`, rooms.js `assertNotSeated`, `roomView`, `newRoomCode`, `ROOM_CODE_ATTEMPTS`, `ROOM_TTL_MS`.
- Produces: `openFriendlyRoom(pool, clock, { player, ruleset, version, newCode, ttlMs })` -> `{ status, body, channels }`; `FRIENDLY_RULESETS = ["league", "raw"]`.

- [ ] **Step 1: Write the failing tests**

```js
// test/league_friendly.test.js
import test from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { newToken, hashToken } from "../src/tokens.js";
import { leagueRouter } from "../src/league/router.js";
import { roomsRouter, ROOM_TTL_MS } from "../src/mm/rooms.js";
import { SiteUnavailable } from "../src/league/site.js";

let n = 0;
async function signedIn(pool, clock) {
  const p = (await pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`6${++n}000000`, `P${n}`])).rows[0];
  const raw = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at, last_used_at) VALUES ($1, $2, 't', $3, $3)`, [p.id, hashToken(raw), clock.now()]);
  return { ...p, bearer: raw };
}
async function serve(t, clock, members, { down = () => false } = {}) {
  const site = {
    async teamHome(id) { if (down()) throw new SiteUnavailable("x"); return { ok: true, member: members.has(String(id)) }; },
    async isMember(id) { return (await site.teamHome(id)).member === true; },
    logoSource: () => null };
  const release = { versions: async () => ({ "league-game": "2026.10.08", "league-content": "1.0.0" }) };
  const app = express();
  app.use(leagueRouter({ pool: t.pool, clock, site, release, publicOrigin: "https://api.test" }));
  app.use(roomsRouter({ pool: t.pool, clock, leagueMember: (id) => site.isMember(id) }));
  app.use((err, req, res, next) => res.status(500).json({ ok: false, error: String(err?.message ?? err) })); // eslint-disable-line no-unused-vars
  const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
  const base = `http://127.0.0.1:${server.address().port}`;
  const call = async (method, path, bearer, body) => {
    const r = await fetch(base + path, { method, headers: { authorization: `Bearer ${bearer}`, ...(body ? { "content-type": "application/json" } : {}) }, body: body ? JSON.stringify(body) : undefined });
    return { status: r.status, json: await r.json() };
  };
  return { call, close: () => new Promise((r) => { server.closeAllConnections?.(); server.close(r); }) };
}

test("a member hosts a league-seed friendly by default (bo1, league edition, not a fixture); another member joins by code", async () => {
  const t = await testPool(); const clock = fixedClock(); const members = new Set();
  const s = await serve(t, clock, members);
  try {
    const a = await signedIn(t.pool, clock); const b = await signedIn(t.pool, clock);
    members.add(a.discord_user_id); members.add(b.discord_user_id);
    const made = await s.call("POST", "/v1/league/friendly", a.bearer, {});
    assert.equal(made.status, 200, JSON.stringify(made.json));
    assert.equal(made.json.action, "created");
    const room = made.json.room;
    assert.equal(room.edition, "league"); assert.equal(room.ruleset, "league"); assert.equal(room.best_of, 1);
    assert.equal(room.league, null); assert.equal(room.league_game_version, "2026.10.08"); assert.equal(room.you_host, true);
    assert.equal(new Date(room.expires_at).getTime() - clock.now().getTime(), ROOM_TTL_MS);
    const j = await s.call("POST", `/v1/mm/rooms/${room.code}/join`, b.bearer, { league_game_version: "2026.10.08" });
    assert.equal(j.status, 200, JSON.stringify(j.json));
  } finally { await s.close(); await t.close(); }
});

test("raw ruleset and the host's version are honoured; a bad ruleset is 400", async () => {
  const t = await testPool(); const clock = fixedClock(); const members = new Set();
  const s = await serve(t, clock, members);
  try {
    const a = await signedIn(t.pool, clock); members.add(a.discord_user_id);
    const bad = await s.call("POST", "/v1/league/friendly", a.bearer, { ruleset: "ranked" });
    assert.equal(bad.status, 400); assert.equal(bad.json.error, "ruleset must be league or raw.");
    const raw = await s.call("POST", "/v1/league/friendly", a.bearer, { ruleset: "raw", league_game_version: "2026.10.01" });
    assert.equal(raw.status, 200, JSON.stringify(raw.json));
    assert.equal(raw.json.room.ruleset, "raw"); assert.equal(raw.json.room.league_game_version, "2026.10.01");
  } finally { await s.close(); await t.close(); }
});

test("refusals: not a member 403, already in a lobby 409, site down 503", async () => {
  const t = await testPool(); const clock = fixedClock(); const members = new Set(); let isDown = false;
  const s = await serve(t, clock, members, { down: () => isDown });
  try {
    const a = await signedIn(t.pool, clock);
    const no = await s.call("POST", "/v1/league/friendly", a.bearer, {});
    assert.equal(no.status, 403); assert.equal(no.json.error, "not_member");
    members.add(a.discord_user_id);
    await s.call("POST", "/v1/league/friendly", a.bearer, {});
    const twice = await s.call("POST", "/v1/league/friendly", a.bearer, {});
    assert.equal(twice.status, 409); assert.equal(twice.json.error, "You are already in a lobby.");
    isDown = true;
    const down = await s.call("POST", "/v1/league/friendly", a.bearer, {});
    assert.equal(down.status, 503); assert.equal(down.json.error, "league_unavailable");
  } finally { await s.close(); await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure** → FAIL (404 on `/v1/league/friendly`).

- [ ] **Step 3: Implement `src/league/friendly.js`**

```js
// src/league/friendly.js
// A members-only friendly on the League edition: an ordinary private code room (best of 1, not a fixture) that boots
// the league ISO with the league seed (default, Adam 10-08: "default on, go with seed") or the raw game. Membership
// is enforced again on join by rooms.js's edition gate, so a code passed to a non-member is a bad code.
import { bump } from "../mm/cursor.js";
import { appendEvent, EVENT_KINDS, actorPlayer } from "../mm/events.js";
import { assertNotSeated, roomView, newRoomCode, ROOM_CODE_ATTEMPTS, ROOM_TTL_MS } from "../mm/rooms.js";
import { withTx } from "../db.js";

export const FRIENDLY_RULESETS = Object.freeze(["league", "raw"]);

export async function openFriendlyRoom(pool, clock, { player, ruleset = "league", version = null, newCode = () => newRoomCode(), ttlMs = ROOM_TTL_MS }) {
  for (let attempt = 0; attempt < ROOM_CODE_ATTEMPTS; attempt++) {
    try {
      return await withTx(pool, async (c) => {
        const now = clock.now();
        await assertNotSeated(c, player.id, now);
        const room = (await c.query(
          `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, seed_kind, created_at, expires_at, capacity,
                                edition, ruleset, league_game_version)
           VALUES ($1, $2, 'casual', 'code', 1, 'regular', $3, $4, 2, 'league', $5, $6) RETURNING id`,
          [newCode(), player.id, now, new Date(now.getTime() + ttlMs), ruleset, version])).rows[0];
        await c.query(
          `INSERT INTO mm_room_member (room_id, player_id, ready, field_pick, joined_at, active, party) VALUES ($1, $2, false, NULL, $3, true, false)`,
          [room.id, player.id, now]);
        await appendEvent(c, {
          roomId: room.id, actor: actorPlayer(player.id), kind: EVENT_KINDS.ROOM_CREATED, at: now,
          payload: { mode: "casual", visibility: "code", best_of: 1, seed_kind: "regular", capacity: 2, edition: "league", ruleset },
        });
        const channels = await bump(c, [player.id]);
        return { status: 200, body: { ok: true, action: "created", room: await roomView(c, room.id, player.id) }, channels };
      });
    } catch (e) {
      if (!(e?.code === "23505" && String(e.constraint ?? "").includes("code"))) throw e;
    }
  }
  return { status: 503, body: { ok: false, error: "Could not open a lobby just now. Try again." }, channels: [] };
}
```

- [ ] **Step 4: Add the route to `src/league/router.js`**

Import `import { openFriendlyRoom, FRIENDLY_RULESETS } from "./friendly.js";`. Before `return r;`:

```js
  r.post("/v1/league/friendly", jsonBody, auth, async (req, res, next) => {
    try {
      const ruleset = req.body?.ruleset ?? "league";
      if (!FRIENDLY_RULESETS.includes(ruleset)) return res.status(400).json({ ok: false, error: "ruleset must be league or raw." });
      let member;
      try { member = await site.isMember(req.player.discord_user_id); }
      catch (e) { if (e instanceof SiteUnavailable) return unavailable(res); throw e; }
      if (!member) return res.status(403).json({ ok: false, error: "not_member" });
      const version = bodyVersion(req.body) ?? await latestGameVersion(release);
      const out = await openFriendlyRoom(pool, clock, { player: req.player, ruleset, version, ...(newCode ? { newCode } : {}) });
      wake(out.channels);
      res.status(out.status).json(out.body);
    } catch (e) { if (!refusal(res, e)) next(e); }
  });
```

- [ ] **Step 5: Run tests** → `test/league_friendly.test.js` PASS (3).

- [ ] **Step 6: Commit**

```powershell
git add src/league/friendly.js src/league/router.js test/league_friendly.test.js
git commit -m "league: POST /v1/league/friendly (members-only league-edition room, league seed default, raw option)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: `GET /v1/league/me` with live merge and edition versions

**Files:**
- Create: `src/league/state.js`, `src/league/merge.js`
- Modify: `src/league/router.js`
- Test: `test/league_merge.test.js` (pure), `test/league_me.test.js` (HTTP + DB)

**Interfaces:**
- Consumes: `playerLive(pool, playerId)` from `src/live.js` (~554; `{series:[mine,theirs], score:[mine,theirs]|null}` from that player's side, or null).
- Produces: `leagueState(pool, clock, fixtureIds)` -> `{ rooms: Map<fixtureId, {room_id, code, host_discord_user_id, members, expires_at, league_game_version}>, live: Map<fixtureId, {match_id, seat0_home, host_discord_user_id, series:[s0,s1], score:[s0,s1]|null}> }`; `onlineByDiscord(pool, clock, discordId)`; `PRESENCE_WINDOW_MS = 90_000`; `mergeLeague(site, state, { publicOrigin, opponentOnline, editionVersions })`.

- [ ] **Step 1: Write the failing pure tests**

```js
// test/league_merge.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { mergeLeague } from "../src/league/merge.js";

const base = () => ({
  ok: true, member: true, schedule_published: true, league: { id: 36, week: 6 },
  team: { id: 493, logo_url: "https://nflstreetreborn.com/league/uploads/493.png" },
  matchup: { fixture_id: 1234, week: 6, home: true, status: "final", forfeit: false, score: { home: 36, away: 20 }, series: { home: 1, away: 0 }, winner_team_id: 493, when: null,
    opponent: { team_id: 486, logo_url: null, discord_user_id: "480" } },
  week_games: [
    { fixture_id: 1234, home: { team_id: 493, logo_url: "https://nflstreetreborn.com/league/uploads/493.png" }, away: { team_id: 486, logo_url: null }, status: "final", forfeit: false, score: { home: 36, away: 20 }, series: { home: 1, away: 0 }, winner_team_id: 493 },
    { fixture_id: 1235, home: { team_id: 482, logo_url: null }, away: { team_id: 483, logo_url: null }, status: "scheduled", forfeit: false, score: null, series: null, winner_team_id: null },
  ],
  standings: [{ team_id: 493, logo_url: "https://nflstreetreborn.com/league/uploads/493.png" }],
  recent: [], leaders: { pass: [], rush: [], rec: [] }, leaders_unit: "TD", roster: [], site_url: "x",
});
const empty = () => ({ rooms: new Map(), live: new Map() });
const ev = { "league-game": "2026.10.08", "league-content": "1.0.0" };
const opts = { publicOrigin: "https://api.nflstreetreborn.com", opponentOnline: false, editionVersions: ev };

test("logos rewritten to the proxy; null stays null; edition_versions attached; non-members pass through", () => {
  const out = mergeLeague(base(), empty(), opts);
  assert.equal(out.team.logo_url, "https://api.nflstreetreborn.com/live-assets/league/493.png");
  assert.equal(out.week_games[0].home.logo_url, "https://api.nflstreetreborn.com/live-assets/league/493.png");
  assert.equal(out.standings[0].logo_url, "https://api.nflstreetreborn.com/live-assets/league/493.png");
  assert.equal(out.matchup.opponent.logo_url, null);
  assert.equal(out.matchup.room, null); assert.equal(out.matchup.match_id, null); assert.equal(out.matchup.opponent.online, false);
  assert.deepEqual(out.edition_versions, ev);
  assert.deepEqual(mergeLeague(base(), empty(), { ...opts, editionVersions: undefined }).edition_versions, { "league-game": null, "league-content": null });
  assert.deepEqual(mergeLeague({ ok: true, member: false }, empty(), opts), { ok: true, member: false });
});

test("a running league match makes the fixture LIVE with series and score mapped from seat to home/away", () => {
  const st = empty();
  st.live.set(1234, { match_id: "m-1", seat0_home: false, host_discord_user_id: "480", series: [0, 1], score: [7, 14] });
  const out = mergeLeague(base(), st, opts);
  const g = out.week_games[0];
  assert.equal(g.status, "live"); assert.equal(g.match_id, "m-1");
  assert.deepEqual(g.series, { home: 1, away: 0 }); assert.deepEqual(g.score, { home: 14, away: 7 });
  assert.equal(out.matchup.status, "live", "the site's early 'final' after game 1 is overridden");
  assert.deepEqual(out.matchup.room, { room_id: null, code: null, host_discord_user_id: "480", members: 2, expires_at: null, state: "in_series", league_game_version: null });
  assert.equal(out.week_games[1].status, "scheduled", "other fixtures untouched");
});

test("an open room: waiting or lobby; a site 'final' with an open room reads in_progress; the room's version is shown", () => {
  const st = empty();
  st.rooms.set(1234, { room_id: "r-1", code: "K7QP2M", host_discord_user_id: "111", members: 1, expires_at: "2027-01-01T00:00:00.000Z", league_game_version: "2026.10.08" });
  let out = mergeLeague(base(), st, opts);
  assert.equal(out.matchup.status, "in_progress");
  assert.equal(out.week_games[0].status, "in_progress");
  assert.deepEqual(out.matchup.room, { room_id: "r-1", code: "K7QP2M", host_discord_user_id: "111", members: 1, expires_at: "2027-01-01T00:00:00.000Z", state: "waiting", league_game_version: "2026.10.08" });
  st.rooms.get(1234).members = 2;
  out = mergeLeague(base(), st, opts);
  assert.equal(out.matchup.room.state, "lobby");
  const sched = base(); sched.matchup.status = "scheduled"; sched.week_games[0].status = "scheduled";
  assert.equal(mergeLeague(sched, st, opts).matchup.status, "scheduled", "an open room does not invent progress");
});

test("a live score of null stays null; a bye matchup passes through with room null", () => {
  const st = empty();
  st.live.set(1234, { match_id: "m-1", seat0_home: true, host_discord_user_id: "1", series: [0, 0], score: null });
  assert.equal(mergeLeague(base(), st, opts).week_games[0].score, null);
  const bye = base(); bye.matchup = { status: "bye", fixture_id: null, week: 6, home: null, forfeit: false, opponent: null, score: null, series: null, winner_team_id: null, when: null };
  const out = mergeLeague(bye, st, opts);
  assert.equal(out.matchup.status, "bye"); assert.equal(out.matchup.room, null);
});
```

- [ ] **Step 2: Run to verify failure** → FAIL (module missing).

- [ ] **Step 3: Implement `src/league/merge.js`**

```js
// src/league/merge.js
// Pure: the site's team_home answer + this server's league rooms and running league matches -> what the launcher
// draws. The site knows the schedule and recorded results; only this server knows a series is being played right now.
// Plan 1 semantics: the site can call a best-of-3 "final" after game 1 -- an open room or a running match here
// always wins over that.

const NO_VERSIONS = Object.freeze({ "league-game": null, "league-content": null });
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

export function mergeLeague(site, state, { publicOrigin, opponentOnline = false, editionVersions = null } = {}) {
  if (!site?.member) return { ok: true, member: false };
  const o = String(publicOrigin ?? "").replace(/\/+$/, "");
  const out = { ...site, edition_versions: { ...NO_VERSIONS, ...(editionVersions ?? {}) } };
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
    ? { room_id: null, code: null, host_discord_user_id: live.host_discord_user_id, members: 2, expires_at: null, state: "in_series", league_game_version: null }
    : room
      ? { room_id: room.room_id, code: room.code, host_discord_user_id: room.host_discord_user_id, members: room.members, expires_at: room.expires_at,
          state: room.members >= 2 ? "lobby" : "waiting", league_game_version: room.league_game_version ?? null }
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
import { ReleaseUnavailable } from "../src/league/release.js";

let n = 0;
async function signedIn(pool, clock, lastUsed = clock.now()) {
  const p = (await pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`3${++n}000000`, `P${n}`])).rows[0];
  const raw = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at, last_used_at) VALUES ($1, $2, 't', $3, $4)`, [p.id, hashToken(raw), clock.now(), lastUsed]);
  return { ...p, bearer: raw };
}
async function serve(t, clock, site, release) {
  const app = express();
  app.use(leagueRouter({ pool: t.pool, clock, site, release, publicOrigin: "https://api.test" }));
  app.use((err, req, res, next) => res.status(500).json({ ok: false, error: String(err?.message ?? err) })); // eslint-disable-line no-unused-vars
  const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
  const base = `http://127.0.0.1:${server.address().port}`;
  const get = async (bearer) => { const r = await fetch(`${base}/v1/league/me`, { headers: bearer ? { authorization: `Bearer ${bearer}` } : {} }); return { status: r.status, json: await r.json() }; };
  return { get, close: () => new Promise((r) => { server.closeAllConnections?.(); server.close(r); }) };
}
const okRelease = { versions: async () => ({ "league-game": "2026.10.08", "league-content": "1.0.0" }) };
const deadRelease = { versions: async () => { throw new ReleaseUnavailable("x"); } };

test("/v1/league/me: 401 signed out, member:false, 503 when the site is down (never member:false)", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock);
  const site = { down: false, async teamHome() { if (site.down) throw new SiteUnavailable("x"); return { ok: true, member: false }; }, logoSource: () => null };
  const s = await serve(t, clock, site, okRelease);
  try {
    assert.equal((await s.get(null)).status, 401);
    assert.deepEqual((await s.get(a.bearer)).json, { ok: true, member: false });
    site.down = true;
    const down = await s.get(a.bearer);
    assert.equal(down.status, 503); assert.equal(down.json.error, "league_unavailable");
  } finally { await s.close(); await t.close(); }
});

test("/v1/league/me: open room shows waiting with code + version; opponent online; edition_versions", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock);
  const b = await signedIn(t.pool, clock, new Date(clock.now().getTime() - 30_000));
  const home = { ok: true, member: true, schedule_published: true, league: { id: 36, week: 6 }, team: { id: 493, logo_url: null },
    matchup: { fixture_id: 1234, week: 6, home: true, status: "scheduled", forfeit: false, score: null, series: null, winner_team_id: null, when: null,
      opponent: { team_id: 486, logo_url: null, discord_user_id: b.discord_user_id } },
    week_games: [{ fixture_id: 1234, home: { team_id: 493, logo_url: null }, away: { team_id: 486, logo_url: null }, status: "scheduled", forfeit: false, score: null, series: null }],
    standings: [], recent: [], leaders: { pass: [], rush: [], rec: [] }, leaders_unit: "TD", roster: [], site_url: "x" };
  const site = { async teamHome() { return home; }, logoSource: () => null };
  const s = await serve(t, clock, site, okRelease);
  try {
    const r = (await t.pool.query(
      `INSERT INTO mm_room (code, host_player, mode, visibility, best_of, created_at, expires_at, capacity, league_id, league_fixture_id, league_week, league_seat0_home, allowed_discord_ids, edition, ruleset, league_game_version)
       VALUES ('K7QP2M', $1, 'casual', 'code', 3, $2, $3, 2, 36, 1234, 6, true, $4, 'league', 'league', '2026.10.08') RETURNING id`,
      [a.id, clock.now(), new Date(clock.now().getTime() + 60_000), [a.discord_user_id, b.discord_user_id]])).rows[0];
    await t.pool.query(`INSERT INTO mm_room_member (room_id, player_id, joined_at, active) VALUES ($1, $2, $3, true)`, [r.id, a.id, clock.now()]);
    const me = await s.get(a.bearer);
    assert.equal(me.status, 200, JSON.stringify(me.json));
    assert.equal(me.json.matchup.room.state, "waiting");
    assert.equal(me.json.matchup.room.code, "K7QP2M");
    assert.equal(me.json.matchup.room.league_game_version, "2026.10.08");
    assert.equal(me.json.matchup.room.host_discord_user_id, a.discord_user_id);
    assert.equal(me.json.matchup.opponent.online, true);
    assert.deepEqual(me.json.edition_versions, { "league-game": "2026.10.08", "league-content": "1.0.0" });
  } finally { await s.close(); await t.close(); }
});

test("/v1/league/me with an unreadable release still answers, edition_versions null", async () => {
  const t = await testPool(); const clock = fixedClock();
  const a = await signedIn(t.pool, clock);
  const site = { async teamHome() { return { ok: true, member: true, league: { id: 36, week: 6 }, team: { id: 1, logo_url: null },
    matchup: { status: "bye", fixture_id: null, week: 6, home: null, forfeit: false, opponent: null }, week_games: [], standings: [], recent: [],
    leaders: { pass: [], rush: [], rec: [] }, leaders_unit: "TD", roster: [], site_url: "x", schedule_published: false }; }, logoSource: () => null };
  const s = await serve(t, clock, site, deadRelease);
  try {
    const me = await s.get(a.bearer);
    assert.equal(me.status, 200);
    assert.deepEqual(me.json.edition_versions, { "league-game": null, "league-content": null });
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
    `SELECT r.id, r.code, r.league_fixture_id, r.expires_at, r.league_game_version, hp.discord_user_id AS host_discord,
            (SELECT count(*) FROM mm_room_member m WHERE m.room_id = r.id AND m.active)::int AS members
       FROM mm_room r JOIN player hp ON hp.id = r.host_player
      WHERE r.status = 'open' AND r.expires_at > $2 AND r.league_fixture_id = ANY($1::bigint[])`,
    [ids, clock.now()])).rows;
  for (const r of roomRows) rooms.set(Number(r.league_fixture_id), {
    room_id: r.id, code: r.code, host_discord_user_id: r.host_discord, members: r.members,
    expires_at: r.expires_at.toISOString(), league_game_version: r.league_game_version ?? null });
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

> If `player.presence_hidden` is named differently on main, use what `src/mm/presence.js` uses for "hide my presence"; a hidden player reads offline. If `device_token.revoked_at` does not exist, drop that predicate.

- [ ] **Step 7: Add the route to `src/league/router.js`**

Imports: `import { leagueState, onlineByDiscord } from "./state.js";`, `import { mergeLeague } from "./merge.js";`. Before `return r;`:

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
      let editionVersions = null;
      try { editionVersions = await release?.versions?.() ?? null; } catch { editionVersions = null; }
      res.set("cache-control", "no-store");
      res.json(mergeLeague(site_, state, { publicOrigin, opponentOnline, editionVersions }));
    } catch (e) { next(e); }
  });
```

- [ ] **Step 8: Run tests** — `test/league_merge.test.js` (4) and `test/league_me.test.js` (3) → PASS.

- [ ] **Step 9: Commit**

```powershell
git add src/league/state.js src/league/merge.js src/league/router.js test/league_merge.test.js test/league_me.test.js
git commit -m "league: GET /v1/league/me (site + rooms + live series + opponent online + edition_versions)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: `POST /v1/league/download`

**Files:**
- Modify: `src/league/router.js`
- Test: `test/league_download.test.js`

**Interfaces:**
- Consumes: `site.isMember`, `release.component` (Task 4), `r2BlobUrl`, `LEAGUE_DOWNLOAD_TTL_S` (Task 4).
- Produces: the contract route; `leagueRouter({ ..., r2: { accountId, accessKeyId, secretAccessKey, bucket } | null })`.

- [ ] **Step 1: Write the failing test**

```js
// test/league_download.test.js
import test from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { newToken, hashToken } from "../src/tokens.js";
import { leagueRouter } from "../src/league/router.js";
import { SiteUnavailable } from "../src/league/site.js";
import { ReleaseUnavailable } from "../src/league/release.js";

let n = 0;
async function signedIn(pool, clock) {
  const p = (await pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`2${++n}000000`, `P${n}`])).rows[0];
  const raw = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at, last_used_at) VALUES ($1, $2, 't', $3, $3)`, [p.id, hashToken(raw), clock.now()]);
  return { ...p, bearer: raw };
}
const GAME = { version: "2026.10.08", files: [{ path: "NSR-League.iso", bytes: 1460000000, sha256: "aa".repeat(32) }, { path: "textures/a.png", bytes: 10, sha256: "bb".repeat(32) }] };
async function serve(t, clock, { members, siteDown = false, release, r2 = { accountId: "acct", accessKeyId: "K", secretAccessKey: "S", bucket: "reborn-league" } }) {
  const site = { async isMember(id) { if (siteDown) throw new SiteUnavailable("x"); return members.has(String(id)); }, async teamHome() { return { ok: true, member: true }; }, logoSource: () => null };
  const app = express();
  app.use(leagueRouter({ pool: t.pool, clock, site, release, r2, publicOrigin: "https://api.test" }));
  app.use((err, req, res, next) => res.status(500).json({ ok: false, error: String(err?.message ?? err) })); // eslint-disable-line no-unused-vars
  const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
  const base = `http://127.0.0.1:${server.address().port}`;
  const post = async (bearer, body) => { const r = await fetch(`${base}/v1/league/download`, { method: "POST", headers: { authorization: `Bearer ${bearer}`, "content-type": "application/json" }, body: JSON.stringify(body) }); return { status: r.status, json: await r.json() }; };
  return { post, close: () => new Promise((r) => { server.closeAllConnections?.(); server.close(r); }) };
}
const release = { component: async (name) => { if (name === "league-game") return GAME; throw new ReleaseUnavailable("nope"); } };

test("a member gets one presigned 6 h R2 URL per file of the published version", async () => {
  const t = await testPool(); const clock = fixedClock(); const members = new Set();
  const a = await signedIn(t.pool, clock); members.add(a.discord_user_id);
  const s = await serve(t, clock, { members, release });
  try {
    const r = await s.post(a.bearer, { component: "league-game", version: "2026.10.08" });
    assert.equal(r.status, 200, JSON.stringify(r.json));
    assert.equal(r.json.component, "league-game"); assert.equal(r.json.version, "2026.10.08");
    assert.equal(r.json.files.length, 2);
    const f = r.json.files[0];
    assert.equal(f.path, "NSR-League.iso"); assert.equal(f.sha256, "aa".repeat(32)); assert.equal(f.size, 1460000000);
    const u = new URL(f.url);
    assert.equal(u.host, "acct.r2.cloudflarestorage.com");
    assert.equal(u.pathname, `/reborn-league/blobs/by-hash/aa/${"aa".repeat(32)}`);
    assert.equal(u.searchParams.get("X-Amz-Expires"), "21600");
    assert.equal(new Date(r.json.expires_at).getTime() - clock.now().getTime(), 21600 * 1000);
  } finally { await s.close(); await t.close(); }
});

test("refusals: bad component 400, not a member 403, old version 404, site/release/R2 missing 503", async () => {
  const t = await testPool(); const clock = fixedClock(); const members = new Set();
  const a = await signedIn(t.pool, clock); const stranger = await signedIn(t.pool, clock); members.add(a.discord_user_id);
  let s = await serve(t, clock, { members, release });
  try {
    assert.equal((await s.post(a.bearer, { component: "textures", version: "1" })).status, 400);
    const no = await s.post(stranger.bearer, { component: "league-game", version: "2026.10.08" });
    assert.equal(no.status, 403); assert.equal(no.json.error, "not_member");
    const old = await s.post(a.bearer, { component: "league-game", version: "2026.09.01" });
    assert.equal(old.status, 404); assert.equal(old.json.error, "unknown_version");
    const unpub = await s.post(a.bearer, { component: "league-content", version: "1.0.0" });
    assert.equal(unpub.status, 503); assert.equal(unpub.json.error, "league_unavailable");
  } finally { await s.close(); }
  s = await serve(t, clock, { members, release, r2: null });
  try { assert.equal((await s.post(a.bearer, { component: "league-game", version: "2026.10.08" })).status, 503); } finally { await s.close(); }
  s = await serve(t, clock, { members, release, siteDown: true });
  try { assert.equal((await s.post(a.bearer, { component: "league-game", version: "2026.10.08" })).status, 503); } finally { await s.close(); await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure** → FAIL (404 on the route).

- [ ] **Step 3: Implement in `src/league/router.js`**

Imports: `import { ReleaseUnavailable, LEAGUE_COMPONENTS } from "./release.js";` and `import { r2BlobUrl, LEAGUE_DOWNLOAD_TTL_S } from "./presign.js";`. Before `return r;`:

```js
  // Members-only League edition files (spec v2 §4). The blobs sit in a private R2 bucket; a member gets one presigned
  // GET per file, valid 6 h (a 1.5 GB ISO on a slow line must finish; the launcher asks again on a 403). The file list
  // comes from the SIGNED release, so a tampered public bucket cannot widen what this route signs.
  r.post("/v1/league/download", jsonBody, auth, async (req, res, next) => {
    try {
      const component = req.body?.component;
      const version = typeof req.body?.version === "string" ? req.body.version : null;
      if (!LEAGUE_COMPONENTS.includes(component)) return res.status(400).json({ ok: false, error: "component must be league-game or league-content." });
      if (!r2?.accountId || !r2?.accessKeyId || !r2?.secretAccessKey || !r2?.bucket || !release) return unavailable(res);
      let member;
      try { member = await site.isMember(req.player.discord_user_id); }
      catch (e) { if (e instanceof SiteUnavailable) return unavailable(res); throw e; }
      if (!member) return res.status(403).json({ ok: false, error: "not_member" });
      let published;
      try { published = await release.component(component); }
      catch (e) { if (e instanceof ReleaseUnavailable) return unavailable(res); throw e; }
      if (version !== published.version) return res.status(404).json({ ok: false, error: "unknown_version" });
      const now = clock.now();
      const files = published.files.map((f) => ({
        path: f.path, sha256: f.sha256, size: f.bytes,
        url: r2BlobUrl({ ...r2, sha256: f.sha256, expiresSec: LEAGUE_DOWNLOAD_TTL_S, now }),
      }));
      res.set("cache-control", "no-store");
      res.json({ ok: true, component, version: published.version, files,
        expires_at: new Date(now.getTime() + LEAGUE_DOWNLOAD_TTL_S * 1000).toISOString() });
    } catch (e) { next(e); }
  });
```

- [ ] **Step 4: Run tests** → `test/league_download.test.js` PASS (2).

- [ ] **Step 5: Commit**

```powershell
git add src/league/router.js test/league_download.test.js
git commit -m "league: POST /v1/league/download (members-only presigned R2 URLs from the signed release)`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: League logo proxy

**Files:**
- Modify: `src/league/router.js`
- Test: `test/league_logo.test.js`

**Interfaces:**
- Consumes: `site.logoSource(teamId)`, `LOGO_ORIGIN`.
- Produces: `GET /live-assets/league/:file` (`<teamId>.png`); `LOGO_TTL_MS = 86_400_000`, `LOGO_MAX_BYTES = 1_048_576`.

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

- [ ] **Step 2: Run to verify failure** → FAIL (`LOGO_TTL_MS` not exported).

- [ ] **Step 3: Implement in `src/league/router.js`**

Near the top:

```js
import { LOGO_ORIGIN } from "./site.js";
export const LOGO_TTL_MS = 24 * 60 * 60 * 1000;
export const LOGO_MAX_BYTES = 1_048_576;
const IMAGE_TYPES = new Set(["image/png", "image/jpeg", "image/webp", "image/gif"]);
```

(merge the `LOGO_ORIGIN` import into the existing `./site.js` import). Before `return r;`:

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

### Task 12: Mount, env, README, full suite

**Files:**
- Modify: `src/app.js`, `src/server.js`, `README.md`
- Test: `test/league_app.test.js`

- [ ] **Step 1: Write the failing test**

```js
// test/league_app.test.js
import test from "node:test";
import assert from "node:assert/strict";
import { testPool } from "./helpers/db.js";
import { fixedClock, fakeDiscord } from "./helpers/fakes.js";
import { buildApp } from "../src/app.js";

test("app mounts the league routes behind requirePlayer; the logo route 404s an unknown team", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const app = buildApp({ pool: t.pool, clock, discord: fakeDiscord(), env: { MM_ENABLED: "0", PUBLIC_ORIGIN: "https://api.test", STATE_SECRET: "x".repeat(32), ADMIN_TOKEN: "a".repeat(32) } });
    const server = await new Promise((resolve) => { const s = app.listen(0, "127.0.0.1", () => resolve(s)); });
    const base = `http://127.0.0.1:${server.address().port}`;
    try {
      assert.equal((await fetch(`${base}/v1/league/me`)).status, 401);
      for (const p of ["/v1/league/room", "/v1/league/friendly", "/v1/league/download"]) {
        assert.equal((await fetch(base + p, { method: "POST" })).status, 401, p);
      }
      assert.equal((await fetch(`${base}/live-assets/league/1.png`)).status, 404);
    } finally { server.closeAllConnections?.(); server.close(); }
  } finally { await t.close(); }
});
```

> Copy the exact `buildApp` env a passing `test/app.test.js` case uses if the one above misses a required key.

- [ ] **Step 2: Run** → FAIL (`/v1/league/me` is 404).

- [ ] **Step 3: Wire `src/app.js`**

Imports:

```js
import { leagueRouter } from "./league/router.js";
import { leagueSite } from "./league/site.js";
import { releaseReader, DEFAULT_RELEASE_KEYS } from "./league/release.js";
```

BEFORE `createMatchmaking(...)` (it needs `leagueMember`):

```js
  // The launcher's League tab (spec 2026-10-08 v2). The site's token and the R2 keys never leave this process.
  const league = leagueSite({ url: env.LEAGUE_SITE_URL, token: env.LEAGUE_SITE_TOKEN, leagueId: env.LEAGUE_ID, clock });
  const leagueRelease = releaseReader({
    base: env.LEAGUE_ORIGIN_BASE || "https://files.nflstreetreborn.com/",
    publicKeysHex: env.RELEASE_PUBLIC_KEYS ? String(env.RELEASE_PUBLIC_KEYS).split(",").map((s) => s.trim()) : DEFAULT_RELEASE_KEYS,
    clock,
  });
  const leagueR2 = env.R2_ACCOUNT_ID && env.R2_ACCESS_KEY_ID && env.R2_SECRET_ACCESS_KEY
    ? { accountId: env.R2_ACCOUNT_ID, accessKeyId: env.R2_ACCESS_KEY_ID, secretAccessKey: env.R2_SECRET_ACCESS_KEY, bucket: env.LEAGUE_BLOB_BUCKET || "reborn-league" }
    : null;
```

Change the matchmaking construction to pass `leagueMember: (id) => league.isMember(id)`:

```js
  const mm = String(env.MM_ENABLED ?? "1") === "0" ? null : createMatchmaking({ pool, clock, env, leagueMember: (id) => league.isMember(id) });
```

After `app.use(nicknameRouter(...))`:

```js
  // Behind the global /v1 limiter like every player route. Unset LEAGUE_SITE_URL/TOKEN = every /v1/league/* is a 503.
  app.use(leagueRouter({ pool, clock, site: league, release: leagueRelease, r2: leagueR2, hub: mm?.hub ?? null, publicOrigin: env.PUBLIC_ORIGIN ?? "" }));
```

> `/v1/league/room|friendly|download` parse their own JSON (`jsonBody` in router.js); if app.js already mounts a global `express.json()`, the router's parser is a no-op. The logo route sits outside `/v1`, so outside the global limiter; it fetches upstream once per team per 24 h, which is its bound.

- [ ] **Step 4: `src/server.js`** — next to `LEAGUE_FORWARD_URL` in the env object:

```js
  // League tab (optional; never through need()). Site = bot.php + its INGEST_TOKEN. R2 = read-only key for the private
  // League edition bucket. RELEASE_PUBLIC_KEYS defaults to the launcher's compiled keys.
  LEAGUE_SITE_URL: process.env.LEAGUE_SITE_URL ?? "",
  LEAGUE_SITE_TOKEN: process.env.LEAGUE_SITE_TOKEN ?? "",
  LEAGUE_ID: process.env.LEAGUE_ID ?? "",
  LEAGUE_ORIGIN_BASE: process.env.LEAGUE_ORIGIN_BASE ?? "",
  RELEASE_PUBLIC_KEYS: process.env.RELEASE_PUBLIC_KEYS ?? "",
  R2_ACCOUNT_ID: process.env.R2_ACCOUNT_ID ?? "",
  R2_ACCESS_KEY_ID: process.env.R2_ACCESS_KEY_ID ?? "",
  R2_SECRET_ACCESS_KEY: process.env.R2_SECRET_ACCESS_KEY ?? "",
  LEAGUE_BLOB_BUCKET: process.env.LEAGUE_BLOB_BUCKET ?? "",
```

- [ ] **Step 5: README**

Env vars section: one line each for the nine variables above (`LEAGUE_SITE_URL` e.g. `https://nflstreetreborn.com/league/api/bot.php`; `LEAGUE_SITE_TOKEN` = the site's `INGEST_TOKEN`; `LEAGUE_ID` e.g. `36`; `LEAGUE_ORIGIN_BASE` default `https://files.nflstreetreborn.com/`; `RELEASE_PUBLIC_KEYS` comma-separated 64-hex; `R2_*` = an R2 API token with Object Read on the private bucket only; `LEAGUE_BLOB_BUCKET` default `reborn-league`). Routes section: `GET /v1/league/me`, `POST /v1/league/room`, `POST /v1/league/friendly`, `POST /v1/league/download`, `GET /live-assets/league/<team>.png`, each with one line from the contract; and a note that `POST /v1/mm/rooms/:code/join` accepts `league_game_version`.

- [ ] **Step 6: Full suite**

Run: `npm test`
Expected: baseline count + all new tests, 0 failures.

- [ ] **Step 7: Commit**

```powershell
git add src/app.js src/server.js README.md test/league_app.test.js
git commit -m "league: mount /v1/league routes + logo proxy; LEAGUE_*/R2_* env; leagueMember into rooms`n`n<npm test summary line>`n`nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Deploy dark to Railway `reborn-api` and smoke test

Dark = no shipped launcher calls these routes yet. Prerequisites: Plan 1 live (done) and Task 1 deployed. The download route stays 503 until Plan 4 creates the private bucket and Adam makes the read token; that is expected.

- [ ] **Step 1: Ask Adam before deploying.** The deploy runs migration 029 on the live database and replaces the live API process. Get an explicit yes for this deploy.

- [ ] **Step 2: Set env vars** (Railway MCP `set-variables` or the dashboard), never echoing values:
  - `LEAGUE_SITE_URL=https://nflstreetreborn.com/league/api/bot.php`
  - `LEAGUE_SITE_TOKEN=<site INGEST_TOKEN from C:\Users\Adam\nsr_local\deploy\secret\ingest_token.live.php>`
  - `LEAGUE_ID=36`
  - R2 variables only once Plan 4's private bucket exists and Adam has created a bucket-scoped Object Read token (Cloudflare dashboard -> R2 -> Manage API tokens). Until then leave them unset.

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
curl.exe -s -o NUL -w "%{http_code}`n" -X POST "$api/v1/league/download"        # 401
curl.exe -s -o NUL -w "%{http_code}`n" "$api/live-assets/league/999999.png"    # 404
# Signed-in check with Adam's own launcher token (Adam reads it from the OS vault; never pasted into a log):
curl.exe -s -H "Authorization: Bearer $env:REBORN_TOKEN" "$api/v1/league/me"
```
Expected: 401, 401, 404, and Adam's Season 4 payload (`member:true`, league 36, `schedule_published`, `edition_versions` with nulls until Plan 4 publishes). A 503 means the site URL/token is wrong: fix the env, never leave it half-configured.

- [ ] **Step 5: Record**

Merge `feat/league-tab` into backend `main` only after Adam confirms. Update the league-tab memory note with the deploy commit and smoke results.

---

## Self-review

- **Spec v2 coverage:** `/v1/league/me` incl. live merge + `edition_versions` (T9); 60 s cache, 403 retry, non-JSON = 503, `LEAGUE_ID` (T3, site side T1); fixture room idempotent + race + 30 min + matchup.week + league seed + bo3 (T7, T5); allowed IDs on join/peek (T6); members-only friendlies with league seed default ON / raw OFF (T8, T6); version-match gate (T6); members-only presigned downloads from the signed release (T10, T4); logo proxy (T11); result tagging (T5 `match.league` + launcher `intent.league`); deploy dark (T13). Bot feed dropped (Adam declined the ping).
- **Contracts doc alignment (incl. the controller rulings section):** every league-edition room call takes `league_game_version` (`/v1/league/room`, `/v1/league/friendly`, join; ignored for retail rooms); mismatch = 409 `league_version_mismatch` + `required` on join AND on `/v1/league/room` when the opponent's room is already open; `/v1/league/room` 200 created/join + 409 in_series map to the launcher's `{action: created|join|in_series}`; `POST /v1/league/download` 200/403/404/503 shapes; `POST /v1/league/friendly {ruleset}` -> `{action:"created", room}`; join 404 for non-members, 409 `league_version_mismatch` + `required`; leave = existing route; room/match carry `edition`, `ruleset`, `league_game_version`, `league`. Additive extras: `/v1/league/room` and `/v1/league/friendly` accept `league_game_version` in the body; the `join` answer carries `league_game_version`; `/v1/league/download` returns `component`/`version` alongside `files`/`expires_at`.
- **Plan 1 alignment:** `{ok,member}` everywhere; `status=="final"` is the only final; `winner_team_id` never read; `opponent.discord_user_id` null -> `opponent_unlinked`; `matchup.week` tags the room; `schedule_published`, `forfeit`, `color`, `week_label`, `poid`, `leaders_unit` pass through (merge spreads the site object).
- **Names:** `leagueSite`/`isMember`/`SiteUnavailable`/`LOGO_ORIGIN`, `releaseReader`/`ReleaseUnavailable`/`LEAGUE_COMPONENTS`/`DEFAULT_RELEASE_KEYS`, `presignGet`/`r2BlobUrl`/`blobKey`/`LEAGUE_DOWNLOAD_TTL_S`, `openLeagueRoom`/`LEAGUE_ROOM_TTL_MS`/`leagueFixtureKey`, `openFriendlyRoom`/`FRIENDLY_RULESETS`, `leagueRouter`/`latestGameVersion`, `leagueState`/`onlineByDiscord`/`mergeLeague`, `LEAGUE_BEST_OF`, `leagueOf`, `assertNotSeated`, `leagueMember` — each defined once and used with the same signature.

## Open risks

1. **Private bucket is Plan 4's.** Members-only blobs must be in a bucket with NO public custom domain (the public manifest lists every sha256, so a public bucket would make the files public). This plan assumes `reborn-league` with the origin's `blobs/by-hash/<xx>/<sha>` layout; Plan 4 must create it that way, and Adam must mint a bucket-scoped read token for the backend.
2. **Launcher must send `league_game_version` on join and `intent.league=true` + fixture on league uploads**, and boot the ISO from `edition` and the seed from `ruleset` (Plans 3a/3b).
3. **`playerLive` score is seat 0's side and needs a play with ports**; between games it reads `[0,0]` and the series score carries the meaning.
4. **Migration 029 collision** with another in-flight backend branch (Task 0 Step 3 checks).
5. **Site `final` after game 1 with nothing open** (both players quit mid-series): `/v1/league/room` answers `already_final`; a commissioner fixes the site row.
6. **Friendly membership is checked at join time only.** Someone removed from the league mid-lobby stays in that lobby until it ends; acceptable for friendlies.
