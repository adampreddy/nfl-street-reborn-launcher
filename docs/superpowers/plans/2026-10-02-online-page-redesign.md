# Online Page Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the launcher's Online page as a game menu (three mode tiles, one action row, a matchup card, a room view, Live now cards and an always-visible Leaderboard card) in the glass design system, with real lobby invite links, without changing any matchmaking, lobby, Netplay or live-game behaviour.

**Architecture:** `online.html` becomes a two-column grid: a mode area (`#mode-slot`, owned by `online-mm.js`) and Live now (`#live-slot`, `live-now.js`) on the left, the Leaderboard card (`#leaderboard-slot`, new `leaderboard.js`) on the right. `online-mm.js` keeps its state logic, commands and 1 s `mm_state` poll and only its render functions are rewritten; pure display helpers live in a new `online-model.js` with node unit tests. The old direct Netplay form (`online.js`, logic untouched) moves into a `<dialog>` opened by a new `online-direct.js`. Rust gains read-only display fields (opponent standing, `you_host`), a `leaderboard` command, a `mm_room_lookup` command and `room/` invite links; the backend and the website follow as their own phases, and the launcher hides every spot whose data the server does not send yet.

**Tech Stack:** Tauri 2 (Rust, `reborn-ui` + `reborn-core`), plain HTML + ES modules + `theme.css`, Node checks with Playwright on msedge (`Tools/*.mjs`), `node --test` for page models; backend: Node 22 + Express 5 + Postgres (`node --test`); website: PHP 8 (SiteGround).

**Spec:** `docs/superpowers/specs/2026-10-02-online-page-redesign-design.md` (mockup: `docs/superpowers/specs/assets/2026-10-02-online-page-mockup.html`, v11, 12 states). Read both before starting any task.

## Global Constraints

- Spec §1: "**Only the look and layout change.** Every matchmaking, lobby, private-room and live-game behaviour stays exactly as it is today: same commands, same arguments, same order, same polling."
- Spec §7: "Rule: a value the server does not send hides its spot; nothing is invented on the client. With ELO_ENABLED off (today), ranks/Elo/changes are hidden and the leaderboard card shows "Rankings start soon." in its place."
- Spec §8 is the complete list of wording changes. "Ranked · best of 3" → "Ranked · first to two wins" ("ruling: never label "Best of 3""). Every other existing sentence keeps its words.
- Spec §1/§8: checks are updated only where they assert old layout or old wording §8 changes; every behavioural assertion (commands, args, `#screen` hidden while busy, timer moves, presence_page sent, no page errors) stays.
- Layout: 1280×800; right column (Leaderboard card) 316px; left column = mode area above Live now.
- Pictures are placeholders: display copies, JPEG, at most 1100px wide, under `crates/reborn-ui/ui/art/online/`.
- DESIGN.md §3/§10: weights ≤ 700, no glows, no coloured `box-shadow`, page files hold layout only and every colour is a `theme.css` token (`node Tools/check-theme-tokens.mjs`). A new colour is a new token in `theme.css`, named in `DESIGN.md` (DESIGN.md §11).
- Focus ring: white 2px (theme.css); on the mode tiles offset 4px so it sits outside the red pick border (spec §6).
- Server text only through `el()`'s `text` / `textContent`, never `innerHTML`. Images only through `assetUrl()` (`/live-assets/…png`) and `avatarSrc()` (`https://cdn.discordapp.com/avatars/`) from `live-common.js`, or `match-rows.js`'s `avatar()` / `teamLogo()` which use them.
- Line endings: LF everywhere. Never introduce CRLF into a `.rs` file (tests read their own source with `include_str!`; `.gitattributes` says `* text=auto eol=lf`). After editing a `.rs` file run `git diff --check` and `file <path>` (must not say "CRLF").
- Launcher work happens on branch `feat/launcher-glass-redesign` in `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher redesign`. Backend work happens in `C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend` on a new branch `feat/online-page-backend`. The website file lives at `C:\Users\Adam\.claude\jobs\f4d73a58\tmp\site\join.php` (no repo; deployed by WinSCP). Never deploy the backend or the site without Adam's go-ahead.
- Never use bare `git stash` (shared stash stack across worktrees).

## Review Focus

- Typing in the room-code or chat field while the 1 s redraw runs: the field keeps its text and focus, even when it is still empty (owned by Task 13: focus restored by `data-key`).
- Long or hostile display names (40 characters, `<img src=x onerror=…>`) in the leaderboard, the matchup card, the room list and the online list: drawn as literal text, ellipsized, no sideways scroll (owned by Task 11: `online-lb-hostile` state).
- Partial or null server data (opponent with no name or standing, `rank: 0`, `best_of: null`, unreadable `relay_deadline`, leaderboard rows with null wins/Elo): every such spot hidden, the page never shows "undefined", "NaN" or "null" (owned by Task 5: `online-match-bare`; Task 11: `online-lb-hostile`).
- A room invite that arrives while the player is queued, in a match, or signed out: never joins on its own, no banner while busy, the invite survives sign-in and is cleared only by Join or Dismiss (owned by Task 12: `online-invite-busy`, `online-invite-signed-out`).
- The minimum window (800×560 inside): one column, no sideways scroll, the mode area still usable, the leaderboard below it (owned by Task 2: `online-small`).

## Rulings on spec ambiguities (made while planning; each is applied in the task named)

1. Spec §3.1 gives the Ranked name "Inter 800", DESIGN.md says nothing above 700 and `check-theme-tokens` fails on 800: the tiles use 700 (Task 4).
2. Spec §3.2/§3.5 ask for a sweeping red line; DESIGN.md §4 allowed only one loop (`.live-dot`): `.sweep` is added to `theme.css` as the second allowed loop, transform-only, stopped by reduced motion, and DESIGN.md §4 says so (Task 1).
3. Mockup caption "Ranked picked: the default" vs spec §1/§3.1 ("stored in `localStorage mm:mode` as today"): the default stays `casual` (today's `store.get('mm:mode', 'casual')`), so a first queue sends exactly the argument it sends today (Task 4).
4. Spec §2 calls the presence switch "Show me as online", but `presence_set_enabled` switches Discord rich presence, and §8 does not list this label: the switch keeps today's words "Show what I'm doing on Discord" (Task 8).
5. Spec §3.1 shows the quiet **Join** button; §8 keeps "Join by code": the button's visible text is "Join" and its accessible name (`aria-label`) is "Join by code" (Task 4).
6. "Direct connection ›" must stay reachable wherever the old form was reachable (signed out, offline, matchmaking off, a failed `mm_state`): the links line shows in every state except busy and idle-with-Ranked/Casual-picked (Task 2, Task 4).
7. Spec §3.9 "Disabled … only the Private room tile … with its action row. Behaviour unchanged": with matchmaking off the `mm_room_*` commands are refused, so the disabled action row's primary is **Direct connection** (opens the dialog, the only private play that works today) (Task 7).
8. Spec §3.1 shows "Open lobbies (n) ›" only when lobbies exist, but "Open a public lobby" lives inside that list and must never disappear: with zero lobbies the link reads "Open a public lobby ›" and opens the same panel with today's sentence "No open lobbies. Open one and someone can join you." (Task 9).
9. `RoomSummary.host` is `Option<String>` but the backend sends `host: {id, display_name}`, so `mm_rooms` fails with a protocol error whenever a public lobby exists (a pre-existing bug the lobby link depends on): the core decodes either shape (Task 9).
10. Spec §5 footer "Ranked · Game 2" / "Casual" replaces "Game 1 of 3"; DESIGN.md §9 said "The live card says 'Game 2 of 3'": spec §5 wins and DESIGN.md §9 is updated; `check-live-now` drops its avatar-badge and chip-contrast assertions (the card has no avatars and no colour chip) and asserts the validated `--tc` instead (Task 10).
11. Spec §4 "Full leaderboard ›" opens a web leaderboard that does not exist (backend `webRouter.js` has `/live`, `/live/:id`, `/games/:id`, `/u/:player` only; spec §10 puts the page out of scope): the link opens `https://online.nflstreetreborn.com/leaderboard` and is shown only while the leaderboard answer has rows (Elo on). The page must exist before `ELO_ENABLED=1` (flagged in Task 16).
12. Spec §3.3 "the opponent the same way" needs the opponent's picture, which no answer carries: Task 17 adds `opponent.avatar_url` beside `rank`/`rating`/`team`; the launcher draws the letter avatar until then.
13. Spec §3.10 "Discord rich presence Join … produces the same `room/` link": nothing in the launcher subscribes to Discord's ACTIVITY_JOIN today; this plan makes the secret's meaning explicit (`presence::join_link`) and tests that it parses as a room invite, but does not add an IPC subscriber (Task 12).
14. The spec's `leaderboard` command is listed under `main.rs`; it lives in `live_view.rs` beside `stats_dashboard` (same error type, same blocking pattern) and is registered in `main.rs` (Task 11).
15. Match chat exists today under the match; the mockup does not show it. It stays, under the matchup card (nothing that exists is removed) (Task 5).
16. While a direct Netplay room is open, its dialog reopens itself (the room stayed on screen today); it closes on its own only when matchmaking takes the player (Task 2).

## Baseline (recorded 2026-10-02 on `feat/launcher-glass-redesign` @ 05a6aa6, before any task)

| Check | Result |
|---|---|
| `check-online-mm` | FAIL: `held` misses "Nothing has changed on the ladder" (the page already says "nothing changed on the ladder"). Task 7 rewrites this expectation. |
| `check-live-view` | FAIL: `ERR_UNSUPPORTED_RESOLVE_REQUEST` for `./live-game-render.js` from its data: URL loader (unrelated to this plan; leave it). |
| `check-online-restored`, `check-live-now`, `check-session-state`, `check-themed-shell`, `check-theme`, `check-theme-tokens`, `check-shell-pages`, `check-page-call-args`, `check-menu`, `check-shell-chrome`, `check-controller-hints`, `check-pad-columns`, `check-controller-navigation` | PASS |

Every task ends with all of these back to this baseline or better. Task 7 turns `check-online-mm` green.

## File map

| File | Responsibility |
|---|---|
| `crates/reborn-ui/ui/online.html` | page structure: heading + meta, grid, slots, links line, direct dialog |
| `crates/reborn-ui/ui/pages/online.css` | rewritten from scratch: layout only, tokens only |
| `crates/reborn-ui/ui/online-model.js` + `online-model.test.mjs` (new) | pure display helpers (words, numbers, the leaderboard model) |
| `crates/reborn-ui/ui/online-mm.js` | matchmaking panel: render functions rewritten; state logic, commands and polling unchanged |
| `crates/reborn-ui/ui/online-direct.js` (new) | the direct Netplay dialog: open, auto-open, close |
| `crates/reborn-ui/ui/online.js` | one added line: tells the dialog an invite filled the form |
| `crates/reborn-ui/ui/live-now.js` | card markup, slot, live count, See all; polling untouched |
| `crates/reborn-ui/ui/leaderboard.js` (new) | the Leaderboard card and the 60 s `leaderboard` + `stats_dashboard` poll |
| `crates/reborn-ui/ui/menu.js` | `openPanel()` beside `openMenu()` |
| `crates/reborn-ui/ui/app-shell.js` | the cancelable `pad-back` event |
| `crates/reborn-ui/ui/home.js` | Play hands a room invite to Online too |
| `crates/reborn-ui/ui/theme.css`, `DESIGN.md` | five tokens, `.sweep`, `dialog.menu.panel`; Online leaves the "not redesigned" list |
| `crates/reborn-ui/ui/art/online/*.jpg` (new) | placeholder display copies |
| `crates/reborn-core/src/matchmaking.rs` | `Room.you_host`, `Opponent` standing fields, tolerant `RoomSummary.host`, `room_lookup` |
| `crates/reborn-core/src/live_view.rs` | `leaderboard()` client |
| `crates/reborn-core/src/presence.rs`, `presence_discord.rs` | `join_link()`; the secret's meaning |
| `crates/reborn-ui/src/online_mm.rs` | `snapshot_json` display fields, `mm_room_lookup` |
| `crates/reborn-ui/src/live_view.rs` | `leaderboard` command |
| `crates/reborn-ui/src/main.rs` | invite kinds (`join/`, `room/`), `mm_pending_room(_clear)`, registrations |
| `crates/reborn-ui/src/shell_tools.rs` | `open_website` allows exactly `/live` and `/leaderboard` on the page origin |
| `Tools/lib/page-states.mjs` | the redesign's fixed states |
| `Tools/check-online-page.mjs` (new) | every state of the redesign, static rules, layout |
| `Tools/check-online-mm.mjs`, `check-online-restored.mjs`, `check-session-state.mjs`, `check-themed-shell.mjs`, `check-live-now.mjs`, `review-finish.mjs`, `run-browser-tests.mjs` | updated for layout/wording only |
| Backend `src/profile/load.js`, `src/mm/presence.js`, `src/mm/snapshot.js`, `src/mm/rooms.js`, new `src/mm/teamView.js`, tests | `week_delta`, `GET /v1/leaderboard`, opponent standing, `GET /v1/rooms/:code` |
| `C:\Users\Adam\.claude\jobs\f4d73a58\tmp\site\join.php` | `?r=` room links |

---

# Phase 1: Launcher

### Task 1: Theme tokens, the sweep, placeholder art, and the page check's static part

**Files:**
- Modify: `crates/reborn-ui/ui/theme.css` (`:root` block; components after `.live-dot`)
- Modify: `crates/reborn-ui/ui/DESIGN.md` (§1 tables, §3 P4-29 sentence, §4 loops, §6 table, §8 "Pages not redesigned" line)
- Create: `crates/reborn-ui/ui/art/online/ranked.jpg`, `casual.jpg`, `private.jpg`, `matchup.jpg`
- Modify: `crates/reborn-ui/ui/art/README.md` (one row)
- Create: `Tools/check-online-page.mjs`

**Interfaces:**
- Consumes: nothing.
- Produces: tokens `--tile-scrim`, `--vs-scrim`, `--pill-bg`, `--lock-bg`, `--red-wash`; component class `.sweep` (absolute, bottom of its positioned parent); art paths `art/online/{ranked,casual,private,matchup}.jpg`; `Tools/check-online-page.mjs` with an `EXPECT` object whose entries later tasks add above the line `  // (states are added above this line)`. An entry is `'<state name>': { text?: string[], absent?: string[], check?: async (page) => string[] }`; the state of the same name must exist in `Tools/lib/page-states.mjs` (`STATES.online`) and may carry `init` (JS string run before the page), `pad` (bool), `viewport` (`{width,height}`), `click` (selector clicked after load).

- [ ] **Step 1: Write the failing check**

Create `Tools/check-online-page.mjs`:

```js
// The Online page redesign (docs/superpowers/specs/2026-10-02-online-page-redesign-design.md): every mockup state on
// the real page, from the fixed states in lib/page-states.mjs (frozen clock 2026-09-30 20:00 UTC, no network), plus the
// static rules the page's files must keep. Fails on a missing text or control, a broken layout rule, a sideways scroll
// or any page error. Screenshots land in target/online-page.
//     node Tools/check-online-page.mjs
import { chromium } from 'playwright';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { STATES, root, serve, openState } from './lib/page-states.mjs';

const ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.join(root, 'target/online-page');
mkdirSync(out, { recursive: true });
let failures = 0;
const fail = (name, msg) => { failures++; console.log(`FAIL ${name}: ${msg}`); };
const ok = (name) => console.log(`ok   ${name}`);
const calls = (page, cmd) => page.evaluate((c) => window.__calls.filter((x) => x.cmd === c).map((x) => x.args), cmd);

// ---- static: tokens, the sweep, the art ----
{
  const theme = readFileSync(path.join(ui, 'theme.css'), 'utf8');
  const design = readFileSync(path.join(ui, 'DESIGN.md'), 'utf8');
  const problems = [];
  for (const t of ['--tile-scrim', '--vs-scrim', '--pill-bg', '--lock-bg', '--red-wash']) {
    if (!new RegExp(`${t}\\s*:`).test(theme)) problems.push(`theme.css does not define ${t}`);
    if (!design.includes(`\`${t}\``)) problems.push(`DESIGN.md does not name ${t}`);
  }
  if (!/\.sweep::after\{[^}]*animation:sweep /.test(theme)) problems.push('theme.css has no .sweep loop');
  if (!design.includes('`.sweep`')) problems.push('DESIGN.md does not name .sweep');
  // A JPEG's width from its first SOFn marker (no image library in Tools).
  const jpegWidth = (buf) => {
    let i = 2;
    while (i + 9 < buf.length && buf[i] === 0xff) {
      const m = buf[i + 1], len = buf.readUInt16BE(i + 2);
      if (m >= 0xc0 && m <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(m)) return buf.readUInt16BE(i + 7);
      i += 2 + len;
    }
    return null;
  };
  for (const f of ['ranked', 'casual', 'private', 'matchup']) {
    const p = path.join(ui, `art/online/${f}.jpg`);
    if (!existsSync(p)) { problems.push(`art/online/${f}.jpg is missing`); continue; }
    const w = jpegWidth(readFileSync(p));
    if (!w || w > 1100) problems.push(`art/online/${f}.jpg is ${w}px wide (a display copy is at most 1100)`);
  }
  if (problems.length) fail('static', problems.join('; ')); else ok('static');
}

// ---- every state ----
const EXPECT = {
  // (states are added above this line)
};

const byName = Object.fromEntries((STATES.online || []).map((s) => [s.name, s]));
if (Object.keys(EXPECT).length) {
  const server = await serve();
  const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
  try {
    for (const [name, exp] of Object.entries(EXPECT)) {
      const state = byName[name];
      if (!state) { fail(name, 'no such state in Tools/lib/page-states.mjs'); continue; }
      const { page, errors, close } = await openState(browser, server.base, state, { pad: !!state.pad, init: state.init ?? null, viewport: state.viewport });
      const problems = [];
      try {
        const text = (await page.locator('body').innerText()).toLowerCase();
        for (const t of exp.text ?? []) if (!text.includes(t.toLowerCase())) problems.push(`missing text "${t}"`);
        for (const t of exp.absent ?? []) if (text.includes(t.toLowerCase())) problems.push(`should not show "${t}"`);
        if (exp.check) problems.push(...(await exp.check(page)));
        if (await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)) problems.push('the page scrolls sideways');
        await page.screenshot({ path: path.join(out, `${name}.png`) });
      } catch (e) {
        problems.push(`threw: ${e.message.split('\n')[0]}`);
      } finally {
        await close();
      }
      if (errors.length) problems.push(`page errors: ${errors.join(' | ')}`);
      if (problems.length) fail(name, problems.join('; ')); else ok(name);
    }
  } finally {
    await browser.close();
    await server.close();
  }
}
if (failures) { console.error(`check-online-page: FAIL (${failures})`); process.exit(1); }
console.log(`check-online-page: PASS (${Object.keys(EXPECT).length} state(s))`);
```

- [ ] **Step 2: Run it to see it fail**

Run: `node Tools/check-online-page.mjs`
Expected: `FAIL static: theme.css does not define --tile-scrim; …; art/online/ranked.jpg is missing; …` and exit code 1.

- [ ] **Step 3: Add the tokens and the sweep to `theme.css`**

In the `:root{…}` block, after the line `  --mix-light:#fff;--mix-dark:#000;--game-row-bg:rgba(0,0,0,.14);`, add:

```css
  /* Online (spec 2026-10-02): the scrim over a mode tile's picture and over the matchup card, a pill on art, the
     sign-in panel over the tiles, your own leaderboard row */
  --tile-scrim:linear-gradient(0deg,rgba(6,7,9,.9) 0,rgba(6,7,9,.2) 50%,rgba(6,7,9,0) 75%);
  --vs-scrim:linear-gradient(0deg,rgba(6,7,9,.95),rgba(6,7,9,.55));
  --pill-bg:rgba(10,11,14,.6);
  --lock-bg:rgba(13,14,17,.72);
  --red-wash:rgba(224,39,46,.12);
```

After the `.live-dot{…}` rule, add:

```css
/* a red line sweeping along the bottom of a surface while something is moving (Online: searching, a match starting;
   spec 2026-10-02 §3.2, §3.5). The second and last loop besides .live-dot: transform only, and reduced motion stops it
   (the prefers-reduced-motion rule at the top sets every animation to none). Its parent must be positioned. */
.sweep{position:absolute;left:0;right:0;bottom:0;height:2px;overflow:hidden;pointer-events:none}
.sweep::after{content:"";position:absolute;top:0;bottom:0;left:0;width:38%;border-radius:2px;background:var(--red);animation:sweep 1.6s var(--ease) infinite}
@keyframes sweep{from{transform:translateX(-100%)}to{transform:translateX(265%)}}
```

- [ ] **Step 4: Name them in `DESIGN.md`**

In §1, at the end of the table under "### Surfaces with a job of their own", add these rows:

```markdown
| `--tile-scrim` | 0deg `rgba(6,7,9,.9)` to transparent | the Online mode tiles' picture scrim |
| `--vs-scrim` | 0deg `rgba(6,7,9,.95)` to `.55` | the Online matchup card's picture scrim |
| `--pill-bg` | `rgba(10,11,14,.6)` | a pill on art (Online "searching" counts, the relay step) |
| `--lock-bg` | `rgba(13,14,17,.72)` | the Online sign-in panel over the tiles |
| `--red-wash` | `rgba(224,39,46,.12)` | your own row on the Online leaderboard |
```

In §3, change the sentence `(exception, ruling P4-29: the pages not redesigned, Recovery, game, live, Online and onboarding, keep the legacy page title, 28px at 600, from their layout sheets; spec §9)` to `(exception, ruling P4-29: the pages not redesigned, Recovery, game, live and onboarding, keep the legacy page title, 28px at 600, from their layout sheets; spec §9)`.

In §4, change `- No bounce, count-up numbers, parallax or looping motion. The one loop is `.live-dot`.` to:

```markdown
- No bounce, count-up numbers, parallax or looping motion. The two loops are `.live-dot` and `.sweep` (a red line along the bottom of a surface while something is moving; Online, spec 2026-10-02).
```

In §6's component table, after the `.cover-wash, …` row, add:

```markdown
| `.sweep` | a 2px red line sweeping along the bottom of its positioned parent; transform only | Online |
```

In §8, change `- Pages not redesigned in plan 4 (recovery, live, online, onboarding) carry the layout their old sheets gave them (made once by `Tools/extract-layout-css.mjs`), on theme tokens.` to:

```markdown
- Pages not redesigned in plan 4 (recovery, live, onboarding) carry the layout their old sheets gave them (made once by `Tools/extract-layout-css.mjs`), on theme tokens. Online was redesigned on 2026-10-02 (`docs/superpowers/specs/2026-10-02-online-page-redesign-design.md`); `pages/online.css` is written by hand.
```

- [ ] **Step 5: Make the placeholder art**

Run (from the repo root):

```bash
python - <<'EOF'
from PIL import Image
art = "crates/reborn-ui/ui/art/"
for name, src in {"ranked": "hero-field.jpg", "casual": "hero-street.jpg", "private": "hero-desert.jpg", "matchup": "hero-giants.jpg"}.items():
    im = Image.open(art + src).convert("RGB")
    w = min(1100, im.width)
    im = im.resize((w, round(im.height * w / im.width)), Image.LANCZOS)
    im.save(f"{art}online/{name}.jpg", "JPEG", quality=90, optimize=True)
    print(name, im.size)
EOF
```

Create the folder first if Python complains: `mkdir -p crates/reborn-ui/ui/art/online`.
Expected output: four lines, each width 1100.

In `crates/reborn-ui/ui/art/README.md`, add this row to the "What to drop where" table:

```markdown
| `online/ranked.jpg`, `casual.jpg`, `private.jpg`, `matchup.jpg` | at most 1100 wide | JPEG, quality 90 | Online mode tiles and the matchup card (placeholders cut from the hero shots; Adam picks the final art) |
```

- [ ] **Step 6: Run the checks**

Run: `node Tools/check-online-page.mjs && node Tools/check-theme.mjs && node Tools/check-theme-tokens.mjs`
Expected: `ok   static`, `check-online-page: PASS (0 state(s))`, `check-theme: PASS (12 pages)`, `check-theme-tokens: PASS (…)`.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/theme.css crates/reborn-ui/ui/DESIGN.md crates/reborn-ui/ui/art/online crates/reborn-ui/ui/art/README.md Tools/check-online-page.mjs
git commit -m "online: theme tokens, the sweep, placeholder art, check-online-page static rules"
```

---

### Task 2: Page skeleton, the new stylesheet, and the direct-connection dialog

**Files:**
- Modify: `crates/reborn-ui/ui/online.html` (whole `<main>`, scripts)
- Replace: `crates/reborn-ui/ui/pages/online.css` (delete all 65 KB of generated layout; write the new file)
- Create: `crates/reborn-ui/ui/online-direct.js`
- Modify: `crates/reborn-ui/ui/online.js:94-104` (one line in `takePendingInvite`)
- Modify: `crates/reborn-ui/ui/online-mm.js:41-43` (mount into `#mode-slot`)
- Modify: `crates/reborn-ui/ui/live-now.js:142-144` (mount into `#live-slot`)
- Modify: `Tools/lib/page-states.mjs` (redesign state helpers + two states)
- Modify: `Tools/check-online-page.mjs` (two entries)
- Modify: `Tools/check-online-restored.mjs`, `Tools/check-session-state.mjs`, `Tools/check-themed-shell.mjs`, `Tools/review-finish.mjs`

**Interfaces:**
- Consumes: Task 1 tokens.
- Produces (DOM ids later tasks use): `#mode-slot` (online-mm.js appends `section.mm`), `#live-slot` (live-now.js), `#leaderboard-slot` (leaderboard.js, Task 11), `#on-links` (the links line; online-mm.js toggles `hidden`), `#direct-open` (opens the dialog), `#lobbies-open` (hidden until Task 9), `#on-players` (hidden until Task 8), `#on-live-count` (hidden until Task 10), `#direct-dialog` (contains `#screen`), `#direct-close`. Event `netplay-invite` on `document` (online.js → online-direct.js). In `page-states.mjs`: module constants `LB_TEAM`, `LEADERBOARD`, `DASH`, and inside `onlineStates()` the helpers `PICK(mode)` (init string) and `N(name, data, extra)` (a redesign state with `leaderboard: LEADERBOARD` and `stats_dashboard: DASH` by default); new states are added above the line `    // (redesign states are added above this line)`.

- [ ] **Step 1: Add the states and the failing expectations**

In `Tools/lib/page-states.mjs`, after the line `const LIST = JSON.parse(…);` add:

```js
// The Online redesign's leaderboard and dashboard answers (spec 2026-10-02 §4, §7). adamknowsball is #7, as in the
// Stats fixture's elo(); week_delta is the backend's Task 15 field.
export const LB_TEAM = { tgid: 14, name: 'Falcons', abbrev: 'ATL', logo_url: '/live-assets/teams/14-128.png', primary: '#A71930' };
const LB_NAMES = ['Jock', 'Playz', 'Tuna', 'Mikel', 'Riley', 'Sauce', 'adamknowsball', 'Bones', 'Vick7', 'Kaz'];
const lbRow = (name, i) => ({ rank: i + 1, player_id: `00000000-0000-4000-8000-0000000000${String(i).padStart(2, '0')}`, display_name: name,
  avatar_url: i % 3 ? null : `https://cdn.discordapp.com/avatars/12345678901234567${i}/0123456789abcdef0123456789abcdef.png?size=64`,
  team: i % 2 ? null : LB_TEAM, wins: 20 - i, losses: 3 + i, elo: 1320 - i * 18, delta_week: i % 4 === 0 ? 16 : i % 4 === 1 ? -9 : 0, you: name === 'adamknowsball' });
export const LEADERBOARD = { ok: true, enabled: true, total: 38, rows: LB_NAMES.map(lbRow), you: null };
export const DASH = (() => { const d = F.dashboard(); d.you.elo.week_delta = 16; return d; })();
```

Inside `onlineStates()`, after `const S = (name, data) => …;` add:

```js
  // The redesign's states (spec 2026-10-02; Tools/check-online-page.mjs). PICK sets the stored mode before load.
  const PICK = (m) => `try{localStorage.setItem('mm:mode','${m}')}catch{}`;
  const N = (name, data, extra = {}) => ({ name, url: 'online.html', scope: 'main', data: { leaderboard: LEADERBOARD, stats_dashboard: DASH, mm_rooms: [], ...data }, ...extra });
```

and change the end of the returned array from

```js
    S('online-room-running', room({ phase: 'running', can_start: false })),
  ];
```

to

```js
    S('online-room-running', room({ phase: 'running', can_start: false })),
    N('online-layout', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-small', { mm_state: mm(empty) }, { init: PICK('ranked'), viewport: { width: 800, height: 560 } }),
    // (redesign states are added above this line)
  ];
```

In `Tools/check-online-page.mjs`, replace `  // (states are added above this line)` with:

```js
  'online-layout': {
    text: ['Online', 'Ranked, casual, or a private game with a friend.'],
    check: async (page) => {
      const p = [];
      const lb = await page.locator('#leaderboard-slot').boundingBox();
      if (!lb || Math.round(lb.width) !== 316) p.push(`the leaderboard column is ${lb?.width}px wide, not 316`);
      const mode = await page.locator('#mode-slot').boundingBox(), live = await page.locator('#live-slot').boundingBox();
      if (!mode || !live || mode.y + mode.height > live.y) p.push('the mode area does not sit above Live now');
      if (await page.locator('#direct-dialog').evaluate((d) => d.open)) p.push('the direct connection dialog is open on load');
      // From Task 4 on, the links line shows under Private room only (ruling 6): pick it when the link is hidden.
      if (!(await page.locator('#direct-open').isVisible())) await page.locator('[data-key="mode-private"]').click();
      await page.locator('#direct-open').click();
      if (!(await page.locator('#direct-dialog').evaluate((d) => d.open))) p.push('Direct connection did not open its dialog');
      if (!(await page.getByRole('button', { name: 'Join game', exact: true }).isVisible())) p.push("the dialog does not hold online.js's form");
      await page.locator('#direct-close').click();
      if (await page.locator('#direct-dialog').evaluate((d) => d.open)) p.push('Close did not close the dialog');
      return p;
    },
  },
  'online-small': {
    check: async (page) => {
      const p = [];
      const mode = await page.locator('#mode-slot').boundingBox(), lb = await page.locator('#leaderboard-slot').boundingBox();
      if (!mode || mode.width < 300) p.push(`the mode area is ${mode?.width}px wide at 800x560`);
      if (!lb || !mode || lb.y < mode.y + mode.height) p.push('the leaderboard does not drop below the mode area at 800x560');
      return p;
    },
  },
  // (states are added above this line)
```

- [ ] **Step 2: Run it to see it fail**

Run: `node Tools/check-online-page.mjs`
Expected: `FAIL online-layout: … the leaderboard column is undefinedpx wide …` (no such element yet) and `FAIL online-small: …`.

- [ ] **Step 3: Write the new `online.html` body**

Replace everything from `  <main id="main" tabindex="-1">` through the closing `</html>` with:

```html
  <main id="main" tabindex="-1">
    <div class="page-heading on-head">
      <div><span class="eyebrow">NFL STREET REBORN</span><h1>Online</h1><p class="purpose">Ranked, casual, or a private game with a friend.</p></div>
      <div class="on-meta">
        <button type="button" id="on-players" class="on-pop" data-key="players" aria-haspopup="dialog" aria-expanded="false" hidden><i class="on-dot" aria-hidden="true"></i><span>Players online</span></button>
        <span id="on-live-count" class="on-live-count" hidden><i class="live-dot" aria-hidden="true"></i><span></span></span>
      </div>
    </div>
    <div class="on-grid">
      <div class="on-left">
        <div id="mode-slot"></div>
        <p id="on-links" class="on-links"><span>Not on our servers?</span> <button type="button" id="direct-open" class="sec-link" data-key="direct-open">Direct connection ›</button> <button type="button" id="lobbies-open" class="sec-link" data-key="lobbies-open" hidden>Open lobbies ›</button></p>
        <div id="live-slot"></div>
      </div>
      <aside id="leaderboard-slot" aria-label="Leaderboard"></aside>
    </div>
    <details class="core"><summary>Diagnostic details</summary><div class="out-head"><button id="clear" type="button">Clear log</button><a href="harness.html">Developer tools</a></div><pre id="out"></pre></details>
  </main>
  <dialog id="direct-dialog" class="on-direct" aria-labelledby="direct-title">
    <div class="on-direct-head"><h2 id="direct-title">Direct connection</h2><button type="button" id="direct-close" class="btn quiet sm">Close</button></div>
    <section id="screen"><p class="loading">Loading…</p></section>
  </dialog>
  <footer class="input-footer"><span id="input-hints"></span><span>NFL STREET REBORN</span></footer>
  <script type="module" src="app-shell.js"></script><script type="module" src="online.js"></script><script type="module" src="online-direct.js"></script><script type="module" src="online-mm.js"></script><script type="module" src="live-now.js"></script>
</body>
</html>
```

The `<head>`, skip link, backdrop, header and `<nav>` stay exactly as they are. The dialog is outside `#main` on purpose (DESIGN.md §4: fixed/top-layer UI never inside `#main`).

- [ ] **Step 4: Replace `pages/online.css`**

Delete the file's whole content and write:

```css
/* Online (spec docs/superpowers/specs/2026-10-02-online-page-redesign-design.md): the mode area and Live now on the
   left, the Leaderboard card on the right, one screen at 1280x800. Hand-written (the generated legacy layout is gone).
   Layout only: every colour is a theme.css token (Tools/check-theme-tokens.mjs); no weight above 700, no glow. */
body.page-online #main{display:flex;flex-direction:column;padding:32px 36px 24px}
.page-online .on-head{display:flex;align-items:flex-end;justify-content:space-between;gap:24px;margin-bottom:24px}
.on-meta{display:flex;align-items:center;gap:16px;font-size:13px;color:var(--fg-2)}
.on-pop{display:inline-flex;align-items:center;gap:8px;height:32px;padding:0 12px;border:1px solid var(--stroke);border-radius:999px;background:var(--glass-1);color:var(--fg-2);font:500 13px Inter,system-ui,sans-serif;cursor:pointer}
.on-pop:hover{background:var(--glass-2);color:var(--fg)}
.on-dot{display:inline-block;flex:none;width:7px;height:7px;border-radius:50%;background:var(--ok)}
.on-live-count{display:inline-flex;align-items:center;gap:8px}
.on-live-count .live-dot{width:7px;height:7px}
.on-grid{flex:1;min-height:0;display:grid;grid-template-columns:minmax(0,1fr) 316px;gap:24px 28px}
.on-left{display:flex;flex-direction:column;gap:16px;min-width:0;min-height:0}
#mode-slot{flex:1;min-height:340px;display:flex;flex-direction:column}
.page-online section.mm{flex:1;display:flex;flex-direction:column;gap:16px;min-height:0}
.on-links{display:flex;flex-wrap:wrap;align-items:center;gap:8px 16px;margin:0;font-size:12.5px;color:var(--fg-2)}
body[data-mm-busy="true"] .on-links{display:none}
#leaderboard-slot{display:flex;flex-direction:column;min-width:0;min-height:0}
@media (max-width:1000px){
  .on-grid{grid-template-columns:minmax(0,1fr)}
  .page-online .on-head{flex-wrap:wrap;align-items:flex-start}
}
/* the direct connection dialog: online.js's Netplay form and room, logic unchanged (spec §3.1) */
.on-direct{width:min(560px,calc(100vw - 48px));max-height:calc(100vh - 48px);overflow:auto}
.on-direct-head{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:16px}
.on-direct-head h2{margin:0;font:600 16px/1.2 Inter,system-ui,sans-serif}
.on-direct #screen{display:block}
.netplay-form,.netplay-lobby{display:grid;gap:16px}
.netplay-intro h2{margin:0;font:600 20px/1.2 Inter,system-ui,sans-serif}
.netplay-intro p{margin:4px 0 0;color:var(--fg-2)}
.section-tabs{justify-self:start;display:inline-flex;gap:4px;padding:4px;border-radius:var(--r-lg);border:1px solid var(--stroke);background:var(--field-bg)}
.section-tabs button{height:32px;padding:0 14px;border:0;border-radius:var(--r-sm);background:transparent;color:var(--fg-2);font:500 13px Inter,system-ui,sans-serif;cursor:pointer}
.section-tabs button[aria-pressed="true"]{background:var(--glass-3);color:var(--fg)}
.netplay-field{display:grid;gap:8px}
.netplay-field label{font:600 11px/1.3 Inter,system-ui,sans-serif;letter-spacing:.07em;text-transform:uppercase;color:var(--fg-3)}
.netplay-field input,.netplay-field select{width:100%}
.netplay-field input[type=number]{height:38px;padding:0 12px;border-radius:var(--r-md);border:1px solid var(--stroke);background:var(--field-bg);color:var(--fg);font:500 13px Inter,system-ui,sans-serif}
.netplay-form>button[type=submit]{justify-self:start}
.netplay-room-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:16px}
.netplay-room-heading h2{margin:4px 0 0;font:600 20px/1.2 Inter,system-ui,sans-serif}
.netplay-kicker{font:600 11px/1.3 Inter,system-ui,sans-serif;letter-spacing:.07em;text-transform:uppercase;color:var(--fg-3)}
.netplay-safe{font-size:12px;color:var(--ok)}
.netplay-code{display:flex;flex-wrap:wrap;align-items:center;gap:8px}
.netplay-code strong{font:700 22px Inter,system-ui,sans-serif;letter-spacing:.08em}
.netplay-players{display:grid;gap:8px}
.netplay-player{display:flex;align-items:center;gap:12px}
.netplay-player>div{flex:1;min-width:0}
.netplay-player small{display:block;color:var(--fg-3)}
.netplay-avatar{display:inline-grid;place-items:center;flex:none;width:32px;height:32px;border-radius:50%;background:var(--avatar-bg);font:700 13px Inter,system-ui,sans-serif}
.netplay-slots{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px 16px}
.netplay-slots label{display:grid;gap:4px;font-size:12px;color:var(--fg-3)}
.netplay-actions{display:flex;flex-wrap:wrap;justify-content:flex-end;gap:8px}
```

- [ ] **Step 5: Create `online-direct.js`**

```js
// The direct (IP / host code) Netplay form, in a dialog (spec 2026-10-02 §3.1). online.js owns the form and the room
// and is unchanged; this file only opens and closes the dialog around #screen:
//   - "Direct connection ›" (#direct-open) opens it; Close, Esc and B close it;
//   - it opens by itself while a direct room is open (online.js shows .netplay-lobby; the room was always on screen
//     before the redesign, so it stays on screen now) and when an invite filled the form (online.js's
//     `netplay-invite` event);
//   - it closes when matchmaking takes the player (body[data-mm-busy]) and no direct room is open.
const dialog = document.getElementById('direct-dialog');
const opener = document.getElementById('direct-open');
const closer = document.getElementById('direct-close');

function open() {
  if (dialog && !dialog.open) dialog.showModal();
}
const lobbyShown = () => {
  const lobby = dialog?.querySelector('.netplay-lobby');
  return !!lobby && !lobby.hidden;
};

opener?.addEventListener('click', open);
closer?.addEventListener('click', () => dialog?.close());
document.addEventListener('netplay-invite', open);
dialog?.addEventListener('close', () => { if (opener && !opener.closest('[hidden]')) opener.focus({ preventScroll: true }); });
if (dialog && typeof MutationObserver === 'function') {
  new MutationObserver(() => { if (lobbyShown()) open(); }).observe(dialog, { subtree: true, attributes: true, attributeFilter: ['hidden'] });
  new MutationObserver(() => {
    if (document.body.dataset.mmBusy === 'true' && dialog.open && !lobbyShown()) dialog.close();
  }).observe(document.body, { attributes: true, attributeFilter: ['data-mm-busy'] });
}
```

- [ ] **Step 6: One line in `online.js`, and the two mounts**

In `crates/reborn-ui/ui/online.js`, inside `takePendingInvite`, change

```js
  status.textContent=`Invite received: room ${pending.code}. Enter your player name and press Join game.`;
```

to

```js
  status.textContent=`Invite received: room ${pending.code}. Enter your player name and press Join game.`;
  document.dispatchEvent(new CustomEvent('netplay-invite'));
```

(The form now lives in a closed dialog; this opens it before online.js focuses the name field. No other change to online.js.)

In `crates/reborn-ui/ui/online-mm.js`, change

```js
const screen = document.getElementById('screen');
const section = el('section', { class: 'mm', 'aria-label': 'Matchmaking' });
screen?.parentNode?.insertBefore(section, screen);
```

to

```js
const screen = document.getElementById('screen');
const section = el('section', { class: 'mm', 'aria-label': 'Matchmaking' });
// The mode area's slot (online.html, spec 2026-10-02 §2); a page without one still gets the panel above #screen.
const slot = document.getElementById('mode-slot');
if (slot) slot.append(section); else screen?.parentNode?.insertBefore(section, screen);
```

In `crates/reborn-ui/ui/live-now.js`, change

```js
const screen = document.getElementById('screen');
const section = el('section', { class: 'live-now', 'aria-label': 'Live now' });
screen?.parentNode?.insertBefore(section, screen);
```

to

```js
const screen = document.getElementById('screen');
const section = el('section', { class: 'live-now', 'aria-label': 'Live now' });
// Live now's slot under the mode area (online.html, spec 2026-10-02 §2); without one (Tools/check-live-now.mjs's
// stand-in DOM) it sits right before #screen as it always did.
const liveSlot = document.getElementById('live-slot');
if (liveSlot) liveSlot.append(section); else screen?.parentNode?.insertBefore(section, screen);
```

- [ ] **Step 7: Update the checks that drive the old inline form (layout only)**

`Tools/check-online-restored.mjs`: the form is now one click away. Change

```js
 await page.goto(url+'/online.html');
 await page.getByLabel('Player name',{exact:true}).fill('Player One');await page.getByLabel('Host code',{exact:true}).fill('ABC12345');
 await page.getByRole('button',{name:'Join game',exact:true}).click();await page.getByRole('heading',{name:'You’re in.',exact:true}).waitFor();
```

to

```js
 // The form lives in the Direct connection dialog (spec 2026-10-02 §3.1): open it first.
 const openDirect=()=>page.getByRole('button',{name:/^Direct connection/}).click();
 await page.goto(url+'/online.html');await openDirect();
 await page.getByLabel('Player name',{exact:true}).fill('Player One');await page.getByLabel('Host code',{exact:true}).fill('ABC12345');
 await page.getByRole('button',{name:'Join game',exact:true}).click();await page.getByRole('heading',{name:'You’re in.',exact:true}).waitFor();
```

and inside `openFresh`, change

```js
  await page.goto(url+'/online.html');
  await page.evaluate(p=>{window.__calls=[];
```

to

```js
  await page.goto(url+'/online.html');await openDirect();
  await page.evaluate(p=>{window.__calls=[];
```

`Tools/check-session-state.mjs`: change

```js
  await page.goto(base+'/online.html');await page.getByRole('button',{name:'Join game',exact:true}).waitFor();
```

to

```js
  await page.goto(base+'/online.html');await page.getByRole('button',{name:/^Direct connection/}).click();await page.getByRole('button',{name:'Join game',exact:true}).waitFor();
```

`Tools/review-finish.mjs` line 170: change

```js
 await page.goto(url+'/online.html');await page.getByLabel('Player name',{exact:true}).fill('Player One');
```

to

```js
 await page.goto(url+'/online.html');await page.getByRole('button',{name:/^Direct connection/}).click();await page.getByLabel('Player name',{exact:true}).fill('Player One');
```

`Tools/check-themed-shell.mjs`: the content under the heading is now `.on-grid`, not `#screen` (which is inside a closed dialog). Change

```js
  const nav=await box(pg,'.shell-nav'),m=await box(pg,'#main'),h=await box(pg,'.page-heading h1'),sc=await box(pg,'#screen');
```

to

```js
  const nav=await box(pg,'.shell-nav'),m=await box(pg,'#main'),h=await box(pg,'.page-heading h1'),sc=await box(pg,'.on-grid');
```

and change the comment line `// The page's own padding places the heading (Online keeps its legacy 36px/32px); …` to `// The page's own padding places the heading (Online: 36px/32px, spec 2026-10-02); …` (rest of the line unchanged).

- [ ] **Step 8: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-restored.mjs; node Tools/check-session-state.mjs; node Tools/check-themed-shell.mjs; node Tools/check-online-mm.mjs; node Tools/check-live-now.mjs; node Tools/check-theme.mjs; node Tools/check-theme-tokens.mjs`
Expected: `check-online-page: PASS (2 state(s))`; `check-online-restored` PASS; `check-session-state` PASS; `check-themed-shell: PASS`; `check-online-mm` at its baseline (only `held` fails); `check-live-now` PASS; `check-theme` and `check-theme-tokens` PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/reborn-ui/ui/online.html crates/reborn-ui/ui/pages/online.css crates/reborn-ui/ui/online-direct.js crates/reborn-ui/ui/online.js crates/reborn-ui/ui/online-mm.js crates/reborn-ui/ui/live-now.js Tools/lib/page-states.mjs Tools/check-online-page.mjs Tools/check-online-restored.mjs Tools/check-session-state.mjs Tools/check-themed-shell.mjs Tools/review-finish.mjs
git commit -m "online: page skeleton (mode area, Live now, leaderboard slot), new stylesheet, direct connection in a dialog"
```

---

### Task 3: `online-model.js`, the pure display helpers

**Files:**
- Create: `crates/reborn-ui/ui/online-model.js`
- Create: `crates/reborn-ui/ui/online-model.test.mjs`
- Modify: `crates/reborn-ui/ui/DESIGN.md` (§12 node test table: one row)

**Interfaces:**
- Consumes: nothing.
- Produces (ES module exports, all pure):
  - `int(n) -> number|null` (a finite number, else null)
  - `fmtInt(n: number) -> string` (`1184 → "1,184"`)
  - `clock(ms: number) -> "m:ss"`
  - `modeLine(mode: string) -> "Ranked · first to two wins" | "Casual · one game"`
  - `seriesLine(m) -> "Game {n} · Series {a}–{b}" | null` (from `m.best_of`, `m.series`, `m.seat`)
  - `acceptWithin(deadline: string|null, nowMs: number) -> "Accept within m:ss" | null`
  - `eloBits(elo) -> { rank: "#7"|null, elo: "1,184 Elo"|null, week: {dir:"up"|"down", text:"▲ 16 this week"}|null }`
  - `standing(rank, elo) -> "#9 · 1,162 Elo" | "#9" | "1,162 Elo" | ""`
  - `seekLine(mode, elo) -> string`
  - `heldNotice(m) -> { title, text }`
  - `everyoneReady(room) -> boolean`
  - `inviteLink(code) -> "https://nflstreetreborn.com/join.php?r=<code>"`
  - `inviteLine(host: string|null, code) -> { who: string|null, rest: string }`
  - `leaderboardView(answer) -> { soon: true } | { soon: false, total: number|null, rows: Row[], you: Row|null }` with `Row = { rank: number|null, name, avatar: string|null, team: object|null, record: "W–L"|null, elo: "1,320"|null, change: {dir, text:"▲ 16"}|null, you: boolean }`

- [ ] **Step 1: Write the failing tests**

Create `crates/reborn-ui/ui/online-model.test.mjs`:

```js
// node --test crates/reborn-ui/ui/online-model.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { int, fmtInt, clock, modeLine, seriesLine, acceptWithin, eloBits, standing, seekLine, heldNotice, everyoneReady, inviteLink, inviteLine, leaderboardView } from './online-model.js';

test('numbers and the clock', () => {
  assert.equal(int(3), 3); assert.equal(int('3'), null); assert.equal(int(NaN), null); assert.equal(int(null), null);
  assert.equal(fmtInt(1184), '1,184');
  assert.equal(clock(42000), '0:42'); assert.equal(clock(61000), '1:01'); assert.equal(clock(-5), '0:00');
});

test('the mode line never says best of', () => {
  assert.equal(modeLine('ranked'), 'Ranked · first to two wins');
  assert.equal(modeLine('casual'), 'Casual · one game');
  assert.equal(modeLine(undefined), 'Casual · one game');
});

test('the series line is from this seat, only in a series', () => {
  assert.equal(seriesLine({ best_of: 3, series: [1, 0], seat: 0 }), 'Game 2 · Series 1–0');
  assert.equal(seriesLine({ best_of: 3, series: [0, 1], seat: 1 }), 'Game 2 · Series 1–0');
  assert.equal(seriesLine({ best_of: 3, series: [1, 1], seat: 0 }), 'Game 3 · Series 1–1');
  assert.equal(seriesLine({ best_of: 1, series: [0, 0], seat: 0 }), null);
  assert.equal(seriesLine({ best_of: null, series: null }), null);
  assert.equal(seriesLine(null), null);
});

test('accept within counts down to the deadline and hides when there is none', () => {
  const now = Date.UTC(2026, 8, 30, 20, 0, 0);
  assert.equal(acceptWithin('2026-09-30T20:00:14.000Z', now), 'Accept within 0:14');
  assert.equal(acceptWithin('2026-09-30T19:59:00.000Z', now), 'Accept within 0:00');
  assert.equal(acceptWithin(null, now), null);
  assert.equal(acceptWithin('not a date', now), null);
});

test('Elo bits hide whatever the server did not send', () => {
  assert.deepEqual(eloBits({ rating: 1184, rank: 7, week_delta: 16 }), { rank: '#7', elo: '1,184 Elo', week: { dir: 'up', text: '▲ 16 this week' } });
  assert.deepEqual(eloBits({ rating: 1184, rank: 7, week_delta: -9 }).week, { dir: 'down', text: '▼ 9 this week' });
  assert.equal(eloBits({ rating: 1184, rank: 7, week_delta: 0 }).week, null);
  assert.equal(eloBits({ rating: 1184, rank: 7 }).week, null);
  assert.deepEqual(eloBits({ rating: null, rank: null, week_delta: null }), { rank: null, elo: null, week: null });
  assert.deepEqual(eloBits({ rating: null, rank: 3, week_delta: 5 }), { rank: null, elo: null, week: null }, 'Elo off: nothing, even if a rank leaked');
  assert.deepEqual(eloBits(undefined), { rank: null, elo: null, week: null });
  assert.equal(eloBits({ rating: 1000, rank: 0 }).rank, null);
});

test('standing joins only what exists', () => {
  assert.equal(standing(9, 1162), '#9 · 1,162 Elo');
  assert.equal(standing(null, 1162), '1,162 Elo');
  assert.equal(standing(9, null), '#9');
  assert.equal(standing(0, null), '');
  assert.equal(standing(undefined, 'x'), '');
});

test('the searching pill', () => {
  assert.equal(seekLine('ranked', { rating: 1184 }), 'Looking for players near 1,184 Elo');
  assert.equal(seekLine('ranked', { rating: null }), 'Looking for a ranked game');
  assert.equal(seekLine('ranked', null), 'Looking for a ranked game');
  assert.equal(seekLine('casual', { rating: 1184 }), 'Looking for a casual game');
});

test('the held notice keeps each reason\'s words', () => {
  assert.equal(heldNotice({ hold_reason: 'x' }).title, 'Your last match is held for review');
  assert.equal(heldNotice({ hold_reason: 'series-too-fast' }).text, 'Nothing changed on the leaderboard. Someone will look at it. You can play again now.');
  assert.match(heldNotice({ hold_reason: 'content-stale' }).text, /^A game update never reached one of your game discs, so that game was not started\./);
  assert.equal(heldNotice({ hold_reason: 'content-mismatch', opponent: 'Jock' }).text,
    "Your game files differ from Jock's, so that game was not started. Both of you: Settings → Check game content, install what it offers, then find a game again.");
  assert.match(heldNotice({ hold_reason: 'content-mismatch', opponent: null }).text, /your opponent's/);
});

test('everyone ready needs two players, all ready', () => {
  assert.equal(everyoneReady({ members: [{ ready: true }, { ready: true }] }), true);
  assert.equal(everyoneReady({ members: [{ ready: true }, { ready: false }] }), false);
  assert.equal(everyoneReady({ members: [{ ready: true }] }), false);
  assert.equal(everyoneReady(null), false);
});

test('invite link and line', () => {
  assert.equal(inviteLink('K7QP2M'), 'https://nflstreetreborn.com/join.php?r=K7QP2M');
  assert.equal(inviteLink('a b&c'), 'https://nflstreetreborn.com/join.php?r=a%20b%26c');
  assert.deepEqual(inviteLine('Jock', 'K7QP2M'), { who: 'Jock', rest: ' invited you to a room · K7QP2M' });
  assert.deepEqual(inviteLine(null, 'K7QP2M'), { who: null, rest: 'You were invited to a room · K7QP2M' });
});

test('the leaderboard model', () => {
  assert.deepEqual(leaderboardView(null), { soon: true });
  assert.deepEqual(leaderboardView({ enabled: false, rows: [] }), { soon: true });
  assert.deepEqual(leaderboardView({ enabled: true, rows: [] }), { soon: true });
  const rows = Array.from({ length: 12 }, (_, i) => ({ rank: i + 1, display_name: `P${i}`, avatar_url: null, team: null, wins: 5, losses: 2, elo: 1300 - i, delta_week: i === 0 ? 16 : i === 1 ? -9 : 0, you: false }));
  const v = leaderboardView({ enabled: true, total: 38, rows, you: { rank: 14, display_name: 'Me', wins: 1, losses: 1, elo: 1001, delta_week: 0 } });
  assert.equal(v.soon, false); assert.equal(v.total, 38); assert.equal(v.rows.length, 10, 'top 10 only');
  assert.deepEqual(v.rows[0], { rank: 1, name: 'P0', avatar: null, team: null, record: '5–2', elo: '1,300', change: { dir: 'up', text: '▲ 16' }, you: false });
  assert.deepEqual(v.rows[1].change, { dir: 'down', text: '▼ 9' });
  assert.equal(v.rows[2].change, null);
  assert.equal(v.you.rank, 14); assert.equal(v.you.you, true);
  const inTop = leaderboardView({ enabled: true, total: 3, rows: [{ ...rows[0], you: true }], you: { rank: 1, display_name: 'P0' } });
  assert.equal(inTop.you, null, 'you are already on the card');
  const bare = leaderboardView({ enabled: true, rows: [{ rank: null, display_name: null, wins: null, losses: 2, elo: null, delta_week: null }] });
  assert.deepEqual(bare.rows[0], { rank: null, name: '', avatar: null, team: null, record: null, elo: null, change: null, you: false });
});
```

- [ ] **Step 2: Run them to see them fail**

Run: `node --test crates/reborn-ui/ui/online-model.test.mjs`
Expected: FAIL, `Cannot find module …/online-model.js`.

- [ ] **Step 3: Write `online-model.js`**

```js
// Pure helpers for the Online page (spec docs/superpowers/specs/2026-10-02-online-page-redesign-design.md §3–§4, §7):
// the words and numbers online-mm.js and leaderboard.js draw. No DOM, no commands. Every value comes from an answer;
// a missing one is null (or "") and the page hides its spot. Nothing here invents a number.

/** A finite number, or null. */
export const int = (n) => (typeof n === 'number' && Number.isFinite(n) ? n : null);
export const fmtInt = (n) => n.toLocaleString('en-US');

export function clock(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** Under the VS and in a room. Never "best of 3" (spec §8, DESIGN.md §9). */
export function modeLine(mode) {
  return mode === 'ranked' ? 'Ranked · first to two wins' : 'Casual · one game';
}

/** "Game 2 · Series 1–0" while a series runs (best_of > 1), counted from this seat; null otherwise. */
export function seriesLine(m) {
  if (!m || !(int(m.best_of) > 1) || !Array.isArray(m.series)) return null;
  const seat = m.seat === 1 ? 1 : 0;
  const mine = int(m.series[seat]) ?? 0, theirs = int(m.series[1 - seat]) ?? 0;
  return `Game ${mine + theirs + 1} · Series ${mine}–${theirs}`;
}

/** "Accept within 0:14" from the server's relay_deadline; null when there is none or it does not read as a time. */
export function acceptWithin(deadline, nowMs) {
  const t = typeof deadline === 'string' ? Date.parse(deadline) : NaN;
  return Number.isFinite(t) ? `Accept within ${clock(t - nowMs)}` : null;
}

/** The Ranked tile's numbers, from stats_dashboard's `you.elo`. With no rating (Elo off) nothing shows at all. */
export function eloBits(elo) {
  const rating = int(elo?.rating), rank = int(elo?.rank), week = int(elo?.week_delta);
  if (rating === null) return { rank: null, elo: null, week: null };
  return {
    rank: rank !== null && rank > 0 ? `#${rank}` : null,
    elo: `${fmtInt(rating)} Elo`,
    week: week ? { dir: week > 0 ? 'up' : 'down', text: `${week > 0 ? '▲' : '▼'} ${Math.abs(week)} this week` } : null,
  };
}

/** "#9 · 1,162 Elo" under a face on the matchup card, from whatever exists. */
export function standing(rank, elo) {
  const r = int(rank), e = int(elo);
  return [r !== null && r > 0 ? `#${r}` : null, e !== null ? `${fmtInt(e)} Elo` : null].filter(Boolean).join(' · ');
}

/** The pill on the picked tile while searching (spec §3.2). */
export function seekLine(mode, elo) {
  if (mode !== 'ranked') return 'Looking for a casual game';
  const r = int(elo?.rating);
  return r !== null ? `Looking for players near ${fmtInt(r)} Elo` : 'Looking for a ranked game';
}

/** The held notice (spec §3.8): today's text per hold_reason; the generic one is §8's new sentence. */
export function heldNotice(m) {
  const opp = m?.opponent ?? 'your opponent';
  const text = m?.hold_reason === 'content-stale'
    ? 'A game update never reached one of your game discs, so that game was not started. Both of you: Settings → Repair game files (the one that is current is told so), then find a game again. Finding a game now also repairs it first.'
    : m?.hold_reason === 'content-mismatch'
      ? `Your game files differ from ${opp}'s, so that game was not started. Both of you: Settings → Check game content, install what it offers, then find a game again.`
      : 'Nothing changed on the leaderboard. Someone will look at it. You can play again now.';
  return { title: 'Your last match is held for review', text };
}

/** Start's enabled state: two or more players in the room, every one ready (spec §3.7). */
export function everyoneReady(room) {
  const ms = Array.isArray(room?.members) ? room.members : [];
  return ms.length >= 2 && ms.every((x) => x && x.ready === true);
}

/** The link "Copy invite link" copies (spec §3.10): a web page, so it pastes as a clickable link. */
export function inviteLink(code) {
  return `https://nflstreetreborn.com/join.php?r=${encodeURIComponent(code)}`;
}

/** The invite line above the tiles; `who` is drawn bold. */
export function inviteLine(host, code) {
  return host ? { who: host, rest: ` invited you to a room · ${code}` } : { who: null, rest: `You were invited to a room · ${code}` };
}

function change(d) {
  const n = int(d);
  return n ? { dir: n > 0 ? 'up' : 'down', text: `${n > 0 ? '▲' : '▼'} ${Math.abs(n)}` } : null;
}
function lbRow(r) {
  const wins = int(r?.wins), losses = int(r?.losses), elo = int(r?.elo);
  return {
    rank: int(r?.rank), name: typeof r?.display_name === 'string' ? r.display_name : '', avatar: typeof r?.avatar_url === 'string' ? r.avatar_url : null,
    team: r?.team && typeof r.team === 'object' ? r.team : null,
    record: wins !== null && losses !== null ? `${wins}–${losses}` : null,
    elo: elo !== null ? fmtInt(elo) : null, change: change(r?.delta_week), you: r?.you === true,
  };
}

/** The Leaderboard card (spec §4, §7): "Rankings start soon." (soon) while there is no leaderboard, Elo is off or nobody
 *  is ranked; otherwise the top 10 and, when you are further down, your own row. */
export function leaderboardView(answer) {
  if (!answer || answer.enabled === false || !Array.isArray(answer.rows) || !answer.rows.length) return { soon: true };
  const rows = answer.rows.slice(0, 10).map(lbRow);
  const you = answer.you && !rows.some((r) => r.you) && int(answer.you.rank) !== null ? { ...lbRow(answer.you), you: true } : null;
  return { soon: false, total: int(answer.total), rows, you };
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `node --test crates/reborn-ui/ui/online-model.test.mjs`
Expected: `# pass 11`, `# fail 0`.

- [ ] **Step 5: Name the test in DESIGN.md**

In `DESIGN.md` §12's node test table, after the `stats-rows.test.mjs` row add:

```markdown
| `online-model.test.mjs` | Online's words and numbers: mode and series lines, accept countdown, Elo bits (hidden with Elo off), held notice, invite link, leaderboard rows |
```

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/ui/online-model.js crates/reborn-ui/ui/online-model.test.mjs crates/reborn-ui/ui/DESIGN.md
git commit -m "online: online-model.js, the page's pure words and numbers, with node tests"
```

---

### Task 4: The mode tiles, the pick, the idle action rows, and searching

**Files:**
- Modify: `crates/reborn-ui/ui/online-mm.js` (header comment, imports, `act`, new pick/tiles/rows, `drawIdle`, `drawQueued`, the queue timer text, `draw`)
- Modify: `crates/reborn-ui/ui/pages/online.css` (append the tile/row/search rules)
- Modify: `Tools/lib/page-states.mjs` (four states)
- Modify: `Tools/check-online-page.mjs` (four entries)
- Modify: `Tools/check-online-mm.mjs` (text source; `idle` and `queued` expectations)

**Interfaces:**
- Consumes: `online-model.js` (`clock`, `eloBits`, `seekLine`); `#mode-slot`, `#on-links` (Task 2).
- Produces (inside `online-mm.js`, used by Tasks 5–9, 12, 13): `act(cmd, args) -> Promise<boolean>` (true when the command answered); `picked() -> 'ranked'|'casual'|'private'`; `pick(value)`; `rankInfo` (`{ elo, playsAs } | null`, set by the `online-rank` event); `tiles(snap, { searching?: 'ranked'|'casual', locked?: boolean, only?: 'private' }) -> HTMLElement` (`div.on-modes[data-nav=column]` of `button.on-tile[data-key=mode-<key>][aria-pressed]`); `findRow(p)`; `privateRow()`; `setLinks(show)`; `let roomCode` (the code field's text). DOM `data-key`s: `mode-ranked|casual|private`, `find`, `host`, `room-code`, `join`, `cancel`. The primary of an action row carries `data-primary="true"`. Event consumed: `online-rank` (`detail: { elo, playsAs } | null`) — Task 11 dispatches it.

- [ ] **Step 1: Add the states and the failing expectations**

In `Tools/lib/page-states.mjs`, above `    // (redesign states are added above this line)` add:

```js
    N('online-ranked', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-casual', { mm_state: mm(empty), content_repair_image: { current: true } }, { init: PICK('casual') }),
    N('online-private', { mm_state: mm(empty) }, { click: '[data-key="mode-private"]' }),
    N('online-searching', { mm_state: mm({ ...empty, ticket: { mode: 'ranked', field: null, waited_ms: 42000, enqueued_at: '2026-09-30T19:59:18.000Z' } }) }, { init: PICK('ranked') }),
```

In `Tools/check-online-page.mjs`, above `  // (states are added above this line)` add:

```js
  'online-ranked': {
    text: ['Ranked', 'First to two wins. Moves your Elo.', 'Casual', 'One game. Elo stays put.', 'Private room', 'Host a room or join with a code.',
      'Find a ranked game', 'Matched by Elo', '0 searching', '1 searching'],
    absent: ['Quick Match', 'Direct connection', 'best of'],
    check: async (page) => {
      const p = [];
      const pressed = await page.locator('.on-tile[aria-pressed="true"]').evaluateAll((n) => n.map((x) => x.dataset.key));
      if (pressed.join() !== 'mode-ranked') p.push(`picked tiles: ${pressed}`);
      const art = await page.locator('[data-key="mode-casual"] .on-art').evaluate((n) => getComputedStyle(n).filter);
      if (!/saturate/.test(art)) p.push(`an unpicked tile's picture is in full colour (${art})`);
      const on = await page.locator('[data-key="mode-ranked"]').evaluate((n) => getComputedStyle(n, '::before').borderTopWidth);
      if (on !== '2px') p.push(`the picked tile's border is ${on}, not 2px`);
      const r = await page.locator('[data-key="mode-ranked"]').boundingBox(), c = await page.locator('[data-key="mode-casual"]').boundingBox();
      if (!r || !c || r.height < c.height * 1.8) p.push('Ranked does not span two rows');
      await page.locator('[data-key="mode-casual"]').click();
      if (await page.evaluate(() => localStorage.getItem('mm:mode')) !== 'casual') p.push('picking Casual did not store mm:mode=casual');
      if (!(await page.getByRole('button', { name: 'Find a casual game', exact: true }).isVisible())) p.push('the row did not follow the pick');
      return p;
    },
  },
  'online-casual': {
    text: ['Find a casual game', 'Matched with anyone'],
    check: async (page) => {
      const p = [];
      const pressed = await page.locator('.on-tile[aria-pressed="true"]').evaluateAll((n) => n.map((x) => x.dataset.key));
      if (pressed.join() !== 'mode-casual') p.push(`picked tiles: ${pressed}`);
      await page.getByRole('button', { name: 'Find a casual game', exact: true }).click();
      await page.waitForFunction(() => window.__calls.some((c) => c.cmd === 'mm_queue'), null, { timeout: 4000 }).catch(() => {});
      const order = await page.evaluate(() => window.__calls.map((c) => c.cmd).filter((c) => /^content_|^mm_queue$/.test(c)));
      if (order.join() !== 'content_check,content_repair_image,mm_queue') p.push(`Find ran ${order.join(' → ')}`);
      const q = await calls(page, 'mm_queue');
      if (JSON.stringify(q[0]) !== '{"mode":"casual"}') p.push(`mm_queue got ${JSON.stringify(q[0])}`);
      return p;
    },
  },
  'online-private': {
    text: ['Host a room', 'Send the code to a friend', 'Not on our servers?', 'Direct connection'],
    absent: ['Find a'],
    check: async (page) => {
      const p = [];
      if (await page.evaluate(() => localStorage.getItem('mm:mode')) === 'private') p.push('Private room was stored in mm:mode');
      const join = page.getByRole('button', { name: 'Join by code', exact: true });
      if (!(await join.evaluate((n) => n.classList.contains('quiet')))) p.push('Join is not quiet while the code field is empty');
      if ((await join.innerText()).trim() !== 'Join') p.push('the Join button does not read "Join"');
      await page.getByLabel('Room code', { exact: true }).fill('k7qp2m');
      if (await join.evaluate((n) => n.classList.contains('quiet'))) p.push('Join stays quiet with a code typed');
      await join.click();
      await page.waitForTimeout(300);
      const j = await calls(page, 'mm_room_join');
      if (JSON.stringify(j[0]) !== '{"code":"k7qp2m"}') p.push(`mm_room_join got ${JSON.stringify(j[0])}`);
      await page.getByRole('button', { name: 'Host a room', exact: true }).click();
      await page.waitForTimeout(300);
      const h = await calls(page, 'mm_room_create');
      if (JSON.stringify(h[0]) !== '{"mode":"casual","public":false}') p.push(`Host a room sent ${JSON.stringify(h[0])}`);
      return p;
    },
  },
  'online-searching': {
    text: ['Looking for a ranked game', 'Searching… 0:42', 'Cancel'],
    absent: ['Find a ranked game', 'Not on our servers?'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-tile.dim').count()) !== 2) p.push('the other two tiles are not dark');
      if ((await page.locator('.on-search .sweep').count()) !== 1) p.push('no sweeping line on the search bar');
      if ((await page.locator('.on-search').innerText()).toLowerCase().indexOf('ranked') < 0) p.push('the bar does not say Ranked');
      await page.getByRole('button', { name: 'Cancel', exact: true }).click();
      await page.waitForTimeout(300);
      if (!(await calls(page, 'mm_leave_queue')).length) p.push('Cancel did not send mm_leave_queue');
      return p;
    },
  },
```

In `Tools/check-online-mm.mjs`:
- change `const text = (await page.locator('section.mm').innerText()).toLowerCase();` to `const text = (await page.locator('#main').innerText()).toLowerCase();` and the comment above it from `…innerText is the RENDERED text…` unchanged, adding one line `    // The whole page body (#main): the online count sits in the heading since the redesign (spec 2026-10-02 §2).`
- change `if (!(await page.locator('section.mm').getByRole('button', { name: b, exact: true }).count()))` to `if (!(await page.locator('#main').getByRole('button', { name: b, exact: true }).count()))`
- change the `idle` and `queued` lines of `EXPECT` to:

```js
  // The redesign (spec 2026-10-02 §3.1): tiles and an action row. Casual is the stored default; the lobby list and
  // the code field are under Private room (their assertions live in check-online-page's online-private/online-lobbies).
  idle: { text: ['Ranked', 'Casual', 'Private room', 'Find a casual game', 'Matched with anyone', '1 searching', '3 online'], buttons: ['Find a casual game'], absent: ['field pick', 'Quick Match'] },
  queued: { text: ['Looking for a casual game', 'Searching… 0:4'], buttons: ['Cancel'], absent: ['Find a casual game', 'field'] },
```

- [ ] **Step 2: Run them to see them fail**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs`
Expected: `FAIL online-ranked: missing text "First to two wins. Moves your Elo."…` (and the other three); `check-online-mm`: `FAIL idle … missing text "Ranked"`, `FAIL queued … missing text "Searching… 0:4"`, `FAIL held` (baseline).

- [ ] **Step 3: Rewrite the top of `online-mm.js`**

Replace the file's header comment and imports (everything from line 1 down to and including `import { bridge, call, el, get, reason } from './shell.js';`) with:

```js
// Matchmaking on the Online page: the mode tiles, the action row, the match in progress and your room
// (spec docs/superpowers/specs/2026-10-02-online-page-redesign-design.md).
//
// Only the look changed in the redesign. Every command, argument, order and poll below is the one this page always
// sent: `mm_state` every second (a read of what the launcher's long-poll already holds in memory, so it never touches
// the network) and one request per action. Which state the mode area shows is decided exactly as before: a live match,
// then a room, then a ticket, then idle (a held match draws idle plus a notice).
import { bridge, call, el, get, reason } from './shell.js';
import { clock, eloBits, seekLine } from './online-model.js';
```

Change `act` to report whether the command answered (callers that ignore the result behave as before):

```js
async function act(cmd, args) {
  if (busy) return false;
  busy = true;
  status.textContent = '';
  section.setAttribute('aria-busy', 'true');
  let ok = false;
  try {
    await call(cmd, args);
    ok = true;
  } catch (e) {
    // The server's own sentence, verbatim, is always the most useful thing to show.
    status.textContent = reason(e);
  } finally {
    busy = false;
    section.removeAttribute('aria-busy');
    lastDrawn = '';
    await draw();
  }
  return ok;
}
```

Change the line `const status = el('p', { class: 'settings-status', role: 'status' });` to `const status = el('p', { class: 'settings-status on-status', role: 'status' });`, and add right after `function mode() { return store.get('mm:mode', 'casual'); }`:

```js
const links = document.getElementById('on-links');
function setLinks(show) { if (links) links.hidden = !show; }

// Private room is the third pick, held in page state only: `mm:mode` keeps ranked|casual, so the queue and the room
// commands get exactly the mode they always got (spec §3.1).
let privatePicked = false;
const picked = () => (privatePicked ? 'private' : mode());
function pick(value) {
  if (value === 'private') privatePicked = true;
  else { privatePicked = false; store.set('mm:mode', value); }
  lastDrawn = '';
  draw();
}

// Your rank, Elo and plays-as team (stats_dashboard's `you`), from leaderboard.js. Null until it answers and whenever
// the server sends none (Elo off): every spot that needs it is then hidden (spec §7).
let rankInfo = null;
document.addEventListener('online-rank', (e) => { rankInfo = e.detail ?? null; lastDrawn = ''; draw(); });
```

Delete the old `modeTabs()` function entirely.

After `queueWhenCurrent` add:

```js
// While queueWhenCurrent works, the Find button says what it is doing: the status lines it already writes (spec §3.1).
new MutationObserver(() => {
  const find = section.querySelector('[data-key="find"]');
  if (find && busy && status.textContent) find.textContent = status.textContent;
}).observe(status, { childList: true, characterData: true, subtree: true });
```

Delete the local `function clock(ms) {…}` (it is imported now).

- [ ] **Step 4: Replace `drawIdle` and `drawQueued`; add the tiles and rows**

Replace the old `function drawIdle(snap) {…}` (the whole function) with:

```js
// ---- the mode area (spec §3.1) ----
const MODES = [
  { key: 'ranked', name: 'Ranked', line: 'First to two wins. Moves your Elo.', art: 'art/online/ranked.jpg' },
  { key: 'casual', name: 'Casual', line: 'One game. Elo stays put.', art: 'art/online/casual.jpg' },
  { key: 'private', name: 'Private room', line: 'Host a room or join with a code.', art: 'art/online/private.jpg' },
];

function tileClick(key) {
  if (picked() !== key) { pick(key); return; }
  // A on the picked tile (a controller drives): the action row's primary (spec §6). A mouse click on it does nothing.
  if (document.body.classList.contains('gamepad-input')) section.querySelector('.on-row [data-primary]')?.click();
}

/** The tiles. `searching`: the ticket's mode keeps colour and its pill, the others go dark and cannot be picked.
 *  `locked`: behind the sign-in panel (no counts, no rank, not reachable). `only`: just that tile (matchmaking off). */
function tiles(snap, { searching = null, locked = false, only = null } = {}) {
  const depth = snap?.queue_depth ?? null;
  const on = only ?? searching ?? picked();
  const bits = eloBits(rankInfo?.elo);
  const box = el('div', { class: `on-modes${only ? ' solo' : ''}`, 'data-nav': 'column', role: 'group', 'aria-label': 'Game modes' });
  if (locked) box.setAttribute('inert', '');
  for (const m of only ? MODES.filter((x) => x.key === only) : MODES) {
    const isOn = m.key === on;
    let pill = null;
    if (searching !== null && isOn) pill = el('span', { class: 'on-pill seek' }, el('i', { 'aria-hidden': 'true' }), seekLine(m.key, rankInfo?.elo));
    else if (searching === null && !locked && m.key !== 'private' && typeof depth?.[m.key] === 'number') {
      pill = el('span', { class: 'on-pill' }, el('i', { 'aria-hidden': 'true' }), `${depth[m.key]} searching`);
    }
    const rank = m.key === 'ranked' && !locked && (bits.rank || bits.elo)
      ? el('span', { class: 'on-rank' },
        bits.rank ? el('b', { class: 'num', text: bits.rank }) : null,
        bits.elo ? el('span', { class: 'num', text: bits.elo }) : null,
        bits.week ? el('span', { class: `on-week ${bits.week.dir}`, text: bits.week.text }) : null)
      : null;
    box.append(el('button', {
      type: 'button', class: `on-tile on-tile-${m.key}${isOn ? ' on' : ''}${searching !== null && !isOn ? ' dim' : ''}`,
      'data-key': `mode-${m.key}`, 'aria-pressed': String(isOn), disabled: searching !== null ? true : null,
      onclick: () => tileClick(m.key),
    },
    el('img', { class: 'on-art', src: m.art, alt: '' }),
    pill,
    el('span', { class: 'on-lab' },
      el('span', { class: 'on-tx' }, el('span', { class: 'on-name', text: m.name }), el('span', { class: 'on-line', text: m.line })),
      rank)));
  }
  return box;
}

function findRow(p) {
  return el('div', { class: 'on-row' },
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'find', 'data-primary': 'true', text: `Find a ${p} game`, onclick: () => queueWhenCurrent() }),
    el('p', { class: 'on-note', text: p === 'ranked' ? 'Matched by Elo' : 'Matched with anyone' }));
}

// The code field's text survives the redraws (the field itself is rebuilt).
let roomCode = '';
function privateRow() {
  const code = el('input', { type: 'text', class: 'on-code', maxlength: 12, placeholder: 'Room code', 'aria-label': 'Room code', spellcheck: 'false', autocomplete: 'off', 'data-key': 'room-code', value: roomCode });
  const join = el('button', { type: 'submit', class: `btn on-join${roomCode.trim() ? '' : ' quiet'}`, 'data-key': 'join', 'aria-label': 'Join by code', text: 'Join' });
  code.addEventListener('input', () => { roomCode = code.value; join.classList.toggle('quiet', !code.value.trim()); });
  const form = el('form', { class: 'on-codeform' }, code, join);
  form.addEventListener('submit', (e) => { e.preventDefault(); const c = code.value.trim(); if (c) act('mm_room_join', { code: c }); });
  return el('div', { class: 'on-row' },
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'host', 'data-primary': 'true', text: 'Host a room', onclick: () => act('mm_room_create', { mode: mode(), public: false }) }),
    form,
    el('p', { class: 'on-note', text: 'Send the code to a friend' }));
}

function drawIdle(snap) {
  const p = picked();
  // The same `mm_rooms` read on every idle draw as before the redesign; the list shows under Private room.
  const lobbies = el('div', { class: 'mm-lobbies' }, el('h3', { text: 'Lobbies' }), el('p', { class: 'loading', text: 'Loading lobbies…' }));
  loadLobbies(lobbies);
  if (p !== 'private') return [tiles(snap), findRow(p)];
  const open = el('button', { type: 'button', class: 'btn sm', text: 'Open a public lobby', onclick: () => act('mm_room_create', { mode: mode(), public: true }) });
  return [tiles(snap), privateRow(), lobbies, el('div', { class: 'mm-actions' }, open)];
}
```

Change the queue timer's text line `if (timer && queueClock.key) timer.textContent = \`Waiting ${clock(queuedMs())}\`;` to `if (timer && queueClock.key) timer.textContent = \`Searching… ${clock(queuedMs())}\`;`

Replace the old `function drawQueued(t) {…}` with:

```js
function drawQueued(t) {
  if (queueClock.key !== t.enqueued_at) queueClock = { key: t.enqueued_at, base: t.waited_ms || 0, at: Date.now() };
  const ranked = t.mode === 'ranked';
  return [
    tiles(null, { searching: ranked ? 'ranked' : 'casual' }),
    el('div', { class: 'on-row' },
      el('div', { class: 'on-search' },
        el('span', { class: 'mm-timer on-timer num', text: `Searching… ${clock(queuedMs())}` }),
        el('span', { class: 'on-search-mode', text: ranked ? 'Ranked' : 'Casual' }),
        el('span', { class: 'sweep', 'aria-hidden': 'true' })),
      el('button', { type: 'button', class: 'btn on-cancel', 'data-key': 'cancel', text: 'Cancel', onclick: () => act('mm_leave_queue') })),
  ];
}
```

- [ ] **Step 5: Show the links line in `draw()`**

In `draw()`:
- in the `if (!bridge) {` branch, add `setLinks(true);` as its first statement;
- change `try { state = await call('mm_state'); } catch (e) { section.replaceChildren(…); return; }` to `try { state = await call('mm_state'); } catch (e) { setLinks(true); section.replaceChildren(el('p', { class: 'note', text: reason(e) })); return; }`;
- change `if (!state || typeof state !== 'object') { section.replaceChildren(); return; }` to `if (!state || typeof state !== 'object') { setLinks(true); section.replaceChildren(); return; }`;
- change `if (state.link === 'disabled') { lastDrawn = ''; markBusy(false); section.replaceChildren(); return; }` to `if (state.link === 'disabled') { lastDrawn = ''; markBusy(false); setLinks(true); section.replaceChildren(); return; }`;
- change `let body;` to `let body; let showLinks = true;`;
- change `    body = drawIdle(snap);` (the `else {` branch) to `    body = drawIdle(snap); showLinks = picked() === 'private';`;
- change `section.replaceChildren(...body, status, drawOnline(snap), presenceToggle(state.presence_on));` to:

```js
  setLinks(showLinks);
  section.replaceChildren(...body, status, drawOnline(snap), presenceToggle(state.presence_on));
```

(The signed-out/offline/loading bodies, the held notices, `drawMatch`, `drawRoom`, `drawOnline` and `presenceToggle` are unchanged in this task.)

- [ ] **Step 6: Append the tile, row and search rules to `pages/online.css`**

```css
/* the three mode tiles (spec §3.1): Ranked spans two rows. Picked = a 2px red edge and the picture in colour; not
   picked = the 1px stroke and the picture desaturated; searching = the others dark (§3.2). */
.on-modes{flex:1;min-height:260px;display:grid;grid-template-columns:1.75fr 1fr;grid-template-rows:1fr 1fr;gap:10px}
.on-modes.solo{grid-template-columns:1fr;grid-template-rows:1fr}
.on-tile{all:unset;box-sizing:border-box;position:relative;overflow:hidden;display:block;min-height:0;border-radius:var(--r-lg);background:var(--team-fallback);box-shadow:var(--shadow);cursor:pointer;isolation:isolate}
.on-tile-ranked{grid-row:1/3}
.on-modes.solo .on-tile{grid-row:auto}
.on-tile .on-art{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;filter:saturate(.25) brightness(.62);z-index:-2}
.on-tile.on .on-art{filter:none}
.on-tile.dim .on-art{filter:saturate(0) brightness(.35)}
.on-tile::after{content:"";position:absolute;inset:0;background:var(--tile-scrim);z-index:-1}
.on-tile::before{content:"";position:absolute;inset:0;border-radius:inherit;border:1px solid var(--stroke);pointer-events:none;z-index:1}
.on-tile.on::before{border:2px solid var(--red)}
.on-tile:disabled{cursor:default}
/* the white focus ring sits outside the red pick edge (spec §6) */
body.shell .on-tile:focus-visible,body.gamepad-input .on-tile.pad-focus{outline-offset:4px!important}
.on-lab{position:absolute;left:22px;right:22px;bottom:20px;display:flex;align-items:flex-end;justify-content:space-between;gap:12px}
.on-tx{display:flex;flex-direction:column;min-width:0}
.on-name{font:700 26px/1 Inter,system-ui,sans-serif;letter-spacing:-.02em;color:var(--fg)}
.on-tile-ranked .on-name{font-size:56px;letter-spacing:-.035em}
.on-line{margin-top:8px;font-size:13px;color:var(--fg-2)}
.on-rank{display:flex;flex-direction:column;align-items:flex-end;gap:2px;font-size:12px;color:var(--fg-2);white-space:nowrap;text-align:right}
.on-rank b{margin-bottom:4px;font:700 34px/1 Inter,system-ui,sans-serif;letter-spacing:-.02em;color:var(--fg)}
.on-week{font-weight:600}
.on-week.up{color:var(--ok)}
.on-week.down{color:var(--red-ink)}
.on-pill{position:absolute;top:16px;right:16px;display:inline-flex;align-items:center;gap:6px;padding:5px 10px;border-radius:999px;background:var(--pill-bg);border:1px solid var(--stroke);font-size:11.5px;color:var(--fg)}
.on-pill i{width:6px;height:6px;border-radius:50%;background:var(--ok)}
.on-pill.seek{left:20px;right:auto;top:18px;padding:7px 14px 7px 12px;font-size:12.5px}
.on-pill.seek i{width:8px;height:8px;background:var(--red)}
/* the action row under the tiles */
.on-row{display:flex;flex-wrap:wrap;align-items:center;gap:16px}
.btn.on-go{height:52px;min-width:min(100%,280px);flex:0 1 calc(63.6% - 4px);font-size:16px}
.on-note{margin:0;font-size:12.5px;line-height:1.5;color:var(--fg-3)}
.on-codeform{display:flex;gap:8px}
.on-codeform .on-code{width:150px;height:52px;font-weight:600;letter-spacing:.3em;text-transform:uppercase}
.btn.on-join,.btn.on-cancel{height:52px}
.on-search{position:relative;overflow:hidden;flex:0 1 calc(63.6% - 4px);display:flex;align-items:center;justify-content:space-between;gap:16px;height:52px;padding:0 22px;border-radius:var(--r-md);background:var(--glass-1);border:1px solid var(--stroke);box-shadow:var(--sheen)}
.on-timer{font:700 18px Inter,system-ui,sans-serif;letter-spacing:-.01em}
.on-search-mode{font-size:13px;color:var(--fg-2)}
.on-status:empty{display:none}
.mm-lobbies h3{margin:0 0 8px;font:600 11px/1.3 Inter,system-ui,sans-serif;letter-spacing:.07em;text-transform:uppercase;color:var(--fg-3)}
.mm-room-list{list-style:none;margin:0;padding:0;display:grid;gap:8px}
.mm-actions{display:flex;flex-wrap:wrap;gap:8px}
```

- [ ] **Step 7: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs; node Tools/check-online-restored.mjs; node Tools/check-session-state.mjs; node Tools/check-theme-tokens.mjs`
Expected: `check-online-page: PASS (6 state(s))`; `check-online-mm`: `ok idle`, `ok queued` (timer moves), only `held` fails (baseline); the other three PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/reborn-ui/ui/online-mm.js crates/reborn-ui/ui/pages/online.css Tools/lib/page-states.mjs Tools/check-online-page.mjs Tools/check-online-mm.mjs
git commit -m "online: mode tiles with the pick, idle action rows (find, host, join by code), searching bar"
```

---

### Task 5: The matchup card (match found, accepted, starting, playing) and the opponent's standing in `mm_state`

**Files:**
- Modify: `crates/reborn-core/src/matchmaking.rs:128-133` (`Opponent`) and its tests module
- Modify: `crates/reborn-ui/src/online_mm.rs:571-578` (`snapshot_json` match) and its tests module
- Modify: `crates/reborn-ui/ui/online-mm.js` (imports, identity read, `drawMatch`, the `drawMatch` call in `draw`)
- Modify: `crates/reborn-ui/ui/pages/online.css` (append)
- Modify: `Tools/lib/page-states.mjs` (six states), `Tools/check-online-page.mjs` (six entries), `Tools/check-online-mm.mjs` (`pairing`, `running_bo3`, `relay_failed`)

**Interfaces:**
- Consumes: `act`, `rankInfo`, `RELAY` (online-mm.js); `modeLine`, `seriesLine`, `acceptWithin`, `standing`, `eloBits` (online-model.js); `avatar(url, name, cls)`, `teamLogo(team, cls)` (match-rows.js).
- Produces: `mm_state().snapshot.match` gains `opponent_elo: number|null`, `opponent_rank: number|null`, `opponent_team: object|null`, `opponent_avatar_url: string|null` (null until the backend sends `opponent.rating/rank/team/avatar_url`, Task 17). Core: `Opponent { id, display_name, rating: Option<i64>, rank: Option<u32>, team: Option<serde_json::Value>, avatar_url: Option<String> }`. DOM `data-key`s: `accept`, `decline`, `leave-match`, `chat`.

- [ ] **Step 1: Write the failing Rust tests**

In `crates/reborn-core/src/matchmaking.rs`'s `mod tests`, add:

```rust
    #[test]
    fn the_opponents_standing_is_read_when_sent_and_absent_when_not() {
        let mut v: serde_json::Value = serde_json::from_str(MATCHED).unwrap();
        v["match"]["opponent"]["rating"] = serde_json::json!(1162);
        v["match"]["opponent"]["rank"] = serde_json::json!(9);
        v["match"]["opponent"]["team"] = serde_json::json!({ "name": "Falcons", "logo_url": "/live-assets/teams/14-128.png" });
        v["match"]["opponent"]["avatar_url"] = serde_json::json!("https://cdn.discordapp.com/avatars/1/a.png");
        let o = serde_json::from_value::<Snapshot>(v).unwrap().matched.unwrap().opponent;
        assert_eq!(o.rating, Some(1162));
        assert_eq!(o.rank, Some(9));
        assert_eq!(o.team.as_ref().and_then(|t| t["name"].as_str()), Some("Falcons"));
        assert_eq!(o.avatar_url.as_deref(), Some("https://cdn.discordapp.com/avatars/1/a.png"));

        // Today's server: `rating: null` and nothing else. Every field is simply absent.
        let o = snap(MATCHED).matched.unwrap().opponent;
        assert_eq!((o.rating, o.rank, o.team, o.avatar_url), (None, None, None, None));
    }
```

In `crates/reborn-ui/src/online_mm.rs`'s `mod tests`, add:

```rust
    #[test]
    fn the_match_view_carries_the_opponents_standing_only_when_the_server_sends_it() {
        let with: Snapshot = serde_json::from_value(serde_json::json!({ "match": { "id": "m", "relay_state": "pairing",
            "opponent": { "id": "o", "display_name": "Jock", "rating": 1162, "rank": 9,
                "team": { "name": "Falcons", "logo_url": "/live-assets/teams/14-128.png" }, "avatar_url": "https://cdn.discordapp.com/avatars/1/a.png" } } }))
            .unwrap();
        let v = snapshot_json(&with);
        assert_eq!(v["match"]["opponent"], "Jock", "the name keeps its old key");
        assert_eq!(v["match"]["opponent_elo"], 1162);
        assert_eq!(v["match"]["opponent_rank"], 9);
        assert_eq!(v["match"]["opponent_team"]["name"], "Falcons");
        assert_eq!(v["match"]["opponent_avatar_url"], "https://cdn.discordapp.com/avatars/1/a.png");

        let without: Snapshot = serde_json::from_value(serde_json::json!({ "match": { "id": "m", "opponent": { "id": "o", "display_name": "Jock", "rating": null } } })).unwrap();
        let v = snapshot_json(&without);
        for k in ["opponent_elo", "opponent_rank", "opponent_team", "opponent_avatar_url"] {
            assert!(v["match"][k].is_null(), "{k} must be null when the server sends nothing");
        }
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p reborn-core the_opponents_standing; cargo test -p reborn-ui the_match_view_carries`
Expected: compile errors `no field 'rating' on type 'Opponent'` and, for reborn-ui, an assertion failure on `opponent_elo`.

- [ ] **Step 3: Add the fields**

In `crates/reborn-core/src/matchmaking.rs`, replace the `Opponent` struct with:

```rust
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Opponent {
    pub id: String,
    pub display_name: String,
    /// Elo, rank on the leaderboard, plays-as team and Discord picture: display only (the Online page's matchup card,
    /// spec 2026-10-02 §3.3). Each is absent while the server does not send it (Elo off: no rating, no rank).
    pub rating: Option<i64>,
    pub rank: Option<u32>,
    pub team: Option<serde_json::Value>,
    pub avatar_url: Option<String>,
}
```

In `crates/reborn-ui/src/online_mm.rs` `snapshot_json`, change the `"match"` object's line

```rust
            "opponent": m.opponent.display_name, "field": m.field.as_deref().map(reborn_core::matchmaking::field_title),
```

to

```rust
            "opponent": m.opponent.display_name, "field": m.field.as_deref().map(reborn_core::matchmaking::field_title),
            // Display only (spec 2026-10-02 §3.3): null until the server sends them, and the page then hides each spot.
            "opponent_elo": m.opponent.rating, "opponent_rank": m.opponent.rank,
            "opponent_team": m.opponent.team, "opponent_avatar_url": m.opponent.avatar_url,
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p reborn-core the_opponents_standing; cargo test -p reborn-ui the_match_view_carries; cargo test -p reborn-core matchmaking`
Expected: all PASS. Then `git diff --check` prints nothing and `file crates/reborn-core/src/matchmaking.rs crates/reborn-ui/src/online_mm.rs` does not say CRLF.

- [ ] **Step 5: Add the page states and the failing expectations**

In `Tools/lib/page-states.mjs`, above `    // (redesign states are added above this line)` add:

```js
    N('online-match-found', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, relay_deadline: '2026-09-30T20:00:14.000Z',
      opponent_elo: 1162, opponent_rank: 9, opponent_team: LB_TEAM, opponent_avatar_url: null }) }) }, { init: PICK('ranked') }),
    N('online-accepted', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, your_accepted: true, opponent_elo: 1162, opponent_rank: 9 }) }) }),
    N('online-starting', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, status: 'starting', relay_state: 'room_formed', your_accepted: true, your_ready: true }) }) }),
    N('online-playing', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, status: 'running', relay_state: 'running', your_accepted: true, your_ready: true, series: [1, 0] }),
      chat: [{ seq: 1, name: 'Jock', body: 'gg', at: '2026-09-30T19:58:00Z' }] }) }),
    N('online-playing-casual', { mm_state: mm({ ...empty, match: match({ id: 'm6', status: 'running', relay_state: 'running', your_accepted: true, your_ready: true }) }) }),
    N('online-match-bare', { mm_state: mm({ ...empty, match: match({ id: 'm7', opponent: null, best_of: null, series: null, relay_deadline: 'not a date', opponent_elo: null, opponent_rank: 0, opponent_team: null }) }) }),
```

In `Tools/check-online-page.mjs`, above `  // (states are added above this line)` add:

```js
  'online-match-found': {
    text: ['Match found', 'VS', 'Ranked · first to two wins', 'Accept within 0:14', 'Jock', '#9 · 1,162 Elo', 'Adam'],
    absent: ['best of', 'Not on our servers?'],
    check: async (page) => {
      const p = [];
      for (const b of ['Accept', 'Decline', 'Leave match']) if (!(await page.getByRole('button', { name: b, exact: true }).count())) p.push(`no ${b} button`);
      if (await page.locator('.on-vs .sweep').count()) p.push('the line sweeps before anyone accepted');
      if ((await page.locator('.on-vs img.team-logo, .on-vs .team-logo').count()) < 1) p.push("the opponent's team logo is missing");
      await page.getByRole('button', { name: 'Accept', exact: true }).click();
      await page.waitForTimeout(300);
      const a = await calls(page, 'mm_match_action');
      if (JSON.stringify(a[0]) !== '{"matchId":"m5","action":"accept","value":true}') p.push(`Accept sent ${JSON.stringify(a[0])}`);
      return p;
    },
  },
  'online-accepted': {
    text: ['Accepted. Waiting for Jock…'],
    check: async (page) => {
      const p = [];
      if (!(await page.locator('.on-ok').isVisible())) p.push('no green tick on your side');
      if (await page.getByRole('button', { name: 'Accept', exact: true }).count()) p.push('Accept is still offered');
      return p;
    },
  },
  'online-starting': {
    text: ['Connecting you both', 'Your game opens on its own'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-vs .sweep').count()) !== 1) p.push('no sweeping line while the relay moves');
      await page.getByRole('button', { name: 'Leave match', exact: true }).click();
      await page.waitForTimeout(300);
      const a = await calls(page, 'mm_match_action');
      if (JSON.stringify(a[0]) !== '{"matchId":"m5","action":"leave","value":null}') p.push(`Leave match sent ${JSON.stringify(a[0])}`);
      return p;
    },
  },
  'online-playing': { text: ['Playing', 'Game 2 · Series 1–0', 'Ranked · first to two wins', 'gg'], absent: ['Accept within', 'Your game opens'] },
  'online-playing-casual': { text: ['Playing', 'Casual · one game'], absent: ['Series'] },
  'online-match-bare': { text: ['your opponent'], absent: ['undefined', 'NaN', 'null', 'Accept within', ' · Elo'] },
```

In `Tools/check-online-mm.mjs`, change the `pairing`, `running_bo3` and `relay_failed` lines of `EXPECT` to:

```js
  pairing: { text: ['Jock', 'Match found', 'VS', 'Casual · one game'], buttons: ['Accept', 'Decline', 'Leave match'] },
  running_bo3: { text: ['Jock', 'Playing', 'Ranked · first to two wins', 'Game 2 · Series 1–0'], buttons: ['Leave match'], absent: ['Accept', 'City Limits', 'best of'] },
  relay_failed: { text: ['Jock', 'Getting ready', 'does not support launcher Netplay'], buttons: ['Leave match'] },
```

- [ ] **Step 6: Run them to see them fail**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs`
Expected: `FAIL online-match-found: missing text "Match found"…` etc.; check-online-mm `FAIL pairing`, `FAIL running_bo3` (and baseline `held`).

- [ ] **Step 7: Rewrite `drawMatch`**

Change the online-model import line to `import { acceptWithin, clock, eloBits, modeLine, seekLine, seriesLine, standing } from './online-model.js';` and add below the `shell.js` import: `import { avatar, teamLogo } from './match-rows.js';`

After the `rankInfo` listener add:

```js
// Who you are, for your own picture on the matchup card: Play caches the Discord picture under `reborn:avatar`
// (home.js) keyed by player id; any other id or no cache draws your letter.
let me = null;
if (bridge) call('identity_status').then((v) => { if (v?.signed_in) { me = { id: v.player_id, name: v.display_name }; lastDrawn = ''; } }).catch(() => {});
function myAvatarUrl() {
  try { const v = JSON.parse(localStorage.getItem('reborn:avatar') || 'null'); return v && me && v.id === me.id ? v.url : null; } catch { return null; }
}
// The relay is moving (accepted … running): one red line along the bottom of the card (spec §3.5).
const MOVING = new Set(['accepted', 'field', 'ready', 'go', 'host_launching', 'code_ready', 'joining', 'room_formed', 'running', 'intermission']);
```

Replace the whole old `function drawMatch(m, chat, relayError) {…}` with:

```js
function face({ url, name, sub, team, tick = false }) {
  return el('div', { class: 'on-p' },
    el('span', { class: 'on-avw' }, avatar(url, name, 'on-av'), tick ? el('span', { class: 'on-ok', role: 'img', 'aria-label': 'Accepted', text: '✓' }) : null),
    el('span', { class: 'on-pname', text: name }),
    sub ? el('span', { class: 'on-psub num', text: sub }) : null,
    team ? teamLogo(team, 'on-plogo') : null);
}

// The matchup card (spec §3.3–§3.6). Only called for a match that is not over (complete, void, abandoned, held).
function drawMatch(m, chat, relayError, snap) {
  const words = RELAY[m.relay_state] ?? 'Starting the game';
  const opp = m.opponent || 'your opponent';
  const pairing = m.relay_state === 'pairing';
  const playing = m.relay_state === 'running' || m.relay_state === 'intermission';
  const series = playing ? seriesLine(m) : null;
  const mid = el('div', { class: 'on-mid' },
    el('span', { class: 'k-label on-found', text: m.relay_state === 'running' ? 'Playing' : 'Match found' }),
    el('span', { class: 'on-x', 'aria-hidden': 'true', text: 'VS' }),
    series ? el('span', { class: 'on-ser num', text: series }) : null,
    el('span', { class: 'on-mode-line', text: modeLine(m.mode) }));
  if (pairing && !m.your_accepted) {
    mid.append(el('div', { class: 'on-acts' },
      el('button', { type: 'button', class: 'btn primary', 'data-key': 'accept', 'data-primary': 'true', text: 'Accept', onclick: () => act('mm_match_action', { matchId: m.id, action: 'accept', value: true }) }),
      el('button', { type: 'button', class: 'btn', 'data-key': 'decline', text: 'Decline', onclick: () => act('mm_match_action', { matchId: m.id, action: 'accept', value: false }) })));
    const left = acceptWithin(m.relay_deadline, Date.now());
    if (left) mid.append(el('span', { class: 'on-tm num', text: left }));
  } else if (pairing) {
    mid.append(el('span', { class: 'on-st' }, el('i', { 'aria-hidden': 'true' }), `Accepted. Waiting for ${opp}…`));
  } else if (m.relay_state !== 'running') {
    mid.append(el('span', { class: 'on-st' }, el('i', { 'aria-hidden': 'true' }), words), el('span', { class: 'on-tm', text: 'Your game opens on its own' }));
  }
  // The relay's own failure (the room would not open, the code would not send) in its own words.
  if (relayError) mid.append(el('p', { class: 'refusal', text: relayError }));
  const mine = eloBits(rankInfo?.elo);
  const card = el('section', { class: 'on-vs', 'aria-label': `${modeLine(m.mode)} against ${opp}` },
    el('img', { class: 'on-art', src: 'art/online/matchup.jpg', alt: '' }),
    el('button', { type: 'button', class: 'btn quiet sm on-leave', 'data-key': 'leave-match', text: 'Leave match', onclick: () => act('mm_match_action', { matchId: m.id, action: 'leave', value: null }) }),
    el('div', { class: 'on-vs-in' },
      face({ url: myAvatarUrl(), name: snap?.player?.name || me?.name || 'You', sub: [mine.rank, mine.elo].filter(Boolean).join(' · '),
        team: rankInfo?.playsAs ?? null, tick: !!m.your_accepted && !playing }),
      mid,
      face({ url: m.opponent_avatar_url ?? null, name: opp, sub: standing(m.opponent_rank, m.opponent_elo), team: m.opponent_team ?? null })),
    MOVING.has(m.relay_state) ? el('span', { class: 'sweep', 'aria-hidden': 'true' }) : null);
  // Match chat stays, under the card (nothing that exists is removed).
  return [card, chatBox(chat, (t) => act('mm_match_action', { matchId: m.id, action: 'chat', value: t }))];
}
```

In `draw()`, change `body = drawMatch(snap.match, snap.chat ?? [], state.relay_error);` to `body = drawMatch(snap.match, snap.chat ?? [], state.relay_error, snap);`.

In `chatBox`, change the input to `el('input', { type: 'text', maxlength: 300, placeholder: 'Say something…', 'aria-label': 'Chat message', 'data-key': 'chat' })` (spec §3.7 wording for the field) and the Send button to `el('button', { type: 'submit', class: 'btn quiet sm', text: 'Send' })`.

(The old `drawMatch` paragraphs for `held` reasons are gone with it: `draw()` only calls `drawMatch` for a match that is not over, so they could never show; the held notice on the idle page carries those words.)

- [ ] **Step 8: Append the matchup rules to `pages/online.css`**

```css
/* the matchup card (spec §3.3–§3.6): you | MATCH FOUND, VS, the mode line | the opponent */
.on-vs{position:relative;overflow:hidden;flex:1;min-height:300px;border-radius:var(--r-lg);border:1px solid var(--stroke);box-shadow:var(--shadow);isolation:isolate}
.on-vs>.on-art{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;z-index:-2}
.on-vs::after{content:"";position:absolute;inset:0;background:var(--vs-scrim);z-index:-1}
.on-vs-in{position:absolute;inset:0;display:grid;grid-template-columns:minmax(0,1fr) auto minmax(0,1fr);align-items:center;gap:16px;padding:0 48px}
.on-p{display:flex;flex-direction:column;align-items:center;gap:10px;min-width:0;text-align:center}
.on-avw{position:relative}
.avatar.on-av{width:104px;height:104px;font-size:42px;box-shadow:var(--face-ring)}
.on-ok{position:absolute;right:-2px;bottom:-2px;display:grid;place-items:center;width:26px;height:26px;border-radius:50%;background:var(--ok);color:var(--badge-bg);font:700 14px Inter,system-ui,sans-serif}
.on-pname{max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font:700 24px/1.1 Inter,system-ui,sans-serif;letter-spacing:-.02em}
.on-psub{font-size:13px;color:var(--fg-2)}
.team-logo.on-plogo{width:44px;height:44px}
.team-logo.on-plogo.text{font-size:13px}
.on-mid{display:flex;flex-direction:column;align-items:center;gap:8px;max-width:360px;text-align:center}
.on-x{font:700 56px/1 Inter,system-ui,sans-serif;letter-spacing:-.04em;color:var(--red-ink)}
.on-mode-line{font-size:13px;color:var(--fg-2)}
.on-ser{font:700 15px Inter,system-ui,sans-serif}
.on-acts{margin-top:12px;display:flex;justify-content:center;gap:10px}
.on-acts .btn{height:48px;min-width:140px}
.on-tm{font-size:12px;color:var(--fg-3)}
.on-st{margin-top:10px;display:inline-flex;align-items:center;gap:10px;padding:8px 16px;border-radius:999px;background:var(--pill-bg);border:1px solid var(--stroke);font-size:13px;white-space:nowrap}
.on-st i{width:7px;height:7px;border-radius:50%;background:var(--ok)}
.on-leave{position:absolute;top:16px;right:16px;z-index:2}
.on-vs .sweep{z-index:2}
/* chat (a match's, and your room's) */
.mm-chat{list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:8px;min-height:0;overflow:auto;font-size:13px;color:var(--fg-2)}
.mm-chat strong{font-weight:600;color:var(--fg)}
.mm-chat-form{display:flex;gap:8px;margin-top:8px}
.mm-chat-form input{flex:1;height:40px}
```

- [ ] **Step 9: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs; node Tools/check-theme-tokens.mjs`
Expected: `check-online-page: PASS (12 state(s))`; check-online-mm all `ok` except baseline `held`; tokens PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/reborn-core/src/matchmaking.rs crates/reborn-ui/src/online_mm.rs crates/reborn-ui/ui/online-mm.js crates/reborn-ui/ui/pages/online.css Tools/lib/page-states.mjs Tools/check-online-page.mjs Tools/check-online-mm.mjs
git commit -m "online: the matchup card (match found, accepted, starting, playing) and the opponent's standing in mm_state"
```

---

### Task 6: Your room (code, invite link, players, chat, Start/Ready/Leave room)

**Files:**
- Modify: `crates/reborn-core/src/matchmaking.rs:107-117` (`Room`) and tests
- Modify: `crates/reborn-ui/src/online_mm.rs` (`snapshot_json` room) and tests
- Modify: `crates/reborn-ui/ui/online-mm.js` (import, `drawRoom`)
- Modify: `crates/reborn-ui/ui/pages/online.css` (append)
- Modify: `Tools/lib/page-states.mjs` (one state), `Tools/check-online-page.mjs` (one entry), `Tools/check-online-mm.mjs` (`lobby` state + expectation)

**Interfaces:**
- Consumes: `act`, `chatBox`, `avatar`; `everyoneReady`, `inviteLink` (online-model.js).
- Produces: `mm_state().snapshot.room.you_host: boolean`; core `Room.you_host: bool`. DOM `data-key`s: `copy-code`, `copy-invite`, `room-start`, `room-ready`, `room-leave`, `chat`.

- [ ] **Step 1: Write the failing Rust tests**

In `crates/reborn-core/src/matchmaking.rs`'s tests add:

```rust
    #[test]
    fn a_room_says_whether_you_host_it() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r", "code": "K7QP2M", "you_host": true, "members": [] } })).unwrap();
        assert!(s.room.unwrap().you_host);
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r", "code": "K7QP2M", "members": [] } })).unwrap();
        assert!(!s.room.unwrap().you_host, "an older server says nothing: not the host");
    }
```

In `crates/reborn-ui/src/online_mm.rs`'s tests add:

```rust
    #[test]
    fn the_room_view_says_whether_you_host_it() {
        let s: Snapshot = serde_json::from_value(serde_json::json!({ "room": { "id": "r", "code": "K7QP2M", "mode": "casual", "you_host": true, "members": [] } })).unwrap();
        assert_eq!(snapshot_json(&s)["room"]["you_host"], true);
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p reborn-core a_room_says_whether; cargo test -p reborn-ui the_room_view_says`
Expected: compile error `no field 'you_host' on type 'Room'`.

- [ ] **Step 3: Add the field**

In `matchmaking.rs` `pub struct Room`, after `pub members: Vec<RoomMember>,` add:

```rust
    /// The signed-in player hosts this room (the server's `you_host`): only the host is offered Start.
    pub you_host: bool,
```

In `online_mm.rs` `snapshot_json`, change `"id": r.id, "code": r.code, "mode": r.mode, "best_of": r.best_of, "status": r.status,` to `"id": r.id, "code": r.code, "mode": r.mode, "best_of": r.best_of, "status": r.status, "you_host": r.you_host,`.

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p reborn-core a_room_says_whether; cargo test -p reborn-ui the_room_view_says; git diff --check`
Expected: PASS; `git diff --check` prints nothing.

- [ ] **Step 5: Add the state and failing expectations**

In `Tools/lib/page-states.mjs` above the redesign marker add:

```js
    N('online-room', { mm_state: mm({ ...empty, room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open', you_host: true,
      members: [{ name: 'Adam', ready: true, field: null, you: true }, { name: 'Jock', ready: false, field: null, you: false }] },
      chat: [{ seq: 1, name: 'Jock', body: 'ready when you are', at: '2026-09-30T19:58:00Z' }] }) },
      { init: "Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async(t)=>{window.__copied=t;}}})" }),
```

In `Tools/check-online-page.mjs` above the states marker add:

```js
  'online-room': {
    text: ['Your room', 'Casual', 'Room code', 'K7QP2M', 'Players', 'Adam', 'you', 'Ready', 'Not ready', 'Chat', 'ready when you are',
      "Start works once you're both ready. The host starts the game."],
    absent: ['Lobby code', 'Leave lobby'],
    check: async (page) => {
      const p = [];
      if (!(await page.getByRole('button', { name: 'Start', exact: true }).isDisabled())) p.push('Start is enabled while Jock is not ready');
      await page.getByRole('button', { name: 'Copy invite link', exact: true }).click();
      await page.waitForTimeout(200);
      if (await page.evaluate(() => window.__copied) !== 'https://nflstreetreborn.com/join.php?r=K7QP2M') p.push(`copied ${await page.evaluate(() => window.__copied)}`);
      await page.getByRole('button', { name: 'Copy code', exact: true }).click();
      await page.waitForTimeout(200);
      if (await page.evaluate(() => window.__copied) !== 'K7QP2M') p.push('Copy code did not copy the code');
      await page.getByRole('button', { name: "I'm not ready", exact: true }).click();
      await page.waitForTimeout(300);
      const r = await calls(page, 'mm_room_action');
      if (JSON.stringify(r[0]) !== '{"roomId":"r1","action":"ready","value":false}') p.push(`ready sent ${JSON.stringify(r[0])}`);
      await page.getByRole('button', { name: 'Leave room', exact: true }).click();
      await page.waitForTimeout(300);
      const l = (await calls(page, 'mm_room_action')).at(-1);
      if (JSON.stringify(l) !== '{"roomId":"r1","action":"leave","value":null}') p.push(`Leave room sent ${JSON.stringify(l)}`);
      return p;
    },
  },
```

In `Tools/check-online-mm.mjs`, in `STATES.lobby`'s room object add `you_host: true,` after `status: 'open',` (the launcher now forwards the server's `you_host`), and change the `lobby` expectation to:

```js
  lobby: { text: ['Your room', 'K7QP2M', 'Adam', 'you', 'Jock', 'Ready', 'Not ready', 'ready when you are'], buttons: ["I'm ready", 'Start', 'Leave room', 'Send', 'Copy code', 'Copy invite link'], absent: ['field pick'] },
```

- [ ] **Step 6: Run them to see them fail**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs`
Expected: `FAIL online-room: missing text "Your room"…`; check-online-mm `FAIL lobby`.

- [ ] **Step 7: Rewrite `drawRoom`**

Change the online-model import to `import { acceptWithin, clock, eloBits, everyoneReady, inviteLink, modeLine, seekLine, seriesLine, standing } from './online-model.js';`. Replace the whole old `function drawRoom(r, chat) {…}` with:

```js
// Your room (spec §3.7): the code to share, who is in and ready, chat; Start for the host once everyone is ready.
function drawRoom(r, chat) {
  // The launcher marks which member is you; member order means nothing.
  const me = r.members.find((m) => m.you);
  const ready = !!me?.ready;
  const copyButton = (key, label, text) => {
    const b = el('button', { type: 'button', class: 'btn sm', 'data-key': key, text: label });
    b.addEventListener('click', () => navigator.clipboard?.writeText(text).then(() => { b.textContent = 'Copied'; }, () => {}));
    return b;
  };
  const members = el('ul', { class: 'on-members' });
  for (const m of r.members) {
    members.append(el('li', {},
      avatar(null, m.name, 'on-mav'),
      el('b', {}, m.name, m.you ? el('small', { text: 'you' }) : null),
      el('span', { class: m.ready ? 'on-rd' : 'on-nr', text: m.ready ? 'Ready' : 'Not ready' })));
  }
  const room = el('section', { class: 'glass on-card', 'aria-label': 'Your room' },
    el('div', { class: 'on-card-h' }, el('h2', { text: 'Your room' }), el('span', { text: r.mode === 'ranked' ? 'Ranked' : 'Casual' })),
    el('span', { class: 'k-label', text: 'Room code' }),
    el('strong', { class: 'on-codebig', text: r.code }),
    el('div', { class: 'on-bt' }, copyButton('copy-code', 'Copy code', r.code), copyButton('copy-invite', 'Copy invite link', inviteLink(r.code))),
    el('span', { class: 'k-label', text: 'Players' }),
    members);
  const talk = el('section', { class: 'glass on-card', 'aria-label': 'Chat' },
    el('div', { class: 'on-card-h' }, el('h2', { text: 'Chat' })),
    chatBox(chat, (t) => act('mm_room_action', { roomId: r.id, action: 'chat', value: t })));
  const row = el('div', { class: 'on-row' },
    r.you_host === true ? el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'room-start', 'data-primary': 'true', text: 'Start',
      disabled: !everyoneReady(r), onclick: () => act('mm_room_action', { roomId: r.id, action: 'start', value: null }) }) : null,
    el('button', { type: 'button', class: 'btn', 'data-key': 'room-ready', text: ready ? "I'm not ready" : "I'm ready",
      onclick: () => act('mm_room_action', { roomId: r.id, action: 'ready', value: !ready }) }),
    el('button', { type: 'button', class: 'btn quiet', 'data-key': 'room-leave', text: 'Leave room',
      onclick: () => act('mm_room_action', { roomId: r.id, action: 'leave', value: null }) }));
  return [el('div', { class: 'on-room' }, room, talk), row, el('p', { class: 'on-note', text: "Start works once you're both ready. The host starts the game." })];
}
```

Change `chatBox`'s returned wrapper from `return el('div', {}, log, form);` to `return el('div', { class: 'on-chat' }, log, form);`.

- [ ] **Step 8: Append the room rules to `pages/online.css`**

```css
/* your room (spec §3.7) */
.on-room{flex:1;min-height:0;display:grid;grid-template-columns:1fr 1fr;gap:14px}
.on-card{display:flex;flex-direction:column;min-height:0;padding:18px 20px}
.on-card-h{display:flex;align-items:baseline;justify-content:space-between;gap:8px;margin-bottom:12px}
.on-card-h h2{margin:0;font:600 16px/1.2 Inter,system-ui,sans-serif}
.on-card-h span{font-size:12px;color:var(--fg-3)}
.on-codebig{margin:4px 0 6px;font:700 44px/1 Inter,system-ui,sans-serif;letter-spacing:.12em}
.on-bt{display:flex;flex-wrap:wrap;gap:8px;margin:12px 0 16px}
.on-members{list-style:none;margin:0;padding:0}
.on-members li{display:flex;align-items:center;gap:12px;height:52px;border-top:1px solid var(--stroke)}
.on-members b{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:14px;font-weight:600}
.on-members small{margin-left:6px;font-size:12px;font-weight:500;color:var(--fg-3)}
.on-rd{font-size:12px;font-weight:600;color:var(--ok)}
.on-nr{font-size:12px;color:var(--fg-3)}
.on-chat{flex:1;display:flex;flex-direction:column;min-height:0}
@media (max-width:1000px){.on-room{grid-template-columns:1fr}}
```

- [ ] **Step 9: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs; node Tools/check-theme-tokens.mjs`
Expected: `check-online-page: PASS (13 state(s))`; check-online-mm all `ok` but baseline `held`; tokens PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/reborn-core/src/matchmaking.rs crates/reborn-ui/src/online_mm.rs crates/reborn-ui/ui/online-mm.js crates/reborn-ui/ui/pages/online.css Tools/lib/page-states.mjs Tools/check-online-page.mjs Tools/check-online-mm.mjs
git commit -m "online: your room (code, invite link, players, chat, Start/ready/Leave room) and you_host in mm_state"
```

---
### Task 7: Held, signed out, offline and matchmaking off

**Files:**
- Modify: `crates/reborn-ui/ui/online-mm.js` (imports, sign-in, `lockPanel`, `directRow`, the held notice, the whole `draw()`)
- Modify: `crates/reborn-ui/ui/pages/online.css` (append)
- Modify: `Tools/lib/page-states.mjs` (five states), `Tools/check-online-page.mjs` (five entries), `Tools/check-online-mm.mjs` (`held` expectation)

**Interfaces:**
- Consumes: `tiles(snap, { locked, only })`, `drawIdle`, `drawMatch`, `drawRoom`, `drawQueued`, `setLinks`, `picked`, `markBusy`, `lastDrawn`, `busy` (online-mm.js); `heldNotice` (online-model.js); `createSignIn({ onChange, onDone }) -> { state, start, cancel }` (sign-in.js); `#direct-open` (Task 2).
- Produces: `lockPanel(title, text, withButton) -> HTMLElement` (Task 12 passes an invite title); `let dismissedHold` (match id). DOM `data-key`s: `signin`, `signin-cancel`, `held-dismiss`, `direct`.

- [ ] **Step 1: Add the states and failing expectations**

In `Tools/lib/page-states.mjs` above the redesign marker add:

```js
    N('online-held', { mm_state: mm({ ...empty, match: match({ id: 'm3', mode: 'ranked', status: 'held', relay_state: 'held', best_of: 3, your_accepted: true, your_ready: true,
      series: [1, 1], wins_needed: 2, hold_reason: 'content-mismatch' }) }) }),
    N('online-held-other', { mm_state: mm({ ...empty, match: match({ id: 'm8', status: 'held', relay_state: 'held', your_accepted: true, your_ready: true, hold_reason: 'series-too-fast' }) }) }),
    N('online-signed-out', { mm_state: mm(null, { link: 'signed_out' }),
      identity_sign_in_start: { device_code: 'dev-1', user_code: 'ABCD-1234', verify_url: 'https://discord.com/activate', interval: 5 } }, { init: PICK('ranked') }),
    N('online-offline', { mm_state: mm(null, { link: 'offline' }) }),
    N('online-disabled', { mm_state: mm(null, { link: 'disabled' }) }),
```

In `Tools/check-online-page.mjs` above the states marker add:

```js
  'online-held': {
    text: ['Your last match is held for review', "Your game files differ from Jock's, so that game was not started.", 'Find a casual game'],
    absent: ['Leave match', 'Accept'],
    check: async (page) => {
      const p = [];
      const notice = page.locator('.on-notice');
      if (!(await notice.isVisible())) return ['no held notice'];
      const n = await notice.boundingBox(), t = await page.locator('.on-modes').boundingBox();
      if (!t || n.y > t.y) p.push('the notice is not above the tiles');
      await page.getByRole('button', { name: 'Dismiss', exact: true }).click();
      await page.waitForTimeout(300);
      if (await page.locator('.on-notice').count()) p.push('Dismiss did not hide the notice');
      return p;
    },
  },
  'online-held-other': { text: ['Nothing changed on the leaderboard. Someone will look at it. You can play again now.'], absent: ['ladder'] },
  'online-signed-out': {
    text: ['Sign in to play online', 'Connect your Discord account to play online and track your stats!', 'Sign in with Discord', 'Live now', 'Direct connection'],
    absent: ['Find a', 'searching', '1,184 Elo', '#7'],
    check: async (page) => {
      const p = [];
      if (!(await page.locator('.on-modes[inert]').count())) p.push('the tiles behind the panel can still be reached');
      await page.getByRole('button', { name: 'Sign in with Discord', exact: true }).click();
      await page.getByText('Your code is ABCD-1234', { exact: true }).waitFor({ timeout: 3000 }).catch(() => p.push('the device code is not shown'));
      if (!(await calls(page, 'identity_sign_in_start')).length) p.push('Sign in did not start the device-code flow');
      return p;
    },
  },
  'online-offline': { text: ["Can't reach the Reborn servers right now.", 'Trying again.'], absent: ['Sign in with Discord', 'Find a'] },
  'online-disabled': {
    text: ['Private room', 'Direct connection'],
    absent: ['Find a', "Can't reach", 'Sign in'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-tile').count()) !== 1) p.push(`${await page.locator('.on-tile').count()} tiles with matchmaking off`);
      await page.locator('.on-row [data-key="direct"]').click();
      if (!(await page.locator('#direct-dialog').evaluate((d) => d.open))) p.push('the Direct connection button did not open the dialog');
      return p;
    },
  },
```

In `Tools/check-online-mm.mjs`, change the `held` expectation to:

```js
  held: { text: ['Your last match is held for review', 'Nothing changed on the leaderboard'], absent: ['Leave match', 'Accept'] },
```

- [ ] **Step 2: Run them to see them fail**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs`
Expected: `FAIL online-held: no held notice`, `FAIL online-signed-out: missing text "Sign in to play online"…`, `FAIL online-offline…`, `FAIL online-disabled…`; check-online-mm `FAIL held: missing text "Your last match is held for review"`.

- [ ] **Step 3: Sign-in, the lock panel, the direct row, the held notice**

Add to the imports of `online-mm.js`: `import { createSignIn } from './sign-in.js';` and add `heldNotice` to the online-model import, so it reads `import { acceptWithin, clock, eloBits, everyoneReady, heldNotice, inviteLink, modeLine, seekLine, seriesLine, standing } from './online-model.js';`.

After `drawIdle` add:

```js
// ---- signed out, offline, matchmaking off, a held match (spec §3.8, §3.9) ----
// The same Discord device-code flow as Stats (sign-in.js). Once it is done the launcher's own poll sees the account.
const signIn = createSignIn({ onChange: () => { lastDrawn = ''; draw(); }, onDone: () => { lastDrawn = ''; draw(); } });

/** One panel over the blurred, unreachable tiles. `withButton`: Sign in with Discord (and, while it waits, the code). */
function lockPanel(title, text, withButton) {
  const si = signIn.state;
  const parts = withButton && si.active
    ? [el('b', { text: 'Waiting for Discord' }),
      el('p', { role: 'status', text: `Your code is ${si.userCode}` }),
      si.verifyUrl ? el('p', { text: `Go to ${si.verifyUrl} to approve it.` }) : null,
      el('button', { type: 'button', class: 'btn', 'data-key': 'signin-cancel', text: 'Cancel', onclick: () => signIn.cancel() })]
    : [el('b', { text: title }),
      el('p', { text }),
      withButton ? el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'signin', 'data-primary': 'true',
        text: si.starting ? 'Opening Discord…' : 'Sign in with Discord', disabled: si.starting, onclick: () => signIn.start() }) : null,
      withButton && si.message ? el('p', { class: 'refusal', role: 'alert', text: si.message }) : null];
  return el('div', { class: 'on-locked' }, tiles(null, { locked: true }),
    el('div', { class: 'on-lock', role: 'region', 'aria-label': title }, ...parts.filter(Boolean)));
}

/** Matchmaking off: the private play that works without it is the direct connection (ruling 7). */
function directRow() {
  return el('div', { class: 'on-row' },
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'direct', 'data-primary': 'true', text: 'Direct connection',
      onclick: () => document.getElementById('direct-open')?.click() }),
    el('p', { class: 'on-note', text: 'Send the code to a friend' }));
}

// A held match is over for the player the moment it is held (seats released), but the server keeps showing it for a
// few minutes: one notice above the normal page, never the page itself (09-26). Dismiss hides it for this match.
let dismissedHold = null;
function heldBox(m) {
  const n = heldNotice(m);
  return el('div', { class: 'partial on-notice', role: 'status' },
    el('div', {}, el('b', { text: n.title }), el('span', { text: n.text })),
    el('button', { type: 'button', class: 'btn quiet sm', 'data-key': 'held-dismiss', text: 'Dismiss',
      onclick: () => { dismissedHold = m.id; lastDrawn = ''; draw(); } }));
}
```

- [ ] **Step 4: Replace `draw()`**

Replace the whole `async function draw() {…}` with:

```js
async function draw() {
  if (!bridge) {
    setLinks(true);
    section.replaceChildren(el('p', { class: 'note', text: 'Matchmaking needs the Reborn launcher.' }));
    return;
  }
  let state;
  try { state = await call('mm_state'); } catch (e) { setLinks(true); section.replaceChildren(el('p', { class: 'note', text: reason(e) })); return; }
  // A launcher build without matchmaking, or a test bridge that does not know this command, answers null. That is not
  // an error the player can act on, so the panel simply stays out of the way.
  if (!state || typeof state !== 'object') { setLinks(true); section.replaceChildren(); return; }
  // The kill switch: the server has matchmaking off (or predates it). Only the Private room tile, whose row opens the
  // direct connection (spec §3.9). Drawn once, not every second.
  if (state.link === 'disabled') {
    markBusy(false);
    setLinks(true);
    if (lastDrawn === 'disabled') return;
    lastDrawn = 'disabled';
    section.replaceChildren(tiles(null, { only: 'private' }), directRow());
    return;
  }

  // Only redraw when something the player can see changed, so typing in chat is never interrupted.
  const signature = JSON.stringify({ l: state.link, s: state.snapshot, r: state.relay_error, p: state.presence_on, m: mode() });
  if (signature === lastDrawn || busy) return;
  // Only a text field being typed in (the chat, the room code) holds a redraw. The presence switch is a button, and
  // focus left on a control must never freeze the page (09-26).
  const active = document.activeElement;
  const focused = section.contains(active) && active.tagName === 'INPUT' && ['text', 'search', 'url', 'email'].includes(active.type) && active.value !== '';
  if (focused) return;
  lastDrawn = signature;

  const snap = state.snapshot;
  // A HELD match is not busy: the server releases a held match's seats straight away, so the player is free to play
  // again. The hold notice still shows; the rest of the page stays usable.
  const liveMatch = snap?.match && !['complete', 'void', 'abandoned', 'held'].includes(snap.match.relay_state);
  markBusy(!!(liveMatch || snap?.room || snap?.ticket));
  let body;
  let showLinks = true;
  if (state.link === 'signed_out') {
    body = [lockPanel('Sign in to play online', 'Connect your Discord account to play online and track your stats!', true)];
  } else if (state.link === 'offline' && !snap) {
    body = [lockPanel("Can't reach the Reborn servers right now.", 'Trying again.', false)];
  } else if (!snap) {
    body = [el('p', { class: 'loading', text: 'Connecting…' })];
  } else if (liveMatch) {
    body = drawMatch(snap.match, snap.chat ?? [], state.relay_error, snap);
  } else if (snap.room) {
    body = drawRoom(snap.room, snap.chat ?? []);
  } else if (snap.ticket) {
    body = drawQueued(snap.ticket);
  } else {
    body = drawIdle(snap);
    showLinks = picked() === 'private';
    if (snap.match?.relay_state === 'held' && dismissedHold !== snap.match.id) body.unshift(heldBox(snap.match));
  }
  setLinks(showLinks);
  section.replaceChildren(...body, status, drawOnline(snap), presenceToggle(state.presence_on));
}
```

- [ ] **Step 5: Append the rules to `pages/online.css`**

```css
/* signed out / offline (spec §3.9): one panel over the blurred tiles */
.on-locked{position:relative;flex:1;display:flex;flex-direction:column;min-height:0}
.on-locked .on-modes{filter:blur(6px)}
.on-lock{position:absolute;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:12px;padding:24px;border-radius:var(--r-lg);background:var(--lock-bg);text-align:center}
.on-lock b{font:700 26px/1.1 Inter,system-ui,sans-serif;letter-spacing:-.02em;color:var(--fg)}
.on-lock p{margin:0;max-width:360px;font-size:14px;color:var(--fg-2)}
.on-lock .btn.on-go{flex:none;min-width:280px}
/* a held match (spec §3.8): .partial is theme.css's amber message */
.on-notice{display:flex;align-items:flex-start;gap:16px}
.on-notice b{display:block;margin-bottom:4px;font-weight:600}
.on-notice span{color:var(--fg-2)}
.on-notice .btn{flex:none;margin-left:auto}
```

- [ ] **Step 6: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs; node Tools/check-online-restored.mjs; node Tools/check-theme-tokens.mjs`
Expected: `check-online-page: PASS (18 state(s))`; `check-online-mm`: every state `ok` and `PASS: 10 matchmaking states …` (the baseline failure is gone); the others PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/online-mm.js crates/reborn-ui/ui/pages/online.css Tools/lib/page-states.mjs Tools/check-online-page.mjs Tools/check-online-mm.mjs
git commit -m "online: held notice, sign-in panel, offline panel, matchmaking-off private tile"
```

---

### Task 8: Players online and the presence switch in the heading

**Files:**
- Modify: `crates/reborn-ui/ui/menu.js` (extract `attach()`, add `openPanel()`)
- Modify: `crates/reborn-ui/ui/theme.css` (`dialog.menu.panel`), `crates/reborn-ui/ui/DESIGN.md` (§6 `dialog.menu` row)
- Modify: `crates/reborn-ui/ui/online-mm.js` (import, `showOnline`, `presencePanel`; delete `drawOnline` and `presenceToggle`; the last lines of `draw()`)
- Modify: `crates/reborn-ui/ui/pages/online.css` (append)
- Modify: `Tools/lib/page-states.mjs` (one state), `Tools/check-online-page.mjs` (one entry, one static rule), `Tools/check-online-mm.mjs` (`idle` text)

**Interfaces:**
- Consumes: `act` (online-mm.js); `#on-players` (Task 2).
- Produces: `openPanel(anchor: HTMLElement, { label: string, build: () => HTMLElement }) -> HTMLDialogElement` in `menu.js` (a `dialog.menu.panel`, modal, anchored like `openMenu`; focus returns to `anchor` on close). `showOnline(snap, presenceOn)` in online-mm.js. Task 9 uses `openPanel` for the lobbies.

- [ ] **Step 1: Add the state and failing expectations**

In `Tools/lib/page-states.mjs` above the redesign marker add:

```js
    N('online-presence', { mm_state: mm(empty) }, { init: PICK('ranked') }),
```

In `Tools/check-online-page.mjs`, in the static block, before `if (problems.length) fail('static', …)` add:

```js
  if (!/dialog\.menu\.panel\{/.test(theme)) problems.push('theme.css has no dialog.menu.panel');
```

and above the states marker add:

```js
  'online-presence': {
    text: ['3 players online'],
    check: async (page) => {
      const p = [];
      await page.locator('#on-players').click();
      const panel = page.locator('dialog.menu.panel[open]');
      if (!(await panel.isVisible())) return ['the players button opened nothing'];
      const t = await panel.innerText();
      for (const w of ['Adam', 'Jock', 'Tuna', 'Looking for a game', "Show what I'm doing on Discord"]) if (!t.includes(w)) p.push(`the panel lacks "${w}"`);
      const sw = panel.locator('#mm-presence');
      if (await sw.getAttribute('aria-checked') !== 'true') p.push('the switch does not show presence on');
      await sw.click();
      await page.waitForTimeout(300);
      const s = await calls(page, 'presence_set_enabled');
      if (JSON.stringify(s[0]) !== '{"enabled":false}') p.push(`the switch sent ${JSON.stringify(s[0])}`);
      await page.keyboard.press('Escape');
      await page.waitForTimeout(200);
      if (await page.locator('dialog.menu.panel[open]').count()) p.push('Esc did not close the panel');
      if (await page.evaluate(() => document.activeElement?.id) !== 'on-players') p.push('focus did not return to the players button');
      return p;
    },
  },
```

In `Tools/check-online-mm.mjs`, in the `idle` expectation change `'3 online'` to `'3 players online'`.

- [ ] **Step 2: Run them to see them fail**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs`
Expected: `FAIL static: theme.css has no dialog.menu.panel`, `FAIL online-presence: missing text "3 players online"`; check-online-mm `FAIL idle: missing text "3 players online"`.

- [ ] **Step 3: `openPanel` in `menu.js`**

Replace everything from `export function openMenu(` down to (not including) the comment `// Above the button, right-aligned 8px inside it …` with:

```js
export function openMenu(anchor, { label, options, current = null, onPick }) {
  openNow?.close();
  let menu = null;
  const items = options.map((o) => el("button", {
    type: "button", class: "menu-item", role: "menuitemradio", "aria-checked": String(o.value === current), "data-value": o.value,
    ...(o.value === current ? { "data-primary": "true" } : {}),
    onclick: () => { menu.close(); onPick(o.value); },
  }, el("span", { text: o.label }), el("span", { class: "menu-check", "aria-hidden": "true", text: "✓" })));
  menu = el("dialog", { class: "menu", "aria-label": label }, el("div", { class: "menu-list", role: "menu", "aria-label": label }, ...items));
  menu.addEventListener("keydown", (e) => {
    const i = items.indexOf(document.activeElement);
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (step) { e.preventDefault(); items[(i + step + items.length) % items.length].focus(); }
    else if (e.key === "Home" || e.key === "End") { e.preventDefault(); items[e.key === "Home" ? 0 : items.length - 1].focus(); }
  });
  attach(anchor, menu);
  (items.find((n) => n.dataset.value === String(current)) || items[0])?.focus();
  return menu;
}

// A small panel anchored the same way, holding any content (Online's players and lobbies, spec 2026-10-02 §2, §3.1).
// Tab and the D-pad move inside it (the shell treats an open dialog as its own scope); B, Esc or a click outside close
// it, and focus goes back to the button.
export function openPanel(anchor, { label, build }) {
  openNow?.close();
  const menu = el("dialog", { class: "menu panel", "aria-label": label }, build());
  attach(anchor, menu);
  menu.querySelector("a[href],button:not(:disabled),input,select,textarea")?.focus();
  return menu;
}

function attach(anchor, menu) {
  // A click on the backdrop, outside the list, closes it.
  menu.addEventListener("click", (e) => {
    if (e.target !== menu) return;
    const r = menu.getBoundingClientRect();
    if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) menu.close();
  });
  menu.addEventListener("close", () => {
    menu.remove();
    if (openNow === menu) openNow = null;
    // `close` is queued, so it can arrive after another menu has opened, even one on this same
    // anchor (a redraw reopening it by id, ruling P2-18). That menu owns the anchor now: leave it be.
    if (openNow && openNow !== menu) return;
    anchor.setAttribute("aria-expanded", "false");
    // The page may have redrawn while the menu was open: go back to the button that replaced this one.
    const back = anchor.isConnected ? anchor : anchor.id ? document.getElementById(anchor.id) : null;
    back?.setAttribute("aria-expanded", "false");
    back?.focus({ preventScroll: true });
  }, { once: true });
  anchor.setAttribute("aria-expanded", "true");
  document.body.append(menu);
  menu.showModal();
  place(anchor, menu);
  openNow = menu;
}
```

In `theme.css`, after the rule `dialog.menu .menu-item[aria-checked="true"] .menu-check{visibility:visible}` add:

```css
/* menu.js openPanel(): the same anchored glass surface, wider, holding any content (Online's players and lobbies) */
dialog.menu.panel{width:280px;padding:12px}
```

In `DESIGN.md` §6, change the `dialog.menu` row to:

```markdown
| `dialog.menu` | anchored glass list in the top layer, check on the current option, D-pad inside it, B closes; `dialog.menu.panel` (280px, `openPanel()`) holds any content the same way (Online's players and lobbies) | `menu.js` |
```

- [ ] **Step 4: The heading's players button in `online-mm.js`**

Add `import { openPanel } from './menu.js';` to the imports. Delete the functions `drawOnline` and `presenceToggle`. Add after `heldBox`:

```js
// ---- players online and the presence switch, in the heading (spec §2) ----
const playersBtn = document.getElementById('on-players');
let lastOnline = { count: null, players: [] };
let presenceOn = false;
const ACTIVITY = { idle: 'Online', queued: 'Looking for a game', in_lobby: 'In a room', in_match: 'In a match' };

/** The heading button: "{n} players online" once the server counted, "Players online" before it has. */
function showOnline(snap, on) {
  presenceOn = !!on;
  const online = snap?.online;
  lastOnline = { count: typeof online?.count === 'number' ? online.count : null, players: Array.isArray(online?.players) ? online.players : [] };
  if (!playersBtn) return;
  const n = lastOnline.count;
  playersBtn.querySelector('span').textContent = n === null ? 'Players online' : `${n} ${n === 1 ? 'player' : 'players'} online`;
  playersBtn.hidden = false;
}

// Who is online (the first 12, as before) and the switch that sat at the bottom of the panel, unchanged: it sends
// presence_set_enabled exactly as the old checkbox did (ruling 4 keeps its words).
function presencePanel() {
  const list = el('ul', { class: 'on-who' });
  for (const p of lastOnline.players.slice(0, 12)) {
    list.append(el('li', {}, el('span', { text: String(p?.name ?? '') }), ACTIVITY[p?.activity] ? el('small', { text: ACTIVITY[p.activity] }) : null));
  }
  const sw = el('button', { type: 'button', class: 'look-switch', id: 'mm-presence', role: 'switch', 'aria-checked': String(presenceOn), 'aria-labelledby': 'mm-presence-label' });
  sw.addEventListener('click', async () => {
    const next = sw.getAttribute('aria-checked') !== 'true';
    if (await act('presence_set_enabled', { enabled: next })) sw.setAttribute('aria-checked', String(next));
  });
  return el('div', { class: 'on-panel' },
    lastOnline.count !== null ? el('span', { class: 'k-label', text: `${lastOnline.count} online` }) : null,
    list,
    el('div', { class: 'on-switch-row' }, el('span', { id: 'mm-presence-label', text: "Show what I'm doing on Discord" }), sw));
}
playersBtn?.addEventListener('click', () => openPanel(playersBtn, { label: 'Players online', build: presencePanel }));
```

In `draw()`, change the last two lines

```js
  setLinks(showLinks);
  section.replaceChildren(...body, status, drawOnline(snap), presenceToggle(state.presence_on));
```

to

```js
  setLinks(showLinks);
  showOnline(snap, state.presence_on);
  section.replaceChildren(...body, status);
```

- [ ] **Step 5: Append the panel rules to `pages/online.css`**

```css
/* the players and lobbies panels (menu.js openPanel) */
.on-panel{display:flex;flex-direction:column;gap:10px}
.on-who,.on-lobbies{list-style:none;margin:0;padding:0;max-height:260px;overflow:auto}
.on-who li,.on-lobbies li{display:flex;align-items:center;justify-content:space-between;gap:8px;min-height:32px;font-size:13px}
.on-who li>span,.on-lobbies li>span{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.on-who small{flex:none;font-size:12px;color:var(--fg-3)}
.on-switch-row{display:flex;align-items:center;justify-content:space-between;gap:12px;padding-top:10px;border-top:1px solid var(--stroke);font-size:13px;color:var(--fg-2)}
```

- [ ] **Step 6: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs; node Tools/check-menu.mjs; node Tools/check-theme.mjs; node Tools/check-theme-tokens.mjs; node Tools/check-play-page.mjs`
Expected: all PASS (`check-online-page: PASS (19 state(s))`; check-menu and check-play-page prove `openMenu` behaves as before).

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/menu.js crates/reborn-ui/ui/theme.css crates/reborn-ui/ui/DESIGN.md crates/reborn-ui/ui/online-mm.js crates/reborn-ui/ui/pages/online.css Tools/lib/page-states.mjs Tools/check-online-page.mjs Tools/check-online-mm.mjs
git commit -m "online: players online and the presence switch move to the heading (menu.js openPanel)"
```

---

### Task 9: Public lobbies behind "Open lobbies (n) ›", and lobby hosts that decode

**Files:**
- Modify: `crates/reborn-core/src/matchmaking.rs` (`RoomSummary.host`, a `host_name` deserializer, a test)
- Modify: `crates/reborn-ui/ui/online-mm.js` (`loadLobbies`, `lobbyPanel`, `drawIdle`)
- Modify: `Tools/lib/page-states.mjs` (two states), `Tools/check-online-page.mjs` (two entries)

**Interfaces:**
- Consumes: `openPanel` (Task 8); `act`, `mode`, `privateRow`, `findRow`, `tiles`, `picked` (online-mm.js); `#lobbies-open` (Task 2). Task 8's `.on-lobbies` CSS.
- Produces: `mm_rooms` answers `[{ code, mode, best_of, host: string|null }]` even when the server sends `host: { id, display_name }`. DOM `data-key`: `open-public`.

- [ ] **Step 1: Write the failing Rust test**

In `crates/reborn-core/src/matchmaking.rs`'s tests add:

```rust
    #[test]
    fn a_lobby_hosts_name_reads_from_either_shape() {
        // rooms.js sends `host: { id, display_name }`; a plain name, null or nothing must read too. Before this, one
        // public lobby made the whole list fail to decode, so the lobby list showed a protocol error.
        let rooms: Rooms = serde_json::from_value(serde_json::json!({ "rooms": [
            { "code": "K7QP2M", "mode": "casual", "host": { "id": "h1", "display_name": "Jock" } },
            { "code": "R4NK3D", "mode": "casual", "host": "Tuna" },
            { "code": "X1", "mode": "casual", "host": null },
            { "code": "X2", "mode": "casual" } ] })).unwrap();
        let hosts: Vec<Option<&str>> = rooms.rooms.iter().map(|r| r.host.as_deref()).collect();
        assert_eq!(hosts, vec![Some("Jock"), Some("Tuna"), None, None]);
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p reborn-core a_lobby_hosts_name`
Expected: FAIL with `invalid type: map, expected a string`.

- [ ] **Step 3: Decode either shape**

In `matchmaking.rs`, change `RoomSummary`'s `pub host: Option<String>,` to:

```rust
    #[serde(deserialize_with = "host_name")]
    pub host: Option<String>,
```

and add right below the `RoomSummary` struct:

```rust
/// The lobby browser's `host`: the server sends `{ id, display_name }` (rooms.js), an older one a plain name. Either
/// reads as the name; anything else as no name.
fn host_name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Object(o)) => o.get("display_name").and_then(|n| n.as_str()).map(str::to_string),
        _ => None,
    })
}
```

Run: `cargo test -p reborn-core matchmaking; git diff --check`
Expected: PASS; no output from `git diff --check`.

- [ ] **Step 4: Add the states and failing expectations**

In `Tools/lib/page-states.mjs` above the redesign marker add:

```js
    N('online-lobbies', { mm_state: mm(empty), mm_rooms: rooms }, { click: '[data-key="mode-private"]' }),
    N('online-lobbies-none', { mm_state: mm(empty), mm_rooms: [] }, { click: '[data-key="mode-private"]' }),
```

In `Tools/check-online-page.mjs` above the states marker add:

```js
  'online-lobbies': {
    text: ['Open lobbies (2) ›'],
    check: async (page) => {
      const p = [];
      await page.locator('#lobbies-open').click();
      const panel = page.locator('dialog.menu.panel[open]');
      const t = await panel.innerText().catch(() => '');
      for (const w of ['Jock · Casual', 'Tuna · Ranked', 'Open a public lobby']) if (!t.includes(w)) p.push(`the lobby panel lacks "${w}"`);
      if (/best of/i.test(t)) p.push('a lobby is labelled best of');
      await panel.getByRole('button', { name: 'Join', exact: true }).first().click();
      await page.waitForTimeout(300);
      const j = await calls(page, 'mm_room_join');
      if (JSON.stringify(j[0]) !== '{"code":"K7QP2M"}') p.push(`Join sent ${JSON.stringify(j[0])}`);
      return p;
    },
  },
  'online-lobbies-none': {
    text: ['Open a public lobby ›'],
    check: async (page) => {
      const p = [];
      await page.locator('#lobbies-open').click();
      const panel = page.locator('dialog.menu.panel[open]');
      if (!(await panel.innerText().catch(() => '')).includes('No open lobbies. Open one and someone can join you.')) p.push('no "No open lobbies" line');
      await panel.getByRole('button', { name: 'Open a public lobby', exact: true }).click();
      await page.waitForTimeout(300);
      const c = await calls(page, 'mm_room_create');
      if (JSON.stringify(c[0]) !== '{"mode":"casual","public":true}') p.push(`Open a public lobby sent ${JSON.stringify(c[0])}`);
      return p;
    },
  },
```

- [ ] **Step 5: Run them to see them fail**

Run: `node Tools/check-online-page.mjs`
Expected: `FAIL online-lobbies: missing text "Open lobbies (2) ›"…`, `FAIL online-lobbies-none: …`.

- [ ] **Step 6: The lobby link and panel in `online-mm.js`**

Replace the whole old `async function loadLobbies(box) {…}` with:

```js
// Public lobbies (spec §3.1): read on every idle draw, as always. Under the Private room row, "Open lobbies (n) ›"
// opens them in a panel with "Open a public lobby"; with none it reads "Open a public lobby ›" (ruling 8).
let lobbies = [];
let lobbyError = null;
const lobbiesLink = document.getElementById('lobbies-open');
const closePanel = () => document.querySelector('dialog.menu.panel[open]')?.close();
async function loadLobbies() {
  try {
    const rooms = await call('mm_rooms');
    lobbies = Array.isArray(rooms) ? rooms : [];
    lobbyError = null;
  } catch (e) {
    lobbies = [];
    lobbyError = reason(e);
  }
  if (lobbiesLink) {
    lobbiesLink.textContent = lobbies.length ? `Open lobbies (${lobbies.length}) ›` : 'Open a public lobby ›';
    lobbiesLink.hidden = false;
  }
}
function lobbyPanel() {
  const list = el('ul', { class: 'on-lobbies' });
  if (lobbyError) list.append(el('li', { class: 'note', text: lobbyError }));
  else if (!lobbies.length) list.append(el('li', { class: 'note', text: 'No open lobbies. Open one and someone can join you.' }));
  for (const r of lobbies) {
    list.append(el('li', {},
      el('span', { text: `${r.host ?? 'A player'} · ${r.mode === 'ranked' ? 'Ranked' : 'Casual'}` }),
      el('button', { type: 'button', class: 'btn sm', text: 'Join', onclick: () => { closePanel(); act('mm_room_join', { code: r.code }); } })));
  }
  return el('div', { class: 'on-panel' },
    el('span', { class: 'k-label', text: 'Public lobbies' }),
    list,
    el('button', { type: 'button', class: 'btn sm', 'data-key': 'open-public', text: 'Open a public lobby',
      onclick: () => { closePanel(); act('mm_room_create', { mode: mode(), public: true }); } }));
}
lobbiesLink?.addEventListener('click', () => openPanel(lobbiesLink, { label: 'Public lobbies', build: lobbyPanel }));
```

Replace `drawIdle` with:

```js
function drawIdle(snap) {
  const p = picked();
  loadLobbies(); // the same `mm_rooms` read on every idle draw as before the redesign
  return [tiles(snap), p === 'private' ? privateRow() : findRow(p)];
}
```

Delete the `.mm-lobbies h3`, `.mm-room-list` and `.mm-actions` rules from `pages/online.css` (nothing draws them any more).

- [ ] **Step 7: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs`
Expected: `check-online-page: PASS (21 state(s))`; check-online-mm PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/reborn-core/src/matchmaking.rs crates/reborn-ui/ui/online-mm.js crates/reborn-ui/ui/pages/online.css Tools/lib/page-states.mjs Tools/check-online-page.mjs
git commit -m "online: public lobbies in a panel under Private room; lobby hosts decode from either shape"
```

---

### Task 10: Live now cards, "See all ›" and the live-games count

**Files:**
- Modify: `crates/reborn-ui/ui/live-now.js` (imports, card markup, heading, count, three cards)
- Modify: `crates/reborn-ui/src/shell_tools.rs` (`allowed`, a test)
- Modify: `crates/reborn-ui/ui/pages/online.css` (append), `crates/reborn-ui/ui/DESIGN.md` (§9 live card words)
- Modify: `Tools/check-live-now.mjs` (sections 1, 5, 8 and the final line), `Tools/lib/page-states.mjs` (two states), `Tools/check-online-page.mjs` (two entries)

**Interfaces:**
- Consumes: `#live-slot`, `#on-live-count` (Task 2); `PAGE_ORIGIN`, `assetUrl`, `makePoller` (live-common.js).
- Produces: `open_website` accepts exactly `https://online.nflstreetreborn.com/live` and `https://online.nflstreetreborn.com/leaderboard` (Task 11 uses the second). Card markup: `li.live-row.glass` > `div.live-side[style=--tc:#rrggbb](.lose)` ×2 > (`span.live-image` > `img.live-logo` + `span.live-badge`) or `span.live-badge`, `span.live-team-name` > `em.live-player-name`, `b.live-score-num`; then `div.live-foot` > `span.live-dot`, `span.live-mode-line`, `a.btn.sm.live-watch`.

- [ ] **Step 1: Write the failing Rust test**

In `crates/reborn-ui/src/shell_tools.rs`'s tests, after `game_and_profile_pages_open_on_both_origins`, add:

```rust
    #[test]
    fn the_live_index_and_the_leaderboard_open_exactly() {
        let o = reborn_core::identity::PAGE_ORIGIN;
        assert!(allowed(&format!("{o}/live")), "Live now's See all");
        assert!(allowed(&format!("{o}/leaderboard")), "Full leaderboard");
        for bad in [format!("{o}/live/"), format!("{o}/live?x=1"), format!("{o}/leaderboard#a"), format!("{o}/leaderboards"),
            format!("{o}/leaderboard/../admin"), format!("{}/leaderboard", reborn_core::identity::DEFAULT_ORIGIN)] {
            assert!(!allowed(&bad), "{bad}");
        }
    }
```

Run: `cargo test -p reborn-ui the_live_index_and_the_leaderboard`
Expected: FAIL on `Live now's See all`.

- [ ] **Step 2: Allow the two pages**

In `shell_tools.rs` `fn allowed`, replace the body with:

```rust
    if url.starts_with(SITE) {
        return true;
    }
    use reborn_core::identity::{DEFAULT_ORIGIN, PAGE_ORIGIN};
    // Two fixed pages on the page origin, exactly (spec 2026-10-02 §4, §5): the live index ("See all") and the
    // leaderboard ("Full leaderboard"). Nothing after them: no slash, query or fragment.
    if [format!("{PAGE_ORIGIN}/live"), format!("{PAGE_ORIGIN}/leaderboard")].iter().any(|u| url == u) {
        return true;
    }
    [DEFAULT_ORIGIN, PAGE_ORIGIN].iter().any(|origin| {
        ["live", "games", "u"].iter().any(|kind| {
            url.strip_prefix(&format!("{origin}/{kind}/"))
                .is_some_and(reborn_core::live_view::is_canonical_uuid)
        })
    })
```

and add this line to the end of the doc comment above `fn allowed`: `/// Also exactly two fixed pages on the page origin, `/live` and `/leaderboard` (the Online page's links).`

Run: `cargo test -p reborn-ui shell_tools; git diff --check`
Expected: PASS; nothing from `git diff --check`.

- [ ] **Step 3: Update `check-live-now.mjs` for the card, and add the page states**

In `Tools/check-live-now.mjs` section 1, change

```js
  check("shows the game numbering for both matches", t.includes("Game 1 of 3") && t.includes("Game 1 of 1"), t.slice(0, 400));
```

to

```js
  check("shows the mode line for both matches (spec 2026-10-02 §5)", t.includes("Ranked · Game 1") && t.includes("Casual"), t.slice(0, 400));
```

and replace the lines from `  const abbrevs = byClass(section, "live-abbrev");` through the end of the `check("the abbreviation chips clear 4.5:1 contrast", …);` statement with:

```js
  const sides = byClass(section, "live-side");
  check("each side's fade is its own team's validated primary colour (--tc)",
    sides.length === 2 * FIXTURE.matches.length && sides.every((s) => /^--tc:#[0-9a-fA-F]{6}$/.test(s.attrs.style || "")), sides.map((s) => s.attrs.style).join(", "));
```

In section 5 change

```js
  check("badges are drawn instead", byClass(section, "live-badge").length === 2 && byClass(section, "live-avatar-badge").length === 2);
```

to

```js
  check("badges are drawn instead (the card draws no avatars)", byClass(section, "live-badge").length === 2 && imgsOf(section).length === 0);
```

Replace the whole section that starts with the comment `// ---- 8: the abbreviation chip's background is the team's own primary; its text is chosen by luminance ----` (through the `}` closing its block, just before `// ---- 9 (I1)`) with:

```js
// ---- 8: a team's primary becomes its side's fade (--tc) only when it is a valid #rrggbb ----
{
  const match = {
    id: "33333333-3333-4333-8333-333333333333", mode: "ranked", best_of: 1, series: [0, 0], started_at: "2026-09-24T00:00:00Z",
    seats: [{ display_name: "A", avatar_url: null }, { display_name: "B", avatar_url: null }],
    game_seq: 1, score: [7, 3], sides: [{ seat: 0 }, { seat: 1 }],
    teams: [
      { side: 0, tgid: 1, name: "Navy", abbrev: "NVY", city: null, primary: "#0E172E", secondary: "#000000", logo_url: null, wordmark_url: null },
      { side: 1, tgid: 2, name: "Odd", abbrev: "ODD", city: null, primary: "red;background:url(x)", secondary: "#000000", logo_url: null, wordmark_url: null },
    ],
  };
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [match] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const [navy, odd] = byClass(page.section(), "live-side");
  check("a valid primary becomes the side's --tc", !!navy && navy.attrs.style === "--tc:#0E172E", navy && navy.attrs.style);
  check("anything else sets no style at all", !!odd && odd.attrs.style === undefined, odd && odd.attrs.style);
  check("the losing side is dimmed", !!odd && String(odd.className).split(/\s+/).includes("lose") && !String(navy.className).split(/\s+/).includes("lose"));
  page.done();
}
```

Change the last PASS line's ending `…and gives each team's abbreviation a readable chip (its own colour as background, black or white text by luminance)."` to `…and fades each side in its own validated team colour."`.

In `Tools/lib/page-states.mjs` above the redesign marker add:

```js
    N('online-live', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-live-many', { mm_state: mm(empty), live_matches: [0, 1, 2, 3, 4].map((i) => ({ ...LIST.matches[i % 2], id: `b41cf41a-43c4-4d1b-aaa8-05e4e72c73${String(i).padStart(2, '0')}` })) }, { init: PICK('ranked') }),
```

In `Tools/check-online-page.mjs` above the states marker add:

```js
  'online-live': {
    text: ['Live now', 'See all', 'Ranked · Game 1', 'Casual', 'Watch', 'Giants', 'Colts', 'Adam', 'Jock', '2 live games'],
    absent: ['Game 1 of 3'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.live-row').count()) !== 2) p.push(`${await page.locator('.live-row').count()} live cards`);
      if ((await page.locator('.live-row').first().locator('.live-side').count()) !== 2) p.push('a card does not have two team rows');
      const href = await page.locator('.live-watch').first().getAttribute('href');
      if (!/^live\.html\?match=/.test(href || '')) p.push(`Watch goes to ${href}`);
      const live = await page.locator('#live-slot').boundingBox(), mode = await page.locator('#mode-slot').boundingBox();
      if (!live || !mode || live.y < mode.y + mode.height) p.push('Live now is not under the mode area');
      await page.getByRole('button', { name: 'See all ›', exact: true }).click();
      await page.waitForTimeout(200);
      const o = await calls(page, 'open_website');
      if (JSON.stringify(o[0]) !== '{"url":"https://online.nflstreetreborn.com/live"}') p.push(`See all opened ${JSON.stringify(o[0])}`);
      return p;
    },
  },
  'online-live-many': {
    text: ['5 live games'],
    check: async (page) => ((await page.locator('.live-row').count()) === 3 ? [] : [`${await page.locator('.live-row').count()} cards, not 3`]),
  },
```

- [ ] **Step 4: Run them to see them fail**

Run: `node Tools/check-live-now.mjs; node Tools/check-online-page.mjs`
Expected: check-live-now fails `shows the mode line…`, `each side's fade…` and section 8; check-online-page `FAIL online-live…`, `FAIL online-live-many…`.

- [ ] **Step 5: Rewrite the card in `live-now.js`**

Change the three import lines to:

```js
import { bridge, call, el, quietCall, reason } from './shell.js';
import { PAGE_ORIGIN, assetUrl, makePoller } from './live-common.js';
```

(`inkFor` and `avatarSrc` are no longer used here: the card has no colour chip and no avatars.)

Delete the functions `initialOf`, `teamAbbrev`, `teamBlock` and `playerAvatar`. Replace `playerLine` with:

```js
/** The player under a team, muted beside its name; a side with no controller is the CPU. */
function playerLine(seat, cpu) {
  if (cpu) return el('em', { class: 'live-player-name live-cpu', text: 'CPU' });
  const name = seat && typeof seat.display_name === 'string' && seat.display_name ? seat.display_name : 'Unknown player';
  return el('em', { class: 'live-player-name', text: name });
}

/** One team row of a card (spec 2026-10-02 §5): crest (or its badge), team name with the player muted, score, on a faint
    fade of the team's own primary (validated, never trusted as is). The losing side is dimmed. */
function sideRow(team, score, seat, cpu, losing) {
  const attrs = { class: `live-side${losing ? ' lose' : ''}` };
  if (team && HEX_RE.test(team.primary)) attrs.style = `--tc:${team.primary}`;
  return el('div', attrs,
    teamLogo(team),
    el('span', { class: 'live-team-name' }, team && team.name ? String(team.name) : 'Team TBD', ' ', playerLine(seat, cpu)),
    el('b', { class: 'live-score-num', text: String(Number.isFinite(score) ? score : 0) }));
}
```

In `watchLink`, change `class: 'button live-watch'` to `class: 'btn sm live-watch'`.

Replace `rowContent` with:

```js
/** The card's whole content, rebuilt fresh every poll; only the <li> that holds it is reused. */
function rowContent(match) {
  const teams = Array.isArray(match.teams) ? match.teams : [null, null];
  const score = Array.isArray(match.score) ? match.score : [0, 0];
  const seats = Array.isArray(match.seats) ? match.seats : [null, null];
  // Each player sits under the team they have (side 0 first, then side 1); a side with no seat shows CPU rather than
  // the other side's player.
  const sides = Array.isArray(match.sides) && match.sides.length ? match.sides : null;
  const s0 = seatOfSide(seats, sides, 0);
  const s1 = seatOfSide(seats, sides, 1);
  const a = Number.isFinite(score[0]) ? score[0] : 0, b = Number.isFinite(score[1]) ? score[1] : 0;
  const gameNo = Number.isInteger(match.game_seq) ? match.game_seq : 1;
  return [
    sideRow(teams[0], score[0], s0.seat, s0.cpu, a < b),
    sideRow(teams[1], score[1], s1.seat, s1.cpu, b < a),
    el('div', { class: 'live-foot' },
      el('span', { class: 'live-dot', 'aria-hidden': 'true' }),
      el('span', { class: 'live-mode-line', text: match.mode === 'ranked' ? `Ranked · Game ${gameNo}` : 'Casual' }),
      watchLink(match)),
  ];
}
```

In `rowFor`, change `li = el('li', { class: 'live-row', 'data-match-id': id });` to `li = el('li', { class: 'live-row glass', 'data-match-id': id });`.

Inside `if (bridge) {`, change

```js
  section.append(el('div', { class: 'live-now-heading' }, el('h2', { text: 'Live now' })), body);
```

to

```js
  section.append(el('div', { class: 'sec-head live-now-heading' }, el('h2', { text: 'Live now' }),
    el('button', { type: 'button', class: 'sec-link', 'data-key': 'live-all', text: 'See all ›',
      onclick: () => call('open_website', { url: `${PAGE_ORIGIN}/live` }).catch(() => {}) })), body);
  // The heading's "● {n} live games" (spec §2); hidden until the first good answer.
  const countBox = document.getElementById('on-live-count');
  function setCount(n) {
    const words = countBox?.querySelector('span');
    if (!words) return;
    words.textContent = `${n} live ${n === 1 ? 'game' : 'games'}`;
    countBox.hidden = false;
  }
```

In `showMatches`, change

```js
    lastJson = json;
    if (matches.length) { drawRows(matches); return; }
```

to

```js
    lastJson = json;
    setCount(matches.length);
    // Up to three cards (spec §5); "See all ›" opens the rest on the web.
    if (matches.length) { drawRows(matches.slice(0, 3)); return; }
```

(Polling, backoff, the error line, row reuse and Watch focus are untouched.)

- [ ] **Step 6: Append the card rules to `pages/online.css`, and fix DESIGN.md §9**

```css
/* Live now (spec §5): up to three cards */
.live-now .sec-head{margin-bottom:8px}
.live-now-note:empty{display:none}
.live-rows{list-style:none;margin:0;padding:0;display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:10px}
.live-row{display:flex;flex-direction:column;overflow:hidden}
.live-side{position:relative;display:grid;grid-template-columns:34px minmax(0,1fr) auto;align-items:center;gap:10px;height:40px;padding:0 14px 0 8px;isolation:isolate}
.live-side::before{content:"";position:absolute;inset:0;z-index:-1;background:linear-gradient(90deg,var(--tc,transparent) 0,transparent 70%);opacity:.3}
.live-image{display:contents}
.live-side .live-logo{justify-self:center;width:26px;height:26px;object-fit:contain}
.live-side .live-badge{justify-self:center;display:inline-grid;place-items:center;width:26px;height:26px;overflow:hidden;border-radius:50%;background:var(--glass-2);font:600 9px Inter,system-ui,sans-serif;color:var(--fg-2)}
.live-team-name{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:13px;font-weight:600}
.live-player-name{font-style:normal;font-weight:500;font-size:12px;color:var(--fg-3)}
.live-score-num{font:700 20px Inter,system-ui,sans-serif;font-variant-numeric:tabular-nums}
.live-side.lose .live-team-name,.live-side.lose .live-score-num{color:var(--fg-2)}
.live-foot{display:flex;align-items:center;gap:8px;padding:7px 10px 7px 14px;border-top:1px solid var(--stroke);font-size:11.5px;color:var(--fg-3)}
.live-mode-line{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.btn.sm.live-watch{height:26px;padding:0 10px;font-size:11.5px}
.live-empty{margin:0;font-size:13px;color:var(--fg-2)}
.live-error{margin:0 0 8px}
@media (max-width:1000px){.live-rows{grid-template-columns:1fr}}
```

In `DESIGN.md` §9 change `- A row never says "Best of 3" (every ranked match is one). The live card says "Game 2 of 3". There are no halves.` to:

```markdown
- A row never says "Best of 3" (every ranked match is one). The match view (live.html) says "Game 2 of 3"; Online's Live now card says "Ranked · Game 2" or "Casual" (spec 2026-10-02 §5). There are no halves.
```

- [ ] **Step 7: Run the checks**

Run: `node Tools/check-live-now.mjs; node Tools/check-online-page.mjs; node Tools/check-theme-tokens.mjs; cargo test -p reborn-ui live_view`
Expected: check-live-now ends with `PASS: Live now renders …`; `check-online-page: PASS (23 state(s))`; tokens PASS; the `live_view` Rust tests (CSP, shared origin, no raw error text) PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/reborn-ui/ui/live-now.js crates/reborn-ui/src/shell_tools.rs crates/reborn-ui/ui/pages/online.css crates/reborn-ui/ui/DESIGN.md Tools/check-live-now.mjs Tools/lib/page-states.mjs Tools/check-online-page.mjs
git commit -m "online: Live now as three glass cards, See all, the live-games count; open_website allows /live and /leaderboard"
```

---

### Task 11: The Leaderboard card, the `leaderboard` command, and your rank on the Ranked tile

**Files:**
- Modify: `crates/reborn-core/src/live_view.rs` (`leaderboard()` + test)
- Modify: `crates/reborn-ui/src/live_view.rs` (`leaderboard` command), `crates/reborn-ui/src/main.rs` (register it)
- Create: `crates/reborn-ui/ui/leaderboard.js`
- Modify: `crates/reborn-ui/ui/online.html` (one script tag)
- Modify: `crates/reborn-ui/ui/pages/online.css` (append)
- Modify: `Tools/lib/page-states.mjs` (five states), `Tools/check-online-page.mjs` (five entries; three existing entries' text)

**Interfaces:**
- Consumes: `leaderboardView` (online-model.js); `avatar`, `teamLogo` (match-rows.js); `PAGE_ORIGIN`, `makePoller` (live-common.js); `#leaderboard-slot` (Task 2); `open_website` allowing `/leaderboard` (Task 10).
- Produces: Tauri command `leaderboard() -> Result<Option<Value>, LiveCommandError>` (`GET /v1/leaderboard?limit=10` with the bearer when signed in; `None` on 404). Core `reborn_core::live_view::leaderboard(http: &dyn Http, origin: &str, token: Option<&str>, limit: u32) -> Result<Option<Value>, LiveError>` (limit clamped to 1..=50). Event `online-rank` on `document`, `detail: { elo: object|null, playsAs: object|null } | null`, dispatched after every `stats_dashboard` read (online-mm.js listens since Task 4). DOM `data-key`: `full-leaderboard`.

- [ ] **Step 1: Write the failing Rust test**

In `crates/reborn-core/src/live_view.rs`'s tests add:

```rust
    #[test]
    fn leaderboard_reads_the_board_sends_the_token_when_there_is_one_and_404_is_none() {
        let url = "https://api.test/v1/leaderboard?limit=10";
        let http = FakeHttp::default().reply(url, 200, r#"{"ok":true,"enabled":true,"total":2,"rows":[{"rank":1}],"you":null}"#);
        let got = leaderboard(&http, ORIGIN, Some("tok"), 10).unwrap().unwrap();
        assert_eq!(got["total"], 2);
        assert_eq!(http.last_bearer().as_deref(), Some("tok"));
        // No such route yet (an older backend, matchmaking off): nothing to show, not an error.
        let none = FakeHttp::default().reply(url, 404, "");
        assert_eq!(leaderboard(&none, ORIGIN, None, 10).unwrap(), None);
        let clamped = FakeHttp::default().reply("https://api.test/v1/leaderboard?limit=50", 200, "{}");
        assert!(leaderboard(&clamped, ORIGIN, None, 500).unwrap().is_some(), "a silly limit is clamped, not refused");
        let broken = FakeHttp::default().reply(url, 500, "");
        assert_eq!(leaderboard(&broken, ORIGIN, None, 10).unwrap_err(), LiveError::Server(500));
    }
```

Run: `cargo test -p reborn-core leaderboard_reads_the_board`
Expected: FAIL, `cannot find function 'leaderboard'`.

- [ ] **Step 2: The client and the command**

In `crates/reborn-core/src/live_view.rs`, after `pub fn dashboard(…) {…}` add:

```rust
/// `GET {origin}/v1/leaderboard?limit={limit}`: the ranked leaderboard's top rows (the Online page's card, launcher
/// spec 2026-10-02 §4) and, with a token, the viewer's own row. `limit` is clamped to `1..=50`. A `404` is `Ok(None)`:
/// a backend without the route (or with matchmaking off) has no leaderboard to show, which the card reads as
/// "Rankings start soon."
pub fn leaderboard(http: &dyn Http, origin: &str, token: Option<&str>, limit: u32) -> Result<Option<Value>, LiveError> {
    let url = format!("{}/v1/leaderboard?limit={}", origin.trim_end_matches('/'), limit.clamp(1, 50));
    get_json_opt(http, &url, token)
}
```

In `crates/reborn-ui/src/live_view.rs`, after `stats_dashboard` add:

```rust
/// The Online page's Leaderboard card (spec 2026-10-02 §4): the top ten, with the player's token when signed in so the
/// server can add their own row. `None` while the server has no leaderboard. Signed out is simply anonymous.
#[tauri::command]
pub async fn leaderboard() -> Result<Option<serde_json::Value>, LiveCommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Platform::detect();
        let signed_in = identity::status(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?.is_some();
        let token = if signed_in { identity::token(&p).unwrap_or(None) } else { None };
        let origin = identity::origin(&p).map_err(|e| LiveCommandError::unavailable(e.player_message()))?;
        reborn_core::live_view::leaderboard(&crate::http_client(), &origin, token.as_deref(), 10).map_err(LiveCommandError::from)
    })
    .await
    .map_err(|_| LiveCommandError::unavailable(NOT_LOADED))?
}
```

In `crates/reborn-ui/src/main.rs`'s `generate_handler![…]`, after `live_view::set_plays_as,` add `live_view::leaderboard,`.

Run: `cargo test -p reborn-core live_view; cargo test -p reborn-ui live_view; git diff --check`
Expected: PASS (the reborn-ui test `no_command_here_hands_a_page_raw_error_text` still passes: the new command maps every error to a player sentence); no whitespace output.

- [ ] **Step 3: Add the states and failing expectations**

In `Tools/lib/page-states.mjs` above the redesign marker add:

```js
    N('online-leaderboard', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-leaderboard-far', { mm_state: mm(empty), leaderboard: { ...LEADERBOARD, rows: LEADERBOARD.rows.map((r) => ({ ...r, you: false })),
      you: { ...LEADERBOARD.rows[6], rank: 14, elo: 1104, you: true } } }, { init: PICK('ranked') }),
    N('online-elo-off', { mm_state: mm(empty), leaderboard: { ok: true, enabled: false, total: 0, rows: [], you: null },
      stats_dashboard: (() => { const d = F.dashboard(); d.you.elo = { rating: null, rank: null, total: 0, games: 0, last_delta: null, peak: null, peak_at: null, history: [], around: [], week_delta: null }; return d; })() },
      { init: PICK('ranked') }),
    N('online-lb-missing', { mm_state: mm(empty), leaderboard: null }, { init: PICK('ranked') }),
    N('online-lb-hostile', { mm_state: mm(empty), leaderboard: { ok: true, enabled: true, total: 2, you: null, rows: [
      { rank: 1, player_id: 'h1', display_name: '<img src=x onerror=alert(1)>', avatar_url: 'https://evil.example/a.png', team: { name: 'X', abbrev: 'X', logo_url: 'https://evil.example/x.png' }, wins: 3, losses: 1, elo: 1200, delta_week: 0, you: false },
      { rank: 2, player_id: 'h2', display_name: 'Averyveryveryveryveryverylongdisplayname1234567890', avatar_url: null, team: null, wins: null, losses: null, elo: null, delta_week: null, you: false },
    ] } }, { init: PICK('ranked') }),
```

In `Tools/check-online-page.mjs`:
- in `'online-ranked'`, append `'#7', '1,184 Elo', '▲ 16 this week'` to its `text` array;
- in `'online-searching'`, change `'Looking for a ranked game'` to `'Looking for players near 1,184 Elo'`;
- in `'online-match-found'`, append `'#7 · 1,184 Elo'` to its `text` array and change the team-logo check line to `if ((await page.locator('.on-vs .team-logo').count()) !== 2) p.push('both plays-as team logos should show');`;

and above the states marker add:

```js
  'online-leaderboard': {
    text: ['Leaderboard', '38 ranked', 'Player', 'Elo', 'Jock', '20–3', '1,320', '▲ 16', '▼ 9', 'Full leaderboard ›'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-lb-row').count()) !== 10) p.push(`${await page.locator('.on-lb-row').count()} rows, not 10`);
      if ((await page.locator('.on-lb-row.top').count()) !== 3) p.push('the top three are not marked');
      const you = page.locator('.on-lb-row.you');
      if ((await you.count()) !== 1 || (await you.locator('.on-lb-rank').innerText()).trim() !== '7') p.push('your row (#7) is not marked');
      if (await page.locator('.on-lb-gap').count()) p.push('a gap row while you are in the top 10');
      const srcs = await page.locator('#leaderboard-slot img').evaluateAll((n) => n.map((x) => x.getAttribute('src')));
      for (const s of srcs) if (!/^https:\/\/(api\.nflstreetreborn\.com\/live-assets\/|cdn\.discordapp\.com\/avatars\/)/.test(s)) p.push(`image from ${s}`);
      const card = await page.locator('#leaderboard-slot').boundingBox(), main = await page.locator('#main').boundingBox();
      if (!card || card.y + card.height > main.y + main.height + 1) p.push('the leaderboard card runs off the window');
      await page.getByRole('button', { name: 'Full leaderboard ›', exact: true }).click();
      await page.waitForTimeout(200);
      const o = await calls(page, 'open_website');
      if (JSON.stringify(o.at(-1)) !== '{"url":"https://online.nflstreetreborn.com/leaderboard"}') p.push(`Full leaderboard opened ${JSON.stringify(o.at(-1))}`);
      return p;
    },
  },
  'online-leaderboard-far': {
    text: ['1,104'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-lb-gap').count()) !== 1) p.push('no gap before your row');
      const last = page.locator('.on-lb-row').last();
      if (!(await last.evaluate((n) => n.classList.contains('you')))) p.push('the last row is not yours');
      if ((await last.locator('.on-lb-rank').innerText()).trim() !== '14') p.push('your row does not say 14');
      return p;
    },
  },
  'online-elo-off': {
    text: ['Leaderboard', 'Rankings start soon.', 'Find a ranked game'],
    absent: ['Full leaderboard', '38 ranked', '1,184 Elo', '#7', 'this week', 'Looking for players near'],
  },
  'online-lb-missing': { text: ['Rankings start soon.'], absent: ['Full leaderboard'] },
  'online-lb-hostile': {
    text: ['<img src=x onerror=alert(1)>', 'Averyveryvery'],
    absent: ['undefined', 'NaN', 'null'],
    check: async (page) => {
      const p = [];
      if (await page.locator('#leaderboard-slot img[src="x"]').count()) p.push('a hostile name became an element');
      if (await page.locator('#leaderboard-slot img[src^="https://evil"]').count()) p.push('an image from outside the two allowed prefixes');
      const long = page.locator('.on-lb-name').nth(1);
      const box = await page.locator('.on-lb-row').nth(1).boundingBox();
      if (!box || box.height > 60) p.push(`the long-named row is ${box?.height}px tall`);
      if (!(await long.evaluate((n) => n.scrollWidth > n.clientWidth))) p.push('the long name is not cut with an ellipsis');
      return p;
    },
  },
```

- [ ] **Step 4: Run them to see them fail**

Run: `node Tools/check-online-page.mjs`
Expected: `FAIL online-ranked: missing text "#7"…`, `FAIL online-searching…`, `FAIL online-match-found…`, `FAIL online-leaderboard…` and the other four new states.

- [ ] **Step 5: Write `leaderboard.js`**

```js
// The Leaderboard card on the Online page (spec docs/superpowers/specs/2026-10-02-online-page-redesign-design.md §4, §7):
// the ranked top 10 and, when you are further down, your own row after a gap. `leaderboard` answers null while the
// server has no leaderboard (an older backend, matchmaking off) and `enabled: false` while Elo is off: the card then
// says "Rankings start soon." and nothing else. On the same 60 s beat it reads `stats_dashboard` and hands your rank,
// Elo and plays-as team to the mode area (online-mm.js) as an `online-rank` event.
// Server text only through el()'s `text`; images only through match-rows.js's avatar() and teamLogo() (the CSP's two
// image prefixes). Polling stops while the page is hidden or a game runs, like Live now.
import { bridge, call, el, quietCall, reason } from './shell.js';
import { avatar, teamLogo } from './match-rows.js';
import { PAGE_ORIGIN, makePoller } from './live-common.js';
import { leaderboardView } from './online-model.js';

const POLL_MS = 60000;
const slot = document.getElementById('leaderboard-slot');
const card = el('section', { class: 'glass on-lb', 'aria-label': 'Leaderboard' });
slot?.append(card);
const head = () => el('div', { class: 'sec-head' }, el('h2', { text: 'Leaderboard' }));

function row(r) {
  return el('li', { class: `on-lb-row${r.rank !== null && r.rank <= 3 ? ' top' : ''}${r.you ? ' you' : ''}` },
    el('span', { class: 'on-lb-rank', text: r.rank === null ? '' : String(r.rank) }),
    el('span', { class: 'on-lb-face' }, avatar(r.avatar, r.name), r.team ? teamLogo(r.team) : null),
    el('span', { class: 'on-lb-who' }, el('span', { class: 'on-lb-name', text: r.name }), r.record ? el('small', { class: 'num', text: r.record }) : null),
    el('span', { class: 'on-lb-elo' }, r.elo ? el('b', { text: r.elo }) : null, r.change ? el('small', { class: r.change.dir, text: r.change.text }) : null));
}

let lastJson;
function draw(answer) {
  const json = JSON.stringify(answer ?? null);
  if (json === lastJson) return;
  lastJson = json;
  const v = leaderboardView(answer);
  if (v.soon) { card.replaceChildren(head(), el('p', { class: 'on-lb-soon', text: 'Rankings start soon.' })); return; }
  const h = head();
  if (v.total !== null) h.append(el('span', { class: 'on-lb-total', text: `${v.total} ranked` }));
  const list = el('ol', { class: 'on-lb-rows' }, ...v.rows.map(row));
  if (v.you) list.append(el('li', { class: 'on-lb-gap', 'aria-hidden': 'true', text: '···' }), row(v.you));
  card.replaceChildren(h,
    el('div', { class: 'on-lb-hd', 'aria-hidden': 'true' }, el('span', { text: '#' }), el('span', { text: 'Player' }), el('span', { text: 'Elo' })),
    list,
    el('button', { type: 'button', class: 'sec-link on-lb-more', 'data-key': 'full-leaderboard', text: 'Full leaderboard ›',
      onclick: () => call('open_website', { url: `${PAGE_ORIGIN}/leaderboard` }).catch(() => {}) }));
}

/** A failed read: the last good card stays; with none yet, the header and the failure's own sentence. */
function showError(text) {
  if (lastJson !== undefined) return;
  card.replaceChildren(head(), el('p', { class: 'note on-lb-error', role: 'alert', text }));
}

async function poll() {
  const [board, dash] = await Promise.allSettled([quietCall('leaderboard'), quietCall('stats_dashboard')]);
  const d = dash.status === 'fulfilled' ? dash.value : null;
  const you = d && d.signed_in && d.ok !== false ? d.you : null;
  document.dispatchEvent(new CustomEvent('online-rank', { detail: you ? { elo: you.elo ?? null, playsAs: you.plays_as ?? null } : null }));
  if (board.status === 'rejected') { showError(reason(board.reason)); throw board.reason; }
  draw(board.value);
}

card.replaceChildren(head());
if (bridge) {
  const poller = makePoller(poll, POLL_MS);
  let sessionActive = false;
  const update = () => { if (document.visibilityState === 'hidden' || sessionActive) poller.stop(); else poller.start(); };
  document.addEventListener('visibilitychange', update);
  document.addEventListener('session-state', (e) => { sessionActive = Boolean(e?.detail); update(); });
  update();
} else {
  draw(null);
}
```

In `online.html`, change the last script line's end `<script type="module" src="live-now.js"></script>` to `<script type="module" src="live-now.js"></script><script type="module" src="leaderboard.js"></script>`.

- [ ] **Step 6: Append the card rules to `pages/online.css`**

```css
/* the Leaderboard card (spec §4): full height of the right column */
.on-lb{flex:1;display:flex;flex-direction:column;min-height:0;padding:16px 18px 14px}
.on-lb .sec-head{height:auto;margin-bottom:10px}
.on-lb-total{font-size:12px;color:var(--fg-3)}
.on-lb-hd,.on-lb-row{display:grid;grid-template-columns:26px 34px minmax(0,1fr) auto;align-items:center;gap:10px}
.on-lb-hd{padding:0 0 8px;font:600 11px Inter,system-ui,sans-serif;letter-spacing:.06em;text-transform:uppercase;color:var(--fg-3)}
.on-lb-hd span:nth-child(2){grid-column:2/4}
.on-lb-hd span:last-child{text-align:right}
.on-lb-rows{list-style:none;margin:0;padding:0;flex:1;display:flex;flex-direction:column;min-height:0}
.on-lb-row{flex:1;min-height:40px;max-height:60px;margin:0 -6px;padding:0 6px;border-radius:var(--r-sm);font-size:13px;color:var(--fg-2)}
.on-lb-row+.on-lb-row{border-top:1px solid var(--stroke)}
.on-lb-rank{text-align:center;font:600 13px Inter,system-ui,sans-serif;font-variant-numeric:tabular-nums;color:var(--fg-3)}
.on-lb-row.top .on-lb-rank{color:var(--fg)}
.on-lb-row.you{padding-left:3px;border-left:3px solid var(--red);background:var(--red-wash)}
.on-lb-row.you .on-lb-rank{color:var(--red-ink)}
.on-lb-row.you .on-lb-name{font-weight:700}
.on-lb-face{position:relative;width:32px;height:32px}
.on-lb-face .team-logo{position:absolute;right:-5px;bottom:-4px;width:18px;height:18px;padding:2px;border-radius:50%;background:var(--badge-bg);box-shadow:0 0 0 1px var(--stroke-hi)}
.on-lb-who{display:flex;flex-direction:column;min-width:0}
.on-lb-name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:500;color:var(--fg)}
.on-lb-who small{font-size:11.5px;color:var(--fg-3)}
.on-lb-elo{text-align:right}
.on-lb-elo b{font:700 14px Inter,system-ui,sans-serif;font-variant-numeric:tabular-nums;color:var(--fg)}
.on-lb-elo small{display:block;font-size:11px;font-weight:600}
.on-lb-elo .up{color:var(--ok)}
.on-lb-elo .down{color:var(--red-ink)}
.on-lb-gap{flex:none;height:22px;display:flex;align-items:center;justify-content:center;font-size:11px;letter-spacing:.3em;color:var(--fg-3)}
.on-lb-soon{margin:auto 0;text-align:center;color:var(--fg-2)}
.on-lb-more{align-self:flex-start;margin-top:10px}
```

(The badge's `0 0 0 1px var(--stroke-hi)` ring is the same neutral ring theme.css gives `.player-face>.badge`; no colour, no glow.)

- [ ] **Step 7: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs; node Tools/check-theme-tokens.mjs; node Tools/check-page-call-args.mjs`
Expected: `check-online-page: PASS (28 state(s))`; the others PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/reborn-core/src/live_view.rs crates/reborn-ui/src/live_view.rs crates/reborn-ui/src/main.rs crates/reborn-ui/ui/leaderboard.js crates/reborn-ui/ui/online.html crates/reborn-ui/ui/pages/online.css Tools/lib/page-states.mjs Tools/check-online-page.mjs
git commit -m "online: Leaderboard card (leaderboard command, Rankings start soon with Elo off), your rank and Elo on the Ranked tile"
```

---

### Task 12: Real invite links for rooms (`nflstreetreborn://room/<code>`)

**Files:**
- Modify: `crates/reborn-ui/src/main.rs` (`Mode`, `mode_from`, invite kinds, pending-invite file, two new commands, start-up and second-instance hand-off, registrations, tests)
- Modify: `crates/reborn-core/src/matchmaking.rs` (`RoomLookup`, `Client::room_lookup`, test)
- Modify: `crates/reborn-ui/src/online_mm.rs` (`mm_room_lookup` command)
- Modify: `crates/reborn-core/src/presence.rs` (`join_link`, test), `crates/reborn-core/src/presence_discord.rs` (comment)
- Modify: `crates/reborn-ui/ui/home.js` (`handOffInvite`)
- Modify: `crates/reborn-ui/ui/online-mm.js` (invite state, banner, signed-out title)
- Modify: `crates/reborn-ui/ui/pages/online.css` (append)
- Modify: `Tools/lib/page-states.mjs` (four states), `Tools/check-online-page.mjs` (four entries)

**Interfaces:**
- Consumes: `act -> Promise<boolean>`, `pick`, `privatePicked`, `roomCode`, `lockPanel`, `drawIdle` (online-mm.js); `inviteLine` (online-model.js); `with_client` (online_mm.rs); `url_path`, `Client::decode` (matchmaking.rs).
- Produces: Tauri commands `mm_pending_room() -> Option<{ kind: "room", code, at }>`, `mm_pending_room_clear()`, `mm_room_lookup(code: String) -> { host_name, mode } | null`; `netplay_pending_join` keeps returning only Netplay invites (a file with no `kind` is one). The pending file (`config/pending-join.json`) now holds `{ kind: "netplay"|"room", code, at }`. Core `Client::room_lookup(&self, code: &str) -> Result<Option<RoomLookup>, MmError>` (`GET /v1/rooms/<code>`, 404 → `None`); `RoomLookup { host_name: Option<String>, mode: Option<String> }`; `reborn_core::presence::join_link(secret: &str) -> String`. DOM `data-key`s: `invite-join`, `invite-dismiss`. The backend route `GET /v1/rooms/:code` arrives in Task 18; until then the lookup answers `None` and the line reads "You were invited to a room · CODE".

- [ ] **Step 1: Write the failing Rust tests**

In `crates/reborn-ui/src/main.rs`'s `mod tests`, after `an_invite_link_argument_yields_its_room_code_and_nothing_else`, add:

```rust
    #[test]
    fn a_room_link_is_its_own_kind_of_invite() {
        let room = |c: &str| Some(Invite { kind: InviteKind::Room, code: c.into() });
        assert_eq!(invite_from("nflstreetreborn://room/K7QP2M"), room("K7QP2M"));
        assert_eq!(invite_from("\"NFLSTREETREBORN://ROOM/k7-qp\""), room("k7-qp"), "quoted, any case; the code keeps its own");
        assert_eq!(invite_from("nflstreetreborn://room/K7QP2M/../x?y"), room("K7QP2M"), "only the code survives");
        assert_eq!(invite_from("nflstreetreborn://join/AB12CD"), Some(Invite { kind: InviteKind::Netplay, code: "AB12CD".into() }));
        assert_eq!(invite_from("nflstreetreborn://room/"), None);
        assert_eq!(join_code_from("nflstreetreborn://room/K7QP2M"), None, "a lobby code is never a Netplay host code");
        assert_eq!(room_code_from("nflstreetreborn://join/AB12CD"), None, "and a host code is never a lobby code");
        assert!(matches!(mode_from(["x.exe".to_string(), "nflstreetreborn://room/Q1".to_string()].into_iter()), Mode::Room(c) if c == "Q1"));
        assert_eq!(invite_from(&reborn_core::presence::join_link("K7QP2M")), room("K7QP2M"), "Discord's Join secret is the same room link");
    }

    #[test]
    fn the_pending_invite_file_keeps_its_kind_and_each_reader_sees_only_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("pending-join.json");
        write_pending(&path, &Invite { kind: InviteKind::Room, code: "K7QP2M".into() }).unwrap();
        assert_eq!(read_pending(&path, InviteKind::Room).unwrap().unwrap()["code"], "K7QP2M");
        assert_eq!(read_pending(&path, InviteKind::Netplay).unwrap(), None, "netplay_pending_join never sees a room invite");
        clear_pending(&path, InviteKind::Netplay).unwrap();
        assert!(path.exists(), "clearing the other kind leaves a room invite alone");
        clear_pending(&path, InviteKind::Room).unwrap();
        assert!(!path.exists());
        // A file written before kinds existed is a Netplay invite.
        std::fs::write(&path, br#"{"code":"AB12CD","at":"2026-09-11T00:00:00Z"}"#).unwrap();
        assert_eq!(read_pending(&path, InviteKind::Netplay).unwrap().unwrap()["code"], "AB12CD");
        assert_eq!(read_pending(&path, InviteKind::Room).unwrap(), None);
        clear_pending(&path, InviteKind::Room).unwrap();
        assert!(path.exists());
        let none = dir.path().join("none.json");
        assert_eq!(read_pending(&none, InviteKind::Room).unwrap(), None);
        clear_pending(&none, InviteKind::Room).unwrap();
    }
```

In `crates/reborn-core/src/matchmaking.rs`'s tests add:

```rust
    #[test]
    fn a_room_lookup_names_the_host_and_a_404_is_no_answer() {
        let url = "https://api.test/v1/rooms/K7QP2M";
        let http = FakeHttp::default().reply(url, 200, r#"{"ok":true,"host_name":"Jock","mode":"casual"}"#);
        let c = Client::new(&http, ORIGIN, "tok");
        assert_eq!(c.room_lookup("K7QP2M").unwrap(), Some(RoomLookup { host_name: Some("Jock".into()), mode: Some("casual".into()) }));
        assert_eq!(http.last_bearer().as_deref(), Some("tok"));
        let gone = FakeHttp::default().reply(url, 404, r#"{"ok":false,"error":"No such lobby."}"#);
        assert_eq!(Client::new(&gone, ORIGIN, "tok").room_lookup("K7QP2M").unwrap(), None, "no such room, or a server without the route");
        assert!(Client::new(&http, ORIGIN, "tok").room_lookup("K7/../QP2M?x").is_err(), "only letters, digits and dashes reach the path");
    }
```

In `crates/reborn-core/src/presence.rs`'s tests add:

```rust
    #[test]
    fn a_join_secret_stands_for_the_room_link() {
        assert_eq!(join_link("K7QP2M"), "nflstreetreborn://room/K7QP2M");
        assert_eq!(join_link(" K7QP2M "), "nflstreetreborn://room/K7QP2M");
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p reborn-core a_room_lookup_names; cargo test -p reborn-core a_join_secret_stands; cargo test -p reborn-ui a_room_link_is_its_own`
Expected: compile errors (`RoomLookup`, `room_lookup`, `join_link`, `Invite`, `invite_from` not found).

- [ ] **Step 3: Core: `join_link` and `room_lookup`**

In `crates/reborn-core/src/presence.rs`, after `pub const USE_FIELD_ART: bool = false;` add:

```rust
/// What a lobby's join secret stands for (launcher spec 2026-10-02 §3.10): the same `nflstreetreborn://room/<code>`
/// link a copied invite opens, so Discord's Join and a pasted link land in the same place on the Online page.
pub fn join_link(secret: &str) -> String {
    format!("nflstreetreborn://room/{}", secret.trim())
}
```

In `crates/reborn-core/src/presence_discord.rs`, change the two comment lines

```rust
                // A join secret is a ROOM CODE. Discord hands it back to the launcher, which turns
                // it into the same nflstreetreborn://join/<code> deep link a pasted invite makes.
```

to

```rust
                // A join secret is a LOBBY CODE. It stands for presence::join_link(code), the same
                // nflstreetreborn://room/<code> link a copied invite opens (never join/, which is a
                // Netplay host code).
```

In `crates/reborn-core/src/matchmaking.rs`, after the `RoomSummary` struct (and `host_name`), add:

```rust
/// `GET /v1/rooms/<code>`: who hosts a room and its mode, for the invite line on the Online page.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RoomLookup {
    pub host_name: Option<String>,
    pub mode: Option<String>,
}
```

and in `impl<'a> Client<'a>`, after `join_room`, add:

```rust
    /// Who invited the player (launcher spec 2026-10-02 §3.10). `None` on a 404: no such open room, or a server
    /// without the route, so the page simply leaves the name out. The code reaches the path as letters, digits and
    /// dashes only.
    pub fn room_lookup(&self, code: &str) -> Result<Option<RoomLookup>, MmError> {
        let url = self.url(&format!("/v1/rooms/{}", url_path(code)));
        let r = self.http.get_text_as(&url, Some(&self.token)).map_err(|e| MmError::Offline(e.to_string()))?;
        if r.status == 404 {
            return Ok(None);
        }
        Self::decode(r.status, r.body.as_bytes()).map(Some)
    }
```

Run: `cargo test -p reborn-core a_room_lookup_names; cargo test -p reborn-core a_join_secret_stands; cargo test -p reborn-core presence`
Expected: PASS.

- [ ] **Step 4: Launcher: the command, the invite kinds, the pending file**

In `crates/reborn-ui/src/online_mm.rs`, after `mm_room_join` add:

```rust
/// Who invited the player to a room (spec 2026-10-02 §3.10): `{ host_name, mode }`, or null when the server does not
/// say. Display only; joining is still `mm_room_join`.
#[tauri::command]
pub async fn mm_room_lookup(code: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let found = with_client(|c| c.room_lookup(code.trim()))?;
        Ok(found.map(|r| serde_json::json!({ "host_name": r.host_name, "mode": r.mode })).unwrap_or(serde_json::Value::Null))
    })
    .await
    .map_err(|e| e.to_string())?
}
```

In `crates/reborn-ui/src/main.rs`:

(a) In `enum Mode`, after the `Join(String),` variant add:

```rust
    /// A lobby invite (`nflstreetreborn://room/<code>`): remember the code, then open the window; Online offers Join.
    Room(String),
```

(b) Replace `mode_from` and `join_code_from` (from `fn mode_from(` through the end of `fn join_code_from`) with:

```rust
fn mode_from(args: impl Iterator<Item = String>) -> Mode {
    for a in args {
        match a.as_str() {
            "--play" => return Mode::Play,
            "--folder" => return Mode::Folder,
            other => {
                if let Some(code) = join_code_from(other) {
                    return Mode::Join(code);
                }
                if let Some(code) = room_code_from(other) {
                    return Mode::Room(code);
                }
            }
        }
    }
    Mode::Window
}

/// What an invite link is for (spec 2026-10-02 §3.10): a Netplay host code (`nflstreetreborn://join/<code>`, the
/// direct connection) or a matchmaking lobby code (`nflstreetreborn://room/<code>`). Two kinds, so a lobby code is
/// never typed into the Netplay form and a host code is never sent to `mm_room_join`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InviteKind {
    Netplay,
    Room,
}

impl InviteKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Netplay => "netplay",
            Self::Room => "room",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Invite {
    kind: InviteKind,
    code: String,
}

/// The invite inside a link, as Windows hands it to the launcher when a friend's link is opened. The scheme and the
/// kind are matched without regard to case; the code keeps its own. Only letters, digits and dashes count, so a link
/// cannot smuggle anything else onto the Online page.
fn invite_from(arg: &str) -> Option<Invite> {
    let arg = arg.trim().trim_matches('"');
    let lower = arg.to_ascii_lowercase();
    let (kind, rest) = if let Some(rest) = lower.strip_prefix("nflstreetreborn://join/") {
        (InviteKind::Netplay, rest)
    } else if let Some(rest) = lower.strip_prefix("nflstreetreborn://room/") {
        (InviteKind::Room, rest)
    } else {
        return None;
    };
    let code: String = arg[arg.len() - rest.len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    (!code.is_empty() && code.len() <= 64).then_some(Invite { kind, code })
}

/// The Netplay host code inside `nflstreetreborn://join/<code>`.
fn join_code_from(arg: &str) -> Option<String> {
    invite_from(arg).filter(|i| i.kind == InviteKind::Netplay).map(|i| i.code)
}

/// The lobby code inside `nflstreetreborn://room/<code>`.
fn room_code_from(arg: &str) -> Option<String> {
    invite_from(arg).filter(|i| i.kind == InviteKind::Room).map(|i| i.code)
}
```

(c) Replace `remember_pending_join`, `netplay_pending_join` and `netplay_pending_join_clear` (keep `pending_join_path` as it is) with:

```rust
fn remember_pending_invite(platform: &Platform, invite: &Invite) -> Result<(), String> {
    write_pending(&pending_join_path(platform)?, invite)
}

fn write_pending(path: &std::path::Path, invite: &Invite) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = serde_json::json!({"kind": invite.kind.as_str(), "code": invite.code, "at": reborn_core::utc_now_iso()});
    std::fs::write(path, serde_json::to_vec_pretty(&body).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// The waiting invite if it is of `kind`. A file written before kinds existed has none and is a Netplay invite.
fn read_pending(path: &std::path::Path, kind: InviteKind) -> Result<Option<serde_json::Value>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .filter(|v| v.get("kind").and_then(|k| k.as_str()).unwrap_or("netplay") == kind.as_str())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Forget the waiting invite, only if it is of `kind`: clearing one kind never drops the other.
fn clear_pending(path: &std::path::Path, kind: InviteKind) -> Result<(), String> {
    if read_pending(path, kind)?.is_none() {
        return Ok(());
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// The room code a Netplay invite link left behind, if any. Not cleared here: the
/// Online page clears it once it has filled the form, so a page that only
/// glanced (Play, deciding whether to redirect) does not lose it.
#[tauri::command]
async fn netplay_pending_join() -> Result<Option<serde_json::Value>, String> {
    read_pending(&pending_join_path(&Platform::detect())?, InviteKind::Netplay)
}

#[tauri::command]
async fn netplay_pending_join_clear() -> Result<(), String> {
    clear_pending(&pending_join_path(&Platform::detect())?, InviteKind::Netplay)
}

/// A lobby invite (`nflstreetreborn://room/<code>`) left behind, if any: `{ kind: "room", code, at }`. Not cleared
/// here: the Online page clears it after Join or Dismiss (spec 2026-10-02 §3.10), so it survives a sign-in.
#[tauri::command]
async fn mm_pending_room() -> Result<Option<serde_json::Value>, String> {
    read_pending(&pending_join_path(&Platform::detect())?, InviteKind::Room)
}

#[tauri::command]
async fn mm_pending_room_clear() -> Result<(), String> {
    clear_pending(&pending_join_path(&Platform::detect())?, InviteKind::Room)
}
```

(d) In `main()`'s `match mode_from(std::env::args())`, change the `Mode::Join(code) => {…}` arm to:

```rust
        Mode::Join(code) => {
            // Remembered, then the window opens as usual; Play sends the
            // player straight on to Online, which fills in the room.
            let p = Platform::detect();
            if let Err(why) = remember_pending_invite(&p, &Invite { kind: InviteKind::Netplay, code }) {
                eprintln!("Could not keep the invite's room code: {why}");
            }
        }
        Mode::Room(code) => {
            // The same for a lobby: Online offers Join (it never joins on its own).
            let p = Platform::detect();
            if let Err(why) = remember_pending_invite(&p, &Invite { kind: InviteKind::Room, code }) {
                eprintln!("Could not keep the invite's room code: {why}");
            }
        }
```

(e) In the `tauri_plugin_single_instance::init(|app, args, _cwd| {…})` closure, change

```rust
            if let Some(code) = args.iter().find_map(|a| join_code_from(a)) {
                if let Err(why) = remember_pending_join(&Platform::detect(), &code) {
```

to

```rust
            if let Some(invite) = args.iter().find_map(|a| invite_from(a)) {
                if let Err(why) = remember_pending_invite(&Platform::detect(), &invite) {
```

(f) In `generate_handler![…]`, after `online_mm::mm_room_join,` add `online_mm::mm_room_lookup,`, and after `netplay_pending_join_clear,` add `mm_pending_room,` and `mm_pending_room_clear,`.

Run: `cargo test -p reborn-ui a_room_link_is_its_own; cargo test -p reborn-ui the_pending_invite_file; cargo test -p reborn-ui an_invite_link_argument; cargo test -p reborn-ui mode_from; git diff --check; file crates/reborn-ui/src/main.rs`
Expected: all PASS (the old invite test still passes unchanged); no whitespace output; `file` does not say CRLF.

- [ ] **Step 5: Add the page states and failing expectations**

In `Tools/lib/page-states.mjs` above the redesign marker add:

```js
    N('online-invite', { mm_state: mm(empty), mm_pending_room: { kind: 'room', code: 'K7QP2M', at: '2026-09-30T19:59:00Z' }, mm_room_lookup: { host_name: 'Jock', mode: 'casual' } }),
    N('online-invite-nohost', { mm_state: mm(empty), mm_pending_room: { kind: 'room', code: 'K7QP2M', at: '2026-09-30T19:59:00Z' }, mm_room_lookup: null }),
    N('online-invite-signed-out', { mm_state: mm(null, { link: 'signed_out' }), mm_pending_room: { kind: 'room', code: 'K7QP2M', at: '2026-09-30T19:59:00Z' } }),
    N('online-invite-busy', { mm_state: mm({ ...empty, ticket: { mode: 'casual', field: null, waited_ms: 5000, enqueued_at: '2026-09-30T19:59:55.000Z' } }),
      mm_pending_room: { kind: 'room', code: 'K7QP2M', at: '2026-09-30T19:59:00Z' } }),
```

In `Tools/check-online-page.mjs` above the states marker add:

```js
  'online-invite': {
    text: ['Jock invited you to a room · K7QP2M', 'Host a room'],
    check: async (page) => {
      const p = [];
      if ((await calls(page, 'mm_room_join')).length) p.push('the invite joined on its own');
      if (await page.locator('[data-key="mode-private"]').getAttribute('aria-pressed') !== 'true') p.push('Private room is not picked');
      if (await page.getByLabel('Room code', { exact: true }).inputValue() !== 'K7QP2M') p.push('the code field is not filled');
      await page.locator('[data-key="invite-join"]').click();
      await page.waitForTimeout(400);
      const j = await calls(page, 'mm_room_join');
      if (JSON.stringify(j[0]) !== '{"code":"K7QP2M"}') p.push(`Join sent ${JSON.stringify(j[0])}`);
      if (!(await calls(page, 'mm_pending_room_clear')).length) p.push('a joined invite was not cleared');
      return p;
    },
  },
  'online-invite-nohost': {
    text: ['You were invited to a room · K7QP2M'],
    check: async (page) => {
      const p = [];
      await page.locator('[data-key="invite-dismiss"]').click();
      await page.waitForTimeout(300);
      if (!(await calls(page, 'mm_pending_room_clear')).length) p.push('Dismiss did not clear the invite');
      if (await page.locator('.on-invite').count()) p.push('the invite line is still there');
      return p;
    },
  },
  'online-invite-signed-out': {
    text: ['Sign in to join K7QP2M', 'Sign in with Discord'],
    check: async (page) => ((await calls(page, 'mm_pending_room_clear')).length || (await calls(page, 'mm_room_join')).length ? ['a signed-out invite was used up'] : []),
  },
  'online-invite-busy': {
    absent: ['invited you', 'You were invited'],
    check: async (page) => ((await calls(page, 'mm_pending_room_clear')).length ? ['the invite was dropped while queued'] : []),
  },
```

- [ ] **Step 6: Run them to see them fail**

Run: `node Tools/check-online-page.mjs`
Expected: `FAIL online-invite: missing text "Jock invited you to a room · K7QP2M"…`, `FAIL online-invite-nohost…`, `FAIL online-invite-signed-out: missing text "Sign in to join K7QP2M"`; `online-invite-busy` already passes.

- [ ] **Step 7: The page side**

In `crates/reborn-ui/ui/home.js`, replace `handOffInvite` with:

```js
// Opened from a friend's invite link (a Netplay `join/` or a lobby `room/`): straight on to Online, which fills in the
// room or offers Join. Play is the window's first page, so this is where the hand-off lives; the code stays put until
// Online has used it.
async function handOffInvite() {
  try {
    const pending = await call("netplay_pending_join");
    const room = pending && pending.code ? null : await call("mm_pending_room").catch(() => null);
    if (!(pending && pending.code) && !(room && room.code)) return;
    // Not before setup: Online needs a game to join with. The code stays
    // put, and this runs again once Play comes back after setup.
    const progress = await call("onboarding_progress", { legacyRoot: null });
    if (progress && progress.complete) location.replace("online.html");
  } catch (_) { /* Online is one click away either way */ }
}
```

In `crates/reborn-ui/ui/online-mm.js`, add `inviteLine` to the online-model import (`… heldNotice, inviteLine, inviteLink, …`). After the `lobbiesLink?.addEventListener(…)` line add:

```js
// ---- a room invite (spec §3.10) ----
// A `nflstreetreborn://room/<code>` link left its code behind (mm_pending_room). On load and on window focus the page
// picks Private room, fills the code and offers one line with Join; it never joins on its own. Join or Dismiss clears
// it; anything else (signed out, busy) leaves it waiting.
let invite = null; // { code, host }
async function takeRoomInvite() {
  if (!bridge) return;
  try {
    const p = await call('mm_pending_room');
    if (!p || typeof p.code !== 'string' || !p.code || invite?.code === p.code) return;
    invite = { code: p.code, host: null };
    privatePicked = true;
    roomCode = p.code;
    lastDrawn = '';
    draw();
    const found = await call('mm_room_lookup', { code: p.code }).catch(() => null);
    if (found && typeof found.host_name === 'string' && found.host_name && invite?.code === p.code) {
      invite.host = found.host_name;
      lastDrawn = '';
      draw();
    }
  } catch { /* an invite that cannot be read is no invite */ }
}
function forgetInvite() {
  invite = null;
  call('mm_pending_room_clear').catch(() => {});
}
function inviteBanner() {
  const line = inviteLine(invite.host, invite.code);
  const code = invite.code;
  return el('div', { class: 'glass on-invite', role: 'status' },
    el('p', {}, line.who ? el('b', { text: line.who }) : null, line.rest),
    el('button', { type: 'button', class: 'btn primary sm', 'data-key': 'invite-join', text: 'Join',
      onclick: async () => { if (await act('mm_room_join', { code })) forgetInvite(); } }),
    el('button', { type: 'button', class: 'btn quiet sm', 'data-key': 'invite-dismiss', text: 'Dismiss',
      onclick: () => { forgetInvite(); lastDrawn = ''; draw(); } }));
}
takeRoomInvite();
window.addEventListener('focus', () => takeRoomInvite());
```

In `draw()`:
- change `body = [lockPanel('Sign in to play online', 'Connect your Discord account to play online and track your stats!', true)];` to `body = [lockPanel(invite ? \`Sign in to join ${invite.code}\` : 'Sign in to play online', 'Connect your Discord account to play online and track your stats!', true)];`
- after the line `if (snap.match?.relay_state === 'held' && dismissedHold !== snap.match.id) body.unshift(heldBox(snap.match));` add `if (invite) body.unshift(inviteBanner());`

(The banner is drawn only in the idle branch, so a queued, matched or roomed player never sees it, and the invite waits.)

Append to `pages/online.css`:

```css
/* a room invite (spec §3.10): one line above the tiles */
.on-invite{display:flex;align-items:center;gap:12px;padding:12px 16px}
.on-invite p{flex:1;min-width:0;margin:0;font-size:13px;color:var(--fg-2)}
.on-invite b{font-weight:600;color:var(--fg)}
```

- [ ] **Step 8: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-online-mm.mjs; node Tools/check-online-restored.mjs; node Tools/check-session-state.mjs; node Tools/check-shell-pages.mjs; node Tools/check-page-call-args.mjs; node Tools/check-play-page.mjs`
Expected: `check-online-page: PASS (32 state(s))`; every other check PASS (check-page-call-args finds `mm_pending_room` and `mm_pending_room_clear` in main.rs with no arguments).

- [ ] **Step 9: Commit**

```bash
git add crates/reborn-ui/src/main.rs crates/reborn-ui/src/online_mm.rs crates/reborn-core/src/matchmaking.rs crates/reborn-core/src/presence.rs crates/reborn-core/src/presence_discord.rs crates/reborn-ui/ui/home.js crates/reborn-ui/ui/online-mm.js crates/reborn-ui/ui/pages/online.css Tools/lib/page-states.mjs Tools/check-online-page.mjs
git commit -m "online: real room invite links (nflstreetreborn://room/<code>), mm_pending_room, the invite line with Join"
```

---

### Task 13: Controller and keyboard: pick follows focus, A, B / Esc, focus kept across redraws

**Files:**
- Modify: `crates/reborn-ui/ui/app-shell.js` (`back()`: the `pad-back` event)
- Modify: `crates/reborn-ui/ui/DESIGN.md` (§7 table: one row)
- Modify: `crates/reborn-ui/ui/online-mm.js` (focus restore in `draw()`, `focusin` pick, `pad-back` listener)
- Modify: `Tools/lib/page-states.mjs` (three states), `Tools/check-online-page.mjs` (four entries)

**Interfaces:**
- Consumes: `pick`, `picked`, `section`, `busy` (online-mm.js); app-shell's `.pad-focus` and `body.gamepad-input`.
- Produces: a cancelable `pad-back` event dispatched on `document` by `back()` (B or Esc from page content, after selects and open dialogs had their turn); a listener that calls `preventDefault()` keeps B on the page.

- [ ] **Step 1: Add the states and failing expectations**

In `Tools/lib/page-states.mjs` above the redesign marker add:

```js
    N('online-pad', { mm_state: mm(empty) }, { init: PICK('ranked'), pad: true }),
    N('online-searching-esc', { mm_state: mm({ ...empty, ticket: { mode: 'ranked', field: null, waited_ms: 42000, enqueued_at: '2026-09-30T19:59:18.000Z' } }) }, { init: PICK('ranked') }),
    N('online-match-found-esc', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2 }) }) }),
    N('online-focus-keep', { mm_state: mm(empty) }, { click: '[data-key="mode-private"]' }),
```

In `Tools/check-online-page.mjs` above the states marker add:

```js
  'online-pad': {
    check: async (page) => {
      const p = [];
      // A controller drives: the shell puts gamepad-input on <body> and focuses with .pad-focus.
      await page.evaluate(() => { document.body.classList.add('gamepad-input'); document.querySelector('[data-key="mode-casual"]').focus(); });
      await page.waitForTimeout(500);
      if (await page.locator('[data-key="mode-casual"]').getAttribute('aria-pressed') !== 'true') p.push('the pick did not follow focus to Casual');
      if (await page.evaluate(() => document.activeElement?.dataset.key) !== 'mode-casual') p.push('focus left the tile after the redraw');
      const offset = await page.evaluate(() => getComputedStyle(document.activeElement).outlineOffset);
      if (offset !== '4px') p.push(`the tile's focus ring sits ${offset} out, not 4px`);
      await page.evaluate(() => document.activeElement.click()); // A
      await page.waitForFunction(() => window.__calls.some((c) => c.cmd === 'content_check'), null, { timeout: 3000 }).catch(() => p.push('A on the picked tile did not start Find a game'));
      return p;
    },
  },
  'online-searching-esc': {
    check: async (page) => {
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
      return (await calls(page, 'mm_leave_queue')).length ? [] : ['Esc while searching did not Cancel'];
    },
  },
  'online-match-found-esc': {
    check: async (page) => {
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
      const a = await calls(page, 'mm_match_action');
      return JSON.stringify(a[0]) === '{"matchId":"m5","action":"accept","value":false}' ? [] : [`Esc on Match found sent ${JSON.stringify(a[0])}`];
    },
  },
  'online-focus-keep': {
    check: async (page) => {
      const p = [];
      await page.getByLabel('Room code', { exact: true }).focus();
      await page.evaluate(() => document.dispatchEvent(new CustomEvent('online-rank', { detail: null }))); // forces a redraw
      await page.waitForTimeout(300);
      if (await page.evaluate(() => document.activeElement?.dataset.key) !== 'room-code') p.push('an empty code field lost focus on a redraw');
      await page.keyboard.type('AB');
      await page.evaluate(() => document.dispatchEvent(new CustomEvent('online-rank', { detail: null })));
      await page.waitForTimeout(300);
      if (await page.getByLabel('Room code', { exact: true }).inputValue() !== 'AB') p.push('typed text was lost on a redraw');
      if (await page.evaluate(() => document.activeElement?.dataset.key) !== 'room-code') p.push('a typed code field lost focus');
      return p;
    },
  },
```

- [ ] **Step 2: Run them to see them fail**

Run: `node Tools/check-online-page.mjs`
Expected: `FAIL online-pad: … focus left the tile after the redraw …`, `FAIL online-searching-esc: Esc while searching did not Cancel`, `FAIL online-match-found-esc…`, `FAIL online-focus-keep: an empty code field lost focus on a redraw`.

- [ ] **Step 3: The `pad-back` event in `app-shell.js`**

In `function back()`, after the line that closes an open modal (`const modal=document.querySelector('dialog[open]');if(modal){…return;}`) add:

```js
  // A page may answer B / Esc itself (Online: Cancel while searching, Decline on Match found; spec 2026-10-02 §6) by
  // calling preventDefault() on this event. Otherwise B goes on as always (folds, wizard Back, the sidebar).
  if (!document.dispatchEvent(new CustomEvent('pad-back', { cancelable: true }))) return;
```

In `DESIGN.md` §7's table add:

```markdown
| `pad-back` event | dispatched on `document` by B / Esc, cancelable | a page that calls `preventDefault()` answers B itself (Online: Cancel while searching, Decline on Match found); otherwise B goes to the sidebar as always |
```

- [ ] **Step 4: Focus, pick and B in `online-mm.js`**

In `draw()`, change the last lines

```js
  setLinks(showLinks);
  showOnline(snap, state.presence_on);
  section.replaceChildren(...body, status);
```

to

```js
  setLinks(showLinks);
  showOnline(snap, state.presence_on);
  // Keep the control the player is on across the redraw, by data-key (spec §6): the page used to drop focus on every
  // state change, which left a controller nowhere.
  const had = document.activeElement;
  const focusKey = had && section.contains(had) ? had.getAttribute('data-key') : null;
  section.replaceChildren(...body, status);
  if (focusKey) section.querySelector(`[data-key="${focusKey}"]`)?.focus({ preventScroll: true });
```

After the `rankInfo` listener add:

```js
// A controller drives (spec §6): the pick follows focus across the tiles, so the D-pad onto Casual picks Casual.
section.addEventListener('focusin', (e) => {
  if (!document.body.classList.contains('gamepad-input')) return;
  const tile = e.target.closest?.('.on-tile');
  const key = tile?.dataset.key?.replace(/^mode-/, '');
  if (key && !tile.disabled && key !== picked()) pick(key);
});
// B / Esc: Cancel while searching, Decline on Match found; anywhere else B goes on to the sidebar as always.
document.addEventListener('pad-back', (e) => {
  const target = section.querySelector('[data-key="cancel"], [data-key="decline"]');
  if (target && !target.disabled && !busy) { e.preventDefault(); target.click(); }
});
```

- [ ] **Step 5: Run the checks**

Run: `node Tools/check-online-page.mjs; node Tools/check-controller-navigation.mjs; node Tools/check-pad-columns.mjs; node Tools/check-controller-hints.mjs; node Tools/check-save-lane.mjs; node Tools/check-menu.mjs`
Expected: `check-online-page: PASS (36 state(s))`; the shell checks PASS (B still goes to the sidebar on every other page; nothing there listens for `pad-back`).

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/ui/app-shell.js crates/reborn-ui/ui/DESIGN.md crates/reborn-ui/ui/online-mm.js Tools/lib/page-states.mjs Tools/check-online-page.mjs
git commit -m "online: pick follows controller focus, A runs the row, B/Esc cancels or declines, focus kept across redraws"
```

---

### Task 14: Launcher phase verification

**Files:**
- Modify: `Tools/run-browser-tests.mjs` (one import)
- No product code.

**Interfaces:**
- Consumes: everything above.
- Produces: a green launcher phase, screenshots of every state in `target/online-page/` for Adam to compare with the mockup.

- [ ] **Step 1: Add the page check to the browser runner**

In `Tools/run-browser-tests.mjs`, after `await import('./check-themed-shell.mjs');` add `await import('./check-online-page.mjs');`.

- [ ] **Step 2: Run the unit tests and the Rust tests**

Run: `node --test crates/reborn-ui/ui/*.test.mjs Tools/lib/*.test.mjs; cargo test -p reborn-core; cargo test -p reborn-ui`
Expected: all pass. If a Rust test fails that this plan did not touch, compare with `git stash`-free evidence: check out the base commit in a temporary worktree (`git worktree add ../online-base 05a6aa6`) and run the same test there; record it as baseline only if it fails there too.

- [ ] **Step 3: Run every check named in the Baseline table, plus the new one**

Run each and record the result:

```bash
for c in check-online-page check-online-mm check-online-restored check-live-now check-session-state check-themed-shell check-theme check-theme-tokens check-live-view check-shell-pages check-page-call-args check-menu check-shell-chrome check-controller-hints check-pad-columns check-controller-navigation check-glass-pages check-play-page check-stats-page check-profile-page; do node Tools/$c.mjs > target/$c.log 2>&1; echo "$c exit=$?"; done
```

Expected: every check exits 0 except `check-live-view` (its baseline failure, unrelated). `check-online-mm` is now green (it failed at baseline). `node Tools/review-finish.mjs` is expected to get past the Online step it failed on at baseline (line 171); if it fails later, compare that failure with the base commit as in Step 2.

- [ ] **Step 4: Look at the screenshots**

Open `target/online-page/*.png` and set them beside the mockup's 12 states (`docs/superpowers/specs/assets/2026-10-02-online-page-mockup.html`): Ranked picked, Casual picked, Private room picked, With a controller (`online-pad`), Searching, Match found, You accepted, Starting the game, Playing a series, Private room: your lobby (`online-room`), Held match, Signed out. Fix any layout difference in `pages/online.css` only, re-run `node Tools/check-online-page.mjs` and `node Tools/check-theme-tokens.mjs`, and commit as `online: layout fixes after the screenshot pass`.

- [ ] **Step 5: Check line endings**

Run: `git diff --check 05a6aa6..HEAD; git ls-files --eol crates/reborn-ui/src crates/reborn-core/src | grep -v "w/lf" || echo all-lf`
Expected: no whitespace errors; `all-lf`.

- [ ] **Step 6: Commit**

```bash
git add Tools/run-browser-tests.mjs
git commit -m "online: check-online-page joins the browser runner"
```

---

# Phase 2: Backend (`C:\Users\Adam\Desktop\Reborn Installer\Reborn Backend`)

Work in a new worktree so the deployed `main` checkout is never touched:

```bash
cd "C:/Users/Adam/Desktop/Reborn Installer/Reborn Backend"
git worktree add "../reborn-backend-online" -b feat/online-page-backend main
cd "../reborn-backend-online" && cp "../Reborn Backend/.env" . && npm ci
```

Every test below runs against `DATABASE_URL_TEST` from `.env` (`node --env-file-if-exists=.env --test test/<file>.test.js`). Elo stays OFF in production until launch (`ELO_ENABLED` unset); tests switch it on with `setEloEnabled(true)` per file.

### Task 15: `elo.week_delta` on the dashboard and the profile

**Files:**
- Modify: `src/profile/load.js` (`UNRANKED`, `eloOf`)
- Modify: `test/profile_load.test.js` (one new test, one updated `deepEqual`), `test/elo_off.test.js` (one updated `deepEqual`)

**Interfaces:**
- Consumes: `rating_event`, `match` tables.
- Produces: `eloOf(pool, playerId)` returns `week_delta: number` (sum of the player's ranked `rating_event.delta` with `applied_at >= now() - interval '7 days'`; `0` when none) for a rated player and `week_delta: null` when unranked or Elo is off. `/v1/me/dashboard`'s `you.elo` and the profile's `elo` carry it (the launcher's Ranked tile reads it, Task 4).

- [ ] **Step 1: Write the failing test**

Append to `test/profile_load.test.js` (Elo is switched on at the top of this file):

```js
test("eloOf: week_delta sums the last seven days of ranked Elo changes", async () => {
  const t = await testPool();
  try {
    const a = (await t.pool.query(`INSERT INTO player (display_name) VALUES ('A') RETURNING id`)).rows[0].id;
    const b = (await t.pool.query(`INSERT INTO player (display_name) VALUES ('B') RETURNING id`)).rows[0].id;
    const c = (await t.pool.query(`INSERT INTO player (display_name) VALUES ('C') RETURNING id`)).rows[0].id;
    await t.pool.query(`INSERT INTO rating (player_id, mode, rating, games) VALUES ($1, 'ranked', 1107, 3), ($2, 'ranked', 1000, 1)`, [a, b]);
    const match = async (mode) => (await t.pool.query(`INSERT INTO match (mode, status, seat0_player, seat1_player) VALUES ($1, 'complete', $2, $3) RETURNING id`, [mode, a, b])).rows[0].id;
    const event = (m, delta, ago) => t.pool.query(`INSERT INTO rating_event (match_id, player_id, seat, rating_before, rating_after, delta, k_factor, expected, cause, applied_at)
      VALUES ($1, $2, 0, 1000, 1000 + $3, $3, 32, 0.5, 'accepted', now() - $4::interval)`, [m, a, delta, ago]);
    await event(await match('ranked'), 12, '2 days');
    await event(await match('ranked'), -5, '1 day');
    await event(await match('ranked'), 30, '9 days');
    await event(await match('casual'), 99, '1 day');
    assert.equal((await eloOf(t.pool, a)).week_delta, 7, "12 - 5; nine days ago and casual do not count");
    assert.equal((await eloOf(t.pool, b)).week_delta, 0, "rated, nothing this week");
    assert.equal((await eloOf(t.pool, c)).week_delta, null, "unranked");
  } finally { await t.close(); }
});
```

In the same file change the unranked assertion

```js
    assert.deepEqual(await eloOf(t.pool, a), { rating: null, rank: null, total: 0, games: 0, last_delta: null, peak: null, peak_at: null, history: [], around: [] });
```

to

```js
    assert.deepEqual(await eloOf(t.pool, a), { rating: null, rank: null, total: 0, games: 0, last_delta: null, peak: null, peak_at: null, history: [], around: [], week_delta: null });
```

and in `test/elo_off.test.js` change

```js
      { rating: null, rank: null, total: 0, games: 0, last_delta: null, peak: null, peak_at: null, history: [], around: [] });
```

to

```js
      { rating: null, rank: null, total: 0, games: 0, last_delta: null, peak: null, peak_at: null, history: [], around: [], week_delta: null });
```

- [ ] **Step 2: Run them to see them fail**

Run: `node --env-file-if-exists=.env --test test/profile_load.test.js test/elo_off.test.js`
Expected: FAIL: `week_delta` is `undefined`.

- [ ] **Step 3: Implement**

In `src/profile/load.js` change

```js
const UNRANKED = (total) => ({ rating: null, rank: null, total, games: 0, last_delta: null, peak: null, peak_at: null, history: [], around: [] });
```

to

```js
const UNRANKED = (total) => ({ rating: null, rank: null, total, games: 0, last_delta: null, peak: null, peak_at: null, history: [], around: [], week_delta: null });
```

In `eloOf`, after the `around` constant, add:

```js
  // The last seven days of ranked movement (launcher spec 2026-10-02 §3.1: "▲ 16 this week" on the Ranked tile).
  const week = (await pool.query(
    `SELECT COALESCE(sum(re.delta), 0)::int AS d FROM rating_event re JOIN match m ON m.id = re.match_id
      WHERE re.player_id = $1 AND m.mode = 'ranked' AND re.applied_at >= now() - interval '7 days'`, [playerId])).rows[0].d;
```

and add `week_delta: week` to the returned object (after `around`).

- [ ] **Step 4: Run them to see them pass**

Run: `node --env-file-if-exists=.env --test test/profile_load.test.js test/elo_off.test.js test/profile_redesign.test.js`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/profile/load.js test/profile_load.test.js test/elo_off.test.js
git commit -m "elo: week_delta (the last seven days of ranked movement) on the dashboard and profile"
```

---

### Task 16: `GET /v1/leaderboard`

**Files:**
- Create: `src/mm/teamView.js`
- Modify: `src/mm/presence.js` (imports, `leaderboardFor`, the route)
- Create: `test/leaderboard.test.js`

**Interfaces:**
- Consumes: `LADDER_COLS`, `ON_LADDER`, `ladderTotal`, `ladderRank`, `eloEnabled`, `avatarUrl`, `bearerPlayer`, `liveAssets`.
- Produces: `teamView(tgid, assets = liveAssets) -> { tgid, name, abbrev, logo_url, primary } | null` (Task 17 uses it). `leaderboardFor(client, { limit, viewerId, assets }) -> { enabled, total, rows, you }`; `GET /v1/leaderboard?limit=10` → `{ ok: true, enabled, total, rows: [{ rank, player_id, display_name, avatar_url, team, wins, losses, elo, delta_week, you }], you }` — public; a valid bearer only adds `you` (the viewer's row when outside `rows`) and the `you` flags. Elo off: `{ ok: true, enabled: false, total: 0, rows: [], you: null }`. This is the launcher's Task 11 `leaderboard` command's answer.
- Note for launch (ruling 11): the launcher's "Full leaderboard ›" opens `https://online.nflstreetreborn.com/leaderboard`, which does not exist yet (`webRouter.js`). Build that page before `ELO_ENABLED=1`; until then the link only shows while Elo is on, so no player can reach the 404.

- [ ] **Step 1: Write the failing tests**

Create `test/leaderboard.test.js`:

```js
// GET /v1/leaderboard (launcher spec 2026-10-02 §4, §7): public, a bearer only adds your own row; Elo off is an empty
// board that says so.
import { test } from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { newToken, hashToken } from "../src/tokens.js";
import { setEloEnabled } from "../src/eloSwitch.js";
import { presenceRouter, leaderboardFor } from "../src/mm/presence.js";
import { liveAssets } from "../src/liveAssets.js";

let seq = 0;
const player = async (pool, name, { tgid = null, avatar = null } = {}) => (await pool.query(
  `INSERT INTO player (discord_user_id, display_name, avatar_hash, plays_as_tgid) VALUES ($1, $2, $3, $4) RETURNING *`,
  [`90000000${String(++seq).padStart(4, "0")}`, name, avatar, tgid])).rows[0];
const rate = (pool, p, rating, wins, losses) => pool.query(
  `INSERT INTO rating (player_id, mode, rating, games, wins, losses) VALUES ($1, 'ranked', $2, $3, $4, $5)`, [p.id, rating, wins + losses, wins, losses]);
const token = async (pool, p, clock) => {
  const raw = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at, last_used_at) VALUES ($1, $2, 'test', $3, $3)`, [p.id, hashToken(raw), clock.now()]);
  return raw;
};
async function board(pool) {
  const ps = [];
  for (let i = 0; i < 12; i++) {
    const p = await player(pool, `P${i}`, i === 0 ? { tgid: 14, avatar: "0123456789abcdef0123456789abcdef" } : {});
    await rate(pool, p, 1300 - i * 10, 10 - (i % 5), i % 5);
    ps.push(p);
  }
  const m = async () => (await pool.query(`INSERT INTO match (mode, status, seat0_player, seat1_player) VALUES ('ranked', 'complete', $1, $2) RETURNING id`, [ps[0].id, ps[1].id])).rows[0].id;
  for (const [delta, ago] of [[16, "2 days"], [40, "9 days"]]) {
    await pool.query(`INSERT INTO rating_event (match_id, player_id, seat, rating_before, rating_after, delta, k_factor, expected, cause, applied_at)
      VALUES ($1, $2, 0, 1000, 1000 + $3, $3, 32, 0.5, 'accepted', now() - $4::interval)`, [await m(), ps[0].id, delta, ago]);
  }
  return ps;
}
async function serve(t, clock) {
  const app = express();
  app.use(presenceRouter({ pool: t.pool, clock }));
  app.use((req, res) => res.status(404).json({ ok: false, error: "No such route" }));
  const server = await new Promise((resolve, reject) => { const s = app.listen(0); s.once("listening", () => resolve(s)); s.once("error", reject); });
  const base = `http://127.0.0.1:${server.address().port}`;
  const get = async (path, bearer = null) => {
    const res = await fetch(base + path, { headers: bearer ? { authorization: `Bearer ${bearer}` } : {} });
    return { status: res.status, json: await res.json() };
  };
  return { get, close: () => new Promise((r) => server.close(r)) };
}

test("Elo off: an empty board that says so, whatever ratings exist", async () => {
  setEloEnabled(false);
  const t = await testPool();
  try {
    await board(t.pool);
    assert.deepEqual(await leaderboardFor(t.pool, {}), { enabled: false, total: 0, rows: [], you: null });
  } finally { await t.close(); }
});

test("Elo on: the top rows in ladder order, with record, Elo, the week's change and the plays-as team; you after them", async () => {
  setEloEnabled(true);
  const t = await testPool();
  try {
    const ps = await board(t.pool);
    const out = await leaderboardFor(t.pool, { limit: 10, viewerId: ps[11].id });
    assert.equal(out.enabled, true);
    assert.equal(out.total, 12);
    assert.deepEqual(out.rows.map((r) => r.rank), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    const top = out.rows[0];
    assert.deepEqual([top.display_name, top.elo, top.wins, top.losses, top.delta_week], ["P0", 1300, 10, 0, 16], "nine days ago does not count");
    assert.match(top.avatar_url, /^https:\/\/cdn\.discordapp\.com\/avatars\//);
    assert.equal(top.team?.tgid ?? null, liveAssets.team(14)?.tgid ?? null);
    assert.equal(out.rows[1].team, null);
    assert.equal(out.rows[1].delta_week, 0);
    assert.deepEqual([out.you.rank, out.you.you, out.you.display_name], [12, true, "P11"]);
    const mine = await leaderboardFor(t.pool, { limit: 10, viewerId: ps[3].id });
    assert.equal(mine.you, null, "already on the card");
    assert.equal(mine.rows[3].you, true);
    assert.equal((await leaderboardFor(t.pool, { limit: 999 })).rows.length, 12, "the ceiling is 50");
    assert.equal((await leaderboardFor(t.pool, { limit: "abc" })).rows.length, 10, "a bad limit is the default");
  } finally { setEloEnabled(false); await t.close(); }
});

test("GET /v1/leaderboard is public; a bearer only adds your own row", async () => {
  setEloEnabled(true);
  const t = await testPool(); const clock = fixedClock();
  const s = await serve(t, clock);
  try {
    const ps = await board(t.pool);
    let r = await s.get("/v1/leaderboard");
    assert.equal(r.status, 200);
    assert.equal(r.json.ok, true);
    assert.equal(r.json.rows.length, 10);
    assert.equal(r.json.you, null);
    r = await s.get("/v1/leaderboard?limit=1", await token(t.pool, ps[4], clock));
    assert.equal(r.json.rows.length, 1);
    assert.equal(r.json.you.rank, 5);
    r = await s.get("/v1/leaderboard", "not-a-real-token-at-all-000000");
    assert.equal(r.status, 200, "a bad bearer is anonymous, never a 401");
  } finally { setEloEnabled(false); await s.close(); await t.close(); }
});
```

- [ ] **Step 2: Run them to see them fail**

Run: `node --env-file-if-exists=.env --test test/leaderboard.test.js`
Expected: FAIL: `leaderboardFor` is not exported.

- [ ] **Step 3: Implement**

Create `src/mm/teamView.js`:

```js
// A team as the launcher draws it beside a player (the leaderboard's badge, the matchup card's logo; launcher spec
// 2026-10-02 §3.3, §4): only what it shows, from the live-assets catalog, or null when there is no team.
import { liveAssets } from "../liveAssets.js";

export function teamView(tgid, assets = liveAssets) {
  if (tgid === null || tgid === undefined) return null;
  const t = assets.team(Number(tgid));
  return t && t.name ? { tgid: t.tgid ?? null, name: t.name, abbrev: t.abbrev ?? null, logo_url: t.logo_url ?? null, primary: t.primary ?? null } : null;
}
```

In `src/mm/presence.js`, change `import { requirePlayer } from "../auth.js";` to `import { bearerPlayer, requirePlayer } from "../auth.js";` and add `import { liveAssets } from "../liveAssets.js";` and `import { teamView } from "./teamView.js";` to the imports. After `ladderAround` add:

```js
// --------------------------------------------------------------------------------------------
// The leaderboard card (launcher spec 2026-10-02 §4, §7)
// --------------------------------------------------------------------------------------------

export const LEADERBOARD_LIMIT_DEFAULT = 10;
export const LEADERBOARD_LIMIT_MAX = 50;

// The ladder's own pool and order (ON_LADDER, rating then id), plus the plays-as team and the last seven days of
// ranked movement. `$1` mode, `$2` offset, `$3` limit.
const BOARD_SQL = `
  SELECT ${LADDER_COLS}, p.plays_as_tgid,
         (SELECT COALESCE(sum(re.delta), 0)::int FROM rating_event re JOIN match m ON m.id = re.match_id
           WHERE re.player_id = r.player_id AND m.mode = 'ranked' AND re.applied_at >= now() - interval '7 days') AS delta_week
    FROM rating r JOIN player p ON p.id = r.player_id
   WHERE ${ON_LADDER}
   ORDER BY r.rating DESC, p.id
  OFFSET $2 LIMIT $3`;

const boardRow = (row, rank, you, assets) => ({
  rank,
  player_id: row.player_id,
  display_name: row.display_name,
  avatar_url: avatarUrl(row.discord_user_id, row.avatar_hash, 64),
  team: teamView(row.plays_as_tgid, assets),
  wins: Number(row.wins),
  losses: Number(row.losses),
  elo: Number(row.rating),
  delta_week: Number(row.delta_week),
  you: you !== null && row.player_id === you,
});

/**
 * The top `limit` ranked rows (default 10, at most 50) and, for a viewer who is not among them, the viewer's own row
 * (`you`). Elo off until launch (src/eloSwitch.js): an empty board that says so, like the ladder.
 */
export async function leaderboardFor(client, { limit, viewerId = null, assets = liveAssets } = {}) {
  if (!eloEnabled()) return { enabled: false, total: 0, rows: [], you: null };
  const asked = Math.trunc(Number(limit));
  const n = Number.isFinite(asked) && asked >= 1 ? Math.min(asked, LEADERBOARD_LIMIT_MAX) : LEADERBOARD_LIMIT_DEFAULT;
  const you = viewerId ? String(viewerId).toLowerCase() : null;
  const total = await ladderTotal(client, "ranked");
  const rows = (await client.query(BOARD_SQL, ["ranked", 0, n])).rows.map((row, i) => boardRow(row, i + 1, you, assets));
  let mine = null;
  if (you !== null && !rows.some((r) => r.you)) {
    const rank = await ladderRank(client, you, "ranked");
    const row = rank === null ? null : (await client.query(BOARD_SQL, ["ranked", rank - 1, 1])).rows[0];
    if (row) mine = boardRow(row, rank, you, assets);
  }
  return { enabled: true, total, rows, you: mine };
}
```

Inside `presenceRouter`, after the `/v1/mm/ladder` route, add:

```js
  // ---------------------------------------------------------------------------- the leaderboard
  // PUBLIC, unlike the ladder above: the launcher's Online page shows this card signed out too (launcher spec
  // 2026-10-02 §4). A bearer, when sent, only adds the viewer's own row; a missing or bad one is simply anonymous.
  r.get("/v1/leaderboard", async (req, res, next) => {
    try {
      const viewer = (await bearerPlayer(pool, clock, req.headers.authorization))?.player ?? null;
      res.json({ ok: true, ...(await leaderboardFor(pool, { limit: req.query.limit, viewerId: viewer?.id ?? null })) });
    } catch (e) { fail(res, e, next); }
  });
```

- [ ] **Step 4: Run them to see them pass**

Run: `node --env-file-if-exists=.env --test test/leaderboard.test.js test/mm_presence.test.js test/elo_off.test.js test/app.test.js`
Expected: PASS (app.test.js still answers 404 for unknown routes).

- [ ] **Step 5: Commit**

```bash
git add src/mm/teamView.js src/mm/presence.js test/leaderboard.test.js
git commit -m "leaderboard: public GET /v1/leaderboard (top rows, record, Elo, week's change, plays-as team, your row)"
```

---

### Task 17: The opponent's rank, Elo, team and picture in the match view

**Files:**
- Modify: `src/mm/snapshot.js` (imports, the Q3 query, the `opponent` object)
- Create: `test/mm_snapshot_opponent.test.js`

**Interfaces:**
- Consumes: `teamView` (Task 16), `ladderRank` (presence.js), `eloEnabled`, `avatarUrl`.
- Produces: `snapshot.match.opponent = { id, display_name, rating: number|null, rank: number|null, team: object|null, avatar_url: string|null }` — `rating` and `rank` are null while Elo is off; `rank` only for a ranked match. The launcher (Task 5) forwards these as `opponent_elo`, `opponent_rank`, `opponent_team`, `opponent_avatar_url`.

- [ ] **Step 1: Write the failing test**

Create `test/mm_snapshot_opponent.test.js`:

```js
// The matchup card's facts about the opponent (launcher spec 2026-10-02 §3.3): standing only with Elo on.
import { test } from "node:test";
import assert from "node:assert/strict";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { setEloEnabled } from "../src/eloSwitch.js";
import { snapshotFor } from "../src/mm/snapshot.js";
import { liveAssets } from "../src/liveAssets.js";

const mk = async (pool, name, { discord = null, avatar = null, tgid = null } = {}) => (await pool.query(
  `INSERT INTO player (discord_user_id, display_name, avatar_hash, plays_as_tgid) VALUES ($1, $2, $3, $4) RETURNING *`, [discord, name, avatar, tgid])).rows[0];

test("the opponent's standing, team and picture ride on the match view; Elo off hides the standing", async () => {
  const t = await testPool(); const clock = fixedClock();
  try {
    const adam = await mk(t.pool, "Adam");
    const jock = await mk(t.pool, "Jock", { discord: "900000000123", avatar: "0123456789abcdef0123456789abcdef", tgid: 14 });
    const top = await mk(t.pool, "Top");
    await t.pool.query(`INSERT INTO rating (player_id, mode, rating, games, wins, losses) VALUES ($1, 'ranked', 1162, 4, 3, 1), ($2, 'ranked', 1300, 9, 8, 1)`, [jock.id, top.id]);
    const m = (await t.pool.query(`INSERT INTO match (mode, seat0_player, seat1_player) VALUES ('ranked', $1, $2) RETURNING *`, [adam.id, jock.id])).rows[0];
    await t.pool.query(`INSERT INTO mm_seat (player_id, match_id, seat) VALUES ($1, $2, 0), ($3, $2, 1)`, [adam.id, m.id, jock.id]);

    setEloEnabled(false);
    let o = (await snapshotFor(t.pool, clock, adam.id)).match.opponent;
    assert.equal(o.display_name, "Jock");
    assert.equal(o.rating, null, "Elo off: no rating");
    assert.equal(o.rank, null, "Elo off: no rank");
    assert.match(o.avatar_url, /^https:\/\/cdn\.discordapp\.com\/avatars\/900000000123\//);
    assert.equal(o.team?.tgid ?? null, liveAssets.team(14)?.tgid ?? null);

    setEloEnabled(true);
    o = (await snapshotFor(t.pool, clock, adam.id)).match.opponent;
    assert.deepEqual([o.rating, o.rank], [1162, 2]);
    const back = (await snapshotFor(t.pool, clock, jock.id)).match.opponent;
    assert.deepEqual([back.display_name, back.rating, back.rank, back.team, back.avatar_url], ["Adam", null, null, null, null], "an unrated opponent with no picture or team shows none");
  } finally { setEloEnabled(false); await t.close(); }
});
```

- [ ] **Step 2: Run it to see it fail**

Run: `node --env-file-if-exists=.env --test test/mm_snapshot_opponent.test.js`
Expected: FAIL: `o.rank` is `undefined` / `avatar_url` missing.

- [ ] **Step 3: Implement**

In `src/mm/snapshot.js` add to the imports:

```js
import { avatarUrl } from "../pbp/avatar.js";
import { eloEnabled } from "../eloSwitch.js";
import { ladderRank } from "./presence.js";
import { teamView } from "./teamView.js";
```

In the Q3 query, change `p0.display_name AS name0, p1.display_name AS name1,` to:

```sql
              p0.display_name AS name0, p1.display_name AS name1,
              p0.discord_user_id AS discord0, p1.discord_user_id AS discord1,
              p0.avatar_hash AS avatar0, p1.avatar_hash AS avatar1,
              p0.plays_as_tgid AS team0, p1.plays_as_tgid AS team1,
```

Before `snapshot.match = {`, add `const oppId = seat === 0 ? m.seat1_player : m.seat0_player;` and replace the `opponent: {…},` member with:

```js
        opponent: {
          id: oppId,
          display_name: seat === 0 ? m.name1 : m.name0,
          // Elo off until launch (src/eloSwitch.js): no rating and no rank, whatever rows exist. The rank is the
          // leaderboard's (ranked) and only means something in a ranked match.
          rating: eloEnabled() ? num(seat === 0 ? m.rating1 : m.rating0) : null,
          rank: eloEnabled() && m.mode === "ranked" ? await ladderRank(client, oppId, "ranked") : null,
          // The matchup card's logo and picture (launcher spec 2026-10-02 §3.3): the chosen plays-as team only.
          team: teamView(seat === 0 ? m.team1 : m.team0),
          avatar_url: avatarUrl(seat === 0 ? m.discord1 : m.discord0, seat === 0 ? m.avatar1 : m.avatar0, 64),
        },
```

- [ ] **Step 4: Run them to see them pass**

Run: `node --env-file-if-exists=.env --test test/mm_snapshot_opponent.test.js test/mm_snapshot.test.js test/mm_match.test.js test/mm_live.test.js`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mm/snapshot.js test/mm_snapshot_opponent.test.js
git commit -m "match view: the opponent's rank, Elo (Elo on only), plays-as team and Discord picture"
```

---

### Task 18: `GET /v1/rooms/:code`, then the whole suite and the deploy

**Files:**
- Modify: `src/mm/rooms.js` (one route)
- Create: `test/mm_rooms_lookup.test.js`

**Interfaces:**
- Consumes: `normalizeRoomCode`, `SAY.NO_ROOM`, `requirePlayer` (rooms.js).
- Produces: `GET /v1/rooms/:code` (signed in) → `{ ok: true, host_name, mode }` for an open, unexpired room; 404 `{ ok: false, error: "No such lobby." }` otherwise; 401 signed out. The launcher's `mm_room_lookup` (Task 12) reads it.

- [ ] **Step 1: Write the failing test**

Create `test/mm_rooms_lookup.test.js`:

```js
// GET /v1/rooms/:code (launcher spec 2026-10-02 §3.10): who invited me. Only to someone holding the code, signed in;
// only the host's name and the mode.
import test from "node:test";
import assert from "node:assert/strict";
import express from "express";
import { testPool } from "./helpers/db.js";
import { fixedClock } from "./helpers/fakes.js";
import { newToken, hashToken } from "../src/tokens.js";
import { newRoomCode, roomsRouter } from "../src/mm/rooms.js";

let seq = 0;
const player = async (pool, name) => (await pool.query(`INSERT INTO player (discord_user_id, display_name) VALUES ($1, $2) RETURNING *`, [`r${++seq}`, name])).rows[0];
const token = async (pool, p, clock) => {
  const raw = newToken();
  await pool.query(`INSERT INTO device_token (player_id, token_hash, label, created_at, last_used_at) VALUES ($1, $2, 'test', $3, $3)`, [p.id, hashToken(raw), clock.now()]);
  return raw;
};
const room = (pool, code, host, expires, status = "open") => pool.query(
  `INSERT INTO mm_room (code, host_player, mode, status, expires_at) VALUES ($1, $2, 'casual', $3, $4)`, [code, host.id, status, expires]);

test("a signed-in player holding the code learns the host's name and the mode, and nothing else", async () => {
  const t = await testPool(); const clock = fixedClock();
  const app = express();
  app.use(roomsRouter({ pool: t.pool, clock }));
  const server = await new Promise((resolve) => { const s = app.listen(0, () => resolve(s)); });
  const get = async (path, bearer = null) => {
    const res = await fetch(`http://127.0.0.1:${server.address().port}${path}`, { headers: bearer ? { authorization: `Bearer ${bearer}` } : {} });
    return { status: res.status, json: await res.json() };
  };
  try {
    const host = await player(t.pool, "Jock"), me = await player(t.pool, "Adam");
    const tok = await token(t.pool, me, clock);
    const hour = 3_600_000, now = clock.now().getTime();
    const code = newRoomCode();
    await room(t.pool, code, host, new Date(now + hour));
    let r = await get(`/v1/rooms/${code}`, tok);
    assert.equal(r.status, 200);
    assert.deepEqual(r.json, { ok: true, host_name: "Jock", mode: "casual" });
    assert.equal((await get(`/v1/rooms/${code.toLowerCase()}`, tok)).json.host_name, "Jock", "the code reads in any case, as join does");
    assert.equal((await get(`/v1/rooms/${code}`)).status, 401, "signed out: no lookup");
    assert.equal((await get(`/v1/rooms/NOPE`, tok)).status, 404);
    const old = newRoomCode();
    await room(t.pool, old, host, new Date(now - 1000));
    assert.equal((await get(`/v1/rooms/${old}`, tok)).status, 404, "an expired room is no room");
    const started = newRoomCode();
    await room(t.pool, started, host, new Date(now + hour), "started");
    assert.equal((await get(`/v1/rooms/${started}`, tok)).status, 404, "a started room takes no one");
  } finally { await new Promise((r) => server.close(r)); await t.close(); }
});
```

- [ ] **Step 2: Run it to see it fail**

Run: `node --env-file-if-exists=.env --test test/mm_rooms_lookup.test.js`
Expected: FAIL: status 404 for the open room (no route).

- [ ] **Step 3: Implement**

In `src/mm/rooms.js`, inside `roomsRouter`, after the `r.post("/v1/mm/rooms/:code/join", …)` route add:

```js
  // ------------------------------------------------------------------- who invited me (launcher spec 2026-10-02 §3.10)
  // Signed in, by code: the code is what makes a private lobby private, so only someone who already holds it can ask,
  // and the answer is the host's name and the mode -- never the members, the room id or the chat.
  r.get("/v1/rooms/:code", auth, async (req, res, next) => {
    try {
      const code = normalizeRoomCode(req.params.code);
      if (code === null) return res.status(404).json({ ok: false, error: SAY.NO_ROOM });
      const row = (await pool.query(
        `SELECT r.mode, p.display_name AS host_name FROM mm_room r JOIN player p ON p.id = r.host_player
          WHERE r.code = $1 AND r.status = 'open' AND r.expires_at > $2`, [code, clock.now()])).rows[0];
      if (!row) return res.status(404).json({ ok: false, error: SAY.NO_ROOM });
      res.json({ ok: true, host_name: row.host_name, mode: row.mode });
    } catch (e) { fail(res, e, next); }
  });
```

- [ ] **Step 4: Run it, then the whole suite**

Run: `node --env-file-if-exists=.env --test test/mm_rooms_lookup.test.js test/mm_rooms.test.js` then `npm test`
Expected: PASS; the whole suite green (compare any failure with `main` in `../Reborn Backend` before calling it new).

- [ ] **Step 5: Commit**

```bash
git add src/mm/rooms.js test/mm_rooms_lookup.test.js
git commit -m "rooms: GET /v1/rooms/:code names the host and the mode for an invite"
```

- [ ] **Step 6: Deploy (only with Adam's go-ahead)**

Ask Adam before merging. With a yes: merge `feat/online-page-backend` into `main` in `../Reborn Backend` (fast-forward), push, and let Railway deploy as it does for every backend release. Then verify against production:

```bash
curl -s https://api.nflstreetreborn.com/v1/leaderboard
```

Expected: `{"ok":true,"enabled":false,"total":0,"rows":[],"you":null}` (Elo is still off). In the launcher, the Leaderboard card still says "Rankings start soon." and an invite line names its host.

---

# Phase 3: Website

### Task 19: `join.php?r=<code>` opens `nflstreetreborn://room/<code>`

**Files:**
- Modify: `C:\Users\Adam\.claude\jobs\f4d73a58\tmp\site\join.php` (the deployed source; there is no repo for the site on this PC)

**Interfaces:**
- Consumes: the launcher's `room/` link (Task 12) and "Copy invite link" (`https://nflstreetreborn.com/join.php?r=<CODE>`, Task 6).
- Produces: `join.php?r=<code>` shows the code, an "Open in NFL Street Reborn" button to `nflstreetreborn://room/<code>` with the same 700 ms hand-off, and, for a visitor without the launcher, a download button and the code to type under Online → Private room. `join.php?c=<code>` (Netplay) is unchanged.

- [ ] **Step 1: Write the failing check**

Save as `C:\Users\Adam\.claude\jobs\f4d73a58\tmp\site\join-check.sh`:

```bash
#!/usr/bin/env bash
# join.php's three answers, rendered by the PHP CLI (no server needed).
cd "$(dirname "$0")"
render() { php -r '$_GET = json_decode($argv[1], true); include "join.php";' "$1"; }
fail=0
r=$(render '{"r":"K7QP2M"}')
grep -q 'nflstreetreborn://room/K7QP2M' <<<"$r" || { echo "FAIL ?r= does not link room/"; fail=1; }
grep -q 'invited to a room' <<<"$r" || { echo "FAIL ?r= headline"; fail=1; }
grep -q 'download.php' <<<"$r" || { echo "FAIL ?r= has no download button"; fail=1; }
c=$(render '{"c":"AB12CD"}')
grep -q 'nflstreetreborn://join/AB12CD' <<<"$c" || { echo "FAIL ?c= changed"; fail=1; }
grep -q 'Your friend is hosting a game' <<<"$c" || { echo "FAIL ?c= headline changed"; fail=1; }
b=$(render '{"r":"<script>x</script>"}')
grep -q '<script>x' <<<"$b" && { echo "FAIL a hostile code reached the page"; fail=1; }
grep -q 'missing its room code' <<<"$b" || { echo "FAIL a bad code is not refused"; fail=1; }
both=$(render '{"r":"K7QP2M","c":"AB12CD"}')
grep -q 'nflstreetreborn://room/K7QP2M' <<<"$both" || { echo "FAIL ?r= does not win over ?c="; fail=1; }
[ $fail = 0 ] && echo "join-check: PASS" || exit 1
```

Run: `bash "C:/Users/Adam/.claude/jobs/f4d73a58/tmp/site/join-check.sh"`
Expected: `FAIL ?r= does not link room/`, `FAIL ?r= headline`, `FAIL ?r= has no download button`, `FAIL ?r= does not win over ?c=` (and exit 1).

- [ ] **Step 2: Implement**

In `join.php`, replace the four lines from `$raw  = $_GET['c'] ?? '';` through `$link = …;` with:

```php
// ?c=<host code> is a Netplay invite (nflstreetreborn://join/); ?r=<lobby code> is a matchmaking room invite
// (nflstreetreborn://room/, launcher spec 2026-10-02 §3.10). A room code wins when both are given.
$valid = static fn ($v): string => (is_string($v) && preg_match('/^[A-Za-z0-9-]{1,64}$/', $v) === 1) ? $v : '';
$room  = $valid($_GET['r'] ?? '');
$code  = $room !== '' ? $room : $valid($_GET['c'] ?? '');
$link  = $room !== '' ? 'nflstreetreborn://room/' . $room : ($code !== '' ? 'nflstreetreborn://join/' . $code : '');
```

Change the `<title>` to `<title><?= $room !== '' ? 'Join a room' : 'Join a game' ?> · NFL Street Reborn</title>`, add one style rule after `.btn:hover{…}`:

```css
  .btn2{display:inline-block;padding:10px 18px;border-radius:8px;border:1px solid #ffffff2e;color:#fff;text-decoration:none}
```

and replace the block from `  <?php if ($code !== ''): ?>` through `  <?php else: ?>` with:

```php
  <?php if ($room !== ''): ?>
    <h1>You're invited to a room</h1>
    <p>Room code</p>
    <div class="code"><?= $h($room) ?></div>
    <p><a class="btn" href="<?= $h($link) ?>">Open in NFL Street Reborn</a></p>
    <p class="note">The launcher opens on Online with the room ready. Press Join.</p>
    <p class="note">No launcher yet?</p>
    <p><a class="btn2" href="https://nflstreetreborn.com/download.php">Download the launcher</a></p>
    <p class="note">Set it up once, then go to Online, pick Private room and type the code above.</p>
    <script>
      // Hand straight to the launcher; the button stays for a browser that asks first.
      setTimeout(function () { window.location.href = <?= json_encode($link) ?>; }, 700);
    </script>
  <?php elseif ($code !== ''): ?>
    <h1>Your friend is hosting a game</h1>
    <p>Room code</p>
    <div class="code"><?= $h($code) ?></div>
    <p><a class="btn" href="<?= $h($link) ?>">Open in NFL Street Reborn</a></p>
    <p class="note">The launcher opens on Online with the room filled in. Type your name and press Join game.<br>
      No launcher yet? Get it from <a href="https://nflstreetreborn.com/">nflstreetreborn.com</a>, set it up once, then paste the code under Online.</p>
    <script>
      // Hand straight to the launcher; the button stays for a browser that asks first.
      setTimeout(function () { window.location.href = <?= json_encode($link) ?>; }, 700);
    </script>
  <?php else: ?>
```

(The `?c=` branch is today's text, unchanged. The file's comment at the top gains one line: `// https://nflstreetreborn.com/join.php?r=<lobby code> is a matchmaking room invite (nflstreetreborn://room/).`)

- [ ] **Step 3: Run the check**

Run: `bash "C:/Users/Adam/.claude/jobs/f4d73a58/tmp/site/join-check.sh"` and `php -l "C:/Users/Adam/.claude/jobs/f4d73a58/tmp/site/join.php"`
Expected: `join-check: PASS`; `No syntax errors detected`.

- [ ] **Step 4: Deploy (only with Adam's go-ahead)**

Ask Adam first. With a yes, upload and read it back with WinSCP's saved session (FTP; do not pass `/ini=nul`):

```powershell
& "C:\Program Files (x86)\WinSCP\WinSCP.com" /log="C:\Users\Adam\.claude\jobs\f4d73a58\tmp\site\join-deploy.log" /command `
  'open "edits@jacobg44.sg.host.com@ftp.jacobg44.sg.host.com" -rawsettings HostName=ftp.nflstreetreborn.com' `
  'put "C:\Users\Adam\.claude\jobs\f4d73a58\tmp\site\join.php" "/jacobg44.sg-host.com/public_html/join.php"' `
  'get "/jacobg44.sg-host.com/public_html/join.php" "C:\Users\Adam\.claude\jobs\f4d73a58\tmp\site\join.php.deployed-readback"' `
  'exit'
(Get-FileHash C:\Users\Adam\.claude\jobs\f4d73a58\tmp\site\join.php).Hash -eq (Get-FileHash C:\Users\Adam\.claude\jobs\f4d73a58\tmp\site\join.php.deployed-readback).Hash
```

Expected: `True`. Then `curl -s "https://nflstreetreborn.com/join.php?r=K7QP2M" | grep -c "nflstreetreborn://room/K7QP2M"` prints `2` (the button and the hand-off script), and `curl -s "https://nflstreetreborn.com/join.php?c=AB12CD" | grep -c "nflstreetreborn://join/AB12CD"` prints `2`. If the FTP login is refused, check for a VPN first (memory: SiteGround blocks VPN addresses).

---

## Self-review (done while writing; kept for the reviewer)

**Spec coverage.** §2 layout and heading meta: Tasks 2, 8, 10. §3.1 tiles, pick, rows, Find text: Task 4 (+ Task 9 lobbies, Task 2 direct dialog). §3.2 searching: Task 4. §3.3–§3.6 matchup states: Task 5 (standing data: Tasks 5, 17). §3.7 room: Task 6. §3.8 held: Task 7. §3.9 signed out/offline/disabled: Task 7. §3.10 invite links: Tasks 6 (copy), 12 (launcher), 18 (lookup), 19 (site). §4 leaderboard: Tasks 11, 16. §5 Live now: Task 10. §6 controller/keyboard: Tasks 4 (A on tile, focus ring offset), 13. §7 data: Tasks 5, 11, 15, 16, 17 and the hide-what-is-missing rule throughout (`online-model.js`). §8 wording: Tasks 4–7, 10 (check updates listed per task). §9 files: all listed in the file map. §10 out of scope: nothing built for it (the web leaderboard page is flagged in Task 16).

**Placeholder scan.** No TBD/TODO; every code step has its code; every check entry names a state that its task adds.

**Type consistency.** `act -> Promise<boolean>` (Task 4) is what Tasks 8 and 12 await. `tiles(snap, { searching, locked, only })` (Task 4) is what Tasks 7 uses. `openPanel(anchor, { label, build })` (Task 8) is what Task 9 calls. `online-rank` detail `{ elo, playsAs }` (Task 11) matches Task 4's listener and Task 5's reads. `mm_state.match.opponent_elo|opponent_rank|opponent_team|opponent_avatar_url` (Task 5) match the page states and Task 17's `opponent.rating|rank|team|avatar_url`. `leaderboard` answer `{ enabled, total, rows[{ rank, display_name, avatar_url, team, wins, losses, elo, delta_week, you }], you }` is the same in Tasks 3, 11 and 16. `mm_room_lookup` → `{ host_name, mode }` matches Task 18.

**Review Focus.** Each line has its state: `online-focus-keep` (Task 13), `online-lb-hostile` (Task 11), `online-match-bare` (Task 5) and `online-lb-hostile`, `online-invite-busy` / `online-invite-signed-out` (Task 12), `online-small` (Task 2).
