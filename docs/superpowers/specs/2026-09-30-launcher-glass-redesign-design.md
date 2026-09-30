# Launcher glass redesign: design spec

2026-09-30 · branch `feat/launcher-glass-redesign` (from `feat/stats-profile-v2` @ `6ba10f7`) · worktree `NFL Street Reborn Launcher redesign`

**Status: spec approved by Adam on 2026-09-30 ("I am ready for the spec"). No product code has been written.**

- Reference mockup: `docs/superpowers/specs/assets/2026-09-30-launcher-glass-mockup.html`.
  - Open it in Edge. It loads art through absolute `file:///` paths into the `rel-pbp` worktree and `reborn-backend-stats-v2`.
- Backend work list: `C:\Users\Adam\Desktop\Reborn Installer\BACKEND-CHANGES-launcher-redesign.md`.

---

## 1. Goal

Adam: *"It's not bad, it just needs to be way better."* Then: *"I want the layout to stay the same, just a more modern look UI, frosted glass looking, premium."*

- **Same layout, new look.**
  - Every page keeps its place, its sidebar item and its job.
  - Every page gets one design system: frosted glass, one set of colours, one type and spacing scale, one motion system.
- **The exception is Stats and Profile.** They join from the Stats v2 branch (handoff `REDESIGN-HANDOFF-stats-profile.md`). Their content was reworked in review, and that content is part of this spec (§8.6, §8.7).

### Success looks like
1. Every page sits on one token set. **Today** there are four `--bg` values, three panel colours and two reds plus an `--accent`, and every page stacks 5–7 stylesheets, including `onboarding.css` on pages that aren't onboarding.
2. The window is real OS glass where the OS has it, and looks deliberate where it doesn't.
3. Nothing a player could do before is lost. The handoff's behaviour contract (§10) still holds.
4. Adam signs off each page against the mockup.

### Not part of this spec
- **Online page:** redesigned separately.
- **Debugging:** Adam: *"a ton of bugs everywhere but mainly with controller, however I'd rather you just work on the redesign."* The one exception is §6.3, where the redesign itself changes how controller prompts appear.
- **Logos:** Adam replaces the sidebar logo and fixes the Play wordmark himself. **Don't resize, stretch or redraw either one.**

---

## 2. Decisions log (Adam, 2026-09-30)

| Topic | Decision |
|---|---|
| Direction | Clean platform (Steam/Battle.net) + frosted glass. **Supersedes `crates/reborn-ui/ui/DESIGN.md` "Direction A"**, which gets rewritten (§11). |
| Layout | Unchanged. All 8 nav items kept (Play, Mods, Music, Controllers, Online, Stats, Settings, Help). Settings keeps all 7 tabs. |
| Glass | Real OS material (Mica on Win 11, Acrylic on Win 10, vibrancy on macOS). **Fallback = the solid dark look**, near-black `#0d0e11` with a soft `#23262d` centre, no art. Players can turn glass off at any time. |
| Accent | Red `#E0272E`. Purple was tried and rejected. |
| Glow | **None anywhere:** no glowing buttons, focus rings or switches. |
| Motion | Slow, smooth animations **on by default**, with a setting to switch them off (§5). |
| Icons | Keep the launcher's current icons. My controller icon was rejected. |
| Mock art | None. No drawn footballs or invented previews. Empty preview areas stay neutral. |
| Controller prompts | Shown **only while a controller is driving the launcher** (§6.3). |
| Stat units | Uppercase in tables (YDS, TD, RTG, C/ATT, CAR for rushing attempts, YCO, DRP). Spelled out in headline tiles (Passing Yards, Passing TD, Passer Rating). |
| Records | **No ties exist.** W–L only. |
| Private rooms | Never shown on public profiles and never counted toward anything (backend §1–§3). |
| Best of 3 | Every ranked match is a best of 3, so rows never say "Best of 3". The live card says "Game 2 of 3". There are no halves. |
| Naming | "Leaderboard", never "ladder". "Live feed", not "Watch live". "Last 10 games", not "Form". |
| Totals | A normal Total row in each table, in the same style as the data. This **overrides** the Stats v2 handoff's "one plain muted line". |

---

## 3. Design tokens

All colours, sizes and durations live in **one** file, `ui/theme.css` (§11). Pages never define colours.

### 3.1 Colour
```css
:root {
  /* material */
  --mica-tint: rgba(18,19,23,.66);          /* painted over the OS material */
  --fallback-bg: radial-gradient(60% 60% at 50% 40%, #23262d 0, transparent 70%), #0d0e11;
  /* glass surfaces (on the material) */
  --glass-1: rgba(255,255,255,.045);         /* card */
  --glass-2: rgba(255,255,255,.075);         /* button, chip, hover */
  --glass-3: rgba(255,255,255,.11);          /* selected tab / nav item */
  --stroke: rgba(255,255,255,.09);
  --stroke-hi: rgba(255,255,255,.16);
  --sheen: inset 0 1px 0 rgba(255,255,255,.07);
  --shadow: 0 12px 40px -12px rgba(0,0,0,.55);
  /* text */
  --fg: #F4F5F7;  --fg-2: #AEB5C0;  --fg-3: #7C8491;
  /* brand + state */
  --red: #E0272E;  --red-hi: #F04A50;  --red-ink: #FF7A7F;   /* red is a fill; --red-ink is red text */
  --ok: #3DDC97;   --warn: #FFC46B;
}
```
- **W** is `--ok`, **L** is `--red-ink`. There is no T.
- Opponent avatars and the profile cover take colours from the Discord avatar (`--c1`/`--c2`, set by `cover-color.js`, existing).
- **Portrait backgrounds** are the team's `primary`, dimmed:
  `radial-gradient(circle at 50% 30%, color-mix(in srgb, var(--tc) 70%, #fff 8%), color-mix(in srgb, var(--tc) 55%, #000))`.
  The portraits are transparent PNGs. `--tc` comes from the team's `primary` (backend §10).

### 3.2 Type
- **Inter** (bundled `InterVariable.woff2`) for everything.
- **Weights:** 500 for UI text, 600 for values and labels, 700 for headline numbers only. **Nothing heavier than 700** (Stats v2 handoff).
- **Sizes:**
  - Page title 30/1.1 at 700.
  - Section title 16 at 600.
  - Table header 10.5 at 600, uppercase, tracking .06–.07em.
  - Body 13–14.
  - Meta 12–12.5.
- Numbers always use `font-variant-numeric: tabular-nums`.
- Anton and Saira are no longer used in the shell. The logos are images.

### 3.3 Spacing, radius, size
- **Spacing:** 8 / 16 / 24 only.
  - 24 between sections and between columns.
  - 16 inside cards and between a section title and its content.
  - 8 inside rows.
- **Section headers** are 40px tall, whatever they contain (title only, or title + tabs), so neighbouring columns' cards start on the same line.
- **Radius:** 14 for cards, 10 for buttons and fields, 8 for small controls, 999 for switches and avatars.
- **Row heights:** match rows 56, game sub-rows 40, player rows 64, table rows about 40.
- **Content width:** `max-width: 1014px` for form pages (Settings, Mods, Music, Controllers). Stats and Profile use the full content width.

### 3.4 Components (one definition each, in `theme.css`)
- **`.glass` card:** `--glass-1` fill, `--stroke` border, radius 14, `--sheen` and `--shadow`, `backdrop-filter: blur(24px) saturate(1.3)`.
- **Buttons:**
  - `.btn`: `--glass-2` fill, 38px tall.
  - `.btn.primary`: red gradient `#ec3a41 → #c91f26`, plain drop shadow, **no glow**.
  - `.btn.quiet`: transparent.
  - `.btn.sm`: 32px.
- **Switch:** 40×22. The thumb slides; on = red, off = `rgba(255,255,255,.14)`.
- **Segmented tabs** (`.tabs`): a track at `rgba(0,0,0,.22)`, with one indicator that slides between options.
- **Menu:** anchored glass popover with a check on the current option. Used by Play's Graphics and Display cells.
- **Save bar:** floating glass card, docked in its own footer lane. It appears **only when something has changed** and never covers content (the scroll area ends above it).
- **Table** (`.tbl`):
  - Uppercase header row.
  - Rows split by `--stroke`.
  - The first column is the name with a team logo.
  - Total row with a `--stroke-hi` top border.
  - `table-layout: fixed`, with a 220px first column shared by every table on a page so columns line up.
- **Avatar:** a round Discord picture, or the first letter on the avatar's colours.
- **Portrait:** round, on the team colour, with the team logo as a badge at the bottom right.
- **Team logo:** full-colour backend logos (`/live-assets/teams/<id>-128.png` through `assetUrl()`). The launcher's own `art/teams/*.png` are dimmed versions and are **not used** in Stats or Profile.

---

## 4. Window material (glass)

### 4.1 Platforms
| Platform | Material | How |
|---|---|---|
| Windows 11 | Mica | Tauri 2 `windowEffects: { effects: ["mica"] }` + `transparent: true` |
| Windows 10 | Acrylic | `effects: ["acrylic"]`. Needs a feasibility check: Acrylic can lag while dragging on some Win 10 builds. If it does, Win 10 uses the fallback. |
| macOS | Vibrancy (`underWindowBackground`) | `windowEffects` + `app.macOSPrivateApi: true` + transparent window |
| Linux / Steam Deck | **None available**, so the **fallback** | Tauri has no Linux window blur, and Deck gaming mode has no compositor blur |
| Glass turned off (any OS) | Fallback | Same as Linux |

- **With glass,** `body` paints `--mica-tint` over the OS material.
- **With the fallback,** `body` paints `--fallback-bg` (exactly the mockup's "Dark" wallpaper look).
- **The Play hero art** is unaffected either way; it covers its own area.

### 4.2 Config and code
- **`tauri.conf.json`:**
  - `app.windows[0].transparent = true`.
  - Keep `decorations: true`: the OS title bar stays, so the window can still be dragged, snapped and closed as usual.
  - `backgroundColor` becomes transparent, with the fallback painted by CSS.
  - `app.macOSPrivateApi = true` (macOS build only).
- **New command `window_glass_apply(enabled: bool)`** in `reborn-ui/src/main.rs`:
  - Calls `WebviewWindow::set_effects(...)` with the platform effect, or `set_effects(None)` when off.
  - Returns `{ applied: bool, reason: "unsupported" | "disabled" | null }`.
  - The UI sets `html[data-glass="on|off"]` from the result, so CSS always matches what the OS actually did.
- **At startup,** read the setting (§4.3) and apply it before the first paint to avoid a flash.
- **Spike first (§12, P0):** prove Mica + transparent + decorations in the real Tauri window on Windows 11, including the title bar, resize, snap layouts, minimise and restore, and fullscreen Dolphin launched on top. If the title bar looks wrong over Mica, **stop and ask Adam** before going custom-frameless.

### 4.3 Setting: Settings › Display › Launcher window › **Window glass**
- **Stored in `reborn-core` `Settings`:** `window_glass: Option<bool>`.
  - `None` means on wherever it's supported, following the existing "absent means never set" rule (`settings.rs`).
- **Preview and save:**
  - Flipping the switch **previews live** (calls `window_glass_apply`).
  - The save bar shows "Window glass". **Save** stores it; **Discard** reverts it.
- **Where glass isn't available:**
  - The switch shows off, disabled, with "Not available on this system."
  - On Linux it says "Frosted glass isn't available on Linux." (help text).

---

## 5. Motion

Adam: *"cleanest animations possible"* → *"I want the damn slow mo motions … Just needs to be toggleable."*

### 5.1 Rules
- **Only animate `opacity` and `transform`.** Width and height changes use `grid-template-rows` or `max-height`, never layout-thrashing properties.
- **One easing curve:** `cubic-bezier(.2,.8,.2,1)`.
- **No bounce, glow, count-up numbers, parallax or looping motion.** The only exception is the pulsing "live" dot.
- **Three durations,** driven by one setting:

| Token | Slow (default) | Quick | Used for |
|---|---|---|---|
| `--d1` | 480 ms | 120 ms | hover, press, colour |
| `--d2` | 720 ms | 180 ms | menus, switches, tabs |
| `--d3` | 960 ms | 240 ms | page enter, save bar, expanding rows, nav marker |

⛔ **Set the durations directly** under `html[data-motion="slow"]`. Don't use a multiplier variable: custom properties are computed where they're declared, so a multiplier on `body` doesn't re-resolve durations declared on `:root`. This was found in the mockup.

### 5.2 What moves
- **Page change:** the content fades in and rises 6px (`--d3`). **The sidebar never moves.**
- **Sidebar:** **one** red marker bar glides to the active item (`--d3`). Items no longer each have their own marker.
- **Tabs and filters:** one highlight slides and resizes to the selected option (`--d2`).
- **Switch:** the thumb slides (`--d2`).
- **Menus:** fade and settle 4px from their anchor (`--d2`).
- **Save bar:** its lane opens (`max-height`), then the bar rises 12px and fades in (`--d3`). The content above shrinks with it and is never covered.
- **Best-of-3 row:** expands via `grid-template-rows: 0fr → 1fr`, the games fade in, and the chevron turns 180° (`--d3`).
- **Buttons:** press = `scale(.98)`.

### 5.3 Setting and accessibility
- **Setting:** Settings › Display › Launcher window › **Slow animations**. On by default, stored as `slow_animations: Option<bool>` (`None` = on).
  - It previews live and goes through the save bar.
  - It sets `html[data-motion="slow|quick"]`.
- **The OS reduce-motion setting always wins.** `@media (prefers-reduced-motion: reduce)` sets every duration to 0, whatever the setting says. The help text says so.
- **The `data-motion` attribute is set before first paint** (inline `<script>` in each page's `<head>`, reading a value the shell caches), so the first page load doesn't animate at the wrong speed.

---

## 6. Shell (every page)

### 6.1 Sidebar
- **Logo:** the current `art/nsr-sidebar.png` as it is. Adam is replacing it; the slot is 84px tall.
- **Items:**
  - 40px tall, icon + label, `--fg-2`.
  - Hover shows `--glass-1`.
  - Active shows `--glass-3` + `--sheen`, plus the shared sliding red marker at the left edge.
- **Settings and Help** stay pinned at the bottom.
- **Nav icons:** keep the **current** SVG paths from the existing pages (including the current controller icon).

### 6.2 Page frame
- **Layout:** page title (30px) + subtitle (`--fg-2`), then content.
- **Scrolling:** each page is its own scroll container. Only the content area scrolls, never the sidebar.
- **Pages with a save bar** (Settings, Music) use a column layout (`scroll area` + `save lane`), so the bar never overlaps content.

### 6.3 Controller prompts and focus ring
- **Today:** `renderHints()` shows the hint bar whenever **a pad is connected** (`app-shell.js:80-83`), even while the player uses the mouse.
- **New rule:** the hint bar and the white focus ring appear only while `controllerMode` is true.
  - `controllerMode` becomes true on the **last input** being a pad.
  - It becomes false on any mouse move, click or key press.
  - The existing `controllerMode` / `syncInputMode()` already tracks this (`app-shell.js:59-107`). Key the hint bar and `body.gamepad-input` on it instead of on a pad being connected.
- **Focus ring:** `outline: 2px solid #fff; outline-offset: 3px`. No glow.
- **Hint bar:** 34px tall, glass, with key-cap glyphs (`A`, `B`, `LB`/`RB` …, or the PlayStation glyphs for PS pads, existing logic).
- **Everything stays reachable by pad.** Gamepad support stays first-class.

---

## 7. Save bar pattern (Settings, Music)
- **Hidden** until there is at least one unsaved change. Then it animates in (§5.2).
- **Content:**
  - A red dot.
  - "N unsaved change(s)" (Music: "N pending change(s)").
  - The name of the last change ("Emulator location", "Window glass", "Song 3 replaced").
  - **Discard** and **Save changes** (Music: **Apply changes**).
- **After Save or Discard** it animates out. Discard puts every previewed setting back (glass, motion).
- It replaces every per-row Save button.

---

## 8. Pages

The layouts match today. Only what changes is listed.

### 8.1 Play
- **Hero:** keeps the full-bleed art (`play-background.png`). The wordmark (`play-logo.png`) at 470px width with its natural aspect ratio, **unchanged by us**.
- **Top right:** a small account block: avatar, name, "N min played". Replaces the "Signed in as …" text.
- **Actions:**
  - **Play now:** the one red primary button, 52px tall, at least 250px wide.
  - **⋯:** a 52×52 glass button beside it, which opens Game details.
- **Status line** under the button: dot + text, with an optional underlined action link. Five states (the existing strings):

  | State | Dot | Text | Action |
  |---|---|---|---|
  | ready | green | Ready to play | none |
  | in session | green, pulsing | Enjoy the game | none (button reads "Playing", dimmed) |
  | setup needed | amber | Setup required | Finish setup |
  | launch failed | red | Your game could not start | See why |
  | crashed | red | Game closed unexpectedly | See why |
- **Bottom strip:** one glass panel, three cells edge to edge.
  - **Controllers:** N connected ›
  - **Graphics:** a menu with Performance / Balanced / All-Out.
  - **Display:** a menu with Fullscreen / Windowed.
  - Hover fills the **whole** cell. The outer corners follow the panel's radius.
  - No checkboxes, switches or segmented controls in this strip.
  - The existing ReShade text in `home.js:48-50` is stale (ReShade has been off since 3.0.20), so the menus show names only.
- **Hint bar:** only in controller mode (§6.3).

### 8.2 Settings
- **Header:** title and subtitle, then a slimmer "NFL Street Reborn Dolphin" card with **Open Dolphin**. It appears once, not repeated per tab.
- **Tabs:** the same 7 (General, Graphics, Display, Audio, Saves, Updates, Files & storage) in the sliding segmented control.
- **Path rows:**
  - Label | field (value + green "Auto" tag when detected) | **Change…**.
  - The current Browse + Save pair becomes Change…, and saving goes through the save bar.
- **Display tab** gains a **Launcher window** section with **Window glass** (§4.3) and **Slow animations** (§5.3).
- **Save bar** per §7.

### 8.3 Mods
- **Toolbar:** search field, **Import mods** (`.btn`) and Refresh (`.btn.quiet`).
- **Tabs:** Footballs / Soundtrack. Count line: "3 footballs · 1 enabled · only one football can be on at a time".
- **Cards (3 columns):**
  - A neutral preview area (`linear-gradient(160deg, rgba(255,255,255,.05), rgba(0,0,0,.18))`). **No drawn art.** A real preview image only if the package includes one.
  - Name and meta.
  - A footer row with "Enabled"/"Off" and **one switch**, replacing the tag + Enable/Disable button pair.
  - The enabled card shows a red check badge at the top right of the preview.
  - Pick-one groups still turn the others off (existing `mods_toggle`).

### 8.4 Music
- **Header row:** **Custom soundtrack** switch on the left. On the right, Reset, Import pack and Export pack (quiet) and **Add songs…**. The duplicate "Add song" is removed.
- **Soundtrack table** (glass): # | reorder | song + source line | length | actions.
  - **Pending replacement:** the source line reads "Replacing Original song 3 **· Pending**" (`--red-ink`), with a red left edge and a faint red tint. Actions show **Undo** only.
    - Rejected: a Pending chip, and Pending text in the actions column.
  - **Custom song:** Restore original (quiet) + Replace.
- **Save bar** per §7, with "Apply changes".

### 8.5 Controllers
- **Header:** title and **Set up launcher controls** as a normal `.btn`, not greyed out.
- **The "B maps to B" warning** becomes a small glass note with an amber icon and **Dismiss**. No longer a permanent red banner. Dismissal is stored per install.
- **Player cards:** 4 glass cards with the **current** controller icon, "Player N", and the device.
  - Selected: red border and 8% red fill.
  - Empty: the icon at 45% opacity.
- **Then:** the Input device row (select + Scan again), and a mapping panel in three columns (sticks and D-pad | controller art + Test controller | buttons and right stick), with binding tiles.
- **No behaviour changes.** Controller bugs are out of scope (§1).

### 8.6 Stats (Stats v2 content, redesigned)
- **Subtitle:** "Every match you've played, where your Elo stands, and who's carrying your team."
- **Layout:** two columns, gap 24. Left `1.5fr`, right `minmax(340px, 1fr)`. **Both columns' first cards start on the same line and end on the same line.**
- **Left: Your matches.**
  - Header: title + All / Online / Offline tabs.
  - A glass list with a column header **RESULT · MATCHUP · OPPONENT · ELO · DATE**, then about 10 rows of 56px.
  - Row:
    - `W 2–1`: coloured letter, your score first.
    - Your logo `vs` their logo, both 24px.
    - Opponent avatar + name, with a second line only for "Offline". **Never "Best of 3".**
    - Elo change (`+16` in green, `−18` in red, blank when unranked).
    - Muted date.
    - Chevron on best-of-3 rows only.
  - **Best-of-3 row:** a `<button aria-expanded>` that expands into "Game 1 · 5:00 PM" sub-rows (40px) in the same columns. A one-game row opens `game.html?game=<id>` (handoff).
  - **Pager:** Newer / "Page 1 of 2" / Older sits **inside** the card as its last row.
- **Right, "You" card:**
  - Header: title + "Your profile ›".
  - Elo 1,184 (700, 32px) with a sparkline, "▲ 16 · #7 on the leaderboard".
  - Then four numbers: **Record 14–9 · Win % .609 · Last 10 6–4 · Streak W2** (a winning streak in green).
  - **Removed:** the recent-form dots (Adam: "doesn't make any sense"), points for/against, Pts/gm, Yds/gm and Plays as. They are on Profile.
- **Right, "Your players":**
  - Header: title + Offense / Defense / Records links.
  - One glass list that stretches so its bottom lines up with Your matches.
  - Rows (64px): portrait on the team colour + badge, **name** with a small role label (BEST PASSER …) on the right, and a full statline underneath: `3,108 YDS · 31 TD · 11 INT · 108.9 RTG` (numbers white, units uppercase and muted).
  - Statline per role:

    | Role | Statline |
    |---|---|
    | Passer | YDS · TD · INT · RTG |
    | Rusher | CAR · YDS · TD · AVG |
    | Receiver | REC · YDS · TD · AVG |
    | DL/LB | TKL · SCK · TFL · PBU |
    | Ballhawk | INT · PBU · TKL · TD |
    | Most played | GAMES · CAR/REC · YDS · TD (by position) |
- **Removed from Stats:** Head to head (it lives on Profile).
- **Footer line:** the queue note ("2 games not uploaded yet: …") on the left and **Profile & sync ›** on the right.

### 8.7 Profile (the online player hub)
Adam: *"it needs to be like something that encompasses everything online gameplay wise … the forefront should be actual stats."*

1. **Back** (returns to the page it came from, existing).
2. **Profile card** (glass, full width):
   - **Background:** the cover gradient (`--c1`/`--c2` from the Discord avatar) runs behind the whole card from the avatar side and fades out to the right.
   - **Avatar:** 104px, round, vertically centred with the identity block, with a green online dot.
   - **Identity:**
     - Name (28px, 700).
     - A meta line: status ("● In a match" / "● Online now" / nothing) · Joined … · Plays as [team select].
     - The summary line: **#7** on the leaderboard | **1,184** Elo | **60.9%** win rate (thin dividers, 16px padding each side, bold number + normal label).
     - **Share profile** (primary) and **View on web**.
   - **Live card**, right side, only while in a match, **never for private rooms**:
     - "● LIVE" (pulsing dot), "Ranked · Game 2 of 3".
     - The logos + abbreviations with the score (`14 – 7`).
     - "vs Mikel · Series 1–0".
     - **Live feed** button.
     - When not live, the card is gone and the layout closes up.
   - **Rejected here:** the six boxed stat columns (read as AI-generated), and a full-width live banner.
3. **Overview panel:** **one** glass surface, three columns split by dividers, identical structure: title + right-side note → big number (34px) + a coloured side value → a muted subline → a visual pinned to the bottom. All three big numbers share a baseline, and all three visuals end on the same line.
   - **Elo:** `1,184 +16`, "Peak 1,212 · started at 1,000", area sparkline.
   - **Record:** `14–9 60.9%`, "Ranked and casual", Ranked 8–5 .615 and Casual 6–4 .600 as label / 4px bar / record / pct.
   - **Last 10 games:** `6–4 +6.4`, "Average winning margin", Points scored 31.4 | Points allowed 25.0.
   - **No private-room text anywhere.**
4. **Two columns** (as on Stats):
   - **Match history:**
     - All / Ranked / Casual. **No Private.**
     - Same row format as Stats; the second line shows the queue (Ranked or Casual).
     - Rows stretch so the list bottom lines up with the right column.
     - The pager sits in the card.
   - **Career:**
     - A 3×3 grid: Passing Yards, Passing TD, Passer Rating, Rushing Yards, Receiving Yards, Total TD, Tackles, Sacks, Interceptions.
     - Each shows the value (18px), then "**#3** · 125.9 / G". Rank among the player base.
     - Footnote: "Rank among 214 online players".
     - "Full stats ›" scrolls to §8.7.5.
   - **Leaderboard:** 5 rows (#5–#9) with rank · avatar · name · Elo. Your row reads "You", with a red left edge and a faint red tint. "Full leaderboard ›".
   - **Records:** record · player (logo + name) · value · date. "All ›".
   - **Removed:** Rivals (random matchmaking makes it meaningless).
5. **Full stats:**
   - Tabs: Passing / Rushing / Receiving / Defense, one table at a time.
   - **Top 5** players, then a "**Show all N players** ⌄" row that reveals the rest, then the **Total** row.
   - Columns:
     - **Passing:** C/ATT · YDS · TD · INT · AIR · YAC · Y/A · RTG · LNG
     - **Rushing:** **CAR** · YDS · AVG · TD · LNG · BTK · **YCO**
     - **Receiving:** REC · YDS · AVG · YAC · TD · LNG · **DRP**
     - **Defense:** TKL · TFL · SCK · INT · PBU · FF · FR · TD
   - All four tables share the first-column width so columns line up.

**Data needed for Profile:** see the backend work list. §1–§3 (private rooms), §5 (leaderboard around + total), §6 (per-stat rank), §7 (`header.live`), §8 (`recent_form`), §9 (`yco`), §10 (team primary) and §11 (peak). **Until those ship, the page hides the pieces with no data** instead of showing placeholders: no rank numbers, no live card, no leaderboard.

---

## 9. Pages not redesigned here
- **Online** (`online.html`): separate redesign. It still picks up `theme.css` tokens so it doesn't clash, but its layout isn't touched.
- **Onboarding** (`onboarding.html`), **Recovery** (`recovery.html`), `game.html`, `live.html`: move onto `theme.css` tokens and the shell. No layout work. They are listed so nothing still loads the old stack.
- **`harness.html`** (developer tools): untouched.

---

## 10. Behaviour that must not change (from the Stats v2 handoff + launcher rules)
- **Safety:**
  - Server text only goes in through `el()` / `textContent`, never `innerHTML`.
  - Images only come through `assetUrl()` or the Discord avatar prefix (CSP `img-src`).
- **Focus restore** after each 30-second refresh via `data-key` (`match-<id>`, `game-<id>`, `newer`, `older`). Keep them when rows are rebuilt.
- **Arrow keys** step through games in `game.js` only with no modifier held.
- **Matches first:** a best of 3 is one row with its series and a single Elo change.
- **The wording Adam chose in the handoff still applies,** except where §2 changes it: "Best players" becomes "Your players", and "Recent form" is gone.
- **`stats-model.js` / `stats-rows.js`:** change only if required, and keep their tests passing.
- **Commands are the contract:** every value on a page comes from a command. Nothing is invented. The mockup's sample numbers are placeholders only.
- **Existing checks** stay green or are updated on purpose:
  - `node --test crates/reborn-ui/ui/stats-model.test.mjs crates/reborn-ui/ui/stats-rows.test.mjs crates/reborn-ui/ui/cover-color.test.mjs`
  - `node Tools/check-shell-pages.mjs`
  - `node Tools/check-page-call-args.mjs`
  - `node Tools/run-browser-tests.mjs`
  - `cargo test --workspace -j 2` and `cargo clippy --workspace --all-targets`

---

## 11. CSS architecture (files)

**Today,** every product page stacks `styles.css` + `onboarding.css` + `launcher.css` + `product-ui.css` + `finish.css` + a page file, with conflicting tokens.

**New:**
| File | Holds |
|---|---|
| `ui/theme.css` (new) | tokens (§3), base, material + fallback, motion (§5), every shared component (§3.4), shell (§6), save bar (§7) |
| `ui/pages/play.css`, `settings.css`, `mods.css`, `music.css`, `controls.css`, `stats.css`, `profile.css` | page layout only; **no colour literals** (enforced by a checker, §13) |
| `ui/onboarding.css` | onboarding only, rebased on `theme.css` tokens |

- **Retired once no page loads them:** `styles.css`, `launcher.css`, `product-ui.css`, `finish.css`, `shell.css`, `play.css`, `players.css`, `stats.css` (old), `onboarding-polish.css`, `live-view.css` (folded in), `music.css` (old).
- **Each page loads exactly** `theme.css` + its page file.
- **`DESIGN.md`** is rewritten to this spec's tokens and rules, replacing "Direction A". Its opening warning stays: *"If a value is not in this file, it is not a decision anyone gets to make in a stylesheet."*

---

## 12. Order of work (each step is its own beta build, Adam approves each)

1. **Mica spike** (throwaway branch): the real window on Windows 11 (and Win 10 Acrylic if a Win 10 machine is available), per §4.2. **Go / no-go from Adam.**
2. **Foundation:**
   - `theme.css`, the shell (sidebar with the sliding marker, page frame, hint bar rule §6.3).
   - The window glass + slow animations settings and commands.
   - Every page switched to `theme.css` with **no page layout changes**, so the look changes everywhere at once and nothing breaks.
3. **Play.**
4. **Settings** (incl. save bar and the Display tab additions).
5. **Mods, Music, Controllers.**
6. **Stats** (needs `feat/stats-profile-v2` merged into this branch, which it already is).
7. **Profile.** Degrades gracefully until the backend work ships (§8.7).
8. **Cleanup:** delete the retired CSS and rewrite `DESIGN.md`.

The backend work (`BACKEND-CHANGES-launcher-redesign.md`) runs in parallel. **Private rooms (§1–§3) should ship before any public profile does.**

---

## 13. Testing
- **Screenshots:** extend `Tools/run-browser-tests.mjs` so every page and state is captured at 1280×800 and 800×560 (the minimum window). Include: controller mode on and off, glass on and off (`data-glass`), slow and quick motion, the save bar hidden and shown, Profile live and not live, and a best-of-3 expanded.
- **Alignment checks** (as done on the mockup, but automated): Stats columns' first cards start and end on the same line, section headers are 40px, the Profile overview's big numbers share a baseline, and full-stats tables share column edges.
- **Motion checks:**
  - Slow: the nav marker transition is 0.96s.
  - Quick: 0.24s.
  - Reduced motion: state changes are instant (computed `transitionDuration` = 0s).
- **Controller mode check:** a connected pad with mouse input shows **no** hint bar. After pad input, the hint bar and focus ring appear. Any mouse move hides them again.
- **Token checker (new `Tools/check-theme-tokens.mjs`):** page CSS files contain no hex/rgb colour literals and no `font-weight` above 700.
- **Content checks:** no "Best of 3", no "ladder", no T results, no "Private" on Profile.
- **Real-app check** on this PC (beta install) for glass, per step, before any build goes to players.

---

## 14. Open questions
1. **Win 10 Acrylic:** ship it, or use the fallback on Win 10? Decided by the spike.
2. **Title bar over Mica:** keep the OS title bar (planned), or go frameless with our own buttons if it looks off? Only if the spike shows a problem.
3. ~~Stats page "Your players" statlines~~: **settled.** Adam: "I think you nailed the stats" (§8.6 table, with CAR for rushing).
4. **Backend decisions:** Elo replay without private rooms; the abandoned 1–1 series rule; per-account career ranks; Last 10 = games or matches. See the backend doc.
5. **Rollout:** one beta build per step (§12), or group Play + Settings? Default: one per step.
