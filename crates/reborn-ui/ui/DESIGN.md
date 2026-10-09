# Locked design spec

**Frosted glass, approved by Adam on 2026-09-30.** A clean platform launcher (Steam, Battle.net) on real OS glass. Every page keeps its layout and shares one set of colours, one type and spacing scale, and one motion system. It replaces "Direction A" (art-led, 2026-09-01). The reasoning is in `docs/superpowers/specs/2026-09-30-launcher-glass-redesign-design.md`.

Every value below lives in `ui/theme.css` and nowhere else. **If a value is not in this file, it is not a decision anyone gets to make in a stylesheet.**

Section 12 lists the checkers that hold this file to the code. A page change follows this file; if the file and the code disagree, fix the file in the same commit.

---

## 1. Tokens

All are defined on `:root` in `theme.css`. Never write the value in a page file; use the token.

### Colour and surface

| Token | Value | Role |
|---|---|---|
| `--mica-tint` | `rgba(18,19,23,.66)` | painted over the OS material when glass is on |
| `--fallback-bg` | radial `#23262d` centre on `#0d0e11` | the solid look: glass off, Linux, Steam Deck, Windows 10 |
| `--glass-1` | `rgba(255,255,255,.045)` | card |
| `--glass-2` | `rgba(255,255,255,.075)` | button, chip, hover |
| `--glass-3` | `rgba(255,255,255,.11)` | selected tab, selected nav item |
| `--stroke` | `rgba(255,255,255,.09)` | every border and row rule |
| `--stroke-hi` | `rgba(255,255,255,.16)` | stronger line: Total rows, floating surfaces |
| `--sheen` | `inset 0 1px 0 rgba(255,255,255,.07)` | top highlight of a glass surface |
| `--shadow` | `0 12px 40px -12px rgba(0,0,0,.55)` | a card's drop shadow |
| `--fg` | `#F4F5F7` | text |
| `--fg-2` | `#AEB5C0` | secondary text |
| `--fg-3` | `#7C8491` | labels, meta, table headers |
| `--red` | `#E0272E` | the accent, as a **fill**: nav marker, switch on, dots |
| `--red-hi` | `#F04A50` | hover on a red fill |
| `--red-ink` | `#FF7A7F` | red as **text**, and L |
| `--ok` | `#3DDC97` | W, "Ready", Elo up, online |
| `--warn` | `#FFC46B` | "Setup required", warnings |
| `--dolphin` | `#22C7B4` | the Dolphin tile, nothing else |

W is `--ok`, L is `--red-ink`. There is no T: ties do not exist.

### Shape

| Token | Value | Role |
|---|---|---|
| `--r-lg` | 14px | cards, menus, dialogs |
| `--r-md` | 10px | buttons, fields |
| `--r-sm` | 8px | small controls, tabs |

### Motion

| Token | Value | Role |
|---|---|---|
| `--ease` | `cubic-bezier(.2,.8,.2,1)` | the only curve |
| `--d1` | 480 ms slow, 120 ms quick | hover, press |
| `--d2` | 720 ms slow, 180 ms quick | menus, switches, tabs |
| `--d3` | 960 ms slow, 240 ms quick | page enter, save bar, reveals, nav marker |

### Surfaces with a job of their own

| Token | Value | Used by |
|---|---|---|
| `--field-bg` | `rgba(0,0,0,.25)` | `.field`, text inputs, selects |
| `--hero-scrim` | two linear gradients | Play hero text |
| `--on-art` | `rgba(10,11,14,.35)` | chips on art |
| `--avatar-bg` | `135deg #5865F2 to #3b44b8` | Discord avatar with no picture |
| `--av-0` to `--av-7`, `--av-cpu` | 135deg gradients | letter avatars, one colour per player name (`match-rows.js`) |
| `--thumb-bg` | 160deg `rgba(255,255,255,.05)` to `rgba(0,0,0,.18)` | the neutral mod preview. No drawn art |
| `--surface-solid` | `#1b1e24` | an opaque popover over content (the controller binding editor) |
| `--tile-scrim` | 0deg `rgba(6,7,9,.9)` to transparent | the Online mode tiles' picture scrim |
| `--vs-scrim` | 0deg `rgba(6,7,9,.95)` to `.55` | the Online matchup card's picture scrim |
| `--tile-ranked` | 150deg `#8a161c` to `#0c0c0e` | the Ranked card (no picture yet) |
| `--tile-casual` | 150deg `#17408a` to `#0b0c0f` | the Casual card (no picture yet) |
| `--tile-private` | 150deg `#15703f` to `#0b0d0c` | the Private room card (no picture yet) |
| `--vs-bg` | 150deg `#34373e` to `#0f1013` | the matchup card (no picture yet) |
| `--pill-bg` | `rgba(10,11,14,.6)` | a pill on art (Online "searching" counts, the relay step) |
| `--lock-bg` | `rgba(13,14,17,.72)` | the Online sign-in panel over the tiles |
| `--red-wash` | `rgba(224,39,46,.12)` | your own row on the Online leaderboard |

### Stats and Profile components

| Token | Value | Used by |
|---|---|---|
| `--face-ring` | `0 0 0 2px rgba(255,255,255,.12)` | portrait ring |
| `--badge-bg` | `#16171c` | the team-logo badge on a portrait |
| `--dot-ring` | `#1a1b21` | the presence dot's ring: the card colour it sits on |
| `--team-fallback` | `#2a2d34` | portrait and cover while the team or avatar colour is unknown |
| `--cover-c2` | `#15161a` | cover second colour fallback |
| `--cover-pic-c1`, `--cover-pic-c2` | `#3a3d45`, `#1b1c21` | cover avatar fallbacks |
| `--cover-pic-shadow` | ring + soft drop | the cover avatar |
| `--cover-mask` | 100deg black to transparent | the cover wash fading to the right |
| `--mix-light`, `--mix-dark` | `#fff`, `#000` | `color-mix` ends for portrait and cover gradients |
| `--game-row-bg` | `rgba(0,0,0,.14)` | a best-of-3 game row |
| `--rule` | `rgba(255,255,255,.055)` | a rule inside a v2 card (table rows, cell borders; mv2 mockup) |
| `--red-deep` | `#b81f25` | the dark end of a red share bar (Win rate by team) |
| `--chart-grey` | `rgba(174,181,192,.45)` | a chart's second series: points allowed, the grey legend key |
| `--chart-grid`, `--chart-base` | `rgba(255,255,255,.06)`, `.14` | a chart's grid lines and its zero line |
| `--chart-hole` | `#0d0e11` | the hole a chart dot sits in (the window colour) |
| `--league-cell-w` | `184px` | one week-scoreboard cell (League) |
| `--mine-edge` | `inset 3px 0 0 var(--red)` | your game, your row and the Update attention mark: a 3px red left edge, never a glow (League) |

Set per element, not on `:root`: `--tc` (portrait team colour, from the server; on the League tab your team's colour), `--oc` (the League tab's opponent colour; both only as an exact `#RRGGBB`), `--c1`/`--c2` (cover colours from the Discord avatar, `cover-color.js`), `--menu-x`/`--menu-y` (menu position, `menu.js`), `--rail` (sidebar width), `--hint-height` (hint bar, 0px or 34px).

Colour rules:

- The accent is red. No purple, no teal accent.
- `--red` is a fill. Red text is `--red-ink`.

## 2. Material and fallback

The page background is painted on `<html>`, so a short body never shows nothing:

| `html[data-glass]` | Background |
|---|---|
| `"on"` | `--mica-tint` over the OS material |
| `"off"` | `--fallback-bg` |

`body` stays transparent so the tint is painted once.

| Platform | Material |
|---|---|
| Windows 11 | Mica (`launcher_look.rs`, build 22000 and up) |
| Windows 10 | the solid look (`WIN10_ACRYLIC` is false) |
| macOS | vibrancy |
| Linux, Steam Deck, glass turned off | the solid look |

Who sets `data-glass`:

- `look-boot.js` is a classic script that runs before first paint. It reads `localStorage["reborn:look"]` and sets `data-glass` and `data-motion`. Glass defaults **off** and slow motion **on**, so a window is never transparent with nothing behind it.
- `look.js` asks the core (`launcher_look`) and sets both from the answer: `data-glass="on"` only when `glassApplied` is true. It stores the answer for the next first paint. If a glass preview was never saved, the first read puts the window back to the saved state (`window_glass_apply`).
- `html[data-motion="slow|quick"]` is set the same way (section 4).
- Settings > Display > Launcher window has **Window glass** and **Slow animations**. Both preview live and go through the save bar.

Never paint a transparent window unless `data-glass="on"` is set. Never use a flat fill in place of glass without the fallback look. Floating surfaces (dialogs, menus, the update card, the save bar) use `rgba(24,25,30,.92)` or `.78` with `backdrop-filter`, because content moves behind them and `--glass-1` is too thin.

## 3. Type, spacing, size

- **Inter** (`fonts/InterVariable.woff2`) for everything. Anton and Saira are not used.
- **Weights:** 500 UI text, 600 labels and values, 700 headline numbers and page titles. **Nothing above 700.**
- **Sizes:** page title 30/1.1 at 700 (exception, ruling P4-29: the pages not redesigned, Recovery, game, live and onboarding, keep the legacy page title, 28px at 600, from their layout sheets; spec §9); section title 16 at 600; table header 10.5 at 600, uppercase, tracking .06em; label 11 at 600, uppercase, tracking .07em; body 13 to 14; meta 12 to 12.5.
- Numbers use `font-variant-numeric: tabular-nums` (`.num`).
- **Spacing:** 8, 16, 24. 24 between sections and columns; 16 inside cards; 8 inside rows. Stats and Profile (v2 grid, mv2 mockup) use 16 between cards and 18 inside them.
- **Section headers are 40px** (`.sec-head`), whatever they hold, so neighbouring cards start on one line.
- **Rows:** match row 56, game row 40, table row 40, table header 36, nav item 40 (36 in short windows).
- **Form column:** Settings, Mods and Music are at most 942px wide. Stats and Profile use the full content width.
- **Logos:** `art/nsr-sidebar.png` and `art/play-logo.png` are Adam's. The sidebar logo slot is 124px tall with the image at 100% width. Never resize, stretch or redraw a logo.
- **Display copies:** pages load `art/nsr-sidebar-display.png` (512 px), `art/play-logo-display.png` (1100 px wide) and `art/play-background-display.jpg` (2560x1440), made from the full-size originals above, never the originals (decoding a 4096 px PNG on every page made pages slow). When an original is replaced, remake its copy at the same size (Pillow LANCZOS; the background as JPEG quality 90).
- **No glows.** No coloured `box-shadow`. The primary button's shadow is a plain dark drop.

## 4. Motion

- One curve, `--ease`. Three speeds, `--d1`, `--d2`, `--d3` (section 1).
- Only `opacity` and `transform` animate. Heights use `grid-template-rows` (`.rows-reveal`) or `max-height` (`.save-lane`). Never animate `width`, `left`, `top` or `height`.
- `html[data-motion="slow"]` and `[data-motion="quick"]` set `--d1`, `--d2`, `--d3` **directly**. There is no multiplier variable: a custom property resolves where it is declared, so a multiplier would not reach the tokens.
- **Reduced motion** (`prefers-reduced-motion: reduce`) sets all three to 0ms, and every animation to `none` and every transition to 0s. The OS setting always wins over `data-motion`.
- Hover and press only move (`transform`). Colours change instantly.
- A page change is a cross-document view transition (`@view-transition{navigation:auto}`): the old page holds until the new one draws, the sidebar (`view-transition-name: nav`) swaps in place, and only `#main` (`page`) animates: the old fades out over `--d1`, the new rises in with `look-enter` over `--d3` (Play: `look-enter-fade`, opacity only). `#main` has no animation of its own, so it is never a containing block for `position:fixed` UI. Reduced motion: instant.
- **Containing block:** a transform on `#main` makes it the containing block for `position:fixed` children. So `position:fixed` UI (menus, dialogs, the update dock) is attached to `<body>`, never inside `#main`. `menu.js` appends its `<dialog>` to `<body>` for this reason.
- No bounce, count-up numbers, parallax or looping motion. The two loops are `.live-dot` and `.sweep` (a red line along the bottom of a surface while something is moving; Online, spec 2026-10-02).

## 5. Shell

- **Grid:** `body.shell` is a grid of the sidebar (`--rail`: 184px, 160px under 1000px wide, 68px under 650px) and the page. Each page scrolls inside `#main`; the sidebar never scrolls with it.
- **Sidebar:** logo slot, then Play, Mods, Music, Controllers, Online, League (league members only, added by `shell.js` from `league_member`), Stats, with Settings and Help pinned to the bottom. Hover is `--glass-1`; the current item is `--glass-3` with `--sheen`. **One** `.nav-marker` (3px, `--red`) glides to the current item (`look.js`, `mountNavMarker`); there is no per-item marker.
- **Focus ring:** 2px solid white, 3px offset, never a glow. It shows on `:focus-visible` and on `.pad-focus` while the pad drives.
- **Controller mode** is `body.gamepad-input` (`app-shell.js`): on while the last input was a pad, off on any mouse move, click or key.
- **Hint bar** (`.input-footer`, `#input-hints`): it exists only while there is a prompt. An empty `#input-hints` sets `--hint-height:0px` and hides the bar; a filled one sets `34px`. Every page ships `<span id="input-hints"></span>` empty. The hints come from `app-shell.js` and appear only in controller mode.
- The sidebar, frame, hint bar and save lane are defined once in `theme.css` and are identical on every page (`html body.shell` prefix to outrank page sheets).

## 6. Components

One definition each, in `theme.css`. A page file may place them, never restyle them.

| Class | What | File |
|---|---|---|
| `.glass` | card: `--glass-1`, `--stroke`, 14px radius, `--sheen` and `--shadow`; blur 24px saturate 1.3 on Play only (over the hero art) | Elsewhere the page fade-in made the blur snap on after a second; there is nothing behind a card to blur there |
| `.btn`, `.btn.primary`, `.btn.quiet`, `.btn.sm` | 38px glass button; red gradient primary; transparent; 32px | |
| `.look-switch` | 40 by 22 switch, `aria-checked`, thumb slides, on = `--red` | `settings.js` |
| `.field` | recessed 38px box for a value, with an optional `.auto` tag (green) | |
| `.tabs`, `.tab`, `.tab-ind` | segmented control, one highlight that slides with `translateX` and `scaleX` | `tabs.js` |
| `dialog.menu` | anchored glass list in the top layer, check on the current option, D-pad inside it, B closes; `dialog.menu.panel` (280px, `openPanel()`) holds any content the same way (Online's players and lobbies) | `menu.js` |
| `.save-lane`, `.save-bar` | the save bar in its own lane | `savebar.js`, `settings-draft.js` |
| `.sec-head`, `.sec-link`, `.k-label` | 40px section header, its link, an uppercase label | |
| `.m-list`, `.m-row`, `.m-hd`, `.m-game`, `.m-pager` | the match list of Stats and Profile (grid columns 84, 92, flexible, 44, 56, 14) | `match-rows.js` |
| `.rows-reveal`, `.chev` | a best of 3 opening in place; the chevron turns | `match-rows.js` |
| `.tbl`, `tr.total` | table: uppercase header, fixed 220px name column shared by every table, Total row with a `--stroke-hi` rule | |
| `.avatar`, `.av-0` to `.av-7`, `.av-cpu` | round Discord picture, or the first letter | `match-rows.js` |
| `.player-face` (`.face`, `.badge`) | round portrait on the team colour (`--tc`), team logo badge bottom right | |
| `.team-logo` | the backend's full-colour logo, or the abbreviation (`.text`) | |
| `.cover-wash`, `.cover-pic`, `.presence-dot`, `.live-dot` | profile cover from the avatar colours, its avatar, the online dot, the pulsing live dot | `cover-color.js` |
| `.sweep` | a 2px red line sweeping along the bottom of its positioned parent; transform only | Online |
| `.refusal`, `.partial`, `.said` | messages: red, amber, green | |
| `.facts` | label and value table | |
| `.vgrid` (`.s3` to `.s12`) | Stats and Profile v2: one 12-column grid, 16px gutters, every row's cards stretch to one height (one column under 1100px) | `stats.js`, `profile.js` |
| `.vc`, `.vc-h`, `.vc-aside`, `.vc-b`, `.vc-th`, `.vc-rows`, `.vc-tr`, `.vc-cells`, `.vc-kv` | a v2 card: 48px header (15px title at 600; aside, link or `.tabs.sm`), a body; table rows and cells grow to fill the card, so no row leaves dead space | `stats-cards.js` |
| `.tabs.sm` | the small segmented control in a card header (26px tabs) | `tabs.js` |
| `.qm` (`.ranked`, `.casual`, `.off`) | the queue word: Ranked with a red dot, Casual with a ring, Offline plain | `match-rows.js` |
| `.wl-strip` | the last results, one W or L square each | `stats-cards.js` |
| `.share` (`.red`) | a share bar, the fill scaled with `transform` (never sized) | `stats-cards.js` |
| `.pl-row`, `.pl-cells` | a player: portrait, name, role, up to five numbers with their words | `stats-cards.js` |
| `.ch`, `.ch-line`, `.ch-bars`, `.ch-mini`, `.ch-donut`, `.ch-days`, `.ch-legend` | charts: shapes in an SVG stretched to its box (non-scaling strokes), every word and dot in HTML so nothing stretches | `stats-cards.js` |
| `.update-card`, `.update-dialog`, `.update-center` | update notice, fixed to the bottom right | `update-notice.js` |

Buttons with no class, from shared scripts (`update-notice`, `menu-pad-setup`), are styled like `.btn` by `body.themed button:not([class])`. The plain `button` element rule sits in `:where(body.shell)` (specificity 0), so any component class wins without a page prefix.

### The save bar

Used by Settings (a page-local draft) and Music (changes the core holds).

- `<body>` has class `has-save-lane`. `theme.css` gives the grid its extra `save` row only then.
- The lane is an empty `<div id="save-lane" inert></div>` right after `#main`. **The id must be exactly `save-lane`.** `app-shell.js` finds it by id.
- `mountSaveBar(lane, { onSave, onDiscard, returnFocus, noun = "unsaved change", saveText = "Save changes", keepsOnLeave = false })` returns `{ update(changes), fail(message), lock(saveHeld, discardHeld = saveHeld) }`.
  - `update(changes)` takes the list of change names. The bar shows `N <noun>(s)` and the last name, opens when the list is not empty, and is `inert` while closed.
  - `fail(message)` shows a reason in the bar (two lines at most, full text in the tooltip). A save or discard that throws is shown there and the bar stays open.
  - `lock()` keeps the buttons disabled until `lock(false)`. With both disabled the `.save-bar` itself gets `tabindex="-1"`, and `app-shell.js` focuses it, so focus never goes nowhere.
- The bar sits in its own lane, never over content. A window-bottom update dock rises above an open bar.
- Music passes `noun: "pending change"`, `saveText: "Apply changes"`, `keepsOnLeave: true`.

**Leaving a page with the bar open:**

| Page | `keepsOnLeave` | Leaving |
|---|---|---|
| Settings | false | blocked. The draft lives in the page, so it would be lost |
| Music | true | allowed. The lane gets `data-keeps`; the core holds the staged songs |

The guards (`app-shell.js`, `blocksLeave()`) are: a click on a link that leaves the page, **B twice** (B from content goes to the sidebar; B from the sidebar would leave), **LB/RB** (bumpers do nothing), and update-notice's **View updates** (it asks `window.pointAtSaveBar`). When blocked, focus goes to Save (`toSaveBar`) and `.save-bar.called` outlines it for 800 ms. In-page `#` links and links opened elsewhere still work.

### The settings draft

`settings-draft.js` is pure (no DOM): `createDraft()` holds pending Settings changes (`put`, `drop`, `has`, `value`, `changes`, `saveAll`, `discardAll`). Each entry brings its own save and, for a live preview, its own revert. One save or discard runs at a time. A save stops at the first failure or `HELD`; a failed revert stops the discard, so nothing still previewed is shown as discarded.

## 7. Pad navigation opt-ins

`app-shell.js`. The default is **column-first**: Up and Down pick the nearest control by position. Controllers depends on this; do not change it.

| Attribute | Where | Effect |
|---|---|---|
| `data-nav="rows"` | Settings and Mods `#screen`; Profile's Back button and its card | Up and Down prefer the nearest row, so a far control in the same column does not skip nearer rows |
| `[data-nav-seq]` | Profile name block (Plays as, Share profile, View on web) | inside a `data-nav="rows"` area, Up and Down step through the block in reading order |
| `data-nav="column"` | Profile's Match history | Up and Down stay in the block while it has a control that way, so the last row's Down is the pager |
| `pad-back` event | dispatched on `document` by B / Esc, cancelable | a page that calls `preventDefault()` answers B itself (Online: Cancel while searching, Decline on Match found); otherwise B goes to the sidebar as always |

## 8. Page files and CSS architecture

- Each page loads exactly `theme.css` then its page file, and `<body class="shell themed …">`.
- A page file holds **layout only**: no colour literals (no hex, `rgb()`, `hsl()`), no weight above 700, no glow, no new duration.

| Page | Page file |
|---|---|
| `index.html` (Play) | `pages/play.css` |
| `settings.html`, `mods.html`, `music.html`, `controls.html` | `pages/settings.css`, `mods.css`, `music.css`, `controls.css` |
| `stats.html`, `profile.html` | `pages/stats.css`, `pages/profile.css` |
| `recovery.html` | `pages/recovery.css` |
| `online.html` | `pages/online.css` |
| `league.html` | `pages/league.css` |
| `leaderboard.html` (the full leaderboard, from Online) | `pages/leaderboard.css` |
| `match.html` (every match and game) | `pages/game.css` |
| `onboarding.html` | `onboarding.css` |
| `harness.html` | `harness.css` (the developer page only; its own plain look) |

- The legacy sheets (`styles.css`, `launcher.css`, `product-ui.css`, `finish.css`, `shell.css`, `players.css`, `onboarding-polish.css`, `live-view.css`, and the old `play.css`, `stats.css`, `music.css`, `profile.css`) are retired and must stay gone.
- Pages not redesigned in plan 4 (recovery, onboarding) carry the layout their old sheets gave them (made once by `Tools/extract-layout-css.mjs`), on theme tokens. Online was redesigned on 2026-10-02 (`docs/superpowers/specs/2026-10-02-online-page-redesign-design.md`); `pages/online.css` is written by hand.
- **The one allowed second stylesheet:** the Controllers page's pad art. `controller-preview.js` loads `art/controllers/<pad>/preview.css` at runtime (one `<link data-controller-art>` per pad folder) to place that pad's preview art. No other page or script adds a stylesheet.

## 9. Words

- "Leaderboard", never "ladder". "Live feed", not "Watch live". "Last 10 games", not "Form".
- A row never says "Best of 3" (every ranked match is one). Online's Live now card says "Ranked · Game 2" or "Casual" (spec 2026-10-02 §5), and its "Live feed" opens the match page (match.html, the site's /live/<match>), which follows a live match play by play.
- A player's name (an account, never an NFL roster player) is a link to their profile wherever it shows (`player-link.js`, `a.player-link`): it keeps the look of the name and underlines on hover. There are no halves.
- Stat units are uppercase in tables (YDS, TD, RTG, C/ATT, CAR, YCO, DRP) and spelled out in headline tiles (Passing Yards, Passing TD, Passer Rating).
- Records are W-L. Private rooms never show on a profile and never count.

## 10. Never

- A glow, anywhere. A coloured `box-shadow`.
- A colour, size or duration in a page file that is not a `theme.css` token.
- Drawn or invented art: an empty preview stays neutral (`--thumb-bg`). Every number comes from a command, never from the mockup.
- Server text through `innerHTML`, or an image from anywhere but `assetUrl()` and the Discord avatar prefix (`avatarSrc`).
- Resizing, stretching or redrawing Adam's logos.
- Content under a `position:fixed` rule inside `#main` (section 4).
- A page that loads a retired sheet, or a second stylesheet (the only exception: the controller art `preview.css` in section 8).

## 11. Adding to the system

1. New colour, size, duration or component: add the token or class to `theme.css`, document it here, add it to the right checker. Then use it.
2. New page: load `theme.css` and its page file, give `<body>` `shell themed page-<name>`, add an empty `<span id="input-hints"></span>`, add the page to `THEMED` in `Tools/check-theme.mjs`.
3. New page with a save bar: add `has-save-lane` and `<div id="save-lane" inert></div>` after `#main`.

## 12. Checks

Run from the repo root with `node Tools/<name>.mjs`. The browser checks need Playwright and Edge.

| Check | Guards |
|---|---|
| `check-theme` | every page loads exactly `theme.css` + its page file; tokens exist; motion values and reduced motion; no weight above 700 or red glow in `theme.css`; legacy sheets gone; this file keeps its warning and names every token |
| `check-theme-tokens` | page files: no colour literals, no weight above 700 |
| `check-themed-shell` | the shell (sidebar, frame, hint bar) on a themed page and on `online.html` |
| `check-save-lane` | the lane's own row, failure shown in the bar, pad reaches the bar, LB/RB and links blocked while changes are pending |
| `check-menu` | menu anchored to its button, check on the current option, keys and pad, closing |
| `check-tabs` | the sliding highlight uses transform only, at `--d2`, and follows its tab when the window crosses the 900px rule |
| `check-glass-pages` | Play and Settings in each look (glass, motion, controller mode, save bar), alignment, one logo size |
| `check-glass-mods-music-controls` | Mods, Music and Controllers in each look, the Music save bar |
| `check-stat-components` | the Stats and Profile components in `theme.css`: headers, rows, table grid, portrait, cover, reveal, live dot |
| `check-stats-page` | the Stats page: aligned columns, match list, W/L only, reveal, small window, signed out, refusals |
| `check-profile-page` | the Profile page: card, overview, history, Full stats, pad moves, edge cases |
| `check-pad-columns` | pad moves stay in their column outside Settings and Mods |
| `check-controller-hints` | the hint bar appears with pad input and goes with mouse or key |
| `check-look-browser` | glass and motion preview, Discard restores, the save lane sits below `#main` |
| `compare-page-layout` | element-by-element layout before and after a stylesheet change (`capture`, `diff`) |
| `lib/page-states.mjs` | the fixed page states those checks open (frozen clock, no network, optional pad) |
| `baseline-messages` | compares checker failure messages between two recorded runs |

Also useful: `check-shell-chrome`, `check-shell-pages` (every page survives every core answer) and `audit-ui`.

The page models have node unit tests. Run them all with the files as a glob (not the directory):

```
node --test crates/reborn-ui/ui/*.test.mjs Tools/lib/*.test.mjs
```

| Test | Covers |
|---|---|
| `profile-model.test.mjs` | Profile's model: overview columns, live card, status line, career tiles, leaderboard rows, private rooms dropped |
| `settings-draft.test.mjs` | the Settings draft behind the save bar |
| `stats-model.test.mjs` | Stats' model: numbers, Elo, trend, role cards, private rooms dropped |
| `stats-rows.test.mjs` | match row models, the results-queue line and unsent counts |
| `online-model.test.mjs` | Online's words and numbers: mode and series lines, accept countdown, Elo bits (hidden with Elo off), held notice, invite link, leaderboard rows |
| `league-model.test.mjs` | League's model: page mode, card states, install/play gate, scoreboard cells, rows |
| `league-nav.test.mjs` | the League link: members only, after Online, never twice |
| `cover-color.test.mjs` | the cover's two colours from an avatar |
| `outcome-words.test.mjs` | onboarding's outcome words (a bad disc or a missing patch is never reported as success) |
| `Tools/lib/colour-tokens.test.mjs` | the colour-to-token mapping `extract-layout-css` uses |
