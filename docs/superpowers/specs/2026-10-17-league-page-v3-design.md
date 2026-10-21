# League page v3: one look, real league data

Approved mockup: `.superpowers/brainstorm/42797-1792231954/content/league-f-v5.html` (layout "F"), built with Adam on 2026-10-17. This replaces the League page on `feat/league-tab-3041` (`crates/reborn-ui/ui/league.js`, `league-model.js`, `pages/league.css`).

## Why

Adam opened the League tab in the rehearsal and said it should "look more like the rest of the launcher". He wants it to drop the "blatant AI stuff", stop saying "This week" everywhere, and lose the roster at the bottom: "we just need to be better". What he actually goes to the League tab for: "I just want to see who I'm playing and some brief league stuff too". The brief stuff is what he checks on the site: standings, the week's scores, and stats.

## Goals

- Who you play this week, with the button to play them, is the first thing on the page.
- The league at a glance: standings, the week's games, and the week's best performers.
- The same look as Stats and Online: page heading, plain glass cards, theme tokens only, real team logos.
- Anything that is a team, a game, a player or a stat opens its page on the league site.

## Non-goals

- No roster card (removed).
- No season leaders card; replaced by the week's best performers.
- No points-difference column in standings.
- No new navigation, tabs or sub-pages inside the launcher. Depth lives on the site.

## Anti-goals (a failure even if it works)

- Copy that repeats itself: "This week" on every card, "Your game" labels everywhere, explanatory sentences under every control.
- Team-colour washes, split colour bars, coloured row backgrounds or red bars to mark your team.
- Cards that stretch to fill the window and leave empty bands.

## The page (top to bottom)

### 1. Page heading
- Left: **League** (the shared page title, 30px), and under it **Reborn Season 4 · Week 3**. The season label comes from the league answer: `league.name` (the site returns "Reborn Season 4") and `league.week_label`.
- Right: a small card with your team logo (40px), team name, and "2-0 · 2nd" (record · standing as an ordinal), plus **View on site ›**, which opens your team page.

### 2. Matchup banner (one slim card, full width)
- Left: your logo (52px), team name, and "OWNER · record" under it; **VS**; then the opponent the same way.
- A faint blur of both logos behind the card (opacity about .16). This is the only decoration on the page.
- Right: the actions, which depend on the state of your game. Wording, in Adam's mockup:

| State | Right side |
|---|---|
| Before your game | **Play offline**, **Friendly**, **Play Wolves Reign** (primary, red) |
| You opened the room | "Waiting for SNIPER to join" · **Cancel** |
| They opened the room | "SNIPER opened the room" · **Join** (primary) |
| Live | score beside each team, "● LIVE" in place of VS; "Game 2 of 3 · you lead 1-0" · **Back to the game** |
| After the game | series score beside each team, "FINAL" in place of VS; "You won the series 2-1" (green) or "You lost the series 1-2"; **Play offline**, **Friendly** |
| Bye | your logo, "No game this week", "Subway Surfers have a bye"; **Play offline**, **Friendly** |
| No schedule yet | your logo, "No schedule yet"; **Play offline**, **Friendly** |

- The primary button names the opponent: "Play {opponent team name}". The existing logic stays as is: `cardState`, `playGate`, rooms, Cancel and Join (`league-model.js`).
- **My call (not in the mockup):** the League files install/update lives in this banner. When the files are not installed, or an update is required, the primary button becomes **Install league files** / **Update league files** with its progress bar, and Play offline / Friendly are disabled. Once installed, nothing about the files is shown.
- The "League settings" switch for friendlies goes away. Friendly always uses league rules (Hard, GameBreakers off), with a ▾ for a one-off standard game (see "Calls" below). That keeps the banner to three buttons.

### 3. Three columns: Standings · Week N · Week N's Top Performers
- The grid is `1.1fr 1fr 1.1fr` with 16px gaps. All three cards stretch to the tallest, so they end on one line. The page is only as tall as its content; it does not stretch to the window.
- **Standings** (heading links to the site's standings): all 12 teams, with rank, logo, name and W-L. Your team is bold with a 6px red dot after the name. There is no row background and no red edge.
- **Week N** (heading links to the week's schedule; aside "3 of 6 final"): order is finished games first, then your game, then the other unplayed games. Each game shows away over home, each with a logo, a name and the score. The loser of a final is in `--fg-3`. The right column reads **Final** for a finished game, **You** (in `--red-ink`) for yours, and nothing for unplayed games. A live game says **Live** with the live dot. The rows share the card height evenly.
- **Week N's Top Performers** (aside **All stats ›**): top 3 passers, rushers and receivers from that week's finished games. Each row has the team logo, the player name (the leader in bold) and the game line, e.g. "318 yds · 4 TD". Before any game of the week is final, show last week's best under the title "Week N-1's Top Performers". In week 1 with nothing final, the card reads "No games played yet".

### Small windows (under 1100px)
Standings and Week sit side by side, with Week's best across the full width under them (three stat groups in a row). The matchup banner wraps so the actions take the full width, with the primary button stretched.

## Links (all open in the browser through `open_website`)

All of these are under `https://nflstreetreborn.com/league/`, with `league_id` from the answer:

| Click | Opens |
|---|---|
| A team: standings row, banner team, a score row's team, your team card | `team.php?league_id=36&team_id=<id>` |
| A game row | `stats/box.php?league_id=36&game_id=<fixture_id>` (the whole best of 3) |
| A player in Week's best | `stats/player.php?league_id=36&poid=<poid>` |
| Standings heading | `standings.php?league_id=36` |
| Week heading | `schedule.php?league_id=36&week=<week>` |
| All stats › | `stats/leaders.php?league_id=36` |

The site pages need the player to be logged in to the site. Adam (10-17) is fine with a logged-out player seeing the site's login page first. Every link is a real button or link with `data-key` for pad navigation. Hover is `--glass-1` on rows and an underline on headings. The URL is built in the launcher from ids, never taken from the answer, and `open_website` keeps refusing anything outside `https://nflstreetreborn.com/`.

## Data

- **Logos:** `league_logo(teamId)` (built in `0c4617a`), asked once per team per visit.
- **Week's best is new.** `_inc/nsr_team_home.php` (live league site) gains `week_best: {week, groups: [{label: "Passing"|"Rushing"|"Receiving", rows: [{poid, name, team_id, line}]}]}`, top 3 per group, from the league stats tables for the current week's finished fixtures, falling back to the previous week. The backend passes it through unchanged (`mergeLeague` spreads the site answer). Site deploys need Adam's OK.
- **Standing** for the team card comes from `team.standing` (already in the answer). The ordinal ("2nd") is formatted in the launcher.
- The existing `leaders`, `recent` and `roster` fields are no longer drawn. The site can keep sending them.

## Look rules

- Tokens only (`theme.css`). The only new value: if the small red dot needs one, it is `--red` at 6px, an inline mark, never a glow.
- Copy rules: no "This week", no "Your game" outside the You marker, no helper sentences under buttons except refusals and the install hint.

## Calls made after the builder check (mine, for Adam to check)

- **Catch-up games:** when `matchup.week` is earlier than `league.week`, the banner shows that fixture, with a small "Week 2 game" over VS. The heading keeps the league's week. The **You** marker goes only on the banner's fixture, and only when it appears in the Week card, so a different unplayed game of yours in that week has no marker.
- **The other banner states:**

  | State | What it shows |
  |---|---|
  | Between games (series 1-0, no room) | Series score beside each team, and "Game 2 next · you lead 1-0" beside **Play {opponent}** |
  | Lobby (both in the room) | "SNIPER is in the room" with **Open room** (primary) |
  | Forfeit final | "FINAL" and "You won by forfeit" or "You lost by forfeit" |
  | Opponent not linked | **Play {opponent}** disabled, with one line: "SNIPER hasn't linked Discord on the league site" |
  | Stale data | Room buttons hidden, and a single amber line "Can't reach the league right now" above the banner (the existing `.partial`) |
- **League files in every state:** when the files are missing or need an update, an **Install / Update league files** button (primary, with progress) replaces the right side in *every* state, except Live and You opened the room, where the game is already running from the installed files. A room refused for a version mismatch shows the existing sentence plus that button.
- **Controller setup:** the existing amber `.partial` box with **Set up controller** / **Play anyway** appears directly under the banner, as today, only after Play offline asks for it.
- **Week's best data:** rows are series totals across the best of 3 (passing yds and TD, rushing yds and TD, receiving yds and TD), counted from every finished game, even inside a series still being played. Ranked by yards. Until the site sends `week_best` (the launcher may ship first), the card reads "No games played yet" in the same place.
- **Week card scores:** the summed points (`score`), as the site's week view shows them. Series wins appear only in the banner.
- **Friendly (Adam, 10-17):** "always default to the league rules", then "dont let us cycle through game rules". **Friendly** always hosts on league rules, with no menu and no other choice. The `league-friendly-league-settings` key and the old switch are removed.

## Left to the builder

- Exact pad navigation order (suggested: banner actions, standings, week, best).
- Whether the live score in the banner reuses `live-common.js` formatting.
- Test fixtures: extend `Tools/fixtures/league-home.json` with `week_best`, and replace the old League states in `Tools/lib/page-states.mjs` with the new ones (each banner state, a small window).
