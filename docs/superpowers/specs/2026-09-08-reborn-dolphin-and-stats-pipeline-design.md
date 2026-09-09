# NFL Street Reborn Dolphin and the stats pipeline

Design, 2026-09-08. Approved section by section in conversation with Adam the same day.

This document covers three repositories and one new one:

| Part | Where |
|---|---|
| The Dolphin fork | `C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\dolphin-exgl` |
| The dev runtime folder | `C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\CREATION LEAGUE AND NEW DOLPHIN`, renamed by this work to `NFL Street Reborn Dolphin` |
| The launcher | `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher` |
| The league site working tree | `C:\Users\Adam\nsr_local\public_html\league` (the current tree; the git mirror under `Documents\jacobg44.sg-host.com` is behind it) |
| The Reborn backend | New repository, `C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend`, deployed on Railway |

It supersedes section 10 of `docs/architecture/ONLINE_AND_RESULTS_PROTOCOL.md` where the two differ, and it adopts that document's identity chain, submission envelope, match result shape, hold rules, security corrections and reuse verdicts everywhere else.

## 1. Decisions this design rests on

Adam decided these in conversation on 2026-09-08. They are not reopened below.

1. **The launcher sends match results. Dolphin never touches the network.** The in-Dolphin uploader and the token compiled into it are removed.
2. **Identity is Discord.** Every player, in every league or none, signs in once with Discord. The system is for the whole player base and is the foundation for online matchmaking, not a league feature.
3. **Every game is recorded**, online or offline, against the player's Discord identity.
4. **Upload first, then capture.** Phase 1 makes results land. Phase 2 hardens the in-game capture and gets its own spec.
5. **The exe is renamed** `NFL Street Reborn Dolphin.exe`, and the window and product strings match.
6. **Server side is a new backend on Railway with Postgres**, the league site being one consumer of it. This matches the locked decision in `SYSTEM_ARCHITECTURE.md`: website on SiteGround, matchmaking and real time on Railway.
7. **Capture folder is `User\GameStats\` with `Online\` and `Offline\` subfolders.**
8. **The league site's intake is rebuilt around identity**, not left alone.

## 2. Why this work exists

Measured on 2026-09-08:

- The launcher's stats queue on Adam's machine holds 5 captures, all `queued`, `attempts 0`. Nothing has ever been sent from the launcher. Its Settings page no longer has a token field, so the submit command cannot run, and if it ran it would post the raw capture with no `league_id`, which the live endpoint refuses with `Missing league_id`.
- Dolphin's own uploader never sees a capture any more, because the launcher moves them out of `LeagueGameStats` after each session. Its shared token is compiled into the exe and was printed into a transcript again on 2026-09-08 while the source was read. It must be rotated regardless of this design.
- On the league site, 34 uploads had produced 0 applied results as of 2026-07-28. Of the 21 still held, 9 arrived without team totals, 7 with unverified sides, and the rest with truncated or unknown team names. The site resolves teams from a 14-character truncated name and cannot know who played.
- The canonical root DOL and the shipped target image (`NFL Street 2 FIELDS 12-19.iso`) both carry the per-player stat hook at `0x8005A960`. The retired 2.1 `NSR.iso` and `FR4N31.iso` do not. The prerequisite for per-player stats therefore holds for the shipping image and for nothing else.

## 3. System shape

| Part | Owns | Never does |
|---|---|---|
| NFL Street Reborn Dolphin | Captures every game to `User\GameStats\Online\` or `Offline\` as `nsr.game.v1` plus one additive `reborn_match` block. | Any network call. Any marker file. |
| Launcher | Discord sign-in through the system browser, token in the Windows vault. Records the league-match intent and the session id before Dolphin starts. Ingests captures and sends automatically after every session. Shows sign-in state and last result. | Derive a destination from a capture's contents. Hold a Discord secret. |
| Reborn backend | Discord OAuth code exchange and its own device tokens. Accepts `nsr.submission.v1`, dedupes by canonical hash, stores every game once with its participants, keeps the career record. Forwards league-intent games to the league site server to server. | Trust a client-supplied identity or league id. Send one capture to two destinations. |
| League site | Unchanged importer, review inbox, leaderboards. Gains one endpoint that receives league games from the backend with both players' Discord ids and resolves club, fixture and slot exactly. | Accept player uploads directly, once the backend is live. |
| Discord bot | Unchanged in phase 1. | |

Everything is one destination from the client. The launcher posts to the backend only; the backend routes internally.

### 3.1 Flow for one game

1. Dolphin writes the capture file at game end into `Online\` or `Offline\`.
2. Dolphin exits. The launcher's session path moves every capture from both folders into its queue, tagged with the session id and the folder it came from.
3. If a token is stored, the launcher wraps each queued capture as a submission and posts it. If not, entries wait for sign-in.
4. The backend verifies the token, recomputes the hash, and either creates the game or attaches this player as another participant to a game the opponent already sent.
5. If the session recorded league intent, the backend forwards the game and both participants' Discord ids to the league site and records the answer.
6. The backend replies accepted, duplicate, held or incomplete. The launcher closes the entry in its ledger and shows the outcome.

## 4. Identity

### 4.1 Sign-in flow (device code)

Nothing listens on the player's machine. The launcher never receives a Discord token.

1. Launcher: `POST /v1/auth/device` with `{ "label": "<machine name>" }`. Reply: `{ device_code, user_code, verify_url, expires_in: 600, interval: 3 }`.
2. Launcher opens `verify_url` (which carries the user code) in the system browser through the platform process host, the same way the folder button opens Explorer. The user code is also shown in the launcher in case the browser did not open.
3. Browser: the verify page shows the code, names the device that asked ("This signs in the launcher on ADAM-PC"), and offers a "Continue with Discord" button. The form is protected against cross-site submission (a same-site cookie paired with a hidden field, plus a same-origin check), so nobody can approve a code on a player's behalf from another page. Clicking redirects to Discord OAuth with scope `identify` only and the redirect URI `https://api.nflstreetreborn.com/v1/auth/discord/callback`, never with `prompt=none`, so Discord's own consent screen stays in the path. A random nonce bound to the device login travels in the OAuth `state` parameter, signed by the backend; the device code itself never reaches the browser.
4. Backend callback: exchanges the code for a Discord access token server side, fetches `/users/@me`, upserts the `player` row by Discord user id, binds the device login to that player in one guarded update (pending and unexpired, or 410), discards the Discord token, and shows "You can close this tab." Everything rendered from user or Discord input is HTML-escaped.
5. Launcher polls `POST /v1/auth/device/poll` with the device code every 3 seconds. Replies: `428` still pending, `410` expired, `200 { token, player: { id, display_name, avatar_hash } }` once. A second poll after the 200 gets `410`.
6. Launcher stores the token in the vault under account `nsr.reborn.player` and the non-secret player facts in `config\identity.json`.

The prototype's `routes/auth.js` design is reused: `identify` scope, server-side exchange, own token. Its localhost callback is replaced by polling, and no prototype file is copied.

### 4.2 Tokens

Opaque, 32 random bytes, base64url on the wire, stored as SHA-256 hex. One per device. Revoked by sign-out, unlink, or delete, effective immediately. No expiry; `last_used_at` is updated on use. They are not JWTs because revocation has to be immediate and the account actions require it.

### 4.3 Tables

```
player         id uuid pk, discord_user_id text unique (null after unlink or delete), display_name text,
               avatar_hash text, show_offline boolean not null default false,
               created_at, last_seen_at, deleted_at null
device_token   id uuid pk, player_id fk, token_hash char(64) unique, label text,
               created_at, last_used_at, revoked_at null
device_login   device_code char(43) pk, user_code char(9), player_id null,
               status text (pending | approved | claimed | expired),
               created_at, expires_at
```

### 4.4 Endpoints

| Method and path | Auth | Purpose |
|---|---|---|
| `POST /v1/auth/device` | none | Start a device sign-in |
| `GET /link?code=` | none | Browser verify page |
| `GET /v1/auth/discord/callback` | none | Discord redirect target |
| `POST /v1/auth/device/poll` | none | Launcher polls for the token |
| `GET /v1/me` | token | Who am I, plus `show_offline` |
| `PATCH /v1/me` | token | Set `show_offline` |
| `DELETE /v1/auth/device` | token | Sign out this device |
| `POST /v1/me/unlink` | token | Remove the Discord binding, keep history under a tombstone |
| `DELETE /v1/me` | token | Delete per section 6.4 of the protocol doc: shared rows keep a tombstone id, the player's own envelopes are deleted, a held game becomes void |
| `GET /v1/me/export` | token | Everything keyed to the player, own envelopes only |

### 4.5 Launcher UI

- Play page: a sign-in card. Signed out: "Sign in with Discord" and, once started, "Waiting for Discord" with the user code. Signed in: "Signed in as Name" and the last result line.
- Settings: "Signed in as Name" and Sign out. The "League token" block, `stats_credential_set` and `stats_credential_forget` are removed.
- Nothing is ever typed.

### 4.6 Signed-out play

Games are captured and queued as usual. Entries stay `queued`; the submit pass reports "waiting for sign-in" and the Play page says so. After sign-in the backlog sends and is attributed to that account.

## 5. The capture, as Dolphin writes it

### 5.1 Folder

`<User>\GameStats\Online\` when `NetPlay::IsNetPlayRunning()` is true at write time, else `<User>\GameStats\Offline\`. Both are created on demand. `LeagueGameStats` is no longer written.

### 5.2 The additive block

`nsr.game.v1` keeps its schema string, every existing key, and the hashed set unchanged. One new top-level key is added after `per_player_matched`:

```json
"reborn_match": {
  "schema": "nsr.reborn_match.v1",
  "netplay_running": true,
  "game_id": "GN7E69",
  "runtime_version": "1.1.0",
  "local_pads": [ { "pad": 0, "registry_team": 1 } ],
  "local_team_index": 1
}
```

- `local_pads` lists every controller port this machine controls that took part in the match, with the team the play-call registry recorded for it: `u8[ *(*(0x803EAB60)) + pad*0xC ]`, read at play-call state (`*(*(0x803EAAD4)) == 5`) and latched for the match, exactly as `NflsPlayHide` reads it today. Under NetPlay a pad is local when `NetPlay_InGamePadToLocalPad(pad) < 4`. Offline, a pad is local when its registry record reads 0 or 1. Four entries are possible; two humans on one machine in a 2v2 is valid.
- `local_team_index` is the `teams[]` index of the lowest-numbered local pad's team. It is **omitted** until the mapping from registry team to `teams[]` index has been measured (section 11, step 6). Consumers treat absence as unknown, never as 0.
- `runtime_version` is a compiled constant in `Common/Version.cpp`, set at build time to the runtime component version.

The block lives outside the hashed set, so both NetPlay clients still hash identically while disagreeing about their own seats.

### 5.3 Gates and removals

- The netplay-only gate is removed. Capture is always on for the three known game ids.
- `NflsStatUpload.{h,cpp}` are deleted, with their CMake and `DolphinLib.props` lines and every call from the tracker. `MarkFinal`, `Nudge` and `Start` go with them. Finality is implicit: the launcher ingests after Dolphin exits.
- The four marker files (`nsr_track_offline.txt`, `nsr_debug.txt`, `playhide_test.txt`, `playhide_debug.txt`) become compile-time flags (`NFLS_DEBUG_OSD`, `NFLS_PLAYHIDE_TEST`), off in shipping builds.
- The debug OSD stays available behind the flag; nothing is lost for diagnosis.

## 6. The submission envelope

`nsr.submission.v1`, built by the launcher:

```json
{
  "schema": "nsr.submission.v1",
  "capture": { "...the full nsr.game.v1 object including reborn_match..." },
  "capture_hash": "<sha256 hex, canonical recipe>",
  "capture_folder": "online",
  "session": {
    "session_id": "1788401069335-offline",
    "launcher_version": "3.1.0",
    "runtime_version": "1.1.0",
    "exit": "clean",
    "exit_code": 0
  },
  "intent": { "league": false },
  "submitted_at": "2026-09-08T21:14:03Z"
}
```

`match_id`, `seat` and `nonce` from the protocol doc's envelope are reserved for matchmaking and absent in v1.

### 6.1 The canonical hash

The recipe is `si_capture_hash()` in `league/_inc/stat_import.php`, reproduced exactly: a JSON object with keys in this order, `teams` (list of `{name: trimmed string, score: int}`), `style` (list of ints), `team_totals` (list of maps with keys sorted, values int; an absent block is an empty list), `players` (sorted by `digp`, each `{digp: int, name: trimmed string, side: int, stats: map with keys sorted, values int}`), serialized with no whitespace, slashes and non-ASCII unescaped, then SHA-256.

Both the launcher (`Capture::canonical_bytes`) and the backend must produce the identical hash. Section 10 requires this to be proven byte for byte against the six real captures on disk by running the PHP function beside them. The backend refuses an envelope whose `capture_hash` does not match its own recomputation (`400 hash-mismatch`).

## 7. The backend

Node 20 or later, ESM, Express 5 and `pg`, no ORM, numbered SQL migrations applied at boot, `node:test` for tests. Railway service with the Postgres addon, `DATABASE_URL` injected. Environment: `DISCORD_CLIENT_ID`, `DISCORD_CLIENT_SECRET`, `DISCORD_REDIRECT_URI`, `STATE_SECRET`, `ADMIN_TOKEN`, `LEAGUE_FORWARD_URL`, `LEAGUE_FORWARD_TOKEN`, `PUBLIC_ORIGIN`, plus optional `RATE_LIMIT_PER_MINUTE` (default 60) and `TRUST_PROXY_HOPS` (default 1; 2 when the Cloudflare record is proxied). Body cap 512 KB on results. 60 requests per minute per client address (a caller-controlled header cannot open a new window).

### 7.1 Tables

```
game           id uuid pk, capture_hash char(64) unique not null,
               mode text (online | offline), teams jsonb, style jsonb,
               team_totals jsonb null, players jsonb,
               target_reached boolean, status text (accepted | incomplete | held | void),
               hold_reason text null, flags text[] not null default '{}',
               decided_by text null (null = the server decided; 'admin' = a person; a later
               submission never overwrites an admin decision or a void),
               captured_at timestamptz, first_seen_at, resolved_at null
submission     id uuid pk, game_id fk, player_id fk, envelope jsonb,
               capture_folder text, session jsonb, intent_league boolean,
               received_at; unique (game_id, player_id)
participation  id uuid pk, game_id fk, player_id fk null,
               kind text (human | guest | cpu), pad smallint null,
               registry_team smallint null, team_index smallint null,
               source text (submission | backfill | admin);
               unique (game_id, player_id) where player_id is not null
league_forward id uuid pk, game_id fk, status text (pending | applied | review | duplicate | failed),
               site_import_id int null, reason text null, attempts int not null default 0,
               last_attempt_at null, created_at
```

Envelopes are stored whole and never overwritten.

### 7.2 Accepting a submission

`POST /v1/results`, token required.

1. Parse. Refuse over 512 KB, wrong schema, missing capture, or a hash that does not recompute (`400`).
2. Insert `game` on conflict do nothing, then insert `submission` on conflict do nothing. A conflict on `submission` means this player already sent this game: reply `duplicate`. Idempotency lives in the unique keys, never in a lookup first. This is the July 2026 lesson from `ingest.php`, kept.
3. `mode` is `online` when the folder is `Online` and `reborn_match.netplay_running` is true, `offline` when both say offline. If they disagree the game is held with `online-signals-disagree`.
4. Participations from `reborn_match.local_pads`: the lowest-numbered local pad is this player, kind `human`; further local pads on the same machine are `guest`. For an offline game with one human pad, the other team is a `cpu` participation. `team_index` is copied from `local_team_index` when present, else null.
5. For an online game with two submissions, the two players' registry teams must differ and cover both teams. Otherwise hold with `seat-conflict`. An online game with one submission is complete as a box score and annotated `unpaired-online`; it counts for the sender.
6. Status: `held` for `no-players`, `online-signals-disagree`, `tied-final-score`, `no-seat-claim` (empty `local_pads`), `seat-conflict`. `incomplete` when the higher score is below 36 (`below-target-score`). Otherwise `accepted`.
7. Every plausibility flag the launcher computes (`sides-unverified`, `no-team-totals`, `passer-credited-with-sacks`, and the rest of `codes::ALL` in `stats.rs`) is recomputed and stored in `flags`. Flags annotate; only the codes in step 6 change status.
8. If `intent.league` is true, insert `league_forward` pending. Forwards are attempted by a timer every five minutes and by the admin's forward-run route, never inline in the request; only games whose status is `accepted` are sent, so a held game reaches the league site once a person accepts it.
9. Reply `200 { ok: true, status, game_id, reason }` where status is `accepted`, `duplicate`, `held` or `incomplete`. A revoked token gets `401`.

Wins and losses are credited only when `target_reached` is true and the player's `team_index` is known. Every other game still appears on the record with its box score.

### 7.3 Reading the record

| Method and path | Auth | Returns |
|---|---|---|
| `GET /v1/players/{id}` | none | Display name, avatar, games, wins, losses, incompletes, points for and against, style, per-category totals of the player's team. Online games only unless the player set `show_offline`. |
| `GET /v1/players/{id}/games` | none | Paged list, same visibility rule |
| `GET /v1/games/{id}` | none | Teams, scores, totals, per-player box score, participants by display name, status and reason |
| `GET /v1/games/{id}/submissions` | participant or admin | The stored envelopes |
| `GET /v1/admin/held` | admin | Held games with reasons and both envelopes |
| `POST /v1/admin/games/{id}` | admin | Set status, set a participation's `team_index`, or void |

Offline games are private to the player by default. Online games are public. This keeps the protocol doc's visibility decision while still tracking offline play.

### 7.4 Backfill

Every envelope carries the raw registry teams. When the registry-to-index mapping constant is known, one admin job fills `participation.team_index` for every stored game with `source = backfill`, and wins and losses appear retroactively.

## 8. League forwarding and the league site endpoint

### 8.1 Forward

`POST <LEAGUE_FORWARD_URL>` with `Authorization: Bearer <LEAGUE_FORWARD_TOKEN>` and a `User-Agent`, which the host firewall requires:

```json
{
  "schema": "nsr.league_result.v1",
  "game_id": "<backend uuid>",
  "captured_at": "2026-09-08T21:10:44Z",
  "capture": { "...nsr.game.v1 including reborn_match..." },
  "participants": [
    { "discord_user_id": "1234", "team_index": 0, "kind": "human" },
    { "discord_user_id": "5678", "team_index": 1, "kind": "human" }
  ]
}
```

The site answers with the same shape `ingest.php` uses today: `{ ok: true, status: applied | duplicate | review, import_id, reason }` or `{ ok: false, error }`. `review` is a success; the backend records it and never resends. Transport failures and `5xx` retry with backoff; `4xx` records `failed` with the body.

### 8.2 The endpoint: `league/stats/results_from_backend.php`

New file. Bearer token from a new `_inc/backend_token.php`, separate from the bot's token. Resolution, in order:

1. Each participant's `discord_user_id` maps to `league_owners.discord_user_id`, then `owner_teams` to `(league_id, team_id)` pairs. A participant with no owner row holds the game with `no owner linked to Discord id <id>`, which is the commissioner's one-time link action.
2. Leagues where both participants own a club and an incomplete fixture between those two clubs exists. A fixture is incomplete when `si_next_slot` still returns a free slot, so a finished best-of-3 never absorbs a later casual game. Exactly one fixture applies; zero or several hold with the candidates named.
3. Scores map to clubs by `team_index` when both are known, else by `si_resolve_team` on the captured names within the two resolved clubs only.
4. Series slot from `si_next_slot`. Players resolve with `si_resolve_player` within each club's roster.
5. Apply with `si_apply`, insert `stat_imports` with the forwarded body as payload, status `applied`. Any hold writes `review` with the precise reason. The dashboard inbox keeps working unchanged.
6. Dedupe on `(league_id, capture_hash)` exactly as today, through the `insert_import` helper.

Live schema changes follow the site conventions: check `information_schema` before any `ALTER`, never `IF NOT EXISTS` on `ALTER`, deploy from PowerShell with one `put` per file and read each back.

### 8.3 Closing the old door

`stats/ingest.php` answers `410 Gone` once the launcher update is live. Its token is rotated first (section 11, step 1) because it printed into a transcript on 2026-09-08. The bot keeps using the rotated shared token through `api/bot.php`.

## 9. Launcher changes

### 9.1 Core

- `identity.rs`: start, poll, sign out, status, over the `Http` trait and the `Credentials` vault. Account `nsr.reborn.player`. Non-secret facts in `config\identity.json`.
- `backend.rs` replaces `league.rs`: implements `Submitter`, builds `nsr.submission.v1` from the entry, its session record and the intent flag, posts to `<backend_origin>/v1/results`, and maps replies: accepted, duplicate, held, incomplete are terminal successes; `401` clears the token and returns the entry to waiting; other `4xx` rejects; `5xx` and transport defer.
- `stats.rs`: `DOLPHIN_CAPTURE_DIR` becomes `GameStats`; `ingest_from` scans `Online`, `Offline` and the legacy `LeagueGameStats`; `QueueEntry` gains `capture_folder`; a submit pass with no token reports `waiting_for_sign_in` and changes nothing.
- `session.rs`: `LaunchRequest` gains `league_match: bool`; `SessionRecord` gains `intent: Option<SessionIntent>` written before Dolphin starts; after ingest, `Session::run` submits pending entries when a token exists. The same pass runs at launcher start and after sign-in.
- `settings.rs`: `league_ingest_url` is replaced by `backend_origin`, default `https://api.nflstreetreborn.com`.
- Runtime discovery prefers `NFL Street Reborn Dolphin.exe` over `Dolphin.exe` when both exist. Every `Dolphin.exe` literal in the workspace is updated; a search decides the list at implementation time, and the ones known today are in `session.rs`, `runtime.rs`, `reshade.rs` and `installer/windows/build.ps1`. The packaging audit adds `User/LeagueGameStats` and `User/GameStats` as never-ship paths.
- `ConfigGenerator`: see section 10.3 for the keys added.

### 9.2 Commands and pages

Commands: `identity_status`, `identity_sign_in_start`, `identity_sign_in_poll`, `identity_sign_out`, `identity_set_show_offline`. Removed: `stats_credential_set`, `stats_credential_forget`. `launch` gains `league_match`. `stats_submit` loses its confirm gate and becomes the manual retry.

Pages: the Play page gets the sign-in card, the "This is a league match" toggle (off by default, not remembered), and the last-result line. Settings gets Sign out and the offline visibility switch. "League results" is renamed "Match results" everywhere. The queue view shows each entry's state and the backend's reason verbatim.

Checkers `check-harness-args`, `check-shell-pages`, `check-page-call-args`, `check-onboarding-ready`, `check-mods-page` must pass, with `check-shell-pages` extended for the sign-in card. `cargo test --workspace` and `cargo clippy --workspace --all-targets` must pass. Another agent edits the UI tree concurrently, so commits stage files by name, never `git add -A`.

## 10. Dolphin runtime changes

### 10.1 Rename and brand

| Where | Change |
|---|---|
| `Source/Core/DolphinQt/DolphinQt.vcxproj` `<ProjectName>` | `NFL Street Reborn Dolphin`, so MSBuild emits `NFL Street Reborn Dolphin.exe` |
| `Source/Core/DolphinQt/CMakeLists.txt` `OUTPUT_NAME` (two places) | `NFL Street Reborn Dolphin` |
| `MainWindow.cpp` lines 213 and 1135, `RenderWidget.cpp` line 44 | Title `NFL Street Reborn Dolphin` |
| `Common/Version.cpp` `scm_rev_str` | `NFL Street Reborn Dolphin 1.1.0`. The NetPlay version string derives from it, so old and new builds refuse to connect, which is intended during a forced update |
| `DolphinQt.rc` | Icon points at the Reborn icon; a `VERSIONINFO` block is added with product name, description and version |
| `Data/Sys/GameSettings` | `FR4N31.ini` and a new `CR4E12.ini` are added to the source tree so a rebuild keeps the per-game defaults; today the runtime folder carries `FR4N31.ini` by hand only |

The uncommitted `NflsUiSplit.cpp` change in the fork (287 insertions, 86 deletions) is the source of the shipped exe of 2026-09-04 and is committed as-is before any other change, so the rename build starts from what shipped.

### 10.2 Removals and capture changes

Section 5 in full: uploader deleted, gate removed, marker files to compile flags, `GameStats` folders, `reborn_match` block, `NflsPlayHide` publishes its latched pads and teams through an accessor the tracker reads.

### 10.3 Optimization keys

In the launcher's generated config and in the shipped `Sys\GameSettings\GN7.ini` (and its FR4N31 and CR4E12 twins), so a launcher-run and a hand-run Dolphin match:

| Key | Value | Why |
|---|---|---|
| `[Settings] DumpTextures`, `DumpBaseTextures`, `DumpMipTextures` | `False` | On today in both of Adam's installs; a disk write per new texture and 52 MB of dumps in the dev folder |
| `[Settings] ArbitraryMipmapDetection` | `False` | Per-upload CPU cost that the HD pack makes pure waste |
| `Logger.ini [Options] WriteToFile` | `False` | The freeze instrumentation left it on |
| `Logger.ini [Options] Verbosity` | `1` | Same |
| `[General] UseDiscordPresence` | `False` | Dolphin's own presence races the launcher's future presence on the local IPC socket |

Kept on purpose, unchanged: `SyncGPU = True` (the 0 FPS freeze fix), `CacheHiresTextures = True` (1.5 GB after the BC7 bake), 64 MB MEM1, `InternalResolution = 3` and `MSAA = 1` (the ReShade depth filter lock), `EFBAccessEnable = True` (portraits), `ImmediateXFBEnable = True` (the reference install's choice), `ShaderCompilationMode = 2`, `WaitForShadersBeforeStarting = True`, `MaxAnisotropy = 4`. Player preferences in `dolphin_preferences.rs` are untouched.

The dev folder's `User\Config\Logger.ini` is edited in place to the same values. No `.bak` is restored.

### 10.4 Dev folder retirement

The folder is renamed `NFL Street Reborn Dolphin`. Nothing is deleted. These move to `DOLPHIN ONLINE TEST\_retired_2026-09-08\` keeping their relative paths:

- `Dolphin.exe.bak_2026-08-29_pre_uisplit`
- `NSRShaderlessMenu.addon64.bak_*` (2), `NSRShaderlessMenu.ini.bak_*` (2), `NSRShaderlessMenu.log`
- `ReShade.ini.bak_*`, `ReShade.log`, `ReShade.log1`, `ReShade.log2`
- `ReshadeEffectShaderToggler.addon64.disabled_2026-09-02`
- `reshade-presets\stages.bak_*` (4)
- `Game Summary.html`, `None\`, `play.txt`
- `Games\*.iso` and `Games\*.png` (four images, 5.7 GB)
- `User\Config\*.bak_*`, `User\GameSettings\*.bak_*` (the `.launcher-original` files stay; the launcher wrote them)
- `User\Logs\dolphin.log`, `User\NflsUiSplit.log`, `User\Dump\*`
- `User\LeagueGameStats\` (the ledger and `_previous`)
- `User\Load\Textures\CR4E12`, `CR4E12 - PS`, `FR4N31`, `FR4N31 - PS`, `GN7E69.before-linking.*`, and the controls note
- In the fork: `Source/Core/VideoCommon/NflsUiSplit.cpp.bak_*` (6)

Also: `User\Config\Dolphin.ini` `DefaultISO` changes from the 2.1 `NSR.iso`, which has no stat hook, to `C:\Users\Adam\AppData\Roaming\NFL Street Reborn\Games\NFL Street 2 FIELDS 12-19.iso`. The launcher's `settings.json` is checked for the old folder path and updated. Memory notes that name the old folder are updated after the rename.

### 10.5 Ship path

Build the fork, stage through `build.ps1`'s allowlist with the new exe name, `rebornctl publish component runtime --version 1.1.0`, then a release at the next free sequence (another session published runtime 1.0.2 as sequence 4 on the evening of 2026-09-08, so read `release.json` and use the next one). The launcher update ships first.

## 11. Rollout, in dependency order

The implementation plan is written per repository in this order, so each piece can ship on its own: backend, launcher, league site, Dolphin fork and dev folder.

1. **Adam:** rotate the league ingest token in `league/_inc/ingest_token.php` and in the bot's Railway variable `NSR_API_TOKEN`. Create the Discord OAuth client secret and add the redirect URI on the existing application. Create the Railway project with a Postgres addon. Add the `api` CNAME at Cloudflare pointing at Railway's target.
2. Backend live: identity, results, career record. Forwarding rows wait until step 4.
3. Launcher update on the bucket: sign-in, GameStats ingest, automatic send, rename support. This is the piece that matters for 2026-09-12.
4. League site endpoint live; `ingest.php` answers 410.
5. Runtime 1.1.0 published. Dev folder renamed, default image fixed.
6. Measurement run: two instances on one PC, one NetPlay game, twice with the team picks swapped, each capture's registry team compared with `teams[]`. Constant baked into Dolphin, `local_team_index` emitted, runtime 1.1.1. Backfill job run once.
7. Phase 2 spec.

## 12. Verification

Nothing is called done until it is observed on real output.

- **Hash recipe:** run the site's `si_capture_hash` through the PHP 8.3 CLI on this machine over the six real captures in the launcher queue and `_previous`, and compare with the backend's and the launcher's hashes byte for byte. Add one synthetic capture with no `team_totals`.
- **Backend:** `node:test` suites over a real Postgres (`DATABASE_URL`): device flow with a stubbed Discord exchange, duplicate and pairing rules, every hold code, incomplete rule, forwarding retry. Then one real Discord sign-in against the deployed service.
- **Launcher:** unit tests for the envelope, reply mapping, ingest of both folders plus legacy, exe preference. Checkers green. Then one real session on Adam's machine: launch, one offline game, capture lands in `GameStats\Offline`, entry sends, backend row exists, Play page reports it.
- **Dolphin:** renamed exe boots with the right title and icon and zero popups. One offline game writes `Offline\` with a `reborn_match` block. Two instances play one NetPlay game twice with picks swapped, writing `Online\` with opposite seat claims. That run is step 6.
- **Site:** the new endpoint against the `nsr_local` replica with a real league pairing, then live, then one held capture from the inbox reprocessed through it.

## 13. Phase 2 outline: deterministic capture

Team names, scores and the 15 totals are today found by sweeping the UI heap while the Game Stats screen is up; a fast dismissal loses them and the capture arrives without `team_totals`. Phase 2 adds a second DOL hook at the point where the game builds that screen, copying names, scores and totals into the reserved arena beside the stat ring, so Dolphin reads a fixed address and the sweeps and their per-frame cost go away. It changes the shipped DOL, so it follows the build sync law (root DOL only, rebuild, `check_build_sync.py`) and ships as a game patch. The registry constant from step 6 is baked in the same build. It gets its own spec.

## 14. Recorded decisions and open items

Recorded: Railway Postgres as the store. Offline games private by default with a per-player switch. Unlink and delete per the protocol doc's anonymize proposal, a held game voided on delete. Express 5 and `pg`, no ORM. Device-code sign-in, opaque revocable tokens. `api.nflstreetreborn.com` as the backend host.

Open, carried from the protocol doc and not closed here: retention periods; Elo policy; region and ping matchmaking; traversal ownership; exit-code classification for crash and desync rows. None of them blocks phase 1.

## 15. Constants referenced

| Name | Value | Source |
|---|---|---|
| Stat hook site | `0x8005A960`, `b 0x803FE4E0` when present | `NflsStatTracker.cpp`, verified in the root DOL and FIELDS 12-19 image on 2026-09-08 |
| Play-call registry header | `*(0x803EAB60)`, team at `rec + pad*0xC` | `NflsPlayHide.cpp` |
| Substate pointer | `*(0x803EAAD4)`, 5 at play call | `NflsPlayHide.cpp` |
| Target score | 36, a floor, never a winner rule | `stat_import.php`, `stats.rs` |
| Payload cap | 512 KB | `ingest.php`, `stats.rs` |
| Team name truncation | 14 characters | `stat_import.php` |
