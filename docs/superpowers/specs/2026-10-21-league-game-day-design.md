# League game day: the league game room on the League tab

Approved mockup: `.superpowers/brainstorm/81379-1792559195/content/gameday-v4.html` (frames "Mid-season" and "Week 1, today"). Adam, 2026-10-21: "Build that."

## Why

Clicking "Play Wolves Reign" today sends you to the Online tab, into the same room card a casual game uses. Adam: "the online tab for the league 'Play Wolves Reign' fucking sucks", "it should stay on the league tab", "i want like stat comparisons and such". A league game is an event between two teams. The screen before it should show both lineups and how the teams compare, while the room part works exactly like the launcher's Online room.

## What it is

A view inside the League tab (`league.html`) that replaces the League home while you are in your league game's room. Layout at 1280x800 (top to bottom):

1. **Top line:** `‹ League` on the left (back to the League home; see "Leaving" below), and on the right `Week N · <away> at <home>`.
2. **Board**, three columns: your team (left), the tale of the tape (middle, 312px), the opponent (right).
   - **Team column:** the team logo, the name, then `OWNER · W-L`. Below that, all 7 players in position order (QB, RB, WR, OL, DL, LB, DB, then by name), each row showing:
     - the in-game portrait (40px, rounded 10px, on a gradient of the team colour);
     - the name;
     - `POS · #number`;
     - the season stat line on the far side.
   - The opponent column is mirrored: portrait on the right, stat line on the left.
   - Each column carries a faint wash of its team colour from the logo corner.
   - **Season stat line** (season totals, not per game), by what the player has. Only non-zero parts show, in this order:
     - passing `X yds · N TD · N INT`;
     - rushing `X yds · N TD`;
     - receiving `X yds · N TD`;
     - defence `N sk · N INT · N tkl`.
   - A QB with rushing yards shows passing only; the first group a player has wins.
   - An OL with nothing reads `No stats kept`. Any other player with nothing reads blank.
   - **Week 1 (no games played by either team): every stat line is blank.** Adam: "should just be blank for week 1". No ratings, no overall.
   - **Tale of the tape:** both logos with their records and `VS` between them. Then 8 rows, each with a label, both values, and two bars (left bar in your team's colour, right bar in theirs; the better value white, the other one dimmed):
     - Points;
     - Points allowed;
     - Total yards;
     - Passing yards;
     - Rushing yards;
     - Turnovers;
     - Sacks;
     - Defensive TDs.
   - The values are season totals; the header's small right label reads `Season`.
   - For Points allowed and Turnovers, lower is better.
   - Before either team has played, the card shows the records and `No games yet`, nothing else.
3. **Room row:**
   - **The room card** is the launcher's Online room card in shape and behaviour:
     - title `Your room`, with `League rules · Best of 3` on the right;
     - the big code with `Copy code` / `Copy invite link`;
     - the two members side by side, each with an avatar, name, `you` and `Ready` / `Not ready`;
     - a footer with `Start` (host only; disabled until both are ready, quiet glass while disabled, red when live), the ready button, and `Leave room`.
   - **The chat card** sits beside it: the same chat as Online (log, input, Send).

**No Hold up.** Adam: "no hold up button". Once you press `I'm ready`, the button becomes a green `Ready` with a drawn check, and nothing un-readies you. Leave room is still there.

**Only the league FIXTURE room gets this view.** A fixture room is told apart by the room snapshot's `league` object (`league.fixture_id`, `league.week`); the backend already sets it, and Online already reads it.

*(Builder's call)* A league friendly stays on Online, unchanged. A friendly opponent can be any club, `team_home` only knows your fixture opponent, and Adam's request was about the league game. Play offline is unchanged too.

## Data

### League site: `_inc/nsr_team_home.php` (`team_home`)

Add, without changing anything that exists:

- **`roster[]`:** each player gains:
  - `portrait_url` (the site's own portrait for that player in this league, from `nsr_portrait_fn`, or null);
  - `season` = `{gp, pass_yds, pass_td, int, rush_yds, rush_td, rec_yds, rec_td, tackles, sacks, def_int, def_td}` (season totals from the same stat rows `leaders` uses; zero when none).
- **`team_season`** = `{gp, pts, pts_allowed, yds, pass_yds, rush_yds, turnovers, sacks, def_td}` for your team (completed games only):
  - `gp` = games played;
  - points from the series games;
  - yards and the rest from the stat rows;
  - turnovers = interceptions thrown + fumbles lost, using the site's existing `fumble_lost` key and its existing turnover computation;
  - *(Builder's call)* points = the sum of game scores across completed games, not series won.
- **`matchup.opponent.roster[]`** and **`matchup.opponent.team_season`**: the same shapes, for the opponent.

Before writing it, check the stat keys and how the site's stats pages already compute team yards, turnovers and points allowed. Reuse those helpers so the League tab and the site agree.

### Reborn backend (`src/league`)

- `/v1/league/me` passes the new fields through `mergeLeague` unchanged (the saved stale answer carries them too).
- **Portraits** go through the backend like team logos do:
  - a route `GET /live-assets/league/portrait/<poid>.png`;
  - a source allowlist built from the `portrait_url`s the site's answers contained (as `logoSource` does for logos);
  - the same size cap, PNG check, cache and rate limit as logos.
  - *(Builder's call)* Keying by `poid` is safe because the backend serves one pinned league (`LEAGUE_ID`). The backend's cache is keyed by the source URL, which carries `psxp` and `version_id`, so a pool change gives a new face and never a stale one. The launcher's day-long cache is keyed by `poid` plus a hash of the backend's `ETag`/URL.
- The launcher never fetches the league site directly.

### Launcher

- A command `league_portrait(poid)` mirrors `league_logo(teamId)`: it returns a data: URL, cached for a day in `cache/league-portraits`, with the same 1 MiB cap and PNG check.
- A player without a portrait shows the team logo in the portrait slot.

## Behaviour

- **Opening:**
  - `Play <opponent>` on the League home opens the room (`league_room_open`, as today), then shows this view instead of going to `online.html`.
  - `Join` (the opponent already hosts) joins, then shows this view.
  - Reopening the League tab while your league room is open shows this view directly.
  - A league friendly does not open this view (see above).
- **The room's live state** comes from the same snapshot Online uses (`mm_state`: room, members, ready, chat). Actions go through the same commands (`mm_room_action` ready / start / leave / chat), so the two pages can never disagree about the room. The room card and chat code is shared with Online, not copied. Online keeps working as it does today for every other room. If someone opens Online while a league room is open, Online still shows the room, using the shared code.
- **Start, and the match:**
  - From Start onward (match forming, game running, between games, series over), the League tab shows a compact state in the room row's place. The board stays above it.
  - The state shows the series score and where it stands: `Starting…`, `Game 2 of 3`, and `Final` with the result.
  - The series-over screen's actions go back to the League home. There is no Rematch: the backend already sends `rematch_available: false` after a league fixture (`noRematchAfter`, Adam 10-09), and the launcher hides the button on that flag, as it does today.
  - *(Builder's call)* A held match keeps its current notice, drawn inside the compact match state.
  - Everything the match flow does today (seed, settings, holds, uploads) is unchanged. Only where it is drawn changes.
- **Leaving:**
  - `‹ League` while the room is open asks nothing. It shows the League home, where the matchup banner already shows the room is waiting (the banner's existing Join/Open state), and the room stays open.
  - `Leave room` leaves it, as on Online.
- **Missing data:** if `team_home` has no opponent roster (an older site), the opponent column shows the team header and `Lineup not available`, and the tape shows `No games yet` unless both `team_season`s are present. An older backend that drops the fields gives the same result.
- **Stale league data** (the site is down; the answer carries `stale`): the view still shows, with a small `League site unreachable, showing saved data` line under the top line, as the League home does.

## Constraints

- **Design system:**
  - every colour is a `theme.css` token (`Tools/check-theme-tokens.mjs`);
  - team colours come from the site (`team.color`) and are used only for that team's wash, portrait backdrop and tape bar;
  - no new fonts;
  - Inter, the glass card, and the button classes are the launcher's own.
- **Layout:**
  - one screen at 1280x800 with no page scroll;
  - at 800x560 it stacks (team, team, tape, room, chat) and the page scrolls;
  - no sideways scroll at any size.
- **Controller:** every control is reachable with a pad in a sensible order (room footer first, then chat, then the back link), like the Online room.
- **Copy:** the strings above, word for word. Adam does not want explanatory text.
- **Tests:**
  - league-model tests for the stat line rules, the tape's better-value rule (lower is better for Points allowed and Turnovers), the Week 1 blank state, and position ordering;
  - page states for Tools: Week 1, mid-season, opponent roster missing, stale, match running, series over, and 800x560;
  - checks for no sideways scroll, no Hold up button, and Start disabled until both are ready;
  - a backend test for the portrait allowlist;
  - site tests in `nsr_local/tests/team_home_test.php` for the new fields.
- **Site deploys go live only with Adam's OK.** Nothing about the backend's league env changes as part of this work.

## Not in this

- Ratings (overall and so on): Adam said Week 1 stays blank.
- Per-game averages: totals only.
- Player-to-player matchup rows: the two lineups face each other instead.
- Any change to the League home's layout.

## Left to the builder

- How the shared room and chat code is split out of `online-mm.js`.
- The exact match-state wording beyond `Starting…`, `Game N of 3` and `Final`, provided it stays short.
- The data shape inside `league-model.js`.
