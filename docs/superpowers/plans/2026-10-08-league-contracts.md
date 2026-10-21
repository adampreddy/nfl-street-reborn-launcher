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
- `league_edition_status()` → `{installed: bool, version: string|null, latest: string|null, update_available: bool, size_bytes: number, game_id: string, installing: bool, progress: <last progress payload>|null}` (network release check). `league_edition_installed()` → the same shape from disk only (`latest` null, `update_available` false); the League page polls this one and calls `league_edition_status` on load, after an install and after a version refusal (3a final fix M2/M4).
- `league_edition_install()` → streams progress events `league-install-progress` `{phase:"download"|"verify"|"compose", done, total, checking?: true}` (`checking` only on the first verify, before anything downloads); resolves `{version}`.
- Membership (3a final fix C1): the core refreshes `league.json` on startup and after sign-in, on its own thread, and emits `league-membership` (bool) on a fresh answer; shell.js adds the League link on `true`.
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

## Rulings (controller, after Plan 2 final review)
- If the joiner's league-game version is NEWER than the room's `required`, Plan 3a shows "Your opponent needs to update their league files" (not "Update league files"). Compare versions client-side: `league_update_required.required < installed` -> opponent message.
- Join body `league_game_version` is REQUIRED for edition:"league" rooms even when the room's version is null; missing -> 409 league_version_mismatch with `required: null`.

## Rulings (Adam, 10-09)
- No rematch after a ranked match or a league fixture match. Backend refuses the rematch route with 409; the launcher (Plan 3b) hides the Rematch button for these matches. Friendlies and casual/room matches keep rematch.
- League live scores show on every member's scoreboard regardless of hidden presence (confirmed).

## Ruling (Adam, 10-09): league game ID = `NSRS04`
- `LEAGUE_GAME_ID = "NSRS04"` (replaces the `LGXXXX` placeholder in Plan 3b; the stager keeps refusing `LGXXXX`).
- Dolphin takes a GameCube disc's REGION from the header at 0x458, not from the ID; the 4th ID char (`S`) only sets the country label. The league ISO must keep region NTSC-U (value 1) at 0x458 — the stager will refuse other regions.
- Paths: textures `Load/Textures/NSRS04/`, per-game settings `GameSettings/NSRS04.ini` (+ 3-char `NSR.ini` layer). Retail's `Sys/GameSettings/GN7.ini` fixes do NOT apply to NSRS04 — Plan 3b must mirror them.

## Ruling (controller, Plan 3b Task 8 review)
- `league_play_offline({play_anyway?: bool})` — default false. When the controller check wants setup, it rejects with `{kind:"controller_setup", message}`; the League page offers "Set up controller" (Controllers page) and "Play anyway" (calls again with `play_anyway:true`). Mirrors the Play page.
- `league_edition_install()` refuses before downloading when the data drive lacks space: `{kind:"no_space", message}` (message names the GB needed); a StorageFull during download maps to the same kind.

## Rulings (Adam, 10-10)
- Pre-game holds free the fixture: `league-version-mismatch`, `seed-mismatch`, `content-mismatch`, `content-stale` join PREGAME_HOLD_REASONS (still only when nothing was played/reported/decided).
- League stats = A: only league FIXTURE games upload (tagged `intent.league=true` + `fixture_id`, edition league). They are forwarded to the league site but NEVER appear on public profiles, public stats pages, leaderboards, live-feed history or Elo. League offline games and league friendlies do not upload at all (launcher skips the upload for edition league without a fixture).

## Ruling (Adam, 10-16): the league ships only the textures it changes
- The league keeps its own disc and game ID (`NSRS04`). What changes is the texture download: members already have the retail pack, so a second ~10 GB copy is not shipped ("idk if we want to download 10 more GB for people").
- Launcher: `Load/Textures/NSRS04` is composed as the retail pack (`mods::base_root_for` = retail `pristine_root(GN7E69)`), then the league tree from `library/league/textures/NSRS04`, then the shared mod list. All hardlinks, no extra disk.
- A league texture replaces every retail file with the same texture NAME (file name, case folded), wherever either sits: Dolphin keys a pack by file name and keeps one of two duplicates at random.
- League textures win over mods ("league textures win but they wont overlap"): a mod file with a league texture's name is left out. A mod over a retail texture still applies.
- The league tree's identity is `<league-game ver#seq>+textures <retail ver#seq>`, so a retail texture update rebuilds it too. The composed record carries `base_root`; a different base rebuilds.
- Publisher: `publish league-game --textures` may be a whole working copy. A file byte-identical (sha256) to a retail file of the same name in the textures component the origin's release pins is left out; the run reports how many. A tree identical to retail is refused. With no textures pinned in the origin, everything ships (reported).
- `league.json` stays `nsr.league-game.v1` and gains `retail_textures: "<version>"`, for the record only. The launcher lays the league files over whichever retail pack the player has.
