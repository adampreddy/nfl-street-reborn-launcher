# Stats page v2 and the public player profile: design

Date: 2026-09-30. Designed with Adam, through 17 mockups in the visual companion. The approved mockup is `stats-layout-v17.html`, with two changes: simpler totals and Pickup in By team. The mockups are in `.superpowers/brainstorm/181729-1790747853/content/`. Repos: the launcher (this repo, branch `release/3.0.27-beta-pbp`) and the backend (`reborn-backend-pbp`, branch `feat/play-by-play`).

Builds on `2026-09-26-stats-everywhere-and-stats-tab-design.md`, which shipped in beta 3.0.27-beta.13 and backend 5bd7a73.

## Purpose

- The launcher's **Stats page** becomes the player's private dashboard: Elo, record, their games (online and offline), their best players by role, recent form and head to head.
- A new **public profile**, viewable by anyone in the launcher and on the web, is the full picture of a player's **online** career. It is readable like a profile on a website or social app.
- The page must use every stat the game records, and everything on it must be reachable.

What Adam said:
- "B v1 was peak". The look is the first two-column mockup: launcher art backdrop, glass panels, our red, Inter at normal weights.
- "Mix of the two": RealSports' clarity (logos everywhere, headshot circles with team badges, numbers over caps labels) without copying its look.
- "Your players should show best passer, rusher, receiver, defender DL/LB, ballhawk with most ints".
- "Elo and stuff in the main part" of the profile, with no touchdown or opponent counts.
- "Offline stats aren't viewable on the main page/page you share".
- "Sum totals look ass, can be way simpler". "Make sure pickup shows in by team".
- "Make sure everything works properly and looks proper".

## Look (binding)

- Follow the launcher's `crates/reborn-ui/ui/DESIGN.md` tokens:
  - Surfaces: `--bg`, `--panel`, `--line`.
  - Colours: `--red` for fills, `--red-ink` for red text, `--teal` for data and positive values.
  - Text: Inter only. Page title 28/700, section 15/600, body 13/400, labels 11/600 uppercase with .08em tracking. Numbers use `tabular-nums` at 600–700. No Anton on this page.
  - Radii: 8px standard and 12px for panels. Spacing on the 4px scale.
- Panels are glass (`backdrop-filter: blur(24px)` over translucent `--panel`) on the launcher's art backdrop, the same as B v1.
- Every player name has their team logo (16px) or a round headshot with a team-logo badge (38px). Every team name has its logo.
- Tables have compact rows (36–42px), caps column headers, and no borders except hairlines between rows.
- **Totals:** a single muted line under the table (for example `Total  212/371 · 3,108 yds · 41 TD · 18 INT · 104.2`). It has no banded background and no bold row.
- Wording:
  - "Head to head", never "Rivals".
  - "Recent form", "Most played", "Best passer / rusher / receiver / DL-LB", "Ballhawk".
  - Offline games are labelled `Offline`.
- Accessibility:
  - Every control is reachable by keyboard and controller, and focus is always visible.
  - Contrast follows DESIGN.md (red is a fill; red text uses `--red-ink`).
  - No number appears without its label.
  - Images carry alt text or `alt=""` when a text label repeats them.

## 1. Stats page (launcher, private: the signed-in player only)

The layout is two columns, as in B v1.

**Left column**
- **Your games:** newest first, 42px rows. Filters: All / Online / Offline.
  - Each row shows:
    - date and time;
    - your team logo, "vs", the opponent team logo;
    - the opponent (avatar and name, or `CPU`);
    - the score;
    - W / L / T;
    - the Elo change for a ranked game (`+16 Elo`), `Offline` for an offline game, otherwise blank.
  - A row opens the game view (shipped in beta.13). The game view gains ‹ Previous / Next › to step through the listed games.
- **Recent form · last 10:** W/L/T tiles, plus a points-for vs points-allowed line.
- **Head to head:** one row per opponent, with record and a win-share bar.

**Right column**
- **You:** one module.
  - Top: Elo (the ranked rating), the change from the last ranked game, ladder position (`#7 on the ladder`) and a small Elo trend line.
  - Under it, one grid: Record, Win %, Streak, Pts/gm, Yds/gm, and Plays as (team logo).
  - A link, `Your profile ›`, opens the public profile.
- **Your players:** six cards (headshot, team badge, name, stat):
  - Best passer (pass yds)
  - Best rusher (rush yds)
  - Best receiver (rec yds)
  - Best DL/LB (tackles + sacks)
  - Ballhawk (INTs)
  - Most played (games)
- Buttons Offense · Defense · Records open the profile's matching section.

The Stats page includes offline games. Every number on it counts **all** the player's games (online and offline). Pickup and Quick game both count. Totals-only games count toward the record only (see Data rules).

## 2. Public profile (launcher and web, anyone): online games only

- **URLs:** `https://api.nflstreetreborn.com/u/<player>`, a web page on the backend beside `/games/:id`, and a native launcher view `profile.html?player=<id>`.
  - The short link `https://nflstreetreborn.com/u/<name>` is one redirect rule on the league site to the backend page. It ships only with Adam's go, like every live-site change.
  - Opened from: an opponent's name anywhere, the live page, the Stats page ("Your profile ›"), and Share profile (which copies the web URL).
- **Header:**
  - A cover whose colours come from the player's Discord avatar (two dominant colours). Fallback: a neutral `--panel-2` gradient when there is no avatar or its colours can't be read.
  - Avatar, display name, `Joined <month year>`, `Plays as <team>` with a team selector, and online status when the player shows it.
  - `Plays as` defaults to the most-played team. The owner can pick another, and the choice is stored on the server.
  - Main strip: **Elo · Ranked # · Record · Win % · Games**.
  - Buttons: Share profile, View on web (launcher only).
- **Sections, in scroll order.** Each has `All ›` to the complete list:
  1. **Elo over time:** the full ranked history, with start, peak and current.
  2. **Recent games:** the last 10 online games in the Stats page row format with Elo change. `All ›` opens the full online game log. Every game opens its game page with ‹ › through the log.
  3. **Best players:** the same six role cards.
  4. **Per game:** total yds, pass/rush split, first downs, turnovers, 1-pt conv %, 2-pt conv %, pts/drive, 3-and-outs, longest drive (plays), lead changes, biggest lead, safeties.
  5. **Career stats:** Passing, Rushing, Receiving, Defense. Every player the owner has used, sortable by any column, with a total line.
     - Passing: C/A, Yds, TD, INT, Air, YAC, Y/A, Rtg, Lng, Sacked
     - Rushing: Att, Yds, Avg, TD, Lng, BTK, Yds after contact
     - Receiving: Rec, Yds, Avg, YAC, TD, Lng, Drops
     - Defense: Tkl, TFL, Sck, INT, PBU, FF, FR, TD
  6. **Best QB → receiver:** combos with Rec, Yds, TD.
  7. **Laterals & pitches:** pitches, laterals, yards after.
  8. **By team:** record, pts/gm and win % per team played as. **Pickup** is a row of its own, for games played with pickup (non-NFL) squads.
  9. **By mode:** two small tables. By queue: Ranked, Casual, Private room. By game type: Pickup, Quick game.
  10. **In wins vs losses:** pts, yds, turnovers.
  11. **Head to head:** every opponent, with record, pts, allowed, last meeting, biggest win and biggest loss.
  12. **Records:** career best single-game total for every stat in section 5, plus longest TD, longest run, most points, biggest win and biggest comeback. Each links to its game.
- **Online games only, enforced on the server.**
  - Every profile aggregate and list query filters to games whose mode is online. Offline games never enter a public number, even when the owner has `show_offline` on.
  - `show_offline` stays for the existing game pages. It no longer affects the profile.
  - The owner's own Stats page is the only place offline games appear.

## 3. Data rules

- Stats come from games with `plays_status = 'verified'`.
- Win/loss records, streaks, head to head and Elo come from all accepted games, including totals-only ones.
- **Side:** "your" stats are the side you played. Use `participation.team_index` (capture order), mapped to play side with `sideTeams` (shipped in 5bd7a73). Opponent stats never count as yours.
- **Players:** a player is identified by the roster PGID (`pid`) plus name. Their team is the team they played for in that game. They appear once per team if they played for several.
- **Elo:** the `rating` table (mode `ranked`) and `rating_event` rows give the current value, the history and each game's delta. Ladder position comes from `ladderFor` (`src/mm/presence.js`).
- **Mode has two parts.**
  - **Queue** (online only): `match.mode` gives Ranked or Casual. An online game with no match row is a Private room.
  - **Game type** (every game): Pickup or Quick game, the choice made in the game's menus. It is not `seed_kind`, because every matchmade game boots the pickup seed today.
  - Game type comes from the capture. The plan's first task confirms which capture field tells them apart (team ids outside the NFL set, pickup crew names, or something else). If none does, it adds a `game_type` field to the fork's capture in the next runtime, and older games show game type `Unknown`, which is left out of By game type.
- **Pickup in By team:** a side whose team is a pickup crew rather than an NFL team counts under one **Pickup** row, not under the crew's name.
- **Per-game team numbers** (first downs, conversions, drives, lead changes, biggest lead) come from the stored plays, using the existing `drives.js` and `situation.js`. Nothing is recomputed from team totals.
- **Aggregation lives on the server.** It is computed per request, with a per-player cache keyed on the newest accepted game. It is never computed from raw plays in the page.

## 4. API (backend)

- `GET /v1/players/:id/profile`: public and online-only. It returns:
  - header: name, avatar, joined, plays_as, elo, rank, record, win%, games, streaks;
  - elo_history;
  - recent_games;
  - best_players;
  - per_game;
  - career tables (passing, rushing, receiving, defense);
  - combos, laterals, by_team, by_mode, wins_vs_losses, head_to_head, records.
  - `?section=<name>&limit&before` pages the long lists (game log, career tables, head to head).
- `GET /v1/players/me/dashboard`: token required. The Stats page's data, **including offline games**: you, games (with elo_delta and offline flag), recent_form, head_to_head, best_players.
- `PUT /v1/players/me/plays-as`: the profile team override.
- The page `GET /u/:player` shares the CSP, versioning and 404 of `/games/:id`. `:player` is the player id; a display-name lookup resolves to the id.

## 5. Launcher

- `stats.js` / `stats.css`: rewritten to section 1. `stats-rows.js` keeps its pure row model and gains the dashboard model.
- `profile.html` / `profile.js`: the native public profile view, reusing `live-game-render.js` pieces for tables. `game.js` gains ‹ Previous / Next ›.
- Commands:
  - `stats_dashboard`
  - `player_profile(player_id, section?, before?)`
  - `set_plays_as(team)`
  - Token handling follows `stats_my_games`.
- `open_website` is allowed for `/u/<id>`.

## 6. Errors and edge cases

- Unreachable, rate limited: keep the last good view, and show one line above it (Live now pattern).
- A new player with 0 games: the profile shows the header and "No online games yet." The Stats page shows its empty state.
- No ranked games: Elo shows `Unranked` and the Elo chart is hidden.
- A player who never used a role (no defense stats, for example): that card and table say "None yet". There is no empty table.
- Hidden or deleted players: `/u/:id` 404s, as game pages do.
- An avatar whose colours can't be read (CORS or no avatar): use the neutral cover.

## 7. Testing

**Backend**
- Aggregation unit tests on fixtures with swapped capture/side order.
- Online-only enforcement: an offline game of the owner is absent from every profile field, even with `show_offline` on.
- Pickup vs Quick game mode.
- Best-player selection per role.
- Records point at the right game.
- Totals equal the sum of the rows.
- Route tests for `/u/:id` (200, 404) and the JSON shapes.

**Launcher**
- Node tests for the dashboard and profile models: row formats, "Offline" label, Elo formatting, empty and unranked states.
- `cargo test` for the commands and their registration.

**Visual**
- Screenshot both pages in the real launcher (signed in as Adam), and compare against `stats-layout-v17.html` before shipping.

## Not in scope

- The live page and the Discord bot as a social feed. Adam wants them to look like RealSports' play feed; that is its own spec next.
- Fantasy-style ratings, chips, performance cards, comments and reactions. Adam declined them.
- Leaderboards across players, beyond the existing ladder link.
