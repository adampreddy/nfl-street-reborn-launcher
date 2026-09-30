# Launcher Glass Redesign, Plan 3: Mods, Music and Controllers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mods, Music and Controllers get the approved frosted-glass design (mockup sections `id="mods"`, `id="music"`, `id="controls"`). Each page loads only `theme.css` and its own `pages/<page>.css`. Music's pending changes go through the same floating save bar as Settings.

**Architecture:**
- The three pages opt in the way plan 2's Play and Settings did: `<body class="shell themed page-<name> …">`, exactly two stylesheets, layout-only page CSS.
- Everything shared comes from plan 2: the `body.themed` base, the shell and hint bar, `.btn`/`.glass`/`.look-switch`/`.field`, `tabs.js` (sliding segmented tabs), `savebar.js` with its lane (`has-save-lane`) and failure line, `icon()` in `shell.js`.
- Plan 3 adds only three shared things (Task 1):
  - `savebar.js` gains `lock()`, so Music can hold Apply while the core encodes or builds;
  - `theme.css` gains two tokens (`--thumb-bg`, `--surface-solid`), field styling for search boxes and text areas, and the controller-art rules the Controllers page used to get from `launcher.css`.
- No Rust. No new command. Every value on these pages comes from a command that exists today.

**Tech Stack:** Tauri 2.11 (Rust untouched), static HTML/JS/CSS in `crates/reborn-ui/ui` (no bundler, `withGlobalTauri`), Playwright + Edge for browser checks, `node --test` / plain node for fake-DOM checks.

**Spec:** `docs/superpowers/specs/2026-09-30-launcher-glass-redesign-design.md`.
- Binding sections: §2, §3, §5, §6, §7, §8.3, §8.4, §8.5, §10, §11, §13.
- Visual target: `docs/superpowers/specs/assets/2026-09-30-launcher-glass-mockup.html`, the `<main class="page …" id="mods">`, `id="music"` and `id="controls"` sections (lines ~906-955) and their CSS (`/* ---------- MODS ---------- */`, `MUSIC`, `CONTROLLERS`, lines ~193-245 and ~278-290).
- This is **plan 3**. Plan 1 (foundation) and plan 2 (Play + Settings, `docs/superpowers/plans/2026-09-30-launcher-glass-play-settings.md`) are complete before this plan starts. Plan 4 (Stats, Profile, cleanup) follows.

**Working copy:** worktree `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher redesign`, branch `feat/launcher-glass-redesign`.
- Never commit to `main` or `release/*`. Never `git stash` (the stash is shared across worktrees).
- **Plan 2 is finished, but its executors may have fixed things beyond its text.** Every edit below that quotes plan-2 code is anchored on that code. If the live file differs, read it (`git log -3 --format="%h %s" -- <file>` then `git show <hash> -- <file>`), apply the same change to the live text, and never undo plan-2 work.

## Owner rulings this plan implements (binding, Adam 2026-09-30)

- **No mock art.** No drawn footballs. Packages ship no preview art, so the preview area is always the neutral gradient.
- **Mods cards:** one switch (Enabled/Off) replaces the tag + Enable/Disable pair; a red check on the enabled card.
- **Music:** the same floating, persistent save bar as Settings ("N pending change(s)", **Apply changes**). A pending row says **"· Pending"** at the end of its source line (in `--red-ink`); its actions are **Undo only**. A Pending chip and Pending text in the actions column were both rejected.
- **Keep the launcher's current icons.** The player cards reuse the Controllers nav icon's two paths.
- **No glows. Weights ≤ 700. Accent red.**
- **The controller window stays in the launcher** (mapping, test and capture stay on this page; nothing opens a separate window).

## Global Constraints

- **Colours** (theme only): `--red #E0272E`, `--red-ink #FF7A7F`, `--ok #3DDC97`, `--warn #FFC46B`, `--fg #F4F5F7`, `--fg-2 #AEB5C0`, `--fg-3 #7C8491`. Glass surfaces are `--glass-1/2/3`, strokes `--stroke`/`--stroke-hi`.
- **Page stylesheets hold layout only.** `ui/pages/*.css` contains no hex, no `rgb()/rgba()/hsl()` colour literals and no `font-weight` above 700. `node Tools/check-theme-tokens.mjs` enforces it. `color-mix(in srgb, var(--token) N%, transparent)` is allowed.
- **Themed pages load exactly** `theme.css` + their page file (spec §11). `node Tools/check-theme.mjs` enforces it through its `THEMED` map.
- **No glow anywhere.** Focus = the theme's white 2px ring. No coloured `box-shadow`.
- **Weights:** 500 UI, 600 labels/values, 700 headline numbers only. Never above 700.
- **Motion:** only `opacity`/`transform` animate (plus the tab highlight's `width`, plan 2's one exception). Durations come from `--d1/--d2/--d3`. Reduced motion = 0.
- **Spacing:** 8 / 16 / 24 only (spec §3.3). Form pages (Mods, Music, Controllers) use a 942px content column, the mockup's `.pinner` (1014px including 36px padding each side).
- **Commands are the contract.** No new Rust command. `node Tools/check-page-call-args.mjs` must pass: keep command arguments as **object literals** at the call site.
- **Behaviour that must not change (spec §10):**
  - Mods: `mods_toggle` with `{ id, version, on, runtime }`; pick-one groups still turn the others off (the core does it); the soundtrack rebuild notice; stale and unreadable packages still listed; `mods.js` keeps reading `withheld_note` (a Rust test in `main.rs` greps for it).
  - Music: the disc-full limits and wording (`mmss` and `tooLongProblem` stay named functions in `music.js`: `Tools/check-music-length-message.mjs` lifts them out of the source); one Apply that rebuilds the disc; drag and arrow reordering; the song dialog, packs, several-songs, export and reset dialogs.
  - Controllers: the binding editor, press-to-map capture, typed binding fallback, test mode, reversed-B/X correction, reset, button textures, the controller report, and pad navigation. `setup-controller.js`, `controller-*.js` and `menu-pad*.js` are **not edited** in this plan.
- **Safety and CSP:** server text only via `el()`/`textContent`. No inline scripts in product pages.
- **The binding popover (`.player-workspace .binding-editor`) stays OPAQUE.**
- **`position:fixed` UI goes on `<body>`**, never inside `#main` (its enter animation makes it a containing block). The menu, dialogs and the drag ghost already do.
- **Controller navigation is first-class.** Everything reachable by D-pad; B/Esc closes dialogs.
- **Line endings:** this plan touches no `.rs` file.
- **No beta builds, no installer.** The end of the plan is a real-window check of `target/debug/reborn-launcher.exe`.
- **Commits:** one per task, message as given, always ending with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

### How to run the checks (every task)

Playwright is not installed in this worktree. Link it for the duration of a task, then unlink it (PowerShell, from the worktree root):

```powershell
New-Item -ItemType Junction -Path "Tools\node_modules" -Target "C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher\Tools\node_modules" | Out-Null
# ... run checks ...
cmd /c rmdir "Tools\node_modules"      # removes the link only. NEVER Remove-Item -Recurse on a junction.
```

**Known failures before this plan (baseline, do not fix, do not count as new):**
- `Tools/review-finish.mjs`
- `Tools/check-launch-polish.mjs`
- `Tools/check-online-mm.mjs`

`node Tools/run-browser-tests.mjs` runs its own main body, then imports `review-players` (passes) and then `review-finish` (fails) and aborts there. So its **main body must print** `PASS: launcher flows, navigation, onboarding, saves, settings, and motion stability.`, and every other checker is run **individually**.

**The standing checks** (referred to by that name below):

```bash
node Tools/check-shell-pages.mjs
node Tools/check-page-call-args.mjs
node Tools/check-theme.mjs
node Tools/check-theme-tokens.mjs
node Tools/check-themed-shell.mjs
node Tools/check-save-lane.mjs
node Tools/check-mods-page.mjs
node Tools/check-music-page.mjs
node Tools/check-music-length-message.mjs
node Tools/check-controller-navigation.mjs
node Tools/check-controller-hints.mjs
node Tools/check-controller-report.mjs
node Tools/review-players.mjs
node Tools/check-look-browser.mjs
node Tools/check-update-notice.mjs
node Tools/run-browser-tests.mjs target/browser-review
```

## Review Focus

1. **A Music apply that fails** (Dolphin open, disk full). The bar says why in its failure line, every change stays pending, and Apply/Discard come back. Nothing is shown as applied. The test is in Task 3 (`check-music-page`).
2. **A pad user on Music with pending changes.** D-pad Down from the last song reaches **Apply changes**; B from the bar goes to the sidebar and never leaves the page (which would look like the changes were lost). The test is in Task 5.
3. **Music while the core is encoding or building.** Apply and Discard stay disabled until the core is idle, so a second Apply cannot start a second rebuild. The tests are in Task 1 (`lock`) and Task 3 (Apply disabled while building).
4. **Switching on a football while another is on** (a pick-one group). After the core answers, exactly one card is Enabled with the red check, and the previous card's switch reads off. The test is in Task 2 (`run-browser-tests` Mods step).
5. **The minimum window (800×560) with long names** (a long mod name, a long song title, four players). No sideways scroll on any of the three pages; the bar and the cards stay inside the window. The test is in Task 5.

---

## File structure

| File | Responsibility |
|---|---|
| `crates/reborn-ui/ui/savebar.js` (modify) | `lock(saveHeld, discardHeld)` |
| `crates/reborn-ui/ui/theme.css` (modify) | tokens `--thumb-bg`, `--surface-solid`; search/textarea fields; controller art for themed pages |
| `crates/reborn-ui/ui/mods.html` (modify) | theme-only Mods |
| `crates/reborn-ui/ui/mods.js` (modify) | toolbar, sliding tabs, count line, cards with one switch |
| `crates/reborn-ui/ui/pages/mods.css` (create) | Mods layout only |
| `crates/reborn-ui/ui/music.html` (modify) | theme-only Music with a save lane |
| `crates/reborn-ui/ui/music.js` (modify) | header row, table rows (source line, Pending, actions), the save bar |
| `crates/reborn-ui/ui/pages/music.css` (create) | Music layout only |
| `crates/reborn-ui/ui/controls.html` (modify) | theme-only Controllers |
| `crates/reborn-ui/ui/controls.js` (modify) | player cards with the icon, the B/X note with Dismiss |
| `crates/reborn-ui/ui/pages/controls.css` (create) | Controllers layout only (replaces `players.css` on this page) |
| `Tools/check-save-lane.mjs` (modify) | `lock()` |
| `Tools/check-mods-page.mjs` (modify) | switch, check badge, count line; loads `mods.js` with its imports |
| `Tools/check-music-page.mjs` (modify) | save bar, Pending line, Undo only, failed apply |
| `Tools/check-shell-chrome.mjs` (modify) | Music has no "Add song" button any more |
| `Tools/review-players.mjs` (modify) | player icons, B/X note and its Dismiss, opaque popover |
| `Tools/check-theme.mjs` (modify) | `THEMED` gains mods, music, controls |
| `Tools/check-themed-shell.mjs` (modify) | the legacy-parity page becomes `online.html` |
| `Tools/run-browser-tests.mjs` (modify) | Mods step uses the switch; one import |
| `Tools/check-glass-mods-music-controls.mjs` (create) | spec §13 for the three pages |

`music.css` (old) and `players.css` stay on disk; nothing loads them after this plan. Plan 4 deletes them.

---

### Task 1: Shared pieces: the save bar's lock, two tokens, fields, controller art

**Files:**
- Modify: `crates/reborn-ui/ui/savebar.js`
- Modify: `crates/reborn-ui/ui/theme.css`
- Modify: `Tools/check-save-lane.mjs`

**Interfaces:**
- Consumes: plan 2's `mountSaveBar(lane, { onSave, onDiscard, returnFocus, noun, saveText }) → { update(changes), fail(message) }`.
- Produces:
  - `bar.lock(saveHeld: boolean, discardHeld: boolean = saveHeld)`: keeps Save and/or Discard disabled until `lock(false)`, also after a save or discard finishes. Task 3 calls `bar.lock(locked() || !status?.image_ok, locked())`.
  - Tokens `--thumb-bg` (the neutral card preview, spec §8.3) and `--surface-solid` (an opaque surface for popovers that float over content: the binding editor, the drag ghost).
  - `body.themed input[type=search]` and `body.themed textarea` look like the plan-2 fields; inside `.field` a search box is borderless.
  - `body.themed .controller-preview …`: the controller art rules for themed pages (Task 4 needs them; the legacy onboarding page keeps its copy in `launcher.css` until plan 4).

- [ ] **Step 1: Write the failing check**

In `Tools/check-save-lane.mjs`:

**(a)** In the page script, replace the line

```js
 onSave:async()=>{if(window.outcome)throw new Error(window.outcome);window.saves++;window.bar.update([]);},
```

with

```js
 onSave:async()=>{if(window.lockDuring)window.bar.lock(true);if(window.outcome)throw new Error(window.outcome);window.saves++;window.bar.update([]);},
```

**(b)** Directly before the line `assert.deepEqual(errors,[]);` insert:

```js
 // lock() (plan 3, Music): the page holds Save, and optionally Discard, while its own work runs.
 await page.evaluate(()=>{bar.update(['Song 3 replaced']);bar.lock(true,false);});
 assert.equal(await page.getByRole('button',{name:'Save changes'}).isDisabled(),true,'lock holds Save');
 assert.equal(await page.getByRole('button',{name:'Discard'}).isDisabled(),false,'Discard can stay free');
 await page.evaluate(()=>bar.lock(true));
 assert.equal(await page.getByRole('button',{name:'Discard'}).isDisabled(),true,'one argument holds both');
 await page.evaluate(()=>bar.lock(false));
 assert.equal(await page.getByRole('button',{name:'Save changes'}).isEnabled(),true,'lock(false) frees them');
 // A save that ends while the page holds the bar leaves the buttons held (it used to free them in finally).
 await page.evaluate(()=>{window.lockDuring=true;window.outcome='The disk is full.';});
 await page.getByRole('button',{name:'Save changes'}).click();
 await page.locator('.save-bar [role=alert]').waitFor();
 assert.equal(await page.getByRole('button',{name:'Save changes'}).isDisabled(),true,'still held after the save ended');
 await page.evaluate(()=>{window.lockDuring=false;window.outcome=null;bar.lock(false);});
 assert.equal(await page.getByRole('button',{name:'Save changes'}).isEnabled(),true);
```

- [ ] **Step 2: Run it and confirm it fails**

Link Playwright (see "How to run the checks"), then run `node Tools/check-save-lane.mjs`.
Expected: FAIL with `bar.lock is not a function` (a page error, or the first `lock holds Save` assertion).

- [ ] **Step 3: Add `lock()` to `savebar.js`**

Make three edits to the plan-2 file.

**(a)** Replace

```js
  let busy = false;
```

with

```js
  let busy = false;
  // A page may hold the buttons while its own work runs (Music: while the core encodes or builds).
  let held = { save: false, discard: false };
  const sync = () => { save.disabled = busy || held.save; discard.disabled = busy || held.discard; };
```

**(b)** Replace

```js
    finally { busy = false; discard.disabled = save.disabled = false; }
```

with

```js
    finally { busy = false; sync(); }
```

**(c)** Replace

```js
    fail,
  };
```

with

```js
    fail,
    // Keep Save (and Discard, unless told otherwise) disabled until lock(false).
    lock(saveHeld, discardHeld = saveHeld) { held = { save: !!saveHeld, discard: !!discardHeld }; sync(); },
  };
```

- [ ] **Step 4: Add the tokens, fields and controller art to `theme.css`**

**(a)** In the first `:root{…}` block, directly after the plan-2 line `--dolphin:#22C7B4;`, add:

```css
  /* plan 3: the neutral mod preview (spec §8.3, no drawn art), and an opaque surface for popovers over content */
  --thumb-bg:linear-gradient(160deg,rgba(255,255,255,.05),rgba(0,0,0,.18));
  --surface-solid:#1b1e24;
```

**(b)** Append at the very end of `theme.css`:

```css
/* ================= Plan 3 shared pieces ================= */
/* search boxes and text areas look like the other fields; inside .field (an icon + a box) the search box is bare */
body.themed input[type=search],body.themed textarea{min-width:0;padding:0 12px;border-radius:var(--r-md);border:1px solid var(--stroke);background:var(--field-bg);color:var(--fg);font:500 13px Inter,system-ui,sans-serif;color-scheme:dark}
body.themed input[type=search]{height:38px}
body.themed textarea{padding:8px 12px;resize:vertical}
body.themed .field input[type=search]{flex:1;height:100%;padding:0;border:0;border-radius:0;background:transparent;box-shadow:none}

/* controller art (controller-preview.js) on themed pages. The onboarding page keeps launcher.css's copy until plan 4. */
body.themed .controller-preview{margin:0 auto;width:min(100%,360px);color:var(--fg-2)}
body.themed .controller-preview svg{display:block;width:100%;height:auto;overflow:visible}
body.themed .controller-preview figcaption{margin-top:2px;text-align:center;font-size:11px;letter-spacing:.03em;color:var(--fg-3)}
body.themed .sourced-preview svg{width:100%;max-height:280px;object-fit:contain}
body.themed .sourced-preview:has(svg) figcaption{display:none}
body.themed .layered-preview{position:relative;display:block!important;aspect-ratio:var(--controller-ratio);min-height:0;margin:0 auto}
body.themed .layered-preview>[class^="vendor-"]{position:absolute;left:0;top:0;transform-origin:0 0;pointer-events:none}
body.themed .layered-preview>[class^="vendor-"] span{display:block}
body.themed .layered-preview>[class^="vendor-"] .clear{clear:both}
body.themed .layered-preview figcaption{display:none}
body.themed .layered-preview .pad-button{border:0;border-radius:0;padding:0;min-width:0;min-height:0;box-shadow:none;transition:none}
body.themed .vendor-dualsense{position:absolute;left:0;top:0;width:1200px;height:700px;transform-origin:0 0}
body.themed .vendor-dualsense .controller{position:relative}
```

- [ ] **Step 5: Run the checks**

- `node Tools/check-save-lane.mjs`: PASS.
- `node Tools/check-theme-tokens.mjs`: PASS.
- The standing checks: all PASS (the three pages still load their legacy sheets; nothing on them changes yet).

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/ui/savebar.js crates/reborn-ui/ui/theme.css Tools/check-save-lane.mjs
git commit -m "ui: save bar lock, thumb and solid-surface tokens, search/textarea fields, controller art on themed pages

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Mods on the design system

**Files:**
- Modify: `crates/reborn-ui/ui/mods.html`
- Modify: `crates/reborn-ui/ui/mods.js`
- Create: `crates/reborn-ui/ui/pages/mods.css`
- Modify: `Tools/check-mods-page.mjs`, `Tools/check-theme.mjs`, `Tools/check-themed-shell.mjs`, `Tools/run-browser-tests.mjs`

**Interfaces:**
- Consumes: `tabs(group, names, current, onPick, { label })` from `tabs.js` (plan 2); `icon(paths)` from `shell.js` (plan 2); `.btn`, `.btn.quiet`, `.glass`, `.field`, `.look-switch`, `.tabs`; `--thumb-bg` (Task 1).
- Produces (the §13 check in Task 5 uses these):
  - `.mods-toolbar` (search `.field.mods-search`, **Import mods** `a.btn`, **Refresh** `.btn.quiet`);
  - the tabs `div.tabs` named "Mod categories";
  - `p.mods-count` e.g. `3 footballs · 1 enabled · only one football can be on at a time`;
  - `section.store-grid` of `article.store-card.glass[.active]`, each with:
    - `button.store-open[aria-label="About <name>"]` holding `span.store-art` (+ `span.store-check` when enabled) and the name/meta;
    - `.store-card-actions` with `span.store-state` ("Enabled"/"Off") and **one** `button.look-switch[role=switch][aria-label="<name>"][aria-checked]`.

- [ ] **Step 1: Update the checks (they fail now)**

`Tools/check-theme.mjs`: replace `const THEMED = { index: "pages/play.css", settings: "pages/settings.css" };` with

```js
const THEMED = { index: "pages/play.css", settings: "pages/settings.css", mods: "pages/mods.css" };
```

`Tools/check-themed-shell.mjs`: Mods is no longer a legacy page. Replace

```js
 await legacy.goto(base+'/mods.html');await legacy.locator('.shell-nav a').first().waitFor();await legacy.evaluate(()=>document.fonts.ready);
```

with

```js
 // Online keeps its own layout (spec §9), so it is the page that still loads the legacy sheets.
 await legacy.goto(base+'/online.html');await legacy.locator('.shell-nav a').first().waitFor();await legacy.evaluate(()=>document.fonts.ready);
```

`Tools/check-mods-page.mjs`:

**(a)** `mods.js` now imports `./tabs.js` and `./shell.js`. A `data:` module cannot resolve relative imports, so point them at the real files. Replace

```js
import { readFileSync } from "fs";

const file = process.env.MODS_JS || "crates/reborn-ui/ui/mods.js";
const src = readFileSync(file, "utf8");
```

with

```js
import { readFileSync } from "fs";
import { resolve } from "path";
import { pathToFileURL } from "url";

const file = process.env.MODS_JS || "crates/reborn-ui/ui/mods.js";
// mods.js imports its neighbours (tabs.js, shell.js). A data: module cannot resolve "./x.js", so
// those imports point at the real files in the UI folder (also when MODS_JS is a copy elsewhere).
const UI_DIR = resolve("crates/reborn-ui/ui");
const src = readFileSync(file, "utf8").replace(/from\s*(["'])\.\/([\w.-]+)\1/g, (_, q, f) => `from ${q}${pathToFileURL(resolve(UI_DIR, f)).href}${q}`);
```

**(b)** Replace everything from the line `// Store behavior, using the same catalog and core mutation contract.` down to (and including) the line `check("enable sends the package identity",calls.some(([cmd,args])=>cmd==="mods_toggle"&&args.id==="jsgme.aqua-football"&&args.on===true));` with:

```js
// Store behavior, using the same catalog and core mutation contract.
const hasClass = (n, c) => String(n.className || "").split(" ").includes(c);
const cards = () => walk(screen()).filter((n) => hasClass(n, "store-card"));
const switchFor = (name) => walk(screen()).find((n) => n.attrs.role === "switch" && n.attrs["aria-label"] === name);
check("store starts with installed footballs", cards().length === 7);
check("button texture packs stay in Controllers", !walk(screen()).some((n) => n.attrs["aria-label"] === "XBOX Controls"));
check("stale activations remain removable", text(screen()).includes("jsgme.ghost-football"));
check("unreadable packages remain visible", text(screen()).includes("not a readable manifest"));
// The glass cards (spec §8.3): one switch each, a red check on the enabled one, and no drawn art.
check("one switch per card", cards().every((c) => walk(c).filter((n) => n.attrs.role === "switch").length === 1));
check("the switch says the state", switchFor("Purple Football")?.attrs["aria-checked"] === "true" && switchFor("Aqua Football")?.attrs["aria-checked"] === "false");
check("the enabled card carries the check", cards().filter((c) => walk(c).some((n) => hasClass(n, "store-check"))).length === 1 && cards().find((c) => hasClass(c, "active")) && walk(cards().find((c) => hasClass(c, "active"))).some((n) => hasClass(n, "store-check")));
check("no drawn art: the preview has no colour of its own", walk(screen()).filter((n) => hasClass(n, "store-art")).every((n) => !n.style.backgroundColor));
check("the count line says how many, how many are on, and the pick-one rule", text(screen()).includes("7 footballs · 1 enabled · only one football can be on at a time"), text(screen()).slice(0, 300));
check("the categories are the shared sliding tabs", walk(screen()).some((n) => hasClass(n, "tabs") && walk(n).some((c) => hasClass(c, "tab-ind"))));
await switchFor("Aqua Football").listeners.click[0]();
{
  const tab = walk(screen()).find((n) => n.tagName === "button" && n.textContent === "Soundtrack");
  tab.listeners.click[0]();
  check("the Soundtrack tab lists the soundtrack packages", cards().length === 3 && text(screen()).includes("NFL Street 1 Soundtrack") && text(screen()).includes("NFL Street Reborn Soundtrack"));
  const enable = switchFor("NFL Street 1 Soundtrack");
  check("a soundtrack package offers a switch", !!enable && enable.attrs["aria-checked"] === "false");
  await enable.listeners.click[0]();
  await tick(); await tick(); await tick();
  check("switching a soundtrack package shows what the disc rebuild did", text(screen()).includes("14 custom songs"));
  const back = walk(screen()).find((n) => n.tagName === "button" && n.textContent === "Footballs");
  back.listeners.click[0]();
}
check("enable sends the package identity", calls.some(([cmd, args]) => cmd === "mods_toggle" && args.id === "jsgme.aqua-football" && args.on === true));
```

`Tools/run-browser-tests.mjs`: replace

```js
await page.getByRole('button',{name:'Enable Black football',exact:true}).click();
await page.waitForFunction(()=>window.__fixture.mod_packages.packages.find(p=>p.id==='football-black').active);
```

with

```js
await page.getByRole('switch',{name:'Black football',exact:true}).click();
await page.waitForFunction(()=>window.__fixture.mod_packages.packages.find(p=>p.id==='football-black').active);
// Review Focus 4: a pick-one group. After the core answers, one card is on, with the check, and the other switch reads off.
await page.waitForFunction(()=>document.querySelector('[role=switch][aria-label="Black football"]')?.getAttribute('aria-checked')==='true');
assert.equal(await page.getByRole('switch',{name:'Red football',exact:true}).getAttribute('aria-checked'),'false');
assert.equal(await page.locator('.store-card.active').count(),1);
assert.equal(await page.locator('.store-card.active .store-check').count(),1);
assert.match(await page.locator('.store-card.active').innerText(),/Black football[\s\S]*Enabled/);
```

Run (Playwright linked):
- `node Tools/check-theme.mjs`. Expected: FAIL (`mods: must load exactly theme.css + pages/mods.css`).
- `node Tools/check-mods-page.mjs`. Expected: FAIL on `one switch per card` and the checks after it.

- [ ] **Step 2: `mods.html` becomes theme-only**

Replace the whole file with the following. The `<nav class="shell-nav" …>…</nav>` block must stay **byte-identical** to today's (copy it from the current file, lines 15-21), and so must the `<details class="core">…</details>` line.

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Make it yours | NFL Street Reborn</title>
  <script src="look-boot.js"></script>
  <link rel="icon" href="art/cover.png">
  <link rel="stylesheet" href="theme.css"><link rel="stylesheet" href="pages/mods.css">
</head>
<body class="shell themed page-mods">
  <a class="skip-link" href="#main">Skip to content</a>
  <header class="app-header"><span id="bridge" class="pill">Connecting</span></header>
  <!-- the existing <nav class="shell-nav" aria-label="Main navigation"> … </nav> block, unchanged -->
  <main id="main" tabindex="-1">
    <div class="page-heading"><h1>Mods</h1><p class="purpose">Choose your football and soundtrack.</p></div>
    <section id="screen"><p class="loading">Loading…</p></section>
    <!-- the existing <details class="core"> … </details> line, unchanged -->
  </main>
  <footer class="input-footer"><span id="input-hints"></span></footer>
  <script type="module" src="app-shell.js"></script><script type="module" src="mods.js"></script>
</body>
</html>
```

(Replace the two `<!-- … -->` lines with the real blocks; do not leave the comments in.)

- [ ] **Step 3: `pages/mods.css`**

Create `crates/reborn-ui/ui/pages/mods.css`:

```css
/* Mods (spec §8.3; mockup "MODS"). Layout only: every colour, radius and duration comes from theme.css.
   A toolbar, the sliding category tabs, a count line, then three columns of cards with one switch each. */
body.page-mods #main{padding:36px 36px 32px}
.page-mods .page-heading,.page-mods #screen{max-width:942px}
.page-mods #screen{display:flex;flex-direction:column}
.page-mods #screen>.refusal,.page-mods #screen>.partial,.page-mods .loading{margin-top:24px}

.mods-toolbar{display:flex;align-items:center;gap:8px;margin-top:24px}
.mods-search{flex:0 1 420px;min-width:0}
.mods-search svg{width:15px;height:15px;flex:none;color:var(--fg-3)}
/* the link (Import mods) is the first button-looking item after the search box; :first-of-type would also match the Refresh <button> */
.mods-toolbar>a.btn{margin-left:auto}
.page-mods .tabs{align-self:flex-start;margin-top:24px}
.mods-count{margin:16px 0;font-size:12.5px;color:var(--fg-3)}

/* cards: a neutral preview (packages ship no art), name and meta, then Enabled/Off and one switch */
.store-grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:16px}
.store-card{display:flex;flex-direction:column;min-width:0;overflow:hidden}
.store-open{display:flex;flex-direction:column;width:100%;padding:0;border:0;border-radius:0;background:transparent;color:var(--fg);font:inherit;text-align:left;cursor:pointer}
.store-art{position:relative;display:block;height:150px;background:var(--thumb-bg);border-bottom:1px solid var(--stroke)}
.store-check{position:absolute;top:12px;right:12px;display:grid;place-items:center;width:24px;height:24px;border-radius:50%;background:var(--red);color:var(--fg);box-shadow:var(--sheen)}
.store-check svg{width:14px;height:14px;stroke-width:3}
.store-card-body{display:block;padding:16px 16px 8px}
.store-card-body strong{display:block;font-size:14.5px;font-weight:600;overflow-wrap:anywhere}
.store-card-body small{display:block;margin-top:2px;font-size:12px;color:var(--fg-3)}
.store-card-actions{display:flex;align-items:center;justify-content:space-between;gap:8px;margin-top:auto;padding:12px 16px;border-top:1px solid var(--stroke)}
.store-state{font-size:12.5px;color:var(--fg-3)}
.store-card.active{border-color:var(--stroke-hi)}
.store-card.active .store-state{color:var(--fg)}
/* the card clips its corners, so the focus ring sits inside the preview button */
body.shell.page-mods .store-card .store-open:focus-visible,body.gamepad-input.page-mods .store-card .store-open.pad-focus{outline-offset:-4px!important}
.store-empty{grid-column:1/-1;display:flex;flex-direction:column;align-items:center;gap:8px;padding:48px 24px;text-align:center}
.store-empty h2{font-size:16px;font-weight:600}
.store-empty p{max-width:460px;font-size:12.5px;color:var(--fg-2)}
.store-empty .btn{margin-top:8px}

/* missing and unreadable packages */
.page-mods .block{margin-top:24px;padding:16px}
.page-mods .block h2{font-size:15px;font-weight:600}
.page-mods .block .note,.page-mods .block .partial{margin-top:8px}
.page-mods .mod{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-top:8px;padding-top:8px;border-top:1px solid var(--stroke);font-size:13px}
.page-mods .mod .what{min-width:0;overflow-wrap:anywhere}
.store-footnote{margin-top:24px;font-size:12.5px;color:var(--fg-3)}

/* details: the card's preview and name open it */
body.page-mods dialog.mod-dialog{width:min(480px,calc(100vw - 48px));padding:0;overflow:hidden}
.mod-dialog>.store-art{height:150px}
.mod-dialog-body{display:flex;flex-direction:column;gap:8px;padding:24px}
.mod-dialog-body h2{font-size:20px;font-weight:600;letter-spacing:-.02em}
.mod-dialog-body p{font-size:13px;color:var(--fg-2)}
.store-kicker{font-size:11.5px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--fg-3)}
.store-dialog-actions{display:flex;justify-content:flex-end;gap:8px;margin-top:16px}

@media (max-width:900px){body.page-mods #main{padding:24px 24px 16px}}
@media (max-width:760px){.store-grid{grid-template-columns:repeat(2,minmax(0,1fr))}.mods-toolbar{flex-wrap:wrap}.mods-search{flex-basis:100%}}
```

- [ ] **Step 4: `mods.js`: toolbar, tabs, count line, cards**

Every function not named here stays exactly as it is (in particular `load`, `act`, `refresh`, `staleSection`'s logic, `unreadableSection` and the boot block; `act` must keep its `withheld_note` line).

**(a)** At the very top of the file, above `// Installed mod collection. …`, add:

```js
import { icon } from "./shell.js";
import { tabs } from "./tabs.js";
```

**(b)** In `staleSection()`, change `el("section", { class: "block" }, el("h2", { text: "Missing mod files" }))` to `el("section", { class: "block glass" }, el("h2", { text: "Missing mod files" }))`. In `unreadableSection()`, change `el("section", { class: "block" }, el("h2", { text: "Unavailable mods" }))` to `el("section", { class: "block glass" }, el("h2", { text: "Unavailable mods" }))`.

**(c)** Replace everything from the line `let query = '', category = 'Footballs', selectedMod = null;` down to the end of `function draw() { … }` (the line `  screen.replaceChildren(...parts);return parts;` and its closing `}`) with:

```js
let query = '', category = 'Footballs';
const categoryOf = p => p.kind==='soundtrack'?'Soundtrack':/football/i.test(p.name+' '+p.id)?'Footballs':/soundtrack|music|audio/i.test(p.name+' '+p.id)?'Soundtrack':null;
const NOUN = { Footballs: ['football', 'footballs'], Soundtrack: ['soundtrack', 'soundtracks'] };
const SEARCH_ICON = ['M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14Z', 'M20 20l-4-4'];
const CHECK_ICON = 'M5 12l5 5 9-10';

// "3 footballs · 1 enabled · only one football can be on at a time" (spec §8.3). The pick-one rule is
// said only when the core's exclusive groups hold more than one of the packages listed.
function countLine(list) {
  const [one, many] = NOUN[category] || ['mod', 'mods'];
  const on = list.filter((p) => p.active).length;
  const groups = library && Array.isArray(library.exclusive) ? library.exclusive : [];
  const pickOne = groups.some((g) => list.filter((p) => (g.members || []).includes(p.id)).length > 1);
  return [`${list.length} ${list.length === 1 ? one : many}`, `${on} enabled`, pickOne ? `only one ${one} can be on at a time` : null].filter(Boolean).join(' · ');
}

// The same core call the Enable/Disable buttons made. A soundtrack rebuilds the disc, so it says so first.
function toggle(p) {
  if (p.kind === 'soundtrack') { notice = 'Rebuilding your game disc with this soundtrack…'; draw(); } else notice = null;
  return act('mods_toggle', { id: p.id, version: p.version, on: !p.active, runtime: runtimeHint() });
}

// One card (spec §8.3): a neutral preview (packages ship no art, so nothing is drawn), the name and
// meta, and one switch. The enabled card carries a red check. The preview and name open the details.
function storeCard(p) {
  const name = p.name || p.id;
  const art = el('span', { class: 'store-art', 'aria-hidden': 'true' }, p.active ? el('span', { class: 'store-check' }, icon(CHECK_ICON)) : null);
  const open = el('button', { type: 'button', class: 'store-open', 'aria-label': `About ${name}`, onclick: () => showMod(p) }, art,
    el('span', { class: 'store-card-body' }, el('strong', { text: name }), el('small', { text: categoryOf(p) + ' · ' + (p.version ? 'Version ' + p.version : 'Installed') })));
  const flip = el('button', { type: 'button', class: 'look-switch', role: 'switch', 'aria-checked': String(!!p.active), 'aria-label': name, disabled: busy, onclick: () => toggle(p) });
  return el('article', { class: 'store-card glass' + (p.active ? ' active' : '') }, open,
    el('div', { class: 'store-card-actions' }, el('span', { class: 'store-state', text: p.active ? 'Enabled' : 'Off' }), flip));
}
function showMod(p) {
  document.querySelector('.mod-dialog')?.remove();
  const dialog=el('dialog',{class:'mod-dialog','aria-label':p.name||p.id});
  const art=el('div',{class:'store-art','aria-hidden':'true'});
  const conflict=(library?.exclusive||[]).find(g=>(g.members||[]).includes(p.id));
  dialog.append(art,el('div',{class:'mod-dialog-body'},el('span',{class:'store-kicker',text:categoryOf(p)}),el('h2',{text:p.name||p.id}),el('p',{text:p.description||'Customize your game with this installed Reborn mod.'}),el('p',{class:'note',text:['Version '+p.version,mb(p.bytes),typeof p.files==='number'?p.files+' files':null].filter(Boolean).join(' · ')}),conflict?el('p',{class:'note',text:'Enabling this mod replaces other enabled mods that change the same files.'}):null,el('div',{class:'store-dialog-actions'},el('button',{type:'button',text:'Close',onclick:()=>dialog.close()}),el('button',{type:'button',class:'primary',text:p.active?'Disable mod':'Enable mod',disabled:busy,onclick:()=>{dialog.close();act('mods_toggle',{id:p.id,version:p.version,on:!p.active,runtime:runtimeHint()});}}))));
  dialog.addEventListener('close',()=>dialog.remove());document.body.append(dialog);dialog.showModal();
}
function draw() {
  const screen=document.getElementById('screen'),parts=[];
  if(problem)parts.push(el('p',{class:'refusal',role:'alert',text:problem}));
  if(notice)parts.push(el('p',{class:'partial',role:'status',text:notice}));
  const all=packages().filter(p=>!managed(p.id));
  const search=el('input',{type:'search',placeholder:'Search your mods','aria-label':'Search mods',value:query});
  const results=el('section',{class:'store-grid','aria-label':'Mods'}),count=el('p',{class:'mods-count',role:'status'});
  const renderResults=()=>{
    const list=all.filter(p=>categoryOf(p)===category&&((p.name||'')+' '+p.id).toLowerCase().includes(query.toLowerCase()));
    count.textContent=countLine(list);
    results.replaceChildren(...list.map(storeCard));
    if(!list.length)results.append(el('div',{class:'store-empty glass'},el('h2',{text:category==='Soundtrack'&&all.length?'No soundtrack mods yet':all.length?'No matching mods':'Your mod library starts here'}),el('p',{text:category==='Soundtrack'&&all.length?'Soundtrack mods such as the NFL Street 1 soundtrack arrive with game content updates. Your own songs live on the Music page.':all.length?'Try another search or category.':'Import your existing installation to bring your mods with you.'}),!all.length?el('a',{href:'onboarding.html',class:'btn',text:'Import mods'}):null));
  };
  search.addEventListener('input',()=>{query=search.value;renderResults();});
  parts.push(el('div',{class:'mods-toolbar'},el('label',{class:'field mods-search'},icon(SEARCH_ICON),search),el('a',{href:'onboarding.html',class:'btn',text:'Import mods'}),el('button',{type:'button',class:'btn quiet',text:'Refresh',disabled:busy,onclick:refresh})));
  parts.push(tabs('mods',['Footballs','Soundtrack'],category,(name)=>{category=name;draw();},{label:'Mod categories'}));
  parts.push(count,results);renderResults();
  const stale=staleSection(),unreadable=unreadableSection();if(stale)parts.push(stale);if(unreadable)parts.push(unreadable);
  parts.push(el('p',{class:'store-footnote',text:'Mods are applied automatically when you start the game. Button textures are in Controllers.'}));
  screen.replaceChildren(...parts);return parts;
}
```

Notes:
- The old per-card colour list (`colors`) is gone on purpose: no mock art (Adam, 09-30).
- `mods.js` keeps its own `el`, `call`, `reason` and `mb`; only `icon` and `tabs` are imported.

- [ ] **Step 5: Run the checks**

- `node Tools/check-theme.mjs` and `node Tools/check-theme-tokens.mjs`: PASS (3 page stylesheets).
- `node Tools/check-mods-page.mjs`: PASS (`PASS: mod store preserves core actions and failure states`).
- `node Tools/check-themed-shell.mjs`: PASS (sidebars match against `online.html`).
- `node Tools/check-page-call-args.mjs`: PASS (`mods_toggle` sends id, version, on, runtime at both call sites).
- The standing checks: PASS. The `run-browser-tests` main body prints its PASS line and saves `target/browser-review/reborn-launcher-mods.png`.

Open `target/browser-review/reborn-launcher-mods.png` beside the mockup's MODS section. Check: toolbar (search, Import mods, Refresh), sliding Footballs/Soundtrack tabs, the count line, glass cards with a plain neutral preview, the red check on the enabled card only, "Enabled"/"Off" with one switch, no hint bar.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/ui/mods.html crates/reborn-ui/ui/mods.js crates/reborn-ui/ui/pages/mods.css Tools/check-mods-page.mjs Tools/check-theme.mjs Tools/check-themed-shell.mjs Tools/run-browser-tests.mjs
git commit -m "mods: theme-only page; sliding tabs, count line, glass cards with one switch and a check on the enabled card

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Music on the design system, pending changes through the save bar

**Files:**
- Modify: `crates/reborn-ui/ui/music.html`
- Modify: `crates/reborn-ui/ui/music.js`
- Create: `crates/reborn-ui/ui/pages/music.css`
- Modify: `Tools/check-music-page.mjs`, `Tools/check-shell-chrome.mjs`, `Tools/check-theme.mjs`

**Interfaces:**
- Consumes: `mountSaveBar(lane, { noun, saveText, onSave, onDiscard, returnFocus })` with `update`, `fail` (plan 2) and `lock` (Task 1); `.look-switch`, `.btn`, `.btn.sm`, `.btn.quiet`, `.glass`; `--surface-solid` (Task 1).
- Produces (Task 5 uses these):
  - body class `has-save-lane` and `<div id="save-lane" inert></div>` after `</main>`;
  - `#music-enabled` (`button.look-switch[role=switch]`, "Custom soundtrack");
  - header buttons **Reset** (accessible name "Reset soundtrack"), **Import pack**, **Export pack** (all `.btn.quiet`) and **Add songs…** (`.btn`). There is no "Add song" button;
  - `section#disc.glass.mz-disc` with `table.mz-table`; rows `tr.mz-track[data-slot][.staged][.custom]` with cells `.mz-col-n`, `.mz-col-mv`, `.mz-col-what` (song button + `span.mz-src`, whose pending suffix is `span.pend` " · Pending"), `.mz-col-time`, `.mz-col-actions`;
  - the bar: "N pending change(s)", the last change named ("Song 3 replaced", "Song 2 restored", "Song 6 moved to 5", "Song 21 details changed"), **Discard** and **Apply changes**.

What goes through the bar and what does not:
- **The bar:** Apply changes (`music_apply {confirm:true}`) and Discard (`music_unstage {slot:null}`), the two buttons of the old `.music-apply` panel, which is removed.
- **Unchanged, as today:** every staging action (Replace, Restore original, Undo, reorder, Import pack, Add songs…, song details) stages in the core at once; the Custom soundtrack switch; Reset; Export. The core owns the pending list; the bar only shows it.

- [ ] **Step 1: Update the checks (they fail now)**

`Tools/check-theme.mjs`: replace the `THEMED` line with

```js
const THEMED = { index: "pages/play.css", settings: "pages/settings.css", mods: "pages/mods.css", music: "pages/music.css" };
```

`Tools/check-shell-chrome.mjs`: replace **both** occurrences of `{name:'Add song',exact:true}` with `{name:'Add songs…',exact:true}`. (Before this task, run `node Tools/check-shell-chrome.mjs` once and record its result. If it already fails before this task at a line unrelated to Music, record that as pre-existing and expect the same failure after.)

`Tools/check-music-page.mjs`:

**(a)** Replace

```js
 assert.ok((await apply.boundingBox()).y<900,'Apply is visible without scrolling through the list');
 await page.getByRole('button',{name:'Add song',exact:true}).click();
```

with

```js
 assert.ok((await apply.boundingBox()).y<900,'Apply is visible without scrolling through the list');
 // The save bar (spec §7, §8.4): its own lane under the page, the pending count, the last change named.
 await page.locator('.save-lane.open').waitFor();
 assert.match(await page.locator('.save-bar').innerText(),/1 pending change[\s\S]*Song 3 restored[\s\S]*Discard[\s\S]*Apply changes/);
 await page.waitForTimeout(400);
 const edge=await page.evaluate(()=>({main:document.getElementById('main').getBoundingClientRect().bottom,lane:document.getElementById('save-lane').getBoundingClientRect().top}));
 assert.ok(Math.abs(edge.main-edge.lane)<1,'the bar never covers the list');
 assert.equal(await page.getByRole('button',{name:'Add song',exact:true}).count(),0,'the duplicate Add song is gone (spec §8.4)');
 // A pending row: "· Pending" ends its source line, and Undo is its only action.
 assert.match(await page.locator('[data-slot="3"] .mz-src').innerText(),/· Pending$/);
 assert.deepEqual(await page.locator('[data-slot="3"] .mz-col-actions button').allInnerTexts(),['Undo']);
 // A custom song: Restore original and Replace. The disc's own song: Replace.
 assert.deepEqual(await page.locator('[data-slot="21"] .mz-col-actions button').allInnerTexts(),['Restore original','Replace']);
 assert.deepEqual(await page.locator('[data-slot="1"] .mz-col-actions button').allInnerTexts(),['Replace']);
 assert.match(await page.locator('[data-slot="21"] .mz-src').innerText(),/Alicia Keys · Your song/);
 await page.getByRole('button',{name:'Replace song 1',exact:true}).click();
```

**(b)** Replace

```js
 assert.match(await page.locator('[data-slot="2"]').innerText(),/My edited song/);
```

with

```js
 assert.match(await page.locator('[data-slot="2"]').innerText(),/My edited song/);
 assert.match(await page.locator('[data-slot="2"] .mz-src').innerText(),/^Replacing Original song 2 · Pending$/);
 assert.match(await page.locator('.save-bar').innerText(),/2 pending changes[\s\S]*Song 2 replaced/);
```

**(c)** Replace

```js
 assert.match(await page.locator('[data-slot="21"]').innerText(),/Restore original/);
```

with

```js
 assert.match(await page.locator('[data-slot="21"] .mz-src').innerText(),/^Restoring the original · Pending$/);
 assert.match(await page.locator('.save-bar').innerText(),/Song 21 restored/);
```

**(d)** Replace

```js
 await page.getByRole('switch',{name:'Custom soundtrack'}).uncheck();
 assert.ok(await page.evaluate(()=>window.__musicCalls.some(c=>c.cmd==='music_set_enabled'&&c.args.on===false)));
 await apply.click();
```

with

```js
 await page.getByRole('switch',{name:'Custom soundtrack'}).click();
 assert.ok(await page.evaluate(()=>window.__musicCalls.some(c=>c.cmd==='music_set_enabled'&&c.args.on===false)));
 await page.waitForFunction(()=>document.getElementById('music-enabled')?.getAttribute('aria-checked')==='false');
 // Review Focus 1: a failed apply says why in the bar, keeps every change pending, and gives the buttons back.
 await page.evaluate(()=>window.__musicFail='music_apply');
 await apply.click();
 await page.locator('.save-bar [role=alert]').waitFor();
 assert.equal(await page.locator('.save-bar [role=alert]').innerText(),'Could not save this song. Try again.');
 assert.equal(await page.locator('.save-lane.open').count(),1,'the changes stay pending');
 assert.equal(await apply.isEnabled(),true,'Apply comes back after a failure');
 await page.evaluate(()=>window.__musicFail=null);
 await apply.click();
```

(The existing next lines, `progressbar … waitFor()` and `assert.equal(await apply.isDisabled(),true);`, now also pin Review Focus 3: Apply stays held while the core builds.)

**(e)** Replace

```js
 await page.getByText('Your soundtrack is up to date.',{exact:true}).waitFor({timeout:4000});
```

with

```js
 await page.getByText('Your soundtrack is up to date.',{exact:true}).waitFor({timeout:4000});
 await page.waitForFunction(()=>!document.querySelector('.save-lane.open'),null,{timeout:4000});
```

**(f)** In the final `for(const width of [800,1280]){ … }` loop, replace `await page.getByRole('button',{name:'Add song',exact:true}).click();` with `await page.getByRole('button',{name:'Replace song 1',exact:true}).click();`.

Run (Playwright linked):
- `node Tools/check-theme.mjs`. Expected: FAIL (`music: must load exactly theme.css + pages/music.css`).
- `node Tools/check-music-page.mjs`. Expected: FAIL (timeout on `.save-lane.open`).

- [ ] **Step 2: `music.html` becomes theme-only, with a save lane**

Replace the whole file with the following. Keep the `<nav …>…</nav>` block (current lines 16-22) and the `<details class="core">…</details>` line **byte-identical**, replacing the two comments.

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Music | NFL Street Reborn</title>
  <script src="look-boot.js"></script>
  <link rel="icon" href="art/cover.png">
  <link rel="stylesheet" href="theme.css"><link rel="stylesheet" href="pages/music.css">
</head>
<body class="shell themed page-music has-save-lane">
  <a class="skip-link" href="#main">Skip to content</a>
  <header class="app-header"><span id="bridge" class="pill">Connecting</span></header>
  <!-- the existing <nav class="shell-nav" aria-label="Main navigation"> … </nav> block, unchanged -->
  <main id="main" tabindex="-1">
    <div class="page-heading"><h1>Music</h1><p class="purpose">Replace songs and build your own soundtrack.</p></div>
    <section id="screen"><p class="loading">Loading…</p></section>
    <!-- the existing <details class="core"> … </details> line, unchanged -->
  </main>
  <div id="save-lane" inert></div>
  <footer class="input-footer"><span id="input-hints"></span></footer>
  <script type="module" src="app-shell.js"></script><script type="module" src="music.js"></script>
</body>
</html>
```

- [ ] **Step 3: `pages/music.css`**

Create `crates/reborn-ui/ui/pages/music.css`:

```css
/* Music (spec §8.4; mockup "MUSIC"). Layout only: colours, radii and durations come from theme.css.
   The header row, the soundtrack table, then the save bar in its own lane (theme.css, has-save-lane). */
body.page-music #main{padding:36px 36px 32px}
.page-music .page-heading,.page-music #screen{max-width:942px}
.page-music #screen{display:flex;flex-direction:column;gap:16px;margin-top:24px}
body.page-music .save-bar{margin:8px 36px 16px}

/* header row: the switch on the left, the pack actions on the right */
.music-toolbar{display:flex;align-items:center;justify-content:space-between;gap:16px;flex-wrap:wrap}
.music-toggle{display:flex;align-items:center;gap:12px;min-width:0}
.music-toggle label{display:flex;flex-direction:column;cursor:pointer}
.music-toggle .t{font-size:14px;font-weight:600}
.music-toggle .d{font-size:12.5px;color:var(--fg-2)}
.music-toolbar-actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap}

/* the soundtrack table: # | order | song + source line | length | actions */
.mz-disc{overflow:hidden}
.music-list-heading{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:16px 20px 8px}
.music-list-heading h2{font-size:15px;font-weight:600}
.music-list-heading>span{font-size:12.5px;color:var(--fg-3)}
.mz-table{width:100%;border-collapse:collapse;table-layout:fixed;font-size:13px}
.mz-table th{padding:8px;border-top:1px solid var(--stroke);font-size:10.5px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;text-align:left;color:var(--fg-3)}
.mz-table td{padding:8px;border-top:1px solid var(--stroke);vertical-align:middle;overflow-wrap:anywhere}
.mz-table :is(th,td):first-child{padding-left:20px}
.mz-table :is(th,td):last-child{padding-right:20px}
.mz-table .mz-col-n{width:64px;color:var(--fg-3);font-size:12.5px;font-variant-numeric:tabular-nums}
.mz-table .mz-col-mv{width:36px}
.mz-table .mz-col-time{width:90px;color:var(--fg-2);font-size:12.5px;font-variant-numeric:tabular-nums}
.mz-table .mz-col-actions{width:260px;text-align:right;white-space:nowrap}
.mz-col-actions .btn+.btn{margin-left:8px}
.mz-move{display:flex;flex-direction:column}
.mz-move button{height:14px;min-height:0;padding:0 4px;border:0;background:transparent;color:var(--fg-3);font-size:9px;line-height:1;cursor:pointer}
.mz-move button:hover:not(:disabled){color:var(--fg)}
.mz-move button:disabled{opacity:.25;cursor:default}
.music-song-name{display:block;max-width:100%;padding:0;border:0;background:transparent;color:var(--fg);font:600 13.5px Inter,system-ui,sans-serif;text-align:left;cursor:pointer}
.music-song-name:hover:not(:disabled){text-decoration:underline;text-underline-offset:3px}
.mz-src{display:block;margin-top:2px;font-size:12px;color:var(--fg-3)}
.mz-track.custom .mz-src{color:var(--fg-2)}
.mz-src .pend{color:var(--red-ink)}
/* a pending row: a faint red tint and a red left edge (spec §8.4) */
.mz-table tr.staged td{background:color-mix(in srgb,var(--red) 6%,transparent)}
.mz-table tr.staged td:first-child{position:relative}
.mz-table tr.staged td:first-child::before{content:"";position:absolute;left:0;top:8px;bottom:8px;width:3px;border-radius:0 3px 3px 0;background:var(--red)}
/* reordering (music.js): drag a row with the mouse (a ghost follows, a line marks where it lands), or the arrows */
.mz-table tbody tr.mz-track{cursor:grab}
.mz-table tr.dragging{opacity:.4}
.mz-table tr.drop-above td{box-shadow:inset 0 3px 0 var(--red)}
.mz-table tr.drop-below td{box-shadow:inset 0 -3px 0 var(--red)}
body.mz-dragging{cursor:grabbing;user-select:none}
body.mz-dragging .mz-table tbody tr.mz-track{cursor:grabbing}
.mz-ghost{position:fixed;left:0;top:0;z-index:20;display:flex;align-items:center;gap:16px;padding:8px 16px;border-radius:var(--r-md);border:1px solid var(--red);background:var(--surface-solid);box-shadow:var(--shadow);font-size:13px;pointer-events:none}
.mz-ghost td{display:inline;padding:0;border:0;background:none}
.mz-ghost td::before{display:none}
.mz-ghost :is(.mz-col-mv,.mz-col-actions,.mz-col-time,.mz-src){display:none}

/* progress, done, messages */
.music-progress{padding:16px 20px}
.music-progress>div{display:flex;justify-content:space-between;gap:16px;font-size:13px}
.music-progress strong{font-weight:600}
.music-progress progress{display:block;width:100%;height:6px;margin:16px 0 8px}
.music-progress .note{font-size:12px;color:var(--fg-2)}
.mz-done{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:12px 16px;font-size:12.5px}
.music-error{padding:12px 16px;border-radius:var(--r-md);border:1px solid color-mix(in srgb,var(--red) 35%,transparent);background:color-mix(in srgb,var(--red) 8%,transparent);color:var(--red-ink);font-size:12.5px;overflow-wrap:anywhere}
.music-empty{padding:48px 24px;text-align:center}
.music-empty h3{margin-bottom:8px;font-size:16px;font-weight:600}
.music-empty button{margin-top:16px}
.music-footnote{font-size:12.5px;color:var(--fg-3)}

/* dialogs: song details, import a pack, add songs, share, reset */
body.page-music dialog.music-dialog{width:min(570px,calc(100vw - 40px));max-height:calc(100dvh - 48px);padding:0;overflow:auto}
.music-editor-form,.music-dialog .mod-dialog-body{display:flex;flex-direction:column;gap:16px;padding:24px}
.music-dialog-heading{display:flex;align-items:center;justify-content:space-between;gap:16px}
.music-dialog h2{font-size:20px;font-weight:600;letter-spacing:-.02em}
.music-dialog .mod-dialog-body>p:not([class]){font-size:13px;color:var(--fg-2)}
.music-file{display:flex;align-items:center;gap:16px;padding:12px 16px;border:1px solid var(--stroke);border-radius:var(--r-md)}
.music-file>div{flex:1;min-width:0}
.music-file strong{display:block;font-size:13px;font-weight:500;overflow-wrap:anywhere}
.music-file small{display:block;margin-top:4px;font-size:12px;color:var(--fg-2)}
.mz-fields{display:grid;grid-template-columns:1fr 1fr;gap:16px}
.mz-fields>.mz-field:first-child{grid-column:1/-1}
.mz-field{display:flex;flex-direction:column;gap:8px;font-size:12.5px;color:var(--fg-2)}
.mz-field :is(input,select){width:100%;min-width:0}
.music-dialog .note{font-size:12px;color:var(--fg-2)}
.music-extra{padding-top:16px;border-top:1px solid var(--stroke)}
.music-extra summary{font-size:12.5px;color:var(--fg-2);cursor:pointer}
.music-extra .mz-row{display:flex;gap:8px;margin-top:16px}
.music-extra .mz-row input{flex:1;min-width:0}
.music-extra .mz-privacy{margin-top:8px}
.mz-candidates{display:flex;flex-direction:column;gap:8px;max-height:155px;margin:16px 0 0;padding:0;list-style:none;overflow:auto}
.mz-candidate{width:100%;padding:8px 12px;border-radius:var(--r-sm);border:1px solid var(--stroke);background:var(--glass-1);color:var(--fg);font:500 12px Inter,system-ui,sans-serif;text-align:left;white-space:normal;cursor:pointer}
.mz-candidate:hover:not(:disabled){background:var(--glass-2)}
.music-dialog-footer{display:flex;align-items:center;justify-content:space-between;gap:16px;padding-top:16px;border-top:1px solid var(--stroke)}
.music-dialog-footer .note{max-width:240px}
.store-kicker{font-size:11.5px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--fg-3)}
.store-dialog-actions{display:flex;justify-content:flex-end;gap:8px;flex-wrap:wrap}
.mz-preview{max-height:260px;padding-left:20px;overflow:auto;font-size:12.5px;line-height:1.8}
.mz-share-list{display:flex;flex-direction:column;gap:8px;max-height:210px;overflow:auto}
.mz-share-row{display:flex;align-items:center;gap:8px;font-size:12.5px}

@media (max-width:900px){
  body.page-music #main{padding:24px 24px 16px}
  body.page-music .save-bar{margin:8px 24px 16px}
  .mz-table .mz-col-n{width:44px}
  .mz-table .mz-col-time{width:56px}
  .mz-table .mz-col-actions{width:200px}
  .mz-table :is(th,td):first-child{padding-left:16px}
  .mz-table :is(th,td):last-child{padding-right:16px}
}
@media (max-width:760px){.mz-fields{grid-template-columns:1fr}.mz-table .mz-col-time{display:none}}
```

- [ ] **Step 4: `music.js`: header, rows, the save bar**

Every function not named here stays exactly as it is. In particular `mmss`, `tooLongProblem`, `slots`, `slotAt`, `working`, `locked`, `slotTitle`, `slotLine`, `firstFreeSlot`, `pendingWhat`, `chooseFile`, `editDetails`, `openEditor`, `renderEditor`, `runSearch`, `beginRowDrag`, every dialog function and the "Boot" block's contents stay.

**(a) Imports.** At the very top of the file, above `// A soundtrack list with focused editing and one apply action.`, add:

```js
import { mountSaveBar } from "./savebar.js";
```

**(b) `act`.** Replace the whole `async function act(cmd, args) { … }` with:

```js
// One action, then the fresh status. From the save bar ({ fromBar: true }) a failure is thrown back to
// the bar, which says it in its failure line (spec §7); anywhere else it is drawn at the top of the page.
async function act(cmd, args, { fromBar = false } = {}) {
  if (busy) return;
  busy = true;
  problem = null;
  notice = null;
  draw();
  let failure = null;
  try {
    const r = await call(cmd, args);
    if (r && typeof r === "object" && Array.isArray(r.slots)) status = r;
    else await load();
    noteChange(cmd, args);
  } catch (e) {
    if (fromBar) failure = e;
    else problem = reason(e);
  }
  busy = false;
  draw();
  schedulePoll();
  if (failure) throw failure;
}

// The change the bar names last ("Song 3 replaced"), while that song is still pending.
let lastChange = null;
function noteChange(cmd, args) {
  if (cmd === "music_move") lastChange = { slot: args.to, label: `Song ${args.from} moved to ${args.to}` };
  else if (cmd === "music_stage_stock") lastChange = { slot: args.slot, label: `Song ${args.slot} restored` };
  else if (cmd === "music_unstage" || cmd === "music_apply") lastChange = null;
}
const PENDING_WORD = { original: "restored", your_file: "replaced", pack: "replaced", original_moved: "moved" };
// The save bar's list (spec §7): one line per song waiting to be applied, the one changed last at the end.
function pendingChanges() {
  const list = slots().filter((s) => s.pending).map((s) => ({ slot: s.slot, label: `Song ${s.slot} ${PENDING_WORD[s.pending.kind] || "changed"}` }));
  const last = lastChange && list.find((c) => c.slot === lastChange.slot);
  if (!last) return list.map((c) => c.label);
  return [...list.filter((c) => c !== last).map((c) => c.label), lastChange.label];
}
```

**(c) `SOURCE_LABEL`.** Replace the whole `const SOURCE_LABEL = { … };` block with:

```js
// Where a song comes from, for the line under its name (spec §8.4).
const SOURCE_WORD = {
  original: "On your disc",
  original_moved: "On your disc, moved",
  your_file: "Your song",
  soundtrack_mod: "Soundtrack mod",
};

// The line under a song: who sings it and where it comes from; or, while a change waits to be applied,
// what it does, ending in " · Pending" (spec §8.4; a chip and Pending in the actions were rejected).
function sourceLine(song, artist) {
  const p = song.pending;
  if (p) {
    const what = p.kind === "original" ? "Restoring the original" : p.kind === "original_moved" ? "Moved here" : `Replacing ${slotTitle(song)}`;
    return [what, el("span", { class: "pend", text: " · Pending" })];
  }
  const where = song.source === "pack" ? (song.source_name ? `From ${song.source_name}` : "From a pack")
    : song.source === "soundtrack_mod" && song.source_name ? song.source_name
    : SOURCE_WORD[song.source] || "";
  return [[artist, where].filter(Boolean).join(" · ")];
}
```

**(d) `addToQueue`.** In `async function addToQueue(){ … }`, replace

```js
    editor.close();notice=null;problem=null;
```

with

```js
    lastChange={slot:args.slot,label:editing!==null?`Song ${args.slot} details changed`:`Song ${args.slot} replaced`};
    editor.close();notice=null;problem=null;
```

(`editing` must be read before `editor.close()`: closing clears it.)

**(e) `discNow`.** Replace the whole `function discNow(){ … }` with:

```js
// The header row (spec §8.4): the Custom soundtrack switch on the left; Reset, Import pack and Export pack
// (quiet) and Add songs… on the right. One song at a time is added from its row's Replace.
function discNow(){
  const on=!!status?.enabled,off=locked()||!status?.image_ok;
  const toggle=el('button',{type:'button',id:'music-enabled',class:'look-switch',role:'switch','aria-label':'Custom soundtrack','aria-checked':String(on),disabled:off,onclick:()=>act('music_set_enabled',{on:!on})});
  const nothingCustom=!slots().some(s=>s.source!=='original'||s.pending)&&status?.base?.kind!=='package';
  const buttons=el('div',{class:'music-toolbar-actions'},
    el('button',{type:'button',class:'btn quiet',text:'Reset','aria-label':'Reset soundtrack',disabled:off||nothingCustom,onclick:resetDialog}),
    el('button',{type:'button',class:'btn quiet',text:'Import pack',disabled:off,onclick:packDialog}),
    el('button',{type:'button',class:'btn quiet',text:'Export pack',disabled:off,onclick:shareDialog}),
    el('button',{type:'button',class:'btn',text:'Add songs…',disabled:off,onclick:bulkDialog}));
  return el('section',{class:'music-toolbar'},
    el('div',{class:'music-toggle'},toggle,el('label',{for:'music-enabled'},el('span',{class:'t',text:'Custom soundtrack'}),el('span',{class:'d',text:on?'Your selected songs play in-game.':'Your game’s default soundtrack plays. Custom songs are kept.'}))),
    buttons);
}
```

**(f) `statusLine`.** In `function statusLine(){ … }` make two string changes and nothing else:
- `el('div',{class:'music-progress',role:'status'}` becomes `el('div',{class:'glass music-progress',role:'status'}`;
- `el('div',{class:'mz-done'+(status.phase==='problem'?' music-error':'')` becomes `el('div',{class:'glass mz-done'+(status.phase==='problem'?' music-error':'')`.

**(g) `queue`.** Delete the whole `function queue(){ … }`. The save bar replaces it.

**(h) `discList`.** Replace the whole `function discList(){ … }` with:

```js
function discList(){
  const list=slots();
  const sec=el('section',{class:'glass mz-disc',id:'disc'});
  const custom=list.filter(s=>s.source!=='original').length;
  sec.append(el('div',{class:'music-list-heading'},el('h2',{text:'Soundtrack'}),el('span',{text:`${list.length} songs${custom?` · ${custom} custom`:''}`})));
  if(!list.length){sec.append(el('div',{class:'music-empty'},el('h3',{text:status?.image_ok?'No songs found':'Set up your game first'}),el('p',{class:'note',text:status?.image_note||problem||'Your soundtrack will appear here once your game is ready.'}),el('button',{type:'button',text:'Try again',disabled:busy,onclick:refresh})));return sec;}
  const table=el('table',{class:'mz-table','aria-label':'Soundtrack songs'});
  table.append(el('thead',{},el('tr',{},el('th',{scope:'col',class:'mz-col-n',text:'#'}),el('th',{scope:'col',class:'mz-col-mv','aria-label':'Order'}),el('th',{scope:'col',text:'Song'}),el('th',{scope:'col',class:'mz-col-time',text:'Length'}),el('th',{scope:'col',class:'mz-col-actions',text:'Actions'}))));
  const body=el('tbody');
  const last=list.length?list[list.length-1].slot:0;
  for(const song of list){
    const pending=song.pending,original=pending?pending.kind==='original':song.source==='original';
    const o=song.original;
    // An original song the player renamed keeps its audio and shows their
    // name: the backend puts that name in song.title, so it wins over the
    // disc's own text. "Restore original" (a pending reset) shows the disc's.
    const renamed=!pending&&song.source==='original'&&!!o&&(song.title!==o.title||song.artist!==o.artist||song.album!==o.album);
    const title=original?((pending?'':song.title)||o?.title||slotTitle({...song,title:'',original:null,source:'original'})):(pending?.title||song.title||'Untitled');
    const artist=original?((pending?'':song.artist)||o?.artist||''):((pending?.artist??song.artist)||'');
    // Actions (spec §8.4): a pending song offers Undo only; a custom (or renamed) song, Restore
    // original and Replace; the disc's own song, Replace.
    const actions=el('td',{class:'mz-col-actions'});
    if(pending)actions.append(el('button',{type:'button',class:'btn sm quiet',text:'Undo','aria-label':`Undo changes to song ${song.slot}`,disabled:locked(),onclick:()=>act('music_unstage',{slot:song.slot})}));
    else{
      if(song.source!=='original'||renamed)actions.append(el('button',{type:'button',class:'btn sm quiet',text:'Restore original','aria-label':`Restore original song ${song.slot}`,disabled:locked(),onclick:()=>act('music_stage_stock',{slot:song.slot})}));
      actions.append(el('button',{type:'button',class:'btn sm',text:'Replace','aria-label':`Replace song ${song.slot}`,disabled:locked(),onclick:()=>chooseFile(song.slot)}));
    }
    const name=el('td',{class:'mz-col-what'},el('button',{type:'button',class:'music-song-name',text:title,'aria-label':`Edit details for song ${song.slot}: ${title}`,disabled:locked(),onclick:()=>editDetails(song.slot)}),el('span',{class:'mz-src'},...sourceLine(song,artist)));
    // The order is the player's to change: drag a row onto another (the
    // songs between shift by one), or use the arrows, which a pad or the
    // keyboard can reach. Both go through music_move.
    const arrows=el('span',{class:'mz-move'},
      el('button',{type:'button',text:'▲','aria-label':`Move song ${song.slot} up`,disabled:locked()||song.slot<=1,onclick:()=>act('music_move',{from:song.slot,to:song.slot-1})}),
      el('button',{type:'button',text:'▼','aria-label':`Move song ${song.slot} down`,disabled:locked()||song.slot>=last,onclick:()=>act('music_move',{from:song.slot,to:song.slot+1})}));
    const row=el('tr',{class:`mz-track${pending?' staged':''}${!pending&&song.source!=='original'?' custom':''}`,'data-slot':String(song.slot)});
    // Plain pointer events, not HTML5 drag and drop: the launcher's window
    // never delivers dragstart (Tauri owns OS drag and drop on Windows), so
    // the row follows the mouse itself. Press anywhere on the row that is
    // not a control, move, and let go over the place it should take.
    row.addEventListener('pointerdown',ev=>{if(ev.button!==0||locked()||ev.target.closest('button,select,input,a'))return;beginRowDrag(ev,row,song.slot);});
    row.append(el('td',{class:'mz-col-n'},el('span',{class:'mz-n',text:String(song.slot).padStart(2,'0')})),el('td',{class:'mz-col-mv'},arrows),name,el('td',{class:'mz-col-time',text:pending?'':original?(mmss(song.duration_ms??o?.duration_ms)||''):(mmss(song.duration_ms)||'')}),actions);
    body.append(row);
  }
  table.append(body);sec.append(table);return sec;
}
```

**(i) `draw`.** Replace the whole `function draw(){ … }` with:

```js
function draw(){
  const screen=document.getElementById('screen'),parts=[discNow()];
  if(problem)parts.push(el('p',{class:'music-error',role:'alert',text:problem}));
  if(notice)parts.push(el('p',{class:'note',role:'status',text:notice}));
  if(status&&!status.image_ok&&slots().length)parts.push(el('p',{class:'music-error',role:'status',text:status.image_note||'Set up your game before editing its soundtrack.'}));
  const progress=statusLine();if(progress)parts.push(progress);
  parts.push(discList());
  parts.push(el('p',{class:'music-footnote',text:'Custom music applies to offline play. Online uses the shared Reborn soundtrack.'}));
  const focus=document.activeElement?.getAttribute?.('aria-label');
  screen.replaceChildren(...parts);
  if(focus&&!editor?.open)Array.from(screen.querySelectorAll('[aria-label]')).find(n=>n.getAttribute('aria-label')===focus)?.focus({preventScroll:true});
  // The bar lists every pending song. Apply waits while the game image is missing or the core is
  // working (a second Apply would start a second rebuild); Discard waits only while the core works.
  bar.update(pendingChanges());
  bar.lock(locked()||!status?.image_ok,locked());
  return parts;
}
```

**(j) The bar.** Directly **above** the `// Boot` banner comment block (the three lines `// ----…`, `// Boot`, `// ----…`), add:

```js
// The one save bar (spec §7, §8.4): the songs waiting to be applied, then Discard and Apply changes,
// in its own lane under the page. The core owns the pending list; Apply rebuilds the disc once.
const bar = mountSaveBar(document.getElementById("save-lane"), {
  noun: "pending change",
  saveText: "Apply changes",
  onSave: () => act("music_apply", { confirm: true }, { fromBar: true }),
  onDiscard: () => act("music_unstage", { slot: null }, { fromBar: true }),
  returnFocus: () => document.querySelector("#disc .mz-track button:not(:disabled)"),
});
```

- [ ] **Step 5: Run the checks**

- `node Tools/check-theme.mjs` and `node Tools/check-theme-tokens.mjs`: PASS (4 page stylesheets).
- `node Tools/check-music-page.mjs`: PASS (`PASS: song list, add/replace/edit/reset/undo, …`).
- `node Tools/check-music-length-message.mjs`: PASS (the disc-full wording is untouched).
- `node Tools/check-page-call-args.mjs`: PASS. `music_apply`, `music_unstage`, `music_set_enabled` are called with literals.
- `node Tools/check-shell-chrome.mjs`: PASS, or the same pre-existing failure recorded in Step 1.
- `node Tools/check-save-lane.mjs`: PASS.
- The standing checks: PASS.

Open `target/review/music-dialog-1280.png` and `music-dialog-800.png`: the song dialog is a glass dialog, fields full width, no sideways scroll.

- [ ] **Step 6: Commit**

```bash
git add crates/reborn-ui/ui/music.html crates/reborn-ui/ui/music.js crates/reborn-ui/ui/pages/music.css Tools/check-music-page.mjs Tools/check-shell-chrome.mjs Tools/check-theme.mjs
git commit -m "music: theme-only page; header row, source lines with Pending, Undo only, Apply and Discard in the save bar

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Controllers on the design system

**Files:**
- Modify: `crates/reborn-ui/ui/controls.html`
- Modify: `crates/reborn-ui/ui/controls.js`
- Create: `crates/reborn-ui/ui/pages/controls.css`
- Modify: `Tools/review-players.mjs`, `Tools/check-theme.mjs`

**Interfaces:**
- Consumes: `icon(paths)`, `get`, `set` from `shell.js`; the workspace from `setup-controller.js` (unchanged: `.setup-controller.player-workspace` with `.controller-stage`, `.controller-controls`, `.mapping-status`, `.mapping-group.left-stick|direction-pad|face-buttons|right-stick|shoulders`, `.mapping-key`, `.binding-editor`, `.mapping-buttons`, `.test-toggle`, `.live-device`, `.controller-needs-setup`); `#menu-pad-setup` (inserted before `#screen` by `menu-pad-setup.js`, unchanged); the controller art rules and `--surface-solid` (Task 1).
- Produces (Task 5 uses these):
  - `.player-tabs` of four `button.player-card[role=tab][aria-selected][.is-empty]`, each with `span.player-icon > svg` (the Controllers nav icon) and `span.player-text` ("Player N" + device line);
  - the note `section.glass.ctl-note[role=note][aria-label="B and X mapping"]` with an amber icon and **Dismiss** (`.btn.sm.quiet`); dismissal is `set('bx-note-dismissed','1')` (the window's local storage for this install);
  - `#menu-pad-setup` top right of the header, level with the title block.

- [ ] **Step 1: Update the checks (they fail now)**

`Tools/check-theme.mjs`: replace the `THEMED` line with

```js
const THEMED = { index: "pages/play.css", settings: "pages/settings.css", mods: "pages/mods.css", music: "pages/music.css", controls: "pages/controls.css" };
```

`Tools/review-players.mjs`:

**(a)** Replace

```js
 assert.equal(await page.getByRole('tab').count(),4);
 assert.equal(await page.locator('.mapping-group').count(),5);
```

with

```js
 assert.equal(await page.getByRole('tab').count(),4);
 assert.equal(await page.locator('.mapping-group').count(),5);
 // The glass page (spec §8.5): four player cards with the launcher's own controller icon, and the B/X note.
 assert.equal(await page.locator('.player-card .player-icon svg').count(),4);
 const bxNote=page.getByRole('note',{name:'B and X mapping'});
 assert.match(await bxNote.innerText(),/B maps to B\. X maps to X\./);
 assert.equal(await bxNote.getByRole('button',{name:'Dismiss',exact:true}).count(),1);
 assert.equal(await page.locator('.ctl-note svg').evaluate(n=>getComputedStyle(n).color),'rgb(255, 196, 107)','the note icon is amber');
```

**(b)** Replace

```js
 await page.getByRole('button',{name:'Assign A',exact:true}).click();
 assert.equal(await page.getByRole('tab').first().isDisabled(),true);
```

with

```js
 await page.getByRole('button',{name:'Assign A',exact:true}).click();
 assert.equal(await page.getByRole('tab').first().isDisabled(),true);
 // The binding popover floats over the bindings: it stays opaque.
 const editorBg=await page.locator('.binding-editor').evaluate(n=>getComputedStyle(n).backgroundColor);
 assert.match(editorBg,/^rgb\(/,'the binding popover is opaque: '+editorBg);
```

**(c)** Replace

```js
 await page.getByRole('combobox',{name:'Button textures',exact:true}).selectOption('playstation');
 await page.getByText('Button textures saved. They apply to all players.',{exact:true}).waitFor();
```

with

```js
 await page.getByRole('combobox',{name:'Button textures',exact:true}).selectOption('playstation');
 await page.getByText('Button textures saved. They apply to all players.',{exact:true}).waitFor();
 // Dismiss hides the B/X note, and it stays dismissed for this install.
 await page.getByRole('note',{name:'B and X mapping'}).getByRole('button',{name:'Dismiss',exact:true}).click();
 assert.equal(await page.getByRole('note',{name:'B and X mapping'}).count(),0);
 await page.reload();await page.getByRole('tab').first().waitFor();
 assert.equal(await page.getByRole('note',{name:'B and X mapping'}).count(),0,'the dismissal is remembered');
```

Run (Playwright linked):
- `node Tools/check-theme.mjs`. Expected: FAIL (`controls: must load exactly theme.css + pages/controls.css`).
- `node Tools/review-players.mjs`. Expected: FAIL at `.player-card .player-icon svg` (count 0).

- [ ] **Step 2: `controls.html` becomes theme-only**

Replace the whole file with the following. Keep the `<nav …>…</nav>` block (current lines 15-21) and the `<details class="core">…</details>` line **byte-identical**, replacing the two comments. `players.css` is no longer loaded.

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Controllers | NFL Street Reborn</title>
  <script src="look-boot.js"></script>
  <link rel="icon" href="art/cover.png">
  <link rel="stylesheet" href="theme.css"><link rel="stylesheet" href="pages/controls.css">
</head>
<body class="shell themed page-controls">
  <a class="skip-link" href="#main">Skip to content</a>
  <header class="app-header"><span id="bridge" class="pill">Connecting</span></header>
  <!-- the existing <nav class="shell-nav" aria-label="Main navigation"> … </nav> block, unchanged -->
  <main id="main" tabindex="-1">
    <div class="page-heading"><h1>Controllers</h1><p class="purpose">Set up your controllers for up to four players.</p></div>
    <section id="screen"><p class="loading">Loading…</p></section>
    <!-- the existing <details class="core"> … </details> line, unchanged -->
  </main>
  <footer class="input-footer"><span id="input-hints"></span></footer>
  <script type="module" src="app-shell.js"></script><script type="module" src="controls.js"></script>
  <script src="controller-preview.js"></script><script src="setup-controller.js"></script>
</body>
</html>
```

- [ ] **Step 3: `pages/controls.css`**

Create `crates/reborn-ui/ui/pages/controls.css`:

```css
/* Controllers (spec §8.5; mockup "CONTROLLERS"). Layout only: colours, radii and durations come from theme.css.
   Header (title + "Set up launcher controls"), the B/X note, four player cards, the input device row, then the
   mapping panel in three columns (sticks and D-pad | controller art + Test controller | buttons and right stick).
   The workspace markup comes from setup-controller.js, unchanged; this file replaces players.css on this page. */

/* header: #menu-pad-setup (menu-pad-setup.js puts it before #screen) sits top right, level with the title.
   The right padding keeps the content column at 942px: min(width - 72, 942). */
body.page-controls #main{display:grid;grid-template-columns:minmax(0,1fr) auto;align-content:start;column-gap:16px;padding:36px max(36px,calc(100% - 978px)) 32px 36px}
.page-controls .page-heading{grid-column:1;grid-row:1;min-width:0}
.page-controls #menu-pad-setup{grid-column:2;grid-row:1;align-self:end}
.page-controls #screen,.page-controls details.core{grid-column:1/-1;min-width:0}
.page-controls #screen{display:flex;flex-direction:column}
.page-controls .loading{margin-top:24px}

/* the B/X note and any one-time notices from the core */
.page-controls .notices{display:flex;flex-direction:column;gap:8px;margin-top:24px}
.page-controls .notices:empty{display:none}
.ctl-note{display:flex;align-items:center;gap:16px;padding:12px 16px;font-size:12.5px;color:var(--fg-2)}
.ctl-note>svg{width:16px;height:16px;flex:none;color:var(--warn)}
.ctl-note p{flex:1;min-width:0}
.ctl-note strong{font-weight:600;color:var(--fg)}
.page-controls .notice.migration{display:grid;gap:8px;padding:12px 16px;border-radius:var(--r-lg);border:1px solid var(--stroke);background:var(--glass-1);box-shadow:var(--sheen)}
.page-controls .notice.migration strong{font-size:14px;font-weight:600}
.page-controls .notice.migration p{font-size:12.5px;color:var(--fg-2)}
.page-controls .notice-dismiss{justify-self:start;height:32px;padding:0 12px;border-radius:var(--r-md);border:1px solid var(--stroke);background:transparent;color:var(--fg-2);font:600 12.5px Inter,system-ui,sans-serif;cursor:pointer}
.page-controls .notice-dismiss:hover{color:var(--fg);background:var(--glass-1)}

/* player cards */
.player-tabs{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:16px;margin-top:24px}
.player-card{display:flex;align-items:center;gap:12px;min-width:0;padding:14px 16px;border-radius:var(--r-lg);border:1px solid var(--stroke);background:var(--glass-1);box-shadow:var(--sheen);color:var(--fg);font:inherit;text-align:left;cursor:pointer;backdrop-filter:blur(24px) saturate(1.3);-webkit-backdrop-filter:blur(24px) saturate(1.3)}
.player-card:hover:not(:disabled){background:var(--glass-2)}
.player-card:disabled{cursor:default}
.player-card[aria-selected="true"]{border-color:var(--red);background:color-mix(in srgb,var(--red) 8%,transparent)}
.player-icon{display:grid;place-items:center;flex:none;width:34px;height:34px;border-radius:var(--r-sm);border:1px solid var(--stroke);background:var(--glass-2)}
.player-card[aria-selected="true"] .player-icon{border-color:color-mix(in srgb,var(--red) 45%,transparent);background:color-mix(in srgb,var(--red) 18%,transparent)}
.player-icon svg{width:18px;height:18px}
.player-card.is-empty .player-icon{opacity:.45}
.player-text{display:flex;flex-direction:column;min-width:0}
.player-text .t{font-size:13.5px;font-weight:600}
.player-text small{max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:12px;color:var(--fg-3)}
.player-text small.disconnected{color:var(--warn)}

/* input device row */
.player-panel{min-width:0}
.player-device{display:flex;align-items:center;gap:16px;margin:16px 0}
.player-device label{font-size:13px;color:var(--fg-2)}
.player-device select{flex:0 1 380px;min-width:0}

/* the mapping panel (setup-controller.js, layout "players") */
.page-controls .player-workspace{position:relative;display:grid;grid-template-columns:minmax(0,1fr) minmax(0,300px) minmax(0,1fr);gap:16px 24px;min-width:0;padding:16px 20px;border-radius:var(--r-lg);border:1px solid var(--stroke);background:var(--glass-1);box-shadow:var(--sheen),var(--shadow);backdrop-filter:blur(24px) saturate(1.3);-webkit-backdrop-filter:blur(24px) saturate(1.3)}
.player-workspace .controller-controls,.player-workspace .compact-mappings{display:contents}
.player-workspace .controller-controls>h2,.player-workspace .mapping-tabs{display:none}
.player-workspace .mapping-status{grid-column:1/-1;grid-row:1;min-height:20px;font-size:12.5px;color:var(--fg-2)}
.player-workspace .left-stick{grid-column:1;grid-row:2}
.player-workspace .direction-pad{grid-column:1;grid-row:3}
.player-workspace .face-buttons{grid-column:3;grid-row:2}
.player-workspace .right-stick{grid-column:3;grid-row:3}
.player-workspace .shoulders{grid-column:2;grid-row:4}
.player-workspace .controller-stage{grid-column:2;grid-row:2/span 2;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:8px;min-width:0;text-align:center}
.player-workspace .controller-art{display:flex;align-items:center;justify-content:center;width:100%;max-width:230px}
.player-workspace .controller-preview{width:100%}
.player-workspace .controller-preview svg{width:100%;max-height:210px}
.player-workspace .controller-live{font-size:12px;color:var(--fg-3)}
.player-workspace .controller-stage .note{max-width:240px;font-size:12px;color:var(--fg-3)}
.player-workspace .live-device{width:100%}
.player-workspace .mapping-group h3{margin:4px 0 8px;font-size:12px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--fg-2)}
.player-workspace .group-bindings{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px}
.player-workspace .mapping-key{display:flex;flex-direction:column;align-items:flex-start;gap:2px;min-width:0;padding:8px 12px;border-radius:var(--r-md);border:1px solid var(--stroke);background:var(--glass-1);color:var(--fg);font:600 13px Inter,system-ui,sans-serif;text-align:left;cursor:pointer}
.player-workspace .mapping-key:hover:not(:disabled){border-color:var(--stroke-hi);background:var(--glass-2)}
.player-workspace .mapping-key:disabled{opacity:.55;cursor:default}
.player-workspace .mapping-key small{max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:11.5px;font-weight:500;color:var(--fg-3)}
.player-workspace .mapping-key[data-dead="true"] small{color:var(--red-ink)}
.player-workspace .mapping-buttons{grid-column:1/-1;display:flex;flex-wrap:wrap;align-items:center;gap:8px}
.player-workspace :is(.test-toggle,.mapping-reset,.mapping-swap){display:inline-flex;align-items:center;justify-content:center;height:38px;padding:0 16px;border-radius:var(--r-md);border:1px solid var(--stroke);background:var(--glass-2);box-shadow:var(--sheen);color:var(--fg);font:600 13px Inter,system-ui,sans-serif;cursor:pointer}
.player-workspace :is(.mapping-reset,.mapping-swap){height:32px;padding:0 12px;font-size:12.5px}
.player-workspace :is(.test-toggle,.mapping-reset,.mapping-swap):hover:not(:disabled){border-color:var(--stroke-hi);background:var(--glass-3)}
.player-workspace :is(.test-toggle,.mapping-reset,.mapping-swap):disabled{opacity:.55;cursor:default}
.player-workspace[data-testing="true"] .test-toggle{border-color:var(--ok);background:color-mix(in srgb,var(--ok) 12%,transparent)}
.page-controls .controller-needs-setup{padding:12px 16px;border-radius:var(--r-md);border:1px solid color-mix(in srgb,var(--red) 35%,transparent);background:color-mix(in srgb,var(--red) 8%,transparent)}
.page-controls .controller-needs-setup strong{display:block;font-size:14px;font-weight:600}
.page-controls .controller-needs-setup p{margin:8px 0 16px;font-size:12.5px;color:var(--fg-2)}
/* the binding popover floats over the bindings: always OPAQUE */
.player-workspace .binding-editor{position:absolute;z-index:5;left:50%;top:50%;transform:translate(-50%,-50%);display:flex;flex-wrap:wrap;gap:16px;width:min(420px,85%);padding:24px;border-radius:var(--r-lg);border:1px solid var(--stroke-hi);background:var(--surface-solid);box-shadow:var(--shadow)}
.player-workspace .binding-editor .note{width:100%;font-size:13px;color:var(--fg)}
.player-workspace .binding-editor label{display:grid;gap:8px;width:100%;font-size:13px}
.player-workspace .binding-editor input{width:100%}
.player-workspace .typed-binding{width:100%}
.player-workspace .typed-binding>summary{font-size:12px;color:var(--fg-2);cursor:pointer}
.player-workspace .typed-binding[open]>summary{margin-bottom:8px}

/* no controller for this player */
.page-controls .player-empty{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:8px;min-height:240px;padding:40px 24px;border-radius:var(--r-lg);border:1px solid var(--stroke);background:var(--glass-1);text-align:center}
.page-controls .player-empty>*{text-align:center}
.page-controls .player-empty strong{font-size:18px;font-weight:600;letter-spacing:-.02em}
.page-controls .player-empty p{max-width:460px;font-size:12.5px;color:var(--fg-2)}

/* button textures, the save line, and the controller report */
.player-textures{display:flex;flex-wrap:wrap;align-items:center;gap:16px;margin-top:24px;padding-top:16px;border-top:1px solid var(--stroke);font-size:13px}
.player-textures label{color:var(--fg-2)}
.player-textures select{min-width:160px}
.player-textures span{font-size:12px;color:var(--fg-3)}
.player-feedback{margin-top:8px;font-size:12px;color:var(--fg-3)}
.controller-report-entry{display:grid;justify-items:start;gap:8px;margin-top:24px;padding-top:16px;border-top:1px solid var(--stroke)}
.controller-report-entry h2{font-size:15px;font-weight:600}
.controller-report-entry p{font-size:12.5px;color:var(--fg-2)}
.controller-report-dialog [hidden]{display:none!important}
.controller-report-dialog h3{margin:16px 0 8px;font-size:15px;font-weight:600}
.controller-report-dialog>p{font-size:13px;color:var(--fg-2)}
.controller-report-fields{display:grid;grid-template-columns:1fr 1fr;gap:16px;margin:16px 0}
.controller-report-fields label{display:grid;gap:8px;min-width:0;font-size:13px}
.controller-report-fields label:last-child{grid-column:1/-1}
.controller-report-fields :is(select,textarea){width:100%;min-width:0}
.controller-report-note{font-size:12px;color:var(--fg-2)}
.menu-pad-setup-dialog select{width:100%;margin:16px 0}

@media (max-width:900px){
  body.page-controls #main{padding:24px max(24px,calc(100% - 966px)) 16px 24px}
  .player-tabs{gap:8px}
  .player-card{padding:12px}
  .page-controls .player-workspace{grid-template-columns:minmax(0,1fr) minmax(0,200px) minmax(0,1fr);gap:16px;padding:16px}
  .player-workspace .controller-art{max-width:175px}
  .player-workspace .controller-preview svg{max-height:160px}
  .player-workspace .mapping-key{padding:8px}
}
```

- [ ] **Step 4: `controls.js`: player cards and the note**

**(a)** Replace the first line

```js
import {bridge,call,el,get,reason,markCurrent,wireCoreFold} from './shell.js';
```

with

```js
import {bridge,call,el,get,icon,reason,markCurrent,set,wireCoreFold} from './shell.js';
```

**(b)** Directly after the line `const connected=device=>state.pads.some(p=>p.device_string===device);` add:

```js
// The launcher's own controller icon (the Controllers nav item's two paths), on each player card (spec §8.5).
const PAD_ICON=['M7 7h10c3 0 5 11 2 11-2 0-3-3-4-3H9c-1 0-2 3-4 3C2 18 4 7 7 7Z','M8 9v5m-2-2h4m5-1h.01m3 2h.01'];
const WARN_ICON=['M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z','M12 8v5','M12 16h.01'];
const padIcon=()=>{const svg=icon(PAD_ICON);svg?.setAttribute('stroke-width','1.6');return svg;};
// "B maps to B" (spec §8.5): a small glass note with Dismiss, no longer a permanent banner. Dismissal is
// kept in this install's window storage, so it does not come back on the next visit.
function mappingNote(){
 if(get('bx-note-dismissed'))return null;
 const note=el('section',{class:'glass ctl-note',role:'note','aria-label':'B and X mapping'},icon(WARN_ICON),
  el('p',{},el('strong',{text:'B maps to B. X maps to X.'}),' Older installs may have these reversed. Check the bindings below.'),
  el('button',{type:'button',class:'btn sm quiet',text:'Dismiss',onclick:()=>{set('bx-note-dismissed','1');note.remove();document.getElementById(`player-tab-${active}`)?.focus({preventScroll:true});}}));
 return note;
}
```

**(c)** In `draw()`, replace the tab line

```js
  const device=state.slots[i];const tab=el('button',{type:'button',role:'tab',id:`player-tab-${i}`,'aria-selected':String(i===active),'aria-controls':'player-panel',tabindex:'0',onclick:()=>{if(busy)return;active=i;draw();document.getElementById(`player-tab-${i}`).focus({preventScroll:true});}},el('span',{text:`Player ${i+1}`}),el('small',{class:device?(connected(device)?'connected':'disconnected'):'',text:device?(connected(device)?friendly(device):'Disconnected'):'No controller'}));
```

with

```js
  const device=state.slots[i];const tab=el('button',{type:'button',role:'tab',class:'player-card'+(device?'':' is-empty'),id:`player-tab-${i}`,'aria-selected':String(i===active),'aria-controls':'player-panel',tabindex:'0',onclick:()=>{if(busy)return;active=i;draw();document.getElementById(`player-tab-${i}`).focus({preventScroll:true});}},
   el('span',{class:'player-icon'},padIcon()),
   el('span',{class:'player-text'},el('span',{class:'t',text:`Player ${i+1}`}),el('small',{class:device?(connected(device)?'connected':'disconnected'):'',text:device?(connected(device)?friendly(device):'Disconnected'):'No controller'})));
```

**(d)** In `draw()`, replace

```js
 const noticeHost=el('div',{class:'notices'},el('section',{class:'controller-mapping-note'},el('strong',{text:'B maps to B. X maps to X.'}),el('p',{text:'Older installs may have these reversed. Check the bindings below: B must use Button B, and X must use Button X. Only use the correction if they are reversed.'})));
```

with

```js
 const noticeHost=el('div',{class:'notices'},mappingNote());
```

Nothing else in `controls.js` changes: the device select, Scan again, the workspace, button textures, the feedback line and the report entry stay as they are.

- [ ] **Step 5: Update the theme comment that named `players.css`**

In `theme.css`, replace

```css
   The controls page keeps players.css's own opaque .binding-editor popover. */
```

with

```css
   The controls page draws its own opaque .binding-editor popover (pages/controls.css, --surface-solid). */
```

- [ ] **Step 6: Run the checks**

- `node Tools/check-theme.mjs` and `node Tools/check-theme-tokens.mjs`: PASS (5 page stylesheets).
- `node Tools/review-players.mjs`: PASS (four players, … responsive layout). It saves `target/browser-review/players-1280.png`, `players-800.png`, `players-empty-centered.png`.
- `node Tools/check-controller-navigation.mjs`: PASS (`#menu-pad-setup` still opens the launcher-controls dialog on Controllers).
- `node Tools/check-controller-report.mjs`: PASS (the report dialog, its fields and the no-sideways-scroll checks at 1280 and 800).
- `node Tools/check-controller-hints.mjs`, `node Tools/check-controller-press-to-map.mjs`, `node Tools/check-update-notice.mjs`: PASS.
- `node Tools/check-page-call-args.mjs`: PASS.
- The standing checks: PASS. The `run-browser-tests` main body still finds `getByRole('tab')` and **Test controller** on Controllers.

Open `target/browser-review/players-1280.png` beside the mockup's CONTROLLERS section. Check: the title with **Set up launcher controls** top right as a normal button; the amber-icon note with Dismiss; four glass player cards with the controller icon, the selected one red-bordered with a faint red fill; the input device row; the three-column mapping panel with binding tiles; Button textures and the report entry below. Open `players-800.png`: nothing overflows sideways.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/controls.html crates/reborn-ui/ui/controls.js crates/reborn-ui/ui/pages/controls.css crates/reborn-ui/ui/theme.css Tools/review-players.mjs Tools/check-theme.mjs
git commit -m "controllers: theme-only page; header action, dismissible B/X note, glass player cards, three-column mapping panel

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Spec §13 for Mods, Music and Controllers: screens, alignment, motion, controller mode

**Files:**
- Create: `Tools/check-glass-mods-music-controls.mjs`
- Modify: `Tools/run-browser-tests.mjs` (one import at the end)

**Interfaces:**
- Consumes everything above. Uses only commands the three pages already call (stubbed).
- Produces screenshots in `target/glass-review/` (or `argv[2]`): `mods-<w>-<glass|solid>-<slow|quick>.png`, `music-…` (bar hidden), `music-bar-…` (bar shown), `controls-…`, `controls-pad-…`.

- [ ] **Step 1: Write the check**

Create `Tools/check-glass-mods-music-controls.mjs`:

```js
// Spec §13 for Mods, Music and Controllers: screens at 1280×800 and 800×560 in each look (glass on and off,
// slow and quick motion), Music's save bar shown and hidden, Controllers in controller mode, and the
// alignment and motion rules those screens are judged by. Also Review Focus 2 and 5 (plan 3).
//     node Tools/check-glass-mods-music-controls.mjs [outdir]      (default target/glass-review)
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';

const root = path.resolve(import.meta.dirname, '..'), ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.resolve(process.argv[2] || path.join(root, 'target/glass-review')); mkdirSync(out, { recursive: true });

const LONG = 'Classic brown football with a very long package name that has to wrap';
const slots = Array.from({ length: 21 }, (_, i) => ({ slot: i + 1, title: '', artist: '', album: '', duration_ms: 180000 + i * 1000, source: 'original', source_name: null, pending: null,
  original: { title: `Song ${i + 1}`, artist: `Artist ${i + 1}`, album: '', duration_ms: 180000 + i * 1000 } }));
slots[2].pending = { kind: 'your_file', title: 'Blinding Lights', artist: 'The Weeknd', album: '' };
slots[4] = { ...slots[4], title: 'HUMBLE. (A much longer title than any real song, to test the narrow window)', artist: 'Kendrick Lamar', source: 'your_file', source_name: 'humble.mp3' };
const ROWS = [...['A', 'B', 'X', 'Y', 'Z', 'Start'].map((x) => ({ key: 'Buttons/' + x, label: x, binding: '`Button ' + x + '`' })),
  ...['Up', 'Down', 'Left', 'Right'].flatMap((x) => ['Main Stick', 'C-Stick', 'D-Pad'].map((g) => ({ key: g + '/' + x, label: g + ' ' + x, binding: '`' + x + '`' }))),
  ...['L', 'R'].map((x) => ({ key: 'Triggers/' + x, label: x, binding: '`Trigger ' + x + '`' }))];
const DATA = {
  session_active: false, ui_notices: [], notices_pending: [],
  settings_paths: { runtime: null, game: null, userDir: null, legacyRoot: null },
  update_policy: { automatic: false }, update_check: { state: 'current' }, update_activity: { downloading: false, ready: null },
  content_status: { phase: 'up_to_date', components: [] },
  mod_packages: { packages_root: 'C:/Reborn/mods', packages: [
    { id: 'football-red', version: '1.0', name: 'Red football', active: true, files: 1, bytes: 1048576 },
    { id: 'football-black', version: '1.0', name: 'Black football', active: false, files: 1, bytes: 1048576 },
    { id: 'football-brown', version: '1.2', name: LONG, active: false, files: 1, bytes: 1048576 }],
    activation: [{ id: 'football-red', version: '1.0' }], unreadable: [],
    exclusive: [{ members: ['football-red', 'football-black', 'football-brown'], computed: true, example_path: 'football.png' }], managed_by_controls: [] },
  music_status: { phase: 'idle', progress: null, enabled: true, base: { kind: 'original', name: null }, slots, pending_count: 1, note: '', image_ok: true, image_note: '', limit_seconds: 380, formats: ['mp3'] },
  launch_check: { ready: true, runtime: 'C:\\rt\\Dolphin.exe' },
  devices: { complete: true, pads: [0, 1, 2].map((i) => ({ device_string: `XInput/${i}/Gamepad` })) },
  controller_slots: { slots: ['XInput/0/Gamepad', 'XInput/1/Gamepad', 'XInput/2/Gamepad', null] },
  controls_current: { profile: 'xbox' }, controls_options: [{ slug: 'xbox', label: 'Xbox' }, { slug: 'gamecube', label: 'GameCube' }],
  setup_controller_mapping: { custom: false, rows: ROWS },
};
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.svg': 'image/svg+xml', '.woff2': 'font/woff2' };
const server = createServer((req, res) => {
  try {
    const p = path.resolve(ui, '.' + decodeURIComponent(new URL(req.url, 'http://x').pathname));
    if (!p.startsWith(ui + path.sep)) throw Error('outside');
    res.writeHead(200, { 'Content-Type': types[path.extname(p)] || 'application/octet-stream' }); res.end(readFileSync(p));
  } catch { res.writeHead(404); res.end(); }
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const errors = [];
async function open(file, { glass = true, slow = true, viewport = { width: 1280, height: 800 } } = {}) {
  const ctx = await browser.newContext({ viewport });
  await ctx.addInitScript(({ data, glass, slow }) => {
    window.pads = [{ id: 'Xbox Wireless Controller', index: 0, connected: true, mapping: 'standard', axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })) }];
    Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => window.pads });
    document.hasFocus = () => true;
    const look = { windowGlass: glass, slowAnimations: slow, glassSupported: true, glassApplied: glass, glassNote: null };
    window.__TAURI__ = { event: { listen: async () => () => {} }, core: { invoke: async (cmd, args = {}) => {
      if (cmd === 'launcher_look') return look;
      if (cmd === 'window_glass_apply') return { ...look, glassApplied: args.enabled };
      if (cmd === 'music_unstage') { data.music_status.slots.forEach((s) => { if (args.slot === null || s.slot === args.slot) s.pending = null; }); return data.music_status; }
      if (cmd === 'remap_detect') return new Promise(() => {});
      if (cmd in data) return data[cmd];
      return null;
    } } };
  }, { data: JSON.parse(JSON.stringify(DATA)), glass, slow });
  const page = await ctx.newPage(); page.on('pageerror', (e) => errors.push(`${file}: ${e.message}`));
  await page.goto(base + '/' + file);
  await page.waitForFunction((g) => document.documentElement.dataset.glass === (g ? 'on' : 'off'), glass);
  await page.evaluate(() => document.fonts.ready);
  return page;
}
const press = async (page, b) => { await page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; }, b); await page.waitForTimeout(90); await page.evaluate((b) => { pads[0].buttons[b] = { pressed: false, value: 0 }; }, b); await page.waitForTimeout(90); };
const style = (page, sel, prop, pseudo = null) => page.locator(sel).first().evaluate((n, [p, ps]) => getComputedStyle(n, ps)[p], [prop, pseudo]);
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height, r: r.right, b: r.bottom }; });
const rects = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => { const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; }));
const noSideways = async (page, what) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${what}: sideways scroll`);
const starts = (s) => new RegExp('^' + s.replace('.', '\\.'));
async function material(page, glass) {
  assert.equal(await page.evaluate(() => getComputedStyle(document.body).backgroundColor), 'rgba(0, 0, 0, 0)', 'body stays transparent over the material');
  const bg = await page.evaluate(() => getComputedStyle(document.documentElement).backgroundImage + '|' + getComputedStyle(document.documentElement).backgroundColor);
  if (glass) assert.match(bg, /rgba\(18, 19, 23, 0\.66\)/, 'glass: the tint over the OS material'); else assert.match(bg, /radial-gradient/, 'solid: the fallback');
}
try {
  for (const viewport of [{ width: 1280, height: 800 }, { width: 800, height: 560 }]) for (const glass of [true, false]) for (const slow of [true, false]) {
    const tag = `${viewport.width}-${glass ? 'glass' : 'solid'}-${slow ? 'slow' : 'quick'}`;
    const t = slow ? { d2: '0.72s', d3: '0.96s' } : { d2: '0.18s', d3: '0.24s' };
    { // Mods (spec §8.3)
      const page = await open('mods.html', { glass, slow, viewport });
      await page.locator('.store-card').first().waitFor();
      await noSideways(page, `mods ${tag}`);
      await material(page, glass);
      const cards = await rects(page, '.store-card');
      assert.equal(cards.length, 3);
      assert.ok(cards.every((c) => Math.abs(c.y - cards[0].y) < 1 && Math.abs(c.w - cards[0].w) < 1), 'three equal cards in one row: ' + JSON.stringify(cards));
      const lefts = await page.evaluate(() => ['.mods-toolbar', '.page-mods .tabs', '.store-grid', '.mods-count'].map((s) => Math.round(document.querySelector(s).getBoundingClientRect().x)));
      assert.ok(lefts.every((v) => v === lefts[0]), 'toolbar, tabs, count and cards share one left edge: ' + lefts);
      assert.deepEqual(await page.locator('.store-card').evaluateAll((ns) => ns.map((n) => n.querySelectorAll('[role=switch]').length)), [1, 1, 1], 'one switch per card');
      assert.equal(await page.locator('.store-card.active').count(), 1);
      assert.equal(await page.locator('.store-card.active .store-check').count(), 1, 'a red check on the enabled card');
      assert.equal(await page.locator('.store-card:not(.active) .store-check').count(), 0);
      assert.equal(await page.locator('.store-art img, .store-art > svg').count(), 0, 'no drawn art in the preview');
      assert.equal(await style(page, '.store-check', 'backgroundColor'), 'rgb(224, 39, 46)', 'the check is red');
      assert.equal(await page.locator('.mods-count').innerText(), '3 footballs · 1 enabled · only one football can be on at a time');
      await page.waitForTimeout(slow ? 1000 : 350);
      assert.match(await style(page, '.tab-ind', 'transitionDuration'), starts(t.d2), 'the category highlight slides at --d2');
      assert.match(await style(page, '.store-card [role=switch]', 'transitionDuration', '::after'), starts(t.d2), 'the switch thumb slides at --d2');
      await page.screenshot({ path: path.join(out, `mods-${tag}.png`) });
      await page.context().close();
    }
    { // Music (spec §8.4, §7)
      const page = await open('music.html', { glass, slow, viewport });
      await page.locator('.mz-track').first().waitFor();
      await page.locator('.save-lane.open').waitFor();
      await material(page, glass);
      assert.equal(await style(page, '.save-lane', 'transitionDuration'), t.d3, 'the save lane opens at --d3');
      await page.waitForTimeout(slow ? 1100 : 400);
      const edge = await page.evaluate(() => ({ main: document.getElementById('main').getBoundingClientRect().bottom, lane: document.getElementById('save-lane').getBoundingClientRect().top,
        bar: Math.round(document.querySelector('.save-bar').getBoundingClientRect().x), table: Math.round(document.getElementById('disc').getBoundingClientRect().x) }));
      assert.ok(Math.abs(edge.main - edge.lane) < 1, 'the bar never covers the list');
      assert.equal(edge.bar, edge.table, 'the bar lines up with the soundtrack table');
      assert.match(await page.locator('.save-bar').innerText(), /1 pending change[\s\S]*Song 3 replaced[\s\S]*Discard[\s\S]*Apply changes/);
      assert.equal(await page.locator('[data-slot="3"] .mz-src').innerText(), 'Replacing Song 3 · Pending');
      assert.equal(await style(page, '[data-slot="3"] .mz-src .pend', 'color'), 'rgb(255, 122, 127)', '· Pending is --red-ink');
      assert.deepEqual(await page.locator('[data-slot="3"] .mz-col-actions button').allInnerTexts(), ['Undo']);
      assert.deepEqual(await page.locator('[data-slot="5"] .mz-col-actions button').allInnerTexts(), ['Restore original', 'Replace']);
      assert.deepEqual(await page.locator('[data-slot="1"] .mz-col-actions button').allInnerTexts(), ['Replace']);
      assert.equal(await page.getByRole('button', { name: 'Add song', exact: true }).count(), 0, 'no duplicate Add song');
      assert.equal(await page.getByRole('switch', { name: 'Custom soundtrack' }).getAttribute('aria-checked'), 'true');
      if (viewport.width === 1280) {
        const add = await rect(page, 'button:text-is("Add songs…")'), disc = await rect(page, '#disc');
        assert.ok(Math.abs(add.r - disc.r) < 1.5, 'Add songs… ends on the table\'s right edge');
      }
      await noSideways(page, `music bar ${tag}`);
      await page.screenshot({ path: path.join(out, `music-bar-${tag}.png`) });
      if (viewport.width === 1280 && glass && slow) { // Review Focus 2: the pad reaches the bar, and Back never leaves the page
        await page.locator('[aria-label="Replace song 21"]').scrollIntoViewIfNeeded();
        await page.locator('[aria-label="Replace song 21"]').focus();
        await press(page, 13);
        assert.equal(await page.evaluate(() => document.activeElement.textContent), 'Apply changes', 'D-pad down from the last song reaches Apply changes');
        await press(page, 1);
        assert.ok(page.url().endsWith('/music.html'), 'Back from the bar stays on Music');
        assert.equal(await page.evaluate(() => !!document.activeElement.closest('.shell-nav')), true, 'Back from the bar goes to the sidebar');
        await page.mouse.move(300, 300); await page.mouse.move(360, 340);
      }
      await page.getByRole('button', { name: 'Discard', exact: true }).click();
      await page.waitForFunction(() => !document.querySelector('.save-lane.open'));
      await page.waitForTimeout(slow ? 1100 : 400);
      assert.equal(await page.locator('.mz-track.staged').count(), 0);
      await noSideways(page, `music ${tag}`);
      await page.screenshot({ path: path.join(out, `music-${tag}.png`) });
      await page.context().close();
    }
    { // Controllers (spec §8.5, §6.3)
      const page = await open('controls.html', { glass, slow, viewport });
      await page.getByRole('button', { name: 'Assign A', exact: true }).waitFor();
      await noSideways(page, `controls ${tag}`);
      await material(page, glass);
      const cards = await rects(page, '.player-card');
      assert.equal(cards.length, 4);
      assert.ok(cards.every((c) => Math.abs(c.y - cards[0].y) < 1 && Math.abs(c.w - cards[0].w) < 1), 'four equal player cards: ' + JSON.stringify(cards));
      assert.equal(await style(page, '.player-card[aria-selected="true"]', 'borderTopColor'), 'rgb(224, 39, 46)', 'the selected player has a red border');
      assert.equal(await style(page, '.player-card.is-empty .player-icon', 'opacity'), '0.45', 'an empty player: the icon at 45%');
      const setup = await rect(page, '#menu-pad-setup'), screen = await rect(page, '#screen'), head = await rect(page, '.page-heading');
      assert.ok(Math.abs(setup.r - screen.r) < 1.5, 'Set up launcher controls sits on the content column\'s right edge');
      assert.ok(setup.y >= head.y - 1 && setup.b <= head.b + 1, 'it sits in the header row');
      assert.ok(screen.w <= 942.5, 'content column ≤ 942px');
      assert.equal(await page.getByRole('note', { name: 'B and X mapping' }).count(), 1);
      const [ls, stage, fb] = await Promise.all(['.left-stick', '.controller-stage', '.face-buttons'].map((s) => rect(page, s)));
      assert.ok(ls.x < stage.x && stage.x < fb.x, 'the mapping panel runs sticks | art | buttons');
      await page.screenshot({ path: path.join(out, `controls-${tag}.png`) });
      // the binding popover stays opaque
      await page.getByRole('button', { name: 'Assign A', exact: true }).click();
      await page.locator('.binding-editor:not([hidden])').waitFor();
      assert.match(await style(page, '.binding-editor', 'backgroundColor'), /^rgb\(/, 'the binding popover is opaque');
      await page.keyboard.press('Escape');
      await page.waitForFunction(() => document.querySelector('.binding-editor').hidden);
      // controller mode (spec §6.3): a pad connected, mouse in use: no prompts. Pad input: prompts + white ring. Mouse: gone.
      await page.mouse.move(300, 300); await page.mouse.move(360, 340);
      await page.waitForFunction(() => !document.body.classList.contains('gamepad-input'));
      assert.equal(await style(page, '.input-footer', 'display'), 'none', 'pad connected, mouse user: no hint bar');
      await page.locator('#player-tab-0').focus();
      await press(page, 15);
      await page.waitForFunction(() => document.body.classList.contains('gamepad-input'));
      assert.equal(await style(page, '.input-footer', 'display'), 'flex', 'pad input: the hint bar');
      const ring = await page.evaluate(() => { const s = getComputedStyle(document.activeElement); return `${s.outlineStyle} ${s.outlineWidth} ${s.outlineColor}`; });
      assert.equal(ring, 'solid 2px rgb(255, 255, 255)', 'a plain white focus ring, no glow');
      await noSideways(page, `controls pad ${tag}`);
      await page.screenshot({ path: path.join(out, `controls-pad-${tag}.png`) });
      await page.mouse.move(300, 300); await page.mouse.move(360, 340);
      await page.waitForFunction(() => !document.body.classList.contains('gamepad-input'));
      assert.equal(await style(page, '.input-footer', 'display'), 'none', 'mouse: the hint bar goes');
      await page.context().close();
    }
  }
  { // Reduced motion: every state change is instant.
    const mods = await open('mods.html');
    await mods.emulateMedia({ reducedMotion: 'reduce' });
    await mods.locator('.store-card').first().waitFor();
    assert.match(await style(mods, '.tab-ind', 'transitionDuration'), /^0s/, 'tabs under reduced motion');
    assert.match(await style(mods, '.store-card [role=switch]', 'transitionDuration', '::after'), /^0s/, 'switch under reduced motion');
    await mods.context().close();
    const music = await open('music.html');
    await music.emulateMedia({ reducedMotion: 'reduce' });
    await music.locator('.save-lane.open').waitFor();
    assert.match(await style(music, '.save-lane', 'transitionDuration'), /^0s/, 'save lane under reduced motion');
    await music.context().close();
  }
  { // Adam's logos: the sidebar logo is the same size on every page.
    const sizes = [];
    for (const f of ['index.html', 'mods.html', 'music.html', 'controls.html']) {
      const p = await open(f);
      await p.locator('.shell-nav .brand img').waitFor();
      sizes.push(await p.locator('.shell-nav .brand img').evaluate((n) => { const r = n.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map((v) => Math.round(v * 2) / 2).join(','); }));
      await p.context().close();
    }
    assert.ok(sizes.every((s) => s === sizes[0]), 'sidebar logo size differs between pages: ' + sizes.join(' | '));
  }
  // index.html only renders the sidebar here (its own commands are not stubbed); only the three pages' errors count.
  assert.deepEqual(errors.filter((e) => !e.startsWith('index.html')), []);
  console.log(`check-glass-mods-music-controls: PASS (screens in ${out})`);
} finally { await browser.close(); server.close(); }
```

- [ ] **Step 2: Run it**

Run: `node Tools/check-glass-mods-music-controls.mjs`
Expected: PASS, with about 40 screenshots in `target/glass-review/`.

If an assertion fails, the fix belongs to the task that owns the code:
- Mods layout: `pages/mods.css` / `mods.js` (Task 2);
- Music layout or bar: `pages/music.css` / `music.js` (Task 3);
- Controllers: `pages/controls.css` / `controls.js` (Task 4);
- durations, fields, controller art: `theme.css` (Task 1).

Fix it there, re-run that task's own check, then this one.

Open these beside the mockup's sections:
- `mods-1280-glass-slow.png`, `mods-800-solid-quick.png`
- `music-bar-1280-glass-slow.png`, `music-800-solid-quick.png`
- `controls-1280-glass-slow.png`, `controls-pad-1280-glass-slow.png`, `controls-800-solid-quick.png`

Check each against the rulings: no glows; plain white focus ring (pad shots only); no chips; red accent; no drawn art on Mods; "· Pending" and Undo only on the pending Music row; the save bar floating in its own lane with the list ending above it; the current controller icon on the player cards; the hint bar only in the `pad` shots.

- [ ] **Step 3: Commit**

Append `await import('./check-glass-mods-music-controls.mjs');` as the last line of `Tools/run-browser-tests.mjs`.

```bash
git add Tools/check-glass-mods-music-controls.mjs Tools/run-browser-tests.mjs
git commit -m "checks: Mods, Music and Controllers screens in every look, alignment, motion and controller mode (spec §13)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Whole-plan verification and the real window

**Files:** none new, unless a check fails. A failing check goes back to its owning task as a fix commit.

- [ ] **Step 1: Every JS check, individually**

Link Playwright, then run each of these and record PASS/FAIL:

```bash
node --test crates/reborn-ui/ui/stats-model.test.mjs crates/reborn-ui/ui/stats-rows.test.mjs crates/reborn-ui/ui/cover-color.test.mjs crates/reborn-ui/ui/settings-draft.test.mjs
node Tools/check-shell-pages.mjs
node Tools/check-page-call-args.mjs
node Tools/check-theme.mjs
node Tools/check-theme-tokens.mjs
node Tools/check-themed-shell.mjs
node Tools/check-save-lane.mjs
node Tools/check-menu.mjs
node Tools/check-tabs.mjs
node Tools/check-play-page.mjs
node Tools/check-settings-page.mjs
node Tools/check-glass-pages.mjs target/glass-review
node Tools/check-glass-mods-music-controls.mjs target/glass-review
node Tools/check-mods-page.mjs
node Tools/check-music-page.mjs
node Tools/check-music-length-message.mjs
node Tools/check-shell-chrome.mjs
node Tools/check-look-browser.mjs
node Tools/check-controller-navigation.mjs
node Tools/check-controller-hints.mjs
node Tools/check-controller-press-to-map.mjs
node Tools/check-controller-report.mjs
node Tools/check-play-controller-gate.mjs
node Tools/check-session-state.mjs
node Tools/check-update-notice.mjs
node Tools/check-content-notice.mjs
node Tools/test-menu-pad.mjs
node Tools/test-controller-report.mjs
node Tools/review-players.mjs
node Tools/run-browser-tests.mjs target/browser-review
```

Expected:
- every one PASSes (`check-shell-chrome`: PASS, or exactly the pre-existing failure recorded in Task 3);
- `run-browser-tests` prints its main-body PASS, then aborts at `review-finish`;
- the three baseline failures (`review-finish`, `check-launch-polish`, `check-online-mm`) fail exactly as before, and nothing else does.

Unlink Playwright.

- [ ] **Step 2: Rust stays green**

This plan touches no Rust, but the UI files are compiled into the binary, and one Rust test reads `mods.js` (it must still contain `withheld_note`).

```bash
taskkill /IM reborn-launcher.exe /F 2>NUL
cargo test --workspace -j 2
cargo build -p reborn-ui
```

Expected: the same test count as at the end of plan 2 (plan 1 recorded 1380 passed, 6 ignored), all passing; the build succeeds.

- [ ] **Step 3: The real window (dev build, never the installed beta)**

The dev build shares its identifier and data folder with the installed beta, so:

1. Back up `%APPDATA%\NFL Street Reborn\config\settings.json` and record its SHA256 with `Get-FileHash`.
2. Close any running launcher: `taskkill /IM reborn-launcher.exe /F`.
3. Start `target\debug\reborn-launcher.exe`.

If `target\glass-real\shot.ps1` does not exist (plan 2 created it), save this screenshot helper there:

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

WebView2 exposes no UI Automation items, so drive the window with clicks at window coordinates: `[W]::SetCursorPos(x,y); [W]::mouse_event(2,0,0,0,0); [W]::mouse_event(4,0,0,0,0)` in a one-off PowerShell call (with the same `Add-Type` block). Screenshot each item with `powershell -File target\glass-real\shot.ps1 <name>`.

**Adam's install is real: do not toggle his mods, apply music, or change his controller mappings.**

1. **Mods** (`11-mods`): Mica behind the sidebar; toolbar; sliding Footballs/Soundtrack tabs (click Soundtrack, then Footballs: the highlight glides slowly); the count line; glass cards with a plain preview; the red check on his enabled football only; one switch per card. Do **not** flip a switch. Click a card's preview: the details dialog opens (`12-mods-details`); press Esc.
2. **Music** (`13-music`): the header row (switch left; Reset, Import pack, Export pack, Add songs… right); the glass table. If his soundtrack has pending changes, the save bar floats in its own lane and the list ends above it (`14-music-bar`); **do not press Apply or Discard**. If nothing is pending, the bar is absent: note that. Do not open Replace (it opens a file picker).
3. **Controllers** (`15-controls`): Set up launcher controls top right as a normal button; the B/X note with an amber icon and Dismiss (do **not** press Dismiss; the check covers it); four glass player cards with the controller icon, the selected one red-bordered; the three-column mapping panel. Press **Test controller** then **Finish test** (`16-controls-test`): nothing is saved.
4. **Minimum window** (`17-mods-800`, `18-music-800`, `19-controls-800`): resize to the minimum (800×560). No sideways scroll on any of the three; the Music bar (if shown) is fully visible.
5. **Glass off** (`20-controls-solid`): Settings › Display › Window glass off, Save; open Controllers: the solid look, readable cards. Turn glass back on and Save.
6. **Needs Adam** (do not fake these): pad navigation with a real pad on all three pages (including D-pad into the Music bar), a real mod switch, a real Apply on Music, and a real press-to-map on Controllers.

Close the app. Restore `settings.json` from the backup and confirm the SHA256 matches.

- [ ] **Step 4: Hand off to Adam (GATE)**

Report:
- the screenshots (`target/glass-real/`, `target/glass-review/`);
- every check with PASS/FAIL;
- the three baseline failures by name;
- the "needs Adam" items;
- the decisions this plan made (listed below).

**Do not build or publish a beta or an installer.** Plan 4 (Stats, Profile, cleanup) follows on this branch.

---

## Decisions this plan made (surface to Adam at handoff)

1. **Mods previews are always the neutral gradient.** `mod_packages` carries no preview image, so "a real preview only if the package includes one" can never trigger without a Rust change. None was made.
2. **A Mods card's preview and name still open the existing details dialog** (description, size, "replaces other enabled mods" note), which keeps its Enable mod / Disable mod button. The card's one switch replaces the Enable/Disable button pair.
3. **The Mods count line counts what is listed** (after search), and says "only one football can be on at a time" only when the core's pick-one groups hold more than one listed package.
4. **Music pending source lines:** "Replacing ‹current song› · Pending" (a new song), "Restoring the original · Pending", "Moved here · Pending". The bar names the last change: "Song 3 replaced", "Song 3 restored", "Song 6 moved to 5", "Song 21 details changed".
5. **A pending Music row shows no length.** The core's pending view carries no duration; no Rust change.
6. **A pending row offers Undo only** (ruling). To replace a pending song, Undo first.
7. **One song at a time is added from its row's Replace** (the duplicate "Add song" is gone, spec §8.4). The song dialog still has its "Replace song" picker, so any slot can be chosen from there.
8. **Music's Apply waits** while the game image is missing or the core is encoding/building; Discard waits only while the core works. `savebar.js` gained `lock()` for this.
9. **Music source line for applied songs:** "Artist · On your disc / Your song / From ‹pack file›".
10. **The B/X note's dismissal is kept in the launcher window's local storage** (`h:bx-note-dismissed`), which lives in this install's WebView data. No new command. Its text is the mockup's shorter line.
11. **"Set up launcher controls" is the shared `menu-pad-setup` button,** now styled as a normal `.btn` top right. It is still disabled while no controller is connected (clicking it then did nothing before either); "not greyed out" was read as "normal button styling", not as enabling a button that cannot work.
12. **Player cards are 16px apart** (spec §3.3's 8/16/24), not the mockup's 12px.
13. **New theme tokens:** `--thumb-bg`, `--surface-solid`. `theme.css` also gained search/textarea field styling and the controller-art rules for themed pages.
14. **The controller live-test highlight** (`controller-preview.js` lights a pressed button with a red drop-shadow) is left as it is: it is test feedback inside the art, and the plan does not touch controller behaviour. Flag it if Adam counts it as a glow.
