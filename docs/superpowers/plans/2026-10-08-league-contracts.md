# League Tab — Shared Contracts (binding for Plans 2, 3a, 3b, 4)

Spec: `docs/superpowers/specs/2026-10-08-league-tab-design.md` (v2). Plan 1 (site) is DONE and LIVE; its JSON contract is the top of `2026-10-08-league-tab-1-site.md` (final, incl. `schedule_published`, `matchup.forfeit`, nullable `opponent.discord_user_id`, `winner_team_id` = leader mid-series → final only when `status=="final"`).

## Names
- Game ID constant (launcher): `LEAGUE_GAME_ID`, placeholder value `"LGXXXX"`. A release-build test fails while it equals the placeholder.
- Components: `league-game` (league ISO + league texture folder), `league-content` (league seed file `league.s01`).
- Edition values: `"retail"` | `"league"`.
- Ruleset values on rooms/matches: `"casual"` (existing), `"ranked"` (existing, retail `ranked.s01`), `"league"` (league seed `league.s01`), `"raw"` (no seed).

## Plan 4 → Plans 2/3b: manifest
- Components in the signed manifest gain `"members_only": true|false` (absent = false).
- For members-only components the manifest lists files `{path, sha256, size}` but NO public URL; blobs are stored under a non-listed prefix.
- Publisher: `publish league-game --iso <path> --textures <dir> --notes-file <md>` and `publish league-content --seed <league.s01> --notes-file <md>`.

## Plan 2 (backend) → Plans 3a/3b
- Backend env `LEAGUE_ID` (36) passed to site `team_home` as `league_id`; site falls back when absent. (Site change: accept optional `league_id` in `team_home` — small follow-up owned by Plan 2's first task, deployed under the existing site-action approval.)
- `GET /v1/league/me` — Plan 1 payload + live merge (unchanged from old Plan 2) + `edition_versions: {"league-game": "<latest version>", "league-content": "<latest>"}`.
- `POST /v1/league/download` body `{component, version}` → 200 `{files:[{path, url, sha256, size}], expires_at}` (URLs short-lived, signed) | 403 `not_member` | 404 `unknown_version` | 503 `league_unavailable`.
- `POST /v1/league/room` (league game, unchanged shape) — room/match carry `edition:"league"`, `ruleset:"league"`, `best_of:3`, `league:{league_id,fixture_id,week}`, `league_game_version`.
- `POST /v1/league/friendly` body `{ruleset:"league"|"raw"}` → 200 `{action:"created", room}` — private room `edition:"league"`, not fixture-tagged, best_of 1, `league_game_version` = caller's installed version (body may carry `league_game_version`).
- Joining any room: existing `POST /v1/mm/rooms/:code/join {}`. For `edition:"league"` rooms the backend refuses non-members (same 404 `No such lobby.`), and refuses a joiner whose reported `league_game_version` differs with 409 `league_version_mismatch` + `{required}`.
- Cancel/leave: existing `POST /v1/mm/rooms/:id/leave`.

## Plan 3b (launcher League edition) → Plan 3a (League tab UI)
Tauri commands (reborn-ui), all async, errors as fixed sentences:
- `league_edition_status()` → `{installed: bool, version: string|null, latest: string|null, update_available: bool, size_bytes: number, game_id: string}`
- `league_edition_install()` → streams progress events `league-install-progress` `{phase:"download"|"verify"|"compose", done, total}`; resolves `{version}`.
- `league_play_offline()` → boots league ISO raw.
- `league_host_friendly(ruleset: "league"|"raw")` → creates friendly room via backend and opens it in the existing room flow.
- `league_room_open()` (league game; from old Plan 3) and the existing `mm_room_join(code)` join path; the room launch path picks the seed from `ruleset` (`league` → `league.s01` from `league-content`, `ranked` → `ranked.s01`, `raw` → none) and the ISO/game ID from `edition`.
- Uploads for league matches set `intent.league=true` + `intent.fixture_id`.

## Rulings (controller, after Plan 3a draft)
- Every league-edition room call carries the caller's installed version in the body: `POST /v1/league/room {league_game_version}`, `POST /v1/league/friendly {ruleset, league_game_version}`, `POST /v1/mm/rooms/:code/join {league_game_version}` (field ignored for retail rooms). Backend compares against the room's `league_game_version`.
- Backend version refusal: 409 `{error:"league_version_mismatch", required:"<version>"}`. The launcher's join/open mapping (Plan 3b) turns it into `{kind:"league_update_required", required}`; Plan 3a matches on that kind.
- `league_room_open()` resolves `{action:"created"|"join"|"in_series", room?, code?, match_id?}` (unchanged from the old plan).
- `league_host_friendly` and `league_room_open` do NOT navigate; the League tab page navigates to `online.html` itself.

## Rulings (controller, after Plans 2 and 4 drafts)
- Storage: private R2 bucket `reborn-league`, keys `blobs/by-hash/<xx>/<sha256>`, no public domain; backend presigns SigV4 GET, 6 h, env `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, `LEAGUE_BLOB_BUCKET` (names per Plan 2).
- Manifest file entries are `{path, bytes, sha256}` (existing schema). The download API keeps `size` in its response; Plan 2 maps `bytes` -> `size`.
- `league-game` tree: `league.json` (`{"schema":"nsr.league-game.v1","game_id","iso","textures"}`), `game/league.iso`, `textures/<GAME_ID>/...`; installed at `library/league/`. `league-content`: `seed/league.s01`, installed at `library/league-content/`. Plan 3b checks `league.json.game_id == LEAGUE_GAME_ID` and composes from `library/league/textures/<ID>`.
- `ComponentId` names `LeagueGame`/`LeagueContent` and install roots must be identical in Plan 4 (deck/publisher line) and Plan 3b (3.0.35 launcher line); merge conflict expected and resolved in favour of identical definitions.
- Publish CLI adds `--origin` and `--version` like every other publish command.
