# Launcher Glass Redesign, Plan 4: Stats, Profile and Cleanup

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stats and Profile get the approved frosted-glass design (mockup sections `id="stats"` and `id="profile"`). Every page that is not redesigned (Recovery, game view, live view, Online, onboarding) moves onto `theme.css` + one page file with its layout unchanged. The retired stylesheets are deleted and `DESIGN.md` is rewritten to the spec.

**Architecture:**
- The shared pieces for Stats and Profile live once each in `theme.css` (spec §3.4): the 40px section header, the match list, the table, the portrait, the cover wash, the best-of-3 reveal. One ES module, `match-rows.js`, draws the match list for both pages.
- Formatting stays in pure modules with `node --test` tests: `stats-model.js` (extended) and a new `profile-model.js`. The pages only build DOM with `el()`.
- The non-redesigned pages are moved by tools, not by hand:
  - `Tools/extract-layout-css.mjs` writes each page's layout out of the old sheets (layout kept, colours mapped to theme tokens).
  - `Tools/compare-page-layout.mjs` proves the layout did not move (element boxes before/after, plus screenshots).
  - The "before" pictures of every page are taken in Task 1, before anything changes.
- Unshipped backend fields are optional everywhere: a piece with no data is left out, never faked (spec §8.7).

**Tech Stack:** Tauri 2.11 (no Rust changes in this plan), static HTML/JS/CSS in `crates/reborn-ui/ui` (no bundler, `withGlobalTauri`), Playwright + Edge for browser checks, `node --test` for pure modules.

**Spec:** `docs/superpowers/specs/2026-09-30-launcher-glass-redesign-design.md`.
- Binding sections: §2, §3, §5, §6, §8.6, §8.7, §9, §10, §11, §12 (steps 6–8), §13.
- Visual target: `docs/superpowers/specs/assets/2026-09-30-launcher-glass-mockup.html`, `<main class="page scrollpage" id="stats">` and `id="profile"` (CSS blocks "STATS v5", "PROFILE v6", "profile round 7/9/10", "overview panel").
- Owner rulings (binding): memory `project_reborn_launcher_redesign_2026_09_30.md`, paragraphs "Stats v2 + Profile JOINED" through "Round 10/11".
- Backend status: `C:\Users\Adam\Desktop\Reborn Installer\BACKEND-CHANGES-launcher-redesign.md`. Items §1–§14 are **built but not deployed** (`feat/profile-redesign-backend` @ `7d94a3a`). The deployed server (`ec5027a`) sends none of the new fields.
- This is **plan 4**. Plan 2 (`docs/superpowers/plans/2026-09-30-launcher-glass-play-settings.md`) and plan 3 (`…-mods-music-controllers.md`) are done before it starts. This plan reuses from them: `body.themed`, the shell and hint bar rules, `tabs.js` (`tabs(group, names, current, onPick, {disabled, label})`), `.avatar`, `--avatar-bg`, `--on-art`, `--field-bg`, `@keyframes live-pulse`, `Tools/check-theme.mjs` (its `THEMED` map), `Tools/check-theme-tokens.mjs`, and the check patterns of `Tools/check-glass-pages.mjs`.

**Working copy:** worktree `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher redesign`, branch `feat/launcher-glass-redesign`.
- Never commit to `main` or `release/*`.
- Before overwriting a file, run `git log -3 --format="%h %s" -- <file>`. If a commit you do not know touched it after this plan started, read that diff and carry the fix into your version.

## Global Constraints

- **Colours** (theme only): `--red #E0272E`, `--red-ink #FF7A7F`, `--ok #3DDC97`, `--warn #FFC46B`, `--fg #F4F5F7`, `--fg-2 #AEB5C0`, `--fg-3 #7C8491`; surfaces `--glass-1/2/3`; lines `--stroke`/`--stroke-hi`.
- **W is `--ok`, L is `--red-ink`. There is no T** anywhere (spec §2, §3.1).
- **Page stylesheets hold layout only.** `ui/pages/*.css` and `ui/onboarding.css` contain no hex, `rgb()`/`rgba()`/`hsl()` colour literals and no `font-weight` above 700 (`Tools/check-theme-tokens.mjs`). `color-mix(in srgb, var(--token) N%, transparent)` is allowed.
- **Each page loads exactly** `theme.css` + its page file and has `body.themed` (`Tools/check-theme.mjs`). `game` and `live` share `pages/live.css`; onboarding's file is `onboarding.css` (spec §11 table).
- **Weights:** 500 UI text, 600 values and labels, 700 headline numbers only. Never above 700.
- **Motion:** only `opacity`/`transform` animate, except the spec's explicit `grid-template-rows` for the best-of-3 reveal (§5.1) and plan 2's tab highlight width. Durations come from `--d1/--d2/--d3`. Hover colours change instantly. Reduced motion = 0.
- **No glow.** Focus = `outline: 2px solid #fff; outline-offset: 3px` (theme).
- **Wording (spec §2, §8.6, §8.7, owner rulings):**
  - Never "Best of 3" on a row. The live card says "Game 2 of 3". No halves.
  - "Leaderboard", never "ladder". "Live feed", not "Watch live". "Last 10 games", not "Form". "Your players", not "Best players".
  - Stat units uppercase in tables (YDS, TD, RTG, C/ATT, CAR, YCO, DRP); spelled out in Career tiles (Passing Yards, Passing TD, Passer Rating…).
  - Records are W–L only. Totals are a normal Total row.
  - **No "Private" anywhere on Profile.**
- **Commands are the contract.** No new Rust commands and no Rust changes. The pages use only: `stats_dashboard`, `stats_queue`, `identity_status`, `identity_set_show_offline {show}`, `identity_sign_out`, `stats_submit`, `player_profile {playerId, before?}`, `set_plays_as {tgid}`, `open_website {url}`. Keep call arguments as object literals (`node Tools/check-page-call-args.mjs`).
- **Safety and CSP** (read from `crates/reborn-ui/tauri.conf.json`: `img-src 'self' data: https://api.nflstreetreborn.com/live-assets/ https://cdn.discordapp.com/avatars/`, `script-src 'self'`):
  - Server text only through `el()`'s `text` (textContent), never `innerHTML`.
  - Images only through `assetUrl()` (a `/live-assets/…png` path becomes `https://api.nflstreetreborn.com/live-assets/…`) or `avatarSrc()` (the Discord prefix). The backend's portraits (`…/portraits/<id>-192.png`) and logos (`…/teams/<id>-128.png`) already pass `assetUrl()`, so **the CSP does not change**.
  - Script-set styles only through the CSSOM (`style.setProperty('--tc', …)`, `style.width`), never a `style` attribute. No inline `<script>` in product pages.
- **Keep (spec §10):** matches first (a best of 3 is one row with its series and one Elo change); focus restored after each refresh via `data-key` (`match-<id>`, `game-<id>`, `newer`, `older`); `game.js` arrow keys untouched; the `stats-model`, `stats-rows` and `cover-color` tests keep passing.
- **Tests never touch the network.** `Tools/lib/page-states.mjs` answers `api.nflstreetreborn.com` and `cdn.discordapp.com` with `art/cover.png`, freezes the clock at 2026-09-30 20:00 UTC, and runs in the `UTC` time zone.
- **Controller navigation is first-class.** Everything reachable by D-pad; collapsed rows are `inert`.
- **Logos are Adam's:** never resize, stretch or redraw `art/nsr-sidebar.png` or `art/play-logo.png`.
- **Line endings:** `.rs` files stay LF (this plan touches none). Keep each file's existing line endings.
- **No beta builds, no installer.** The end of the plan is a real-window check of `target/debug/reborn-launcher.exe`.
- **Commits:** one per task, message as given, always ending with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never `git stash` (the stash is shared across worktrees).

### How to run the checks (every task)

Playwright is not installed in this worktree. Link it for the duration of a task, then unlink it (PowerShell, from the worktree root):

```powershell
New-Item -ItemType Junction -Path "Tools\node_modules" -Target "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\Tools\node_modules" | Out-Null
# ... run checks ...
cmd /c rmdir "Tools\node_modules"      # removes the link only. NEVER Remove-Item -Recurse on a junction.
```

**Known failures before this plan (baseline):** `Tools/review-finish.mjs`, `Tools/check-online-mm.mjs`, and `Tools/check-launch-polish.mjs`. Task 4 rewrites the stale Stats block of `check-launch-polish`. After that it may only fail where Task 1's baseline log shows another failure. `node Tools/run-browser-tests.mjs` runs its own main body, then imports later checkers and aborts at `review-finish`. So its **main body must print** `PASS: launcher flows, navigation, onboarding, saves, settings, and motion stability.`, and every other checker is run **individually**.

**"The standing checks"** (run at the end of every task; each must PASS, the last one by its main-body PASS line):

```bash
node --test crates/reborn-ui/ui/stats-model.test.mjs crates/reborn-ui/ui/stats-rows.test.mjs crates/reborn-ui/ui/cover-color.test.mjs
node Tools/check-shell-pages.mjs
node Tools/check-page-call-args.mjs
node Tools/check-theme.mjs
node Tools/check-theme-tokens.mjs
node Tools/check-themed-shell.mjs
node Tools/check-controller-navigation.mjs
node Tools/check-controller-hints.mjs
node Tools/check-session-state.mjs
node Tools/run-browser-tests.mjs target/browser-review
```

A task adds its own new checks to what it runs.

## Review Focus

1. **The deployed server's answers** (no `header.live`, `recent_form`, `career_rank`, `elo.total/peak/around` or team `primary`; rushing `yac`; a private room in `by_queue` and `recent_matches`). Profile draws without errors, hides the live card, the Last 10 column, the rank lines and the Leaderboard, shows YCO from `yac`, and says "Private" nowhere. Tests: Tasks 6 and 7 (`oldProfile()`), Task 4 (`oldDashboard()`).
2. **A 30-second refresh while a best-of-3 row is open and one of its games has focus.** The row stays open, focus returns to the same `game-<id>`, and nothing re-animates. Test: Task 4.
3. **A pad user on Stats.** D-pad Down from a closed best-of-3 row skips its (inert) games; A opens it; Down then lands on Game 1. Mouse movement hides the prompts again. Test: Task 14.
4. **Signed out, refused, or no games.** Stats never shows an empty two-column frame, and Profile & sync stays reachable. Test: Task 4.
5. **A page moved onto the theme loses a rule for a state the fixtures never reach.** The extractor keeps every old rule whose class and id names appear in the page's own HTML and scripts, and the layout diff fails on any sideways change. Tests: Task 8 (self-test) and Tasks 9–11 (diffs).

---

## File structure

| File | Responsibility |
|---|---|
| `Tools/fixtures/stats-fixtures.mjs` (create) | backend-shaped answers: `dashboard()`, `oldDashboard()`, `profile()`, `oldProfile()`, `allMatches()`, `publicMatches()` |
| `Tools/lib/page-states.mjs` (create) | the screens every tool and check opens: `STATES`, `answers()`, `serve()`, `openState()` |
| `Tools/compare-page-layout.mjs` (create) | `capture <page> <dir>` and `diff <page> <before> <after>` |
| `Tools/theme-page.mjs` (create) | puts a page's `<head>` on `theme.css` + its file and adds `body.themed` |
| `crates/reborn-ui/ui/theme.css` (modify) | the Stats/Profile components (§3.4); later, the old variable aliases removed |
| `Tools/check-stat-components.mjs` (create) | those components in a fixture page |
| `crates/reborn-ui/ui/stats-model.js` (+ test) (modify) | `eloChange`, `matchSubline`, `lastTen`, `youNumbers`, `statLine`, `teamColour`; W/L only; "leaderboard" |
| `crates/reborn-ui/ui/match-rows.js` (create) | the one match list: `matchHeader`, `matchRows`, `pager`, `teamLogo`, `avatar`, `storeGameList` |
| `crates/reborn-ui/ui/stats.html`, `stats.js`, `pages/stats.css` | the Stats page |
| `Tools/check-stats-page.mjs` (create) | Stats layout, content and behaviour |
| `crates/reborn-ui/ui/profile-model.js` (+ test) (create) | pure Profile view models |
| `crates/reborn-ui/ui/profile.html`, `profile.js`, `pages/profile.css` | the Profile page |
| `Tools/check-profile-page.mjs` (create) | Profile layout, content and graceful degradation |
| `Tools/lib/colour-tokens.mjs` (+ test), `Tools/extract-layout-css.mjs` (create) | old colours onto tokens; layout out of the old sheets |
| `crates/reborn-ui/ui/pages/recovery.css`, `pages/live.css`, `pages/online.css` (create), `onboarding.css` (regenerate) | generated layout for the pages that are not redesigned (spec §9) |
| `crates/reborn-ui/ui/harness.css` (renamed from `styles.css`) | the developer harness's own sheet (harness untouched, spec §9) |
| retired (deleted) from `ui/`: `launcher.css`, `product-ui.css`, `finish.css`, `shell.css`, `play.css`, `players.css`, `stats.css`, `onboarding-polish.css`, `live-view.css`, `music.css`, `profile.css` (`styles.css` leaves by the rename) | spec §11 |
| `crates/reborn-ui/ui/DESIGN.md` (rewrite) | the spec's tokens and rules |
| `Tools/check-glass-stats-profile.mjs` (create) | §13 screens, motion and controller mode for Stats and Profile |
| `Tools/check-theme.mjs`, `Tools/check-theme-tokens.mjs`, `Tools/check-launch-polish.mjs`, `Tools/check-live-view.mjs`, `Tools/run-browser-tests.mjs` (modify) | expectations that change on purpose |

---

### Task 1: Before pictures, and the tools that take them

Nothing visible changes in this task. It records how every page this plan touches looks **today**, so later tasks can prove what moved.

**Files:**
- Create: `Tools/fixtures/stats-fixtures.mjs`, `Tools/lib/page-states.mjs`, `Tools/compare-page-layout.mjs`, `Tools/theme-page.mjs`

**Interfaces:**
- Produces (used by every later task):
  - `stats-fixtures.mjs`: `ME` (the sample player's id), `allMatches()`, `publicMatches()`, `dashboard({lines = false})`, `oldDashboard()`, `profile({live = true})`, `oldProfile()`.
  - `page-states.mjs`: `root`, `ui`, `GAME_ID`, `STATES` (page → `[{name, url, scope: 'main' | 'page'}]`), `answers()`, `serve() → {base, close}`, `openState(browser, base, state, {viewport, glass, slow, reducedMotion, pad, data}) → {page, errors, context, close}`. In the page, `window.__calls` records every command, and `window.pads[0]` is the fake pad when `pad: true`.
  - `node Tools/compare-page-layout.mjs capture <page> <dir>` and `node Tools/compare-page-layout.mjs diff <page> <beforeDir> <afterDir> [--loose <regex>]…`.
  - `node Tools/theme-page.mjs <page> <stylesheet>`.
  - On disk (not committed; `target/` is git-ignored): `target/p4-before/<page>.json` and screenshots for `stats profile recovery game live online onboarding`, and `target/p4-before/checks.txt` (every checker's exit code at the start of plan 4).

- [ ] **Step 1: Write the fixtures**

Create `Tools/fixtures/stats-fixtures.mjs`:

```js
// Answers shaped like reborn-backend's (feat/profile-redesign-backend @7d94a3a, the redesign's new fields included) for
// the Stats page (`stats_dashboard` plus the launcher's own `signed_in`) and a public profile (`player_profile`).
// Sample data only: the numbers are placeholders, like the mockup's.
// oldDashboard() / oldProfile(): the same answers as the DEPLOYED backend (ec5027a) sends them: no header.live,
// recent_form, career_rank, elo total/peak/around or team colours; rushing `yac`; a private room still listed.
export const ME = '0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d';
const P2 = '1b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e', P3 = '2c3d4e5f-6071-4c8d-9e0f-1a2b3c4d5e6f', P4 = '3d4e5f60-7182-4d9e-8f10-2b3c4d5e6f70';
const avatar = (n) => `https://cdn.discordapp.com/avatars/12345678901234567${n}/0123456789abcdef0123456789abcde${n}.png?size=64`;
const team = (tgid, name, abbrev, primary) => ({ name, abbrev, tgid, logo_url: `/live-assets/teams/${tgid}-128.png`, primary, secondary: '#101010' });
const ATL = team(14, 'Falcons', 'ATL', '#A71930'), PHI = team(13, 'Eagles', 'PHI', '#004C54'), KC = team(9, 'Chiefs', 'KC', '#E31837'), NYG = team(16, 'Giants', 'NYG', '#0B2265');
const PLAYZ = { player_id: P2, display_name: 'Playz', avatar_url: avatar(1) };
const MIKEL = { player_id: P3, display_name: 'Mikel', avatar_url: null };
const JOCK = { player_id: P4, display_name: 'Jock', avatar_url: avatar(2) };
const at = (day, hour = 20, min = 0) => new Date(Date.UTC(2026, 8, day, hour, min)).toISOString();
const clone = (x) => JSON.parse(JSON.stringify(x));
const face = (t) => ({ team: t.name, tgid: t.tgid, logo_url: t.logo_url, primary: t.primary, secondary: t.secondary });

function game(id, when, score, opp, opponent) {
  return { id, at: when, score, result: score[0] > score[1] ? 'win' : 'loss', elo_delta: null, me: ATL, opp, opponent,
    opponent_kind: opponent ? 'human' : 'cpu', game_type: 'normal' };
}
function match(id, games, { queue = 'ranked', mode = 'online', elo = null } = {}) {
  const bo = games.length > 1 ? 3 : 1;
  const won = games.filter((g) => g.result === 'win').length, lost = games.length - won;
  const last = games[games.length - 1];
  const q = mode === 'offline' ? null : queue;
  return { id, match_id: mode === 'online' ? id : null, at: last.at, mode, queue: q, game_type: 'normal', best_of: bo,
    me: last.me, opp: last.opp, opponent: last.opponent, opponent_kind: last.opponent_kind,
    games: games.map((g) => ({ ...g, mode, queue: q })), series: bo > 1 ? [won, lost] : null,
    score: [games.reduce((s, g) => s + g.score[0], 0), games.reduce((s, g) => s + g.score[1], 0)],
    result: bo > 1 ? (won > lost ? 'win' : 'loss') : last.result, elo_delta: elo };
}

/** The owner's whole list, newest first: a ranked best of 3 won 2–1, a casual game, an offline CPU game, a ranked best
 *  of 3 lost 0–2, a private room, then nine casual games (14 matches: two pages of ten). */
export function allMatches() {
  return clone([
    match('m-01', [game('g-011', at(26, 17, 0), [28, 21], PHI, PLAYZ), game('g-012', at(26, 17, 25), [20, 31], PHI, PLAYZ), game('g-013', at(26, 17, 50), [35, 14], PHI, PLAYZ)], { elo: 16 }),
    match('m-02', [game('g-021', at(25, 21), [42, 35], KC, MIKEL)], { queue: 'casual' }),
    match('g:g-031', [game('g-031', at(24, 19), [49, 7], NYG, null)], { mode: 'offline' }),
    match('m-04', [game('g-041', at(23, 20), [14, 28], PHI, JOCK), game('g-042', at(23, 20, 30), [21, 24], PHI, JOCK)], { elo: -18 }),
    match('m-05', [game('g-051', at(22, 18), [31, 30], KC, MIKEL)], { queue: 'private' }),
    ...Array.from({ length: 9 }, (_, i) => match(`m-1${i}`, [game(`g-1${i}`, at(21 - i), i % 3 ? [24, 17] : [10, 27], i % 2 ? PHI : KC, [PLAYZ, MIKEL, JOCK][i % 3])], { queue: 'casual' })),
  ]);
}
/** What a public profile may list: online, never a private room (backend §1). */
export const publicMatches = () => allMatches().filter((m) => m.mode === 'online' && m.queue !== 'private');

const RATINGS = [1000, 1016, 1004, 1030, 1022, 1048, 1070, 1062, 1091, 1120, 1212, 1168, 1184];
const history = () => RATINGS.map((rating, i) => ({ at: at(8 + i), rating, delta: i ? rating - RATINGS[i - 1] : 0, match_id: `m-h${i}` }));
const around = () => [
  { rank: 5, player_id: '4e5f6071-8293-4e0f-9a1b-3c4d5e6f7081', display_name: 'Kay', avatar_url: avatar(4), rating: 1231, games: 30, wins: 19, losses: 11, is_player: false },
  { rank: 6, player_id: '5f607182-93a4-4f1a-8b2c-4d5e6f708192', display_name: 'Lo', avatar_url: null, rating: 1197, games: 22, wins: 13, losses: 9, is_player: false },
  { rank: 7, player_id: ME, display_name: 'adamknowsball', avatar_url: avatar(3), rating: 1184, games: 13, wins: 8, losses: 5, is_player: true },
  { rank: 8, player_id: P2, display_name: 'Playz', avatar_url: avatar(1), rating: 1170, games: 18, wins: 9, losses: 9, is_player: false },
  { rank: 9, player_id: P4, display_name: 'Jock', avatar_url: avatar(2), rating: 1150, games: 14, wins: 7, losses: 7, is_player: false },
];
const elo = () => ({ rating: 1184, rank: 7, total: 214, games: 13, last_delta: 16, peak: 1212, peak_at: at(18), history: history(), around: around() });
const SUMMARY = { games: 23, wins: 14, losses: 9, win_pct: 0.609, streak: 'W2', pts_per_game: 31.4, yds_per_game: 412 };

function best(lines) {
  const b = {
    passer: { key: 'p-penix', name: 'M. Penix Jr.', ...face(ATL), portrait_url: '/live-assets/portraits/221-192.png', value: 3108, unit: 'pass_yds' },
    rusher: { key: 'p-barkley', name: 'S. Barkley', ...face(PHI), portrait_url: '/live-assets/portraits/198-192.png', value: 1715, unit: 'rush_yds' },
    receiver: { key: 'p-london', name: 'D. London', ...face(ATL), portrait_url: '/live-assets/portraits/216-192.png', value: 1714, unit: 'rec_yds' },
    dl_lb: { key: 'p-bolton', name: 'N. Bolton', ...face(KC), portrait_url: '/live-assets/portraits/128-192.png', value: 96, unit: 'tkl_sck' },
    ballhawk: { key: 'p-terrell', name: 'A.J. Terrell', ...face(ATL), portrait_url: '/live-assets/portraits/224-192.png', value: 7, unit: 'int' },
    most_played: { key: 'p-robinson', name: 'B. Robinson', ...face(ATL), portrait_url: '/live-assets/portraits/223-192.png', value: 23, unit: 'games' },
  };
  if (lines) {
    Object.assign(b.passer, { yds: 3108, td: 31, int: 11, rtg: 108.9 });
    Object.assign(b.rusher, { att: 212, yds: 1715, td: 12, avg: 5.6 });
    Object.assign(b.receiver, { rec: 98, yds: 1714, td: 14, avg: 14.3 });
    Object.assign(b.dl_lb, { tkl: 71, sck: 5, tfl: 9, pbu: 6 });
    Object.assign(b.ballhawk, { int: 7, pbu: 14, tkl: 33, td: 1 });
    Object.assign(b.most_played, { games: 23, att: 96, yds: 535, td: 5 });
  }
  return clone(b);
}

function stripColours(x) {
  if (Array.isArray(x)) x.forEach(stripColours);
  else if (x && typeof x === 'object') { delete x.primary; delete x.secondary; Object.values(x).forEach(stripColours); }
}

/** `stats_dashboard`: the launcher adds `signed_in`. `lines: true` = a server that also sends each role's statline. */
export function dashboard({ lines = false } = {}) {
  return { ok: true, signed_in: true,
    you: { player_id: ME, display_name: 'adamknowsball', plays_as: clone(ATL), elo: elo(), summary: { ...SUMMARY } },
    matches: allMatches(),
    recent_form: { results: ['W', 'W', 'L', 'W', 'L', 'W', 'W', 'L', 'W', 'L'], pts_for: 314, pts_against: 250 },
    head_to_head: [], best_players: best(lines) };
}
export function oldDashboard() {
  const d = dashboard();
  for (const k of ['total', 'peak', 'peak_at', 'around']) delete d.you.elo[k];
  stripColours(d);
  return d;
}

const row = (name, t, cols) => ({ key: name, name, ...face(t), games: 20, ...cols });
function career() {
  return {
    passing: { rows: [
      row('M. Penix Jr.', ATL, { cmp: 141, att: 238, yds: 2210, td: 31, int: 11, air: 1402, yac: 808, ypa: 9.3, rtg: 110.5, long: 74, sacked: 6 }),
      row('J. Hurts', PHI, { cmp: 44, att: 81, yds: 561, td: 6, int: 4, air: 330, yac: 231, ypa: 6.9, rtg: 80.3, long: 52, sacked: 3 }),
      ...['A', 'B', 'C', 'D', 'E'].map((x, i) => row(`Backup ${x}`, NYG, { cmp: 5 - i, att: 9, yds: 60 - i * 5, td: 0, int: 0, air: 30, yac: 20, ypa: 6.6, rtg: 70.1, long: 18, sacked: 0 })) ],
      total: { key: 'total', name: 'Total', cmp: 212, att: 371, yds: 3021, td: 40, int: 18, air: 1900, yac: 1121, ypa: 8.1, rtg: 104.2, long: 74, sacked: 11 } },
    rushing: { rows: [
      row('S. Barkley', PHI, { att: 212, yds: 1180, avg: 5.6, td: 12, long: 61, btk: 31, yco: 540 }),
      row('B. Robinson', ATL, { att: 96, yds: 535, avg: 5.6, td: 5, long: 38, btk: 14, yco: 230 }) ],
      total: { key: 'total', name: 'Total', att: 308, yds: 1715, avg: 5.6, td: 17, long: 61, btk: 45, yco: 770 } },
    receiving: { rows: [
      row('D. London', ATL, { rec: 98, yds: 1402, avg: 14.3, yac: 512, td: 14, long: 74, drops: 4 }),
      row('K. Pitts', ATL, { rec: 22, yds: 312, avg: 14.2, yac: 101, td: 3, long: 41, drops: 2 }) ],
      total: { key: 'total', name: 'Total', rec: 120, yds: 1714, avg: 14.3, yac: 613, td: 17, long: 74, drops: 6 } },
    defense: { rows: [
      row('N. Bolton', KC, { tkl: 71, tfl: 9, sck: 5, int: 2, pbu: 6, ff: 2, fr: 1, td: 1 }),
      row('A.J. Terrell', ATL, { tkl: 33, tfl: 2, sck: 0, int: 7, pbu: 14, ff: 0, fr: 1, td: 1 }) ],
      total: { key: 'total', name: 'Total', tkl: 104, tfl: 11, sck: 5, int: 9, pbu: 20, ff: 2, fr: 2, td: 2 } },
  };
}
const RECORDS = () => [
  { key: 'pass_yds', label: 'Passing Yards', value: 288, game_id: 'g-011', at: at(19), name: 'M. Penix Jr.', ...face(ATL) },
  { key: 'rush_yds', label: 'Rushing Yards', value: 171, game_id: 'g-012', at: at(19), name: 'S. Barkley', ...face(PHI) },
  { key: 'rec_yds', label: 'Receiving Yards', value: 164, game_id: 'g-013', at: at(20), name: 'D. London', ...face(ATL) },
  { key: 'tackles', label: 'Tackles', value: 12, game_id: 'g-021', at: at(21), name: 'N. Bolton', ...face(KC) },
  { key: 'longest_td', label: 'Longest TD', value: 99, game_id: 'g-041', at: at(22), name: null, team: null, tgid: null, logo_url: null },
  { key: 'most_points', label: 'Most points', value: 49, game_id: 'g-031', at: at(24), name: null, team: null, tgid: null, logo_url: null },
  { key: 'biggest_win', label: 'Biggest win', value: 42, game_id: 'g-031', at: at(24), name: null, team: null, tgid: null, logo_url: null },
];

/** `player_profile` for ME. `live: false` = online, but not in a match. */
export function profile({ live = true } = {}) {
  const ms = publicMatches();
  return clone({ ok: true,
    header: { player_id: ME, display_name: 'adamknowsball', avatar_url: avatar(3), joined: at(1), plays_as: ATL, plays_as_auto: true, online: true,
      live: live ? { match_id: 'dc5226af-d3b8-4c0b-89e8-1c53b2972a30', queue: 'ranked', best_of: 3, game_number: 2, series: [1, 0], score: [14, 7],
        me: { abbrev: 'ATL', logo_url: ATL.logo_url, primary: ATL.primary }, opp: { abbrev: 'PHI', logo_url: PHI.logo_url, primary: PHI.primary },
        opponent: MIKEL } : null },
    elo: elo(),
    career_rank: { pass_yds: 3, pass_td: 2, pass_rtg: 5, rush_yds: 4, rec_yds: 6, total_td: 2, tkl: 8, sck: 5, def_int: 3, pool: 214 },
    summary: SUMMARY,
    recent_form: { games: 10, wins: 6, losses: 4, avg_margin: 6.4, pts_for_pg: 31.4, pts_against_pg: 25, last_loss_at: at(25) },
    recent_matches: ms.slice(0, 10), matches_total: ms.length,
    best_players: best(false),
    per_game: { games: 24, yds: 412.3, pass: 251.2, rush: 161.1, first_downs: 8.2, turnovers: 1.4, conv1_pct: 0.62, conv2_pct: 0.41, pts_per_drive: 3.1, three_outs: 1.2, longest_drive: 11, lead_changes: 1.6, biggest_lead: 21, safeties: 1 },
    career: career(),
    combos: [{ passer: { name: 'M. Penix Jr.', ...face(ATL) }, receiver: { name: 'D. London', ...face(ATL) }, rec: 61, yds: 902, td: 9 }],
    pitches: { pitches: 41, pitch_plays: 30, pitch_yds: 388 },
    by_team: [{ key: 'tgid:14', name: 'Falcons', abbrev: 'ATL', tgid: 14, logo_url: ATL.logo_url, primary: ATL.primary, secondary: ATL.secondary, games: 19, wins: 12, losses: 7, win_pct: 0.632, pts_per_game: 32.1 },
      { key: 'pickup', name: 'Pickup', abbrev: null, tgid: null, logo_url: null, primary: null, secondary: null, games: 4, wins: 2, losses: 2, win_pct: 0.5, pts_per_game: 27 }],
    by_queue: [{ key: 'ranked', name: 'Ranked', games: 13, wins: 8, losses: 5, win_pct: 0.615, pts_per_game: 30.2 }, { key: 'casual', name: 'Casual', games: 10, wins: 6, losses: 4, win_pct: 0.6, pts_per_game: 33 }],
    by_game_type: [{ key: 'normal', name: 'Normal', games: 23, wins: 14, losses: 9, win_pct: 0.609, pts_per_game: 31.4 }],
    wins_vs_losses: { wins: { games: 14, pts: 36.2, yds: 455, to: 0.9 }, losses: { games: 9, pts: 23.9, yds: 346, to: 2.1 } },
    head_to_head: [{ ...PLAYZ, wins: 5, losses: 3, pts_for: 212, pts_against: 180, last: { game_id: 'g-013', at: at(26), result: 'win', score: [35, 14], margin: 21 }, biggest_win: { margin: 21 }, biggest_loss: { margin: 11 } }],
    records: RECORDS() });
}
export function oldProfile() {
  const p = profile();
  delete p.header.live; delete p.recent_form; delete p.career_rank;
  for (const k of ['total', 'peak', 'peak_at', 'around']) delete p.elo[k];
  p.by_queue.push({ key: 'private', name: 'Private room', games: 3, wins: 2, losses: 1, win_pct: 0.667, pts_per_game: 30 });
  p.recent_matches = [...p.recent_matches.slice(0, 3), allMatches().find((m) => m.queue === 'private'), ...p.recent_matches.slice(3, 9)];
  p.matches_total += 1;
  for (const r of [...p.career.rushing.rows, p.career.rushing.total]) { r.yac = r.yco; delete r.yco; }
  stripColours(p);
  return p;
}
```

- [ ] **Step 2: Write the page states**

Create `Tools/lib/page-states.mjs`:

```js
// The screens the layout tools and the Stats/Profile checks open (plan 4): each page in the states that draw its
// layout, with answers shaped like the real commands, a frozen clock (2026-09-30 20:00 UTC, time zone UTC), no network
// (the backend's images and Discord avatars are answered with art/cover.png) and, on request, a connected controller.
//     import { STATES, answers, serve, openState } from './lib/page-states.mjs';
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import * as F from '../fixtures/stats-fixtures.mjs';

export const root = path.resolve(import.meta.dirname, '..', '..');
export const ui = path.join(root, 'crates/reborn-ui/ui');
export const GAME_ID = 'aaaaaaaa-1111-4222-8333-444444444444';
const STAGES = ['welcome', 'game_verification', 'controller_setup', 'controls_profile', 'system_check', 'updates', 'preparation', 'completion'];
const SNAP = JSON.parse(readFileSync(path.join(root, 'Tools/fixtures/live-snapshot.json'), 'utf8'));
const LIST = JSON.parse(readFileSync(path.join(root, 'Tools/fixtures/live-list.json'), 'utf8'));

export const STATES = {
  stats: [{ name: 'stats', url: 'stats.html', scope: 'main' }],
  profile: [{ name: 'profile', url: `profile.html?player=${F.ME}`, scope: 'main' }],
  recovery: [{ name: 'recovery', url: 'recovery.html', scope: 'main' }],
  game: [{ name: 'game', url: `game.html?game=${GAME_ID}`, scope: 'main' }, { name: 'game-missing', url: 'game.html?game=nope', scope: 'main' }],
  live: [{ name: 'live', url: `live.html?match=${SNAP.match.id}`, scope: 'main' }, { name: 'live-missing', url: 'live.html?match=nope', scope: 'main' }],
  online: [{ name: 'online', url: 'online.html', scope: 'main' }],
  onboarding: STAGES.map((s) => ({ name: `onboarding-${s}`, url: `onboarding.html?stage=${s}`, scope: 'page' })),
};

/** Every command's answer (check-shell-pages.mjs's FULL, plus what these pages read). Unknown commands answer null. */
export function answers() {
  const source = readFileSync(path.join(root, 'Tools/check-shell-pages.mjs'), 'utf8');
  const data = vm.runInNewContext(source.slice(source.indexOf('const FULL ='), source.indexOf('// Every field null')) + '\nFULL');
  const g0 = SNAP.games[0];
  const bindings = [...['A', 'B', 'X', 'Y', 'Z', 'Start'].map((x) => ({ key: `Buttons/${x}`, label: x, binding: `\`Button ${x}\`` })),
    ...['Up', 'Down', 'Left', 'Right'].flatMap((x) => ['Main Stick', 'C-Stick', 'D-Pad'].map((g) => ({ key: `${g}/${x}`, label: `${g} ${x}`, binding: `\`${x}\`` }))),
    ...['L', 'R'].map((x) => ({ key: `Triggers/${x}`, label: x, binding: `\`Trigger ${x}\`` }))];
  Object.assign(data, {
    session_active: false, ui_notices: [], netplay_status: null, netplay_pending_join: null, mm_state: null, mm_rooms: null, mm_launch_prefs: null,
    settings_paths: { runtime: null, game: null, userDir: null, legacyRoot: null },
    update_policy: { automatic: false, source: 'settings' }, update_check: { state: 'current', current: '3.0.0' }, update_activity: { downloading: false, ready: null },
    content_status: { phase: 'up_to_date', note: 'Everything is current.', components: [] },
    content_check: { phase: 'up_to_date', note: 'Everything is current.', components: [] },
    onboarding_progress: { complete: true, next: { kind: 'already_complete' }, plan: [], set_aside: [], summary: { graphics: 'balanced' } },
    launcher_look: { windowGlass: false, slowAnimations: false, glassSupported: true, glassApplied: false, glassNote: null },
    identity_status: { signed_in: true, display_name: 'adamknowsball', player_id: F.ME, show_offline: false },
    stats_queue: { ...data.stats_queue, depth: 2, entries: [{ key: 'q1', state: 'queued' }, { key: 'q2', state: 'stalled', reason: 'The server did not answer.' }] },
    live_matches: LIST.matches,
    live_match: SNAP,
    stats_game: { game: { id: GAME_ID, mode: 'online', participants: [], teams_view: g0.teams,
      teams: [{ name: g0.teams[0].name, abbrev: g0.teams[0].abbrev, score: g0.score[0] }, { name: g0.teams[1].name, abbrev: g0.teams[1].abbrev, score: g0.score[1] }] },
      plays: SNAP.games[1]?.plays ?? [], lines: g0.box.lines, drives: g0.drives },
    stats_dashboard: F.dashboard(),
    player_profile: F.profile(),
    player_profile_all: F.publicMatches(),
    setup_install_info: { mode: 'migrate', root: 'C:/Reborn', games: ['C:/Reborn/Games/NSR.iso'], settings_found: true, installed_release: { release: '2.1', game: 'week10', textures: 'week10' }, memory_cards: [] },
    setup_graphics_options: { current: null, previous: null },
    setup_hardware_info: { description: 'NVIDIA GeForce RTX 4070 · 32 GB RAM', recommended: 'quality' },
    setup_controller_mapping: { custom: false, rows: bindings },
  });
  return data;
}

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.jpg': 'image/jpeg', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.json': 'application/json' };
export async function serve() {
  const server = createServer((req, res) => {
    try {
      const p = path.resolve(ui, '.' + decodeURIComponent(new URL(req.url, 'http://x').pathname));
      if (!p.startsWith(ui + path.sep)) throw new Error('outside');
      res.writeHead(200, { 'Content-Type': TYPES[path.extname(p)] || 'application/octet-stream' });
      res.end(readFileSync(p));
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  return { base: `http://127.0.0.1:${server.address().port}`, close: () => new Promise((r) => server.close(r)) };
}

export async function openState(browser, base, state, { viewport = { width: 1280, height: 800 }, glass = false, slow = false, reducedMotion = 'reduce', pad = false, data = null } = {}) {
  const context = await browser.newContext({ viewport, reducedMotion, timezoneId: 'UTC', locale: 'en-US' });
  const png = readFileSync(path.join(ui, 'art/cover.png'));
  await context.route(/^https:\/\/(api\.nflstreetreborn\.com|cdn\.discordapp\.com)\//, (route) => route.fulfill({ status: 200, contentType: 'image/png', body: png }));
  const all = { ...answers(), ...(data || {}) };
  await context.addInitScript(({ data, stages, glass, slow, pad }) => {
    const FIXED = Date.UTC(2026, 8, 30, 20, 0, 0);
    const RealDate = Date;
    class FixedDate extends RealDate { constructor(...a) { super(...(a.length ? a : [FIXED])); } static now() { return FIXED; } }
    window.Date = FixedDate;
    try { localStorage.setItem('reborn:look', JSON.stringify({ glass, slow })); } catch { /* private mode */ }
    const stage = new URL(location.href).searchParams.get('stage');
    if (stage) data.onboarding_progress = { complete: false, next: { kind: 'show', stage, position: stages.indexOf(stage) + 1, total: stages.length },
      plan: stages.map((s) => ({ stage: s, satisfied: stages.indexOf(s) < stages.indexOf(stage) })), set_aside: [], summary: {}, preparation_remaining: [], tasks_done: [] };
    if (pad) {
      window.pads = [{ id: 'Xbox Wireless Controller', index: 0, connected: true, mapping: 'standard', axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })) }];
      Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => window.pads });
      document.hasFocus = () => true;
    }
    window.__calls = [];
    window.__TAURI__ = { event: { listen: async () => () => {} }, core: { invoke: async (cmd, args = {}) => {
      window.__calls.push({ cmd, args });
      if (cmd === 'launcher_look') return { ...data.launcher_look, windowGlass: glass, slowAnimations: slow, glassApplied: glass };
      if (cmd === 'window_glass_apply') return { ...data.launcher_look, windowGlass: !!args.enabled, glassApplied: !!args.enabled };
      if (cmd === 'player_profile' && args.before) return { ok: true, matches: (data.player_profile_all || []).filter((m) => m.at < args.before).slice(0, 10) };
      if (cmd === 'launch') return new Promise(() => {});
      if (cmd in data) return data[cmd];
      if (/^onboarding_/.test(cmd)) return { outcome: 'recorded' };
      return null;
    } } };
  }, { data: JSON.parse(JSON.stringify(all)), stages: STAGES, glass, slow, pad });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(`${state.name}: ${e.message}`));
  await page.goto(`${base}/${state.url}`);
  await page.waitForFunction(() => { const s = document.getElementById('screen'); return s && !s.querySelector('.loading'); }, null, { timeout: 8000 }).catch(() => {});
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(600);
  return { page, errors, context, close: () => context.close() };
}
```

- [ ] **Step 3: Write the layout compare tool**

Create `Tools/compare-page-layout.mjs`:

```js
// Layout snapshots of a page before and after its stylesheets change (spec §9 "no layout work", §13).
//   node Tools/compare-page-layout.mjs capture <page> <dir>
//        writes <dir>/<page>.json and, per state and window size, <state>-<w>.png (top) and <state>-<w>-end.png (scrolled)
//   node Tools/compare-page-layout.mjs diff <page> <beforeDir> <afterDir> [--loose <regex>]...
// Each element's box is taken relative to its parent's box, so a taller heading moves nothing but its own row.
// FAIL: an element appears, disappears or changes visibility, or moves / resizes SIDEWAYS by more than 2px (unless its
//       ancestor chain matches a --loose regex). WARN: a vertical move or height change over 2px; judge each one in the
//       before/after screenshots. The page heading, the diagnostic fold and the shell are not compared.
import { chromium } from 'playwright';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { STATES, serve, openState } from './lib/page-states.mjs';

const [mode, page, a, b, ...rest] = process.argv.slice(2);
if (!STATES[page] || !['capture', 'diff'].includes(mode)) {
  console.error(`usage: capture <page> <dir> | diff <page> <before> <after> [--loose <regex>]...  (pages: ${Object.keys(STATES).join(', ')})`);
  process.exit(2);
}
const IGNORE = '.page-heading, details.core, .backdrop, .app-header, .input-footer, .skip-link, .update-center, script, style';
const VIEWPORTS = [{ width: 1280, height: 800 }, { width: 800, height: 560 }];

async function capture(dir) {
  mkdirSync(dir, { recursive: true });
  const srv = await serve();
  const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
  const out = {};
  try {
    for (const state of STATES[page]) for (const viewport of VIEWPORTS) {
      const { page: p, errors, close } = await openState(browser, srv.base, state, { viewport });
      const key = `${state.name}@${viewport.width}`;
      out[key] = await p.evaluate(({ scope, ignore }) => {
        const top = scope === 'page' ? document.body : document.getElementById('main');
        const rows = {};
        const name = (n) => `${n.tagName.toLowerCase()}${n.id ? `#${n.id}` : ''}${n.classList.length ? `.${[...n.classList].join('.')}` : ''}`;
        const walk = (node, keyPath, chain) => {
          [...node.children].forEach((child, i) => {
            if (child.matches(ignore)) return;
            const k = `${keyPath}/${child.tagName.toLowerCase()}[${i}]`;
            const c = `${chain} > ${name(child)}`;
            const r = child.getBoundingClientRect(), pr = node.getBoundingClientRect(), cs = getComputedStyle(child);
            const shown = cs.display !== 'none' && cs.visibility !== 'hidden' && (r.width > 0 || r.height > 0);
            rows[k] = { chain: c, shown, dx: Math.round(r.left - pr.left), dy: Math.round(r.top - pr.top), w: Math.round(r.width), h: Math.round(r.height) };
            if (shown) walk(child, k, c);
          });
        };
        walk(top, name(top), name(top));
        return rows;
      }, { scope: state.scope, ignore: IGNORE });
      if (errors.length) out[key].__errors = errors;
      const file = key.replace('@', '-');
      await p.screenshot({ path: path.join(dir, `${file}.png`) });
      await p.evaluate((scope) => { const m = scope === 'page' ? document.scrollingElement : document.getElementById('main'); if (m) m.scrollTop = m.scrollHeight; }, state.scope);
      await p.waitForTimeout(150);
      await p.screenshot({ path: path.join(dir, `${file}-end.png`) });
      await close();
    }
  } finally { await browser.close(); await srv.close(); }
  writeFileSync(path.join(dir, `${page}.json`), JSON.stringify(out, null, 1));
  console.log(`compare-page-layout: captured ${Object.keys(out).length} screen(s) of ${page} in ${dir}`);
}

function diff(beforeDir, afterDir, loose) {
  const A = JSON.parse(readFileSync(path.join(beforeDir, `${page}.json`), 'utf8'));
  const B = JSON.parse(readFileSync(path.join(afterDir, `${page}.json`), 'utf8'));
  const fails = [], warns = [];
  for (const screen of Object.keys(A)) {
    const x0 = A[screen], y0 = B[screen];
    if (!y0) { fails.push(`${screen}: not captured after`); continue; }
    if (y0.__errors) fails.push(`${screen}: page errors after: ${y0.__errors.join(' | ')}`);
    for (const k of Object.keys(x0)) {
      if (k.startsWith('__')) continue;
      const x = x0[k], y = y0[k];
      if (!y) { fails.push(`${screen} ${x.chain}: gone after`); continue; }
      if (x.shown !== y.shown) { fails.push(`${screen} ${x.chain}: ${x.shown ? 'hidden' : 'shown'} after`); continue; }
      if (!x.shown) continue;
      const side = Math.abs(x.dx - y.dx) > 2 || Math.abs(x.w - y.w) > 2;
      const vert = Math.abs(x.dy - y.dy) > 2 || Math.abs(x.h - y.h) > 2;
      const line = `${screen} ${x.chain}: x ${x.dx}→${y.dx} w ${x.w}→${y.w} y ${x.dy}→${y.dy} h ${x.h}→${y.h}`;
      if (side && !loose.some((re) => re.test(x.chain))) fails.push(line);
      else if (side || vert) warns.push(line);
    }
    for (const k of Object.keys(y0)) if (!k.startsWith('__') && !x0[k]) fails.push(`${screen} ${y0[k].chain}: new after`);
  }
  for (const w of warns) console.log(`WARN  ${w}`);
  for (const f of fails) console.log(`FAIL  ${f}`);
  console.log(`compare-page-layout ${page}: ${fails.length ? 'FAIL' : 'PASS'} (${fails.length} fail, ${warns.length} warn)`);
  process.exit(fails.length ? 1 : 0);
}

if (mode === 'capture') await capture(path.resolve(a));
else {
  const loose = [];
  for (let i = 0; i < rest.length; i++) if (rest[i] === '--loose') loose.push(new RegExp(rest[++i]));
  diff(path.resolve(a), path.resolve(b), loose);
}
```

- [ ] **Step 4: Write the page switch tool**

Create `Tools/theme-page.mjs`:

```js
// Put a page on the design system: its <head> loads exactly theme.css + its own stylesheet, and <body> says so with the
// `themed` class (Tools/check-theme.mjs). Nothing else in the page changes: the DOM stays identical, so
// Tools/compare-page-layout.mjs can compare it element by element.
//     node Tools/theme-page.mjs <page> <stylesheet>        e.g.  node Tools/theme-page.mjs recovery pages/recovery.css
import { readFileSync, writeFileSync } from 'node:fs';

const [page, sheet] = process.argv.slice(2);
if (!page || !sheet) { console.error('usage: node Tools/theme-page.mjs <page> <stylesheet>'); process.exit(2); }
const file = `crates/reborn-ui/ui/${page}.html`;
const html = readFileSync(file, 'utf8');
const nl = html.includes('\r\n') ? '\r\n' : '\n';
const end = html.indexOf('</head>');
let head = html.slice(0, end);
const lines = [...head.matchAll(/[ \t]*(?:<link rel="stylesheet" href="[^"]+">)+[ \t]*\r?\n/g)];
if (!lines.length) throw new Error(`${page}: no stylesheet lines in <head>`);
lines.forEach((m, i) => { head = head.replace(m[0], i === 0 ? `  <link rel="stylesheet" href="theme.css"><link rel="stylesheet" href="${sheet}">${nl}` : ''); });
let out = head + html.slice(end);
out = out.replace(/<body class="([^"]*)"/, (m, cls) => (/\bthemed\b/.test(cls) ? m : `<body class="${cls.replace(/\bshell\b/, 'shell themed')}"`));
writeFileSync(file, out);
console.log(`${page}: theme.css + ${sheet}, body.themed`);
```

- [ ] **Step 5: Take the before pictures twice, and prove the tool is steady**

Link Playwright, then (Git Bash, worktree root):

```bash
for p in stats profile recovery game live online onboarding; do node Tools/compare-page-layout.mjs capture $p target/p4-before || exit 1; done
for p in recovery game live online onboarding; do node Tools/compare-page-layout.mjs capture $p target/p4-steady; done
for p in recovery game live online onboarding; do node Tools/compare-page-layout.mjs diff $p target/p4-before target/p4-steady || exit 1; done
```

Expected:
- 7 lines `compare-page-layout: captured N screen(s) of <page> in …target\p4-before` (onboarding: 16 screens; game and live: 4 each; the rest: 2).
- 5 lines `compare-page-layout <page>: PASS (0 fail, 0 warn)`.

If a steady diff shows a difference, something on that page depends on time or randomness the tool did not freeze. Find it from the WARN/FAIL line, freeze it in `openState` (an answer or the init script), and capture both runs again. Do not continue until all five diffs are clean.

Open `target/p4-before/stats-1280.png`, `profile-1280.png`, `recovery-1280.png`, `game-1280.png`, `live-1280.png`, `online-1280.png` and `onboarding-welcome-1280.png`. Each must show real content, not only an error line or "Loading…". If one shows only an error, `answers()` is missing a command that page reads: add it and capture again.

- [ ] **Step 6: Record every checker's result at the start of plan 4**

PowerShell, worktree root:

```powershell
New-Item -ItemType Directory -Force target\p4-before | Out-Null
$list = @(Get-ChildItem Tools\check-*.mjs) + @(Get-Item Tools\review-players.mjs, Tools\review-onboarding.mjs, Tools\review-finish.mjs, Tools\test-menu-pad.mjs, Tools\test-controller-report.mjs)
$list | ForEach-Object { node $_.FullName *> "target\p4-before\$($_.BaseName).log"; "$($_.BaseName) $LASTEXITCODE" } | Set-Content target\p4-before\checks.txt
node Tools\run-browser-tests.mjs target\browser-review *> target\p4-before\run-browser-tests.log
Get-Content target\p4-before\checks.txt
```

Expected: one line per checker, `0` for passing. `review-finish`, `check-online-mm` and `check-launch-polish` are non-zero (baseline). Keep this file: Task 15 compares against it.

Run the standing checks. Unlink Playwright.

- [ ] **Step 7: Commit**

```bash
git add Tools/fixtures/stats-fixtures.mjs Tools/lib/page-states.mjs Tools/compare-page-layout.mjs Tools/theme-page.mjs
git commit -m "tools: page states, layout snapshots and the theme switch for plan 4

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The Stats and Profile components in `theme.css`

Nothing visible changes yet: these classes are new and no page uses them until Task 4.

**Files:**
- Modify: `crates/reborn-ui/ui/theme.css` (append)
- Create: `Tools/check-stat-components.mjs`

**Interfaces:**
- Consumes: plan 2's `.tabs`/`.tab`/`.tab-ind`, `.avatar`, `.glass`, `@keyframes live-pulse`, tokens `--d2`, `--d3`, `--ease`.
- Produces (class names every later task uses):
  - `.sec-head` (40px; `h2` 16/600; a `.tabs` inside is 40px), `.sec-link`, `.sec-links`, `.k-label`, `.num`.
  - `.team-logo` (24px img; `.team-logo.text` = abbreviation badge), `img.avatar`.
  - `.player-face` > `.face` (img, or `span.face.letter`) + optional `img.badge`; the element's `--tc` is the team primary.
  - `.cover-wash` (reads `--c1`/`--c2`), `.cover-pic`, `.presence-dot`, `.live-dot`.
  - `.tbl` (`table-layout: fixed`, first column 220px, `tr.total`), `.tbl .who`.
  - `.rows-reveal` (+ `.open`, inner `div`), `.chev` (turns under `[aria-expanded="true"]`).
  - `.m-list`, `.m-row` (`.m-hd` 36px, match 56px, `.m-game` 40px, `.m-static`), `.m-res` (`b.W`/`b.L`), `.m-mu`, `.m-opp`, `.m-opp-tx`, `.m-opp-n`, `.m-elo` (`.up`/`.down`), `.m-date`, `.m-pager`, `.m-empty`.

- [ ] **Step 1: Write the failing check**

Create `Tools/check-stat-components.mjs`:

```js
// The Stats and Profile components in theme.css (spec §3.3, §3.4, §5.2): the 40px section header with or without tabs,
// the match list's row heights and result colours, the shared table grid and Total row, the portrait on the team
// colour, the cover wash from the avatar's colours, the best-of-3 reveal and its motion, the live dot.
//     node Tools/check-stat-components.mjs
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { serve } from './lib/page-states.mjs';

const html = `<!doctype html><html lang="en" data-glass="off" data-motion="slow"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css"></head>
<body class="shell themed page-test"><main id="main"><section id="screen" style="width:900px;padding:24px">
<div class="sec-head" id="h1"><h2>Your matches</h2><a class="sec-link" href="#">Your profile ›</a></div>
<div class="sec-head" id="h2"><h2>Match history</h2><div class="tabs"><span class="tab-ind"></span><button class="tab" aria-pressed="true">All</button><button class="tab" aria-pressed="false">Ranked</button></div></div>
<div class="glass m-list" id="list"><div class="m-row m-hd"><span>Result</span><span>Matchup</span><span>Opponent</span><span>Elo</span><span>Date</span><span></span></div>
<button class="m-row" id="row" aria-expanded="false"><span class="m-res"><b class="W">W</b><span>2–1</span></span><span class="m-mu"></span><span class="m-opp"></span><span class="m-elo up">+16</span><span class="m-date">Sep 26</span><span class="chev"></span></button>
<div class="rows-reveal" id="rev"><div inert><button class="m-row m-game" id="g1"><span class="m-res"><b class="L">L</b><span>20–31</span></span><span></span><span></span><span></span><span></span><span></span></button></div></div></div>
<table class="tbl" id="t1"><thead><tr><th>Player</th><th>YDS</th><th>TD</th></tr></thead><tbody><tr><th scope="row">A</th><td>1</td><td>2</td></tr><tr class="total"><th scope="row">Total</th><td>1</td><td>2</td></tr></tbody></table>
<table class="tbl" id="t2"><thead><tr><th>Player</th><th>CAR</th><th>YDS</th><th>AVG</th></tr></thead><tbody><tr><th scope="row">B</th><td>1</td><td>2</td><td>3</td></tr></tbody></table>
<span class="player-face" id="pf" style="--tc:#A71930"><span class="face letter">A</span></span><span class="player-face" id="pf0"><span class="face letter">B</span></span>
<div class="glass" id="card" style="position:relative;height:80px;--c1:#A71930;--c2:#2b0a10"><div class="cover-wash"></div></div>
<span class="live-dot" id="live"></span>
</section></main></body></html>`;

const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
try {
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  await context.route('**/fixture.html', (r) => r.fulfill({ contentType: 'text/html', body: html }));
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(`${srv.base}/fixture.html`);
  await page.evaluate(() => document.fonts.ready);
  const h = (sel) => page.locator(sel).evaluate((n) => n.getBoundingClientRect().height);
  const css = (sel, prop) => page.locator(sel).evaluate((n, p) => getComputedStyle(n)[p], prop);
  // 40px section headers, tabs or not (spec §3.3)
  assert.equal(await h('#h1'), 40);
  assert.equal(await h('#h2'), 40);
  assert.equal(await h('#h2 .tabs'), 40, 'tabs fit the 40px header');
  // row heights (spec §3.3): header 36, match 56, game 40 once open; W and L colours
  assert.equal(await h('#list .m-hd'), 36);
  assert.equal(await h('#row'), 56);
  assert.equal(await css('#row .m-res b', 'color'), 'rgb(61, 220, 151)', 'W is --ok');
  assert.equal(await css('#g1 .m-res b', 'color'), 'rgb(255, 122, 127)', 'L is --red-ink');
  // the reveal: closed = 0 high, at --d3; open = the games, and the chevron turns
  assert.equal(await h('#rev'), 0);
  assert.equal(await css('#rev', 'transitionDuration'), '0.96s', 'slow: --d3');
  const shut = await css('#row .chev', 'transform');
  await page.evaluate(() => {
    document.getElementById('rev').classList.add('open');
    document.querySelector('#rev>div').removeAttribute('inert');
    document.getElementById('row').setAttribute('aria-expanded', 'true');
  });
  await page.waitForFunction(() => document.getElementById('rev').getBoundingClientRect().height === 40);
  assert.equal(await h('#g1'), 40);
  assert.notEqual(await css('#row .chev', 'transform'), shut, 'the chevron turns');
  await page.evaluate(() => { document.documentElement.dataset.motion = 'quick'; });
  assert.equal(await css('#rev', 'transitionDuration'), '0.24s', 'quick: --d3');
  // tables share one grid: a 220px name column, then the numbers start on the same edge
  const cols = await page.evaluate(() => ['#t1', '#t2'].map((t) => {
    const first = document.querySelector(`${t} th`).getBoundingClientRect(), second = document.querySelector(`${t} th:nth-child(2)`).getBoundingClientRect();
    return [Math.round(first.width), Math.round(second.left)];
  }));
  assert.deepEqual(cols[0], cols[1]);
  assert.equal(cols[0][0], 220);
  assert.equal(await css('#t1 tr.total th', 'borderTopColor'), 'rgba(255, 255, 255, 0.16)', 'Total row: --stroke-hi');
  assert.equal(await css('#t1 thead th:nth-child(2)', 'textTransform'), 'uppercase');
  // the portrait follows its team colour; the cover wash uses the avatar's colours
  assert.notEqual(await css('#pf .face', 'backgroundImage'), await css('#pf0 .face', 'backgroundImage'), 'the face follows --tc');
  assert.match(await css('#card .cover-wash', 'backgroundImage'), /rgb\(167, 25, 48\)/);
  assert.equal(await css('#live', 'animationName'), 'live-pulse');
  // reduced motion: instant
  await page.emulateMedia({ reducedMotion: 'reduce' });
  assert.match(await css('#rev', 'transitionDuration'), /^0s/);
  assert.equal(await css('#live', 'animationName'), 'none');
  assert.deepEqual(errors, []);
  console.log('check-stat-components: PASS');
} finally { await browser.close(); await srv.close(); }
```

- [ ] **Step 2: Run it and confirm it fails**

Link Playwright. Run `node Tools/check-stat-components.mjs`.
Expected: FAIL at the first assertion (`#h1` height is not 40): no `.sec-head` rule exists yet.

- [ ] **Step 3: Append the components to `theme.css`**

Append at the very end of `crates/reborn-ui/ui/theme.css`:

```css
/* ================= Stats and Profile components (spec §3.3, §3.4, §5.2, §8.6, §8.7) ================= */
/* section header: 40px whatever it holds, so neighbouring columns' cards start on one line */
.sec-head{display:flex;align-items:center;justify-content:space-between;gap:16px;height:40px;margin-bottom:8px}
.sec-head h2{margin:0;font:600 16px/1.2 Inter,system-ui,sans-serif;letter-spacing:0;text-transform:none;color:var(--fg)}
.sec-head .tabs{padding:3px}
.sec-link{padding:0;border:0;background:none;font:600 12.5px Inter,system-ui,sans-serif;color:var(--red-ink);text-decoration:none;cursor:pointer}
.sec-link:hover{color:var(--fg)}
.sec-links{display:flex;gap:16px}
.sec-links a{font-size:12.5px;color:var(--fg-2);text-decoration:none}
.sec-links a:hover{color:var(--fg)}
.k-label{display:block;font:600 11px/1.3 Inter,system-ui,sans-serif;letter-spacing:.07em;text-transform:uppercase;color:var(--fg-3)}
.num{font-variant-numeric:tabular-nums}
/* team logo: the backend's full-colour logos (spec §3.4); the abbreviation in a small badge when there is none */
.team-logo{display:block;flex:none;width:24px;height:24px;object-fit:contain}
.team-logo.text{display:inline-grid;place-items:center;border-radius:50%;background:var(--glass-2);color:var(--fg-2);font:600 8.5px Inter,system-ui,sans-serif;letter-spacing:.02em}
img.avatar{object-fit:cover}
/* portrait: round, on the team's primary (--tc, set per face from the server), team logo badge at the bottom right */
.player-face{position:relative;display:block;flex:none;width:36px;height:36px}
.player-face>.face{display:block;width:100%;height:100%;border-radius:50%;object-fit:cover;object-position:50% 20%;box-shadow:0 0 0 2px rgba(255,255,255,.12);
  background:radial-gradient(circle at 50% 30%,color-mix(in srgb,var(--tc,#2a2d34) 70%,#fff 8%),color-mix(in srgb,var(--tc,#2a2d34) 55%,#000))}
.player-face>.face.letter{display:grid;place-items:center;color:var(--fg);font:700 14px Inter,system-ui,sans-serif}
.player-face>.badge{position:absolute;right:-4px;bottom:-3px;width:17px;height:17px;padding:2px;border-radius:50%;background:#16171c;box-shadow:0 0 0 1px var(--stroke-hi);object-fit:contain}
/* profile cover: the avatar's two colours (--c1/--c2 from cover-color.js) behind the whole card, fading out to the right */
.cover-wash{position:absolute;inset:0;pointer-events:none;opacity:.55;background:linear-gradient(115deg,var(--c1,#2a2d34),var(--c2,#15161a));
  -webkit-mask-image:linear-gradient(100deg,#000 0%,rgba(0,0,0,.35) 55%,transparent 85%);mask-image:linear-gradient(100deg,#000 0%,rgba(0,0,0,.35) 55%,transparent 85%)}
.cover-pic{background:radial-gradient(circle at 35% 30%,color-mix(in srgb,var(--c1,#3a3d45) 85%,#fff),var(--c1,#3a3d45) 45%,var(--c2,#1b1c21));box-shadow:0 0 0 4px rgba(255,255,255,.08),0 10px 28px rgba(0,0,0,.45)}
.presence-dot{position:absolute;right:6px;bottom:6px;width:18px;height:18px;border-radius:50%;background:var(--ok);border:3px solid #1a1b21}
.live-dot{display:inline-block;flex:none;width:8px;height:8px;border-radius:50%;background:var(--red);animation:live-pulse 1.6s ease-in-out infinite}
/* table: uppercase header, rows split by --stroke, a normal Total row, one fixed 220px name column shared by every table */
.tbl{width:100%;border-collapse:collapse;table-layout:fixed;font-size:12.5px;font-variant-numeric:tabular-nums}
.tbl th,.tbl td{height:40px;padding:0 12px;text-align:right;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.tbl thead th{height:36px;font:600 10.5px Inter,system-ui,sans-serif;letter-spacing:.06em;text-transform:uppercase;color:var(--fg-3)}
.tbl th:first-child,.tbl td:first-child{width:220px;text-align:left}
.tbl tbody>tr>*{border-top:1px solid var(--stroke);color:var(--fg-2)}
.tbl tbody th{font-weight:500;color:var(--fg)}
.tbl tr.total>*{border-top:1px solid var(--stroke-hi);font-weight:600;color:var(--fg)}
.tbl .who{display:inline-flex;align-items:center;gap:8px;max-width:100%;vertical-align:middle;color:inherit;text-decoration:none}
.tbl .who>span:last-child{overflow:hidden;text-overflow:ellipsis}
.tbl .who .team-logo{width:18px;height:18px}
.tbl .who .avatar{width:22px;height:22px;font-size:10px}
/* best of 3: the games open in place (grid rows 0fr to 1fr, spec §5.1), fade in, and the chevron turns (§5.2) */
.rows-reveal{display:grid;grid-template-rows:0fr;transition:grid-template-rows var(--d3) var(--ease)}
.rows-reveal>div{min-height:0;overflow:hidden;opacity:0;transition:opacity var(--d2) var(--ease)}
.rows-reveal.open{grid-template-rows:1fr}
.rows-reveal.open>div{opacity:1}
.chev{display:block;width:8px;height:8px;margin:auto;border-right:1.5px solid currentColor;border-bottom:1.5px solid currentColor;color:var(--fg-3);transform:translateY(-2px) rotate(45deg);transition:transform var(--d3) var(--ease)}
[aria-expanded="true"]>.chev{transform:translateY(2px) rotate(225deg)}
/* match list (match-rows.js): Stats "Your matches" and Profile "Match history" (spec §8.6, §8.7) */
.m-list{display:flex;flex-direction:column;overflow:hidden}
.m-row{all:unset;box-sizing:border-box;display:grid;grid-template-columns:84px 92px minmax(0,1fr) 44px 56px 14px;align-items:center;gap:16px;width:100%;height:56px;padding:0 16px;cursor:pointer}
.m-row:hover{background:var(--glass-1)}
.m-list>*+.m-row{border-top:1px solid var(--stroke)}
.m-row.m-hd{height:36px;cursor:default;font:600 10.5px Inter,system-ui,sans-serif;letter-spacing:.07em;text-transform:uppercase;color:var(--fg-3)}
.m-row.m-hd>:nth-child(4),.m-row.m-hd>:nth-child(5){text-align:right}
.m-row.m-hd:hover,.m-row.m-static:hover{background:none}
.m-row.m-static{cursor:default}
.m-row.m-game{height:40px;border-top:1px solid var(--stroke);background:rgba(0,0,0,.14)}
.m-row.m-game:hover{background:var(--glass-1)}
.m-res{display:flex;align-items:baseline;gap:8px;font:600 15px Inter,system-ui,sans-serif;font-variant-numeric:tabular-nums;color:var(--fg)}
.m-res b{width:12px;font-weight:700;font-size:13px}
.m-res b.W{color:var(--ok)}
.m-res b.L{color:var(--red-ink)}
.m-game .m-res{font-size:13.5px}
.m-mu{display:flex;align-items:center;gap:6px}
.m-mu i{font-style:normal;font-size:10.5px;color:var(--fg-3)}
.m-game .m-mu .team-logo{width:20px;height:20px}
.m-opp{display:flex;align-items:center;gap:10px;min-width:0;font-size:12.5px;color:var(--fg-2)}
.m-opp .avatar{width:26px;height:26px;font-size:12px}
.m-opp-tx{display:flex;flex-direction:column;min-width:0}
.m-opp-n{font-size:13.5px;color:var(--fg);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.m-opp small{font-size:11.5px;color:var(--fg-3)}
.m-elo,.m-date{font-size:12.5px;text-align:right;white-space:nowrap;font-variant-numeric:tabular-nums;color:var(--fg-3)}
.m-elo.up{color:var(--ok)}
.m-elo.down{color:var(--red-ink)}
.m-pager{margin-top:auto;display:flex;align-items:center;justify-content:space-between;min-height:52px;padding:0 16px;border-top:1px solid var(--stroke);font-size:12.5px;color:var(--fg-3)}
.m-empty{margin:0;padding:16px;color:var(--fg-2)}
@media (max-width:720px){.m-row{grid-template-columns:72px 76px minmax(0,1fr) 40px 14px;gap:12px}.m-row>:nth-child(5){display:none}}
```

- [ ] **Step 4: Run it and confirm it passes**

- `node Tools/check-stat-components.mjs`: PASS.
- The standing checks: PASS. (Nothing uses the new classes yet, so every page looks as before.)

Unlink Playwright.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/theme.css Tools/check-stat-components.mjs
git commit -m "theme: Stats and Profile components (section header, match list, table, portrait, cover, reveal)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `stats-model.js`: the redesign's numbers and words

**Files:**
- Modify: `crates/reborn-ui/ui/stats-model.js`
- Test: `crates/reborn-ui/ui/stats-model.test.mjs`

**Interfaces:**
- Produces (new exports, used by Tasks 4–7):
  - `eloChange(delta) → {text: '+16' | '−18' | '', kind: 'up' | 'down' | ''}`.
  - `matchSubline(match, where = 'stats') → string`. Stats: `'Offline'` (offline or CPU), `'Private room'` (queue `private`), else `''`. Profile: `'Ranked'` / `'Casual'` / `''`. Never "Best of 3".
  - `lastTen(results) → '6–4' | '—'` (W/L letters, newest first, first ten).
  - `youNumbers(summary, recentForm) → [{label, value, kind}]` for Record, Win %, Last 10, Streak (`kind: 'up'` on a winning streak).
  - `statLine(role, data) → [{value, unit}]`: the role's four numbers when the server sent them, else its one `value` + unit, else `[]`.
  - `teamColour(hex) → '#RRGGBB' | null`.
  - `roleCards(best)` cards gain `primary` (a `teamColour`) and `line` (a `statLine`).
- Changes: result letters are `W` or `L` only (a `tie` or anything else draws nothing); `fmtElo(...).rank` reads `#7 on the leaderboard`.

- [ ] **Step 1: Write the failing tests**

In `crates/reborn-ui/ui/stats-model.test.mjs`:

Replace the line
```js
  assert.deepEqual(fmtElo({ rating: 1184, last_delta: 16, rank: 7 }), { value: '1,184', change: '▲ 16', rank: '#7 on the ladder' });
```
with
```js
  assert.deepEqual(fmtElo({ rating: 1184, last_delta: 16, rank: 7 }), { value: '1,184', change: '▲ 16', rank: '#7 on the leaderboard' });
```

Then append to the end of the file:

```js
import { eloChange, matchSubline, lastTen, youNumbers, statLine, teamColour } from './stats-model.js';

test('result letters are W and L only: ties do not exist', () => {
  const g = { id: 'g', at: '2026-09-27T01:18:00Z', mode: 'online', score: [21, 21], opponent: null, opponent_kind: 'cpu', me: {}, opp: {} };
  assert.equal(gameRowModel({ ...g, result: 'tie' }).result, '');
  assert.equal(gameRowModel({ ...g, result: 'LOSS' }).result, 'L');
  assert.equal(gameRowModel({ ...g, result: null }).result, '');
});

test('eloChange: +16, −18, or nothing', () => {
  assert.deepEqual(eloChange(16), { text: '+16', kind: 'up' });
  assert.deepEqual(eloChange(-18), { text: '−18', kind: 'down' });
  for (const x of [0, null, undefined, NaN]) assert.deepEqual(eloChange(x), { text: '', kind: '' });
});

test('matchSubline: Stats says Offline or Private room, Profile says the queue, never Best of 3', () => {
  assert.equal(matchSubline({ mode: 'offline' }), 'Offline');
  assert.equal(matchSubline({ mode: 'online', opponent_kind: 'cpu' }), 'Offline');
  assert.equal(matchSubline({ mode: 'online', queue: 'private' }), 'Private room');
  assert.equal(matchSubline({ mode: 'online', queue: 'ranked', best_of: 3 }), '');
  assert.equal(matchSubline({ queue: 'ranked', best_of: 3 }, 'profile'), 'Ranked');
  assert.equal(matchSubline({ queue: 'casual' }, 'profile'), 'Casual');
  assert.equal(matchSubline({ queue: 'private' }, 'profile'), '');
});

test('lastTen and youNumbers', () => {
  assert.equal(lastTen(['W', 'W', 'L', 'W', 'L', 'W', 'W', 'L', 'W', 'L', 'W']), '6–4');
  assert.equal(lastTen([]), '—');
  assert.equal(lastTen(['T', 'W']), '1–0');
  const n = youNumbers({ wins: 14, losses: 9, ties: 0, win_pct: 0.609, streak: 'W2' }, { results: ['W', 'L'] });
  assert.deepEqual(n.map((x) => [x.label, x.value, x.kind]), [['Record', '14–9', ''], ['Win %', '.609', ''], ['Last 10', '1–1', ''], ['Streak', 'W2', 'up']]);
  assert.deepEqual(youNumbers({}, null).map((x) => x.value), ['—', '—', '—', '—']);
  assert.equal(youNumbers({ wins: 1, losses: 3, streak: 'L3' }, null)[3].kind, '');
});

test('statLine: the four numbers of a role when the server sends them, else its one number', () => {
  assert.deepEqual(statLine('passer', { yds: 3108, td: 31, int: 11, rtg: 108.9, value: 3108, unit: 'pass_yds' }),
    [{ value: '3,108', unit: 'YDS' }, { value: '31', unit: 'TD' }, { value: '11', unit: 'INT' }, { value: '108.9', unit: 'RTG' }]);
  assert.deepEqual(statLine('rusher', { att: 212, yds: 1715, td: 12, avg: 5.6 }).map((s) => s.unit), ['CAR', 'YDS', 'TD', 'AVG']);
  assert.deepEqual(statLine('most_played', { games: 23, att: 96, yds: 535, td: 5 }).map((s) => `${s.value} ${s.unit}`), ['23 GAMES', '96 CAR', '535 YDS', '5 TD']);
  assert.deepEqual(statLine('most_played', { games: 23, rec: 40, yds: 535, td: 5 })[1], { value: '40', unit: 'REC' });
  assert.deepEqual(statLine('passer', { value: 3108, unit: 'pass_yds' }), [{ value: '3,108', unit: 'YDS' }]);
  assert.deepEqual(statLine('dl_lb', { value: 96, unit: 'tkl_sck' }), [{ value: '96', unit: 'TKL + SCK' }]);
  assert.deepEqual(statLine('passer', null), []);
});

test('teamColour, and role cards carry the colour and the statline', () => {
  assert.equal(teamColour('#A71930'), '#A71930');
  for (const x of ['red', '#fff', 'url(x)', null, '#A7193']) assert.equal(teamColour(x), null);
  const c = roleCards({ passer: { name: 'Penix', team: 'Falcons', logo_url: '/l', portrait_url: null, value: 2104, unit: 'pass_yds', primary: '#A71930' }, rusher: null });
  assert.equal(c[0].primary, '#A71930');
  assert.deepEqual(c[0].line, [{ value: '2,104', unit: 'YDS' }]);
  assert.equal(c[1].primary, null);
  assert.deepEqual(c[1].line, []);
});
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `node --test crates/reborn-ui/ui/stats-model.test.mjs`
Expected: FAIL. `fmtElo` still says "ladder", and the new imports are not exported (`SyntaxError: The requested module './stats-model.js' does not provide an export named 'eloChange'`).

- [ ] **Step 3: Change `stats-model.js`**

1. Replace the whole `getResultChar` function (the block starting `function getResultChar(result) {`) with:

```js
/**
 * The result letter: W or L. Ties do not exist in Reborn (spec §2), so anything else draws nothing.
 * @param {string|null} result
 * @returns {string}
 */
function getResultChar(result) {
  const r = String(result || '').toLowerCase();
  return r === 'win' ? 'W' : r === 'loss' ? 'L' : '';
}
```

2. In `fmtElo`, replace
```js
  const rank = elo.rank ? `#${elo.rank} on the ladder` : '';
```
with
```js
  const rank = elo.rank ? `#${elo.rank} on the leaderboard` : '';
```

3. In `roleCards`, replace (the empty card's end, 8-space indent)
```js
        initial: '',
        stat: ''
      };
```
with
```js
        initial: '',
        stat: '',
        primary: null,
        line: []
      };
```
and replace (the filled card's end, 6-space indent)
```js
      initial,
      stat
    };
```
with
```js
      initial,
      stat,
      primary: teamColour(data.primary),
      line: statLine(key, data)
    };
```

4. Append to the end of the file:

```js
// ---- the redesign (spec §8.6, §8.7) ----

/**
 * A row's Elo change: "+16" (up), "−18" (down), or nothing (unranked, offline, zero).
 * @param {number|null|undefined} delta
 * @returns {{text: string, kind: string}}
 */
export function eloChange(delta) {
  if (typeof delta !== 'number' || !Number.isFinite(delta) || delta === 0) return { text: '', kind: '' };
  return delta > 0 ? { text: `+${delta}`, kind: 'up' } : { text: `${MINUS}${-delta}`, kind: 'down' };
}

/**
 * The second line under a match row's opponent. Stats (the owner's own list): "Offline" for an offline or CPU match,
 * "Private room" for a private one, else nothing. Profile: the queue, "Ranked" or "Casual". Never "Best of 3": every
 * ranked match is one (spec §2).
 * @param {Object} match
 * @param {'stats'|'profile'} where
 * @returns {string}
 */
export function matchSubline(match, where = 'stats') {
  if (where === 'profile') return match?.queue === 'ranked' ? 'Ranked' : match?.queue === 'casual' ? 'Casual' : '';
  if (match?.mode === 'offline' || match?.opponent_kind === 'cpu') return 'Offline';
  if (match?.queue === 'private') return 'Private room';
  return '';
}

/**
 * "6–4" from the dashboard's recent_form.results (newest first; only W and L count), or an em dash.
 * @param {Array<string>|null|undefined} results
 * @returns {string}
 */
export function lastTen(results) {
  const r = (Array.isArray(results) ? results : []).filter((x) => x === 'W' || x === 'L').slice(0, 10);
  if (!r.length) return EM_DASH;
  const w = r.filter((x) => x === 'W').length;
  return `${w}${EN_DASH}${r.length - w}`;
}

/**
 * The You card's four numbers (spec §8.6): Record, Win %, Last 10, Streak. A winning streak is marked 'up'.
 * @param {Object|null} summary
 * @param {{results: Array<string>}|null} recentForm
 * @returns {Array<{label: string, value: string, kind: string}>}
 */
export function youNumbers(summary, recentForm) {
  const s = summary || {};
  const isNum = (n) => typeof n === 'number' && Number.isFinite(n);
  const streak = typeof s.streak === 'string' && /^[WL]\d+$/.test(s.streak) ? s.streak : EM_DASH;
  return [
    { label: 'Record', value: isNum(s.wins) && isNum(s.losses) ? `${s.wins}${EN_DASH}${s.losses}` : EM_DASH, kind: '' },
    { label: 'Win %', value: isNum(s.win_pct) ? fmtPct(s.win_pct) : EM_DASH, kind: '' },
    { label: 'Last 10', value: lastTen(recentForm?.results), kind: '' },
    { label: 'Streak', value: streak, kind: streak.startsWith('W') ? 'up' : '' },
  ];
}

// Each role's statline (spec §8.6 table): [field, unit, one decimal?].
const STATLINES = {
  passer: [['yds', 'YDS'], ['td', 'TD'], ['int', 'INT'], ['rtg', 'RTG', true]],
  rusher: [['att', 'CAR'], ['yds', 'YDS'], ['td', 'TD'], ['avg', 'AVG', true]],
  receiver: [['rec', 'REC'], ['yds', 'YDS'], ['td', 'TD'], ['avg', 'AVG', true]],
  dl_lb: [['tkl', 'TKL'], ['sck', 'SCK'], ['tfl', 'TFL'], ['pbu', 'PBU']],
  ballhawk: [['int', 'INT'], ['pbu', 'PBU'], ['tkl', 'TKL'], ['td', 'TD']],
};
const UNIT_SHORT = { pass_yds: 'YDS', rush_yds: 'YDS', rec_yds: 'YDS', tkl_sck: 'TKL + SCK', int: 'INT', games: 'GAMES' };

/**
 * A player row's statline: the role's four numbers when the server sends them (numbers formatted, units uppercase),
 * else the one number it always sends (`value` + `unit`), else nothing.
 * @param {string} role
 * @param {Object|null} data
 * @returns {Array<{value: string, unit: string}>}
 */
export function statLine(role, data) {
  if (!data) return [];
  const isNum = (n) => typeof n === 'number' && Number.isFinite(n);
  const fmt = (v, dec) => (dec ? v.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 }) : fmtInt(v));
  const spec = role === 'most_played'
    ? [['games', 'GAMES'], isNum(data.att) && !isNum(data.rec) ? ['att', 'CAR'] : ['rec', 'REC'], ['yds', 'YDS'], ['td', 'TD']]
    : STATLINES[role];
  if (spec && spec.every(([k]) => isNum(data[k]))) return spec.map(([k, unit, dec]) => ({ value: fmt(data[k], dec), unit }));
  if (!isNum(data.value)) return [];
  return [{ value: fmtInt(data.value), unit: UNIT_SHORT[data.unit] || String(data.unit || '').toUpperCase() }];
}

/**
 * A team's primary colour as the server sent it, only when it is exactly #RRGGBB (it is set as a CSS custom property).
 * @param {unknown} hex
 * @returns {string|null}
 */
export function teamColour(hex) {
  return typeof hex === 'string' && /^#[0-9a-fA-F]{6}$/.test(hex) ? hex : null;
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `node --test crates/reborn-ui/ui/stats-model.test.mjs crates/reborn-ui/ui/stats-rows.test.mjs crates/reborn-ui/ui/cover-color.test.mjs`
Expected: every test passes (the old ones and the six new ones).

Run the standing checks (`stats.js` and `profile.js` still use `fmtElo` and `roleCards`; nothing they read changed shape).

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/stats-model.js crates/reborn-ui/ui/stats-model.test.mjs
git commit -m "stats-model: Elo change, sublines, last 10, You numbers, statlines, team colour; W/L only; leaderboard

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The Stats page

**Files:**
- Create: `crates/reborn-ui/ui/match-rows.js`, `crates/reborn-ui/ui/pages/stats.css`, `Tools/check-stats-page.mjs`
- Modify: `crates/reborn-ui/ui/stats.html`, `Tools/check-theme.mjs`, `Tools/check-launch-polish.mjs`
- Rewrite: `crates/reborn-ui/ui/stats.js`

**Interfaces:**
- Consumes: Task 2's classes; Task 3's `eloChange`, `matchSubline`, `youNumbers`, `roleCards` (`primary`, `line`); `tabs()` from plan 2; `fmtElo`, `trendPoints`, `matchRowModel` (existing); `queueLine`, `unsentCount` (`stats-rows.js`).
- Produces (`match-rows.js`, reused by Profile in Task 6):
  - `initial(name) → string`
  - `teamLogo(team, cls = '') → HTMLElement` (`img.team-logo`, or `span.team-logo.text` with the abbreviation)
  - `avatar(url, name, cls = '') → HTMLElement` (`img.avatar`, or `span.avatar` with the first letter)
  - `matchHeader() → HTMLElement` (`div.m-row.m-hd`, `aria-hidden`)
  - `gameIdsOf(matches) → string[]`, `storeGameList(matches, label, from)` (writes `sessionStorage['nsr.gameList']`, as today)
  - `matchRows(match, {where: 'stats' | 'profile', open: Set<matchId>, openGame: (gameId) => void}) → HTMLElement[]` (a row, plus `div.rows-reveal#games-<id>` for a best of 3; toggles in place, no redraw)
  - `pager({page, pages, onNewer, onOlder, olderDisabled}) → HTMLElement` (`div.m-pager`; `pages` null draws "Page N")

- [ ] **Step 1: Write the failing page check**

Create `Tools/check-stats-page.mjs`:

```js
// The Stats page (spec §8.6, §10, §13): the two columns' cards start and end on one line, 40px section headers, the
// match list (header, 56px rows, the pager inside the card), W/L only, never "Best of 3" or "ladder", the best-of-3
// row opening in place, focus kept across a refresh, You and Your players, the footer line, the small window, signed
// out, a refusal, and the deployed server's answers.
//     node Tools/check-stats-page.mjs [outdir]      (screens in target/p4-after by default)
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { serve, openState, root } from './lib/page-states.mjs';
import * as F from './fixtures/stats-fixtures.mjs';

const out = path.resolve(process.argv[2] || path.join(root, 'target/p4-after'));
mkdirSync(out, { recursive: true });
const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const STATE = { name: 'stats', url: 'stats.html', scope: 'main' };
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, h: r.height }; });
const texts = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => n.textContent.trim()));
const heights = async (page, sel) => [...new Set(await page.locator(sel).evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().height)))];
const noSideways = async (page) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
try {
  { // 1280×800, the redesign's full answers
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.locator('.st-cols').waitFor();
    assert.equal(await page.locator('.page-heading .purpose').textContent(), 'Every match you’ve played, where your Elo stands, and who’s carrying your team.');
    // the two columns: first cards start on one line and end on one line; every section header is 40px
    const list = await rect(page, '.st-list'), you = await rect(page, '.st-you'), players = await rect(page, '.st-plist');
    assert.ok(Math.abs(list.top - you.top) < 1, `first cards start together: ${list.top} vs ${you.top}`);
    assert.ok(Math.abs(list.bottom - players.bottom) < 1, `the columns end together: ${list.bottom} vs ${players.bottom}`);
    assert.deepEqual(await heights(page, '.sec-head'), [40]);
    // the list: header, ten 56px rows, the pager inside the card as its last row
    assert.deepEqual(await texts(page, '.st-list .m-hd span'), ['Result', 'Matchup', 'Opponent', 'Elo', 'Date', '']);
    const rows = page.locator('.st-list > .m-row:not(.m-hd)');
    assert.equal(await rows.count(), 10);
    assert.deepEqual(await heights(page, '.st-list > .m-row:not(.m-hd)'), [56]);
    assert.ok(await page.locator('.st-list > .m-pager').evaluate((n) => n === n.parentElement.lastElementChild), 'the pager is the card’s last row');
    assert.equal(await page.locator('.m-pager span').textContent(), 'Page 1 of 2');
    assert.equal(await page.locator('[data-key="newer"]').isDisabled(), true);
    // a row reads RESULT · MATCHUP · OPPONENT · ELO · DATE
    const row = (id) => page.locator(`[data-key="match-${id}"]`);
    assert.equal(await row('m-01').locator('.m-res').textContent(), 'W2–1');
    assert.equal(await row('m-01').locator('.m-opp-n').textContent(), 'Playz');
    assert.equal(await row('m-01').locator('small').count(), 0, 'a ranked match has no second line');
    assert.equal(await row('m-01').locator('.m-elo').textContent(), '+16');
    assert.match(await row('m-01').locator('.m-elo').getAttribute('class'), /\bup\b/);
    assert.equal(await row('m-01').locator('.m-date').textContent(), 'Sep 26');
    assert.equal(await row('m-01').locator('.m-mu .team-logo').count(), 2);
    assert.equal(await row('m-04').locator('.m-res').textContent(), 'L0–2');
    assert.equal(await row('m-04').locator('.m-elo').textContent(), '−18');
    assert.equal(await row('g:g-031').locator('small').textContent(), 'Offline');
    assert.equal(await row('g:g-031').locator('.m-opp-n').textContent(), 'CPU');
    assert.equal(await row('m-05').locator('small').textContent(), 'Private room');
    assert.equal(await row('m-02').locator('.m-elo').textContent(), '');
    // content rules (spec §13)
    const body = await page.locator('#screen').innerText();
    assert.doesNotMatch(body, /best of 3/i);
    assert.doesNotMatch(body, /ladder/i);
    assert.ok((await texts(page, '.m-res b')).every((t) => t === 'W' || t === 'L' || t === ''), 'results are W or L');
    // a best of 3 opens in place into its games
    assert.equal(await page.locator('#games-m-01').evaluate((n) => n.firstElementChild.hasAttribute('inert')), true, 'closed games cannot take focus');
    assert.equal(Math.round((await rect(page, '#games-m-01')).h), 0);
    await row('m-01').click();
    assert.equal(await row('m-01').getAttribute('aria-expanded'), 'true');
    await page.waitForFunction(() => document.getElementById('games-m-01').getBoundingClientRect().height > 100);
    assert.deepEqual(await texts(page, '#games-m-01 .m-game .m-opp'), ['Game 1 · 5:00 PM', 'Game 2 · 5:25 PM', 'Game 3 · 5:50 PM']);
    assert.deepEqual(await texts(page, '#games-m-01 .m-game .m-res'), ['W28–21', 'L20–31', 'W35–14']);
    assert.deepEqual(await heights(page, '#games-m-01 .m-game'), [40]);
    await page.screenshot({ path: path.join(out, 'stats-1280-bo3.png') });
    // a refresh (the 30 s timer, or a game ending) keeps the row open and the focus on the same game
    await page.locator('[data-key="game-g-012"]').focus();
    const before = await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'stats_dashboard').length);
    await page.evaluate(() => {
      document.dispatchEvent(new CustomEvent('session-state', { detail: true }));
      document.dispatchEvent(new CustomEvent('session-state', { detail: false }));
    });
    await page.waitForFunction((n) => window.__calls.filter((c) => c.cmd === 'stats_dashboard').length > n, before);
    await page.waitForTimeout(150);
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'game-g-012', 'focus kept across a refresh');
    assert.equal(await row('m-01').getAttribute('aria-expanded'), 'true', 'the row stays open');
    assert.ok((await rect(page, '#games-m-01')).h > 100, 'and stays drawn open');
    // the pager
    await page.locator('[data-key="older"]').click();
    assert.equal(await page.locator('.m-pager span').textContent(), 'Page 2 of 2');
    assert.equal(await rows.count(), 4);
    assert.equal(await page.locator('[data-key="older"]').isDisabled(), true);
    await page.locator('[data-key="newer"]').click();
    // filters
    await page.getByRole('button', { name: 'Offline', exact: true }).click();
    assert.deepEqual(await rows.evaluateAll((ns) => ns.map((n) => n.dataset.key)), ['match-g:g-031']);
    await page.getByRole('button', { name: 'All', exact: true }).click();
    assert.equal(await rows.count(), 10);
    // You: Elo, its change and the leaderboard rank, then Record · Win % · Last 10 · Streak
    assert.equal(await page.locator('.st-elo-big').textContent(), '1,184');
    assert.equal(await page.locator('.st-chg').textContent(), '▲ 16 · #7 on the leaderboard');
    assert.equal(await page.locator('.st-you svg.st-spark').count(), 1);
    assert.deepEqual(await texts(page, '.st-nums .k-label'), ['Record', 'Win %', 'Last 10', 'Streak']);
    assert.deepEqual(await texts(page, '.st-nums b'), ['14–9', '.609', '6–4', 'W2']);
    assert.match(await page.locator('.st-nums b').nth(3).getAttribute('class'), /\bup\b/);
    // Your players: the portrait on the team colour, the name and role, the statline the server sent
    assert.equal(await page.locator('.st-player').count(), 6);
    const first = page.locator('.st-player').first();
    assert.equal(await first.locator('.player-face').evaluate((n) => getComputedStyle(n).getPropertyValue('--tc').trim()), '#A71930');
    assert.equal(await first.locator('.st-pname').textContent(), 'M. Penix Jr.');
    assert.equal(await first.locator('.k-label').textContent(), 'Best passer');
    assert.equal(await first.locator('.st-line').textContent(), '3,108 YDS');
    assert.match(await first.getAttribute('href'), /#passing$/);
    assert.deepEqual(await texts(page, '.sec-links a'), ['Offense', 'Defense', 'Records']);
    // the footer line: the queue note on the left, Profile & sync on the right, on the columns' right edge
    assert.equal(await page.locator('.st-foot .st-queue').textContent(), '2 games not uploaded yet: The server did not answer.');
    assert.equal(await page.locator('.st-account > summary').textContent(), 'Profile & sync ›');
    const cols = await rect(page, '.st-cols'), sync = await rect(page, '.st-account > summary');
    assert.ok(Math.abs(cols.right - sync.right) < 1.5, 'Profile & sync ends on the columns’ right edge');
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'stats-1280.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // a server that sends every role's statline
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: F.dashboard({ lines: true }) } });
    await page.locator('.st-player').first().waitFor();
    assert.equal(await page.locator('.st-player').first().locator('.st-line').textContent(), '3,108 YDS·31 TD·11 INT·108.9 RTG');
    assert.equal(await page.locator('.st-player').nth(1).locator('.st-line').textContent(), '212 CAR·1,715 YDS·12 TD·5.6 AVG');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the deployed server: no team colours, no leaderboard neighbours; nothing invented
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: F.oldDashboard() } });
    await page.locator('.st-cols').waitFor();
    assert.equal(await page.locator('.player-face').first().evaluate((n) => getComputedStyle(n).getPropertyValue('--tc').trim()), '', 'no colour sent, none invented');
    assert.equal(await page.locator('.st-chg').textContent(), '▲ 16 · #7 on the leaderboard');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window: one column, long names end in an ellipsis, nothing scrolls sideways
    const { page, errors, close } = await openState(browser, srv.base, STATE, { viewport: { width: 800, height: 560 } });
    await page.locator('.st-cols').waitFor();
    await noSideways(page);
    assert.ok((await rect(page, '.st-you')).top > (await rect(page, '.st-list')).bottom - 1, 'one column in a small window');
    await page.evaluate(() => { document.querySelector('[data-key="match-m-01"] .m-opp-n').textContent = 'A player with an unusually long display name that goes on'; });
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'stats-800.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // signed out: the way in and Profile & sync, never an empty two-column frame
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { signed_in: false } } });
    await page.getByText('Sign in with Discord on the Play page to see your games.').waitFor();
    assert.equal(await page.locator('.st-cols').count(), 0);
    assert.equal(await page.locator('.st-account').count(), 1);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the server refused: its reason, and nothing invented
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { signed_in: true, ok: false, error: 'The server is busy.' } } });
    await page.getByText('The server is busy.').waitFor();
    assert.equal(await page.locator('.st-cols').count(), 0);
    assert.equal(await page.locator('.st-account').count(), 1);
    assert.deepEqual(errors, []);
    await close();
  }
  { // no games yet
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { ...F.dashboard(), matches: [] } } });
    await page.getByText('No games yet. Play one and it shows up here.').waitFor();
    assert.equal(await page.locator('.st-cols').count(), 0);
    assert.deepEqual(errors, []);
    await close();
  }
  console.log(`check-stats-page: PASS (screens in ${out})`);
} finally { await browser.close(); await srv.close(); }
```

- [ ] **Step 2: Run it and confirm it fails**

Link Playwright. Run `node Tools/check-stats-page.mjs`.
Expected: FAIL waiting for `.st-cols` (the old page has no such element), or at the subtitle assertion.

- [ ] **Step 3: Write `match-rows.js`**

Create `crates/reborn-ui/ui/match-rows.js`:

```js
// One match list for Stats (Your matches) and Profile (Match history), spec §8.6 and §8.7: a header RESULT · MATCHUP ·
// OPPONENT · ELO · DATE, one row per match (a best of 3 is one row with its series and a single Elo change, and opens
// in place into its games), and the pager inside the card. Matches first (spec §10).
// Server text only through el()'s `text`; images only through assetUrl() / avatarSrc() (the CSP's two image prefixes).
import { el } from './shell.js';
import { assetUrl, avatarSrc } from './live-common.js';
import { eloChange, matchRowModel, matchSubline } from './stats-model.js';

const RESULT_WORD = { W: 'win', L: 'loss' };

export function initial(name) {
  const first = Array.from(String(name || '').trim())[0];
  return first ? first.toUpperCase() : '?';
}

/** A team's full-colour backend logo, or its abbreviation in a small round badge when there is none or it fails. */
export function teamLogo(team, cls = '') {
  const c = `team-logo${cls ? ` ${cls}` : ''}`;
  const badge = () => el('span', { class: `${c} text`, 'aria-hidden': 'true', text: team?.abbrev || initial(team?.name) });
  const src = assetUrl(team?.logo_url);
  if (!src) return badge();
  const img = el('img', { class: c, alt: '', src, title: team?.name || null });
  img.addEventListener('error', () => img.replaceWith(badge()), { once: true });
  return img;
}

/** A player's Discord avatar, or the first letter of their name in the same circle. */
export function avatar(url, name, cls = '') {
  const c = `avatar${cls ? ` ${cls}` : ''}`;
  const letter = () => el('span', { class: c, 'aria-hidden': 'true', text: initial(name) });
  const src = avatarSrc({ avatar_url: url });
  if (!src) return letter();
  const img = el('img', { class: c, alt: '', src });
  img.addEventListener('error', () => img.replaceWith(letter()), { once: true });
  return img;
}

/** The list's column header (the rows carry full aria-labels, so it is hidden from screen readers). */
export function matchHeader() {
  return el('div', { class: 'm-row m-hd', 'aria-hidden': 'true' }, ...['Result', 'Matchup', 'Opponent', 'Elo', 'Date', ''].map((t) => el('span', { text: t })));
}

/** The ids of a set of matches' games, newest first (the server sends each match's games oldest first). This is the
 *  ordered list game.html's ‹ Previous / Next › bar steps through. */
export function gameIdsOf(matches) {
  const ids = [];
  for (const m of Array.isArray(matches) ? matches : []) {
    const games = Array.isArray(m?.games) ? m.games : [];
    for (let i = games.length - 1; i >= 0; i--) if (games[i] && typeof games[i].id === 'string') ids.push(games[i].id);
  }
  return ids;
}

/** Remembered right before opening a game, so the game view can step through this list and come back (`from`). */
export function storeGameList(matches, label, from) {
  try { sessionStorage.setItem('nsr.gameList', JSON.stringify({ ids: gameIdsOf(matches), label, from })); } catch { /* best effort */ }
}

const resultCell = (result, score) => el('span', { class: 'm-res' }, el('b', { class: result || null, text: result }), el('span', { text: score }));
const matchup = (me, opp) => el('span', { class: 'm-mu' }, teamLogo(me), el('i', { text: 'vs' }), teamLogo(opp));

function label(m, r, sub, elo) {
  const teams = `${m?.me?.name || 'Your team'} vs ${m?.opp?.name || 'their team'}`;
  const who = m?.opponent_kind === 'cpu' ? 'against the CPU' : r.opponent !== '—' ? `against ${r.opponent}` : '';
  const score = r.expandable ? `series ${r.score}` : r.score !== '—' ? `score ${r.score}` : 'no score';
  const parts = [`${r.date} ${r.time}`, [teams, who].filter(Boolean).join(', '), [score, RESULT_WORD[r.result]].filter(Boolean).join(', ')];
  if (sub) parts.push(sub);
  if (elo.text) parts.push(`${elo.text} Elo`);
  if (r.expandable) parts.push(`${r.games.length} games`);
  return `${parts.join('. ')}.`;
}

/**
 * A match's row, and for a best of 3 its games under it. `where` picks the second line ('stats': Offline / Private
 * room; 'profile': Ranked / Casual). `open` (a Set of match ids) is read and updated, so it survives redraws; opening
 * toggles classes in place (the reveal animates), closed games are inert. `openGame(id)` opens one game.
 */
export function matchRows(m, { where, open, openGame }) {
  const r = matchRowModel({ ...m, games: Array.isArray(m?.games) ? m.games : [] });
  const sub = matchSubline(m, where);
  const elo = eloChange(m?.elo_delta);
  const kids = [
    resultCell(r.result, r.score),
    matchup(r.me, r.opp),
    el('span', { class: 'm-opp' }, m?.opponent_kind === 'cpu' || !m?.opponent ? null : avatar(r.avatar, r.opponent),
      el('span', { class: 'm-opp-tx' }, el('span', { class: 'm-opp-n', text: r.opponent }), sub ? el('small', { text: sub }) : null)),
    el('span', { class: `m-elo${elo.kind ? ` ${elo.kind}` : ''}`, text: elo.text }),
    el('span', { class: 'm-date', text: r.date }),
    el('span', { class: r.expandable ? 'chev' : null, 'aria-hidden': 'true' }),
  ];
  const attrs = { class: 'm-row', 'data-key': `match-${r.id}`, 'aria-label': label(m, r, sub, elo) };
  if (!r.expandable) {
    if (r.onlyGame) return [el('button', { ...attrs, type: 'button', onclick: () => openGame(r.onlyGame) }, ...kids)];
    // A match with no recorded game has nothing to open.
    return [el('div', { ...attrs, class: 'm-row m-static', role: 'group' }, ...kids)];
  }
  const isOpen = open.has(r.id);
  const id = `games-${String(r.id).replace(/[^A-Za-z0-9_-]/g, '_')}`;
  const inner = el('div', { inert: isOpen ? null : '' }, ...r.games.map((g, i) => el('button', {
    type: 'button', class: 'm-row m-game', 'data-key': `game-${g.id}`, onclick: () => openGame(g.id),
    'aria-label': `Game ${i + 1}. ${g.score !== '—' ? `Score ${g.score}` : 'No score'}${g.result ? `, ${RESULT_WORD[g.result]}` : ''}. ${g.date} ${g.time}.`,
  }, resultCell(g.result, g.score), matchup(g.me, g.opp), el('span', { class: 'm-opp', text: `Game ${i + 1} · ${g.time}` }), el('span'), el('span'), el('span'))));
  const reveal = el('div', { class: `rows-reveal${isOpen ? ' open' : ''}`, id }, inner);
  const row = el('button', { ...attrs, type: 'button', 'aria-expanded': String(isOpen), 'aria-controls': id }, ...kids);
  row.addEventListener('click', () => {
    const now = !open.has(r.id);
    if (now) open.add(r.id); else open.delete(r.id);
    row.setAttribute('aria-expanded', String(now));
    reveal.classList?.toggle('open', now);
    if (now) inner.removeAttribute?.('inert'); else inner.setAttribute('inert', '');
  });
  return [row, reveal];
}

/** Newer / "Page N of M" / Older, inside the card as its last row. `pages` null draws "Page N" (the total is not known). */
export function pager({ page, pages, onNewer, onOlder, olderDisabled }) {
  return el('div', { class: 'm-pager' },
    el('button', { type: 'button', class: 'btn sm quiet', 'data-key': 'newer', text: 'Newer', disabled: page === 0, onclick: onNewer }),
    el('span', { text: pages ? `Page ${page + 1} of ${pages}` : `Page ${page + 1}` }),
    el('button', { type: 'button', class: 'btn sm', 'data-key': 'older', text: 'Older', disabled: Boolean(olderDisabled), onclick: onOlder }));
}
```

- [ ] **Step 4: Rewrite `stats.js`**

Replace the whole of `crates/reborn-ui/ui/stats.js` with:

```js
// The Stats tab (spec §8.6): the signed-in player's own dashboard (`stats_dashboard`). Left: Your matches (a match of
// one game opens game.html?game=<id>; a best of 3 opens in place into its games). Right: You (Elo, trend, Record ·
// Win % · Last 10 · Streak) and Your players. Under both, one line: the local results queue on the left and
// Profile & sync on the right (offline visibility, sign out, send results now).
// Everything here is server data (player-typed team and player names among it): drawn with el()'s `text`
// (textContent) only, never markup. Images only through assetUrl() (our /live-assets/ catalog) or avatarSrc() (the
// Discord avatar CDN), the two image prefixes the CSP allows. The only script-set style is a portrait's --tc (CSSOM).
import { call, el, markCurrent, partial, quietCall, reason, wireCoreFold } from './shell.js';
import { queueLine, unsentCount } from './stats-rows.js';
import { assetUrl } from './live-common.js';
import { fmtElo, roleCards, trendPoints, youNumbers } from './stats-model.js';
import { matchHeader, matchRows, pager, storeGameList } from './match-rows.js';
import { tabs } from './tabs.js';

const PAGE_SIZE = 10;
const REFRESH_MS = 30000;
const SIGNED_OUT = 'Sign in with Discord on the Play page to see your games.';
const EMPTY = 'No games yet. Play one and it shows up here.';
const FILTERS = [['all', 'All'], ['online', 'Online'], ['offline', 'Offline']];
// Which part of the profile each player opens: their role's Full stats table, or Career.
const ROLE_HASH = { passer: 'passing', rusher: 'rushing', receiver: 'receiving', most_played: 'career', dl_lb: 'defense', ballhawk: 'defense' };
const SVG_NS = 'http://www.w3.org/2000/svg';
const PAGER_OTHER = { older: 'newer', newer: 'older' };

const state = { signedIn: false, answered: false, dash: null, queue: [], filter: 'all', page: 0, error: null, loaded: false,
  // Which best-of-3 rows are open (by match id), kept across the 30 s refreshes.
  expanded: new Set(),
  // Profile & sync: the identity_status and stats_queue answers, or { error } when one failed.
  identity: null, queueView: null };
let busy = false, said = null;
const ok = (value) => value && !value.error;
const screen = document.getElementById('screen');
let generation = 0;

// The first load goes to the diagnostic log; the 30 s refreshes do not (they would push everything else out of it).
async function load(quiet = false) {
  const mine = ++generation;
  const ask = quiet ? quietCall : call;
  const [dash, queue, identity] = await Promise.allSettled([ask('stats_dashboard'), ask('stats_queue'), ask('identity_status')]);
  // A later load answered first (a sign-out mid-request, say): the newest request's answer wins, never an older one.
  if (mine !== generation) return;
  if (dash.status === 'fulfilled') {
    const v = dash.value;
    state.signedIn = Boolean(v?.signed_in);
    state.answered = true;
    if (!state.signedIn) { state.dash = null; state.error = null; }
    else if (v.ok === false) state.error = typeof v.error === 'string' && v.error ? v.error : 'Your stats could not be loaded.';
    else { state.dash = v; state.error = null; }
  } else {
    state.error = reason(dash.reason);
  }
  state.queue = queue.status === 'fulfilled' && Array.isArray(queue.value?.entries) ? queue.value.entries : [];
  state.queueView = queue.status === 'fulfilled' ? queue.value : { error: reason(queue.reason) };
  state.identity = identity.status === 'fulfilled' ? identity.value : { error: reason(identity.reason) };
  state.loaded = true;
  draw();
}

const profileHref = (id, hash = '') => `profile.html?player=${encodeURIComponent(id)}${hash ? `#${hash}` : ''}`;
const gameHref = (id) => `game.html?game=${encodeURIComponent(id)}`;

function svg(tag, attrs = {}) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  return node;
}

// ---- Your matches ----

function filtered(matches) {
  return matches.filter((m) => state.filter === 'all' || (state.filter === 'online' ? m.mode === 'online' : m.mode !== 'online'));
}

function matchesBlock(matches) {
  const bar = tabs('stats-matches', FILTERS.map(([, label]) => label), FILTERS.find(([k]) => k === state.filter)[1],
    (label) => { state.filter = FILTERS.find(([, l]) => l === label)[0]; state.page = 0; draw(); }, { label: 'Show matches' });
  if (typeof bar.querySelectorAll === 'function') bar.querySelectorAll('.tab').forEach((b, i) => b.setAttribute('data-key', `filter-${FILTERS[i][0]}`));
  const card = el('div', { class: 'glass m-list st-list' }, matchHeader());
  const list = filtered(matches);
  if (!list.length) card.append(el('p', { class: 'm-empty', text: 'No matches of that kind yet.' }));
  else {
    const pages = Math.max(1, Math.ceil(list.length / PAGE_SIZE));
    state.page = Math.max(0, Math.min(state.page, pages - 1));
    const openGame = (id) => { storeGameList(list, 'Your matches', 'stats.html'); location.href = gameHref(id); };
    card.append(...list.slice(state.page * PAGE_SIZE, (state.page + 1) * PAGE_SIZE).flatMap((m) => matchRows(m, { where: 'stats', open: state.expanded, openGame })));
    if (pages > 1) card.append(pager({ page: state.page, pages, onNewer: () => { state.page--; draw(); }, onOlder: () => { state.page++; draw(); }, olderDisabled: state.page >= pages - 1 }));
  }
  return el('section', { class: 'st-col', 'aria-labelledby': 'st-matches-h' },
    el('div', { class: 'sec-head' }, el('h2', { id: 'st-matches-h', text: 'Your matches' }), bar), card);
}

// ---- You ----

function youBlock(you, recentForm) {
  const e = fmtElo(you.elo);
  const delta = you.elo?.last_delta;
  const hist = Array.isArray(you.elo?.history) ? you.elo.history : [];
  const trend = trendPoints(hist, 200, 54);
  const link = you.player_id ? el('a', { class: 'sec-link', href: profileHref(you.player_id), 'data-key': 'profile', text: 'Your profile ›' }) : null;
  const chg = el('div', { class: 'st-chg' });
  if (e.change) chg.append(el('span', { class: delta > 0 ? 'up' : 'down', 'aria-label': `${delta > 0 ? 'Up' : 'Down'} ${Math.abs(delta)} since your last ranked match`, text: e.change }));
  if (e.change && e.rank) chg.append(el('span', { 'aria-hidden': 'true', text: ' · ' }));
  if (e.rank) chg.append(el('span', { text: e.rank }));
  const top = el('div', { class: 'st-you-top' }, el('div', {}, el('span', { class: 'k-label', text: 'Elo' }),
    el('div', { class: `st-elo-big${you.elo?.rating == null ? ' unranked' : ''}`, text: e.value }), chg));
  if (trend && typeof document.createElementNS === 'function') {
    const chart = svg('svg', { class: 'st-spark', viewBox: '0 0 200 54', preserveAspectRatio: 'none', role: 'img', 'aria-label': `Elo trend over your last ${hist.length} ranked matches` });
    chart.append(svg('polyline', { fill: 'none', stroke: 'currentColor', 'stroke-width': 2, 'stroke-linejoin': 'round', 'stroke-linecap': 'round', 'vector-effect': 'non-scaling-stroke', points: trend }));
    top.append(chart);
  }
  const nums = el('div', { class: 'st-nums' }, ...youNumbers(you.summary, recentForm).map((n) =>
    el('div', {}, el('span', { class: 'k-label', text: n.label }), el('b', { class: n.kind || null, text: n.value }))));
  return el('section', { 'aria-labelledby': 'st-you-h' }, el('div', { class: 'sec-head' }, el('h2', { id: 'st-you-h', text: 'You' }), link),
    el('div', { class: 'glass st-you' }, top, nums));
}

// ---- Your players ----

function playerRow(c, playerId) {
  const face = el('span', { class: 'player-face', 'aria-hidden': 'true' });
  if (c.primary && face.style?.setProperty) face.style.setProperty('--tc', c.primary);
  const letter = () => el('span', { class: 'face letter', text: c.initial || '' });
  const src = assetUrl(c.portrait);
  if (src) { const img = el('img', { class: 'face', alt: '', src }); img.addEventListener('error', () => img.replaceWith(letter()), { once: true }); face.append(img); }
  else face.append(letter());
  const badge = assetUrl(c.logo);
  if (badge) { const img = el('img', { class: 'badge', alt: '', src: badge }); img.addEventListener('error', () => img.remove(), { once: true }); face.append(img); }
  const line = el('div', { class: 'st-line' });
  c.line.forEach((s, i) => {
    if (i) line.append(el('span', { class: 'sep', 'aria-hidden': 'true', text: '·' }));
    line.append(el('span', {}, el('b', { text: s.value }), ` ${s.unit}`));
  });
  const tx = el('div', { class: 'st-ptx' },
    el('div', { class: 'st-phead' }, el('span', { class: 'st-pname', text: c.name }), el('span', { class: 'k-label', text: c.label })),
    c.line.length ? line : null);
  if (!c.line.length) return el('div', { class: 'st-player none' }, face, tx);
  if (!playerId) return el('div', { class: 'st-player' }, face, tx);
  return el('a', { class: 'st-player', href: profileHref(playerId, ROLE_HASH[c.role] || 'career'), 'data-key': `card-${c.role}` }, face, tx);
}

function playersBlock(best, playerId) {
  const links = playerId ? el('nav', { class: 'sec-links', 'aria-label': 'Your profile sections' },
    ...[['Offense', 'career'], ['Defense', 'defense'], ['Records', 'records']].map(([label, hash]) =>
      el('a', { href: profileHref(playerId, hash), 'data-key': `more-${hash}`, text: label }))) : null;
  return el('section', { 'aria-labelledby': 'st-players-h' }, el('div', { class: 'sec-head' }, el('h2', { id: 'st-players-h', text: 'Your players' }), links),
    el('div', { class: 'glass st-plist' }, ...roleCards(best || {}).map((c) => playerRow(c, playerId))));
}

// ---- Profile & sync (the only place the offline-visibility toggle lives, and one of the sign-out buttons) ----

// One action at a time; afterwards the whole page state is read again.
async function act(label, command, args, after) {
  if (busy) return;
  busy = true; state.error = null; draw();
  try { after(await call(command, args)); }
  catch (error) { state.error = `${label}: ${reason(error)}`; }
  await load();
  busy = false;
  draw();
}

function accountBlock() {
  const identity = state.identity, queue = state.queueView;
  const status = !ok(identity) ? 'Account unavailable' : identity.signed_in ? `Signed in as ${identity.display_name || 'your Discord account'}` : 'Not signed in';
  const profile = el('div', {}, el('h3', { text: 'Profile' }), el('p', { text: status }));
  if (!ok(identity)) profile.append(partial('Your account could not be loaded. Try refreshing.'));
  else if (identity.signed_in) {
    profile.append(el('label', { for: 'show-offline' },
      el('input', { type: 'checkbox', id: 'show-offline', checked: !!identity.show_offline, disabled: busy,
        onchange: (event) => act('Offline visibility', 'identity_set_show_offline', { show: !!event.target.checked }, () => { said = 'Profile settings saved.'; }) }),
      el('span', { text: 'Show my offline games on my profile' })),
      el('button', { type: 'button', class: 'btn sm', id: 'stats-sign-out', text: 'Sign out', disabled: busy, onclick: () => act('Sign out', 'identity_sign_out', {}, () => { said = 'Signed out.'; }) }));
  } else profile.append(el('p', { text: 'Connect your Discord account to add these results to your profile.' }), el('a', { href: 'index.html', class: 'btn sm', text: 'Sign in on Play' }));
  const sync = el('div', {}, el('h3', { text: 'Result sync' }));
  if (!ok(queue)) sync.append(partial('Result sync status could not be loaded.'));
  else sync.append(el('p', { text: queue.depth ? `${queue.depth} result${queue.depth === 1 ? ' is' : 's are'} waiting to sync.` : 'No results waiting to sync.' }));
  sync.append(el('p', { text: 'Results sync automatically after each game.' }),
    el('button', { type: 'button', class: 'btn sm', id: 'stats-send', text: 'Send results now', disabled: busy, onclick: () => act('Sync', 'stats_submit', {}, (result) => { said = result?.note || 'Sync complete.'; }) }));
  return el('details', { class: 'st-account' }, el('summary', { text: 'Profile & sync ›' }),
    el('div', { class: 'glass st-account-body' }, profile, sync, said ? el('p', { class: 'said', role: 'status', text: said }) : null));
}

// ---- drawing ----

// Redraws replace everything (every refresh): put the focus back on the same control, by data-key (spec §10).
function draw() {
  if (!screen) return;
  // Guarded: the Node page checker (Tools/check-shell-pages.mjs) draws into a minimal fake DOM without these.
  const active = typeof screen.contains === 'function' && screen.contains(document.activeElement) ? document.activeElement : null;
  const focusKey = active?.getAttribute('data-key') ?? null, focusId = active?.id || null;
  const accountOpen = Boolean(screen.querySelector?.('.st-account')?.open);
  paint(accountOpen);
  if (focusKey) {
    // "Older" on the last page (or "Newer" on the first) is now disabled and can't take focus: use the other one.
    const byKey = (k) => Array.from(screen.querySelectorAll('[data-key]')).find((n) => n.getAttribute('data-key') === k) ?? null;
    let target = byKey(focusKey);
    if (target?.disabled && PAGER_OTHER[focusKey]) target = byKey(PAGER_OTHER[focusKey]) ?? target;
    target?.focus({ preventScroll: true });
  } else if (focusId) document.getElementById(focusId)?.focus({ preventScroll: true });
}

function paint(accountOpen) {
  if (!state.loaded) { screen.replaceChildren(el('p', { class: 'loading', text: 'Loading…' })); return; }
  const account = accountBlock();
  account.open = accountOpen;
  const foot = (note) => el('div', { class: 'st-foot' }, note ? el('span', { class: 'st-queue', text: note }) : null, account);
  const q = queueLine(state.queue);
  const parts = [];
  if (state.error) parts.push(el('p', { class: 'refusal', role: 'alert', text: state.error }));
  if (!state.signedIn) {
    // Signed out: no rows, only the way in (and what is waiting locally). Never answered at all: only the error.
    if (state.answered) parts.push(el('p', { class: 'note', text: SIGNED_OUT }));
    const n = unsentCount(state.queue);
    screen.replaceChildren(...parts, foot(n ? `${n} ${n === 1 ? 'game' : 'games'} will upload when you sign in.` : null));
    return;
  }
  const d = state.dash;
  if (!d) { screen.replaceChildren(...parts, foot(q)); return; }
  const matches = Array.isArray(d.matches) ? d.matches : [];
  if (!matches.length) {
    parts.push(el('p', { class: 'note', text: EMPTY }));
    screen.replaceChildren(...parts, foot(q));
    return;
  }
  const you = d.you || {};
  parts.push(el('div', { class: 'st-cols' }, matchesBlock(matches),
    el('div', { class: 'st-side' }, youBlock(you, d.recent_form), playersBlock(d.best_players, you.player_id))));
  screen.replaceChildren(...parts, foot(q));
}

markCurrent();
wireCoreFold();
draw();
load();
// No refresh while a game is running (app-shell.js repeats `session-state` every 1.5 s): a match in progress
// needs the network. When the session ends, one quiet reload picks up the game that just finished.
let sessionActive = false;
document.addEventListener('session-state', (e) => {
  const next = Boolean(e?.detail);
  const ended = sessionActive && !next;
  sessionActive = next;
  if (ended && document.visibilityState !== 'hidden') load(true);
});
setInterval(() => { if (!sessionActive && document.visibilityState !== 'hidden') load(true); }, REFRESH_MS);
```

- [ ] **Step 5: Write `pages/stats.css`**

Create `crates/reborn-ui/ui/pages/stats.css`:

```css
/* Stats (spec §8.6): matches first, the full content width, two columns whose first cards start and end on one line.
   Layout only: every colour is a theme.css token (Tools/check-theme-tokens.mjs). The match rows are theme.css's .m-*. */
body.page-stats #main{padding:36px 36px 32px}
.page-stats .page-heading{margin-bottom:24px}
.page-stats #screen{display:flex;flex-direction:column;gap:24px;min-width:0}
.page-stats #screen>.refusal,.page-stats #screen>.note{margin:0}
.st-cols{display:grid;grid-template-columns:minmax(0,1.5fr) minmax(340px,1fr);gap:24px;align-items:stretch}
.st-col{display:flex;flex-direction:column;min-width:0}
.st-col>.st-list{flex:1}
.st-side{display:flex;flex-direction:column;gap:24px;min-width:0}
.st-side>section{display:flex;flex-direction:column}
.st-side>section:last-child{flex:1}
/* You */
.st-you{padding:16px}
.st-you-top{display:flex;align-items:flex-end;gap:16px;padding-bottom:16px;border-bottom:1px solid var(--stroke)}
.st-elo-big{margin-top:4px;font:700 32px/1.1 Inter,system-ui,sans-serif;letter-spacing:-.02em;font-variant-numeric:tabular-nums}
.st-elo-big.unranked{font-size:22px;color:var(--fg-2)}
.st-chg{margin-top:4px;font-size:12px;color:var(--fg-2)}
.st-chg .up{font-weight:600;color:var(--ok)}
.st-chg .down{font-weight:600;color:var(--red-ink)}
.st-spark{flex:1;min-width:0;height:54px;color:var(--red)}
.st-nums{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:16px;padding-top:16px}
.st-nums b{display:block;margin-top:4px;font:600 17px Inter,system-ui,sans-serif;font-variant-numeric:tabular-nums}
.st-nums b.up{color:var(--ok)}
/* Your players: one list that stretches so its bottom meets Your matches */
.st-plist{flex:1;display:flex;flex-direction:column;padding:0 16px}
.st-player{flex:1;display:flex;align-items:center;gap:16px;min-height:64px;color:inherit;text-decoration:none}
.st-player+.st-player{border-top:1px solid var(--stroke)}
a.st-player:hover .st-pname{text-decoration:underline}
.st-ptx{flex:1;min-width:0}
.st-phead{display:flex;align-items:baseline;justify-content:space-between;gap:8px}
.st-pname{font-size:13.5px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.st-phead .k-label{flex:none;font-size:10px}
.st-player.none .st-pname{font-weight:500;color:var(--fg-3)}
.st-line{margin-top:4px;font-size:12.5px;color:var(--fg-2);font-variant-numeric:tabular-nums;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.st-line b{font-weight:600;color:var(--fg)}
.st-line .sep{margin:0 6px;color:var(--fg-3)}
/* the footer line: the queue note on the left, Profile & sync on the right */
.st-foot{display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:16px 24px;font-size:12px;color:var(--fg-3)}
.st-account{margin-left:auto}
.st-account[open]{flex-basis:100%}
.st-account>summary{list-style:none;cursor:pointer;text-align:right;font-weight:600;color:var(--fg-2)}
.st-account>summary::-webkit-details-marker{display:none}
.st-account>summary:hover{color:var(--fg)}
.st-account-body{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px;margin-top:16px;padding:16px}
.st-account-body h3{margin:0 0 8px;font:600 14px Inter,system-ui,sans-serif;color:var(--fg)}
.st-account-body p{margin:0 0 8px;font-size:12.5px;color:var(--fg-2)}
.st-account-body label{display:flex;align-items:center;gap:8px;margin-bottom:12px;font-size:12.5px;color:var(--fg-2)}
.st-account-body>.said{grid-column:1/-1;margin:0}
@media (max-width:1100px){.st-cols{grid-template-columns:minmax(0,1fr)}}
@media (max-width:1000px){body.page-stats #main{padding:24px 24px 16px}}
@media (max-width:720px){.st-nums{grid-template-columns:repeat(2,minmax(0,1fr))}.st-account-body{grid-template-columns:minmax(0,1fr)}}
```

- [ ] **Step 6: Put `stats.html` on the theme**

Run: `node Tools/theme-page.mjs stats pages/stats.css`
Expected: `stats: theme.css + pages/stats.css, body.themed`.

Then in `crates/reborn-ui/ui/stats.html` replace
```html
<p class="purpose">Your game history and box scores.</p>
```
with
```html
<p class="purpose">Every match you’ve played, where your Elo stands, and who’s carrying your team.</p>
```

In `Tools/check-theme.mjs`, add `stats: "pages/stats.css",` inside the `THEMED` object literal (next to the entries plans 2 and 3 added).

- [ ] **Step 7: Replace the stale Stats block in `check-launch-polish.mjs`**

In `Tools/check-launch-polish.mjs`, delete every line from the line that begins
` await page.goto(base+'/stats.html');await page.getByRole('heading',{name:'Game history'}).waitFor();`
through the line that begins
` await page.getByRole('button',{name:'Send results now',exact:true}).click();`
(both included), and put these lines in their place:

```js
 // Stats v2 replaced the old searchable history (Tools/check-stats-page.mjs covers the page); what stays here is the
 // account and sync block this checker always drove.
 await page.goto(base+'/stats.html');await page.getByText('Profile & sync ›',{exact:true}).waitFor();
 await page.getByText('Profile & sync ›',{exact:true}).click();await page.getByRole('checkbox',{name:'Show my offline games on my profile'}).check();
 await page.waitForFunction(()=>calls.some(c=>c.cmd==='identity_set_show_offline'&&c.args.show));
 await page.getByRole('button',{name:'Send results now',exact:true}).click();await page.waitForFunction(()=>calls.some(c=>c.cmd==='stats_submit'));
```

In its final `console.log('PASS: …')` line, replace `searchable paged stats, small-window layout, ` with `Stats account and sync, `.

- [ ] **Step 8: Run the checks**

- `node Tools/check-stats-page.mjs`: PASS.
- `node Tools/check-theme.mjs`: PASS (stats loads exactly `theme.css` + `pages/stats.css`).
- `node Tools/check-theme-tokens.mjs`: PASS.
- `node Tools/check-launch-polish.mjs`: it must get past the Stats block. If it still fails, the failing line must be one that also failed in `target/p4-before/check-launch-polish.log` for a reason outside Stats; say so in the report.
- The standing checks: PASS. `check-shell-pages` still finds "Signed in as Adam", "Sign out" and "Send results now" on Stats.

Open `target/p4-after/stats-1280.png` beside `target/p4-before/stats-1280.png` and the mockup's Stats section. Check: two columns on one line, the matches card and players card ending together, W/L coloured letters, logos `vs` logos, the pager inside the card, no glow, no "Best of 3". Also open `stats-1280-bo3.png` and `stats-800.png`.

Unlink Playwright.

- [ ] **Step 9: Commit**

```bash
git add crates/reborn-ui/ui/match-rows.js crates/reborn-ui/ui/stats.js crates/reborn-ui/ui/stats.html crates/reborn-ui/ui/pages/stats.css Tools/check-stats-page.mjs Tools/check-theme.mjs Tools/check-launch-polish.mjs
git commit -m "stats: the redesigned Stats page on the design system (matches first, You, Your players, one footer line)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: `profile-model.js`: the Profile's view models

**Files:**
- Create: `crates/reborn-ui/ui/profile-model.js`
- Test: `crates/reborn-ui/ui/profile-model.test.mjs`

**Interfaces:**
- Consumes: `eloChange`, `fmtInt`, `fmtPct` (`stats-model.js`).
- Produces (used by Tasks 6–7). Every function returns `null` (or an empty list) when the server did not send what the piece needs, so the page leaves the piece out:
  - `isPublic(match) → boolean`; `publicQueues(byQueue) → rows` (no private row; an older server still sends one).
  - `winRate(p) → '60.9%' | null`.
  - `summaryFacts(profile) → [{strong, text}]` for "#7 on the leaderboard", "1,184 Elo", "60.9% win rate".
  - `liveCard(live) → {head, me, opp, score, line, href} | null` (null for a private room, or no match id).
  - `statusLine(header) → {text: 'In a match' | 'Online now', kind: 'live' | 'online'} | null`.
  - `areaPoints(values, w = 300, h = 70, pad = 6) → {line, area} | null`.
  - `eloOverview(elo) → {value, change, sub, note, points} | null`.
  - `recordOverview(summary, byQueue) → {value, pct, note, sub, rows: [{label, record, pct, share}]} | null`.
  - `lastTenOverview(recentForm) → {value, margin: {text, kind}, scored, allowed} | null`.
  - `careerTiles(career, careerRank, games) → {tiles: [{key, label, value, rank, perGame}], footnote}`.
  - `leaderboardRows(around, {self}) → [{rank, name, elo, me, avatar_url, player_id, letterOf}]`.
  - `FULL_STATS` (`passing | rushing | receiving | defense` → `{title, cols: [{label, title, key, dec?, get?, text?}]}`), `sortValue(col, row)`, `cellValue(col, row) → string`, `splitTop(rows, n = 5) → {top, rest}`.

- [ ] **Step 1: Write the failing tests**

Create `crates/reborn-ui/ui/profile-model.test.mjs`:

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { FULL_STATS, areaPoints, careerTiles, cellValue, eloOverview, isPublic, lastTenOverview, leaderboardRows, liveCard, publicQueues,
  recordOverview, splitTop, statusLine, summaryFacts, winRate } from './profile-model.js';

test('private rooms never pass', () => {
  assert.equal(isPublic({ queue: 'private' }), false);
  assert.equal(isPublic({ queue: 'ranked' }), true);
  assert.equal(isPublic({ queue: null }), true);
  assert.equal(isPublic(null), false);
  assert.deepEqual(publicQueues([{ key: 'ranked' }, { key: 'private', name: 'Private room' }, { key: 'x', name: 'Private' }, { key: 'casual' }]).map((r) => r.key), ['ranked', 'casual']);
});

test('summary line: rank, Elo and win rate, each only when the server sent it', () => {
  assert.deepEqual(summaryFacts({ elo: { rank: 7, rating: 1184 }, summary: { win_pct: 0.609 } }).map((f) => `${f.strong} ${f.text}`),
    ['#7 on the leaderboard', '1,184 Elo', '60.9% win rate']);
  assert.deepEqual(summaryFacts({ elo: { rank: null, rating: null }, summary: { win_pct: null } }), []);
  assert.deepEqual(summaryFacts({}), []);
  assert.equal(winRate(0.5), '50.0%');
  assert.equal(winRate(null), null);
});

test('live card: game N of 3 and the series for a best of 3; never a private room', () => {
  const c = liveCard({ match_id: 'm1', queue: 'ranked', best_of: 3, game_number: 2, series: [1, 0], score: [14, 7], me: { abbrev: 'ATL' }, opp: { abbrev: 'PHI' }, opponent: { display_name: 'Mikel' } });
  assert.equal(c.head, 'Ranked · Game 2 of 3');
  assert.equal(c.score, '14 – 7');
  assert.equal(c.line, 'vs Mikel · Series 1–0');
  assert.equal(c.href, 'live.html?match=m1');
  assert.deepEqual([c.me, c.opp], [{ abbrev: 'ATL' }, { abbrev: 'PHI' }]);
  const one = liveCard({ match_id: 'm2', queue: 'casual', best_of: 1, game_number: 1, series: [0, 0], score: null, opponent: null });
  assert.equal(one.head, 'Casual');
  assert.equal(one.score, null);
  assert.equal(one.line, '');
  assert.equal(liveCard({ match_id: 'm3', queue: 'private' }), null);
  assert.equal(liveCard(null), null);
  assert.equal(liveCard({ queue: 'ranked' }), null);
});

test('status: in a match, online now, or nothing', () => {
  assert.deepEqual(statusLine({ live: { match_id: 'x', queue: 'ranked' }, online: true }), { text: 'In a match', kind: 'live' });
  assert.deepEqual(statusLine({ live: null, online: true }), { text: 'Online now', kind: 'online' });
  assert.deepEqual(statusLine({ live: { match_id: 'x', queue: 'private' }, online: true }), { text: 'Online now', kind: 'online' });
  assert.equal(statusLine({ online: null }), null);
});

test('area chart points', () => {
  assert.deepEqual(areaPoints([1, 3], 100, 50, 0), { line: '0,50 100,0', area: 'M0,50 L100,0 L100,50 L0,50Z' });
  assert.equal(areaPoints([5]), null);
  assert.equal(areaPoints([2, 2], 100, 50, 0).line, '0,25 100,25');
});

test('Elo overview: value, change, peak and start; the peak from history when the server sends none', () => {
  const e = eloOverview({ rating: 1184, last_delta: 16, peak: 1212, history: [{ rating: 1000, delta: 0 }, { rating: 1212, delta: 212 }, { rating: 1184, delta: -28 }] });
  assert.equal(e.value, '1,184');
  assert.deepEqual(e.change, { text: '+16', kind: 'up' });
  assert.equal(e.sub, 'Peak 1,212 · started at 1,000');
  assert.equal(e.note, '3 ranked matches');
  assert.equal(e.points.line.split(' ').length, 4);
  const old = eloOverview({ rating: 1184, last_delta: -18, history: [{ rating: 1202, delta: 2 }, { rating: 1184, delta: -18 }] });
  assert.equal(old.sub, 'Peak 1,202 · started at 1,200');
  assert.deepEqual(old.change, { text: '−18', kind: 'down' });
  assert.equal(eloOverview({ rating: null }), null);
  assert.equal(eloOverview(undefined), null);
});

test('record overview: ranked and casual only, W–L, bars as the win share', () => {
  const r = recordOverview({ wins: 14, losses: 9, win_pct: 0.609 }, [{ key: 'ranked', wins: 8, losses: 5 }, { key: 'casual', wins: 6, losses: 4 }, { key: 'private', name: 'Private room', wins: 2, losses: 1 }]);
  assert.equal(r.value, '14–9');
  assert.equal(r.pct, '60.9%');
  assert.equal(r.note, '23 matches');
  assert.equal(r.sub, 'Ranked and casual');
  assert.deepEqual(r.rows, [{ label: 'Ranked', record: '8–5', pct: '.615', share: 61.5 }, { label: 'Casual', record: '6–4', pct: '.600', share: 60 }]);
  assert.equal(recordOverview({}, []), null);
});

test('last 10 games: record, average margin, points; nothing from an older server', () => {
  assert.deepEqual(lastTenOverview({ games: 10, wins: 6, losses: 4, avg_margin: 6.4, pts_for_pg: 31.4, pts_against_pg: 25 }),
    { value: '6–4', margin: { text: '+6.4', kind: 'up' }, scored: '31.4', allowed: '25.0' });
  assert.deepEqual(lastTenOverview({ games: 3, wins: 1, losses: 2, avg_margin: -3, pts_for_pg: null, pts_against_pg: 8 }),
    { value: '1–2', margin: { text: '−3.0', kind: 'down' }, scored: '—', allowed: '8.0' });
  assert.equal(lastTenOverview(null), null);
  assert.equal(lastTenOverview({ games: 0, wins: 0, losses: 0 }), null);
  assert.equal(lastTenOverview({ results: ['W'] }), null, 'the dashboard shape is not the profile block');
});

test('career tiles: nine spelled-out labels; the rank and per game only when sent', () => {
  const career = { passing: { total: { yds: 3021, td: 40, rtg: 104.2 } }, rushing: { total: { yds: 1715, td: 17 } }, receiving: { total: { yds: 1714, td: 17 } }, defense: { total: { tkl: 104, sck: 5, int: 9, td: 2 } } };
  const { tiles, footnote } = careerTiles(career, { pass_yds: 3, pass_td: 2, pass_rtg: 5, rush_yds: 4, rec_yds: 6, total_td: 2, tkl: 8, sck: 5, def_int: null, pool: 214 }, 24);
  assert.deepEqual(tiles.map((t) => t.label), ['Passing Yards', 'Passing TD', 'Passer Rating', 'Rushing Yards', 'Receiving Yards', 'Total TD', 'Tackles', 'Sacks', 'Interceptions']);
  assert.deepEqual(tiles[0], { key: 'pass_yds', label: 'Passing Yards', value: '3,021', rank: '#3', perGame: '125.9 / G' });
  assert.deepEqual(tiles[2], { key: 'pass_rtg', label: 'Passer Rating', value: '104.2', rank: '#5', perGame: null });
  assert.equal(tiles[5].value, '36', 'Total TD = rushing + receiving + defensive');
  assert.equal(tiles[8].rank, null);
  assert.equal(footnote, 'Rank among 214 online players');
  const bare = careerTiles(career, undefined, undefined);
  assert.ok(bare.tiles.every((t) => t.rank === null && t.perGame === null));
  assert.equal(bare.footnote, null);
  assert.equal(careerTiles({}, null, 10).tiles[0].value, '—');
});

test('leaderboard rows: "You" only on your own profile', () => {
  const around = [{ rank: 6, display_name: 'Lo', rating: 1197, is_player: false, player_id: 'a' }, { rank: 7, display_name: 'adam', rating: 1184, is_player: true, player_id: 'b' }];
  assert.deepEqual(leaderboardRows(around, { self: true }).map((r) => [r.rank, r.name, r.elo, r.me]), [['#6', 'Lo', '1,197', false], ['#7', 'You', '1,184', true]]);
  assert.equal(leaderboardRows(around, { self: false })[1].name, 'adam');
  assert.deepEqual(leaderboardRows(undefined), []);
});

test('full stats: columns, values, an older server\'s yac for YCO, top five', () => {
  assert.deepEqual(FULL_STATS.passing.cols.map((c) => c.label), ['C/ATT', 'YDS', 'TD', 'INT', 'AIR', 'YAC', 'Y/A', 'RTG', 'LNG']);
  assert.deepEqual(FULL_STATS.rushing.cols.map((c) => c.label), ['CAR', 'YDS', 'AVG', 'TD', 'LNG', 'BTK', 'YCO']);
  assert.deepEqual(FULL_STATS.receiving.cols.map((c) => c.label), ['REC', 'YDS', 'AVG', 'YAC', 'TD', 'LNG', 'DRP']);
  assert.deepEqual(FULL_STATS.defense.cols.map((c) => c.label), ['TKL', 'TFL', 'SCK', 'INT', 'PBU', 'FF', 'FR', 'TD']);
  const [ca, yds] = FULL_STATS.passing.cols;
  assert.equal(cellValue(ca, { cmp: 141, att: 238 }), '141/238');
  assert.equal(cellValue(yds, { yds: 2210 }), '2,210');
  assert.equal(cellValue(FULL_STATS.passing.cols[7], { rtg: 110.5 }), '110.5');
  assert.equal(cellValue(FULL_STATS.passing.cols[7], {}), '—');
  const yco = FULL_STATS.rushing.cols[6];
  assert.equal(cellValue(yco, { yco: 540 }), '540');
  assert.equal(cellValue(yco, { yac: 530 }), '530', 'an older server still calls it yac');
  const { top, rest } = splitTop([1, 2, 3, 4, 5, 6, 7]);
  assert.deepEqual([top.length, rest.length], [5, 2]);
});
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `node --test crates/reborn-ui/ui/profile-model.test.mjs`
Expected: FAIL (`Cannot find module … profile-model.js`).

- [ ] **Step 3: Write `profile-model.js`**

Create `crates/reborn-ui/ui/profile-model.js`:

```js
// Pure view models for the public profile, the online player hub (spec §8.7). No DOM. Each function takes the server's
// answer as it is and returns strings ready to draw, or null / [] when the server did not send what a piece needs, so
// the page leaves that piece out: nothing is invented (spec §8.7, "Degrades gracefully until the backend work ships").
// An older server's shapes are handled here too: private rooms (backend §1) and rushing `yac` for YCO (backend §9).
import { eloChange, fmtInt, fmtPct } from './stats-model.js';

const DASH = '—', EN = '–', MINUS = '−';
const num = (n) => typeof n === 'number' && Number.isFinite(n);
const one = (n) => n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
const sum = (...xs) => (xs.some(num) ? xs.reduce((a, x) => a + (num(x) ? x : 0), 0) : null);

/** Private rooms never show on a profile (backend §1; the deployed server still lists them). */
export const isPublic = (m) => Boolean(m) && m.queue !== 'private';
export function publicQueues(rows) {
  return (Array.isArray(rows) ? rows : []).filter((r) => r && r.key !== 'private' && !/private/i.test(String(r.name ?? '')));
}

export const winRate = (p) => (num(p) ? `${one(p * 100)}%` : null);

/** The card's one summary line: "#7 on the leaderboard | 1,184 Elo | 60.9% win rate" (owner round 11). */
export function summaryFacts(p) {
  const out = [];
  if (num(p?.elo?.rank)) out.push({ strong: `#${p.elo.rank}`, text: 'on the leaderboard' });
  if (num(p?.elo?.rating)) out.push({ strong: fmtInt(p.elo.rating), text: 'Elo' });
  const w = winRate(p?.summary?.win_pct);
  if (w) out.push({ strong: w, text: 'win rate' });
  return out;
}

/** The live card (`header.live`, backend §7): "Ranked · Game 2 of 3", "14 – 7", "vs Mikel · Series 1–0". No halves. */
export function liveCard(live) {
  if (!live || typeof live !== 'object' || live.queue === 'private' || !live.match_id) return null;
  const queue = live.queue === 'ranked' ? 'Ranked' : live.queue === 'casual' ? 'Casual' : 'Online';
  const bo = num(live.best_of) ? live.best_of : 1;
  const head = bo > 1 && num(live.game_number) ? `${queue} · Game ${live.game_number} of ${bo}` : queue;
  const score = Array.isArray(live.score) && num(live.score[0]) && num(live.score[1]) ? `${live.score[0]} ${EN} ${live.score[1]}` : null;
  const parts = [];
  if (live.opponent?.display_name) parts.push(`vs ${live.opponent.display_name}`);
  if (bo > 1 && Array.isArray(live.series) && num(live.series[0]) && num(live.series[1])) parts.push(`Series ${live.series[0]}${EN}${live.series[1]}`);
  return { head, me: live.me || null, opp: live.opp || null, score, line: parts.join(' · '), href: `live.html?match=${encodeURIComponent(live.match_id)}` };
}

/** "● In a match" while a (public) match runs, "● Online now" when online, else nothing. */
export function statusLine(header) {
  if (liveCard(header?.live)) return { text: 'In a match', kind: 'live' };
  if (header?.online === true) return { text: 'Online now', kind: 'online' };
  return null;
}

/** An area sparkline in a w×h box: the line's points and the filled area's path. */
export function areaPoints(values, w = 300, h = 70, pad = 6) {
  if (!Array.isArray(values) || values.length < 2) return null;
  const lo = Math.min(...values), hi = Math.max(...values);
  const r1 = (x) => Math.round(x * 10) / 10;
  const pts = values.map((v, i) => `${r1((i / (values.length - 1)) * w)},${r1(hi === lo ? h / 2 : pad + (h - 2 * pad) * (1 - (v - lo) / (hi - lo)))}`);
  return { line: pts.join(' '), area: `M${pts.join(' L')} L${w},${h} L0,${h}Z` };
}

/** Overview column 1: "1,184 +16", "Peak 1,212 · started at 1,000". The peak comes from the server (backend §11) or
 *  from the history it sent. */
export function eloOverview(elo) {
  if (!num(elo?.rating)) return null;
  const hist = (Array.isArray(elo.history) ? elo.history : []).filter((x) => x && num(x.rating));
  const start = hist.length ? hist[0].rating - (num(hist[0].delta) ? hist[0].delta : 0) : elo.rating;
  const peak = num(elo.peak) ? elo.peak : Math.max(elo.rating, start, ...hist.map((x) => x.rating));
  const n = hist.length;
  return { value: fmtInt(elo.rating), change: eloChange(elo.last_delta), sub: `Peak ${fmtInt(peak)} · started at ${fmtInt(start)}`,
    note: `${n} ranked ${n === 1 ? 'match' : 'matches'}`, points: areaPoints([start, ...hist.map((x) => x.rating)]) };
}

/** Overview column 2: "14–9 60.9%", "Ranked and casual", then Ranked and Casual as label / bar / record / pct. */
export function recordOverview(summary, byQueue) {
  const s = summary || {};
  if (!num(s.wins) || !num(s.losses)) return null;
  const total = s.wins + s.losses;
  const rows = publicQueues(byQueue).filter((r) => r.key === 'ranked' || r.key === 'casual').map((r) => {
    const w = num(r.wins) ? r.wins : 0, l = num(r.losses) ? r.losses : 0, n = w + l;
    return { label: r.key === 'ranked' ? 'Ranked' : 'Casual', record: `${w}${EN}${l}`, pct: n ? fmtPct(w / n) : DASH, share: n ? Math.round((w / n) * 1000) / 10 : 0 };
  });
  return { value: `${s.wins}${EN}${s.losses}`, pct: winRate(s.win_pct) ?? DASH, note: `${total} ${total === 1 ? 'match' : 'matches'}`, sub: 'Ranked and casual', rows };
}

/** Overview column 3 (profile `recent_form`, backend §8): "6–4 +6.4", points scored and allowed per game. */
export function lastTenOverview(rf) {
  if (!rf || !num(rf.games) || rf.games === 0 || !num(rf.wins) || !num(rf.losses)) return null;
  const m = num(rf.avg_margin) ? rf.avg_margin : null;
  const margin = m === null ? { text: '', kind: '' } : { text: `${m > 0 ? '+' : m < 0 ? MINUS : ''}${one(Math.abs(m))}`, kind: m > 0 ? 'up' : m < 0 ? 'down' : '' };
  return { value: `${rf.wins}${EN}${rf.losses}`, margin, scored: num(rf.pts_for_pg) ? one(rf.pts_for_pg) : DASH, allowed: num(rf.pts_against_pg) ? one(rf.pts_against_pg) : DASH };
}

// Career tiles: [key (career_rank field), label, value from the career totals, per game?, one decimal?]
const TILES = [
  ['pass_yds', 'Passing Yards', (c) => c.passing?.total?.yds, true],
  ['pass_td', 'Passing TD', (c) => c.passing?.total?.td, true],
  ['pass_rtg', 'Passer Rating', (c) => c.passing?.total?.rtg, false, true],
  ['rush_yds', 'Rushing Yards', (c) => c.rushing?.total?.yds, true],
  ['rec_yds', 'Receiving Yards', (c) => c.receiving?.total?.yds, true],
  // The backend's total_td (§6) is rushing + receiving + defensive: a thrown TD is also a caught one.
  ['total_td', 'Total TD', (c) => sum(c.rushing?.total?.td, c.receiving?.total?.td, c.defense?.total?.td), true],
  ['tkl', 'Tackles', (c) => c.defense?.total?.tkl, true],
  ['sck', 'Sacks', (c) => c.defense?.total?.sck, true],
  ['def_int', 'Interceptions', (c) => c.defense?.total?.int, true],
];

/** The Career card: nine tiles, each "#3 · 125.9 / G" when the rank (backend §6) and a game count are known. */
export function careerTiles(career, rank, games) {
  const c = career || {};
  const tiles = TILES.map(([key, label, get, perGame, dec]) => {
    const v = get(c);
    return { key, label, value: num(v) ? (dec ? one(v) : fmtInt(v)) : DASH, rank: num(rank?.[key]) ? `#${rank[key]}` : null,
      perGame: perGame && num(v) && num(games) && games > 0 ? `${one(v / games)} / G` : null };
  });
  const pool = num(rank?.pool) && tiles.some((t) => t.rank) ? rank.pool : null;
  return { tiles, footnote: pool ? `Rank among ${fmtInt(pool)} online players` : null };
}

/** The Leaderboard card (`elo.around`, backend §5/§14). The profile's own player is marked; it reads "You" only when the
 *  viewer is that player. */
export function leaderboardRows(around, { self = false } = {}) {
  return (Array.isArray(around) ? around : []).filter((r) => r && num(r.rank)).map((r) => ({
    rank: `#${r.rank}`, name: r.is_player && self ? 'You' : String(r.display_name || 'Player'), elo: num(r.rating) ? fmtInt(r.rating) : DASH,
    me: Boolean(r.is_player), avatar_url: r.avatar_url ?? null, player_id: typeof r.player_id === 'string' ? r.player_id : null,
    letterOf: String(r.display_name || 'Player'),
  }));
}

const col = (label, title, key, extra = {}) => ({ label, title, key, ...extra });
/** Full stats (spec §8.7.5): the four tables' columns, units uppercase. */
export const FULL_STATS = {
  passing: { title: 'Passing', cols: [
    col('C/ATT', 'Completions / attempts', 'cmp', { text: (r) => (num(r?.cmp) && num(r?.att) ? `${fmtInt(r.cmp)}/${fmtInt(r.att)}` : DASH) }),
    col('YDS', 'Passing yards', 'yds'), col('TD', 'Touchdowns', 'td'), col('INT', 'Interceptions', 'int'), col('AIR', 'Air yards', 'air'),
    col('YAC', 'Yards after catch', 'yac'), col('Y/A', 'Yards per attempt', 'ypa', { dec: true }), col('RTG', 'Passer rating', 'rtg', { dec: true }),
    col('LNG', 'Longest', 'long')] },
  rushing: { title: 'Rushing', cols: [
    col('CAR', 'Carries', 'att'), col('YDS', 'Rushing yards', 'yds'), col('AVG', 'Yards per carry', 'avg', { dec: true }), col('TD', 'Touchdowns', 'td'),
    col('LNG', 'Longest', 'long'), col('BTK', 'Broken tackles', 'btk'),
    col('YCO', 'Yards after contact', 'yco', { get: (r) => (num(r?.yco) ? r.yco : r?.yac) })] },
  receiving: { title: 'Receiving', cols: [
    col('REC', 'Receptions', 'rec'), col('YDS', 'Receiving yards', 'yds'), col('AVG', 'Yards per catch', 'avg', { dec: true }),
    col('YAC', 'Yards after catch', 'yac'), col('TD', 'Touchdowns', 'td'), col('LNG', 'Longest', 'long'), col('DRP', 'Drops', 'drops')] },
  defense: { title: 'Defense', cols: [
    col('TKL', 'Tackles', 'tkl'), col('TFL', 'Tackles for loss', 'tfl'), col('SCK', 'Sacks', 'sck'), col('INT', 'Interceptions', 'int'),
    col('PBU', 'Passes broken up', 'pbu'), col('FF', 'Forced fumbles', 'ff'), col('FR', 'Fumble recoveries', 'fr'), col('TD', 'Defensive touchdowns', 'td')] },
};

export function sortValue(c, r) { return c.get ? c.get(r) : r?.[c.key]; }
export function cellValue(c, r) {
  if (c.text) return c.text(r);
  const v = sortValue(c, r);
  if (!num(v)) return DASH;
  return c.dec ? one(v) : fmtInt(Math.round(v));
}
export function splitTop(rows, n = 5) {
  const list = Array.isArray(rows) ? rows : [];
  return { top: list.slice(0, n), rest: list.slice(n) };
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `node --test crates/reborn-ui/ui/profile-model.test.mjs crates/reborn-ui/ui/stats-model.test.mjs`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/profile-model.js crates/reborn-ui/ui/profile-model.test.mjs
git commit -m "profile-model: pure view models for the online player hub, degrading on an older server

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: The Profile page: card, live card, overview, Match history, Career, Leaderboard, Records

**Files:**
- Create: `crates/reborn-ui/ui/pages/profile.css`, `Tools/check-profile-page.mjs`
- Modify: `crates/reborn-ui/ui/profile.html`, `Tools/check-theme.mjs`
- Rewrite: `crates/reborn-ui/ui/profile.js`

**Interfaces:**
- Consumes: Task 2's classes; `match-rows.js` (Task 4); `profile-model.js` (Task 5); `tabs()`; `dominantColors` (`cover-color.js`); `notFound` (`live-game-render.js`).
- Produces, for Task 7:
  - In `profile.js`: `draw(p, ownerId)` holds the comment line `  // More sections below the columns (plan 4 Task 7)` right before `screen.replaceChildren(...out);`; a function `jump()` (scrolls to `location.hash`); a function `careerSection(p)` whose head is `sectionHead('career', 'Career')`; helpers `sectionHead(id, title, extra = null)`, `num`, `list`, `int`, `day`, `profileHref`, `gameHref`, `DASH`, `svg`.
  - In `check-profile-page.mjs`: two marker comments, `    // (Task 7 adds the older server's rushing table here)` and `  // (Task 7 adds the Full stats checks here)`.
  - Section ids: `#history`, `#career`, `#leaderboard`, `#records`.

- [ ] **Step 1: Write the failing page check**

Create `Tools/check-profile-page.mjs`:

```js
// The Profile page, the online player hub (spec §8.7, §10, §13): Back first; the card (status, summary line, Share /
// View on web, the live card); the overview (three columns, big numbers on one line, visuals ending on one line); Match
// history beside Career / Leaderboard / Records (first cards start and end on one line; 40px headers); the queue filter
// and the pager; W/L only, never "Best of 3", "ladder" or "Private"; the deployed server's answers; the small window.
//     node Tools/check-profile-page.mjs [outdir]      (screens in target/p4-after by default)
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { serve, openState, root } from './lib/page-states.mjs';
import * as F from './fixtures/stats-fixtures.mjs';

const out = path.resolve(process.argv[2] || path.join(root, 'target/p4-after'));
mkdirSync(out, { recursive: true });
const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const STATE = { name: 'profile', url: `profile.html?player=${F.ME}`, scope: 'main' };
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, h: r.height }; });
const texts = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => n.textContent.trim()));
const noSideways = async (page) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
const contentRules = async (page) => {
  const body = await page.locator('#screen').innerText();
  assert.doesNotMatch(body, /best of 3/i, 'never "Best of 3"');
  assert.doesNotMatch(body, /ladder/i, '"Leaderboard", never "ladder"');
  assert.doesNotMatch(body, /private/i, 'no "Private" on Profile');
  assert.ok((await texts(page, '.m-res b')).every((t) => t === 'W' || t === 'L' || t === ''), 'results are W or L');
};
try {
  { // 1280×800, the redesign's full answers, the owner looking at their own profile
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('#screen > :first-child').getAttribute('id'), 'back', 'Back comes first');
    assert.equal(await page.locator('.pf-name').textContent(), 'adamknowsball');
    assert.equal(await page.locator('.pf-status').textContent(), '● In a match');
    assert.equal(await page.locator('.pf-pic .presence-dot').count(), 1);
    assert.deepEqual(await texts(page, '.pf-sum > span'), ['#7 on the leaderboard', '1,184 Elo', '60.9% win rate']);
    assert.deepEqual(await texts(page, '.pf-acts .btn'), ['Share profile', 'View on web']);
    assert.equal(await page.locator('#plays-as').count(), 1, 'the owner picks Plays as');
    const pic = await rect(page, '.pf-pic'), id = await rect(page, '.pf-id');
    assert.equal(Math.round(pic.h), 104);
    assert.ok(Math.abs((pic.top + pic.bottom) / 2 - (id.top + id.bottom) / 2) < 2, 'the avatar is centred on the identity block');
    // the live card
    assert.equal(await page.locator('.pf-live .q').textContent(), 'Ranked · Game 2 of 3');
    assert.equal(await page.locator('.pf-live strong').textContent(), '14 – 7');
    assert.equal(await page.locator('.pf-live-bot > span').textContent(), 'vs Mikel · Series 1–0');
    // overview: three identical columns; big numbers on one line; the visuals end on one line
    assert.deepEqual(await texts(page, '.pf-ovh h2'), ['Elo', 'Record', 'Last 10 games']);
    assert.deepEqual(await texts(page, '.pf-big'), ['1,184+16', '14–960.9%', '6–4+6.4']);
    const bigs = await page.locator('.pf-big').evaluateAll((ns) => ns.map((n) => Math.round(n.getBoundingClientRect().top)));
    assert.equal(new Set(bigs).size, 1, `the big numbers share a line: ${bigs}`);
    const ends = await page.evaluate(() => ['.pf-spark', '.pf-bars', '.pf-pair'].map((s) => Math.round(document.querySelector(s).getBoundingClientRect().bottom)));
    assert.ok(Math.max(...ends) - Math.min(...ends) <= 1, `the visuals end on one line: ${ends}`);
    assert.deepEqual(await texts(page, '.pf-bar-row > :first-child'), ['Ranked', 'Casual']);
    assert.deepEqual(await texts(page, '.pf-bar-row b'), ['8–5', '6–4']);
    // two columns: first cards start together and the columns end together; 40px section headers
    const hl = await rect(page, '#history .m-list');
    const first = await rect(page, '.pf-side > section:first-child > .glass');
    const last = await rect(page, '.pf-side > section:last-child > .glass');
    assert.ok(Math.abs(hl.top - first.top) < 1, `first cards start together: ${hl.top} vs ${first.top}`);
    assert.ok(Math.abs(hl.bottom - last.bottom) < 1, `the columns end together: ${hl.bottom} vs ${last.bottom}`);
    assert.deepEqual([...new Set(await page.locator('.sec-head').evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().height)))], [40]);
    // Match history: the queue as the second line, the pager in the card, more pages loaded as needed
    const rows = page.locator('#history .m-list > .m-row:not(.m-hd)');
    assert.equal(await rows.count(), 10);
    assert.equal(await page.locator('[data-key="match-m-01"] small').textContent(), 'Ranked');
    assert.equal(await page.locator('[data-key="match-m-02"] small').textContent(), 'Casual');
    assert.equal(await page.locator('#history .m-pager span').textContent(), 'Page 1');
    await page.locator('#history [data-key="older"]').click();
    await page.waitForFunction(() => document.querySelector('#history .m-pager span')?.textContent === 'Page 2 of 2');
    assert.equal(await rows.count(), 2);
    assert.equal(await page.locator('#history [data-key="older"]').isDisabled(), true);
    await page.locator('#history [data-key="newer"]').click();
    await page.getByRole('button', { name: 'Ranked', exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll('#history .m-list > .m-row:not(.m-hd)').length === 2);
    assert.deepEqual(await rows.evaluateAll((ns) => ns.map((n) => n.dataset.key)), ['match-m-01', 'match-m-04']);
    assert.equal(await page.locator('#history .m-pager').count(), 0, 'one page: no pager');
    await page.getByRole('button', { name: 'All', exact: true }).click();
    // Career: nine spelled-out tiles, rank among the player base, per game
    assert.deepEqual(await texts(page, '#career .k-label'), ['Passing Yards', 'Passing TD', 'Passer Rating', 'Rushing Yards', 'Receiving Yards', 'Total TD', 'Tackles', 'Sacks', 'Interceptions']);
    assert.equal(await page.locator('#career .pf-car > div b.num').first().textContent(), '3,021');
    assert.equal(await page.locator('#career .pf-car-s').first().textContent(), '#3 · 125.9 / G');
    assert.equal(await page.locator('#career .pf-note').textContent(), 'Rank among 214 online players');
    // Leaderboard: #5 to #9 around the player; their own row reads You
    assert.deepEqual(await texts(page, '.pf-lb-rank'), ['#5', '#6', '#7', '#8', '#9']);
    assert.equal(await page.locator('.pf-lb-row.me .pf-lb-name').textContent(), 'You');
    // Records: five, then All ›
    assert.equal(await page.locator('#records .pf-rec:not([hidden])').count(), 5);
    assert.equal(await page.locator('#records .sec-link').textContent(), 'All ›');
    await contentRules(page);
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'profile-1280.png') });
    assert.deepEqual(errors, []);
    // Live feed opens the native live view
    await page.getByRole('button', { name: 'Live feed', exact: true }).click();
    await page.waitForURL(/live\.html\?match=dc5226af-d3b8-4c0b-89e8-1c53b2972a30$/);
    await close();
  }
  // (Task 7 adds the Full stats checks here)
  { // online, not in a match: no live card; Back with nothing behind goes to Stats
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: F.profile({ live: false }) } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-live').count(), 0);
    assert.equal(await page.locator('.pf-status').textContent(), '● Online now');
    await page.screenshot({ path: path.join(out, 'profile-1280-idle.png') });
    await page.locator('#back').click();
    await page.waitForURL(/stats\.html$/);
    assert.deepEqual(errors, []);
    await close();
  }
  { // someone else's profile: their row keeps their name, and only the owner picks Plays as
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { identity_status: { signed_in: true, display_name: 'Playz', player_id: '1b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e', show_offline: false } } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-lb-row.me .pf-lb-name').textContent(), 'adamknowsball');
    assert.equal(await page.locator('#plays-as').count(), 0);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the deployed server: everything it does not send is left out, never faked; its private room is dropped
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: F.oldProfile(), player_profile_all: F.allMatches().filter((m) => m.mode === 'online') } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-live').count(), 0);
    assert.equal(await page.locator('.pf-status').textContent(), '● Online now');
    assert.deepEqual(await texts(page, '.pf-ovh h2'), ['Elo', 'Record'], 'no Last 10 games without recent_form');
    assert.ok((await texts(page, '#career .pf-car-s')).every((t) => !t.includes('#')), 'no ranks without career_rank');
    assert.equal(await page.locator('#career .pf-note').count(), 0);
    assert.equal(await page.locator('#leaderboard').count(), 0, 'no Leaderboard without elo.around');
    assert.deepEqual(await texts(page, '.pf-bar-row > :first-child'), ['Ranked', 'Casual']);
    assert.equal(await page.locator('[data-key="match-m-05"]').count(), 0, 'the private room is dropped');
    // (Task 7 adds the older server's rushing table here)
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window
    const { page, errors, close } = await openState(browser, srv.base, STATE, { viewport: { width: 800, height: 560 } });
    await page.locator('.pf-card').waitFor();
    await noSideways(page);
    assert.ok((await rect(page, '.pf-side')).top > (await rect(page, '#history')).bottom - 1, 'one column in a small window');
    await page.screenshot({ path: path.join(out, 'profile-800.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // a profile that does not exist
    const { page, errors, close } = await openState(browser, srv.base, { ...STATE, url: 'profile.html?player=nope' });
    await page.getByText("That profile couldn't be found.").waitFor();
    assert.equal(await page.locator('#back').textContent(), 'Back to Stats');
    assert.deepEqual(errors, []);
    await close();
  }
  console.log(`check-profile-page: PASS (screens in ${out})`);
} finally { await browser.close(); await srv.close(); }
```

- [ ] **Step 2: Run it and confirm it fails**

Link Playwright. Run `node Tools/check-profile-page.mjs`.
Expected: FAIL waiting for `.pf-card` (the old page has no such element).

- [ ] **Step 3: Rewrite `profile.js`**

Replace the whole of `crates/reborn-ui/ui/profile.js` with:

```js
// The public profile, the online player hub (spec §8.7): one player's ONLINE games (`player_profile`), opened as
// profile.html?player=<uuid>[#section]. Top to bottom: Back · the profile card (the cover from the avatar's colours,
// avatar, name, status, Joined, Plays as, the summary line, Share profile / View on web, the live card while they
// play) · the overview (Elo | Record | Last 10 games) · Match history beside Career, Leaderboard and Records.
// A piece whose data the server does not send yet is left out, never drawn as a placeholder (spec §8.7). Private rooms
// never show (the deployed server still sends them; they are dropped on the way in).
// Everything drawn is server data: el()'s `text` (textContent) only, never markup. Images only through assetUrl() or
// avatarSrc() (the CSP's two image prefixes). Script-set styles are the card's --c1/--c2 and a bar's width, through the
// CSSOM, never a style attribute.
import { bridge, call, el, markCurrent, quietCall, reason, wireCoreFold } from './shell.js';
import { PAGE_ORIGIN, avatarSrc } from './live-common.js';
import { notFound } from './live-game-render.js';
import { dominantColors } from './cover-color.js';
import { fmtInt } from './stats-model.js';
import { avatar, matchHeader, matchRows, pager, storeGameList, teamLogo } from './match-rows.js';
import { careerTiles, eloOverview, isPublic, lastTenOverview, leaderboardRows, liveCard, recordOverview, statusLine, summaryFacts } from './profile-model.js';
import { tabs } from './tabs.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const SHOWN = 5;     // a long list shows five rows; "All ›" shows every row in place
const HPAGE = 10;    // Match history rows per page
const DASH = '—';
const SVG_NS = 'http://www.w3.org/2000/svg';
const HISTORY_TABS = [['all', 'All'], ['ranked', 'Ranked'], ['casual', 'Casual']];
const PAGER_OTHER = { older: 'newer', newer: 'older' };

const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const playerId = (params.get('player') || '').toLowerCase();
const screen = document.getElementById('screen');

const num = (n) => typeof n === 'number' && Number.isFinite(n);
const list = (v) => (Array.isArray(v) ? v : []);
const int = (n) => fmtInt(num(n) ? Math.round(n) : null);
const day = (iso) => { const d = new Date(iso); return Number.isFinite(d.getTime()) ? d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) : DASH; };
const monthYear = (iso) => { const d = new Date(iso); return Number.isFinite(d.getTime()) ? d.toLocaleDateString('en-US', { month: 'short', year: 'numeric' }) : null; };
const profileHref = (id) => `profile.html?player=${encodeURIComponent(id)}`;
const gameHref = (id) => `game.html?game=${encodeURIComponent(id)}`;
const webUrl = () => `${PAGE_ORIGIN}/u/${playerId}`;

function svg(tag, attrs = {}) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  return node;
}

/** A section's 40px header: its <h2> and, on the right, a link or tabs. */
function sectionHead(id, title, extra = null) {
  return el('div', { class: 'sec-head' }, el('h2', { id: `${id}-h`, text: title }), extra);
}

/** Hide every row after the fifth; "All ›" shows them in place, "Fewer ‹" hides them again. */
function expandable(id, head, rows, what) {
  if (rows.length <= SHOWN) return;
  const set = (open) => rows.slice(SHOWN).forEach((r) => { r.hidden = !open; });
  set(false);
  const b = el('button', { type: 'button', class: 'sec-link', 'aria-expanded': 'false', 'aria-controls': id, text: 'All ›', 'aria-label': `Show all ${rows.length} ${what}` });
  b.addEventListener('click', () => {
    const open = b.getAttribute('aria-expanded') !== 'true';
    set(open);
    b.setAttribute('aria-expanded', String(open));
    b.textContent = open ? 'Fewer ‹' : 'All ›';
    b.setAttribute('aria-label', open ? `Show the first ${SHOWN} ${what}` : `Show all ${rows.length} ${what}`);
  });
  head.append(b);
}

// ---- Back ----

// Back returns to the launcher page this one was opened from (Stats, a game, the live view, another profile); opened
// any other way, it goes to Stats.
function backButton() {
  const b = el('button', { type: 'button', class: 'pf-back', id: 'back', 'data-key': 'back' }, el('span', { class: 'chev', 'aria-hidden': 'true' }), el('span', { text: 'Back' }));
  b.addEventListener('click', () => {
    let fromLauncher = false;
    try { fromLauncher = Boolean(document.referrer) && new URL(document.referrer).origin === location.origin; } catch { /* no referrer */ }
    if (fromLauncher && history.length > 1) history.back(); else location.href = 'stats.html';
  });
  return b;
}

// ---- the profile card ----

function playsAsText(team) {
  if (!team || !team.name) return null;
  return el('span', { class: 'pf-team' }, el('span', { text: 'Plays as' }), teamLogo(team), el('span', { text: team.name }));
}

/** The owner's own "Plays as" choice: "Most played" (the server's pick) or any team in By team except Pickup. */
function playsAsPicker(p) {
  const h = p.header || {};
  const teams = list(p.by_team).filter((t) => t && t.key !== 'pickup' && Number.isInteger(t.tgid));
  const mostPlayed = () => { const t = teams.find((x) => (x.games ?? 0) > 0); return t ? { name: t.name, abbrev: t.abbrev, tgid: t.tgid, logo_url: t.logo_url } : null; };
  const current = h.plays_as_auto === false && Number.isInteger(h.plays_as?.tgid) ? String(h.plays_as.tgid) : '';
  const select = el('select', { id: 'plays-as', 'data-key': 'plays-as' },
    el('option', { value: '', text: 'Most played' }),
    ...teams.map((t) => el('option', { value: String(t.tgid), text: t.name })));
  select.value = current;
  const logoSlot = el('span', { class: 'pf-team-logo' }, h.plays_as ? teamLogo(h.plays_as) : null);
  const note = el('span', { class: 'pf-said', role: 'status' });
  let saved = current;
  select.addEventListener('change', async () => {
    const value = select.value;
    select.disabled = true;
    note.textContent = '';
    try {
      const reply = await call('set_plays_as', { tgid: value === '' ? null : Number(value) });
      if (reply && reply.ok === false) throw new Error(typeof reply.error === 'string' && reply.error ? reply.error : 'Your team could not be saved.');
      saved = value;
      const team = reply?.plays_as ?? (value === '' ? mostPlayed() : teams.find((t) => String(t.tgid) === value) ?? null);
      logoSlot.replaceChildren(...(team ? [teamLogo(team)] : []));
      note.textContent = 'Saved';
      setTimeout(() => { note.textContent = ''; }, 2000);
    } catch (e) {
      select.value = saved;
      note.textContent = reason(e);
    }
    select.disabled = false;
  });
  return el('span', { class: 'pf-team' }, el('label', { for: 'plays-as', text: 'Plays as' }), logoSlot, select, note);
}

/** The cover's two colours from the avatar; any failure (no avatar, CORS, a tainted canvas) keeps the neutral cover. */
function coverColours(target, url) {
  const src = avatarSrc({ avatar_url: url });
  if (!src || typeof Image === 'undefined') return;
  const im = new Image();
  im.crossOrigin = 'anonymous';
  im.decoding = 'async';
  im.onload = () => {
    try {
      const c = document.createElement('canvas');
      c.width = c.height = 32;
      const x = c.getContext('2d');
      x.drawImage(im, 0, 0, 32, 32);
      const cols = dominantColors(x.getImageData(0, 0, 32, 32).data);
      if (cols) { target.style.setProperty('--c1', cols[0]); target.style.setProperty('--c2', cols[1]); target.classList.add('has-colour'); }
    } catch { /* tainted canvas: keep the neutral cover */ }
  };
  im.src = src;
}

/** The live card, only while they play a public match (spec §8.7.2): LIVE · queue · game, the score, vs · series. */
function liveBlock(raw) {
  const c = liveCard(raw);
  if (!c) return null;
  const side = (t, right) => el('span', { class: `t${right ? ' r' : ''}` },
    ...(right ? [el('b', { text: t?.abbrev || '' }), teamLogo(t)] : [teamLogo(t), el('b', { text: t?.abbrev || '' })]));
  return el('div', { class: 'pf-live', role: 'group', 'aria-label': `Live now: ${c.head}` },
    el('div', { class: 'pf-live-top' }, el('span', { class: 'live-dot', 'aria-hidden': 'true' }), el('span', { class: 'lbl', text: 'Live' }), el('span', { class: 'q', text: c.head })),
    c.score ? el('div', { class: 'pf-live-sc' }, side(c.me, false), el('strong', { class: 'num', text: c.score }), side(c.opp, true)) : null,
    el('div', { class: 'pf-live-bot' }, el('span', { text: c.line }),
      el('button', { type: 'button', class: 'btn sm', 'data-key': 'live-feed', text: 'Live feed', onclick: () => { location.href = c.href; } })));
}

function card(p, ownerId) {
  const h = p.header || {};
  const name = h.display_name || 'Player';
  const status = statusLine(h);
  const pic = el('div', { class: 'pf-pic cover-pic' }, avatar(h.avatar_url, name, 'pf-avatar'), status ? el('span', { class: 'presence-dot', 'aria-hidden': 'true' }) : null);
  const meta = el('p', { class: 'pf-meta' });
  const bits = [];
  if (status) bits.push(el('span', { class: 'pf-status', text: `● ${status.text}` }));
  const joined = monthYear(h.joined);
  if (joined) bits.push(el('span', { text: `Joined ${joined}` }));
  const owner = Boolean(ownerId) && String(h.player_id || '').toLowerCase() === ownerId;
  const team = owner ? playsAsPicker(p) : playsAsText(h.plays_as);
  if (team) bits.push(team);
  bits.forEach((b, i) => { if (i) meta.append(el('span', { class: 'pf-dot', 'aria-hidden': 'true', text: '·' })); meta.append(b); });
  const facts = summaryFacts(p);
  const said = el('span', { class: 'pf-said', role: 'status' });
  let saidTimer = null;
  const share = el('button', { type: 'button', class: 'btn primary sm', 'data-key': 'share', text: 'Share profile' });
  share.addEventListener('click', async () => {
    const url = webUrl();
    try { await navigator.clipboard.writeText(url); said.textContent = 'Link copied'; } catch { said.textContent = url; }
    clearTimeout(saidTimer);
    saidTimer = setTimeout(() => { said.textContent = ''; }, 2000);
  });
  const web = el('button', { type: 'button', class: 'btn sm', 'data-key': 'web', text: 'View on web' });
  web.addEventListener('click', async () => {
    try { await call('open_website', { url: webUrl() }); } catch (e) { said.textContent = reason(e); }
  });
  const id = el('div', { class: 'pf-id' }, el('h2', { class: 'pf-name', text: name }), meta,
    facts.length ? el('p', { class: 'pf-sum' }, ...facts.map((f) => el('span', {}, el('b', { class: 'num', text: f.strong }), ` ${f.text}`))) : null,
    el('div', { class: 'pf-acts' }, share, web, said));
  const box = el('div', { class: 'glass pf-card' }, el('div', { class: 'cover-wash', 'aria-hidden': 'true' }), el('div', { class: 'pf-row' }, pic, id, liveBlock(h.live)));
  coverColours(box, h.avatar_url);
  if (typeof document.title === 'string') document.title = `${name} | NFL Street Reborn`;
  return box;
}

// ---- the overview: one surface, three identical columns (spec §8.7.3) ----

function areaChart(points, label) {
  if (!points || typeof document.createElementNS !== 'function') return null;
  const s = svg('svg', { class: 'pf-spark', viewBox: '0 0 300 70', preserveAspectRatio: 'none', role: 'img', 'aria-label': label });
  const defs = svg('defs'), grad = svg('linearGradient', { id: 'pf-elo-fill', x1: 0, x2: 0, y1: 0, y2: 1 });
  grad.append(svg('stop', { offset: 0, 'stop-color': 'currentColor', 'stop-opacity': 0.22 }), svg('stop', { offset: 1, 'stop-color': 'currentColor', 'stop-opacity': 0 }));
  defs.append(grad);
  s.append(defs, svg('path', { d: points.area, fill: 'url(#pf-elo-fill)' }),
    svg('polyline', { points: points.line, fill: 'none', stroke: 'currentColor', 'stroke-width': 2, 'stroke-linejoin': 'round', 'vector-effect': 'non-scaling-stroke' }));
  return s;
}

function ovColumn(title, note, big, side, kind, sub, visual) {
  return el('section', { class: 'pf-ovc' },
    el('div', { class: 'pf-ovh' }, el('h2', { text: title }), note ? el('span', { text: note }) : null),
    el('div', { class: 'pf-big num' }, el('span', { text: big }), side ? el('em', { class: kind || null, text: side }) : null),
    el('p', { class: 'pf-sub', text: sub }), visual);
}

function overview(p) {
  const cols = [];
  const e = eloOverview(p.elo);
  if (e) cols.push(ovColumn('Elo', e.note, e.value, e.change.text, e.change.kind, e.sub, areaChart(e.points, `Elo over ${e.note}, now ${e.value}`)));
  const r = recordOverview(p.summary, p.by_queue);
  if (r) cols.push(ovColumn('Record', r.note, r.value, r.pct, '', r.sub, r.rows.length ? el('div', { class: 'pf-bars' }, ...r.rows.map((row) => {
    const fill = el('i');
    if (fill.style) fill.style.width = `${row.share}%`;
    return el('div', { class: 'pf-bar-row' }, el('span', { text: row.label }), el('span', { class: 'pf-bar', role: 'img', 'aria-label': `${row.label}: ${row.record}` }, fill),
      el('b', { class: 'num', text: row.record }), el('span', { class: 'pf-mute num', text: row.pct }));
  })) : null));
  const l = lastTenOverview(p.recent_form);
  if (l) cols.push(ovColumn('Last 10 games', null, l.value, l.margin.text, l.margin.kind, 'Average winning margin', el('div', { class: 'pf-pair' },
    el('div', {}, el('span', { text: 'Points scored' }), el('b', { class: 'num', text: l.scored })),
    el('div', {}, el('span', { text: 'Points allowed' }), el('b', { class: 'num', text: l.allowed })))));
  if (!cols.length) return null;
  return el('div', { class: `glass pf-ov cols-${cols.length}` }, ...cols);
}

// ---- Match history: the Stats rows, the queue as the second line; All / Ranked / Casual (spec §8.7.4) ----
// `player_profile` takes no queue, so the filter runs here, over the matches loaded so far, and "Older" loads more
// pages (`before`) until the page on screen is full or the server has no more.

const hist = { filter: 'all', page: 0, loaded: [], cursor: null, total: 0, done: false, busy: false, error: '', open: new Set(), render: () => {} };
const shownMatches = () => hist.loaded.filter((m) => hist.filter === 'all' || m.queue === hist.filter);

async function fill() {
  if (hist.busy) return;
  const need = () => shownMatches().length < (hist.page + 1) * HPAGE;
  if (hist.done || !need()) return;
  hist.busy = true; hist.error = ''; hist.render();
  try {
    while (!hist.done && need()) {
      const page = await quietCall('player_profile', { playerId, before: hist.cursor });
      if (page && page.ok === false) throw new Error(typeof page.error === 'string' && page.error ? page.error : 'More matches could not be loaded.');
      const raw = list(page?.matches);
      hist.loaded.push(...raw.filter(isPublic));
      if (raw.length) hist.cursor = raw.at(-1).at;
      if (!raw.length || !hist.cursor) hist.done = true;
    }
  } catch (e) { hist.error = `More matches could not be loaded: ${reason(e)}`; }
  hist.busy = false;
  hist.render();
}

function historySection(p) {
  const raw = list(p.recent_matches);
  Object.assign(hist, { filter: 'all', page: 0, loaded: raw.filter(isPublic), cursor: raw.at(-1)?.at ?? null,
    total: num(p.matches_total) ? p.matches_total : raw.length, busy: false, error: '' });
  hist.done = raw.length >= hist.total || !hist.cursor;
  const label = `${p.header?.display_name || 'Player'}'s matches`;
  const openGame = (id) => { storeGameList(hist.loaded, label, profileHref(playerId)); location.href = gameHref(id); };
  const head = el('div', { class: 'sec-head' });
  const cardEl = el('div', { class: 'glass m-list' });
  hist.render = () => {
    const tabFocus = typeof head.contains === 'function' && head.contains(document.activeElement);
    const active = typeof cardEl.contains === 'function' && cardEl.contains(document.activeElement) ? document.activeElement?.getAttribute('data-key') : null;
    head.replaceChildren(el('h2', { id: 'history-h', text: 'Match history' }),
      tabs('profile-history', HISTORY_TABS.map(([, l]) => l), HISTORY_TABS.find(([k]) => k === hist.filter)[1],
        (l) => { hist.filter = HISTORY_TABS.find(([, x]) => x === l)[0]; hist.page = 0; hist.render(); fill(); }, { label: 'Show matches' }));
    const rows = shownMatches();
    const pageRows = rows.slice(hist.page * HPAGE, (hist.page + 1) * HPAGE);
    cardEl.replaceChildren(matchHeader());
    if (pageRows.length) cardEl.append(...pageRows.flatMap((m) => matchRows(m, { where: 'profile', open: hist.open, openGame })));
    else cardEl.append(el('p', { class: 'm-empty', text: hist.busy ? 'Loading…' : 'No matches of that kind yet.' }));
    if (hist.error) cardEl.append(el('p', { class: 'pf-err', role: 'status', text: hist.error }));
    const more = (hist.page + 1) * HPAGE < rows.length || !hist.done;
    if (hist.page > 0 || more) cardEl.append(pager({ page: hist.page, pages: hist.done ? Math.max(1, Math.ceil(rows.length / HPAGE)) : null,
      onNewer: () => { hist.page--; hist.render(); }, onOlder: () => { hist.page++; hist.render(); fill(); }, olderDisabled: !more || hist.busy }));
    if (tabFocus) head.querySelector('[aria-pressed="true"]')?.focus({ preventScroll: true });
    else if (active) {
      const all = Array.from(cardEl.querySelectorAll('[data-key]'));
      let t = all.find((n) => n.getAttribute('data-key') === active);
      if (t?.disabled && PAGER_OTHER[active]) t = all.find((n) => n.getAttribute('data-key') === PAGER_OTHER[active]) ?? t;
      t?.focus({ preventScroll: true });
    }
  };
  hist.render();
  return el('section', { id: 'history', class: 'pf-col', 'aria-labelledby': 'history-h' }, head, cardEl);
}

// ---- the right column: Career, Leaderboard, Records ----

function careerSection(p) {
  const { tiles, footnote } = careerTiles(p.career, p.career_rank, p.per_game?.games);
  if (tiles.every((t) => t.value === DASH)) return null;
  const grid = el('div', { class: 'pf-car' }, ...tiles.map((t) => el('div', {},
    el('span', { class: 'k-label', text: t.label }), el('b', { class: 'num', text: t.value }),
    t.rank || t.perGame ? el('span', { class: 'pf-car-s num' }, t.rank ? el('b', { text: t.rank }) : null, t.rank && t.perGame ? ' · ' : null, t.perGame) : null)));
  return el('section', { id: 'career', 'aria-labelledby': 'career-h' }, sectionHead('career', 'Career'),
    el('div', { class: 'glass pf-pad' }, grid, footnote ? el('p', { class: 'pf-note', text: footnote }) : null));
}

function leaderboardSection(p, self) {
  const rows = leaderboardRows(p.elo?.around, { self });
  if (!rows.length) return null;
  return el('section', { id: 'leaderboard', 'aria-labelledby': 'leaderboard-h' }, sectionHead('leaderboard', 'Leaderboard'),
    el('div', { class: 'glass pf-lb' }, ...rows.map((r) => {
      const cells = [el('span', { class: 'pf-lb-rank num', text: r.rank }), avatar(r.avatar_url, r.letterOf), el('span', { class: 'pf-lb-name', text: r.name }), el('b', { class: 'num', text: r.elo })];
      if (r.player_id && !r.me && UUID_RE.test(r.player_id)) return el('a', { class: 'pf-lb-row', href: profileHref(r.player_id), 'data-key': `lb-${r.player_id}` }, ...cells);
      return el('div', { class: `pf-lb-row${r.me ? ' me' : ''}` }, ...cells);
    })));
}

function recordsSection(p) {
  const rows = list(p.records).filter((r) => r && r.label);
  if (!rows.length) return null;
  const head = sectionHead('records', 'Records');
  const nodes = rows.map((r) => {
    const cells = [el('span', { class: 'k', text: r.label }),
      r.name ? el('span', { class: 'who' }, teamLogo({ name: r.team, logo_url: r.logo_url, abbrev: r.abbrev }), el('span', { text: r.name })) : el('span'),
      el('b', { class: 'num', text: int(r.value) }), el('span', { class: 'pf-mute num', text: day(r.at) })];
    const label = `${r.label}: ${int(r.value)}${r.name ? `, ${r.name}` : ''}, ${day(r.at)}`;
    return r.game_id
      ? el('a', { class: 'pf-rec', href: gameHref(r.game_id), 'aria-label': `${label}. Open the game` }, ...cells)
      : el('div', { class: 'pf-rec', role: 'group', 'aria-label': label }, ...cells);
  });
  const body = el('div', { class: 'glass pf-recs', id: 'records-rows' }, ...nodes);
  expandable('records-rows', head, nodes, 'records');
  return el('section', { id: 'records', 'aria-labelledby': 'records-h' }, head, body);
}

// ---- the page ----

function jump() {
  const hash = typeof location !== 'undefined' ? decodeURIComponent(location.hash || '').slice(1) : '';
  if (hash) document.getElementById(hash)?.scrollIntoView?.({ block: 'start' });
}

function draw(p, ownerId) {
  const self = Boolean(ownerId) && ownerId === String(p.header?.player_id || '').toLowerCase();
  const out = [backButton(), card(p, ownerId)];
  if (!(num(p.matches_total) && p.matches_total > 0)) {
    out.push(el('p', { class: 'pf-none', text: 'No online games yet.' }));
    screen.replaceChildren(...out);
    return;
  }
  const ov = overview(p);
  if (ov) out.push(ov);
  out.push(el('div', { class: 'pf-cols' }, historySection(p),
    el('div', { class: 'pf-side' }, ...[careerSection(p), leaderboardSection(p, self), recordsSection(p)].filter(Boolean))));
  // More sections below the columns (plan 4 Task 7)
  screen.replaceChildren(...out);
  jump();
}

function missing() {
  const nodes = notFound('stats.html', 'Back to Stats');
  nodes[0].textContent = "That profile couldn't be found.";
  screen.replaceChildren(...nodes);
}

async function load() {
  if (!screen) return;
  if (!UUID_RE.test(playerId)) { missing(); screen.removeAttribute?.('aria-busy'); return; }
  if (!bridge) { screen.replaceChildren(el('p', { class: 'refusal', role: 'alert', text: 'Open this page inside the launcher window.' })); return; }
  const [profile, identity] = await Promise.allSettled([call('player_profile', { playerId }), quietCall('identity_status')]);
  const ownerId = identity.status === 'fulfilled' && identity.value?.signed_in && typeof identity.value.player_id === 'string' ? identity.value.player_id.toLowerCase() : null;
  if (profile.status === 'rejected') {
    screen.replaceChildren(el('p', { class: 'refusal', role: 'alert', text: `That profile could not be loaded: ${reason(profile.reason)}` }),
      el('a', { class: 'button', id: 'back', href: 'stats.html', text: 'Back to Stats' }));
  } else if (!profile.value || typeof profile.value !== 'object' || profile.value.ok === false || !profile.value.header) missing();
  else draw(profile.value, ownerId);
  screen.removeAttribute?.('aria-busy');
}

markCurrent();
wireCoreFold();
load();
```

- [ ] **Step 4: Write `pages/profile.css`**

Create `crates/reborn-ui/ui/pages/profile.css`:

```css
/* Profile, the online player hub (spec §8.7). Layout only: every colour is a theme.css token (Tools/check-theme-tokens.mjs).
   The match rows, table, portrait, cover wash and dots are theme.css components. */
body.page-profile #main{padding:36px 36px 32px}
/* the page title stays for screen readers; the page starts with Back and the card (mockup) */
.page-profile .page-heading{position:absolute;width:1px;height:1px;margin:-1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
.page-profile #screen{display:flex;flex-direction:column;gap:24px;min-width:0}
.page-profile #screen>.refusal,.page-profile .lv-empty{margin:0;color:var(--fg-2)}
.pf-back{all:unset;align-self:flex-start;display:inline-flex;align-items:center;gap:8px;font-size:13px;color:var(--fg-2);cursor:pointer}
.pf-back:hover{color:var(--fg)}
.pf-back .chev{margin:0;transform:rotate(135deg)}
/* the profile card: the cover wash behind the whole card, the avatar centred on the identity block, the live card right */
.pf-card{position:relative;overflow:hidden}
.pf-row{position:relative;display:grid;grid-template-columns:auto minmax(0,1fr) auto;align-items:center;gap:24px;padding:24px}
.pf-pic{position:relative;display:grid;place-items:center;width:104px;height:104px;border-radius:50%}
.pf-pic>.pf-avatar{width:104px;height:104px;font-size:42px;background:transparent}
.pf-name{font:700 28px/1.15 Inter,system-ui,sans-serif;letter-spacing:-.01em}
.pf-meta{display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin-top:8px;font-size:12.5px;color:var(--fg-2)}
.pf-dot{color:var(--fg-3)}
.pf-status{font-weight:600;color:var(--ok)}
.pf-team{display:inline-flex;align-items:center;gap:8px}
.pf-team .team-logo{width:18px;height:18px}
.pf-team select{height:28px;padding:0 8px;font-size:12.5px}
.pf-said{font-size:12px;color:var(--fg-2)}
.pf-sum{display:flex;flex-wrap:wrap;margin-top:12px;font-size:13.5px;color:var(--fg-2)}
.pf-sum>span{display:inline-flex;align-items:baseline;gap:6px;padding:0 16px;line-height:20px}
.pf-sum>span:first-child{padding-left:0}
.pf-sum>span+span{border-left:1px solid var(--stroke-hi)}
.pf-sum b{font:700 15px Inter,system-ui,sans-serif;color:var(--fg)}
.pf-acts{display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin-top:16px}
.pf-live{width:260px;padding:12px 14px;border-radius:12px;background:var(--on-art);border:1px solid color-mix(in srgb,var(--red) 35%,transparent);backdrop-filter:blur(16px);-webkit-backdrop-filter:blur(16px)}
.pf-live-top{display:flex;align-items:center;gap:8px;font-size:11.5px;color:var(--fg-2)}
.pf-live-top .lbl{font:700 11px Inter,system-ui,sans-serif;letter-spacing:.08em;text-transform:uppercase;color:var(--red-ink)}
.pf-live-top .q{margin-left:auto}
.pf-live-sc{display:grid;grid-template-columns:1fr auto 1fr;align-items:center;gap:10px;margin:10px 0}
.pf-live-sc .t{display:flex;align-items:center;gap:6px;font-size:12px;color:var(--fg-2)}
.pf-live-sc .t b{font-weight:600}
.pf-live-sc .t.r{justify-content:flex-end}
.pf-live-sc .team-logo{width:28px;height:28px}
.pf-live-sc strong{font:700 22px Inter,system-ui,sans-serif}
.pf-live-bot{display:flex;align-items:center;justify-content:space-between;gap:8px;font-size:12.5px;color:var(--fg-2)}
/* the overview: one surface, identical columns; the big numbers share a line, the visuals end on one line */
.pf-ov{display:grid;grid-template-columns:repeat(3,minmax(0,1fr))}
.pf-ov.cols-2{grid-template-columns:repeat(2,minmax(0,1fr))}
.pf-ov.cols-1{grid-template-columns:minmax(0,1fr)}
.pf-ovc{display:flex;flex-direction:column;min-width:0;padding:20px 24px}
.pf-ovc+.pf-ovc{border-left:1px solid var(--stroke)}
.pf-ovh{display:flex;align-items:baseline;justify-content:space-between;gap:8px}
.pf-ovh h2{font:600 14px/1.3 Inter,system-ui,sans-serif}
.pf-ovh span{font-size:12px;color:var(--fg-3)}
.pf-big{display:flex;align-items:baseline;gap:10px;margin-top:14px;font:700 34px/1.1 Inter,system-ui,sans-serif;letter-spacing:-.02em}
.pf-big em{font:600 15px Inter,system-ui,sans-serif;font-style:normal;letter-spacing:0;color:var(--fg-2)}
.pf-big em.up{color:var(--ok)}
.pf-big em.down{color:var(--red-ink)}
.pf-sub{margin-top:4px;font-size:12.5px;color:var(--fg-3)}
.pf-spark{display:block;box-sizing:content-box;width:100%;height:70px;margin-top:auto;padding-top:16px;color:var(--red)}
.pf-bars,.pf-pair{margin-top:auto;padding-top:16px}
.pf-bar-row{display:grid;grid-template-columns:62px minmax(0,1fr) 40px 40px;align-items:center;gap:10px;height:28px;font-size:13px}
.pf-bar{display:block;height:4px;border-radius:2px;overflow:hidden;background:color-mix(in srgb,var(--red) 35%,transparent)}
.pf-bar i{display:block;height:100%;background:var(--ok)}
.pf-bar-row b{font-weight:600;text-align:right}
.pf-mute{font-size:12px;text-align:right;color:var(--fg-3)}
.pf-pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}
.pf-pair span{display:block;font-size:12px;color:var(--fg-3)}
.pf-pair b{font:600 18px Inter,system-ui,sans-serif}
/* Match history beside Career / Leaderboard / Records: first cards start and end on one line */
.pf-cols{display:grid;grid-template-columns:minmax(0,1.5fr) minmax(340px,1fr);gap:24px;align-items:stretch}
.pf-col{display:flex;flex-direction:column;min-width:0}
.pf-col>.m-list{flex:1}
.pf-col .m-list>.m-row:not(.m-hd){flex:1 0 56px;height:auto;min-height:56px}
.pf-side{display:flex;flex-direction:column;gap:24px;min-width:0}
.pf-side>section{display:flex;flex-direction:column}
.pf-side>section:last-child{flex:1}
.pf-side>section:last-child>.glass{flex:1}
.pf-pad{padding:16px}
.pf-car{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:16px}
.pf-car b{display:block;margin-top:4px;font:600 18px Inter,system-ui,sans-serif}
.pf-car-s{display:block;margin-top:2px;font-size:11.5px;color:var(--fg-3)}
.pf-car-s b{display:inline;margin:0;font:600 11.5px Inter,system-ui,sans-serif;color:var(--fg)}
.pf-note{margin-top:14px;padding-top:12px;border-top:1px solid var(--stroke);font-size:11.5px;color:var(--fg-3)}
.pf-lb{display:flex;flex-direction:column;padding:0 16px}
.pf-lb-row{flex:1;display:grid;grid-template-columns:36px 28px minmax(0,1fr) auto;align-items:center;gap:12px;min-height:48px;font-size:13px;color:inherit;text-decoration:none}
.pf-lb-row+.pf-lb-row{border-top:1px solid var(--stroke)}
.pf-lb-row .avatar{width:28px;height:28px;font-size:12px}
.pf-lb-rank{color:var(--fg-3)}
.pf-lb-name{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.pf-lb-row.me{margin:0 -16px;padding:0 16px;background:color-mix(in srgb,var(--red) 8%,transparent);box-shadow:inset 3px 0 0 var(--red)}
.pf-lb-row.me .pf-lb-name{font-weight:600}
a.pf-lb-row:hover .pf-lb-name{text-decoration:underline}
.pf-recs{display:flex;flex-direction:column;padding:0 16px}
.pf-rec{flex:1;display:grid;grid-template-columns:minmax(0,1fr) auto 44px 48px;align-items:center;gap:12px;min-height:44px;font-size:12.5px;color:inherit;text-decoration:none}
.pf-rec[hidden]{display:none}
.pf-rec+.pf-rec{border-top:1px solid var(--stroke)}
.pf-rec .k{color:var(--fg-2)}
.pf-rec .who{display:inline-flex;align-items:center;gap:6px;min-width:0}
.pf-rec .who .team-logo{width:18px;height:18px}
.pf-rec b{font-weight:600;text-align:right}
.pf-none{margin:0;padding:16px 0;color:var(--fg-2)}
.pf-err{margin:0;padding:8px 16px;font-size:12px;color:var(--red-ink)}
@media (max-width:1100px){
  .pf-cols{grid-template-columns:minmax(0,1fr)}
  .pf-ov,.pf-ov.cols-2{grid-template-columns:minmax(0,1fr)}
  .pf-ovc+.pf-ovc{border-left:0;border-top:1px solid var(--stroke)}
  .pf-row{grid-template-columns:auto minmax(0,1fr)}
  .pf-live{grid-column:1/-1;width:auto}
}
@media (max-width:1000px){body.page-profile #main{padding:24px 24px 16px}}
@media (max-width:720px){.pf-row{grid-template-columns:minmax(0,1fr)}.pf-car{grid-template-columns:repeat(2,minmax(0,1fr))}}
```

- [ ] **Step 5: Put `profile.html` on the theme**

Run: `node Tools/theme-page.mjs profile pages/profile.css`
Expected: `profile: theme.css + pages/profile.css, body.themed`.

In `crates/reborn-ui/ui/profile.html` replace
```html
<section id="screen" class="live-view" aria-busy="true">
```
with
```html
<section id="screen" aria-busy="true">
```

In `Tools/check-theme.mjs`, add `profile: "pages/profile.css",` inside the `THEMED` object literal.

- [ ] **Step 6: Run the checks**

- `node Tools/check-profile-page.mjs`: PASS.
- `node Tools/check-stats-page.mjs`: PASS (the "Your profile ›" link and player links still point here).
- `node --test crates/reborn-ui/ui/profile-model.test.mjs`: PASS.
- The standing checks: PASS (`check-page-call-args` reads `player_profile {playerId, before}` and `set_plays_as {tgid}` as object literals).

Open `target/p4-after/profile-1280.png` beside `target/p4-before/profile-1280.png` and the mockup's Profile section. Check: Back, the card with the cover wash from the avatar side, the 104px avatar centred, one summary line with thin dividers, the live card on the right, the one-surface overview, the two columns ending together, W/L letters, no "Private", no glow. Also open `profile-1280-idle.png` and `profile-800.png`.

Unlink Playwright.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/profile.js crates/reborn-ui/ui/profile.html crates/reborn-ui/ui/pages/profile.css Tools/check-profile-page.mjs Tools/check-theme.mjs
git commit -m "profile: the online player hub (card, live card, overview, match history, career, leaderboard, records)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Profile: Full stats, More stats, and links that land on the right table

**Files:**
- Modify: `crates/reborn-ui/ui/profile.js`, `crates/reborn-ui/ui/pages/profile.css`, `Tools/check-profile-page.mjs`

**Interfaces:**
- Consumes: Task 6's `profile.js` helpers and marker comment; `FULL_STATS`, `cellValue`, `sortValue`, `splitTop` (Task 5); `fmtPct` (`stats-model.js`).
- Produces: sections `#full-stats` (tabs Passing / Rushing / Receiving / Defense; `.tbl`; `tr.more` with `button.pf-showall`; `tr.total`; header sort buttons `data-key="sort-<kind>-<key>"`) and `#more` (cards with `h3` titles). `location.hash` `passing | rushing | receiving | defense` opens that table and scrolls to Full stats (the links from Stats' Your players).

- [ ] **Step 1: Add the failing checks**

In `Tools/check-profile-page.mjs`, replace the line
```js
  // (Task 7 adds the Full stats checks here)
```
with:

```js
  { // Full stats: one table at a time, top five, "Show all N players", a Total row; every table on one column grid
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.locator('#full-stats .tbl').waitFor();
    assert.deepEqual(await texts(page, '#full-stats .tab'), ['Passing', 'Rushing', 'Receiving', 'Defense']);
    assert.deepEqual(await texts(page, '#full-stats thead th'), ['Player', 'C/ATT', 'YDS', 'TD', 'INT', 'AIR', 'YAC', 'Y/A', 'RTG', 'LNG']);
    const body = page.locator('#full-stats tbody tr:not(.more):not(.total)');
    assert.equal(await body.count(), 5);
    assert.equal(await page.locator('#full-stats .pf-showall').textContent(), 'Show all 7 players');
    assert.deepEqual(await texts(page, '#full-stats tr.total > *'), ['Total', '212/371', '3,021', '40', '18', '1,900', '1,121', '8.1', '104.2', '74']);
    const grid = () => page.evaluate(() => { const ths = [...document.querySelectorAll('#full-stats thead th')]; return [Math.round(ths[0].getBoundingClientRect().width), Math.round(ths[1].getBoundingClientRect().left)]; });
    const passing = await grid();
    assert.equal(passing[0], 220, 'the shared 220px name column');
    await page.locator('#full-stats .pf-showall').click();
    assert.equal(await body.count(), 7);
    await page.getByRole('button', { name: 'Rushing', exact: true }).click();
    assert.deepEqual(await texts(page, '#full-stats thead th'), ['Player', 'CAR', 'YDS', 'AVG', 'TD', 'LNG', 'BTK', 'YCO']);
    assert.deepEqual(await grid(), passing, 'the tables share their column edges');
    assert.deepEqual(await texts(page, '#full-stats tr.total > *'), ['Total', '308', '1,715', '5.6', '17', '61', '45', '770']);
    // sorting by a column still works (it did before the redesign)
    const firstName = () => page.locator('#full-stats tbody tr').first().locator('.who > span:last-child').textContent();
    await page.locator('[data-key="sort-rushing-yds"]').click();
    assert.equal(await firstName(), 'S. Barkley');
    await page.locator('[data-key="sort-rushing-yds"]').click();
    assert.equal(await firstName(), 'B. Robinson');
    // More stats keeps every number the old profile showed
    assert.deepEqual(await texts(page, '#more h3'), ['Head to head', 'By team', 'Per game', 'Best QB → receiver', 'Laterals & pitches', 'In wins vs losses', 'By game type']);
    // Career's "Full stats ›" goes there
    await page.locator('#career .sec-link').click();
    await page.waitForFunction(() => { const t = document.getElementById('full-stats').getBoundingClientRect().top; return t >= 0 && t < 120; });
    await contentRules(page);
    await page.screenshot({ path: path.join(out, 'profile-1280-full-stats.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // a link from Stats (#defense) opens that table and scrolls to Full stats
    const { page, errors, close } = await openState(browser, srv.base, { ...STATE, url: `${STATE.url}#defense` });
    await page.locator('#full-stats .tbl').waitFor();
    assert.equal(await page.locator('#full-stats .tab[aria-pressed="true"]').textContent(), 'Defense');
    await page.waitForFunction(() => { const t = document.getElementById('full-stats').getBoundingClientRect().top; return t >= 0 && t < 120; });
    assert.deepEqual(errors, []);
    await close();
  }
```

and replace the line
```js
    // (Task 7 adds the older server's rushing table here)
```
with:

```js
    await page.getByRole('button', { name: 'Rushing', exact: true }).click();
    assert.equal(await page.locator('#full-stats tr.total > *').last().textContent(), '770', 'YCO from the older server\'s yac');
```

- [ ] **Step 2: Run it and confirm it fails**

Link Playwright. Run `node Tools/check-profile-page.mjs`.
Expected: FAIL waiting for `#full-stats .tbl`.

- [ ] **Step 3: Add Full stats and More stats to `profile.js`**

1. Replace the import line
```js
import { fmtInt } from './stats-model.js';
```
with
```js
import { fmtInt, fmtPct } from './stats-model.js';
```
and the import line
```js
import { careerTiles, eloOverview, isPublic, lastTenOverview, leaderboardRows, liveCard, recordOverview, statusLine, summaryFacts } from './profile-model.js';
```
with
```js
import { FULL_STATS, careerTiles, cellValue, eloOverview, isPublic, lastTenOverview, leaderboardRows, liveCard, recordOverview, sortValue, splitTop, statusLine, summaryFacts } from './profile-model.js';
```

2. In `careerSection`, replace
```js
  return el('section', { id: 'career', 'aria-labelledby': 'career-h' }, sectionHead('career', 'Career'),
```
with
```js
  const hasTables = Object.keys(FULL_STATS).some((k) => list(p.career?.[k]?.rows).length);
  return el('section', { id: 'career', 'aria-labelledby': 'career-h' },
    sectionHead('career', 'Career', hasTables ? el('a', { class: 'sec-link', href: '#full-stats', 'data-key': 'full-stats', text: 'Full stats ›' }) : null),
```

3. Replace the whole `jump()` function with:

```js
// The links from Stats land here: #passing | #rushing | #receiving | #defense open that table in Full stats; any other
// hash (#career, #records, #history) scrolls to its section.
function jump() {
  const hash = typeof location !== 'undefined' ? decodeURIComponent(location.hash || '').slice(1) : '';
  if (!hash) return;
  if (FULL_STATS[hash] && fs.kinds.includes(hash)) {
    fs.tab = hash;
    fs.render();
    document.getElementById('full-stats')?.scrollIntoView?.({ block: 'start' });
    return;
  }
  document.getElementById(hash)?.scrollIntoView?.({ block: 'start' });
}
```

4. In `draw`, replace
```js
  // More sections below the columns (plan 4 Task 7)
```
with
```js
  out.push(...[fullStatsSection(p), moreStats(p)].filter(Boolean));
```

5. Insert this block directly above the line `// ---- the page ----`:

```js
// ---- Full stats: one table at a time, the top five, "Show all N players", then Total (spec §8.7.5) ----

const fs = { tab: 'passing', kinds: [], open: new Set(), sort: {}, render: () => {} };

function who(name, team) {
  return el('span', { class: 'who' }, teamLogo(team), el('span', { text: name || DASH }));
}

function statTable(kind, data) {
  const def = FULL_STATS[kind];
  const order = fs.sort[kind] || null;
  const rows = list(data?.rows).slice();
  if (order) rows.sort((a, z) => {
    const x = order.key === 'name' ? String(a.name ?? '') : sortValue(def.cols[order.i], a);
    const y = order.key === 'name' ? String(z.name ?? '') : sortValue(def.cols[order.i], z);
    if (typeof x === 'string' || typeof y === 'string') return order.dir * String(x ?? '').localeCompare(String(y ?? ''));
    return order.dir * ((num(x) ? x : -Infinity) - (num(y) ? y : -Infinity));
  });
  const { top, rest } = splitTop(rows);
  const all = fs.open.has(kind);
  const head = (label, title, key, i) => {
    const th = el('th', { scope: 'col' });
    if (order && order.key === key) th.setAttribute('aria-sort', order.dir > 0 ? 'ascending' : 'descending');
    th.append(el('button', { type: 'button', class: 'pf-sort', 'data-key': `sort-${kind}-${key}`, text: label, title: `Sort by ${title.toLowerCase()}`,
      onclick: () => { fs.sort[kind] = { key, i, dir: order?.key === key ? -order.dir : key === 'name' ? 1 : -1 }; fs.render(); } }));
    return th;
  };
  const body = [...top, ...(all ? rest : [])].map((r) => el('tr', {},
    el('th', { scope: 'row' }, who(r.name, { name: r.team, logo_url: r.logo_url, abbrev: r.abbrev })), ...def.cols.map((c) => el('td', { text: cellValue(c, r) }))));
  if (rest.length && !all) body.push(el('tr', { class: 'more' }, el('td', { colspan: String(def.cols.length + 1) },
    el('button', { type: 'button', class: 'pf-showall', 'data-key': `showall-${kind}`, onclick: () => { fs.open.add(kind); fs.render(); } },
      el('span', { text: `Show all ${rows.length} players` }), el('span', { class: 'chev', 'aria-hidden': 'true' })))));
  if (data?.total) body.push(el('tr', { class: 'total' }, el('th', { scope: 'row', text: 'Total' }), ...def.cols.map((c) => el('td', { text: cellValue(c, data.total) }))));
  return el('table', { class: 'tbl' }, el('caption', { class: 'pf-sr', text: `${def.title}, every player used. Select a column heading to sort.` }),
    el('thead', {}, el('tr', {}, head('Player', 'Player', 'name', -1), ...def.cols.map((c, i) => head(c.label, c.title, c.key, i)))),
    el('tbody', {}, ...body));
}

function fullStatsSection(p) {
  fs.kinds = Object.keys(FULL_STATS).filter((k) => list(p.career?.[k]?.rows).length);
  if (!fs.kinds.length) return null;
  if (!fs.kinds.includes(fs.tab)) fs.tab = fs.kinds[0];
  const head = el('div', { class: 'sec-head' });
  const body = el('div', { class: 'glass pf-tbl' });
  fs.render = () => {
    const tabFocus = typeof head.contains === 'function' && head.contains(document.activeElement);
    const active = typeof body.contains === 'function' && body.contains(document.activeElement) ? document.activeElement?.getAttribute('data-key') : null;
    head.replaceChildren(el('h2', { id: 'full-stats-h', text: 'Full stats' }),
      tabs('profile-full', fs.kinds.map((k) => FULL_STATS[k].title), FULL_STATS[fs.tab].title,
        (title) => { fs.tab = fs.kinds.find((k) => FULL_STATS[k].title === title); fs.render(); }, { label: 'Stat tables' }));
    body.replaceChildren(statTable(fs.tab, p.career[fs.tab]));
    if (tabFocus) head.querySelector('[aria-pressed="true"]')?.focus({ preventScroll: true });
    else if (active) Array.from(body.querySelectorAll('[data-key]')).find((n) => n.getAttribute('data-key') === active)?.focus({ preventScroll: true });
  };
  fs.render();
  return el('section', { id: 'full-stats', 'aria-labelledby': 'full-stats-h' }, head, body);
}

// ---- More stats: the rest of the profile's numbers, kept so nothing a player could see before is lost ----

const oneOr = (n) => (num(n) ? n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 }) : DASH);
const recordOf = (r) => `${num(r?.wins) ? r.wins : 0}–${num(r?.losses) ? r.losses : 0}`;
const pctOf = (p) => (num(p) ? fmtPct(p) : DASH);
const convOf = (p) => (num(p) ? `${Math.round(p * 100)}%` : DASH);

function smallTable(caption, headers, rows) {
  return el('table', { class: 'tbl' }, el('caption', { class: 'pf-sr', text: caption }),
    el('thead', {}, el('tr', {}, ...headers.map((h) => el('th', { scope: 'col', text: h })))),
    el('tbody', {}, ...rows.map((cells) => el('tr', {}, ...cells.map((c, i) => {
      const cell = i === 0 ? el('th', { scope: 'row' }) : el('td');
      if (typeof c === 'string') cell.textContent = c; else if (c) cell.append(c);
      return cell;
    })))));
}

function moreCard(id, title, table, wide = false) {
  return el('section', { id, class: `glass pf-more-card${wide ? ' wide' : ''}`, 'aria-labelledby': `${id}-h` }, el('h3', { id: `${id}-h`, text: title }), table);
}

function moreStats(p) {
  const cards = [];
  const h2h = list(p.head_to_head).filter(Boolean);
  if (h2h.length) cards.push(moreCard('h2h', 'Head to head', smallTable('Record against every opponent',
    ['Opponent', 'Record', 'PTS', 'Allowed', 'Last', 'Best win', 'Worst loss'], h2h.map((h) => {
      const name = h.display_name || 'Player';
      const inner = [avatar(h.avatar_url, name), el('span', { text: name })];
      const cell = h.player_id ? el('a', { class: 'who', href: profileHref(h.player_id), 'data-key': `h2h-${h.player_id}` }, ...inner) : el('span', { class: 'who' }, ...inner);
      const letter = h.last?.result === 'win' ? 'W' : h.last?.result === 'loss' ? 'L' : '';
      return [cell, recordOf(h), int(h.pts_for), int(h.pts_against), h.last ? `${letter} ${day(h.last.at)}`.trim() : DASH,
        num(h.biggest_win?.margin) ? `+${h.biggest_win.margin}` : DASH, num(h.biggest_loss?.margin) ? `−${h.biggest_loss.margin}` : DASH];
    })), true));
  const teams = list(p.by_team).filter(Boolean);
  if (teams.length) cards.push(moreCard('by-team', 'By team', smallTable('Record by team played as', ['Team', 'Record', 'Pts / gm', 'Win %'],
    teams.map((t) => [who(t.name, t.key === 'pickup' ? { abbrev: 'PU' } : { name: t.name, logo_url: t.logo_url, abbrev: t.abbrev }), recordOf(t), oneOr(t.pts_per_game), pctOf(t.win_pct)]))));
  const pg = p.per_game;
  if (pg && pg.games) cards.push(moreCard('per-game', 'Per game', smallTable(`Per game, over ${int(pg.games)} games with play-by-play`, ['Stat', `Per game (${int(pg.games)} games)`], [
    ['Total yards', oneOr(pg.yds)], ['Pass / rush', `${oneOr(pg.pass)} / ${oneOr(pg.rush)}`], ['First downs', oneOr(pg.first_downs)], ['Turnovers', oneOr(pg.turnovers)],
    ['1-pt conversions', convOf(pg.conv1_pct)], ['2-pt conversions', convOf(pg.conv2_pct)], ['Points per drive', oneOr(pg.pts_per_drive)], ['Three-and-outs', oneOr(pg.three_outs)],
    ['Longest drive', num(pg.longest_drive) ? `${pg.longest_drive} plays` : DASH], ['Lead changes', oneOr(pg.lead_changes)], ['Biggest lead', oneOr(pg.biggest_lead)], ['Safeties', int(pg.safeties)]])));
  const combos = list(p.combos).filter((c) => c && c.passer && c.receiver);
  if (combos.length) cards.push(moreCard('combos', 'Best QB → receiver', smallTable('Quarterback to receiver combinations', ['Combo', 'REC', 'YDS', 'TD'],
    combos.map((c) => [who(`${c.passer.name} → ${c.receiver.name}`, { name: c.passer.team, logo_url: c.passer.logo_url }), int(c.rec), int(c.yds), int(c.td)]))));
  const pt = p.pitches;
  if (pt && (pt.pitches || pt.pitch_plays)) cards.push(moreCard('pitches', 'Laterals & pitches', smallTable('Laterals and pitches', ['Stat', 'Total'],
    [['Pitches', int(pt.pitches)], ['Plays with a pitch', int(pt.pitch_plays)], ['Yards on those plays', int(pt.pitch_yds)]])));
  const w = p.wins_vs_losses?.wins, l = p.wins_vs_losses?.losses;
  if (w?.games || l?.games) cards.push(moreCard('wins-losses', 'In wins vs losses', smallTable('Per-game numbers in wins and in losses', ['Games', 'PTS', 'YDS', 'TO'],
    [[`Wins (${int(w?.games ?? 0)})`, oneOr(w?.pts), oneOr(w?.yds), oneOr(w?.to)], [`Losses (${int(l?.games ?? 0)})`, oneOr(l?.pts), oneOr(l?.yds), oneOr(l?.to)]])));
  const types = list(p.by_game_type).filter(Boolean);
  if (types.length) cards.push(moreCard('by-type', 'By game type', smallTable('Record by game type', ['Game type', 'Record', 'Pts / gm', 'Win %'],
    types.map((t) => [String(t.name ?? DASH), recordOf(t), oneOr(t.pts_per_game), pctOf(t.win_pct)]))));
  if (!cards.length) return null;
  return el('section', { id: 'more', 'aria-labelledby': 'more-h' }, sectionHead('more', 'More stats'), el('div', { class: 'pf-more' }, ...cards));
}

```

- [ ] **Step 4: Add their layout to `pages/profile.css`**

Insert these lines directly above the `@media (max-width:1100px){` line of `crates/reborn-ui/ui/pages/profile.css`:

```css
/* Full stats and More stats: theme.css's .tbl (a shared 220px name column, so every table's columns line up) */
.pf-tbl{overflow:hidden}
.pf-sort{all:unset;cursor:pointer}
.pf-sort:hover{color:var(--fg)}
.tbl tr.more td{height:auto;padding:0}
.pf-showall{all:unset;box-sizing:border-box;display:flex;align-items:center;justify-content:center;gap:8px;width:100%;height:38px;cursor:pointer;font:600 12.5px Inter,system-ui,sans-serif;color:var(--fg-2)}
.pf-showall:hover{background:var(--glass-1);color:var(--fg)}
.pf-showall .chev{margin:0}
.pf-more{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:24px;align-items:start}
.pf-more-card{overflow:hidden;padding-top:8px}
.pf-more-card.wide{grid-column:1/-1}
.pf-more-card h3{margin:0;padding:8px 16px;font:600 14px Inter,system-ui,sans-serif}
.pf-sr{position:absolute;width:1px;height:1px;margin:-1px;padding:0;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap;border:0}
```

and add `.pf-more{grid-template-columns:minmax(0,1fr)}` inside the `@media (max-width:1100px){ … }` block (as its last rule).

- [ ] **Step 5: Run the checks**

- `node Tools/check-profile-page.mjs`: PASS (including the new blocks).
- `node Tools/check-stats-page.mjs`: PASS.
- `node Tools/check-theme-tokens.mjs`: PASS.
- The standing checks: PASS.

Open `target/p4-after/profile-1280-full-stats.png` beside the mockup's Full stats. Check: one table, top five, the Show all row, a Total row in the same style with a brighter top line, uppercase units.

Unlink Playwright.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/ui/profile.js crates/reborn-ui/ui/pages/profile.css Tools/check-profile-page.mjs
git commit -m "profile: Full stats (one table, top five, Total), More stats, links from Stats land on the right table

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: The layout extractor

Nothing visible changes in this task. It builds the tool that moves the pages which are not redesigned (spec §9) onto `theme.css` without touching their layout.

**Files:**
- Create: `Tools/lib/colour-tokens.mjs`, `Tools/lib/colour-tokens.test.mjs`, `Tools/extract-layout-css.mjs`

**Interfaces:**
- Consumes: `STATES`, `serve`, `openState`, `ui` (Task 1).
- Produces:
  - `colour-tokens.mjs`: `hue([r,g,b]) → 'red' | 'amber' | 'green' | 'neutral'`, `textToken([r,g,b,a])`, `fillToken([r,g,b,a])`, `lineToken([r,g,b,a])` → a CSS value naming only theme tokens (`var(--…)`, `color-mix(in srgb,var(--…) N%,transparent)` or `transparent`).
  - `node Tools/extract-layout-css.mjs <page>[,<page>…] <out.css>`: run **while the page still loads the old sheets**. Prints `extract-layout-css: N rule(s) kept, M left out, K keyframe(s) -> <file>`.

- [ ] **Step 1: Write the failing colour tests**

Create `Tools/lib/colour-tokens.test.mjs`:

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { hue, textToken, fillToken, lineToken } from './colour-tokens.mjs';

test('hue', () => {
  assert.equal(hue([224, 39, 46]), 'red');
  assert.equal(hue([243, 195, 106]), 'amber');
  assert.equal(hue([34, 199, 180]), 'green');
  assert.equal(hue([128, 128, 128]), 'neutral');
  assert.equal(hue([19, 21, 25]), 'neutral');
});

test('text colours go to the nearest text token', () => {
  assert.equal(textToken([255, 255, 255, 1]), 'var(--fg)');
  assert.equal(textToken([164, 172, 184, 1]), 'var(--fg-2)');
  assert.equal(textToken([133, 135, 130, 1]), 'var(--fg-3)');
  assert.equal(textToken([34, 199, 180, 1]), 'var(--ok)');
  assert.equal(textToken([243, 195, 106, 1]), 'var(--warn)');
  assert.equal(textToken([255, 154, 154, 1]), 'var(--red-ink)');
  assert.equal(textToken([0, 0, 0, 0]), 'transparent');
});

test('fills: state colours stay state colours, neutral fills become glass', () => {
  assert.equal(fillToken([224, 39, 46, 1]), 'var(--red)');
  assert.equal(fillToken([224, 39, 46, 0.1]), 'color-mix(in srgb,var(--red) 10%,transparent)');
  assert.equal(fillToken([34, 199, 180, 0.12]), 'color-mix(in srgb,var(--ok) 12%,transparent)');
  assert.equal(fillToken([243, 195, 106, 1]), 'var(--warn)');
  assert.equal(fillToken([19, 21, 25, 1]), 'var(--glass-1)');
  assert.equal(fillToken([255, 255, 255, 0.045]), 'var(--glass-1)');
  assert.equal(fillToken([255, 255, 255, 0.075]), 'var(--glass-2)');
  assert.equal(fillToken([255, 255, 255, 0.11]), 'var(--glass-3)');
  assert.equal(fillToken([255, 255, 255, 1]), 'var(--fg)');
  assert.equal(fillToken([0, 0, 0, 0]), 'transparent');
});

test('lines: neutral lines become strokes, state lines a tint', () => {
  assert.equal(lineToken([255, 255, 255, 0.09]), 'var(--stroke)');
  assert.equal(lineToken([255, 255, 255, 0.12]), 'var(--stroke)');
  assert.equal(lineToken([255, 255, 255, 0.16]), 'var(--stroke-hi)');
  assert.equal(lineToken([224, 39, 46, 0.25]), 'color-mix(in srgb,var(--red) 35%,transparent)');
  assert.equal(lineToken([0, 0, 0, 0]), 'transparent');
});
```

Run: `node --test Tools/lib/colour-tokens.test.mjs`
Expected: FAIL (`Cannot find module … colour-tokens.mjs`).

- [ ] **Step 2: Write `colour-tokens.mjs`**

Create `Tools/lib/colour-tokens.mjs`:

```js
// Old colours onto the theme's tokens (spec §3.1), for the layout files Tools/extract-layout-css.mjs writes.
// Input: a computed colour [r, g, b, a] (0-255, alpha 0-1). Output: a CSS value that names only theme tokens.
export const TEXT = { '--fg': [244, 245, 247], '--fg-2': [174, 181, 192], '--fg-3': [124, 132, 145], '--red-ink': [255, 122, 127], '--ok': [61, 220, 151], '--warn': [255, 196, 107] };
const STATE = { red: '--red', amber: '--warn', green: '--ok' };
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
// How light a colour looks over the near-black window (#0d0e11 is about .05).
const light = ([r, g, b, a]) => a * ((0.2126 * r + 0.7152 * g + 0.0722 * b) / 255) + (1 - a) * 0.05;
const tint = (token, a, lo, hi) => `color-mix(in srgb,var(${token}) ${Math.max(lo, Math.min(hi, Math.round(a * 100)))}%,transparent)`;

export function hue([r, g, b]) {
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  if (max < 60 || (max - min) / max < 0.35) return 'neutral';
  if (r === max && g < r * 0.6 && b < r * 0.7) return 'red';
  if (r === max && g >= r * 0.6 && b < g * 0.75) return 'amber';
  return 'green';   // green, teal and blue: the theme's one cool state colour is --ok
}

/** Text, icons (fill/stroke), carets: the nearest of the six text tokens. */
export function textToken(c) {
  if (c[3] < 0.02) return 'transparent';
  let best = '--fg', d = Infinity;
  for (const [token, v] of Object.entries(TEXT)) { const x = dist(c, v); if (x < d) { d = x; best = token; } }
  return `var(${best})`;
}

/** Backgrounds: a state colour stays one (solid, or a tint when it was faint); anything neutral becomes a glass level. */
export function fillToken(c) {
  if (c[3] < 0.02) return 'transparent';
  const h = hue(c);
  if (h !== 'neutral') return c[3] >= 0.6 ? `var(${STATE[h]})` : tint(STATE[h], c[3], 6, 40);
  const e = light(c);
  if (e > 0.5) return 'var(--fg)';
  return e < 0.107 ? 'var(--glass-1)' : e < 0.138 ? 'var(--glass-2)' : 'var(--glass-3)';
}

/** Borders and rules: --stroke or --stroke-hi; a state colour keeps its colour as a tint. */
export function lineToken(c) {
  if (c[3] < 0.02) return 'transparent';
  const h = hue(c);
  if (h !== 'neutral') return tint(STATE[h], Math.max(c[3], 0.35), 35, 100);
  return light(c) >= 0.169 ? 'var(--stroke-hi)' : 'var(--stroke)';
}
```

Run: `node --test Tools/lib/colour-tokens.test.mjs`
Expected: 4 tests pass.

- [ ] **Step 3: Write the extractor**

Create `Tools/extract-layout-css.mjs`:

```js
// The layout a page gets from the retired stylesheets, written as one page stylesheet that loads after theme.css
// (spec §9, §11): layout properties kept exactly, every colour turned into a theme token (Tools/lib/colour-tokens.mjs),
// shadows, glows, filters, transitions and font families dropped. Run it while the page STILL loads the old sheets.
//     node Tools/extract-layout-css.mjs <page>[,<page>...] <out.css>
// Which rules: every rule of an old sheet that matches an element in one of the pages' states (Tools/lib/page-states.mjs),
// plus every rule whose class and id names all appear in the pages' own HTML and scripts, so a state the fixtures never
// reach keeps its layout too. Rules for what theme.css owns (the shell, the page heading, the hint bar, update cards,
// menus, the save bar) are left out. Every kept selector is prefixed `html:not(#_)`, so the old layout outranks
// theme.css's base rules (buttons, fields, headings) the way it did before. That prefix matches every page: the file
// must only ever be loaded by the page(s) it was made for.
import { chromium } from 'playwright';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { STATES, serve, openState, ui } from './lib/page-states.mjs';
import { textToken, fillToken, lineToken } from './lib/colour-tokens.mjs';

const [pagesArg, outArg] = process.argv.slice(2);
const pages = String(pagesArg || '').split(',').filter(Boolean);
if (!pages.length || !outArg || pages.some((p) => !STATES[p])) {
  console.error(`usage: node Tools/extract-layout-css.mjs <page>[,<page>...] <out.css>   (pages: ${Object.keys(STATES).join(', ')})`);
  process.exit(2);
}
const out = path.resolve(outArg);
const scope = STATES[pages[0]][0].scope;

// What theme.css owns. Onboarding (scope 'page') keeps its own sidebar: theme.css's shell skips body.page-setup.
const OWNED = [/\.app-header\b/, /\.input-footer\b/, /#input-hints\b/, /\.input-hint\b/, /(^|[\s>+~(,])kbd\b/, /\.skip-link\b/, /\.backdrop\b/,
  /\.page-heading\b/, /\.eyebrow\b/, /details\.core\b/, /\.out-head\b/, /#out\b/, /#clear\b/, /\.update-/, /\.menu-pad-setup/, /\.menu(?![\w-])/,
  /\.save-lane\b/, /\.save-bar\b/, /\.nav-marker\b/, /\.pill\b/, /#bridge\b/, /\.look-/,
  ...(scope === 'page' ? [] : [/\.shell-nav\b/, /\.brand\b/])];
const owned = (sel) => OWNED.some((re) => re.test(sel));
const subjectIsRoot = (sel) => /^(html|body|:root|\*)([.#:[]|$)/.test(sel.trim().split(/\s*[>+~]\s*|\s+/).pop());
const DYN = /::?(hover|focus-visible|focus-within|focus|active|visited|link|checked|disabled|enabled|placeholder-shown|placeholder|before|after|marker|selection|backdrop|first-line|first-letter|open|target|invalid|valid|indeterminate|-webkit-[\w-]+|-moz-[\w-]+)(\([^)]*\))?/g;
const COLOR = /^(color|background-color|border-(top|right|bottom|left)-color|outline-color|column-rule-color|text-decoration-color|caret-color|accent-color|fill|stroke)$/;
const TEXTISH = /^(color|fill|stroke|caret-color|text-decoration-color)$/;
const KEYWORD = /^(transparent|currentcolor|inherit|initial|unset|none)$/i;
const DROP = /^(box-shadow|text-shadow|outline(-[\w-]+)?|filter|backdrop-filter|-webkit-backdrop-filter|mix-blend-mode|transition(-[\w-]+)?|font-family|font-stretch|font-feature-settings|font-kerning|font-optical-sizing|font-palette|font-synthesis(-[\w-]+)?|font-language-override|font-size-adjust|font-variation-settings|-webkit-font-smoothing|-webkit-tap-highlight-color|-webkit-text-fill-color|-webkit-text-stroke(-[\w-]+)?|scrollbar-(color|width|gutter)|color-scheme|mask(-[\w-]+)?|-webkit-mask(-[\w-]+)?|border-image(-[\w-]+)?|forced-color-adjust|print-color-adjust|accent-color)$/;

// ---- the pages' own sources: their HTML and every script they load, followed through imports ----
function sources() {
  const seen = new Set(), texts = [];
  const add = (file) => {
    if (seen.has(file) || !existsSync(file)) return;
    seen.add(file);
    const t = readFileSync(file, 'utf8');
    texts.push(t);
    for (const m of t.matchAll(/(?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s+|<script\b[^>]*\bsrc=)["']([\w./-]+\.js)["']/g)) add(path.resolve(path.dirname(file), m[1]));
  };
  for (const p of pages) add(path.join(ui, `${p}.html`));
  return texts.join('\n');
}
const SRC = sources();
const named = (name) => new RegExp(`(?<![\\w-])${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\w-])`).test(SRC);
const tokensKnown = (sel) => {
  const names = [...sel.replace(DYN, '').replace(/\[[^\]]*\]/g, '').matchAll(/[.#](-?[_a-zA-Z][\w-]*)/g)].map((m) => m[1]);
  return names.length > 0 && names.every(named);
};
const prefix = (sel) => (/^html\b/.test(sel) ? sel.replace(/^html/, 'html:not(#_)') : /^:root\b/.test(sel) ? sel.replace(/^:root/, ':root:not(#_)') : `html:not(#_) ${sel}`);

// ---- in the page: every old rule, whether each selector matches something on screen, its declarations as longhands ----
async function collect(page) {
  return page.evaluate(({ scopeMain, dynSource, colorSource }) => {
    const DYNRE = new RegExp(dynSource, 'g');
    const COLORRE = new RegExp(colorSource);
    const main = document.getElementById('main');
    const inScope = (n) => !scopeMain || n === main || Boolean(main && main.contains(n)) || Boolean(n.closest('dialog:not(.menu):not(.update-dialog):not(.menu-pad-setup-dialog)'));
    const hit = (sel) => {
      const s = sel.replace(DYNRE, '').replace(/:not\(\s*\)/g, '').trim() || '*';
      try { return [...document.querySelectorAll(s)].some(inScope); } catch { return false; }
    };
    const split = (text, sep) => {
      const parts = []; let depth = 0, cur = '';
      for (const ch of text) {
        if (ch === '(' || ch === '[') depth++;
        if (ch === ')' || ch === ']') depth--;
        if (ch === sep && !depth) { parts.push(cur); cur = ''; } else cur += ch;
      }
      parts.push(cur);
      return parts.map((s) => s.trim()).filter(Boolean);
    };
    const probe = document.createElement('div');
    document.body.append(probe);
    const rgba = (v) => {
      if (/^(currentcolor|inherit|initial|unset|revert)$/i.test(String(v).trim())) return null;
      probe.style.color = '';
      probe.style.color = v;
      if (!probe.style.color) return null;
      const m = getComputedStyle(probe).color.match(/[\d.]+/g);
      if (!m) return null;
      const n = m.map(Number);
      return [n[0], n[1], n[2], n.length > 3 ? n[3] : 1];
    };
    const vars = getComputedStyle(document.body);
    const resolve = (v) => {
      let s = v;
      for (let i = 0; i < 10 && /var\(/.test(s); i++) {
        s = s.replace(/var\(\s*(--[\w-]+)\s*(?:,\s*([^()]*(?:\([^()]*\)[^()]*)*))?\)/g, (m, name, fb) => vars.getPropertyValue(name).trim() || (fb || '').trim());
      }
      return s;
    };
    const expand = (prop, value) => {
      const st = document.createElement('div').style;
      st.setProperty(prop, value);
      const res = [];
      for (let i = 0; i < st.length; i++) res.push([st[i], st.getPropertyValue(st[i])]);
      return res;
    };
    const sheets = [...document.styleSheets].filter((s) => s.href && !/\/theme\.css(\?|$)/.test(s.href) && !/\/pages\//.test(s.href));
    const rules = [], keyframes = {};
    for (const sheet of sheets) {
      const file = sheet.href.split('/').pop().split('?')[0];
      let ord = 0;
      const walk = (list, media) => {
        for (const r of list) {
          if (r instanceof CSSKeyframesRule) {
            keyframes[r.name] = { css: r.cssText, safe: [...r.cssRules].every((k) => [...k.style].every((p) => /^(opacity|transform|translate|scale|rotate|animation-timing-function)$/.test(p))) };
            continue;
          }
          if (r instanceof CSSMediaRule || r instanceof CSSSupportsRule) {
            walk(r.cssRules, [...media, `@${r instanceof CSSMediaRule ? 'media' : 'supports'} ${r.conditionText}`]);
            continue;
          }
          if (!(r instanceof CSSStyleRule)) continue;
          const key = `${file}:${ord++}`;
          const parts = split(r.selectorText, ',').map((sel) => ({ sel, hit: hit(sel) }));
          const decls = [];
          for (const d of split(r.style.cssText, ';')) {
            const i = d.indexOf(':');
            if (i < 0) continue;
            const prop = d.slice(0, i).trim().toLowerCase();
            let value = d.slice(i + 1).trim();
            const important = /!\s*important$/i.test(value);
            value = value.replace(/!\s*important$/i, '').trim();
            if (prop.startsWith('--')) continue;
            for (const [p, v] of expand(prop, resolve(value))) decls.push({ group: prop, prop: p, value: v, important, rgba: COLORRE.test(p) ? rgba(v) : null });
          }
          rules.push({ key, media, parts, decls });
        }
      };
      walk([...sheet.cssRules], []);
    }
    probe.remove();
    return { rules, keyframes };
  }, { scopeMain: scope === 'main', dynSource: DYN.source, colorSource: COLOR.source });
}

// ---- in Node: which declarations survive, as what ----
const rewriteUrls = (v) => v.replace(/url\(\s*(["']?)([^"')]+)\1\s*\)/g, (m, q, u) => {
  const local = u.replace(/^https?:\/\/127\.0\.0\.1:\d+\//, '');
  if (/^(data:|https?:|\/|#)/.test(local)) return m;
  return `url("${path.relative(path.dirname(out), path.join(ui, local)).split(path.sep).join('/')}")`;
});
function colourValue(d) {
  if (!d.rgba) return KEYWORD.test(d.value.trim()) ? d.value.trim().toLowerCase() : null;
  if (TEXTISH.test(d.prop)) return textToken(d.rgba);
  if (d.prop === 'background-color') return fillToken(d.rgba);
  return lineToken(d.rgba);
}
function keep(d, imageFromShorthand) {
  if (DROP.test(d.prop)) return null;
  if (d.prop === 'background-image') return /url\(/.test(d.value) && !/gradient/.test(d.value) ? rewriteUrls(d.value) : null;
  if (d.prop.startsWith('background-') && d.prop !== 'background-color') return d.group === d.prop || imageFromShorthand ? d.value : null;
  if (COLOR.test(d.prop)) return colourValue(d);
  if (d.prop === 'font-weight') {
    const n = d.value === 'bold' ? 700 : d.value === 'normal' ? 400 : Number(d.value);
    return Number.isFinite(n) ? String(Math.min(700, n)) : d.value;
  }
  return /url\(/.test(d.value) ? rewriteUrls(d.value) : d.value;
}

const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const merged = new Map();
let keyframes = {};
try {
  for (const p of pages) for (const state of STATES[p]) {
    const { page, close } = await openState(browser, srv.base, state);
    const got = await collect(page);
    await close();
    keyframes = { ...keyframes, ...got.keyframes };
    for (const r of got.rules) {
      const was = merged.get(r.key);
      if (!was) merged.set(r.key, r);
      else r.parts.forEach((part, i) => { if (part.hit && was.parts[i]) was.parts[i].hit = true; });
    }
  }
} finally { await browser.close(); await srv.close(); }

const lines = [], used = new Set(), oldFonts = new Set();
let kept = 0, left = 0;
for (const r of merged.values()) {
  const sels = r.parts.filter((p) => !owned(p.sel) && (scope === 'page' || !subjectIsRoot(p.sel)) && (p.hit || tokensKnown(p.sel))).map((p) => prefix(p.sel));
  if (!sels.length) { left++; continue; }
  const imageFromShorthand = r.decls.some((d) => d.group === 'background' && d.prop === 'background-image' && /url\(/.test(d.value) && !/gradient/.test(d.value));
  const names = (r.decls.find((d) => d.prop === 'animation-name')?.value || 'none').split(',').map((s) => s.trim()).filter((n) => n && n !== 'none');
  const animOk = names.length > 0 && names.every((n) => keyframes[n]?.safe);
  if (r.decls.some((d) => d.prop === 'font-family' && /anton|saira/i.test(d.value))) sels.forEach((s) => oldFonts.add(s));
  const body = [];
  for (const d of r.decls) {
    const v = /^animation/.test(d.prop) ? (animOk ? d.value : null) : keep(d, imageFromShorthand);
    if (v === null || v === undefined || v === '') continue;
    body.push(`${d.prop}:${v}${d.important ? '!important' : ''}`);
  }
  if (!body.length) { left++; continue; }
  if (animOk) names.forEach((n) => used.add(n));
  kept++;
  let text = `${sels.join(',')}{${body.join(';')}}`;
  for (const m of [...r.media].reverse()) text = `${m}{${text}}`;
  lines.push(text);
}
for (const n of used) lines.push(keyframes[n].css);

const these = pages.length > 1 ? 'these pages' : 'this page';
const head = [
  `/* ${pages.map((p) => `${p}.html`).join(' + ')}: the layout the retired stylesheets gave ${these} (spec §9, §11),`,
  '   made by Tools/extract-layout-css.mjs and checked with Tools/compare-page-layout.mjs.',
  '   Layout only: colours are theme tokens; shadows, glows, filters, transitions and font families are gone.',
  `   Every selector starts with html:not(#_) so the old layout outranks theme.css's base rules: load this file on ${these} only.`,
  ...(oldFonts.size ? [`   The old font of these was Anton or Saira (now Inter): ${[...oldFonts].join(', ')}`] : []),
  '*/',
].join('\n');
writeFileSync(out, `${head}\n${lines.join('\n')}\n`);
console.log(`extract-layout-css: ${kept} rule(s) kept, ${left} left out, ${used.size} keyframe(s) -> ${path.relative(process.cwd(), out)}`);
```

- [ ] **Step 4: Prove it on the Online page (nothing is switched yet)**

Link Playwright, then (Git Bash):

```bash
mkdir -p target/p4-probe
node Tools/extract-layout-css.mjs online target/p4-probe/online.css
node Tools/check-theme-tokens.mjs target/p4-probe
grep -c 'html:not(#_)' target/p4-probe/online.css
grep -c 'netplay-lobby' target/p4-probe/online.css
grep -cE 'shell-nav|input-footer|page-heading|update-card' target/p4-probe/online.css
```

Expected:
- `extract-layout-css: N rule(s) kept, M left out, K keyframe(s) -> target\p4-probe\online.css` with N > 50.
- `check-theme-tokens: PASS (1 …)`. If it FAILs, a colour property is reaching the output unmapped: add that property to `COLOR` (a colour) or `DROP` (visual only) in the extractor and run again.
- The first two `grep -c` print a number above 0. `.netplay-lobby` is the room lobby, which the fixtures never show, so this proves Review Focus 5: a rule for a state the fixtures do not reach is kept because `online.js` names its class.
- The last `grep -c` prints `0`.

Delete `target/p4-probe`. Run `node --test Tools/lib/colour-tokens.test.mjs` and the standing checks (nothing in the product changed). Unlink Playwright.

- [ ] **Step 5: Commit**

```bash
git add Tools/lib/colour-tokens.mjs Tools/lib/colour-tokens.test.mjs Tools/extract-layout-css.mjs
git commit -m "tools: layout extractor for the pages moving onto theme.css (colours onto tokens)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Recovery, the game view and the live view onto `theme.css`, layout unchanged

**Files:**
- Create: `crates/reborn-ui/ui/pages/recovery.css`, `crates/reborn-ui/ui/pages/live.css` (both generated)
- Modify: `crates/reborn-ui/ui/recovery.html`, `crates/reborn-ui/ui/game.html`, `crates/reborn-ui/ui/live.html` (by `theme-page.mjs`), `Tools/check-theme.mjs`, `Tools/check-live-view.mjs`

**Interfaces:**
- Consumes: Task 1 (`theme-page.mjs`, `compare-page-layout.mjs`, `target/p4-before/`), Task 8 (`extract-layout-css.mjs`).
- Produces: `recovery`, `game` and `live` in `THEMED`; `game` and `live` share `pages/live.css`.

- [ ] **Step 1: Extract the layout while the old sheets are still loaded**

Link Playwright, then (Git Bash):

```bash
node Tools/extract-layout-css.mjs recovery crates/reborn-ui/ui/pages/recovery.css
node Tools/extract-layout-css.mjs game,live crates/reborn-ui/ui/pages/live.css
node Tools/check-theme-tokens.mjs
```

Expected: two `extract-layout-css: …` lines, then `check-theme-tokens: PASS`.

- [ ] **Step 2: Switch the three pages**

```bash
node Tools/theme-page.mjs recovery pages/recovery.css
node Tools/theme-page.mjs game pages/live.css
node Tools/theme-page.mjs live pages/live.css
```

In `Tools/check-theme.mjs`, add inside the `THEMED` object literal:

```js
  recovery: "pages/recovery.css", game: "pages/live.css", live: "pages/live.css",
```

In `Tools/check-live-view.mjs`, replace
```js
  check("live.html loads live-view.css", LIVE_HTML.includes('href="live-view.css"'));
```
with
```js
  check("live.html loads pages/live.css", LIVE_HTML.includes('href="pages/live.css"'));
```

- [ ] **Step 3: Compare with the before pictures**

```bash
for p in recovery game live; do node Tools/compare-page-layout.mjs capture $p target/p4-after; done
for p in recovery game live; do node Tools/compare-page-layout.mjs diff $p target/p4-before target/p4-after; done
```

Expected: `compare-page-layout <page>: PASS (0 fail, N warn)` three times.

**For each FAIL line, fix and re-run capture + diff until it passes:**
- `gone after` / `new after`: the DOM changed. Only `theme-page.mjs` may edit these HTML files; undo anything else.
- `hidden after` / `shown after`: a `display`/`visibility` rule was lost. Find the old rule (`grep -n "<class from the chain>" crates/reborn-ui/ui/*.css`; the old sheets are still on disk until Task 12), and copy its `display`/`visibility` declaration into the page file as `html:not(#_) <old selector>{…}`.
- a sideways move or resize: find the element's old rules the same way. The usual cause is a `theme.css` `body.themed` base rule that the old sheets never overrode for this element (classless buttons and `a.button` are 38px high with `0 16px` padding; `select` and `input[type=text]` are 38px; `.facts th` is 120px wide; `.refusal`/`.partial`/`.said` have `12px 16px` padding). Add a rule at the end of the page file, below a `/* hand-kept: <why> */` comment, that restores the old value (the before value is in the FAIL line), e.g. `html:not(#_) body.page-recovery .facts th{width:140px}`. Keep it layout only (no colours).

Then open each WARN pair side by side (`target/p4-before/<state>-<w>.png` and `target/p4-after/<state>-<w>.png`, and the `-end` pair). A WARN is acceptable only when it comes from the page heading (theme's 30px title) or a text-metric change; list every WARN line and its reason in your report.

Look at `game-1280`, `live-1280` and `recovery-1280` after: the same layout, now on the dark glass look (theme colours, no old reds or backdrops).

- [ ] **Step 4: Run the checks**

- `node Tools/check-theme.mjs`: PASS.
- `node Tools/check-theme-tokens.mjs`: PASS.
- `node Tools/check-live-view.mjs`: PASS.
- `node Tools/check-live-now.mjs`: PASS.
- The standing checks: PASS (`check-shell-pages` loads `recovery.js`; `run-browser-tests` opens Recovery).

Unlink Playwright.

- [ ] **Step 5: Commit**

```bash
git add crates/reborn-ui/ui/pages/recovery.css crates/reborn-ui/ui/pages/live.css crates/reborn-ui/ui/recovery.html crates/reborn-ui/ui/game.html crates/reborn-ui/ui/live.html Tools/check-theme.mjs Tools/check-live-view.mjs
git commit -m "ui: Recovery, game and live views load only theme.css + their layout (layout unchanged)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Online onto `theme.css`, layout unchanged

The Online page is redesigned separately (spec §1, §9). Here it only stops loading the old stack.

**Files:**
- Create: `crates/reborn-ui/ui/pages/online.css` (generated)
- Modify: `crates/reborn-ui/ui/online.html` (by `theme-page.mjs`), `Tools/check-theme.mjs`

**Interfaces:**
- Consumes: Tasks 1 and 8.
- Produces: `online` in `THEMED`.

- [ ] **Step 1: Extract, then switch**

Link Playwright, then (Git Bash):

```bash
node Tools/extract-layout-css.mjs online crates/reborn-ui/ui/pages/online.css
node Tools/check-theme-tokens.mjs
node Tools/theme-page.mjs online pages/online.css
```

In `Tools/check-theme.mjs`, add `online: "pages/online.css",` inside the `THEMED` object literal.

`online-mm.js` injects one `<style>` rule (`body[data-mm-busy="true"] #screen{display:none!important}`). It is not a stylesheet file and stays as it is.

- [ ] **Step 2: Compare with the before pictures**

```bash
node Tools/compare-page-layout.mjs capture online target/p4-after
node Tools/compare-page-layout.mjs diff online target/p4-before target/p4-after
```

Expected: `compare-page-layout online: PASS (0 fail, N warn)`. Fix FAIL lines and judge WARN lines exactly as in Task 9 Step 3 (same rules, file `pages/online.css`, body class `page-online`).

- [ ] **Step 3: Run the checks**

- `node Tools/check-theme.mjs`, `node Tools/check-theme-tokens.mjs`: PASS.
- `node Tools/check-online-restored.mjs`: PASS.
- `node Tools/check-live-now.mjs`: PASS.
- `node Tools/check-online-mm.mjs`: baseline failure. Its first FAIL line must be the same as in `target/p4-before/check-online-mm.log`.
- The standing checks: PASS (`run-browser-tests` navigates to Online).

Unlink Playwright.

- [ ] **Step 4: Commit**

```bash
git add crates/reborn-ui/ui/pages/online.css crates/reborn-ui/ui/online.html Tools/check-theme.mjs
git commit -m "ui: Online loads only theme.css + its layout (layout unchanged; its redesign is separate)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Onboarding: `onboarding.css` rebased on the theme tokens

**Files:**
- Rewrite: `crates/reborn-ui/ui/onboarding.css` (generated; it absorbs `onboarding-polish.css`)
- Modify: `crates/reborn-ui/ui/onboarding.html` (by `theme-page.mjs`), `Tools/check-theme.mjs`, `Tools/check-theme-tokens.mjs`

**Interfaces:**
- Consumes: Tasks 1 and 8.
- Produces: `onboarding` in `THEMED` (file `onboarding.css`, spec §11); `node Tools/check-theme-tokens.mjs [path …]` accepts files as well as folders and checks `ui/pages` and `ui/onboarding.css` by default.

- [ ] **Step 1: Make sure only onboarding still loads `onboarding.css`**

Run (Git Bash): `grep -l 'href="onboarding.css"\|href="onboarding-polish.css"' crates/reborn-ui/ui/*.html`
Expected: exactly `crates/reborn-ui/ui/onboarding.html`. If any other page is listed, **stop**: that page was not moved onto the theme (plans 2/3, or Tasks 4, 6, 9, 10), and overwriting `onboarding.css` would change it.

- [ ] **Step 2: Extract to a scratch file, then replace `onboarding.css`**

Link Playwright, then:

```bash
node Tools/extract-layout-css.mjs onboarding target/p4-onboarding.css
cp target/p4-onboarding.css crates/reborn-ui/ui/onboarding.css
node Tools/theme-page.mjs onboarding onboarding.css
```

The extraction must run before the copy: it reads the old `onboarding.css` and `onboarding-polish.css` from the page. (If you ever need to run it again, restore the old files first with `git checkout HEAD -- crates/reborn-ui/ui/onboarding.css crates/reborn-ui/ui/onboarding.html`.)

In `Tools/check-theme.mjs`, add `onboarding: "onboarding.css",` inside the `THEMED` object literal.

- [ ] **Step 3: Let the token checker read `onboarding.css`**

Replace the whole of `Tools/check-theme-tokens.mjs` with:

```js
// Page stylesheets hold layout only (spec §11, §13): no colour literals, no weight above 700. Every colour comes from a
// theme.css token. Checks every .css in a folder, or a single file.
//     node Tools/check-theme-tokens.mjs [path ...]      (default: crates/reborn-ui/ui/pages and crates/reborn-ui/ui/onboarding.css)
import { readFileSync, readdirSync, existsSync, statSync } from "node:fs";
import path from "node:path";

const targets = process.argv.length > 2 ? process.argv.slice(2) : ["crates/reborn-ui/ui/pages", "crates/reborn-ui/ui/onboarding.css"];
const problems = [];
let files = 0;
for (const target of targets.map((t) => path.resolve(t))) {
  if (!existsSync(target)) continue;
  const list = statSync(target).isDirectory() ? readdirSync(target).filter((f) => f.endsWith(".css")).map((f) => path.join(target, f)) : [target];
  for (const file of list) {
    files++;
    const name = path.relative(process.cwd(), file);
    // Comments out, newlines kept so line numbers stay true.
    const css = readFileSync(file, "utf8").replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "));
    // Innermost { … } blocks are declarations; selectors (#main, #play-now) sit outside them.
    for (const block of css.matchAll(/\{([^{}]*)\}/g)) {
      const where = `${name}:${css.slice(0, block.index).split("\n").length}`;
      const body = block[1];
      for (const m of body.matchAll(/#[0-9a-fA-F]{3,8}\b/g)) problems.push(`${where}: colour literal ${m[0]}`);
      for (const m of body.matchAll(/\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\s*\(/gi)) problems.push(`${where}: colour function ${m[0]}`);
      for (const m of body.matchAll(/font-weight\s*:\s*(\d+)/g)) if (+m[1] > 700) problems.push(`${where}: font-weight ${m[1]}`);
      for (const m of body.matchAll(/(?:^|[;\s])font\s*:\s*(\d{3})\b/g)) if (+m[1] > 700) problems.push(`${where}: font weight ${m[1]}`);
    }
  }
}
if (problems.length) { console.error("check-theme-tokens: FAIL\n  " + problems.join("\n  ")); process.exit(1); }
console.log(`check-theme-tokens: PASS (${files} stylesheet(s))`);
```

Run `node Tools/check-theme-tokens.mjs`. Expected: PASS, counting `onboarding.css` among the files.

- [ ] **Step 4: Compare with the before pictures**

```bash
node Tools/compare-page-layout.mjs capture onboarding target/p4-after
node Tools/compare-page-layout.mjs diff onboarding target/p4-before target/p4-after --loose "section#screen > h1" --loose "nav\.shell-nav"
```

The two `--loose` patterns are the only elements whose font changed on purpose: the setup title and the setup sidebar used Anton (the header comment of the new `onboarding.css` lists them; spec §3.2 "Anton and Saira are no longer used"). Sideways changes there are WARNs, not FAILs.

Expected: `compare-page-layout onboarding: PASS (0 fail, N warn)` over all 16 screens. Fix FAIL lines and judge WARN lines exactly as in Task 9 Step 3 (file `onboarding.css`, body class `page-setup`).

- [ ] **Step 5: Run the checks**

- `node Tools/check-theme.mjs`, `node Tools/check-theme-tokens.mjs`: PASS.
- `node Tools/review-onboarding.mjs`: exit code as in `target/p4-before/checks.txt`.
- `node Tools/check-onboarding-ready.mjs`: PASS.
- `node Tools/check-controller-navigation.mjs`, `node Tools/check-controller-hints.mjs`: PASS (they drive the setup controller step).
- The standing checks: PASS. `run-browser-tests` walks every onboarding stage and must still print its main-body PASS line.

Open `target/p4-after/onboarding-welcome-1280.png`, `onboarding-controller_setup-1280.png` and `onboarding-completion-800.png` beside their before pictures: the same layout, on the theme's colours.

Unlink Playwright.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/ui/onboarding.css crates/reborn-ui/ui/onboarding.html Tools/check-theme.mjs Tools/check-theme-tokens.mjs
git commit -m "onboarding: onboarding.css rebased on theme tokens (polish folded in); onboarding loads only theme.css + onboarding.css

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Retire the old stylesheets

**Files:**
- Rename: `crates/reborn-ui/ui/styles.css` → `crates/reborn-ui/ui/harness.css`; modify `crates/reborn-ui/ui/harness.html` (its one `<link>`)
- Delete from `crates/reborn-ui/ui/`: `launcher.css`, `product-ui.css`, `finish.css`, `shell.css`, `play.css`, `players.css`, `stats.css`, `onboarding-polish.css`, `live-view.css`, `music.css`, `profile.css`
- Modify: `crates/reborn-ui/ui/theme.css` (the old variable aliases), `Tools/check-theme.mjs`

**Interfaces:**
- Consumes: every page themed (plans 2 and 3, Tasks 4, 6, 9, 10, 11).
- Produces: `check-theme.mjs` asserts the retired files are gone, every shell page is themed, and the harness keeps `harness.css`.

- [ ] **Step 1: Write the failing check**

In `Tools/check-theme.mjs`:
- change the `node:fs` import to `import { readFileSync, existsSync, readdirSync } from "node:fs";`
- insert this block directly above the final `console.log(…)` line:

```js
// Spec §11: the old stylesheets are retired once no page loads them. Every shell page is on the design system; the
// developer harness (spec §9, untouched) keeps its own sheet, renamed from styles.css.
const RETIRED = ["styles.css", "launcher.css", "product-ui.css", "finish.css", "shell.css", "play.css", "players.css", "stats.css", "onboarding-polish.css", "live-view.css", "music.css", "profile.css"];
for (const p of PAGES) assert.ok(THEMED[p], `${p} must load only theme.css + its page file (spec §11)`);
for (const f of RETIRED) assert.ok(!existsSync(ui + f), `ui/${f} is retired (spec §11) and must be gone`);
for (const f of readdirSync(ui).filter((x) => x.endsWith(".html"))) {
  const html = readFileSync(ui + f, "utf8");
  for (const r of RETIRED) assert.ok(!html.includes(`href="${r}"`), `${f} still loads ${r}`);
}
assert.match(readFileSync(ui + "harness.html", "utf8"), /href="harness\.css"/, "the developer harness keeps its own stylesheet");
assert.doesNotMatch(theme, /--(bg|panel|line|muted|accent|accent-border|control-bg|control-border|panel-border|selected-bg|focus|teal|motion-ease|motion-fast|motion-normal)\s*:/, "the old stylesheets' variable aliases are gone");
```

Run `node Tools/check-theme.mjs`. Expected: FAIL on `ui/styles.css is retired`.

- [ ] **Step 2: Make sure nothing loads them any more**

Run (Git Bash):

```bash
grep -nE 'href="(styles|launcher|product-ui|finish|shell|play|players|stats|onboarding-polish|live-view|music|profile)\.css"' crates/reborn-ui/ui/*.html
grep -rnE 'var\(--(bg|panel|line|muted|accent|accent-border|control-bg|control-border|panel-border|selected-bg|focus|teal|motion-ease|motion-fast|motion-normal)\)' crates/reborn-ui/ui --include=*.css --include=*.js --include=*.html
```

Expected:
- The first command prints exactly one line: `harness.html` loading `styles.css`. Any other line means a page was not moved: **stop** and report it.
- The second command prints only lines from `styles.css` (the harness's own literal-colour sheet does not use them either, so normally nothing). Any hit in another file must be changed to its token first (`--bg`→`transparent`, `--panel`→`--glass-1`, `--line`→`--stroke`, `--muted`→`--fg-2`, `--accent`→`--red`, `--accent-border`→`--stroke-hi`, `--control-bg`→`--glass-2`, `--control-border`→`--stroke`, `--panel-border`→`--stroke`, `--selected-bg`→`--glass-3`, `--focus`→`#fff` in `theme.css` only, `--teal`→`--ok`, `--motion-ease`→`--ease`, `--motion-fast`→`--d1`, `--motion-normal`→`--d2`).

- [ ] **Step 3: Rename the harness sheet, delete the rest, drop the aliases**

```bash
git mv crates/reborn-ui/ui/styles.css crates/reborn-ui/ui/harness.css
git rm --ignore-unmatch crates/reborn-ui/ui/launcher.css crates/reborn-ui/ui/product-ui.css crates/reborn-ui/ui/finish.css crates/reborn-ui/ui/shell.css crates/reborn-ui/ui/play.css crates/reborn-ui/ui/players.css crates/reborn-ui/ui/stats.css crates/reborn-ui/ui/onboarding-polish.css crates/reborn-ui/ui/live-view.css crates/reborn-ui/ui/music.css crates/reborn-ui/ui/profile.css
```

In `crates/reborn-ui/ui/harness.html` replace `href="styles.css"` with `href="harness.css"`. Change nothing else in the harness.

In `crates/reborn-ui/ui/theme.css`, delete these five lines from the first `:root{…}` block:

```css
  /* the old stylesheets' variables, pointed at the new tokens (retired in the Cleanup plan) */
  --bg:transparent;--panel:var(--glass-1);--line:var(--stroke);--muted:var(--fg-2);--accent:var(--red);
  --accent-border:var(--stroke-hi);--control-bg:var(--glass-2);--control-border:var(--stroke);
  --panel-border:var(--stroke);--selected-bg:var(--glass-3);--focus:#fff;--teal:var(--ok);
  --motion-ease:var(--ease);--motion-fast:var(--d1);--motion-normal:var(--d2);
```

(If plan 2 or 3 already changed that block, delete whatever alias declarations of those fifteen names remain, and nothing else.)

- [ ] **Step 4: Prove nothing moved**

Link Playwright, then (Git Bash):

```bash
rm -rf target/p4-final
for p in recovery game live online onboarding; do node Tools/compare-page-layout.mjs capture $p target/p4-final; done
for p in recovery game live online; do node Tools/compare-page-layout.mjs diff $p target/p4-after target/p4-final || exit 1; done
node Tools/compare-page-layout.mjs diff onboarding target/p4-after target/p4-final
```

Expected: five `PASS (0 fail, 0 warn)` lines. The comparison is against Tasks 9–11's after pictures: deleting files nobody loads, and aliases nobody reads, must change nothing.

Then run:
- `node Tools/check-theme.mjs`: PASS.
- `node Tools/check-theme-tokens.mjs`: PASS.
- `node Tools/check-stats-page.mjs`, `node Tools/check-profile-page.mjs`, `node Tools/check-stat-components.mjs`: PASS.
- The standing checks: PASS.
- `node Tools/check-harness-args.mjs`: PASS (the harness page is untouched apart from its sheet's name).

Open `crates/reborn-ui/ui/harness.html` directly in Edge (a `file://` URL). It must look exactly as before: its own plain look (system font, two colours), not the glass theme.

Unlink Playwright.

- [ ] **Step 5: Commit**

```bash
git add -A crates/reborn-ui/ui/harness.css crates/reborn-ui/ui/harness.html crates/reborn-ui/ui/theme.css Tools/check-theme.mjs
git commit -m "ui: retire the old stylesheets (spec §11); the harness keeps its own sheet as harness.css

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

(`git rm` already staged the deletions; `git add -A` on the listed paths stages the rename.)

---

### Task 13: Rewrite `DESIGN.md` to the spec

**Files:**
- Rewrite: `crates/reborn-ui/ui/DESIGN.md`
- Modify: `Tools/check-theme.mjs`

**Interfaces:**
- Consumes: the finished `theme.css` (tokens, components) and the page files.
- Produces: `DESIGN.md` naming every token `check-theme.mjs` requires, keeping the opening warning line; `check-theme.mjs` asserts both.

- [ ] **Step 1: Write the failing check**

In `Tools/check-theme.mjs`, insert directly above the final `console.log(…)` line:

```js
// DESIGN.md is the written form of theme.css (spec §11): it keeps its opening warning and names every token.
const design = readFileSync(ui + "DESIGN.md", "utf8");
assert.match(design, /If a value is not in this file, it is not a decision anyone gets to make in a stylesheet\./, "DESIGN.md keeps its opening warning");
assert.doesNotMatch(design, /Direction A, approved/, "DESIGN.md no longer describes Direction A as current");
for (const t of ["--mica-tint", "--fallback-bg", "--glass-1", "--glass-2", "--glass-3", "--stroke", "--stroke-hi", "--sheen", "--shadow", "--fg", "--fg-2", "--fg-3", "--red", "--red-hi", "--red-ink", "--ok", "--warn", "--ease", "--d1", "--d2", "--d3"])
  assert.ok(design.includes(`\`${t}\``), `DESIGN.md names ${t}`);
```

Run `node Tools/check-theme.mjs`. Expected: FAIL (`DESIGN.md no longer describes Direction A as current`).

- [ ] **Step 2: Rewrite `DESIGN.md`**

Replace the whole of `crates/reborn-ui/ui/DESIGN.md` with:

````markdown
# Locked design spec

**Frosted glass, approved by Adam on 2026-09-30.** A clean platform launcher (Steam, Battle.net) on real OS glass: every page keeps its layout, and every page shares one set of colours, one type and spacing scale and one motion system. It replaces "Direction A" (art-led, 2026-09-01). The full reasoning is in `docs/superpowers/specs/2026-09-30-launcher-glass-redesign-design.md`; the approved mockup is `docs/superpowers/specs/assets/2026-09-30-launcher-glass-mockup.html`.

Every value below lives in `ui/theme.css` and nowhere else. **If a value is not in this file, it is not a decision anyone gets to make in a stylesheet.**

---

## 1. Colour

| Token | Value | Role |
|---|---|---|
| `--mica-tint` | `rgba(18,19,23,.66)` | painted over the OS material when glass is on |
| `--fallback-bg` | radial `#23262d` centre on `#0d0e11` | the solid look: glass off, Linux, Steam Deck, Win 10 |
| `--glass-1` | `rgba(255,255,255,.045)` | card |
| `--glass-2` | `rgba(255,255,255,.075)` | button, chip, hover |
| `--glass-3` | `rgba(255,255,255,.11)` | selected tab, selected nav item |
| `--stroke` | `rgba(255,255,255,.09)` | every border and row rule |
| `--stroke-hi` | `rgba(255,255,255,.16)` | a stronger line: Total rows, floating surfaces |
| `--sheen` | `inset 0 1px 0 rgba(255,255,255,.07)` | the top highlight of a glass surface |
| `--shadow` | `0 12px 40px -12px rgba(0,0,0,.55)` | a card's drop shadow |
| `--fg` | `#F4F5F7` | text |
| `--fg-2` | `#AEB5C0` | secondary text |
| `--fg-3` | `#7C8491` | labels, meta, table headers |
| `--red` | `#E0272E` | **the accent**, as a fill: Play now, primary buttons, the nav marker, switches on |
| `--red-hi` | `#F04A50` | hover on a red fill |
| `--red-ink` | `#FF7A7F` | red as text, and **L** |
| `--ok` | `#3DDC97` | **W**, "Ready", Elo up, online |
| `--warn` | `#FFC46B` | "Setup required", warnings |

- **W is `--ok`, L is `--red-ink`. There is no T:** ties do not exist in Reborn.
- The accent is red. No purple, no teal accent (the one exception is the Dolphin tile, `--dolphin`).
- Profile covers and opponent avatars take `--c1`/`--c2` from the Discord avatar (`cover-color.js`).
- Portraits sit on the team's `primary`, dimmed (`--tc`, set per portrait from the server).
- Surfaces with a job of their own: `--field-bg`, `--hero-scrim`, `--on-art`, `--avatar-bg`, `--dolphin`.

## 2. Window material

| Platform | Material |
|---|---|
| Windows 11 | Mica, under `--mica-tint` |
| Windows 10, Linux, Steam Deck, glass turned off | the solid look, `--fallback-bg` |
| macOS | vibrancy, under `--mica-tint` |

- `html[data-glass="on|off"]` always matches what the OS actually applied (`window_glass_apply`).
- Settings › Display › Launcher window › **Window glass** turns it off at any time (it previews live and goes through the save bar).
- The Play hero art covers its own area either way.

## 3. Type

- **Inter** (`fonts/InterVariable.woff2`) for everything. Anton and Saira are not used. The logos are images, and they are Adam's: never resize, stretch or redraw `art/nsr-sidebar.png` or `art/play-logo.png`.
- **Weights:** 500 UI text, 600 values and labels, 700 headline numbers only. **Nothing above 700.**
- **Sizes:** page title 30/1.1 at 700; section title 16 at 600; table header 10.5 at 600, uppercase, tracking .06–.07em; body 13–14; meta 12–12.5.
- Numbers use `font-variant-numeric: tabular-nums` (`.num`).

## 4. Spacing, radius, size

- **Spacing:** 8 / 16 / 24 only. 24 between sections and columns; 16 inside cards and between a section title and its content; 8 inside rows.
- **Section headers are 40px** (`.sec-head`), whatever they hold, so neighbouring columns' cards start on one line.
- **Radius:** 14 cards (`--r-lg`), 10 buttons and fields (`--r-md`), 8 small controls (`--r-sm`), 999 switches and avatars.
- **Rows:** match 56, game 40, player 64, table about 40.
- **Width:** form pages (Settings, Mods, Music, Controllers) 1014px at most; Stats and Profile use the full content width.

## 5. Components (one definition each, in `theme.css`)

| Class | What |
|---|---|
| `.glass` | card: `--glass-1`, `--stroke`, radius 14, `--sheen` + `--shadow`, `backdrop-filter: blur(24px) saturate(1.3)` |
| `.btn`, `.btn.primary`, `.btn.quiet`, `.btn.sm` | 38px glass button; red gradient primary with a plain drop shadow; transparent; 32px |
| `.look-switch` | 40×22 switch, the thumb slides, on = red |
| `.tabs` (`tabs.js`) | segmented control, one highlight that slides |
| `dialog.menu` (`menu.js`) | anchored glass menu with a check on the current option |
| `.save-lane` / `.save-bar` (`savebar.js`) | the floating save bar in its own lane |
| `.sec-head`, `.sec-link`, `.sec-links`, `.k-label` | section header, its link(s), an uppercase label |
| `.m-list`, `.m-row`, `.m-hd`, `.m-game`, `.m-pager` (`match-rows.js`) | the match list of Stats and Profile |
| `.rows-reveal`, `.chev` | a best of 3 opening in place |
| `.tbl`, `tr.total` | a table: uppercase header, a fixed 220px name column shared by every table, a normal Total row |
| `.avatar` | a round Discord picture, or the first letter |
| `.player-face` | a round portrait on the team colour, the team logo as a badge |
| `.team-logo` | the backend's full-colour logo, or the abbreviation |
| `.cover-wash`, `.cover-pic`, `.presence-dot`, `.live-dot` | the profile cover, its avatar, the online dot, the pulsing live dot |

## 6. Motion

| Token | Slow (default) | Quick | Used for |
|---|---|---|---|
| `--d1` | 480 ms | 120 ms | hover, press |
| `--d2` | 720 ms | 180 ms | menus, switches, tabs |
| `--d3` | 960 ms | 240 ms | page enter, save bar, expanding rows, nav marker |

- One easing curve, `--ease` = `cubic-bezier(.2,.8,.2,1)`.
- Only `opacity` and `transform` animate. Heights use `grid-template-rows` (the best-of-3 reveal) or `max-height` (the save lane). The one other exception is the tab highlight's width.
- No bounce, glow, count-up numbers, parallax or looping motion, except the pulsing live dot.
- `html[data-motion="slow|quick"]` sets the durations **directly** (never a multiplier variable: custom properties resolve where they are declared). Settings › Display › **Slow animations** switches it; it is on by default.
- The OS reduce-motion setting always wins: every duration is 0.

## 7. The shell

- **Sidebar:** the logo slot as it is today; 8 items (Play, Mods, Music, Controllers, Online, Stats, and Settings + Help pinned at the bottom), 40px, the current SVG icons; hover `--glass-1`; active `--glass-3` + `--sheen` and **one** red marker that glides to it.
- **Page frame:** title (30px) and subtitle, then content. Each page scrolls on its own; the sidebar never does. Pages with a save bar (Settings, Music) put it in its own lane.
- **Controller prompts and the focus ring only while a controller is driving** (`body.gamepad-input`): the last input was a pad. Any mouse movement, click or key hides them. The ring is `outline: 2px solid #fff; outline-offset: 3px`, never a glow. Everything stays reachable by pad.

## 8. The save bar (Settings, Music)

Hidden until something changes. Then: a red dot, "N unsaved change(s)" (Music: "pending"), the last change's name, **Discard** and **Save changes** (Music: **Apply changes**). Discard puts every previewed setting back. It never covers content.

## 9. Words

- "Leaderboard", never "ladder". "Live feed", not "Watch live". "Last 10 games", not "Form". "Your players".
- A row never says "Best of 3" (every ranked match is one); the live card says "Game 2 of 3". There are no halves.
- Stat units are uppercase in tables (YDS, TD, RTG, C/ATT, CAR, YCO, DRP) and spelled out in headline tiles (Passing Yards, Passing TD, Passer Rating).
- Records are W–L. Private rooms never show on a profile and never count.

## 10. Files and checks

| File | Holds |
|---|---|
| `theme.css` | every token, the base, the material, the motion, every shared component, the shell, the save bar |
| `pages/<page>.css` | one page's layout, **no colour literals** |
| `pages/recovery.css`, `pages/live.css` (game + live), `pages/online.css`, `onboarding.css` | the layout the retired sheets gave the pages not redesigned (made by `Tools/extract-layout-css.mjs`), on theme tokens |
| `harness.css` | the developer harness only (its own plain look) |

- Each page loads exactly `theme.css` + its page file, with `body.themed` (`Tools/check-theme.mjs`).
- `Tools/check-theme-tokens.mjs`: no hex or `rgb()`/`hsl()` colours and no weight above 700 in page files.
- `Tools/check-stat-components.mjs`, `check-stats-page.mjs`, `check-profile-page.mjs`, `check-glass-pages.mjs`, `check-glass-stats-profile.mjs`: alignment, content and motion.

## 11. Never

- A glow, anywhere. A coloured `box-shadow`.
- A colour, size or duration in a page file that is not a `theme.css` token.
- Drawn or invented art: an empty preview stays neutral. Sample numbers from the mockup: every value comes from a command.
- Server text through `innerHTML`, or an image from anywhere but `assetUrl()` and the Discord avatar prefix.
- Resizing, stretching or redrawing Adam's logos.
````

- [ ] **Step 3: Run the check**

- `node Tools/check-theme.mjs`: PASS.
- The standing checks: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/reborn-ui/ui/DESIGN.md Tools/check-theme.mjs
git commit -m "docs: DESIGN.md rewritten to the glass design system (tokens, components, motion, rules)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: Spec §13 for Stats and Profile: screens in every look, motion, controller mode

**Files:**
- Create: `Tools/check-glass-stats-profile.mjs`

**Interfaces:**
- Consumes: everything above; `openState(…, {glass, slow, reducedMotion: 'no-preference', pad})`; plan 1's `.nav-marker` (`.still` until placed).
- Produces: screens in `target/glass-review/` (or `argv[2]`): `stats-<w>-<glass|solid>-<slow|quick>.png`, `stats-bo3-…`, `profile-live-…`, `profile-idle-…`, `stats-pad-1280.png`.

- [ ] **Step 1: Write the check**

Create `Tools/check-glass-stats-profile.mjs`:

```js
// Spec §13 for Stats and Profile: screens at 1280×800 and 800×560 in each look (glass on and off, slow and quick
// motion), a best of 3 open, Profile live and not live; the motion each uses (nav marker --d3, tab highlight --d2, the
// best-of-3 reveal --d3; reduced motion instant); controller mode (spec §6.3): a mouse user with a pad connected sees no
// prompts, pad input brings the prompts and a plain white ring, D-pad Down skips a closed best of 3's games and reaches
// them once A opens it, and a mouse move hides the prompts again.
//     node Tools/check-glass-stats-profile.mjs [outdir]      (default target/glass-review)
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { serve, openState, root } from './lib/page-states.mjs';
import * as F from './fixtures/stats-fixtures.mjs';

const out = path.resolve(process.argv[2] || path.join(root, 'target/glass-review'));
mkdirSync(out, { recursive: true });
const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const STATS = { name: 'stats', url: 'stats.html', scope: 'main' };
const PROFILE = { name: 'profile', url: `profile.html?player=${F.ME}`, scope: 'main' };
const style = (page, sel, prop) => page.locator(sel).first().evaluate((n, p) => getComputedStyle(n)[p], prop);
const noSideways = async (page, what) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${what}: sideways scroll`);
const settle = (page) => page.waitForFunction(() => { const m = document.querySelector('.nav-marker'); return m && !m.classList.contains('still'); });
const look = (page, glass) => page.waitForFunction((g) => document.documentElement.dataset.glass === (g ? 'on' : 'off'), glass);
const press = async (page, b) => {
  await page.evaluate((i) => { window.pads[0].buttons[i] = { pressed: true, value: 1 }; }, b); await page.waitForTimeout(90);
  await page.evaluate((i) => { window.pads[0].buttons[i] = { pressed: false, value: 0 }; }, b); await page.waitForTimeout(150);
};
const activeKey = (page) => page.evaluate(() => document.activeElement?.getAttribute('data-key'));
try {
  for (const viewport of [{ width: 1280, height: 800 }, { width: 800, height: 560 }]) for (const glass of [true, false]) for (const slow of [true, false]) {
    const tag = `${viewport.width}-${glass ? 'glass' : 'solid'}-${slow ? 'slow' : 'quick'}`;
    const t = slow ? { d2: '0.72s', d3: '0.96s' } : { d2: '0.18s', d3: '0.24s' };
    const opts = { viewport, glass, slow, reducedMotion: 'no-preference' };
    { // Stats
      const { page, errors, close } = await openState(browser, srv.base, STATS, opts);
      await page.locator('.st-cols').waitFor();
      await look(page, glass);
      await settle(page);
      assert.equal(await style(page, '.nav-marker', 'transitionDuration'), t.d3, 'nav marker at --d3');
      await page.waitForTimeout(slow ? 1000 : 350);
      assert.match(await style(page, '.st-col .tab-ind', 'transitionDuration'), new RegExp(`^${t.d2.replace('.', '\\.')}`), 'tab highlight at --d2');
      assert.equal(await style(page, '#games-m-01', 'transitionDuration'), t.d3, 'best-of-3 reveal at --d3');
      await noSideways(page, `stats ${tag}`);
      await page.screenshot({ path: path.join(out, `stats-${tag}.png`) });
      await page.locator('[data-key="match-m-01"]').click();
      await page.waitForTimeout(slow ? 1100 : 350);
      assert.ok(await page.locator('#games-m-01').evaluate((n) => n.getBoundingClientRect().height > 100), 'the best of 3 opened');
      await page.screenshot({ path: path.join(out, `stats-bo3-${tag}.png`) });
      assert.deepEqual(errors, []);
      await close();
    }
    for (const live of [true, false]) { // Profile, in a match and not
      const { page, errors, close } = await openState(browser, srv.base, PROFILE, { ...opts, data: { player_profile: F.profile({ live }) } });
      await page.locator('.pf-card').waitFor();
      await look(page, glass);
      assert.equal(await page.locator('.pf-live').count(), live ? 1 : 0);
      await page.waitForTimeout(slow ? 1000 : 350);
      await noSideways(page, `profile ${tag}`);
      await page.screenshot({ path: path.join(out, `profile-${live ? 'live' : 'idle'}-${tag}.png`) });
      assert.deepEqual(errors, []);
      await close();
    }
  }
  { // reduced motion: every state change is instant
    const { page, close } = await openState(browser, srv.base, STATS);
    await page.locator('.st-cols').waitFor();
    for (const sel of ['.nav-marker', '.st-col .tab-ind', '#games-m-01']) assert.match(await style(page, sel, 'transitionDuration'), /^0s/, `${sel} under reduced motion`);
    await close();
  }
  { // controller mode on Stats (spec §6.3)
    const { page, errors, close } = await openState(browser, srv.base, STATS, { pad: true });
    await page.locator('.st-cols').waitFor();
    assert.equal(await style(page, '.input-footer', 'display'), 'none', 'a pad connected, a mouse user: no prompts');
    await page.locator('[data-key="match-m-02"]').focus();
    await press(page, 12);                                   // the first pad input turns controller mode on
    await page.waitForFunction(() => document.body.classList.contains('gamepad-input'));
    assert.equal(await style(page, '.input-footer', 'display'), 'flex', 'pad input: the prompts');
    await page.locator('[data-key="match-m-01"]').focus();
    await press(page, 13);                                   // D-pad Down
    assert.equal(await activeKey(page), 'match-m-02', 'Down skips a closed best of 3’s games');
    await press(page, 12);                                   // D-pad Up
    assert.equal(await activeKey(page), 'match-m-01');
    await press(page, 0);                                    // A opens it
    assert.equal(await page.locator('[data-key="match-m-01"]').getAttribute('aria-expanded'), 'true');
    await press(page, 13);
    assert.equal(await activeKey(page), 'game-g-011', 'Down reaches Game 1 once it is open');
    const ring = await page.evaluate(() => { const s = getComputedStyle(document.activeElement); return `${s.outlineStyle} ${s.outlineWidth} ${s.outlineColor}`; });
    assert.equal(ring, 'solid 2px rgb(255, 255, 255)', 'a plain white focus ring, no glow');
    await page.screenshot({ path: path.join(out, 'stats-pad-1280.png') });
    await page.mouse.move(300, 300);
    await page.mouse.move(360, 340);
    await page.waitForFunction(() => !document.body.classList.contains('gamepad-input'));
    assert.equal(await style(page, '.input-footer', 'display'), 'none', 'mouse: the prompts go');
    assert.deepEqual(errors, []);
    await close();
  }
  console.log(`check-glass-stats-profile: PASS (screens in ${out})`);
} finally { await browser.close(); await srv.close(); }
```

- [ ] **Step 2: Run it**

Link Playwright. Run `node Tools/check-glass-stats-profile.mjs`.
Expected: `check-glass-stats-profile: PASS (screens in …target\glass-review)` and 49 screenshots (8 looks × 6, plus the pad shot).

If an assertion fails, the fix belongs in the task that owns the code: page layout in `pages/stats.css` / `pages/profile.css`; durations and components in `theme.css`; rows in `match-rows.js`. Fix it there, re-run that task's own check, then this one.

**If only the controller-mode D-pad assertions fail** because `app-shell.js`'s spatial navigation lands somewhere else (for example the right column), do **not** change `app-shell.js`: controller bugs are out of scope (spec §1). Check that the element it lands on is not a game row of the closed match (that is the rule that matters: closed games are unreachable), change the assertion to exactly that, and report the actual target to Adam.

Open these pairs beside the mockup: `stats-1280-glass-slow.png`, `stats-bo3-1280-glass-slow.png`, `stats-800-solid-quick.png`, `profile-live-1280-glass-slow.png`, `profile-idle-800-solid-quick.png`, `stats-pad-1280.png`. Check every one against the spec's rulings: no glow; a plain white ring only in the pad shot; red accent; W green / L red letters; no "Best of 3", "ladder" or "Private"; the logos untouched.

Unlink Playwright.

- [ ] **Step 3: Commit**

```bash
git add Tools/check-glass-stats-profile.mjs
git commit -m "checks: Stats and Profile in every look, motion and controller mode (spec §13)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: Whole-redesign verification and the real window

**Files:** none new, unless a check fails. A failing check goes back to the task that owns the code, as a fix commit.

- [ ] **Step 1: Every checker, individually, against the start of plan 4**

Link Playwright, then (PowerShell, worktree root):

```powershell
New-Item -ItemType Directory -Force target\p4-end | Out-Null
$list = @(Get-ChildItem Tools\check-*.mjs) + @(Get-Item Tools\review-players.mjs, Tools\review-onboarding.mjs, Tools\review-finish.mjs, Tools\test-menu-pad.mjs, Tools\test-controller-report.mjs)
$list | ForEach-Object { node $_.FullName *> "target\p4-end\$($_.BaseName).log"; "$($_.BaseName) $LASTEXITCODE" } | Set-Content target\p4-end\checks.txt
$before = @{}; Get-Content target\p4-before\checks.txt | ForEach-Object { $n, $c = $_ -split ' '; $before[$n] = $c }
Get-Content target\p4-end\checks.txt | ForEach-Object { $n, $c = $_ -split ' '; if ($c -ne '0') { "$n exit $c (at the start: $($before[$n]))" } }
node Tools\run-browser-tests.mjs target\browser-review *> target\p4-end\run-browser-tests.log
Select-String -Path target\p4-end\run-browser-tests.log -Pattern '^PASS: launcher flows'
node --test crates/reborn-ui/ui/stats-model.test.mjs crates/reborn-ui/ui/stats-rows.test.mjs crates/reborn-ui/ui/cover-color.test.mjs crates/reborn-ui/ui/profile-model.test.mjs crates/reborn-ui/ui/settings-draft.test.mjs Tools/lib/colour-tokens.test.mjs
```

Expected:
- Every checker that was `0` at the start is `0` now. This includes plan 2's and plan 3's checkers and the new `check-stat-components`, `check-stats-page`, `check-profile-page`, `check-glass-stats-profile` (new ones have no "at the start" value and must be `0`).
- Only baseline failures remain: `review-finish` and `check-online-mm` (same first FAIL line as their `target\p4-before` logs), and `check-launch-polish` only if its first failure is outside Stats and was already in the before log.
- The `run-browser-tests` main-body PASS line is found.
- Every `node --test` file passes.

- [ ] **Step 2: The pages not redesigned: final layout against the before pictures**

(Git Bash)

```bash
rm -rf target/p4-end-layout
for p in recovery game live online onboarding; do node Tools/compare-page-layout.mjs capture $p target/p4-end-layout; done
for p in recovery game live online; do node Tools/compare-page-layout.mjs diff $p target/p4-before target/p4-end-layout; done
node Tools/compare-page-layout.mjs diff onboarding target/p4-before target/p4-end-layout --loose "section#screen > h1" --loose "nav\.shell-nav"
```

Expected: five `PASS (0 fail, …)` lines, with the same WARN lines Tasks 9–11 reported and accepted.

Unlink Playwright.

- [ ] **Step 3: Rust stays green**

This plan touches no Rust, but the UI files are compiled into the binary.

```bash
taskkill /IM reborn-launcher.exe /F 2>NUL
cargo test --workspace -j 2
cargo clippy --workspace --all-targets
cargo build -p reborn-ui
```

Expected: the test count equals the one plans 2 and 3 ended with (report the number); clippy reports nothing new; the build succeeds.

- [ ] **Step 4: The real window (dev build, never the installed beta)**

The dev build shares its identifier and data folder with the installed beta, so:

1. Back up `%APPDATA%\NFL Street Reborn\config\settings.json` and record its SHA256 with `Get-FileHash`.
2. Close any running launcher: `taskkill /IM reborn-launcher.exe /F`.
3. Start `target\debug\reborn-launcher.exe`.

If `target\glass-real\shot.ps1` does not exist yet (plan 2 creates it), save this as `target\glass-real\shot.ps1`:

```powershell
param([string]$Name)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Ri, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(int f, int x, int y, int d, int e);
}
"@
$p = Get-Process reborn-launcher -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
[W]::SetForegroundWindow($p.MainWindowHandle) | Out-Null; Start-Sleep -Milliseconds 400
$r = New-Object W+R; [W]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$bmp = New-Object System.Drawing.Bitmap ($r.Ri - $r.L), ($r.B - $r.T)
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
New-Item -ItemType Directory -Force target\glass-real | Out-Null
$bmp.Save("target\glass-real\$Name.png"); $g.Dispose(); $bmp.Dispose()
"window: $($r.L),$($r.T) to $($r.Ri),$($r.B)"
```

WebView2 exposes no UI Automation items, so drive the window with clicks at window coordinates (`[W]::SetCursorPos(x,y); [W]::mouse_event(2,0,0,0,0); [W]::mouse_event(4,0,0,0,0)` in a one-off PowerShell call). Screenshot each item with `powershell -File target\glass-real\shot.ps1 <name>`:

1. **Stats** (`p4-01-stats`): Mica behind the sidebar; the two columns; real logos and portraits from the live server (or their abbreviation / letter fallbacks when the server is unreachable: never a broken image). If the account is signed out, the sign-in line and Profile & sync.
2. **A best of 3** (`p4-02-bo3`): click one; it opens slowly into its games. Click it again; it closes.
3. **Your profile ›** (`p4-03-profile`): the card with the avatar's cover colours, the summary line, the overview, Match history beside Career (and Leaderboard / Records if the deployed server sends them). The deployed server (`ec5027a`) sends no live card, Last 10 or ranks: they must simply be absent. **Do not press Share profile or View on web.**
4. **Back** (`p4-04-back`): returns to Stats.
5. **A game from Stats** (`p4-05-game`): the game view on the theme; ← Back returns to Stats.
6. **Online** (`p4-06-online`) and **Help** (`p4-07-help`): the same layouts as before, on the theme's colours. Do not start matchmaking.
7. **Minimum window** (`p4-08-stats-800`, `p4-09-profile-800`): resize to 800×560. One column, nothing scrolls sideways.
8. **Needs Adam** (do not fake these): a real pad on Stats (D-pad through the rows, A opening a best of 3); the onboarding flow (not reachable on a set-up install; covered by the browser screens in Task 11).

Close the app. Restore `settings.json` from the backup and confirm its SHA256 matches.

- [ ] **Step 5: Hand off to Adam (GATE)**

Report:
- the screenshots (`target/glass-real/`, `target/glass-review/`, `target/p4-after/`, and the before/after pairs in `target/p4-before/` and `target/p4-end-layout/`);
- every checker with its exit code, and the baseline failures by name;
- every WARN line from Tasks 9–11 with its reason;
- the "needs Adam" items;
- the decisions below.

**Do not build or publish a beta or an installer.** The redesign is complete on `feat/launcher-glass-redesign`; merging and building wait for Adam's go.

---

## Decisions this plan made (surface to Adam at handoff)

Each "DECISION:" is a place where the approved design needs data no command provides yet, or where the spec left a choice open.

1. **DECISION: Your players' statlines.** `stats_dashboard`'s `best_players` sends one number per role (`value` + `unit`), not the four-number statline of spec §8.6. The row shows that one number ("3,108 YDS") and switches to the full line automatically when the role's fields are present (`statLine`). This is **not** in `BACKEND-CHANGES-launcher-redesign.md`: it needs a new item (send each role row's own stat columns in `best_players`, `src/profile/players.js` `top()`).
2. **DECISION: team colour behind portraits** comes from the team objects' `primary` (backend §10, built, not deployed). Until it ships, portraits sit on the neutral default; nothing is guessed from team names.
3. **DECISION: the Leaderboard card** needs `elo.around` (backend §5/§14); it is hidden until that ships. "#7 on the leaderboard" uses `elo.rank`, which the deployed server already sends. **"Full leaderboard ›" is left out**: no page in the launcher shows a full leaderboard. "You" appears only on the viewer's own profile; on someone else's profile their row keeps their name (still highlighted).
4. **DECISION: Career ranks** ("#3 · 125.9 / G") need `career_rank` (backend §6); without it the tiles show value and per game only, and the footnote is hidden. Per game divides by `per_game.games` (the games with play-by-play the totals come from).
5. **DECISION: the overview's Last 10 games column** needs the profile's `recent_form` (backend §8); without it the overview has two columns. Its date range ("Sep 11 – Sep 26") is omitted (no field). The Elo column's note reads "N ranked matches" instead of the mockup's "Last 30 days" (no command gives a 30-day window). Peak Elo uses `elo.peak` (backend §11) or, before it ships, the history's highest point.
6. **DECISION: the live card and "● In a match"** need `header.live` (backend §7); until then the status falls back to "● Online now" (`header.online`), and there is no card.
7. **DECISION: Match history's All / Ranked / Casual filter runs in the launcher.** `player_profile` takes no `queue` argument (backend §12's server filter would need a Rust command change, which this plan does not make). "Older" loads more pages with `before` until the page is full; the pager says "Page N" until the whole list is loaded, then "Page N of M".
8. **DECISION: private rooms.** On Profile they are dropped client-side (the deployed server still lists them, backend §1), so "Private" never appears. On the owner's own Stats page a private match stays listed (the backend ruling) with the second line **"Private room"** and no Elo change. The spec only names "Offline" for that line: Adam may prefer no label.
9. **DECISION: Profile keeps the Stats v2 sections the approved layout dropped** (Head to head, By team, Per game, Best QB → receiver, Laterals & pitches, In wins vs losses, By game type) under **More stats** below Full stats, so nothing a player could see before is lost (spec §1 "Nothing a player could do before is lost"). Full stats tables keep their sortable headers.
10. **The pages not redesigned are moved by a tool,** not by hand: `Tools/extract-layout-css.mjs` copies their old layout rules verbatim behind an `html:not(#_)` prefix (so they outrank the theme's base the way they did) and maps colours to the nearest tokens; `Tools/compare-page-layout.mjs` proves no element moved sideways. `game` and `live` share `pages/live.css`. The setup title and sidebar lose Anton (spec §3.2).
11. **`styles.css` becomes `harness.css`** (the developer harness is "untouched", spec §9, but `styles.css` is retired). The old `ui/profile.css` is deleted too (unused after Task 6; not in the spec's list).
12. **`check-launch-polish`'s stale Stats block** (the pre-v2 searchable history) is replaced by the account-and-sync steps it also drove; `check-stats-page.mjs` covers the page.
13. **Profile's Back** goes back when the page was opened from inside the launcher, else to Stats. The Profile page title stays for screen readers but is visually hidden (the page starts with Back and the card, as in the mockup).
14. **The CSP is unchanged**: `img-src` already allows `https://api.nflstreetreborn.com/live-assets/` (logos, portraits through `assetUrl()`) and `https://cdn.discordapp.com/avatars/`.
