# Reborn Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and deploy the Reborn backend: Discord device sign-in with revocable per-device tokens, a results endpoint that stores every game once with its participants, a per-player career record, and server-to-server forwarding of league games to the league site.

**Architecture:** A single Node service on Railway with a Postgres addon. Express 5 routes call small plain modules (canonical hash, identity, results, forwarding, admin). No ORM; numbered SQL migrations run at boot. Every module takes its dependencies (pool, clock, Discord client, fetch) as arguments so tests inject fakes and run against a throwaway schema in a real Postgres.

**Tech Stack:** Node 24 (engines `>=20`), ESM, `express` 5, `pg` 8, `node:test`, `node:crypto`, built-in `fetch`. Railway (Nixpacks) with Postgres addon. Cloudflare DNS for `api.nflstreetreborn.com`.

**Spec:** `docs/superpowers/specs/2026-09-08-reborn-dolphin-and-stats-pipeline-design.md` (sections 4, 6, 7, 8.1, 11, 12) in the launcher repo.

## Global Constraints

- New repository at `C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend`, private GitHub repo, Railway root directory = repository root.
- Node `>=20` (machine has v24.16.0), ESM only (`"type": "module"`), no TypeScript, no ORM, no build step.
- Body cap 512 KB on `POST /v1/results`. 60 requests per minute per token.
- Tokens: 32 random bytes, base64url on the wire, stored as SHA-256 hex. Device code TTL 600 s, poll interval 3 s. Discord scope `identify` only.
- Canonical hash recipe = `si_capture_hash()` from `league/_inc/stat_import.php`, proven byte for byte against the six real captures before anything else is built on it.
- Idempotency by unique key, never by SELECT first (spec 7.2 step 2).
- Hold codes (status `held`): `no-players`, `online-signals-disagree`, `tied-final-score`, `no-seat-claim`, `seat-conflict`. Incomplete code: `below-target-score` (higher score < 36). Everything else is an annotation.
- Offline games private to the player unless `player.show_offline` is true. Online games public.
- No Railway CLI, no local Postgres, no Docker on this machine (checked 2026-09-08). Tests run against `DATABASE_URL_TEST`.
- Secrets never printed into a transcript: token values are written to files or Railway variables, never echoed.
- US English in every user-facing string. No em dashes.

---

## File structure

```
Reborn Backend/
  package.json                 name, scripts (start, test, migrate), deps
  railway.json                 Nixpacks, start command, restart policy
  .env.example                 variable NAMES only
  .gitignore                   node_modules, .env
  README.md                    how to run, test, deploy, rotate
  migrations/001_init.sql      every table in spec 4.3 and 7.1 plus schema_migrations
  src/canonical.js             wireHash(capture) -> sha256 hex, phpTrim, canonicalBytes
  src/db.js                    createPool(url, {schema}), migrate(pool), withTx(pool, fn)
  src/tokens.js                newToken(), hashToken(), newDeviceCode(), newUserCode()
  src/discord.js               realDiscord(env): authorizeUrl(state), exchangeCode(code), fetchUser(accessToken)
  src/auth.js                  identity routes and helpers: deviceStart, devicePoll, callback, bearer middleware
  src/results.js               processSubmission(pool, playerId, envelope) -> reply; status rules
  src/players.js               summary and game list queries with visibility rule
  src/forward.js               enqueueForward, runForwards(pool, fetch, env)
  src/admin.js                 held list, resolve, backfill routes behind ADMIN_TOKEN
  src/account.js               sign out, unlink, delete, export
  src/app.js                   buildApp(deps) -> express app; mounts everything; body cap; rate limit
  src/server.js                boot: env, pool, migrate, app.listen, forward timer
  test/helpers/db.js           testPool(): fresh schema per run, migrate, drop on close
  test/helpers/fakes.js        fakeDiscord(), fakeFetch(), fixedClock()
  test/fixtures/captures/*.json   the six real captures
  test/fixtures/expected-hashes.json   PHP-computed hashes for those six
  test/canonical.test.js
  test/auth.test.js
  test/results.test.js
  test/players.test.js
  test/forward.test.js
  test/admin_account.test.js
  tools/php-hash.php           prints si_capture_hash for each file given, using the site's own function
```

---

### Task 1: Repository scaffold and a test database

**Files:**
- Create: `package.json`, `railway.json`, `.env.example`, `.gitignore`, `README.md`

**Interfaces:**
- Produces: `npm test` runs `node --test test/`; `npm start` runs `node src/server.js`; `npm run migrate` runs `node src/db.js`.

- [ ] **Step 1: Create the repository**

```powershell
New-Item -ItemType Directory "C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend" | Out-Null
Set-Location "C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend"
git init -b main
```

- [ ] **Step 2: Write package.json**

```json
{
  "name": "reborn-backend",
  "version": "1.0.0",
  "private": true,
  "description": "NFL Street Reborn backend: Discord identity, match results, career record",
  "type": "module",
  "engines": { "node": ">=20" },
  "scripts": {
    "start": "node src/server.js",
    "migrate": "node src/db.js",
    "test": "node --test test/"
  },
  "dependencies": {
    "express": "^5.1.0",
    "pg": "^8.13.0"
  }
}
```

- [ ] **Step 3: Write railway.json, .env.example, .gitignore**

`railway.json`:
```json
{
  "$schema": "https://railway.app/railway.schema.json",
  "build": { "builder": "NIXPACKS" },
  "deploy": {
    "startCommand": "node src/server.js",
    "healthcheckPath": "/health",
    "restartPolicyType": "ON_FAILURE",
    "restartPolicyMaxRetries": 10
  }
}
```

`.env.example` (names only, no values):
```
PORT=
DATABASE_URL=
DATABASE_URL_TEST=
DISCORD_CLIENT_ID=
DISCORD_CLIENT_SECRET=
DISCORD_REDIRECT_URI=
STATE_SECRET=
ADMIN_TOKEN=
LEAGUE_FORWARD_URL=
LEAGUE_FORWARD_TOKEN=
PUBLIC_ORIGIN=
```

`.gitignore`:
```
node_modules/
.env
```

- [ ] **Step 4: Install and confirm the toolchain**

Run: `npm install`
Expected: `express` and `pg` under `node_modules`, a `package-lock.json` written.

- [ ] **Step 5: Obtain a test database**

Pick one, record which in `README.md`:

- Local: `winget install PostgreSQL.PostgreSQL.16`, then create a database `reborn_test` and a user; set `DATABASE_URL_TEST=postgres://<user>:<pass>@127.0.0.1:5432/reborn_test` in a local `.env` (never committed).
- Railway: create the project and Postgres addon now (spec 11 step 1), copy the public connection string from the addon's Variables tab into `DATABASE_URL_TEST` in `.env`.

Run: `node -e "import('pg').then(async ({default: pg}) => { const c = new pg.Client({connectionString: process.env.DATABASE_URL_TEST}); await c.connect(); console.log((await c.query('select 1 as ok')).rows); await c.end(); })"` with the variable set in the shell.
Expected: `[ { ok: 1 } ]`.

- [ ] **Step 6: Write README.md**

```markdown
# Reborn Backend

Discord identity, match results and the career record for NFL Street Reborn.
Spec: `NFL Street Reborn Launcher/docs/superpowers/specs/2026-09-08-reborn-dolphin-and-stats-pipeline-design.md`.

## Run
`npm install`, set the variables in `.env.example` in `.env`, `npm start`. Migrations run at boot.

## Test
Set `DATABASE_URL_TEST` to a Postgres this machine can reach (see the plan, Task 1 step 5). `npm test`.
Every test run creates its own schema and drops it.

## Deploy
Push to the private GitHub repo. Railway builds from the repo root. Variables: see `.env.example`.
Custom domain `api.nflstreetreborn.com`, CNAME at Cloudflare to the Railway target.

## Rotate
`LEAGUE_FORWARD_TOKEN` must equal `BACKEND_TOKEN` in `league/_inc/backend_token.php` on the site.
`STATE_SECRET` and `ADMIN_TOKEN` are free to rotate at any time. Rotating the Discord client secret
requires the same value in the Developer Portal.
```

- [ ] **Step 7: Commit**

```bash
git add package.json package-lock.json railway.json .env.example .gitignore README.md
git commit -m "Scaffold the Reborn backend"
```

---

### Task 2: Canonical hash, proven against the site's PHP

**Files:**
- Create: `src/canonical.js`, `tools/php-hash.php`, `test/fixtures/captures/*.json`, `test/fixtures/expected-hashes.json`, `test/canonical.test.js`

**Interfaces:**
- Produces: `wireHash(capture: object) -> string` (64 hex), `canonicalBytes(capture) -> string`, `phpTrim(s) -> string`, `toInt(v) -> number`.

- [ ] **Step 1: Copy the six real captures into fixtures**

```powershell
$q = "$env:APPDATA\NFL Street Reborn Launcher\stats\queue"
$dst = "test\fixtures\captures"
New-Item -ItemType Directory $dst -Force | Out-Null
Get-ChildItem "$q\*.capture.json" | ForEach-Object { Copy-Item $_.FullName (Join-Path $dst ($_.BaseName.Split('.')[0].Substring(0,12) + ".json")) }
Copy-Item "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN\User\LeagueGameStats\_previous\Tri_City_Wolfp_vs_Shadow_Knights_001.json" "$dst\tri_city_001.json"
Get-ChildItem $dst
```
Expected: six `.json` files.

- [ ] **Step 2: Write tools/php-hash.php**

```php
<?php
// Print si_capture_hash() for each capture file, using the site's OWN function.
// Usage: php tools/php-hash.php <file> [<file> ...]
declare(strict_types=1);
$site = 'C:/Users/Adam/nsr_local/public_html/league/_inc/stat_import.php';
// stat_import.php requires db.php and nsr_data.php at include time. Stub what it needs.
if (!function_exists('db')) { function db(): PDO { throw new RuntimeException('no db in hash tool'); } }
if (!function_exists('nsr_current_week')) { function nsr_current_week(PDO $p, int $l): int { return 0; } }
if (!function_exists('nsr_series')) { function nsr_series(PDO $p, int $l, int $w = 0): array { return []; } }
require_once $site;
$out = [];
foreach (array_slice($argv, 1) as $file) {
  $cap = json_decode((string)file_get_contents($file), true);
  if (!is_array($cap)) { fwrite(STDERR, "not JSON: $file\n"); exit(1); }
  $out[basename($file)] = si_capture_hash($cap);
}
echo json_encode($out, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES), "\n";
```

- [ ] **Step 3: Produce expected-hashes.json with PHP**

Run (PowerShell):
```powershell
$php = "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\PHP.PHP.8.3_Microsoft.Winget.Source_8wekyb3d8bbwe\php.exe"
& $php tools\php-hash.php (Get-ChildItem test\fixtures\captures\*.json | ForEach-Object FullName) > test\fixtures\expected-hashes.json
Get-Content test\fixtures\expected-hashes.json
```
Expected: a JSON object with six file names and six 64-hex values. If PHP complains about a missing constant or function from `stat_import.php`'s includes, stub it in `tools/php-hash.php` the same way; do not edit the site file.

- [ ] **Step 4: Write the failing test**

`test/canonical.test.js`:
```js
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { wireHash, canonicalBytes, phpTrim } from "../src/canonical.js";

const dir = new URL("./fixtures/captures/", import.meta.url);
const expected = JSON.parse(readFileSync(new URL("./fixtures/expected-hashes.json", import.meta.url), "utf8"));

test("every real capture hashes exactly as the site's PHP does", () => {
  const files = readdirSync(dir).filter((f) => f.endsWith(".json"));
  assert.equal(files.length, 6);
  for (const f of files) {
    const cap = JSON.parse(readFileSync(new URL(f, dir), "utf8"));
    assert.equal(wireHash(cap), expected[f], f);
  }
});

test("an absent team_totals block canonicalizes to an empty list, as PHP does", () => {
  const cap = { teams: [{ name: "A", score: 1 }, { name: "B", score: 0 }], style: [1, 0], players: [] };
  assert.equal(canonicalBytes(cap), '{"teams":[{"name":"A","score":1},{"name":"B","score":0}],"style":[1,0],"team_totals":[],"players":[]}');
});

test("an empty stats map becomes [] because PHP encodes an empty array that way", () => {
  const cap = { teams: [], style: [], players: [{ digp: 3, name: " x ", side: 1, stats: {} }] };
  assert.equal(canonicalBytes(cap), '{"teams":[],"style":[],"team_totals":[],"players":[{"digp":3,"name":"x","side":1,"stats":[]}]}');
});

test("phpTrim strips only what PHP trim() strips", () => {
  assert.equal(phpTrim(" \t\n\r\0\x0B a \u00a0"), "a \u00a0");
});
```

- [ ] **Step 5: Run it to verify it fails**

Run: `npm test -- test/canonical.test.js`
Expected: FAIL, `Cannot find module '../src/canonical.js'`.

- [ ] **Step 6: Write src/canonical.js**

```js
// The canonical capture hash: sha256 over ONLY the stat-bearing fields.
//
// This is si_capture_hash() from league/_inc/stat_import.php, reproduced byte
// for byte. Both NetPlay clients of one game must hash identically, so every
// per-machine field (captured_at, source, schema, reborn_match) is excluded.
// PHP's json_encode with JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE and
// JSON.stringify agree on escaping for everything a team or player name can
// hold, with one known exception: PHP escapes U+2028 and U+2029 and JS does
// not. The six real captures prove the rest; see test/canonical.test.js.
import { createHash } from "node:crypto";

/** PHP trim(): " \t\n\r\0\x0B" only. JS trim() strips more, so it is not used. */
export function phpTrim(s) {
  return String(s ?? "").replace(/^[ \t\n\r\0\x0B]+|[ \t\n\r\0\x0B]+$/g, "");
}

/** PHP (int) cast for the values a capture carries. */
export function toInt(v) {
  const n = Number(v);
  return Number.isFinite(n) ? Math.trunc(n) : 0;
}

/** A PHP assoc array after ksort(): keys in byte order; an EMPTY one encodes as []. */
function sortedIntMap(obj) {
  const keys = Object.keys(obj ?? {}).sort();
  if (keys.length === 0) return "[]";
  return "{" + keys.map((k) => JSON.stringify(k) + ":" + toInt(obj[k])).join(",") + "}";
}

export function canonicalBytes(cap) {
  const players = (Array.isArray(cap.players) ? cap.players : [])
    .map((p) => ({ digp: toInt(p?.digp), name: phpTrim(p?.name), side: toInt(p?.side), stats: p?.stats ?? {} }))
    .sort((a, b) => a.digp - b.digp);
  const teams = (Array.isArray(cap.teams) ? cap.teams : []).map((t) => `{"name":${JSON.stringify(phpTrim(t?.name))},"score":${toInt(t?.score)}}`);
  const style = (Array.isArray(cap.style) ? cap.style : []).map(toInt);
  const totals = (Array.isArray(cap.team_totals) ? cap.team_totals : []).map((row) => sortedIntMap(row));
  const playersJson = players.map((p) => `{"digp":${p.digp},"name":${JSON.stringify(p.name)},"side":${p.side},"stats":${sortedIntMap(p.stats)}}`);
  return `{"teams":[${teams.join(",")}],"style":[${style.join(",")}],"team_totals":[${totals.join(",")}],"players":[${playersJson.join(",")}]}`;
}

export function wireHash(cap) {
  return createHash("sha256").update(canonicalBytes(cap), "utf8").digest("hex");
}
```

Note on `usort` stability: PHP 8's sort is stable, and `Array.prototype.sort` is stable, so equal `digp` values keep input order in both.

- [ ] **Step 7: Run the tests**

Run: `npm test -- test/canonical.test.js`
Expected: 4 passing. If the six-file test fails on one file, diff `canonicalBytes` output against `php -r` output for that file before changing anything: `& $php -r "require 'tools/php-hash.php';"` is not enough; add a temporary `echo json_encode($canon)` in a copy of the function inside `tools/php-hash.php` (never in the site file).

- [ ] **Step 8: Commit**

```bash
git add src/canonical.js tools/php-hash.php test/canonical.test.js test/fixtures
git commit -m "Canonical capture hash, proven against the site's PHP on six real captures"
```

---

### Task 3: Database pool, migrations and the test harness

**Files:**
- Create: `migrations/001_init.sql`, `src/db.js`, `test/helpers/db.js`, `test/helpers/fakes.js`, `test/db.test.js`

**Interfaces:**
- Produces: `createPool(url, { schema? }) -> pg.Pool` (search_path set per connection), `migrate(pool) -> Promise<string[]>` (applied file names), `withTx(pool, fn) -> Promise<T>`, `testPool() -> Promise<{ pool, schema, close }>`, `fixedClock(startMs)` with `now()` and `advance(ms)`, `fakeDiscord()`, `fakeFetch()`.

- [ ] **Step 1: Write migrations/001_init.sql**

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
  name text PRIMARY KEY,
  applied_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE player (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  discord_user_id text UNIQUE,
  display_name text NOT NULL DEFAULT '',
  avatar_hash text,
  show_offline boolean NOT NULL DEFAULT false,
  created_at timestamptz NOT NULL DEFAULT now(),
  last_seen_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz
);

CREATE TABLE device_token (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  player_id uuid NOT NULL REFERENCES player(id),
  token_hash char(64) NOT NULL UNIQUE,
  label text NOT NULL DEFAULT '',
  created_at timestamptz NOT NULL DEFAULT now(),
  last_used_at timestamptz,
  revoked_at timestamptz
);

CREATE TABLE device_login (
  device_code char(43) PRIMARY KEY,
  user_code char(9) NOT NULL,
  label text NOT NULL DEFAULT '',
  player_id uuid REFERENCES player(id),
  status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','claimed','expired')),
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL
);
CREATE INDEX device_login_user_code ON device_login (user_code) WHERE status = 'pending';

CREATE TABLE game (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  capture_hash char(64) NOT NULL UNIQUE,
  mode text NOT NULL CHECK (mode IN ('online','offline')),
  teams jsonb NOT NULL,
  style jsonb NOT NULL,
  team_totals jsonb,
  players jsonb NOT NULL,
  target_reached boolean NOT NULL,
  status text NOT NULL CHECK (status IN ('accepted','incomplete','held','void')),
  hold_reason text,
  flags text[] NOT NULL DEFAULT '{}',
  captured_at timestamptz,
  first_seen_at timestamptz NOT NULL DEFAULT now(),
  resolved_at timestamptz
);

CREATE TABLE submission (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  game_id uuid NOT NULL REFERENCES game(id),
  player_id uuid NOT NULL REFERENCES player(id),
  envelope jsonb NOT NULL,
  capture_folder text NOT NULL,
  session jsonb NOT NULL,
  intent_league boolean NOT NULL DEFAULT false,
  received_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (game_id, player_id)
);

CREATE TABLE participation (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  game_id uuid NOT NULL REFERENCES game(id),
  player_id uuid REFERENCES player(id),
  kind text NOT NULL CHECK (kind IN ('human','guest','cpu')),
  pad smallint,
  registry_team smallint,
  team_index smallint,
  source text NOT NULL DEFAULT 'submission' CHECK (source IN ('submission','backfill','admin'))
);
CREATE UNIQUE INDEX participation_one_per_player ON participation (game_id, player_id) WHERE player_id IS NOT NULL;
CREATE INDEX participation_by_player ON participation (player_id);

CREATE TABLE league_forward (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  game_id uuid NOT NULL UNIQUE REFERENCES game(id),
  status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','applied','review','duplicate','failed')),
  site_import_id int,
  reason text,
  attempts int NOT NULL DEFAULT 0,
  last_attempt_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);
```

- [ ] **Step 2: Write src/db.js**

```js
// The one place a database connection is made.
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import pg from "pg";

const here = dirname(fileURLToPath(import.meta.url));
const MIGRATIONS = join(here, "..", "migrations");

/** A pool. `schema`, when given, is created if absent and used as search_path on every connection. */
export function createPool(url, { schema } = {}) {
  const pool = new pg.Pool({ connectionString: url, max: 5 });
  if (schema) {
    if (!/^[a-z_][a-z0-9_]*$/.test(schema)) throw new Error(`bad schema name ${schema}`);
    pool.on("connect", (client) => {
      client.query(`SET search_path TO ${schema}, public`).catch(() => {});
    });
  }
  return pool;
}

/** Apply every migrations/*.sql not yet recorded. Returns the names applied. */
export async function migrate(pool, { schema } = {}) {
  const client = await pool.connect();
  try {
    if (schema) {
      await client.query(`CREATE SCHEMA IF NOT EXISTS ${schema}`);
      await client.query(`SET search_path TO ${schema}, public`);
    }
    await client.query(`CREATE TABLE IF NOT EXISTS schema_migrations (name text PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now())`);
    const done = new Set((await client.query(`SELECT name FROM schema_migrations`)).rows.map((r) => r.name));
    const applied = [];
    for (const name of readdirSync(MIGRATIONS).filter((f) => f.endsWith(".sql")).sort()) {
      if (done.has(name)) continue;
      await client.query("BEGIN");
      try {
        await client.query(readFileSync(join(MIGRATIONS, name), "utf8"));
        await client.query(`INSERT INTO schema_migrations (name) VALUES ($1)`, [name]);
        await client.query("COMMIT");
      } catch (e) {
        await client.query("ROLLBACK");
        throw new Error(`migration ${name} failed: ${e.message}`);
      }
      applied.push(name);
    }
    return applied;
  } finally {
    client.release();
  }
}

/** Run fn(client) inside one transaction. */
export async function withTx(pool, fn) {
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const out = await fn(client);
    await client.query("COMMIT");
    return out;
  } catch (e) {
    await client.query("ROLLBACK").catch(() => {});
    throw e;
  } finally {
    client.release();
  }
}

// `npm run migrate`: apply against DATABASE_URL and exit.
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const url = process.env.DATABASE_URL;
  if (!url) { console.error("DATABASE_URL is not set"); process.exit(2); }
  const pool = createPool(url);
  migrate(pool).then((a) => { console.log(a.length ? `applied ${a.join(", ")}` : "nothing to apply"); return pool.end(); });
}
```

- [ ] **Step 3: Write test/helpers/db.js and test/helpers/fakes.js**

`test/helpers/db.js`:
```js
import { createPool, migrate } from "../../src/db.js";

/** A fresh schema in the test database, migrated, dropped on close. */
export async function testPool() {
  const url = process.env.DATABASE_URL_TEST;
  if (!url) throw new Error("DATABASE_URL_TEST is not set; see README.md");
  const schema = `t_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
  const pool = createPool(url, { schema });
  await migrate(pool, { schema });
  return {
    pool,
    schema,
    async close() {
      await pool.query(`DROP SCHEMA ${schema} CASCADE`);
      await pool.end();
    },
  };
}
```

`test/helpers/fakes.js`:
```js
export function fixedClock(startMs = 1_800_000_000_000) {
  let t = startMs;
  return { now: () => new Date(t), advance: (ms) => { t += ms; } };
}

/** A Discord that accepts one code and knows one user. */
export function fakeDiscord({ code = "good-code", user = { id: "111222333", username: "adam", global_name: "Adam", avatar: "abc" } } = {}) {
  return {
    authorizeUrl: (state) => `https://discord.test/authorize?state=${encodeURIComponent(state)}`,
    async exchangeCode(c) { if (c !== code) throw new Error("invalid_grant"); return { access_token: "at" }; },
    async fetchUser(token) { if (token !== "at") throw new Error("bad token"); return user; },
  };
}

/** fetch() replaced by canned replies per URL; records every call. */
export function fakeFetch(replies = {}) {
  const calls = [];
  const f = async (url, init = {}) => {
    calls.push({ url, init });
    const r = replies[url];
    if (!r) throw new Error(`ECONNREFUSED ${url}`);
    const { status = 200, body = "" } = typeof r === "function" ? r(init) : r;
    return { status, ok: status >= 200 && status < 300, text: async () => body, json: async () => JSON.parse(body) };
  };
  f.calls = calls;
  return f;
}
```

- [ ] **Step 4: Write the failing test**

`test/db.test.js`:
```js
import test from "node:test";
import assert from "node:assert/strict";
import { testPool } from "./helpers/db.js";

test("migrations apply into a fresh schema and are recorded once", async () => {
  const t = await testPool();
  try {
    const rows = (await t.pool.query(`SELECT name FROM schema_migrations ORDER BY name`)).rows.map((r) => r.name);
    assert.deepEqual(rows, ["001_init.sql"]);
    const tables = (await t.pool.query(`SELECT table_name FROM information_schema.tables WHERE table_schema = $1 ORDER BY table_name`, [t.schema])).rows.map((r) => r.table_name);
    assert.deepEqual(tables, ["device_login", "device_token", "game", "league_forward", "participation", "player", "schema_migrations", "submission"]);
  } finally {
    await t.close();
  }
});
```

- [ ] **Step 5: Run, expect failure, then run again after the files exist**

Run: `npm test -- test/db.test.js`
Expected first: FAIL (module missing) if Step 2 and 3 were not saved yet; after saving: PASS with the eight table names.

- [ ] **Step 6: Commit**

```bash
git add migrations src/db.js test/helpers test/db.test.js
git commit -m "Postgres pool, migrations, and a per-run test schema"
```

---

### Task 4: Tokens, Discord client, and device sign-in

**Files:**
- Create: `src/tokens.js`, `src/discord.js`, `src/auth.js`, `test/auth.test.js`

**Interfaces:**
- Produces:
  - `newToken() -> string` (43-char base64url), `hashToken(t) -> string` (64 hex), `newDeviceCode() -> string` (43 chars), `newUserCode() -> string` (`XXXX-XXXX`).
  - `realDiscord({ clientId, clientSecret, redirectUri }) -> { authorizeUrl(state), exchangeCode(code), fetchUser(accessToken) }`.
  - `deviceStart(pool, clock, { label, publicOrigin }) -> { device_code, user_code, verify_url, expires_in, interval }`
  - `devicePoll(pool, clock, deviceCode) -> { kind: "pending" | "expired" | "done", token?, player? }`
  - `signState(deviceCode, secret) -> string`, `verifyState(state, secret) -> deviceCode | null`
  - `completeCallback(pool, clock, discord, { code, state, stateSecret }) -> { player }`
  - `bearerPlayer(pool, clock, authorizationHeader) -> { player, tokenId } | null`
  - `authRouter(deps) -> express.Router` mounting `POST /v1/auth/device`, `GET /link`, `POST /link/start`, `GET /v1/auth/discord/callback`, `POST /v1/auth/device/poll`, `GET /v1/me`, `PATCH /v1/me`, `DELETE /v1/auth/device`.

- [ ] **Step 1: Write src/tokens.js**

```js
import { createHash, randomBytes } from "node:crypto";

export function newToken() { return randomBytes(32).toString("base64url"); }        // 43 chars
export function hashToken(t) { return createHash("sha256").update(t, "utf8").digest("hex"); }
export function newDeviceCode() { return randomBytes(32).toString("base64url"); }   // 43 chars

// No vowels, no 0/O/1/I: a code a person reads aloud without ambiguity.
const ALPHABET = "BCDFGHJKLMNPQRSTVWXZ23456789";
export function newUserCode() {
  const b = randomBytes(8);
  let s = "";
  for (let i = 0; i < 8; i++) s += ALPHABET[b[i] % ALPHABET.length];
  return `${s.slice(0, 4)}-${s.slice(4)}`;
}
```

- [ ] **Step 2: Write src/discord.js**

```js
// Discord OAuth, `identify` scope only. The access token never leaves this process.
export function realDiscord({ clientId, clientSecret, redirectUri, fetchImpl = fetch }) {
  return {
    authorizeUrl(state) {
      const p = new URLSearchParams({ client_id: clientId, redirect_uri: redirectUri, response_type: "code", scope: "identify", state, prompt: "none" });
      return `https://discord.com/oauth2/authorize?${p}`;
    },
    async exchangeCode(code) {
      const res = await fetchImpl("https://discord.com/api/oauth2/token", {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: new URLSearchParams({ client_id: clientId, client_secret: clientSecret, grant_type: "authorization_code", code, redirect_uri: redirectUri }),
      });
      const data = await res.json();
      if (!res.ok || data.error) throw new Error(`discord token exchange failed: ${data.error ?? res.status}`);
      return data;
    },
    async fetchUser(accessToken) {
      const res = await fetchImpl("https://discord.com/api/users/@me", { headers: { Authorization: `Bearer ${accessToken}` } });
      if (!res.ok) throw new Error(`discord /users/@me answered ${res.status}`);
      return res.json();
    },
  };
}
```

- [ ] **Step 3: Write the failing tests**

`test/auth.test.js`:
```js
import test from "node:test";
import assert from "node:assert/strict";
import { testPool } from "./helpers/db.js";
import { fakeDiscord, fixedClock } from "./helpers/fakes.js";
import { deviceStart, devicePoll, completeCallback, signState, verifyState, bearerPlayer } from "../src/auth.js";
import { hashToken } from "../src/tokens.js";

const SECRET = "test-state-secret";

test("device sign-in: start, browser completes Discord, poll returns the token once", async () => {
  const t = await testPool();
  const clock = fixedClock();
  try {
    const start = await deviceStart(t.pool, clock, { label: "ADAM-PC", publicOrigin: "https://api.test" });
    assert.match(start.device_code, /^[A-Za-z0-9_-]{43}$/);
    assert.match(start.user_code, /^[BCDFGHJKLMNPQRSTVWXZ2-9]{4}-[BCDFGHJKLMNPQRSTVWXZ2-9]{4}$/);
    assert.equal(start.verify_url, `https://api.test/link?code=${start.user_code}`);
    assert.equal(start.expires_in, 600);
    assert.equal(start.interval, 3);

    assert.deepEqual(await devicePoll(t.pool, clock, start.device_code), { kind: "pending" });

    const state = signState(start.device_code, SECRET);
    assert.equal(verifyState(state, SECRET), start.device_code);
    assert.equal(verifyState(state + "x", SECRET), null);

    const { player } = await completeCallback(t.pool, clock, fakeDiscord(), { code: "good-code", state, stateSecret: SECRET });
    assert.equal(player.discord_user_id, "111222333");
    assert.equal(player.display_name, "Adam");

    const done = await devicePoll(t.pool, clock, start.device_code);
    assert.equal(done.kind, "done");
    assert.match(done.token, /^[A-Za-z0-9_-]{43}$/);
    assert.equal(done.player.display_name, "Adam");

    const again = await devicePoll(t.pool, clock, start.device_code);
    assert.equal(again.kind, "expired");

    const stored = (await t.pool.query(`SELECT token_hash, label FROM device_token`)).rows;
    assert.equal(stored.length, 1);
    assert.equal(stored[0].token_hash, hashToken(done.token));
    assert.equal(stored[0].label, "ADAM-PC");

    const who = await bearerPlayer(t.pool, clock, `Bearer ${done.token}`);
    assert.equal(who.player.display_name, "Adam");
    assert.equal(await bearerPlayer(t.pool, clock, `Bearer ${done.token}x`), null);
    assert.equal(await bearerPlayer(t.pool, clock, undefined), null);
  } finally {
    await t.close();
  }
});

test("a device code expires after 600 seconds", async () => {
  const t = await testPool();
  const clock = fixedClock();
  try {
    const start = await deviceStart(t.pool, clock, { label: "x", publicOrigin: "https://api.test" });
    clock.advance(601_000);
    assert.deepEqual(await devicePoll(t.pool, clock, start.device_code), { kind: "expired" });
  } finally {
    await t.close();
  }
});

test("a returning Discord user reuses the same player row", async () => {
  const t = await testPool();
  const clock = fixedClock();
  try {
    for (let i = 0; i < 2; i++) {
      const start = await deviceStart(t.pool, clock, { label: `pc${i}`, publicOrigin: "https://api.test" });
      await completeCallback(t.pool, clock, fakeDiscord(), { code: "good-code", state: signState(start.device_code, SECRET), stateSecret: SECRET });
      await devicePoll(t.pool, clock, start.device_code);
    }
    assert.equal((await t.pool.query(`SELECT count(*)::int AS n FROM player`)).rows[0].n, 1);
    assert.equal((await t.pool.query(`SELECT count(*)::int AS n FROM device_token`)).rows[0].n, 2);
  } finally {
    await t.close();
  }
});
```

- [ ] **Step 4: Run to verify failure**

Run: `npm test -- test/auth.test.js`
Expected: FAIL, `Cannot find module '../src/auth.js'`.

- [ ] **Step 5: Write src/auth.js**

```js
// Device-code sign-in. Nothing listens on the player's machine, and the Discord
// access token never leaves this process.
import { createHmac, timingSafeEqual } from "node:crypto";
import express from "express";
import { hashToken, newDeviceCode, newToken, newUserCode } from "./tokens.js";

const DEVICE_TTL_MS = 600_000;
const POLL_INTERVAL_S = 3;

export function signState(deviceCode, secret) {
  const sig = createHmac("sha256", secret).update(deviceCode).digest("base64url");
  return `${deviceCode}.${sig}`;
}

export function verifyState(state, secret) {
  const i = String(state ?? "").lastIndexOf(".");
  if (i < 0) return null;
  const deviceCode = state.slice(0, i);
  const sig = state.slice(i + 1);
  const want = createHmac("sha256", secret).update(deviceCode).digest("base64url");
  const a = Buffer.from(sig), b = Buffer.from(want);
  if (a.length !== b.length || !timingSafeEqual(a, b)) return null;
  return deviceCode;
}

export async function deviceStart(pool, clock, { label, publicOrigin }) {
  const device_code = newDeviceCode();
  const user_code = newUserCode();
  const now = clock.now();
  await pool.query(
    `INSERT INTO device_login (device_code, user_code, label, status, created_at, expires_at) VALUES ($1, $2, $3, 'pending', $4, $5)`,
    [device_code, user_code, String(label ?? "").slice(0, 80), now, new Date(now.getTime() + DEVICE_TTL_MS)],
  );
  return { device_code, user_code, verify_url: `${publicOrigin}/link?code=${user_code}`, expires_in: DEVICE_TTL_MS / 1000, interval: POLL_INTERVAL_S };
}

/** The browser step finished: bind the device login to the Discord user. */
export async function completeCallback(pool, clock, discord, { code, state, stateSecret }) {
  const deviceCode = verifyState(state, stateSecret);
  if (!deviceCode) throw Object.assign(new Error("The sign-in link is not valid. Start again from the launcher."), { status: 400 });
  const login = (await pool.query(`SELECT device_code, status, expires_at FROM device_login WHERE device_code = $1`, [deviceCode])).rows[0];
  if (!login || login.status !== "pending" || login.expires_at < clock.now()) {
    throw Object.assign(new Error("This sign-in code has expired. Start again from the launcher."), { status: 410 });
  }
  const tok = await discord.exchangeCode(code);
  const u = await discord.fetchUser(tok.access_token);
  const display = u.global_name || u.username || "Player";
  const player = (await pool.query(
    `INSERT INTO player (discord_user_id, display_name, avatar_hash, last_seen_at)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (discord_user_id) DO UPDATE SET display_name = EXCLUDED.display_name, avatar_hash = EXCLUDED.avatar_hash, last_seen_at = EXCLUDED.last_seen_at
     RETURNING id, discord_user_id, display_name, avatar_hash, show_offline`,
    [String(u.id), display, u.avatar ?? null, clock.now()],
  )).rows[0];
  await pool.query(`UPDATE device_login SET status = 'approved', player_id = $2 WHERE device_code = $1`, [deviceCode, player.id]);
  return { player };
}

/** The launcher asks whether the browser step finished. The token is handed out exactly once. */
export async function devicePoll(pool, clock, deviceCode) {
  const row = (await pool.query(`SELECT status, expires_at, player_id, label FROM device_login WHERE device_code = $1`, [deviceCode])).rows[0];
  if (!row || row.status === "claimed" || row.status === "expired") return { kind: "expired" };
  if (row.expires_at < clock.now()) {
    await pool.query(`UPDATE device_login SET status = 'expired' WHERE device_code = $1`, [deviceCode]);
    return { kind: "expired" };
  }
  if (row.status === "pending") return { kind: "pending" };
  // approved: mint a token and claim the code atomically, so a second poller gets "expired".
  const claimed = (await pool.query(`UPDATE device_login SET status = 'claimed' WHERE device_code = $1 AND status = 'approved' RETURNING player_id`, [deviceCode])).rows[0];
  if (!claimed) return { kind: "expired" };
  const token = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at) VALUES ($1, $2, $3, $4)`, [claimed.player_id, hashToken(token), row.label, clock.now()]);
  const player = (await pool.query(`SELECT id, discord_user_id, display_name, avatar_hash, show_offline FROM player WHERE id = $1`, [claimed.player_id])).rows[0];
  return { kind: "done", token, player };
}

/** Who a bearer header names. Null for anything but a live token. */
export async function bearerPlayer(pool, clock, header) {
  const m = /^Bearer\s+([A-Za-z0-9_-]{20,})$/.exec(String(header ?? ""));
  if (!m) return null;
  const row = (await pool.query(
    `SELECT t.id AS token_id, p.id, p.discord_user_id, p.display_name, p.avatar_hash, p.show_offline, p.deleted_at
       FROM device_token t JOIN player p ON p.id = t.player_id
      WHERE t.token_hash = $1 AND t.revoked_at IS NULL`,
    [hashToken(m[1])],
  )).rows[0];
  if (!row || row.deleted_at) return null;
  await pool.query(`UPDATE device_token SET last_used_at = $2 WHERE id = $1`, [row.token_id, clock.now()]);
  const { token_id, deleted_at, ...player } = row;
  return { player, tokenId: token_id };
}

/** Express middleware: sets req.player and req.tokenId or answers 401. */
export function requirePlayer(pool, clock) {
  return async (req, res, next) => {
    try {
      const who = await bearerPlayer(pool, clock, req.headers.authorization);
      if (!who) return res.status(401).json({ ok: false, error: "Signed out. Sign in from the launcher." });
      req.player = who.player;
      req.tokenId = who.tokenId;
      next();
    } catch (e) { next(e); }
  };
}

const page = (title, body) => `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>${title}</title>
<style>body{font:16px system-ui;background:#111;color:#eee;display:grid;place-items:center;min-height:100vh;margin:0}main{max-width:28rem;padding:2rem;text-align:center}code{font-size:1.6rem;letter-spacing:.15em}button{font:inherit;padding:.7rem 1.4rem;border-radius:.5rem;border:0;background:#5865F2;color:#fff;cursor:pointer}</style>
<main>${body}</main>`;

export function authRouter({ pool, clock, discord, stateSecret, publicOrigin }) {
  const r = express.Router();
  r.use(express.json({ limit: "16kb" }));
  r.use(express.urlencoded({ extended: false }));

  r.post("/v1/auth/device", async (req, res, next) => {
    try { res.json(await deviceStart(pool, clock, { label: req.body?.label ?? "", publicOrigin })); } catch (e) { next(e); }
  });

  r.get("/link", (req, res) => {
    const code = String(req.query.code ?? "").toUpperCase().replace(/[^A-Z0-9-]/g, "").slice(0, 9);
    res.type("html").send(page("Sign in to NFL Street Reborn", `
      <h1>NFL Street Reborn</h1>
      <p>Confirm the code shown in your launcher, then continue with Discord.</p>
      <form method="post" action="/link/start">
        <p><input name="code" value="${code}" maxlength="9" required style="font:inherit;font-size:1.4rem;letter-spacing:.15em;text-align:center;padding:.5rem;border-radius:.5rem;border:1px solid #444;background:#222;color:#eee"></p>
        <p><button type="submit">Continue with Discord</button></p>
      </form>`));
  });

  r.post("/link/start", async (req, res, next) => {
    try {
      const code = String(req.body?.code ?? "").toUpperCase().trim();
      const row = (await pool.query(`SELECT device_code FROM device_login WHERE user_code = $1 AND status = 'pending' AND expires_at > $2 ORDER BY created_at DESC LIMIT 1`, [code, clock.now()])).rows[0];
      if (!row) return res.status(410).type("html").send(page("Code expired", `<h1>That code is not waiting for a sign-in.</h1><p>Start again from the launcher.</p>`));
      res.redirect(discord.authorizeUrl(signState(row.device_code, stateSecret)));
    } catch (e) { next(e); }
  });

  r.get("/v1/auth/discord/callback", async (req, res, next) => {
    try {
      if (req.query.error) return res.status(400).type("html").send(page("Sign-in canceled", `<h1>Discord did not sign you in.</h1><p>${String(req.query.error_description ?? req.query.error)}</p>`));
      const { player } = await completeCallback(pool, clock, discord, { code: String(req.query.code ?? ""), state: String(req.query.state ?? ""), stateSecret });
      res.type("html").send(page("Signed in", `<h1>Signed in as ${escapeHtml(player.display_name)}</h1><p>You can close this tab and go back to the launcher.</p>`));
    } catch (e) {
      if (e.status) return res.status(e.status).type("html").send(page("Sign-in problem", `<h1>${escapeHtml(e.message)}</h1>`));
      next(e);
    }
  });

  r.post("/v1/auth/device/poll", async (req, res, next) => {
    try {
      const out = await devicePoll(pool, clock, String(req.body?.device_code ?? ""));
      if (out.kind === "pending") return res.status(428).json({ ok: false, status: "pending" });
      if (out.kind === "expired") return res.status(410).json({ ok: false, status: "expired" });
      res.json({ ok: true, token: out.token, player: out.player });
    } catch (e) { next(e); }
  });

  r.get("/v1/me", requirePlayer(pool, clock), (req, res) => res.json({ ok: true, player: req.player }));

  // PATCH is the natural verb; POST /v1/me/show_offline is the same handler for the launcher,
  // whose HTTP seam has GET, POST and DELETE only.
  const setShowOffline = async (req, res, next) => {
    try {
      if (typeof req.body?.show_offline !== "boolean") return res.status(400).json({ ok: false, error: "show_offline must be true or false" });
      const p = (await pool.query(`UPDATE player SET show_offline = $2 WHERE id = $1 RETURNING id, discord_user_id, display_name, avatar_hash, show_offline`, [req.player.id, req.body.show_offline])).rows[0];
      res.json({ ok: true, player: p });
    } catch (e) { next(e); }
  };
  r.patch("/v1/me", requirePlayer(pool, clock), setShowOffline);
  r.post("/v1/me/show_offline", requirePlayer(pool, clock), setShowOffline);

  r.delete("/v1/auth/device", requirePlayer(pool, clock), async (req, res, next) => {
    try {
      await pool.query(`UPDATE device_token SET revoked_at = $2 WHERE id = $1`, [req.tokenId, clock.now()]);
      res.json({ ok: true });
    } catch (e) { next(e); }
  });

  return r;
}

function escapeHtml(s) { return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])); }
```

- [ ] **Step 6: Run the tests**

Run: `npm test -- test/auth.test.js`
Expected: 3 passing.

- [ ] **Step 7: Commit**

```bash
git add src/tokens.js src/discord.js src/auth.js test/auth.test.js
git commit -m "Device-code Discord sign-in with revocable per-device tokens"
```

---

### Task 5: Results intake: one game, its participants, its status

**Files:**
- Create: `src/results.js`, `test/results.test.js`

**Interfaces:**
- Consumes: `wireHash` (Task 2), `withTx` (Task 3), `requirePlayer` (Task 4).
- Produces:
  - `TARGET_SCORE = 36`
  - `classifyMode(folder, rebornMatch) -> "online" | "offline" | null`
  - `processSubmission(pool, clock, player, envelope) -> { status: "accepted"|"duplicate"|"held"|"incomplete", game_id, reason? }` (throws `{status: 400, message}` on a bad envelope)
  - `resultsRouter({ pool, clock }) -> express.Router` mounting `POST /v1/results`

- [ ] **Step 1: Write the failing tests**

`test/results.test.js`:
```js
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { processSubmission, classifyMode } from "../src/results.js";
import { wireHash } from "../src/canonical.js";

const fixture = () => JSON.parse(readFileSync(new URL("./fixtures/captures/tri_city_001.json", import.meta.url), "utf8"));

async function player(pool, discordId, name) {
  return (await pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [discordId, name])).rows[0];
}

function envelope(cap, { folder = "offline", netplay = false, pads = [{ pad: 0, registry_team: 1 }], league = false, teamIndex } = {}) {
  cap = structuredClone(cap);
  cap.reborn_match = { schema: "nsr.reborn_match.v1", netplay_running: netplay, game_id: "GN7E69", runtime_version: "1.1.0", local_pads: pads };
  if (teamIndex !== undefined) cap.reborn_match.local_team_index = teamIndex;
  return {
    schema: "nsr.submission.v1",
    capture: cap,
    capture_hash: wireHash(cap),
    capture_folder: folder,
    session: { session_id: "1-offline", launcher_version: "3.1.0", runtime_version: "1.1.0", exit: "clean", exit_code: 0 },
    intent: { league },
    submitted_at: "2026-09-08T21:00:00Z",
  };
}

/** A capture that finished normally: 36 to 24. */
function complete(cap) {
  cap = structuredClone(cap);
  cap.teams[0].score = 36; cap.teams[1].score = 24;
  if (cap.team_totals) { cap.team_totals[0].score = 36; cap.team_totals[1].score = 24; }
  return cap;
}

test("classifyMode needs the folder and the netplay flag to agree", () => {
  assert.equal(classifyMode("online", { netplay_running: true }), "online");
  assert.equal(classifyMode("offline", { netplay_running: false }), "offline");
  assert.equal(classifyMode("online", { netplay_running: false }), null);
  assert.equal(classifyMode("offline", undefined), "offline");
  assert.equal(classifyMode("online", undefined), "online");
});

test("a completed offline game is accepted once and duplicate after", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const p = await player(t.pool, "1", "A");
    const env = envelope(complete(fixture()), { teamIndex: 1 });
    const first = await processSubmission(t.pool, clock, p, env);
    assert.equal(first.status, "accepted");
    const second = await processSubmission(t.pool, clock, p, env);
    assert.equal(second.status, "duplicate");
    assert.equal(second.game_id, first.game_id);
    const parts = (await t.pool.query(`SELECT kind, team_index, player_id FROM participation WHERE game_id = $1 ORDER BY kind`, [first.game_id])).rows;
    assert.deepEqual(parts.map((x) => x.kind), ["cpu", "human"]);
    assert.equal(parts[1].team_index, 1);
    assert.equal(parts[0].team_index, 0);
  } finally { await t.close(); }
});

test("a short game records as incomplete, not held", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const p = await player(t.pool, "1", "A");
    const r = await processSubmission(t.pool, clock, p, envelope(fixture()));   // 0 to 6
    assert.equal(r.status, "incomplete");
    assert.equal(r.reason, "below-target-score");
  } finally { await t.close(); }
});

test("hold codes", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const p = await player(t.pool, "1", "A");
    const base = complete(fixture());
    let r = await processSubmission(t.pool, clock, p, envelope(base, { folder: "online", netplay: false }));
    assert.deepEqual([r.status, r.reason], ["held", "online-signals-disagree"]);
    const noPlayers = structuredClone(base); noPlayers.players = [];
    r = await processSubmission(t.pool, clock, p, envelope(noPlayers));
    assert.deepEqual([r.status, r.reason], ["held", "no-players"]);
    const tied = structuredClone(base); tied.teams[1].score = 36;
    r = await processSubmission(t.pool, clock, p, envelope(tied));
    assert.deepEqual([r.status, r.reason], ["held", "tied-final-score"]);
    // A distinct capture: the first call already stored `base` for this player, and the same hash
    // from the same player is correctly a duplicate, never a second status decision.
    const noSeat = structuredClone(base); noSeat.style = [11, 12];
    r = await processSubmission(t.pool, clock, p, envelope(noSeat, { pads: [] }));
    assert.deepEqual([r.status, r.reason], ["held", "no-seat-claim"]);
  } finally { await t.close(); }
});

test("two clients of one online game meet on one row; matching seats hold it", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const a = await player(t.pool, "1", "A"), b = await player(t.pool, "2", "B");
    const cap = complete(fixture());
    const ra = await processSubmission(t.pool, clock, a, envelope(cap, { folder: "online", netplay: true, pads: [{ pad: 0, registry_team: 0 }] }));
    assert.equal(ra.status, "accepted");
    const rb = await processSubmission(t.pool, clock, b, envelope(cap, { folder: "online", netplay: true, pads: [{ pad: 1, registry_team: 1 }] }));
    assert.equal(rb.status, "accepted");
    assert.equal(rb.game_id, ra.game_id);
    assert.equal((await t.pool.query(`SELECT count(*)::int AS n FROM participation WHERE game_id = $1 AND kind = 'human'`, [ra.game_id])).rows[0].n, 2);

    const c = await player(t.pool, "3", "C"), d = await player(t.pool, "4", "D");
    const cap2 = complete(fixture()); cap2.style = [999, 1];
    await processSubmission(t.pool, clock, c, envelope(cap2, { folder: "online", netplay: true, pads: [{ pad: 0, registry_team: 0 }] }));
    const rd = await processSubmission(t.pool, clock, d, envelope(cap2, { folder: "online", netplay: true, pads: [{ pad: 0, registry_team: 0 }] }));
    assert.deepEqual([rd.status, rd.reason], ["held", "seat-conflict"]);
  } finally { await t.close(); }
});

test("a wrong hash is refused before anything is written", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const p = await player(t.pool, "1", "A");
    const env = envelope(fixture()); env.capture_hash = "00".repeat(32);
    await assert.rejects(processSubmission(t.pool, clock, p, env), (e) => e.status === 400 && /hash-mismatch/.test(e.message));
    assert.equal((await t.pool.query(`SELECT count(*)::int AS n FROM game`)).rows[0].n, 0);
  } finally { await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm test -- test/results.test.js`
Expected: FAIL, `Cannot find module '../src/results.js'`.

- [ ] **Step 3: Write src/results.js**

```js
// One submission in, one game row out, with its participants and status.
import express from "express";
import { wireHash } from "./canonical.js";
import { withTx } from "./db.js";
import { requirePlayer } from "./auth.js";

export const TARGET_SCORE = 36;
export const MAX_BODY_BYTES = 512 * 1024;

const bad = (msg) => Object.assign(new Error(msg), { status: 400 });

export function classifyMode(folder, rebornMatch) {
  const f = folder === "online" ? "online" : folder === "offline" ? "offline" : null;
  if (!f) return null;
  if (!rebornMatch || typeof rebornMatch.netplay_running !== "boolean") return f;   // an older Dolphin: the folder is the only signal
  const n = rebornMatch.netplay_running ? "online" : "offline";
  return n === f ? f : null;
}

function validateEnvelope(env) {
  if (!env || env.schema !== "nsr.submission.v1") throw bad("wrong-schema: expected nsr.submission.v1");
  const cap = env.capture;
  if (!cap || typeof cap !== "object" || cap.schema !== "nsr.game.v1") throw bad("wrong-schema: capture is not nsr.game.v1");
  if (!Array.isArray(cap.teams) || cap.teams.length !== 2) throw bad("wrong-team-count");
  if (typeof env.capture_hash !== "string" || wireHash(cap) !== env.capture_hash) throw bad("hash-mismatch: the envelope's capture_hash does not recompute");
  if (!["online", "offline"].includes(env.capture_folder)) throw bad("capture_folder must be online or offline");
  return cap;
}

/** Status for a game from its own facts plus the participations known so far. */
export function decideStatus({ cap, mode, pads, seatClaims }) {
  const s0 = Number(cap.teams[0]?.score ?? 0), s1 = Number(cap.teams[1]?.score ?? 0);
  if (!Array.isArray(cap.players) || cap.players.length === 0) return { status: "held", reason: "no-players" };
  if (mode === null) return { status: "held", reason: "online-signals-disagree" };
  if (s0 === s1) return { status: "held", reason: "tied-final-score" };
  if (!pads.length) return { status: "held", reason: "no-seat-claim" };
  if (mode === "online" && seatClaims.length >= 2) {
    const teams = new Set(seatClaims.map((c) => c.registry_team));
    if (teams.size < 2 || seatClaims.some((c) => c.registry_team !== 0 && c.registry_team !== 1)) return { status: "held", reason: "seat-conflict" };
  }
  if (Math.max(s0, s1) < TARGET_SCORE) return { status: "incomplete", reason: "below-target-score" };
  return { status: "accepted", reason: null };
}

export async function processSubmission(pool, clock, player, env) {
  const cap = validateEnvelope(env);
  const rm = cap.reborn_match && typeof cap.reborn_match === "object" ? cap.reborn_match : null;
  const pads = Array.isArray(rm?.local_pads) ? rm.local_pads.filter((p) => Number.isInteger(p?.pad)).sort((a, b) => a.pad - b.pad) : [];
  const mode = classifyMode(env.capture_folder, rm);
  const flags = Array.isArray(env.flags) ? env.flags.filter((f) => typeof f === "string").slice(0, 64) : [];
  const targetReached = Math.max(Number(cap.teams[0]?.score ?? 0), Number(cap.teams[1]?.score ?? 0)) >= TARGET_SCORE;
  const now = clock.now();

  return withTx(pool, async (c) => {
    // The game row, created once per hash. `mode` for an ambiguous game is recorded from the folder.
    const inserted = (await c.query(
      `INSERT INTO game (capture_hash, mode, teams, style, team_totals, players, target_reached, status, hold_reason, flags, captured_at, first_seen_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7, 'held', 'pending', $8, $9, $10)
       ON CONFLICT (capture_hash) DO NOTHING RETURNING id`,
      [env.capture_hash, mode ?? env.capture_folder, JSON.stringify(cap.teams), JSON.stringify(cap.style ?? []), cap.team_totals ? JSON.stringify(cap.team_totals) : null,
       JSON.stringify(cap.players ?? []), targetReached, flags, cap.captured_at ? new Date(cap.captured_at) : null, now],
    )).rows[0];
    const game = inserted ?? (await c.query(`SELECT id FROM game WHERE capture_hash = $1`, [env.capture_hash])).rows[0];

    const sub = (await c.query(
      `INSERT INTO submission (game_id, player_id, envelope, capture_folder, session, intent_league, received_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (game_id, player_id) DO NOTHING RETURNING id`,
      [game.id, player.id, JSON.stringify(env), env.capture_folder, JSON.stringify(env.session ?? {}), env.intent?.league === true, now],
    )).rows[0];
    if (!sub) return { status: "duplicate", game_id: game.id };

    // Participations from this machine's pads. The lowest pad is the signed-in player.
    const teamIndex = Number.isInteger(rm?.local_team_index) ? rm.local_team_index : null;
    for (let i = 0; i < pads.length; i++) {
      const p = pads[i];
      await c.query(
        `INSERT INTO participation (game_id, player_id, kind, pad, registry_team, team_index, source) VALUES ($1, $2, $3, $4, $5, $6, 'submission')
         ON CONFLICT (game_id, player_id) WHERE player_id IS NOT NULL DO NOTHING`,
        [game.id, i === 0 ? player.id : null, i === 0 ? "human" : "guest", p.pad, Number.isInteger(p.registry_team) ? p.registry_team : null, i === 0 ? teamIndex : null],
      );
    }
    if (mode === "offline" && pads.length === 1) {
      const other = teamIndex === null ? null : 1 - teamIndex;
      await c.query(`INSERT INTO participation (game_id, player_id, kind, pad, registry_team, team_index, source) VALUES ($1, NULL, 'cpu', NULL, NULL, $2, 'submission')`, [game.id, other]);
    }

    const seatClaims = (await c.query(`SELECT registry_team FROM participation WHERE game_id = $1 AND kind = 'human'`, [game.id])).rows;
    const decided = decideStatus({ cap, mode, pads, seatClaims });
    const unpaired = mode === "online" && seatClaims.length < 2;
    await c.query(
      `UPDATE game SET status = $2, hold_reason = $3, resolved_at = $4,
              flags = (SELECT ARRAY(SELECT DISTINCT unnest(flags || $5::text[])))
        WHERE id = $1`,
      [game.id, decided.status, decided.reason, decided.status === "held" ? null : now, unpaired ? ["unpaired-online"] : []],
    );
    if (env.intent?.league === true) {
      await c.query(`INSERT INTO league_forward (game_id) VALUES ($1) ON CONFLICT (game_id) DO NOTHING`, [game.id]);
    }
    return { status: decided.status, game_id: game.id, reason: decided.reason ?? undefined };
  });
}

export function resultsRouter({ pool, clock }) {
  const r = express.Router();
  r.post("/v1/results", express.json({ limit: MAX_BODY_BYTES }), requirePlayer(pool, clock), async (req, res, next) => {
    try {
      const out = await processSubmission(pool, clock, req.player, req.body);
      res.json({ ok: true, ...out });
    } catch (e) {
      if (e.status === 400) return res.status(400).json({ ok: false, error: e.message });
      if (e.type === "entity.too.large") return res.status(413).json({ ok: false, error: "Payload too large" });
      next(e);
    }
  });
  return r;
}
```

- [ ] **Step 4: Run the tests**

Run: `npm test -- test/results.test.js`
Expected: 6 passing. If the `participation_one_per_player` partial index conflict clause errors ("there is no unique or exclusion constraint matching the ON CONFLICT specification"), replace that `ON CONFLICT ... DO NOTHING` with a preceding `SELECT 1 FROM participation WHERE game_id = $1 AND player_id = $2` guard inside the same transaction; the transaction holds the row lock on `submission`, so the guard is safe.

- [ ] **Step 5: Commit**

```bash
git add src/results.js test/results.test.js
git commit -m "Results intake: one game per hash, participants from the client's own pads, hold and incomplete rules"
```

---

### Task 6: The career record and game reads

**Files:**
- Create: `src/players.js`, `test/players.test.js`

**Interfaces:**
- Consumes: `bearerPlayer` (Task 4).
- Produces: `playerSummary(pool, playerId, { viewerId }) -> summary | null`, `playerGames(pool, playerId, { viewerId, limit, before }) -> rows`, `gameView(pool, gameId) -> view | null`, `playersRouter({ pool, clock })` mounting `GET /v1/players/:id`, `GET /v1/players/:id/games`, `GET /v1/games/:id`, `GET /v1/games/:id/submissions`.

- [ ] **Step 1: Write the failing test**

`test/players.test.js`:
```js
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { processSubmission } from "../src/results.js";
import { playerSummary, playerGames, gameView } from "../src/players.js";
import { wireHash } from "../src/canonical.js";

const fixture = () => JSON.parse(readFileSync(new URL("./fixtures/captures/tri_city_001.json", import.meta.url), "utf8"));
const env = (cap, folder, netplay, teamIndex, style) => {
  cap = structuredClone(cap); cap.teams[0].score = 36; cap.teams[1].score = 24; cap.style = style;
  cap.reborn_match = { schema: "nsr.reborn_match.v1", netplay_running: netplay, game_id: "GN7E69", runtime_version: "1.1.0", local_pads: [{ pad: 0, registry_team: 0 }], local_team_index: teamIndex };
  return { schema: "nsr.submission.v1", capture: cap, capture_hash: wireHash(cap), capture_folder: folder, session: {}, intent: { league: false }, submitted_at: "2026-09-08T21:00:00Z" };
};

test("summary counts wins and losses from the player's team index, and hides offline games from strangers", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const p = (await t.pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ('1','A') RETURNING *`)).rows[0];
    await processSubmission(t.pool, clock, p, env(fixture(), "offline", false, 0, [1, 2]));   // won 36-24 as team 0
    await processSubmission(t.pool, clock, p, env(fixture(), "offline", false, 1, [3, 4]));   // lost as team 1
    await processSubmission(t.pool, clock, p, env(fixture(), "online", true, 0, [5, 6]));     // won online

    const mine = await playerSummary(t.pool, p.id, { viewerId: p.id });
    assert.equal(mine.games, 3); assert.equal(mine.wins, 2); assert.equal(mine.losses, 1);
    assert.equal(mine.points_for, 36 + 24 + 36); assert.equal(mine.points_against, 24 + 36 + 24);

    const theirs = await playerSummary(t.pool, p.id, { viewerId: null });
    assert.equal(theirs.games, 1); assert.equal(theirs.wins, 1);

    await t.pool.query(`UPDATE player SET show_offline = true WHERE id = $1`, [p.id]);
    assert.equal((await playerSummary(t.pool, p.id, { viewerId: null })).games, 3);

    const games = await playerGames(t.pool, p.id, { viewerId: p.id, limit: 10 });
    assert.equal(games.length, 3);
    const view = await gameView(t.pool, games[0].id);
    assert.equal(view.teams.length, 2);
    assert.equal(view.participants.length, 2);
    assert.equal(view.participants.find((x) => x.kind === "human").display_name, "A");
  } finally { await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm test -- test/players.test.js`
Expected: FAIL, module missing.

- [ ] **Step 3: Write src/players.js**

```js
// Reading the record. Offline games are private to the player unless show_offline is set.
import express from "express";
import { bearerPlayer } from "./auth.js";

const VISIBLE = `(g.mode = 'online' OR p.show_offline OR p.id = $2::uuid)`;

export async function playerSummary(pool, playerId, { viewerId = null } = {}) {
  const p = (await pool.query(`SELECT id, display_name, avatar_hash, show_offline, deleted_at FROM player WHERE id = $1`, [playerId])).rows[0];
  if (!p || p.deleted_at) return null;
  const row = (await pool.query(
    `SELECT count(*)::int AS games,
            count(*) FILTER (WHERE g.status = 'accepted' AND pa.team_index IS NOT NULL
                             AND (g.teams->pa.team_index->>'score')::int > (g.teams->(1 - pa.team_index)->>'score')::int)::int AS wins,
            count(*) FILTER (WHERE g.status = 'accepted' AND pa.team_index IS NOT NULL
                             AND (g.teams->pa.team_index->>'score')::int < (g.teams->(1 - pa.team_index)->>'score')::int)::int AS losses,
            count(*) FILTER (WHERE g.status = 'incomplete')::int AS incompletes,
            count(*) FILTER (WHERE g.status = 'held')::int AS held,
            coalesce(sum(CASE WHEN pa.team_index IS NULL THEN 0 ELSE (g.teams->pa.team_index->>'score')::int END), 0)::int AS points_for,
            coalesce(sum(CASE WHEN pa.team_index IS NULL THEN 0 ELSE (g.teams->(1 - pa.team_index)->>'score')::int END), 0)::int AS points_against,
            coalesce(sum(CASE WHEN pa.team_index IS NULL OR g.team_totals IS NULL THEN 0 ELSE (g.team_totals->pa.team_index->>'style')::int END), 0)::bigint AS style
       FROM participation pa JOIN game g ON g.id = pa.game_id JOIN player p ON p.id = pa.player_id
      WHERE pa.player_id = $1 AND g.status <> 'void' AND ${VISIBLE}`,
    [playerId, viewerId],
  )).rows[0];
  const totals = (await pool.query(
    `SELECT key, sum(value::int)::int AS total
       FROM participation pa JOIN game g ON g.id = pa.game_id JOIN player p ON p.id = pa.player_id,
            jsonb_each_text(g.team_totals->pa.team_index) AS kv(key, value)
      WHERE pa.player_id = $1 AND pa.team_index IS NOT NULL AND g.team_totals IS NOT NULL AND g.status <> 'void' AND ${VISIBLE}
        AND key NOT IN ('score','style')
      GROUP BY key ORDER BY key`,
    [playerId, viewerId],
  )).rows;
  return { id: p.id, display_name: p.display_name, avatar_hash: p.avatar_hash, ...row, style: Number(row.style), category_totals: Object.fromEntries(totals.map((r) => [r.key, r.total])) };
}

export async function playerGames(pool, playerId, { viewerId = null, limit = 50, before = null } = {}) {
  const lim = Math.max(1, Math.min(200, Number(limit) || 50));
  return (await pool.query(
    `SELECT g.id, g.mode, g.teams, g.status, g.hold_reason, g.captured_at, g.first_seen_at, pa.team_index
       FROM participation pa JOIN game g ON g.id = pa.game_id JOIN player p ON p.id = pa.player_id
      WHERE pa.player_id = $1 AND g.status <> 'void' AND ${VISIBLE} AND ($3::timestamptz IS NULL OR g.first_seen_at < $3)
      ORDER BY g.first_seen_at DESC LIMIT $4`,
    [playerId, viewerId, before, lim],
  )).rows;
}

export async function gameView(pool, gameId) {
  const g = (await pool.query(`SELECT id, mode, teams, style, team_totals, players, status, hold_reason, flags, captured_at, first_seen_at FROM game WHERE id = $1`, [gameId])).rows[0];
  if (!g) return null;
  const participants = (await pool.query(
    `SELECT pa.kind, pa.pad, pa.team_index, pa.player_id, p.display_name, p.avatar_hash
       FROM participation pa LEFT JOIN player p ON p.id = pa.player_id WHERE pa.game_id = $1 ORDER BY pa.kind, pa.pad`, [gameId])).rows;
  return { ...g, participants };
}

export function playersRouter({ pool, clock }) {
  const r = express.Router();
  const viewer = async (req) => (await bearerPlayer(pool, clock, req.headers.authorization))?.player ?? null;

  r.get("/v1/players/:id", async (req, res, next) => {
    try {
      const v = await viewer(req);
      const s = await playerSummary(pool, req.params.id, { viewerId: v?.id ?? null });
      if (!s) return res.status(404).json({ ok: false, error: "No such player" });
      res.json({ ok: true, player: s });
    } catch (e) { next(e); }
  });
  r.get("/v1/players/:id/games", async (req, res, next) => {
    try {
      const v = await viewer(req);
      res.json({ ok: true, games: await playerGames(pool, req.params.id, { viewerId: v?.id ?? null, limit: req.query.limit, before: req.query.before ?? null }) });
    } catch (e) { next(e); }
  });
  r.get("/v1/games/:id", async (req, res, next) => {
    try {
      const g = await gameView(pool, req.params.id);
      if (!g) return res.status(404).json({ ok: false, error: "No such game" });
      const v = await viewer(req);
      const isParticipant = g.participants.some((p) => p.player_id && p.player_id === v?.id);
      if (g.mode === "offline" && !isParticipant) {
        const owner = g.participants.find((p) => p.kind === "human" && p.player_id);
        const shown = owner ? (await pool.query(`SELECT show_offline FROM player WHERE id = $1`, [owner.player_id])).rows[0]?.show_offline : false;
        if (!shown) return res.status(404).json({ ok: false, error: "No such game" });
      }
      res.json({ ok: true, game: g });
    } catch (e) { next(e); }
  });
  r.get("/v1/games/:id/submissions", async (req, res, next) => {
    try {
      const v = await viewer(req);
      const admin = req.headers["x-admin-token"] && req.headers["x-admin-token"] === process.env.ADMIN_TOKEN;
      const rows = (await pool.query(`SELECT id, player_id, envelope, capture_folder, session, intent_league, received_at FROM submission WHERE game_id = $1`, [req.params.id])).rows;
      if (!admin && !rows.some((s) => s.player_id === v?.id)) return res.status(404).json({ ok: false, error: "No such game" });
      res.json({ ok: true, submissions: admin ? rows : rows.filter((s) => s.player_id === v.id) });
    } catch (e) { next(e); }
  });
  return r;
}
```

- [ ] **Step 4: Run the tests**

Run: `npm test -- test/players.test.js`
Expected: 1 passing.

- [ ] **Step 5: Commit**

```bash
git add src/players.js test/players.test.js
git commit -m "Career record: summary, game list and game view with the offline visibility rule"
```

---

### Task 7: League forwarding with retry

**Files:**
- Create: `src/forward.js`, `test/forward.test.js`

**Interfaces:**
- Consumes: `league_forward` rows created by `processSubmission` (Task 5).
- Produces: `buildForwardBody(pool, gameId) -> object` (spec 8.1), `runForwards(pool, clock, { fetchImpl, url, token, userAgent, maxAttempts = 20 }) -> { attempted, applied, review, duplicate, failed, deferred }`, `classifyForwardReply(status, body) -> { status, site_import_id?, reason?, retry: boolean }`.

- [ ] **Step 1: Write the failing test**

`test/forward.test.js`:
```js
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { testPool } from "./helpers/db.js";
import { fixedClock, fakeFetch } from "./helpers/fakes.js";
import { processSubmission } from "../src/results.js";
import { buildForwardBody, runForwards, classifyForwardReply } from "../src/forward.js";
import { wireHash } from "../src/canonical.js";

const fixture = () => JSON.parse(readFileSync(new URL("./fixtures/captures/tri_city_001.json", import.meta.url), "utf8"));
function leagueEnv(cap, pads, teamIndex) {
  cap = structuredClone(cap); cap.teams[0].score = 36; cap.teams[1].score = 24;
  cap.reborn_match = { schema: "nsr.reborn_match.v1", netplay_running: true, game_id: "GN7E69", runtime_version: "1.1.0", local_pads: pads, local_team_index: teamIndex };
  return { schema: "nsr.submission.v1", capture: cap, capture_hash: wireHash(cap), capture_folder: "online", session: {}, intent: { league: true }, submitted_at: "2026-09-08T21:00:00Z" };
}
const URL_ = "https://site.test/league/stats/results_from_backend.php";

test("classifyForwardReply", () => {
  assert.deepEqual(classifyForwardReply(200, '{"ok":true,"status":"applied","import_id":7}'), { status: "applied", site_import_id: 7, reason: null, retry: false });
  assert.deepEqual(classifyForwardReply(200, '{"ok":true,"status":"review","import_id":8,"reason":"no owner linked to Discord id 5"}'), { status: "review", site_import_id: 8, reason: "no owner linked to Discord id 5", retry: false });
  assert.deepEqual(classifyForwardReply(401, '{"ok":false,"error":"Unauthorized"}'), { status: "failed", site_import_id: null, reason: "Unauthorized", retry: false });
  assert.equal(classifyForwardReply(503, "").retry, true);
});

test("a league game is forwarded with both Discord ids, and a 5xx retries later", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const a = (await t.pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ('1001','A') RETURNING *`)).rows[0];
    const b = (await t.pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ('1002','B') RETURNING *`)).rows[0];
    const cap = fixture();
    const ra = await processSubmission(t.pool, clock, a, leagueEnv(cap, [{ pad: 0, registry_team: 0 }], 0));
    await processSubmission(t.pool, clock, b, leagueEnv(cap, [{ pad: 1, registry_team: 1 }], 1));

    const body = await buildForwardBody(t.pool, ra.game_id);
    assert.equal(body.schema, "nsr.league_result.v1");
    assert.deepEqual(body.participants.map((p) => [p.discord_user_id, p.team_index, p.kind]).sort(), [["1001", 0, "human"], ["1002", 1, "human"]]);
    assert.equal(body.capture.schema, "nsr.game.v1");

    let f = fakeFetch({ [URL_]: { status: 503, body: "" } });
    let r = await runForwards(t.pool, clock, { fetchImpl: f, url: URL_, token: "secret", userAgent: "Reborn-Backend/1.0" });
    assert.equal(r.deferred, 1);
    assert.equal(f.calls[0].init.headers.Authorization, "Bearer secret");
    assert.equal(f.calls[0].init.headers["User-Agent"], "Reborn-Backend/1.0");
    assert.equal((await t.pool.query(`SELECT status, attempts FROM league_forward`)).rows[0].attempts, 1);

    f = fakeFetch({ [URL_]: { status: 200, body: '{"ok":true,"status":"applied","import_id":42}' } });
    r = await runForwards(t.pool, clock, { fetchImpl: f, url: URL_, token: "secret", userAgent: "x" });
    assert.equal(r.applied, 1);
    const row = (await t.pool.query(`SELECT status, site_import_id FROM league_forward`)).rows[0];
    assert.deepEqual([row.status, row.site_import_id], ["applied", 42]);

    r = await runForwards(t.pool, clock, { fetchImpl: f, url: URL_, token: "secret", userAgent: "x" });
    assert.equal(r.attempted, 0);
  } finally { await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm test -- test/forward.test.js`
Expected: FAIL, module missing.

- [ ] **Step 3: Write src/forward.js**

```js
// League games go to the league site server to server. The site's shared
// token lives here and nowhere on a player's machine.

export function classifyForwardReply(status, body) {
  let parsed = null;
  try { parsed = JSON.parse(body); } catch { parsed = null; }
  if (status >= 200 && status < 300 && parsed?.ok === true) {
    const s = ["applied", "duplicate", "review"].includes(parsed.status) ? parsed.status : "review";
    return { status: s, site_import_id: Number.isInteger(parsed.import_id) ? parsed.import_id : null, reason: parsed.reason ?? null, retry: false };
  }
  if (status >= 500 || status === 0) return { status: "pending", site_import_id: null, reason: `server error ${status}`, retry: true };
  return { status: "failed", site_import_id: null, reason: parsed?.error ?? parsed?.reason ?? `refused with ${status}`, retry: false };
}

export async function buildForwardBody(pool, gameId) {
  const g = (await pool.query(`SELECT id, captured_at FROM game WHERE id = $1`, [gameId])).rows[0];
  const sub = (await pool.query(`SELECT envelope FROM submission WHERE game_id = $1 ORDER BY received_at ASC LIMIT 1`, [gameId])).rows[0];
  const parts = (await pool.query(
    `SELECT pa.kind, pa.team_index, p.discord_user_id FROM participation pa LEFT JOIN player p ON p.id = pa.player_id
      WHERE pa.game_id = $1 AND pa.kind <> 'cpu' ORDER BY pa.pad NULLS LAST`, [gameId])).rows;
  return {
    schema: "nsr.league_result.v1",
    game_id: g.id,
    captured_at: g.captured_at ? g.captured_at.toISOString() : null,
    capture: sub.envelope.capture,
    participants: parts.map((p) => ({ discord_user_id: p.discord_user_id ?? null, team_index: p.team_index, kind: p.kind })),
  };
}

export async function runForwards(pool, clock, { fetchImpl = fetch, url, token, userAgent, maxAttempts = 20, limit = 20 }) {
  const out = { attempted: 0, applied: 0, review: 0, duplicate: 0, failed: 0, deferred: 0 };
  if (!url || !token) return out;
  const due = (await pool.query(
    `SELECT id, game_id, attempts FROM league_forward WHERE status = 'pending' AND attempts < $1
      ORDER BY created_at ASC LIMIT $2`, [maxAttempts, limit])).rows;
  for (const row of due) {
    out.attempted++;
    let status = 0, text = "";
    try {
      const body = await buildForwardBody(pool, row.game_id);
      const res = await fetchImpl(url, { method: "POST", headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}`, "User-Agent": userAgent }, body: JSON.stringify(body) });
      status = res.status; text = await res.text();
    } catch (e) { status = 0; text = String(e.message); }
    const c = classifyForwardReply(status, text);
    if (c.retry) {
      await pool.query(`UPDATE league_forward SET attempts = attempts + 1, last_attempt_at = $2, reason = $3, status = CASE WHEN attempts + 1 >= $4 THEN 'failed' ELSE 'pending' END WHERE id = $1`, [row.id, clock.now(), c.reason, maxAttempts]);
      out.deferred++;
    } else {
      await pool.query(`UPDATE league_forward SET status = $2, site_import_id = $3, reason = $4, attempts = attempts + 1, last_attempt_at = $5 WHERE id = $1`, [row.id, c.status, c.site_import_id, c.reason, clock.now()]);
      out[c.status]++;
    }
  }
  return out;
}
```

- [ ] **Step 4: Run the tests**

Run: `npm test -- test/forward.test.js`
Expected: 2 passing.

- [ ] **Step 5: Commit**

```bash
git add src/forward.js test/forward.test.js
git commit -m "Forward league games to the league site with retry, holding the shared token server side"
```

---

### Task 8: Admin and account actions

**Files:**
- Create: `src/admin.js`, `src/account.js`, `test/admin_account.test.js`

**Interfaces:**
- Produces:
  - `adminRouter({ pool, clock, adminToken })` mounting `GET /v1/admin/held`, `POST /v1/admin/games/:id` (`{ status?, participations?: [{ id, team_index }] }`), `POST /v1/admin/backfill` (`{ registry_team_to_index: { "0": n, "1": n } }`), `POST /v1/admin/forward/run`.
  - `backfillTeamIndex(pool, map) -> number` rows updated.
  - `accountRouter({ pool, clock })` mounting `POST /v1/me/unlink`, `DELETE /v1/me`, `GET /v1/me/export`.
  - `deletePlayer(pool, clock, playerId)`, `unlinkPlayer(pool, clock, playerId)`, `exportPlayer(pool, playerId)`.

- [ ] **Step 1: Write the failing test**

`test/admin_account.test.js`:
```js
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { processSubmission } from "../src/results.js";
import { backfillTeamIndex } from "../src/admin.js";
import { deletePlayer, unlinkPlayer, exportPlayer } from "../src/account.js";
import { wireHash } from "../src/canonical.js";
import { hashToken, newToken } from "../src/tokens.js";

const fixture = () => JSON.parse(readFileSync(new URL("./fixtures/captures/tri_city_001.json", import.meta.url), "utf8"));
const env = (cap, registryTeam, style) => {
  cap = structuredClone(cap); cap.teams[0].score = 36; cap.teams[1].score = 24; cap.style = style;
  cap.reborn_match = { schema: "nsr.reborn_match.v1", netplay_running: false, game_id: "GN7E69", runtime_version: "1.1.0", local_pads: [{ pad: 0, registry_team: registryTeam }] };
  return { schema: "nsr.submission.v1", capture: cap, capture_hash: wireHash(cap), capture_folder: "offline", session: {}, intent: { league: false }, submitted_at: "2026-09-08T21:00:00Z" };
};

test("backfill fills team_index from the raw registry team once the mapping is known", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const p = (await t.pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ('1','A') RETURNING *`)).rows[0];
    await processSubmission(t.pool, clock, p, env(fixture(), 1, [1, 1]));
    assert.equal((await t.pool.query(`SELECT team_index FROM participation WHERE kind = 'human'`)).rows[0].team_index, null);
    const n = await backfillTeamIndex(t.pool, { 0: 1, 1: 0 });
    assert.equal(n, 2);   // the human and the cpu opponent
    const rows = (await t.pool.query(`SELECT kind, team_index, source FROM participation ORDER BY kind`)).rows;
    assert.deepEqual(rows, [{ kind: "cpu", team_index: 1, source: "backfill" }, { kind: "human", team_index: 0, source: "backfill" }]);
  } finally { await t.close(); }
});

test("unlink keeps history under a tombstone; delete removes envelopes and voids a held game", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const p = (await t.pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ('1','A') RETURNING *`)).rows[0];
    await t.pool.query(`INSERT INTO device_token (player_id, token_hash) VALUES ($1, $2)`, [p.id, hashToken(newToken())]);
    await processSubmission(t.pool, clock, p, env(fixture(), 1, [1, 1]));
    const tied = env(fixture(), 1, [2, 2]); tied.capture.teams[1].score = 36; tied.capture_hash = wireHash(tied.capture);
    await processSubmission(t.pool, clock, p, tied);

    const exp = await exportPlayer(t.pool, p.id);
    assert.equal(exp.games.length, 2); assert.equal(exp.submissions.length, 2);

    await unlinkPlayer(t.pool, clock, p.id);
    const after = (await t.pool.query(`SELECT discord_user_id, display_name FROM player WHERE id = $1`, [p.id])).rows[0];
    assert.equal(after.discord_user_id, null);
    assert.equal((await t.pool.query(`SELECT count(*)::int AS n FROM participation WHERE player_id = $1`, [p.id])).rows[0].n, 2);

    await deletePlayer(t.pool, clock, p.id);
    assert.equal((await t.pool.query(`SELECT count(*)::int AS n FROM submission WHERE player_id = $1`, [p.id])).rows[0].n, 0);
    assert.equal((await t.pool.query(`SELECT count(*)::int AS n FROM device_token WHERE player_id = $1 AND revoked_at IS NULL`, [p.id])).rows[0].n, 0);
    assert.equal((await t.pool.query(`SELECT display_name, deleted_at IS NOT NULL AS gone FROM player WHERE id = $1`, [p.id])).rows[0].gone, true);
    assert.equal((await t.pool.query(`SELECT count(*)::int AS n FROM game WHERE status = 'void'`)).rows[0].n, 1);
    assert.equal((await t.pool.query(`SELECT count(*)::int AS n FROM game WHERE status = 'accepted'`)).rows[0].n, 1);
  } finally { await t.close(); }
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm test -- test/admin_account.test.js`
Expected: FAIL, modules missing.

- [ ] **Step 3: Write src/admin.js**

```js
import express from "express";
import { runForwards } from "./forward.js";

export function requireAdmin(adminToken) {
  return (req, res, next) => {
    const given = req.headers["x-admin-token"];
    if (!adminToken || !given || given !== adminToken) return res.status(401).json({ ok: false, error: "admin token required" });
    next();
  };
}

/** Fill team_index from registry_team for every participation still missing it. cpu rows take the other side. */
export async function backfillTeamIndex(pool, map) {
  const m0 = Number(map?.[0]), m1 = Number(map?.[1]);
  if (![0, 1].includes(m0) || ![0, 1].includes(m1) || m0 === m1) throw Object.assign(new Error("registry_team_to_index must map 0 and 1 to different indexes"), { status: 400 });
  const humans = await pool.query(
    `UPDATE participation SET team_index = CASE registry_team WHEN 0 THEN $1::smallint WHEN 1 THEN $2::smallint END, source = 'backfill'
      WHERE team_index IS NULL AND registry_team IN (0, 1) AND kind IN ('human','guest')`, [m0, m1]);
  const cpu = await pool.query(
    `UPDATE participation c SET team_index = 1 - h.team_index, source = 'backfill'
       FROM participation h
      WHERE c.kind = 'cpu' AND c.team_index IS NULL AND h.game_id = c.game_id AND h.kind = 'human' AND h.team_index IS NOT NULL`);
  return humans.rowCount + cpu.rowCount;
}

export function adminRouter({ pool, clock, adminToken, forwardEnv }) {
  const r = express.Router();
  r.use(express.json({ limit: "64kb" }));
  r.use(requireAdmin(adminToken));

  r.get("/v1/admin/held", async (req, res, next) => {
    try {
      const games = (await pool.query(`SELECT id, mode, teams, hold_reason, flags, first_seen_at FROM game WHERE status = 'held' ORDER BY first_seen_at DESC LIMIT 200`)).rows;
      for (const g of games) {
        g.submissions = (await pool.query(`SELECT player_id, envelope, received_at FROM submission WHERE game_id = $1`, [g.id])).rows;
        g.participants = (await pool.query(`SELECT id, kind, pad, registry_team, team_index, player_id FROM participation WHERE game_id = $1`, [g.id])).rows;
      }
      res.json({ ok: true, games });
    } catch (e) { next(e); }
  });

  r.post("/v1/admin/games/:id", async (req, res, next) => {
    try {
      const { status, participations } = req.body ?? {};
      if (status && !["accepted", "incomplete", "held", "void"].includes(status)) return res.status(400).json({ ok: false, error: "bad status" });
      if (status) await pool.query(`UPDATE game SET status = $2, hold_reason = CASE WHEN $2 = 'held' THEN hold_reason ELSE NULL END, resolved_at = $3 WHERE id = $1`, [req.params.id, status, clock.now()]);
      for (const p of Array.isArray(participations) ? participations : []) {
        if (![0, 1].includes(p.team_index)) continue;
        await pool.query(`UPDATE participation SET team_index = $3, source = 'admin' WHERE id = $1 AND game_id = $2`, [p.id, req.params.id, p.team_index]);
      }
      res.json({ ok: true });
    } catch (e) { next(e); }
  });

  r.post("/v1/admin/backfill", async (req, res, next) => {
    try { res.json({ ok: true, updated: await backfillTeamIndex(pool, req.body?.registry_team_to_index) }); }
    catch (e) { if (e.status === 400) return res.status(400).json({ ok: false, error: e.message }); next(e); }
  });

  r.post("/v1/admin/forward/run", async (req, res, next) => {
    try { res.json({ ok: true, ...(await runForwards(pool, clock, forwardEnv)) }); } catch (e) { next(e); }
  });
  return r;
}
```

- [ ] **Step 4: Write src/account.js**

```js
// Sign out, unlink, export, delete. Spec section 4.4 and the protocol doc's 6.4.
import express from "express";
import { withTx } from "./db.js";
import { requirePlayer } from "./auth.js";

/** Remove the Discord binding. History stays; the account becomes unreachable, not absent. */
export async function unlinkPlayer(pool, clock, playerId) {
  await withTx(pool, async (c) => {
    await c.query(`UPDATE device_token SET revoked_at = $2 WHERE player_id = $1 AND revoked_at IS NULL`, [playerId, clock.now()]);
    await c.query(`UPDATE player SET discord_user_id = NULL, avatar_hash = NULL WHERE id = $1`, [playerId]);
  });
}

/** Anonymize, do not erase: shared rows keep a tombstone, the player's own envelopes go, a held game is voided. */
export async function deletePlayer(pool, clock, playerId) {
  await withTx(pool, async (c) => {
    await c.query(`UPDATE game SET status = 'void', hold_reason = 'participant deleted', resolved_at = $2
                    WHERE status = 'held' AND id IN (SELECT game_id FROM participation WHERE player_id = $1)`, [playerId, clock.now()]);
    await c.query(`DELETE FROM submission WHERE player_id = $1`, [playerId]);
    await c.query(`UPDATE device_token SET revoked_at = $2 WHERE player_id = $1 AND revoked_at IS NULL`, [playerId, clock.now()]);
    await c.query(`UPDATE player SET discord_user_id = NULL, display_name = 'Deleted player', avatar_hash = NULL, deleted_at = $2 WHERE id = $1`, [playerId, clock.now()]);
  });
}

export async function exportPlayer(pool, playerId) {
  const player = (await pool.query(`SELECT id, discord_user_id, display_name, avatar_hash, show_offline, created_at FROM player WHERE id = $1`, [playerId])).rows[0];
  const games = (await pool.query(`SELECT g.*, pa.team_index, pa.kind FROM participation pa JOIN game g ON g.id = pa.game_id WHERE pa.player_id = $1 ORDER BY g.first_seen_at`, [playerId])).rows;
  const submissions = (await pool.query(`SELECT * FROM submission WHERE player_id = $1 ORDER BY received_at`, [playerId])).rows;
  const tokens = (await pool.query(`SELECT label, created_at, last_used_at, revoked_at FROM device_token WHERE player_id = $1`, [playerId])).rows;
  return { player, games, submissions, tokens };
}

export function accountRouter({ pool, clock }) {
  const r = express.Router();
  r.post("/v1/me/unlink", requirePlayer(pool, clock), async (req, res, next) => {
    try { await unlinkPlayer(pool, clock, req.player.id); res.json({ ok: true }); } catch (e) { next(e); }
  });
  r.delete("/v1/me", requirePlayer(pool, clock), async (req, res, next) => {
    try { await deletePlayer(pool, clock, req.player.id); res.json({ ok: true }); } catch (e) { next(e); }
  });
  r.get("/v1/me/export", requirePlayer(pool, clock), async (req, res, next) => {
    try { res.json({ ok: true, ...(await exportPlayer(pool, req.player.id)) }); } catch (e) { next(e); }
  });
  return r;
}
```

- [ ] **Step 5: Run the tests**

Run: `npm test -- test/admin_account.test.js`
Expected: 2 passing.

- [ ] **Step 6: Commit**

```bash
git add src/admin.js src/account.js test/admin_account.test.js
git commit -m "Admin held list, resolve and backfill; sign out, unlink, export and delete"
```

---

### Task 9: The app, the server, rate limiting, and a full-stack test

**Files:**
- Create: `src/app.js`, `src/server.js`, `test/app.test.js`

**Interfaces:**
- Produces: `buildApp({ pool, clock, discord, env }) -> express app` with `GET /health`; `server.js` boots on `PORT` and runs `runForwards` every 5 minutes.

- [ ] **Step 1: Write the failing test**

`test/app.test.js`:
```js
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { testPool } from "./helpers/db.js";
import { fakeDiscord, fixedClock } from "./helpers/fakes.js";
import { buildApp } from "../src/app.js";
import { wireHash } from "../src/canonical.js";

const ENV = { STATE_SECRET: "s", ADMIN_TOKEN: "admin", PUBLIC_ORIGIN: "http://127.0.0.1", LEAGUE_FORWARD_URL: "", LEAGUE_FORWARD_TOKEN: "" };

async function withServer(fn) {
  const t = await testPool(); const clock = fixedClock();
  const app = buildApp({ pool: t.pool, clock, discord: fakeDiscord(), env: ENV });
  const server = await new Promise((r) => { const s = app.listen(0, () => r(s)); });
  const base = `http://127.0.0.1:${server.address().port}`;
  try { await fn(base, t.pool); } finally { server.close(); await t.close(); }
}

test("health, sign-in over HTTP, a result, and the record", async () => {
  await withServer(async (base) => {
    assert.equal((await fetch(`${base}/health`)).status, 200);

    const start = await (await fetch(`${base}/v1/auth/device`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ label: "PC" }) })).json();
    assert.equal((await fetch(`${base}/v1/auth/device/poll`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ device_code: start.device_code }) })).status, 428);

    // The verify page hands out the CSRF cookie and hidden field; the form post must carry both and a same-origin Origin.
    const linkPage = await fetch(`${base}/link?code=${start.user_code}`);
    const cookie = /nsr_link=([A-Za-z0-9_-]+)/.exec(linkPage.headers.get("set-cookie") ?? "")?.[1];
    const csrf = /name="csrf" value="([A-Za-z0-9_-]+)"/.exec(await linkPage.text())?.[1];
    assert.ok(cookie && csrf, "the verify page must set the CSRF pair");
    const startPage = await fetch(`${base}/link/start`, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded", Cookie: `nsr_link=${cookie}`, Origin: "http://127.0.0.1" }, body: `code=${start.user_code}&csrf=${csrf}`, redirect: "manual" });
    assert.equal(startPage.status, 302);
    const state = new URL(startPage.headers.get("location")).searchParams.get("state");
    const cb = await fetch(`${base}/v1/auth/discord/callback?code=good-code&state=${encodeURIComponent(state)}`);
    assert.equal(cb.status, 200);
    const done = await (await fetch(`${base}/v1/auth/device/poll`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ device_code: start.device_code }) })).json();
    assert.ok(done.token);

    const me = await (await fetch(`${base}/v1/me`, { headers: { Authorization: `Bearer ${done.token}` } })).json();
    assert.equal(me.player.display_name, "Adam");

    const cap = JSON.parse(readFileSync(new URL("./fixtures/captures/tri_city_001.json", import.meta.url), "utf8"));
    cap.teams[0].score = 36; cap.teams[1].score = 24;
    // local_team_index 0 is the 36-point side, so the summary below counts one win.
    cap.reborn_match = { schema: "nsr.reborn_match.v1", netplay_running: false, game_id: "GN7E69", runtime_version: "1.1.0", local_pads: [{ pad: 0, registry_team: 1 }], local_team_index: 0 };
    const env = { schema: "nsr.submission.v1", capture: cap, capture_hash: wireHash(cap), capture_folder: "offline", session: { session_id: "1" }, intent: { league: false }, submitted_at: "2026-09-08T21:00:00Z" };
    const r = await (await fetch(`${base}/v1/results`, { method: "POST", headers: { "Content-Type": "application/json", Authorization: `Bearer ${done.token}` }, body: JSON.stringify(env) })).json();
    assert.equal(r.status, "accepted");

    const summary = await (await fetch(`${base}/v1/players/${me.player.id}`, { headers: { Authorization: `Bearer ${done.token}` } })).json();
    assert.equal(summary.player.wins, 1);
    assert.equal((await fetch(`${base}/v1/results`, { method: "POST", headers: { "Content-Type": "application/json" }, body: "{}" })).status, 401);
    assert.equal((await fetch(`${base}/v1/admin/held`)).status, 401);
    assert.equal((await fetch(`${base}/v1/admin/held`, { headers: { "x-admin-token": "admin" } })).status, 200);
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npm test -- test/app.test.js`
Expected: FAIL, `Cannot find module '../src/app.js'`.

- [ ] **Step 3: Write src/app.js**

```js
import express from "express";
import { authRouter } from "./auth.js";
import { resultsRouter } from "./results.js";
import { playersRouter } from "./players.js";
import { accountRouter } from "./account.js";
import { adminRouter } from "./admin.js";

/** 60 requests per minute per bearer token (or per address without one). In memory; one process. */
function rateLimit({ perMinute = 60, clock }) {
  const hits = new Map();
  return (req, res, next) => {
    const key = req.headers.authorization ?? req.ip;
    const now = clock.now().getTime();
    const list = (hits.get(key) ?? []).filter((t) => now - t < 60_000);
    if (list.length >= perMinute) return res.status(429).json({ ok: false, error: "Too many requests. Try again in a minute." });
    list.push(now); hits.set(key, list);
    if (hits.size > 10_000) hits.clear();
    next();
  };
}

export function buildApp({ pool, clock, discord, env }) {
  const app = express();
  app.disable("x-powered-by");
  app.set("trust proxy", 1);
  app.get("/health", async (req, res) => {
    try { await pool.query("select 1"); res.json({ ok: true }); } catch { res.status(503).json({ ok: false }); }
  });
  app.use("/v1", rateLimit({ clock }));
  const forwardEnv = { url: env.LEAGUE_FORWARD_URL, token: env.LEAGUE_FORWARD_TOKEN, userAgent: "NFLStreetReborn-Backend/1.0" };
  app.use(authRouter({ pool, clock, discord, stateSecret: env.STATE_SECRET, publicOrigin: env.PUBLIC_ORIGIN }));
  app.use(resultsRouter({ pool, clock }));
  app.use(playersRouter({ pool, clock }));
  app.use(accountRouter({ pool, clock }));
  app.use(adminRouter({ pool, clock, adminToken: env.ADMIN_TOKEN, forwardEnv }));
  app.use((req, res) => res.status(404).json({ ok: false, error: "No such route" }));
  app.use((err, req, res, next) => {
    if (err?.type === "entity.too.large") return res.status(413).json({ ok: false, error: "Payload too large" });
    if (err?.type === "entity.parse.failed") return res.status(400).json({ ok: false, error: "Body is not JSON" });
    console.error(err);
    res.status(500).json({ ok: false, error: "Something went wrong on the server." });
  });
  return app;
}
```

- [ ] **Step 4: Write src/server.js**

```js
import { createPool, migrate } from "./db.js";
import { realDiscord } from "./discord.js";
import { buildApp } from "./app.js";
import { runForwards } from "./forward.js";

const need = (k) => { const v = process.env[k]; if (!v) { console.error(`${k} is not set`); process.exit(2); } return v; };
const env = {
  DATABASE_URL: need("DATABASE_URL"),
  DISCORD_CLIENT_ID: need("DISCORD_CLIENT_ID"),
  DISCORD_CLIENT_SECRET: need("DISCORD_CLIENT_SECRET"),
  DISCORD_REDIRECT_URI: need("DISCORD_REDIRECT_URI"),
  STATE_SECRET: need("STATE_SECRET"),
  ADMIN_TOKEN: need("ADMIN_TOKEN"),
  PUBLIC_ORIGIN: need("PUBLIC_ORIGIN"),
  LEAGUE_FORWARD_URL: process.env.LEAGUE_FORWARD_URL ?? "",
  LEAGUE_FORWARD_TOKEN: process.env.LEAGUE_FORWARD_TOKEN ?? "",
};
const clock = { now: () => new Date() };
const pool = createPool(env.DATABASE_URL);
const applied = await migrate(pool);
console.log(applied.length ? `migrations applied: ${applied.join(", ")}` : "schema up to date");
const app = buildApp({ pool, clock, discord: realDiscord({ clientId: env.DISCORD_CLIENT_ID, clientSecret: env.DISCORD_CLIENT_SECRET, redirectUri: env.DISCORD_REDIRECT_URI }), env });
const port = Number(process.env.PORT ?? 3000);
app.listen(port, () => console.log(`reborn-backend listening on ${port}`));
const forwardEnv = { url: env.LEAGUE_FORWARD_URL, token: env.LEAGUE_FORWARD_TOKEN, userAgent: "NFLStreetReborn-Backend/1.0" };
setInterval(() => runForwards(pool, clock, forwardEnv).then((r) => { if (r.attempted) console.log("forwards", r); }).catch((e) => console.error("forward pass failed", e)), 5 * 60_000).unref();
```

- [ ] **Step 5: Run the whole suite**

Run: `npm test`
Expected: every test in `test/` passing, including `app.test.js`.

- [ ] **Step 6: Commit**

```bash
git add src/app.js src/server.js test/app.test.js
git commit -m "Express app with health, rate limit and body cap; server boot with migrations and the forward timer"
```

---

### Task 10: Deploy to Railway, domain, and a real sign-in

**Files:**
- Modify: `README.md` (record the live URL and the smoke commands)

- [ ] **Step 1: Adam's prerequisites (spec 11 step 1), checked before continuing**

- Discord Developer Portal, the existing application: OAuth2 > add redirect `https://api.nflstreetreborn.com/v1/auth/discord/callback`; reset the client secret; keep the value in a password manager, never in chat.
- Railway: a project with a Postgres addon (Task 1 step 5 may have created it).
- Cloudflare DNS: `api` CNAME to the Railway-provided target (added in step 3 below).

- [ ] **Step 2: Push the repository and connect Railway**

```bash
gh repo create nflstreetreborn/reborn-backend --private --source . --push
```
Railway dashboard: New Service > Deploy from GitHub repo > this repo. Root directory stays the repository root. Variables: every name in `.env.example` except `DATABASE_URL_TEST`; `DATABASE_URL` from the addon reference; `PUBLIC_ORIGIN=https://api.nflstreetreborn.com`; `DISCORD_REDIRECT_URI=https://api.nflstreetreborn.com/v1/auth/discord/callback`; `STATE_SECRET` and `ADMIN_TOKEN` generated with `node -e "console.log(require('crypto').randomBytes(32).toString('base64url'))"` and pasted into Railway only. Leave `LEAGUE_FORWARD_URL` and `LEAGUE_FORWARD_TOKEN` empty until the league site plan's Task 4 sets them.

- [ ] **Step 3: Domain**

Railway service > Settings > Networking > Custom Domain `api.nflstreetreborn.com`; copy the CNAME target into Cloudflare DNS (DNS only is fine). Wait for Railway to show the certificate as issued.

Run: `curl -s https://api.nflstreetreborn.com/health`
Expected: `{"ok":true}`.

- [ ] **Step 4: One real sign-in**

Run: `curl -s -X POST https://api.nflstreetreborn.com/v1/auth/device -H "Content-Type: application/json" -d "{\"label\":\"smoke\"}"`
Open the returned `verify_url` in a browser, continue with Discord, then poll:
`curl -s -X POST https://api.nflstreetreborn.com/v1/auth/device/poll -H "Content-Type: application/json" -d "{\"device_code\":\"<device_code>\"}"`
Expected: `{"ok":true,"token":"...","player":{...}}`. Immediately revoke that smoke token: `curl -s -X DELETE https://api.nflstreetreborn.com/v1/auth/device -H "Authorization: Bearer <token>"` and expect `{"ok":true}`. Do not paste the token anywhere else.

- [ ] **Step 5: Record the live facts in README.md and commit**

Add under Deploy: the live origin, the Railway project name, and the smoke commands above with placeholders.

```bash
git add README.md
git commit -m "Live at api.nflstreetreborn.com; smoke procedure recorded"
git push
```

---

## Amendments made during execution (2026-09-08)

Rulings recorded in the execution ledger that changed this plan's code. The code in the repository is authoritative where it differs from the blocks above.

- Task 1: the test database is EnterpriseDB's portable PostgreSQL 16 at `C:\Users\Adam\pgsql16`, port 54329, trust auth on localhost, not the winget install (which needs a UAC prompt a non-interactive shell cannot answer).
- Task 1 and 3: the test script is `"test": "node --env-file-if-exists=.env --test"`; `node --test test/` fails on Node 24 and `.env` is not loaded otherwise.
- Task 3: `createPool` applies the schema as a connection startup parameter (`options: "-c search_path=<schema>,public"`), not in a connect hook, which raced the first query; `testPool` drops its schema and ends the pool if migration fails; `close()` ends the pool in `finally`; `migrate()` guards its ROLLBACK.
- Task 4: the OAuth `state` carries a random nonce stored in a new `device_login.state_nonce` column (migration `002_device_login_state_nonce.sql`), never the device code; the approve step is one guarded UPDATE; `/link` shows the device label and sets an HttpOnly SameSite=Strict `nsr_link` cookie paired with a hidden `csrf` field that `/link/start` requires together with a same-origin check; `prompt=none` is gone; the callback error page escapes its input; the router has HTTP-level tests; `POST /v1/auth/device` replies with `ok: true`; `Bearer` is matched case-insensitively; `newUserCode` uses rejection sampling.
- Task 5: the no-seat-claim test uses a distinct capture; `entity.too.large` and `entity.parse.failed` are mapped by a router-scoped error middleware (the in-handler branch was unreachable); `processSubmission` takes `pg_advisory_xact_lock(hashtext(capture_hash))` first so submissions for one game serialize.
- Task 6: `gameView` excludes void games; the games route validates `limit` (integer 1 to 200) and `before` (a parseable date) and answers 400 otherwise; the submissions route answers 404 for a missing game before the admin or participant branch; `playersRouter` has HTTP-level tests.
- Task 7: the retry UPDATE returns the status it wrote, and the pass summary counts a row that hit `maxAttempts` as `failed`, not `deferred`.
- Task 8: `requireAdmin` is mounted before the JSON parser and compares tokens in constant time (SHA-256 digests through `timingSafeEqual`); `/v1/admin/forward/run` tolerates a missing `forwardEnv`; the resolve route runs in one transaction, validates its id and `participations`, answers 404 for a missing game and stamps `resolved_at` only when the new status is not held; the admin and account routers have HTTP-level tests.
- Task 9 (carried): `engines` becomes `>=22` for `--env-file-if-exists`; the rate limiter also covers `/link` and `/link/start`; the app test drives `/link/start` with the cookie, `csrf` field and `Origin`; `src/players.js`'s inline `x-admin-token` check is replaced by an exported `isAdmin(req, adminToken)` helper from `src/admin.js` (the same constant-time compare `requireAdmin` uses), with `playersRouter` taking `adminToken`; `adminRouter`'s guard and JSON parser are scoped with `r.use("/v1/admin", ...)` so an unknown route reaches the 404 handler instead of a 401, proven by the app test. The rate limiter keys on the client address only (a caller-controlled header would mint unlimited buckets), evicts quiet clients instead of clearing every window, sends `Retry-After: 60`, and reads an optional `RATE_LIMIT_PER_MINUTE` so a test can prove a 429; the error handler guards `res.headersSent`; `/health` logs its failure; `PORT` is validated.

- Final review fix wave: `authRouter`'s body parsers are per route, so the 512 KB cap on `/v1/results` is real; `TRUST_PROXY_HOPS` sets the proxy hop count; migration `003_game_decided_by.sql` adds `game.decided_by`, and a submission never overwrites an admin decision or a void; pairing removes `unpaired-online`; only accepted games are forwarded; malformed uuids answer 404; `captured_at` is validated; HTML pages carry `frame-ancestors 'none'`, `nosniff` and `no-store`; migrations take an advisory lock; the test glob is `test/*.test.js`; `engines` is `>=22.9`.
- Spec corrections recorded here rather than silently: `player.discord_user_id` is nullable because unlink and delete null it (spec 4.3 said not null); forwards are enqueued and attempted by the timer or the admin route, not inline (spec 7.2 step 8 said inline once).

## Self-review

**Spec coverage.** 4.1 device flow: Task 4. 4.2 tokens: Task 4. 4.3 tables: Task 3. 4.4 endpoints: Tasks 4 and 8. 6 envelope and 6.1 hash: Tasks 2 and 5. 7.1 tables: Task 3. 7.2 steps 1 to 9: Task 5 (steps 1 to 7, 9), Task 7 (step 8). 7.3 reads and visibility: Task 6. 7.4 backfill: Task 8. 8.1 forward body and reply handling: Task 7. 11 steps 1 and 2: Task 10. 12 hash proof: Task 2. One deviation, stated: the backend stores the launcher's plausibility flags verbatim and computes only the status-bearing codes itself, rather than recomputing all 26 launcher flags; status is still decided server side, which is what the spec is protecting.

**Placeholders.** None. Every step has its content. Secrets are referenced by variable name only.

**Type consistency.** `processSubmission(pool, clock, player, envelope)` is used identically in Tasks 5, 6, 7, 8, 9. `classifyForwardReply` returns `{status, site_import_id, reason, retry}` in Task 7 and its test. `fixedClock().now()` returns a `Date` everywhere. `testPool()` returns `{pool, schema, close}` everywhere.
