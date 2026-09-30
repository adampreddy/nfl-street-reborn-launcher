# Launcher Glass Redesign, Plan 2: Play and Settings

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Play and Settings get the approved frosted-glass design (mockup "PLAY" and "SETTINGS"). Both pages load only `theme.css` and their own `pages/<page>.css`. Every Settings change goes through the save bar.

**Architecture:**
- Theme-only pages opt in with `<body class="shell themed …">`.
  - `theme.css` gains a base layer scoped to `body.themed`: type, fields, messages, dialogs and the update cards.
  - It also gains the shell for **every** page (grid, sidebar geometry, hint bar), written to outrank the legacy sheets. The sidebar is then identical, to the pixel, on themed and legacy pages.
- Two new shared components live in `theme.css` with small ES modules:
  - Menus: `menu.js`. A modal `<dialog>` on `<body>`, in the top layer.
  - Segmented tabs: `tabs.js`. One sliding highlight.
- The save bar gets:
  - a generic lane (`has-save-lane`);
  - a failure line;
  - pad reachability in `app-shell.js`.
- Settings keeps every pending change in one pure module (`settings-draft.js`). Save writes them in order; Discard reverts the previewed ones.

**Tech Stack:** Tauri 2.11 (Rust, untouched in this plan), static HTML/JS/CSS in `crates/reborn-ui/ui` (no bundler, `withGlobalTauri`), Playwright + Edge for browser checks, `node --test` for pure modules.

**Spec:** `docs/superpowers/specs/2026-09-30-launcher-glass-redesign-design.md`.
- Binding sections: §2, §3, §5, §6, §7, §8.1, §8.2, §10, §11, §13.
- Visual target: `docs/superpowers/specs/assets/2026-09-30-launcher-glass-mockup.html`, sections `<!-- PLAY -->` and `<!-- SETTINGS -->`.
- This is **plan 2**. Plan 1 (foundation) is done at `54288f0`. Plan 3 (Mods/Music/Controllers) reuses the save lane, menus and tabs built here.

**Working copy:** worktree `C:\Users\Adam\Desktop\Reborn Installer\NFL Street Reborn Launcher redesign`, branch `feat/launcher-glass-redesign`.
- Never commit to `main` or `release/*`.
- **Another agent may commit small fixes to `settings.js` on this branch.** Before overwriting a file, run `git log -3 --format="%h %s" -- <file>`. If a commit newer than `54288f0` touched it, read that diff and carry the fix into your version.

## Global Constraints

- **Colours** (theme only): `--red #E0272E`, `--red-ink #FF7A7F`, `--ok #3DDC97`, `--warn #FFC46B`, `--fg #F4F5F7`, `--fg-2 #AEB5C0`, `--fg-3 #7C8491`. Glass surfaces are `--glass-1/2/3`, strokes are `--stroke`/`--stroke-hi`.
- **Page stylesheets hold layout only.** `ui/pages/*.css` contains no hex, `rgb()`/`rgba()`/`hsl()` colour literals and no `font-weight` above 700. `Tools/check-theme-tokens.mjs` enforces this. `color-mix(…, var(--token) N%, transparent)` is allowed.
- **Themed pages load exactly** `theme.css` + their page file (spec §11). `Tools/check-theme.mjs` enforces this.
- **No glow anywhere.** Focus = `outline: 2px solid #fff; outline-offset: 3px` (already in `theme.css`). There are no coloured `box-shadow`s.
- **Weights:** 500 UI, 600 labels/values, 700 headline numbers and the Play now label (mockup). **Never above 700.**
- **Motion:**
  - Only `opacity`/`transform` animate. Heights use `max-height`/`grid-template-rows`. One exception: the tab highlight's `width` (see Task 6).
  - Durations come from `--d1/--d2/--d3`. Menus, switches and tabs use `--d2`; page enter, save bar and nav marker use `--d3`.
  - Reduced motion = 0.
- **Logos are Adam's:** never resize, stretch, filter or redraw `art/play-logo.png` or `art/nsr-sidebar.png`.
  - The Play wordmark renders at `width:470px; max-width:100%; height:auto`.
  - The sidebar logo slot stays **exactly as it renders today**: 124px tall, image `width:100%` of a nav with `14px` side padding.
- **Keep every existing nav icon SVG** byte-identical.
- **Accent is red.** No purple, no teal accents, except the approved Dolphin icon tile (token `--dolphin`).
- **Commands are the contract.**
  - No new Rust commands in this plan.
  - Every page value comes from a command that exists today.
  - `node Tools/check-page-call-args.mjs` must pass. Keep command arguments as **object literals** at the call site: the checker cannot read variables.
- **Safety and CSP:**
  - Server text only via `el()`/`textContent`, never `innerHTML` (test fixtures may use `innerHTML`).
  - CSP `script-src 'self'`: **no inline scripts in product pages**.
- **Controller navigation is first-class.**
  - Everything must be reachable by D-pad. B/Esc closes menus and dialogs.
  - The hint bar and focus ring appear only in controller mode (§6.3).
- **Line endings:** `.rs` files stay LF. This plan touches no `.rs` file.
- **No beta builds, no installer.** The end of the plan is a real-window check of `target/debug/reborn-launcher.exe`.
- **Commits:** one per task, message as given. Always end with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never `git stash` (the stash is shared across worktrees).

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

`node Tools/run-browser-tests.mjs` runs its own main body, then imports `review-players` (passes) and then `review-finish` (fails), and aborts there. So:
- **its main body must print** `PASS: launcher flows, navigation, onboarding, saves, settings, and motion stability.`;
- every other checker is run **individually**.

The standing check list, referred to below as **"the standing checks"**:

```bash
node Tools/check-shell-pages.mjs
node Tools/check-page-call-args.mjs
node Tools/check-theme.mjs
node Tools/check-theme-tokens.mjs
node Tools/check-controller-navigation.mjs
node Tools/check-controller-hints.mjs
node Tools/check-play-controller-gate.mjs
node Tools/check-look-browser.mjs
node Tools/check-session-state.mjs
node Tools/check-update-notice.mjs
node Tools/check-content-notice.mjs
node Tools/run-browser-tests.mjs target/browser-review
```

## Review Focus

1. **A pad user with unsaved Settings changes.** D-pad Down past the last row reaches **Save changes**. Up goes back into the page. B from the bar goes to the sidebar and never leaves the page (which would drop the changes). The test is in Task 2.
2. **A save that fails halfway** (Dolphin opened meanwhile, disk error). The bar names the setting and the reason. The failed change and everything after it stay pending. Nothing is shown as saved that was not. The tests are in Task 6 (draft) and Task 7 (page).
3. **A background refresh while a Play menu is open** (a pad connects, a session ends, the page redraws). The menu stays open, a pick still applies, and focus lands on the new cell. The test is in Task 5.
4. **An unconverted game image chosen in Settings, then Save.**
   - The Tier 2 choice appears.
   - Nothing is acknowledged until **Play it anyway**.
   - Cancel leaves the change pending, so Discard still drops it.
   - The test is in Task 7 (check-shell-pages driven test).
5. **The minimum window (800×560) with a controller-setup note on Play.** Nothing overlaps the wordmark, the notes end above the strip, and there is no sideways scroll. The test is in Task 5.

---

## File structure

| File | Responsibility |
|---|---|
| `crates/reborn-ui/ui/theme.css` (modify) | new tokens; shell for every page; `body.themed` base; save lane generalised; `.menu`, `.tabs`, `.field`, `.avatar` components |
| `crates/reborn-ui/ui/savebar.js` (modify) | failure line, `noun`/`saveText` options |
| `crates/reborn-ui/ui/app-shell.js` (modify) | the save lane counts as content for pad navigation and Back |
| `crates/reborn-ui/ui/menu.js` (create) | `openMenu(anchor, {label, options, current, onPick})` |
| `crates/reborn-ui/ui/tabs.js` (create) | `tabs(group, names, current, onPick, {disabled, label})` |
| `crates/reborn-ui/ui/settings-draft.js` (create) | pure pending-change list: `createDraft()`, `HELD` |
| `crates/reborn-ui/ui/settings-draft.test.mjs` (create) | `node --test` for the draft |
| `crates/reborn-ui/ui/shell.js` (modify) | `icon(paths, {fill})` helper (SVG namespace) |
| `crates/reborn-ui/ui/pages/play.css` (create) | Play layout only |
| `crates/reborn-ui/ui/pages/settings.css` (create) | Settings layout only |
| `crates/reborn-ui/ui/index.html` (modify) | theme-only Play |
| `crates/reborn-ui/ui/home.js` (modify) | Play: account, actions, status, notes, strip, menus |
| `crates/reborn-ui/ui/settings.html` (modify) | theme-only Settings with save lane |
| `crates/reborn-ui/ui/settings.js` (rewrite) | Settings: frame, tabs, rows, everything through the draft |
| `crates/reborn-ui/ui/*.html` (modify, all 12 shell pages) | empty `#input-hints` (no static hint text) |
| `Tools/check-theme.mjs` (modify) | themed pages load exactly theme + page file |
| `Tools/check-theme-tokens.mjs` (create) | no colour literals / weights > 700 in `ui/pages/*.css` |
| `Tools/check-themed-shell.mjs` (create) | shell geometry on a themed page, parity with a legacy page |
| `Tools/check-save-lane.mjs` (create) | lane row, failure line, pad into/out of the bar |
| `Tools/check-menu.mjs` (create) | menu component |
| `Tools/check-tabs.mjs` (create) | tabs component |
| `Tools/check-play-page.mjs` (create) | Play states, account, strip, menus, small window |
| `Tools/check-settings-page.mjs` (create) | Settings tabs, save bar for every change, paths |
| `Tools/check-glass-pages.mjs` (create) | §13 screenshots, alignment and motion for Play + Settings |
| `Tools/check-shell-pages.mjs`, `Tools/run-browser-tests.mjs`, `Tools/check-session-state.mjs`, `Tools/check-launch-polish.mjs` (modify) | expectations that change on purpose |

---

### Task 1: The shell for every page, and the theme-only base

**Files:**
- Modify: `crates/reborn-ui/ui/theme.css` (append; add tokens to `:root`)
- Modify: all 12 shell pages `crates/reborn-ui/ui/{index,controls,game,live,mods,music,onboarding,online,profile,recovery,settings,stats}.html` (empty `#input-hints` only)
- Modify: `Tools/check-theme.mjs`
- Create: `Tools/check-theme-tokens.mjs`, `Tools/check-themed-shell.mjs`

**Interfaces:**
- Produces (used by every later task):
  - body class `themed` = "this page loads only theme.css + its page file";
  - tokens `--field-bg`, `--hero-scrim`, `--on-art`, `--avatar-bg`, `--dolphin`;
  - components `.avatar` and `@keyframes live-pulse`;
  - the `THEMED` map in `Tools/check-theme.mjs` (page name → its stylesheet), which later tasks extend;
  - `node Tools/check-theme-tokens.mjs [dir]`.
- The hint bar exists only while `#input-hints` has children. `app-shell.js` already empties it outside controller mode.

- [ ] **Step 1: Write the failing checks**

Create `Tools/check-theme-tokens.mjs`:

```js
// Page stylesheets hold layout only (spec §11, §13): no colour literals, no weight above 700.
// Every colour comes from a theme.css token.
//     node Tools/check-theme-tokens.mjs [dir]      (default crates/reborn-ui/ui/pages)
import { readFileSync, readdirSync, existsSync } from "node:fs";
import path from "node:path";

const dir = path.resolve(process.argv[2] || "crates/reborn-ui/ui/pages");
if (!existsSync(dir)) { console.log(`check-theme-tokens: PASS (no ${dir} yet)`); process.exit(0); }
const problems = [];
let files = 0;
for (const name of readdirSync(dir).filter((f) => f.endsWith(".css"))) {
  files++;
  // Comments out, newlines kept so line numbers stay true.
  const css = readFileSync(path.join(dir, name), "utf8").replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "));
  // Innermost { … } blocks are declarations; selectors (#main, #play-now) sit outside them.
  for (const block of css.matchAll(/\{([^{}]*)\}/g)) {
    const line = css.slice(0, block.index).split("\n").length;
    const body = block[1];
    const where = `${name}:${line}`;
    for (const m of body.matchAll(/#[0-9a-fA-F]{3,8}\b/g)) problems.push(`${where}: colour literal ${m[0]}`);
    for (const m of body.matchAll(/\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\s*\(/gi)) problems.push(`${where}: colour function ${m[0]}`);
    for (const m of body.matchAll(/font-weight\s*:\s*(\d+)/g)) if (+m[1] > 700) problems.push(`${where}: font-weight ${m[1]}`);
    for (const m of body.matchAll(/(?:^|[;\s])font\s*:\s*(\d{3})\b/g)) if (+m[1] > 700) problems.push(`${where}: font weight ${m[1]}`);
  }
}
if (problems.length) { console.error("check-theme-tokens: FAIL\n  " + problems.join("\n  ")); process.exit(1); }
console.log(`check-theme-tokens: PASS (${files} page stylesheet(s))`);
```

In `Tools/check-theme.mjs`, replace the loop:

```js
for (const p of PAGES) {
  const html = readFileSync(ui + p + ".html", "utf8");
  const head = html.slice(0, html.indexOf("</head>"));
  assert.match(head, /<script src="look-boot\.js"><\/script>/, `${p}: look-boot.js must load in <head>, classic and synchronous`);
  const sheets = [...head.matchAll(/href="([^"]+\.css)"/g)].map(m => m[1]);
  assert.equal(sheets.at(-1), "theme.css", `${p}: theme.css must be the LAST stylesheet (got ${sheets.join(", ")})`);
  assert.doesNotMatch(html, /<script>(?!<\/script>)/, `${p}: no inline scripts (CSP script-src 'self')`);
}
```

with:

```js
// Pages moved onto the design system load exactly theme.css + their own page file (spec §11) and say so
// with body.themed. Later tasks add pages here as they move.
const THEMED = {};
for (const p of PAGES) {
  const html = readFileSync(ui + p + ".html", "utf8");
  const head = html.slice(0, html.indexOf("</head>"));
  assert.match(head, /<script src="look-boot\.js"><\/script>/, `${p}: look-boot.js must load in <head>, classic and synchronous`);
  const sheets = [...head.matchAll(/href="([^"]+\.css)"/g)].map(m => m[1]);
  if (THEMED[p]) {
    assert.deepEqual(sheets, ["theme.css", THEMED[p]], `${p}: must load exactly theme.css + ${THEMED[p]} (got ${sheets.join(", ")})`);
    assert.match(html, /<body class="[^"]*\bthemed\b/, `${p}: body must carry the themed class`);
    assert.ok(existsSync(ui + THEMED[p]), `${p}: ${THEMED[p]} must exist`);
  } else {
    assert.equal(sheets.at(-1), "theme.css", `${p}: theme.css must be the LAST stylesheet (got ${sheets.join(", ")})`);
  }
  assert.match(html, /<span id="input-hints"><\/span>/, `${p}: #input-hints starts empty (the hint bar exists only while a prompt is shown)`);
  assert.doesNotMatch(html, /<script>(?!<\/script>)/, `${p}: no inline scripts (CSP script-src 'self')`);
}
```

and change its import line to `import { readFileSync, existsSync } from "node:fs";`.

Create `Tools/check-themed-shell.mjs`:

```js
// The shell every page shares (spec §6) on a page that loads only theme.css (body.themed), and the
// same sidebar, to the half pixel, on a page that still loads the legacy sheets.
//     node Tools/check-themed-shell.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const nav=readFileSync(path.join(ui,'index.html'),'utf8').match(/<nav class="shell-nav"[\s\S]*?<\/nav>/)[0];
const fixture=`<!doctype html><html lang="en" data-glass="off" data-motion="quick"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css"></head>
<body class="shell themed page-test"><a class="skip-link" href="#main">Skip to content</a>${nav}
<main id="main" tabindex="-1"><div class="page-heading"><span class="eyebrow">NFL STREET REBORN</span><h1>Title</h1><p class="purpose">Subtitle</p></div>
<section id="screen"><button class="btn" id="b1">One</button><button id="plain">Plain</button><p class="refusal">Nope</p></section></main>
<footer class="input-footer"><span id="input-hints"></span><span>NFL STREET REBORN</span></footer></body></html>`;
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/fixture.html'){res.setHeader('Content-Type','text/html');res.end(fixture);return;}
 const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
const box=(p,sel)=>p.locator(sel).first().evaluate(n=>{const r=n.getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height};});
const navBoxes=p=>p.locator('.shell-nav a').evaluateAll(ns=>ns.map(n=>{const r=n.getBoundingClientRect();return [r.x,r.y,r.width,r.height].map(v=>Math.round(v*2)/2).join(',');}));
try{
 const ctx=await browser.newContext({viewport:{width:1280,height:800}});
 const p=await ctx.newPage();const errors=[];p.on('pageerror',e=>errors.push(e.message));
 await p.goto(base+'/fixture.html');await p.evaluate(()=>document.fonts.ready);
 assert.equal((await box(p,'.shell-nav')).w,184,'sidebar is 184px');
 const items=await p.locator('.shell-nav a:not(.brand)').evaluateAll(ns=>ns.map(n=>n.getBoundingClientRect().height));
 assert.ok(items.length===8&&items.every(h=>h===40),'every nav item is 40px: '+items);
 assert.equal((await box(p,'.shell-nav .brand')).h,124,'logo slot as it is today');
 const main=await box(p,'#main');assert.deepEqual([main.x,main.w,main.h],[184,1096,800],'content fills the rest of the window');
 assert.equal(await p.locator('.input-footer').evaluate(n=>getComputedStyle(n).display),'none','no prompt, no hint bar');
 const skip=await box(p,'.skip-link');assert.ok(skip.y+skip.h<=0,'skip link off screen until focused');
 assert.equal(await p.locator('.page-heading h1').evaluate(n=>getComputedStyle(n).fontSize+' '+getComputedStyle(n).fontWeight),'30px 700');
 assert.equal(await p.locator('.page-heading .eyebrow').evaluate(n=>getComputedStyle(n).display),'none');
 assert.equal((await box(p,'#b1')).h,38,'.btn is 38px');
 assert.equal((await box(p,'#plain')).h,38,'a classless button (shared dialogs) looks like .btn');
 // A prompt appears: the bar is 34px and the content gives it the room.
 await p.evaluate(()=>{const s=document.createElement('span');s.className='input-hint';s.innerHTML='<kbd>A</kbd> Select';document.getElementById('input-hints').append(s);});
 assert.equal(await p.locator('.input-footer').evaluate(n=>getComputedStyle(n).display),'flex');
 assert.equal((await box(p,'.input-footer')).h,34);
 assert.equal((await box(p,'#main')).h,766);
 assert.equal(await p.locator('.input-footer > span:not(#input-hints)').evaluate(n=>getComputedStyle(n).display),'none','the hint bar shows prompts only');
 const themedNav=await navBoxes(p);
 // The same sidebar on a page that still loads the legacy sheets.
 const legacy=await ctx.newPage();
 await legacy.addInitScript(()=>{window.__TAURI__={core:{invoke:async()=>null}};});
 await legacy.goto(base+'/mods.html');await legacy.locator('.shell-nav a').first().waitFor();await legacy.evaluate(()=>document.fonts.ready);
 assert.deepEqual(await navBoxes(legacy),themedNav,'legacy and themed sidebars match');
 assert.equal(await legacy.locator('.input-footer').evaluate(n=>getComputedStyle(n).display),'none','legacy page: no prompt, no hint bar');
 // Minimum window.
 await p.setViewportSize({width:800,height:560});await p.goto(base+'/fixture.html');
 assert.equal((await box(p,'.shell-nav')).w,160);
 assert.ok(await p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no sideways scroll at 800x560');
 assert.deepEqual(errors,[]);
 console.log('check-themed-shell: PASS');
}finally{await browser.close();server.close();}
```

- [ ] **Step 2: Run them and confirm they fail**

Link Playwright (see Global Constraints), then:
- `node Tools/check-theme.mjs`. Expected: FAIL on `index: #input-hints starts empty`.
- `node Tools/check-themed-shell.mjs`. Expected: FAIL. The sidebar is not 184px, or the nav items are not 40px: the fixture has no legacy sheet, so nothing lays the shell out yet.
- `node Tools/check-theme-tokens.mjs`. Expected: PASS (no `pages/` yet).

Prove the token checker bites. Write `target/tokens-probe/bad.css` containing `.a{color:#fff;font-weight:800}`, then run `node Tools/check-theme-tokens.mjs target/tokens-probe`. Expected: FAIL listing `colour literal #fff` and `font-weight 800`. Delete `target/tokens-probe` afterwards.

- [ ] **Step 3: Empty `#input-hints` on every page**

Run from the worktree root (Git Bash):

```bash
node -e "const fs=require('fs');for(const p of ['index','controls','game','live','mods','music','onboarding','online','profile','recovery','settings','stats']){const f='crates/reborn-ui/ui/'+p+'.html';const s=fs.readFileSync(f,'utf8');const t=s.replace(/<span id=\"input-hints\">[\s\S]*?<\/span>/,'<span id=\"input-hints\"></span>');if(t===s)throw new Error(p+': no #input-hints');fs.writeFileSync(f,t);}"
```

`app-shell.js` fills `#input-hints` in controller mode. A static text there made the hint bar flash at 34px on every page load.

- [ ] **Step 4: Add the tokens**

In `theme.css`, inside the first `:root{…}` block, directly after `--d1:480ms;--d2:720ms;--d3:960ms;` add:

```css
  /* surfaces with a job of their own (spec §3.4, §8.1, §8.2; mockup values) */
  --field-bg:rgba(0,0,0,.25);
  --hero-scrim:linear-gradient(90deg,rgba(10,11,14,.78) 0,rgba(10,11,14,.35) 38%,transparent 62%),linear-gradient(0deg,rgba(10,11,14,.85) 0,transparent 42%);
  --on-art:rgba(10,11,14,.35);
  --avatar-bg:linear-gradient(135deg,#5865F2,#3b44b8);
  --dolphin:#22C7B4;
```

- [ ] **Step 5: Append the shell and the themed base to `theme.css`**

Append at the very end of `theme.css`:

```css
/* ================= Shell, every page (spec §6) =================
   Written with an html prefix so it outranks the legacy sheets (launcher/product-ui/finish.css):
   the sidebar and frame are identical on themed and legacy pages. */
*,*::before,*::after{box-sizing:border-box}
html body.shell{--rail:184px;margin:0}
@media (max-width:1000px){html body.shell{--rail:160px}}
@media (max-width:650px){html body.shell{--rail:68px}}
html body.shell:not(.page-setup){display:grid;grid-template-columns:var(--rail) minmax(0,1fr);grid-template-rows:0 minmax(0,1fr) var(--hint-height);grid-template-areas:'nav head' 'nav main' 'nav foot';height:100vh;overflow:hidden}
html body.shell:not(.page-setup) .app-header{display:none}
html body.shell:not(.page-setup) #main{grid-area:main;min-width:0;min-height:0}
/* sidebar geometry. The logo slot is exactly today's (124px, image 100% of a 14px-padded nav): Adam's logo is never resized */
html body.shell:not(.page-setup) .shell-nav{grid-area:nav;display:flex;flex-direction:column;gap:4px;min-height:0;overflow:auto;padding:0 14px 20px}
html body.shell:not(.page-setup) .shell-nav .brand{display:flex;align-items:center;justify-content:center;flex:none;height:124px;padding:0;overflow:hidden;background:none;border:0}
html body.shell:not(.page-setup) .shell-nav .brand img{display:block;width:100%;height:auto;max-height:none;object-fit:contain}
html body.shell:not(.page-setup) .shell-nav a:not(.brand){display:flex;align-items:center;gap:12px;height:40px;min-height:40px;padding:0 12px;border:0;font:500 13px/1 Inter,system-ui,sans-serif;letter-spacing:0;text-transform:none;text-decoration:none}
html body.shell:not(.page-setup) .shell-nav a:not(.brand)>svg{display:block;width:17px;height:17px;flex:none;opacity:.85}
html body.shell:not(.page-setup) .shell-nav .nav-bottom{margin-top:auto;padding-top:16px;display:flex;flex-direction:column;gap:4px}
@media (max-width:650px){
  html body.shell:not(.page-setup) .shell-nav{padding:0 8px 16px}
  html body.shell:not(.page-setup) .shell-nav a:not(.brand){justify-content:center;padding:0}
  html body.shell:not(.page-setup) .shell-nav a:not(.brand)>span{display:none}
  html body.shell:not(.page-setup) .shell-nav .brand{height:88px}
}
/* hint bar: exists only while there is a prompt to show (spec §6.3). :has(#id) outranks every legacy rule. */
html body.shell:has(#input-hints:empty){--hint-height:0px}
html body.shell:has(#input-hints:not(:empty)){--hint-height:34px}
html body.shell:has(#input-hints:empty) .input-footer{display:none}
html body.shell:has(#input-hints:not(:empty)) .input-footer{display:flex}
html body.shell .input-footer{grid-area:foot;align-items:center;gap:22px;height:34px;min-height:34px;padding:0 24px;overflow:hidden;white-space:nowrap;font-size:11.5px;color:var(--fg-3);background:rgba(10,11,14,.45)!important;border-top:1px solid var(--stroke)!important;backdrop-filter:blur(20px);-webkit-backdrop-filter:blur(20px)}
html body.shell .input-footer>span:not(#input-hints){display:none}
html body.shell #input-hints{display:flex;align-items:center;gap:22px;min-width:0}
html body.shell .input-hint{display:inline-flex;align-items:center;white-space:nowrap}
html body.shell .input-hint kbd{margin:0 7px 0 0!important}

/* shared small components */
.avatar{display:inline-grid;place-items:center;flex:none;width:32px;height:32px;border-radius:50%;background:var(--avatar-bg);color:#fff;font:700 13px Inter,system-ui,sans-serif}
@keyframes live-pulse{50%{opacity:.35}}
.btn{text-decoration:none;white-space:nowrap}
.btn:disabled{opacity:.55;cursor:default;pointer-events:none}

/* ================= Base for pages that load only theme.css + their page file (body.themed) ================= */
body.themed{font:500 14px/1.45 Inter,system-ui,sans-serif;color:var(--fg);-webkit-font-smoothing:antialiased;color-scheme:dark}
body.themed :where(h1,h2,h3,p,figure,ul,ol){margin:0}
body.themed a{color:inherit}
body.themed button{font:inherit;color:inherit}
body.themed img{max-width:100%}
body.themed [hidden]{display:none!important}
body.themed *{scrollbar-width:thin;scrollbar-color:rgba(255,255,255,.2) transparent}
body.themed #main{overflow:auto;outline:none}
body.themed .page-heading .eyebrow{display:none}
body.themed .page-heading h1{font:700 30px/1.1 Inter,system-ui,sans-serif;letter-spacing:-.02em}
body.themed .page-heading .purpose{margin-top:8px;color:var(--fg-2)}
body.themed .loading{color:var(--fg-2)}
body.themed details.core{display:none}
body.themed .skip-link{position:fixed;left:16px;top:8px;z-index:100;padding:8px 16px;border-radius:var(--r-sm);background:var(--red);color:#fff;text-decoration:none;transform:translateY(-200%)}
body.themed .skip-link:focus{transform:none}
body.themed.gamepad-input .skip-link{visibility:hidden}
/* buttons with no class of their own come from shared scripts (update-notice, menu-pad-setup): they look like .btn */
body.themed button:not([class]),body.themed button.primary:not(.btn),body.themed a.button{display:inline-flex;align-items:center;justify-content:center;gap:8px;height:38px;padding:0 16px;border-radius:var(--r-md);font:600 13px Inter,system-ui,sans-serif;color:var(--fg);background:var(--glass-2);border:1px solid var(--stroke);box-shadow:var(--sheen);cursor:pointer;text-decoration:none}
body.themed button:not([class]):hover,body.themed a.button:hover{background:var(--glass-3);border-color:var(--stroke-hi)}
body.themed button.primary:not(.btn){background:linear-gradient(180deg,#ec3a41,#c91f26);border-color:rgba(255,255,255,.18);box-shadow:inset 0 1px 0 rgba(255,255,255,.22),0 6px 16px -8px rgba(0,0,0,.6)}
body.themed button:disabled{opacity:.55;cursor:default}
/* fields */
body.themed select,body.themed input[type=text]{height:38px;min-width:0;padding:0 12px;border-radius:var(--r-md);border:1px solid var(--stroke);background:var(--field-bg);color:var(--fg);font:500 13px Inter,system-ui,sans-serif;color-scheme:dark}
body.themed select:hover:not(:disabled){border-color:var(--stroke-hi)}
body.themed select:disabled,body.themed input:disabled{opacity:.55}
body.themed input[type=range],body.themed progress,body.themed input[type=checkbox]{accent-color:var(--red)}
body.themed.gamepad-input .pad-editing{outline-color:var(--red-ink)!important}
/* messages */
body.themed .refusal,body.themed .partial,body.themed .said{padding:12px 16px;border-radius:var(--r-md);border:1px solid;font-size:12.5px;line-height:1.6;overflow-wrap:anywhere}
body.themed .refusal{border-color:rgba(224,39,46,.35);background:rgba(224,39,46,.08);color:var(--red-ink)}
body.themed .partial{border-color:rgba(255,196,107,.3);background:rgba(255,196,107,.07);color:var(--warn)}
body.themed .said{border-color:rgba(61,220,151,.3);background:rgba(61,220,151,.07);color:var(--ok)}
body.themed .note,body.themed .hint{font-size:12px;color:var(--fg-2)}
/* facts table */
body.themed .facts{width:100%;border-collapse:collapse;table-layout:fixed;font-size:12px}
body.themed .facts th{width:120px;padding:8px 16px 8px 0;text-align:left;vertical-align:top;font-weight:500;color:var(--fg-3)}
body.themed .facts td{padding:8px 0;color:var(--fg-2);overflow-wrap:anywhere}
body.themed .facts tr+tr>*{border-top:1px solid var(--stroke)}
body.themed .facts .absent{color:var(--fg-3);font-style:italic}
/* dialogs and the update dock (the surface colour comes from the floating-surfaces rule above) */
body.themed dialog{max-width:calc(100vw - 48px);color:var(--fg);border:1px solid var(--stroke-hi);border-radius:var(--r-lg);padding:24px;box-shadow:var(--sheen),var(--shadow)}
body.themed dialog::backdrop{background:rgba(8,9,12,.6)}
body.themed .update-center{position:fixed;right:24px;bottom:24px;z-index:50;display:grid;gap:16px;width:min(360px,calc(100vw - 48px));max-height:65vh;overflow:auto}
body.themed:has(#input-hints:not(:empty)) .update-center{bottom:58px}
body.themed .update-card{display:grid;gap:8px;padding:16px;border:1px solid var(--stroke-hi);border-radius:var(--r-lg);box-shadow:var(--sheen),var(--shadow)}
body.themed .update-card .notice-headline{font-size:14px;font-weight:600}
body.themed .update-card .update-message{font-size:12.5px;color:var(--fg-2);overflow-wrap:anywhere}
body.themed .update-card button{justify-self:end}
body.themed .update-card progress{width:100%;height:8px}
body.themed .update-dialog{width:min(540px,calc(100vw - 48px));max-height:80vh;overflow:auto}
body.themed .update-dialog h2{font:600 20px Inter,system-ui,sans-serif;margin-bottom:8px}
body.themed .update-version{margin-bottom:16px;color:var(--fg-2);font-size:13px}
body.themed .update-changes{max-height:34vh;overflow:auto;border-block:1px solid var(--stroke);padding:16px 0}
body.themed .update-changes ul{padding-left:20px}
body.themed .update-changes li{padding:4px 0;font-size:13px;overflow-wrap:anywhere}
body.themed .update-countdown{margin:16px 0;font-size:12px;color:var(--fg-2)}
body.themed .update-dialog-actions{display:flex;justify-content:flex-end;gap:8px;flex-wrap:wrap}
body.themed #menu-pad-setup[hidden],body.themed .menu-pad-setup-dialog [hidden]{display:none!important}
```

- [ ] **Step 6: Run the checks and confirm they pass**

- `node Tools/check-theme.mjs`: PASS (12 pages).
- `node Tools/check-themed-shell.mjs`: PASS.
- `node Tools/check-theme-tokens.mjs`: PASS.

Then run **the standing checks**. Expected: all PASS, and the `run-browser-tests` main body prints its PASS line.

Open `target/browser-review/reborn-launcher-mods.png` and `reborn-launcher-home.png`. Confirm the sidebar looks as before, with items 40px apart, and no hint strip at the bottom.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/theme.css crates/reborn-ui/ui/*.html Tools/check-theme.mjs Tools/check-theme-tokens.mjs Tools/check-themed-shell.mjs
git commit -m "theme: shell for every page, body.themed base, hint bar only with a prompt

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Append `await import('./check-themed-shell.mjs');` as the last line of `Tools/run-browser-tests.mjs`. Include that file in the same commit: add it to the `git add` above.

---

### Task 2: The save lane for any page, a failure line, and a pad that reaches it

**Files:**
- Modify: `crates/reborn-ui/ui/theme.css` (replace the settings-only lane rule)
- Modify: `crates/reborn-ui/ui/savebar.js`
- Modify: `crates/reborn-ui/ui/app-shell.js`
- Modify: `crates/reborn-ui/ui/settings.html` (body class only)
- Create: `Tools/check-save-lane.mjs`

**Interfaces:**
- Consumes: `body.themed`, the Task 1 shell grid.
- Produces:
  - A page opts in with `<body class="… has-save-lane">` and `<div id="save-lane"></div>` directly after `</main>`.
  - `mountSaveBar(lane, { onSave, onDiscard, returnFocus, noun = "unsaved change", saveText = "Save changes" }) → { update(changes: string[]), fail(message: string) }`
    - If `onSave`/`onDiscard` throws, the reason (`reason(e)`) is shown in `.save-bar [role=alert]`. The bar stays open and the buttons come back.
    - Plan 3's Music calls it with `noun: "pending change", saveText: "Apply changes"`.

- [ ] **Step 1: Write the failing check**

Create `Tools/check-save-lane.mjs`:

```js
// The save lane (spec §7): its own row under #main; a failed save says why in the bar; a pad reaches
// the bar from the bottom of the page, moves along it, goes back up, and Back goes to the sidebar.
//     node Tools/check-save-lane.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const nav=readFileSync(path.join(ui,'index.html'),'utf8').match(/<nav class="shell-nav"[\s\S]*?<\/nav>/)[0];
const html=`<!doctype html><html lang="en" data-glass="off" data-motion="quick"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css"></head>
<body class="shell themed page-test has-save-lane">${nav}
<main id="main" tabindex="-1"><section id="screen"><button class="btn" id="a">First</button><br><button class="btn" id="b">Second</button></section></main>
<div id="save-lane"></div><footer class="input-footer"><span id="input-hints"></span></footer>
<script type="module">
import {mountSaveBar} from './savebar.js';
window.outcome=null;window.saves=0;
window.bar=mountSaveBar(document.getElementById('save-lane'),{
 onSave:async()=>{if(window.outcome)throw new Error(window.outcome);window.saves++;window.bar.update([]);},
 onDiscard:async()=>{window.bar.update([]);},
 returnFocus:()=>document.getElementById('b')});
window.ready=true;
</script><script type="module" src="app-shell.js"></script></body></html>`;
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/lane.html'){res.setHeader('Content-Type','text/html');res.end(html);return;}
 const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
try{
 const page=await browser.newPage({viewport:{width:1280,height:800}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  window.pads=[{id:'Test pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:18},()=>({pressed:false,value:0}))}];
  Object.defineProperty(navigator,'getGamepads',{value:()=>pads});
  document.hasFocus=()=>true;
  window.__TAURI__={core:{invoke:async cmd=>cmd==='session_active'?false:null}};
  window.frames=[];window.clock=0;window.requestAnimationFrame=fn=>(frames.push(fn),frames.length);window.cancelAnimationFrame=()=>{};
  window.tick=()=>{clock+=16;frames.splice(0).forEach(fn=>fn(clock));};
 });
 await page.goto(`http://127.0.0.1:${server.address().port}/lane.html`);
 await page.waitForFunction(()=>window.ready&&frames.length>0);
 const press=b=>page.evaluate(b=>{pads[0].buttons[b]={pressed:true,value:1};tick();pads[0].buttons[b]={pressed:false,value:0};tick();},b);
 const lane=()=>page.evaluate(()=>{const l=document.getElementById('save-lane');return {open:l.classList.contains('open'),inert:l.inert};});
 assert.deepEqual(await lane(),{open:false,inert:true},'closed until something changes');
 await page.evaluate(()=>bar.update(['Emulator location']));
 assert.deepEqual(await lane(),{open:true,inert:false});
 await page.waitForTimeout(400);
 const edge=await page.evaluate(()=>({main:document.getElementById('main').getBoundingClientRect().bottom,lane:document.getElementById('save-lane').getBoundingClientRect().top}));
 assert.ok(Math.abs(edge.main-edge.lane)<0.5,'the lane is its own row: the content ends where it starts');
 assert.match(await page.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Emulator location/);
 // A failed save says why, keeps the bar, and gives the buttons back.
 await page.evaluate(()=>window.outcome='The disk is full.');
 await page.getByRole('button',{name:'Save changes'}).click();
 await page.locator('.save-bar [role=alert]').waitFor();
 assert.equal(await page.locator('.save-bar [role=alert]').innerText(),'The disk is full.');
 assert.equal((await lane()).open,true,'the bar stays open');
 assert.equal(await page.getByRole('button',{name:'Save changes'}).isEnabled(),true);
 // The next attempt clears the old reason.
 await page.evaluate(()=>window.outcome=null);
 await page.getByRole('button',{name:'Save changes'}).click();
 await page.waitForFunction(()=>!document.getElementById('save-lane').classList.contains('open'));
 assert.equal(await page.evaluate(()=>saves),1);
 assert.equal(await page.locator('.save-bar [role=alert]').isHidden(),true);
 // Pad: Down from the last control enters the bar at Save; Left moves along it; Up goes back; Back goes to the sidebar.
 await page.evaluate(()=>bar.update(['Window glass']));
 await page.evaluate(()=>document.getElementById('b').focus());
 await press(13);
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'Save changes','D-pad down reaches the save bar');
 await press(14);
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'Discard','left moves along the bar');
 await press(12);
 assert.equal(await page.evaluate(()=>document.getElementById('main').contains(document.activeElement)),true,'up goes back into the page');
 await press(13);await press(1);
 assert.equal(await page.evaluate(()=>location.pathname),'/lane.html','Back from the bar never leaves the page');
 assert.equal(await page.evaluate(()=>!!document.activeElement.closest('.shell-nav')),true,'Back from the bar goes to the sidebar');
 assert.deepEqual(errors,[]);
 console.log('check-save-lane: PASS');
}finally{await browser.close();server.close();}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `node Tools/check-save-lane.mjs`
Expected: FAIL. Either the lane is not its own row (no `has-save-lane` rule), or the save rejects with an unhandled error and no `[role=alert]` appears.

- [ ] **Step 3: Generalise the lane in `theme.css`**

Replace these three lines:

```css
/* settings: the save lane is its own grid row under #main, in the same column (never over content) */
body.shell.page-settings,body.shell.page-settings.gamepad-input{grid-template-rows:0 minmax(0,1fr) auto var(--hint-height);grid-template-areas:'nav head' 'nav main' 'nav save' 'nav foot'}
body.shell.page-settings #save-lane{grid-area:save;min-width:0}
```

with:

```css
/* save lane (savebar.js): any page with body.has-save-lane gets its own grid row under #main, in the same
   column, so the bar never covers content (spec §6.2, §7). Settings now; Music in plan 3. */
html body.shell.has-save-lane:not(.page-setup){grid-template-rows:0 minmax(0,1fr) auto var(--hint-height);grid-template-areas:'nav head' 'nav main' 'nav save' 'nav foot'}
html body.shell.has-save-lane #save-lane{grid-area:save;min-width:0}
.save-bar .save-text{min-width:0}
.save-bar .err{margin-top:2px;font-size:12.5px;color:var(--red-ink);overflow-wrap:anywhere}
```

In `settings.html`, change `<body class="shell ob page-settings">` to `<body class="shell ob page-settings has-save-lane">`.

- [ ] **Step 4: The failure line in `savebar.js`**

Replace the whole file with:

```js
// The one save bar (spec §7). It lives in its own lane at the bottom of the page, so it never covers content.
// A page opts in with <body class="… has-save-lane"> and an empty <div id="save-lane"></div> after #main.
// A save or discard that throws is said in the bar (its reason, not thrown past it); the bar stays open.
import { el, reason } from "./shell.js";

export function mountSaveBar(lane, { onSave, onDiscard, returnFocus, noun = "unsaved change", saveText = "Save changes" }) {
  lane.classList.add("save-lane");
  lane.inert = true; // closed = not focusable, not clickable
  const title = el("div", { class: "t" }), detail = el("div", { class: "d" });
  const error = el("p", { class: "err", role: "alert" });
  error.hidden = true;
  const discard = el("button", { type: "button", class: "btn", text: "Discard" });
  const save = el("button", { type: "button", class: "btn primary", text: saveText });
  let busy = false;
  const fail = (message) => { error.textContent = message || ""; error.hidden = !message; };
  const run = (fn) => async () => {
    if (busy) return;
    busy = true; discard.disabled = save.disabled = true; fail("");
    try { await fn(); } catch (e) { fail(reason(e)); }
    finally { busy = false; discard.disabled = save.disabled = false; }
  };
  discard.addEventListener("click", run(onDiscard));
  save.addEventListener("click", run(onSave));
  lane.replaceChildren(el("div", { class: "save-bar", role: "region", "aria-label": "Unsaved changes" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("div", { class: "save-text" }, title, detail, error), discard, save));
  return {
    update(changes) {
      const n = changes.length;
      title.textContent = `${n} ${noun}${n === 1 ? "" : "s"}`;
      detail.textContent = changes.at(-1) || "";
      const hadFocus = n === 0 && lane.contains(document.activeElement);
      lane.classList.toggle("open", n > 0);
      lane.inert = n === 0;
      if (n === 0) fail("");
      if (hadFocus) (returnFocus?.() || document.getElementById("main"))?.focus({ preventScroll: true });
    },
    fail,
  };
}
```

- [ ] **Step 5: The pad reaches the lane (`app-shell.js`)**

Make these five edits.

**(a)** After `const hints = document.getElementById('input-hints');` add:

```js
// Pages with a save bar keep it in its own lane after #main (spec §7). It is content too: the pad reaches
// it from the bottom of the page, and Back from it goes to the sidebar, never off the page.
const saveLane = () => document.getElementById('save-lane');
const inContent = n => !!n && (main.contains(n) || !!saveLane()?.contains(n));
```

**(b)** Replace the whole `function focus(n, reveal = true) { … }` with:

```js
function focus(n, reveal = true) {
  if (!visible(n)) return;
  document.querySelectorAll('.pad-focus').forEach(x => x.classList.remove('pad-focus'));
  n.focus({preventScroll:true});
  n.classList.add('pad-focus');
  // The save lane never scrolls; everything else scrolls its own area to show the control.
  const area = n.closest('dialog[open]') || n.closest('.shell-nav') || (main.contains(n) ? main : null);
  if (reveal && area) {
    const target = n.closest('label.choice') || n;
    const rect = target.getBoundingClientRect(), viewport = area.getBoundingClientRect();
    const top = viewport.top + 16, bottom = viewport.bottom - 16;
    const delta = rect.top < top ? rect.top-top : rect.bottom > bottom ? rect.bottom-bottom : 0;
    if (delta) scrollToTarget(area,area.scrollTop+delta);
  }
}
```

**(c)** In the `focusin` listener, replace `if (main.contains(n)) contentFocus = lastFocus;` with `if (inContent(n)) contentFocus = lastFocus;`.

**(d)** In `move(direction)`:

Replace

```js
  if (inNav && direction === 'right') {
    focus(contentFocus && reachable(main).find(n => identity(n) === contentFocus.key) || primary()); return;
  }
```

with

```js
  if (inNav && direction === 'right') {
    const content = [...reachable(main), ...(saveLane() ? reachable(saveLane()) : [])];
    focus(contentFocus && content.find(n => identity(n) === contentFocus.key) || primary()); return;
  }
```

Then replace

```js
  const modal = document.querySelector('dialog[open]');
  const items = reachable(modal || (inNav ? document.querySelector('.shell-nav') : main));
  const a = box(current), ax = a.x+a.width/2, ay = a.y+a.height/2;
  const viewport = (current.closest('dialog[open]') || main).getBoundingClientRect();
  if (!inNav && (a.bottom < viewport.top || a.top > viewport.bottom)) {
```

with

```js
  const modal = document.querySelector('dialog[open]');
  const lane = saveLane(), inLane = !modal && !!lane?.contains(current);
  // From the save bar, Up goes back into the page; Left and Right stay on the bar.
  const items = reachable(modal || (inNav ? document.querySelector('.shell-nav') : inLane && direction !== 'up' ? lane : main));
  const a = box(current), ax = a.x+a.width/2, ay = a.y+a.height/2;
  const viewport = (current.closest('dialog[open]') || main).getBoundingClientRect();
  if (!inNav && !inLane && (a.bottom < viewport.top || a.top > viewport.bottom)) {
```

Then replace

```js
  if (ranked[0]) focus(ranked[0].n);
  else if (!modal && !inNav && direction === 'left') focus(document.querySelector('.shell-nav a.current') || reachable(document.querySelector('.shell-nav'))[0]);
```

with

```js
  if (ranked[0]) focus(ranked[0].n);
  // Nothing further down the page: the save bar, while it is open, comes next (Save first).
  else if (!modal && !inNav && !inLane && direction === 'down' && lane && reachable(lane).length) focus(reachable(lane).find(n => n.matches('.primary')) || reachable(lane)[0]);
  else if (!modal && !inNav && direction === 'left') focus(document.querySelector('.shell-nav a.current') || reachable(document.querySelector('.shell-nav'))[0]);
```

**(e)** In `back()`, replace `if (main.contains(current)) {` with `if (inContent(current)) {`.

- [ ] **Step 6: Run the checks**

- `node Tools/check-save-lane.mjs`: PASS.
- The standing checks: all PASS. `check-look-browser` still sees `1 unsaved change … Window glass` and the inert lane.
- `node Tools/check-controller-navigation.mjs`: PASS. It exercises modal isolation and movement.

- [ ] **Step 7: Commit**

Append `await import('./check-save-lane.mjs');` as the last line of `Tools/run-browser-tests.mjs`.

```bash
git add crates/reborn-ui/ui/theme.css crates/reborn-ui/ui/savebar.js crates/reborn-ui/ui/app-shell.js crates/reborn-ui/ui/settings.html Tools/check-save-lane.mjs Tools/run-browser-tests.mjs
git commit -m "savebar: a lane for any page, failures said in the bar, reachable by pad

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The menu component

**Files:**
- Create: `crates/reborn-ui/ui/menu.js`
- Modify: `crates/reborn-ui/ui/theme.css` (append)
- Create: `Tools/check-menu.mjs`

**Interfaces:**
- Consumes: `el` from `shell.js`; the modal handling already in `app-shell.js`.
  - `dialog[open]` is the pad's scope.
  - B dispatches `cancel` and calls `close()`.
  - `primary()` prefers `[data-primary]`.
- Produces: `openMenu(anchor: HTMLElement, { label: string, options: {value: string, label: string}[], current: string|null, onPick: (value: string) => void }) → HTMLDialogElement`.
  - A modal `<dialog class="menu">` appended to `<body>`: the top layer, never inside the animating `#main`.
  - Items are `button.menu-item[role=menuitemradio]` with `aria-checked`. The current item is `data-primary` and is focused first.
  - Positioning:
    - It opens **above** the anchor, right-aligned 8px inside it (`data-side="above"`).
    - When there is no room above, it opens **below** (`data-side="below"`).
  - It closes on a pick, Esc, B, or a click outside. It removes itself and focuses the anchor again (or the element with the anchor's `id` if the page redrew).
  - It sets `aria-expanded` on the anchor. Pages give the anchor `aria-haspopup="menu"`.

- [ ] **Step 1: Write the failing check**

Create `Tools/check-menu.mjs`:

```js
// The menu (spec §3.4, §5.2): a glass list anchored to its button, a check on the current option,
// keyboard and pad inside it, and B / Esc / a click outside close it with focus back on the button.
//     node Tools/check-menu.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const html=`<!doctype html><html lang="en" data-glass="off" data-motion="quick"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css">
<style>#low{position:absolute;left:40px;bottom:40px}#high{position:absolute;right:40px;top:40px}</style></head>
<body class="shell themed page-test"><nav class="shell-nav"><a href="index.html">Play</a></nav>
<main id="main" tabindex="-1"><section id="screen"><button id="low" class="btn" aria-haspopup="menu">Graphics</button><button id="high" class="btn" aria-haspopup="menu">Display</button></section></main>
<footer class="input-footer"><span id="input-hints"></span></footer>
<script type="module">
import {openMenu} from './menu.js';
const options=[{value:'performance',label:'Performance'},{value:'balanced',label:'Balanced'},{value:'quality',label:'All-Out'}];
for(const id of ['low','high'])document.getElementById(id).addEventListener('click',e=>openMenu(e.currentTarget,{label:id,options,current:'balanced',onPick:v=>{window.picked=v;}}));
window.ready=true;
</script><script type="module" src="app-shell.js"></script></body></html>`;
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/menu.html'){res.setHeader('Content-Type','text/html');res.end(html);return;}
 const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
try{
 const page=await browser.newPage({viewport:{width:1280,height:800}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  window.pads=[{id:'Test pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:18},()=>({pressed:false,value:0}))}];
  Object.defineProperty(navigator,'getGamepads',{value:()=>pads});
  document.hasFocus=()=>true;
  window.__TAURI__={core:{invoke:async cmd=>cmd==='session_active'?false:null}};
  window.frames=[];window.clock=0;window.requestAnimationFrame=fn=>(frames.push(fn),frames.length);window.cancelAnimationFrame=()=>{};
  window.tick=()=>{clock+=16;frames.splice(0).forEach(fn=>fn(clock));};
 });
 await page.goto(`http://127.0.0.1:${server.address().port}/menu.html`);
 await page.waitForFunction(()=>window.ready&&frames.length>0);
 const press=b=>page.evaluate(b=>{pads[0].buttons[b]={pressed:true,value:1};tick();pads[0].buttons[b]={pressed:false,value:0};tick();},b);
 const menu=page.locator('dialog.menu[open]');
 const closed=()=>page.waitForFunction(()=>!document.querySelector('dialog.menu'));
 const active=()=>page.evaluate(()=>document.activeElement.dataset.value||document.activeElement.id);
 await page.locator('#low').click();await menu.waitFor();
 assert.equal(await page.evaluate(()=>document.querySelector('dialog.menu').parentElement===document.body),true,'mounted on body');
 assert.equal(await page.getByRole('menuitemradio').count(),3);
 assert.equal(await page.getByRole('menuitemradio',{name:'Balanced'}).getAttribute('aria-checked'),'true','a check on the current option');
 assert.equal(await page.locator('#low').getAttribute('aria-expanded'),'true');
 assert.equal(await active(),'balanced','focus starts on the current option');
 const geo=await page.evaluate(()=>{const m=document.querySelector('dialog.menu'),r=m.getBoundingClientRect(),a=document.getElementById('low').getBoundingClientRect();return {mb:r.bottom,at:a.top,side:m.dataset.side};});
 assert.ok(geo.mb<=geo.at&&geo.side==='above','opens above its button: '+JSON.stringify(geo));
 assert.equal(await menu.evaluate(n=>getComputedStyle(n).animationDuration),'0.18s','quick motion: --d2');
 await page.keyboard.press('ArrowDown');
 assert.equal(await active(),'quality','arrow keys move inside');
 await page.keyboard.press('Enter');await closed();
 assert.equal(await page.evaluate(()=>window.picked),'quality');
 assert.equal(await active(),'low','focus returns to the button');
 assert.equal(await page.locator('#low').getAttribute('aria-expanded'),'false');
 await page.locator('#low').click();await menu.waitFor();await page.keyboard.press('Escape');await closed();
 assert.equal(await active(),'low','Esc closes');
 await page.locator('#low').click();await menu.waitFor();await page.mouse.click(640,200);await closed();
 assert.equal(await page.evaluate(()=>window.picked),'quality','a click outside picks nothing');
 await page.locator('#high').click();await menu.waitFor();
 assert.equal(await page.evaluate(()=>document.querySelector('dialog.menu').dataset.side),'below','no room above: opens below');
 await page.keyboard.press('Escape');await closed();
 await page.evaluate(()=>document.documentElement.dataset.motion='slow');
 await page.locator('#low').click();await menu.waitFor();
 assert.equal(await menu.evaluate(n=>getComputedStyle(n).animationDuration),'0.72s','slow motion: --d2');
 await page.keyboard.press('Escape');await closed();
 await page.locator('#low').click();await menu.waitFor();
 await press(13);
 assert.equal(await active(),'quality','D-pad moves inside the menu');
 await press(1);await closed();
 assert.equal(await active(),'low','B closes it and focus returns');
 await page.emulateMedia({reducedMotion:'reduce'});
 await page.locator('#low').click();await menu.waitFor();
 assert.equal(await menu.evaluate(n=>getComputedStyle(n).animationName),'none','reduced motion: no animation');
 assert.deepEqual(errors,[]);
 console.log('check-menu: PASS');
}finally{await browser.close();server.close();}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `node Tools/check-menu.mjs`
Expected: FAIL. The page errors with a failed module import of `./menu.js` (404), and `window.ready` never becomes true.

- [ ] **Step 3: Write `menu.js`**

Create `crates/reborn-ui/ui/menu.js`:

```js
// One anchored menu (spec §3.4, §5.2): a glass list with a check on the current option.
// A modal <dialog> on <body>, so it sits in the top layer: never clipped by a panel, never anchored to
// #main while #main animates in, and the shell's pad navigation treats it as its own scope (the D-pad
// moves inside it, B closes it). Opened by a click; nothing here runs on page load.
import { el } from "./shell.js";

let openNow = null;

export function openMenu(anchor, { label, options, current = null, onPick }) {
  openNow?.close();
  let menu = null;
  const items = options.map((o) => el("button", {
    type: "button", class: "menu-item", role: "menuitemradio", "aria-checked": String(o.value === current), "data-value": o.value,
    ...(o.value === current ? { "data-primary": "true" } : {}),
    onclick: () => { menu.close(); onPick(o.value); },
  }, el("span", { text: o.label }), el("span", { class: "menu-check", "aria-hidden": "true", text: "\u2713" })));
  menu = el("dialog", { class: "menu", "aria-label": label }, el("div", { class: "menu-list", role: "menu", "aria-label": label }, ...items));
  menu.addEventListener("keydown", (e) => {
    const i = items.indexOf(document.activeElement);
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (step) { e.preventDefault(); items[(i + step + items.length) % items.length].focus(); }
    else if (e.key === "Home" || e.key === "End") { e.preventDefault(); items[e.key === "Home" ? 0 : items.length - 1].focus(); }
  });
  // A click on the backdrop, outside the list, closes it.
  menu.addEventListener("click", (e) => {
    if (e.target !== menu) return;
    const r = menu.getBoundingClientRect();
    if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) menu.close();
  });
  menu.addEventListener("close", () => {
    menu.remove();
    if (openNow === menu) openNow = null;
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
  (items.find((n) => n.dataset.value === String(current)) || items[0])?.focus();
  openNow = menu;
  return menu;
}

// Above the button, right-aligned 8px inside it (the mockup's strip sits at the bottom of Play);
// below it when there is no room above; always inside the window.
function place(anchor, menu) {
  const a = anchor.getBoundingClientRect(), m = menu.getBoundingClientRect(), gap = 8;
  const above = a.top - m.height - gap >= gap;
  const top = above ? a.top - m.height - gap : Math.min(innerHeight - m.height - gap, a.bottom + gap);
  const left = Math.max(gap, Math.min(innerWidth - m.width - gap, a.right - m.width - gap));
  menu.style.setProperty("--menu-x", `${Math.round(left)}px`);
  menu.style.setProperty("--menu-y", `${Math.round(top)}px`);
  menu.dataset.side = above ? "above" : "below";
}
```

- [ ] **Step 4: The menu styles**

Append to `theme.css`:

```css
/* menu (menu.js): an anchored glass list, a modal <dialog> in the top layer (spec §3.4); fades and settles 4px from its anchor (§5.2) */
dialog.menu{position:fixed;inset:auto;left:var(--menu-x,0px);top:var(--menu-y,0px);margin:0;width:200px;max-width:calc(100vw - 16px);padding:6px;border-radius:var(--r-lg);border:1px solid var(--stroke-hi);color:var(--fg);box-shadow:var(--sheen),var(--shadow);transform-origin:bottom right;animation:menu-in var(--d2) var(--ease)}
dialog.menu[data-side="below"]{transform-origin:top right;animation-name:menu-in-below}
dialog.menu::backdrop{background:transparent}
@keyframes menu-in{from{opacity:0;transform:translateY(4px) scale(.98)}}
@keyframes menu-in-below{from{opacity:0;transform:translateY(-4px) scale(.98)}}
dialog.menu .menu-list{display:flex;flex-direction:column;gap:2px}
dialog.menu .menu-item{display:flex;align-items:center;justify-content:space-between;width:100%;height:36px;padding:0 12px;border:0;border-radius:var(--r-sm);background:transparent;color:var(--fg-2);font:500 13px Inter,system-ui,sans-serif;text-align:left;cursor:pointer}
dialog.menu .menu-item:hover{background:var(--glass-2);color:var(--fg)}
dialog.menu .menu-item[aria-checked="true"]{color:var(--fg)}
dialog.menu .menu-check{visibility:hidden}
dialog.menu .menu-item[aria-checked="true"] .menu-check{visibility:visible}
```

The surface colour and blur come from the existing floating-surfaces rule, which already names `dialog`.

- [ ] **Step 5: Run the checks**

- `node Tools/check-menu.mjs`: PASS.
- The standing checks: PASS.

- [ ] **Step 6: Commit**

Append `await import('./check-menu.mjs');` to the end of `Tools/run-browser-tests.mjs`.

```bash
git add crates/reborn-ui/ui/menu.js crates/reborn-ui/ui/theme.css Tools/check-menu.mjs Tools/run-browser-tests.mjs
git commit -m "ui: anchored glass menu, top layer, pad and keyboard inside, focus returns

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Play on the design system

This task delivers the account block, the actions, the status line, the notes, the strip (with an interim Graphics link and Display toggle) and Game details. It does **not** deliver the menus.

**Files:**
- Modify: `crates/reborn-ui/ui/index.html`
- Create: `crates/reborn-ui/ui/pages/play.css`
- Modify: `crates/reborn-ui/ui/home.js`
- Modify: `crates/reborn-ui/ui/shell.js` (add `icon`)
- Modify: `Tools/check-theme.mjs` (`THEMED.index`)
- Modify: `Tools/check-shell-pages.mjs`, `Tools/run-browser-tests.mjs`, `Tools/check-session-state.mjs` (expectations that change on purpose)

**Interfaces:**
- Consumes:
  - Task 1 tokens (`--hero-scrim`, `--on-art`), `.avatar`, `@keyframes live-pulse`, `look-enter`;
  - `body.themed`;
  - `.btn`, `.btn.primary`, `.btn.sm`, `.glass`.
- Produces:
  - `export function icon(paths: string|string[], { fill = false } = {}) → SVGElement|null` in `shell.js`. It returns null where there is no real DOM.
  - DOM ids and classes that Task 5 and the checkers use:
    - `#play-now`, `#game-details-open`, `#game-details`;
    - `.play-status`, `.play-state-title`, `.play-status-action`;
    - `.play-account`, `.play-account-chip[aria-label="Signed in as <name>"]`;
    - `.play-notes`, `.play-controller-setup`, `#play-anyway`;
    - `.play-strip`, `.play-cell`, `#play-graphics`, `#play-display`;
    - `.hero-art`, `.play-wordmark img`.
  - In `home.js`: `strip(inSession)`, `cell(tag, attrs, key, value)`, `pickDisplay(value: "true"|"false")`, and `let notice` (one line shown as a note).

- [ ] **Step 1: Update the expectations that change on purpose (they fail now)**

`Tools/check-theme.mjs`: change `const THEMED = {};` to `const THEMED = { index: "pages/play.css" };`.

`Tools/check-shell-pages.mjs`:
- Replace `check("Play shows who is signed in", t.includes("Signed in as Adam"));` with:
  ```js
  // The account block (spec §8.1): avatar, name and playtime; "Signed in as" is its accessible name.
  check("Play shows who is signed in", !!findByAttr(screen, "aria-label", "Signed in as Adam") && t.includes("12 min played"), t.slice(0, 300));
  ```
- Replace `check("A finished sign-in is shown", t.includes("Signed in as Adam"), t.slice(0, 300));` with:
  ```js
  check("A finished sign-in is shown", !!findByAttr(page.screen(), "aria-label", "Signed in as Adam"), t.slice(0, 300));
  ```

`Tools/check-session-state.mjs`: replace both `{name:'Session active',exact:true}` with `{name:'Playing',exact:true}`. The spec's in-session button reads "Playing".

`Tools/run-browser-tests.mjs`:
- Replace the two lines
  ```js
  await page.getByRole('button',{name:'Session active'}).waitFor();
  assert.equal(await page.getByRole('button',{name:'Session active'}).isDisabled(),true);
  ```
  with the same two lines using `'Playing'`.
- Replace
  ```js
  await page.getByRole('alert').filter({hasText:'The emulator could not start.'}).waitFor();
  ```
  with
  ```js
  // The status line says it could not start; "See why" opens the reason in Game details (spec §8.1).
  await page.getByRole('alert').filter({hasText:'Your game could not start'}).waitFor();
  await page.getByRole('button',{name:'See why',exact:true}).click();
  await page.locator('#game-details[open]').getByText('The emulator could not start.').waitFor();
  await page.keyboard.press('Escape');
  ```

Run:
- `node Tools/check-theme.mjs`. Expected: FAIL (`index: must load exactly theme.css + pages/play.css`).
- `node Tools/check-shell-pages.mjs`. Expected: FAIL (`Play shows who is signed in`).

- [ ] **Step 2: `icon()` in `shell.js`**

Add after `export function partial(text) { … }`:

```js
// A small inline icon (24×24 viewBox), drawn in the SVG namespace, which el() cannot do.
// Null where there is no real DOM (the page checkers' stand-in); el() skips null children.
export function icon(paths, { fill = false } = {}) {
  if (typeof document.createElementNS !== "function") return null;
  const ns = "http://www.w3.org/2000/svg", svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  if (fill) svg.setAttribute("fill", "currentColor");
  else {
    svg.setAttribute("fill", "none"); svg.setAttribute("stroke", "currentColor"); svg.setAttribute("stroke-width", "2");
    svg.setAttribute("stroke-linecap", "round"); svg.setAttribute("stroke-linejoin", "round");
  }
  for (const d of [].concat(paths)) { const p = document.createElementNS(ns, "path"); p.setAttribute("d", d); svg.append(p); }
  return svg;
}
```

- [ ] **Step 3: `index.html` becomes theme-only**

Make these changes and keep everything else byte-identical. In particular, the whole `<nav class="shell-nav">…</nav>` block stays exactly as it is.

- **`<head>`:** replace the two `<link rel="stylesheet" …>` lines with `<link rel="stylesheet" href="theme.css"><link rel="stylesheet" href="pages/play.css">`.
- **`<body class="shell ob page-home">`** becomes `<body class="shell themed page-home">`.
- **Delete:**
  - `<div class="backdrop" aria-hidden="true"></div>`;
  - `<div class="page-heading">…</div>` (the wordmark is the page's heading);
  - `<div id="play-notices" aria-label="Updates"></div>` (unused: updates live in `#update-center`).
- **Header:** replace the `<header class="app-header">…</header>` line with `<header class="app-header"><span id="bridge" class="pill">Connecting</span></header>`. It is hidden by the shell, and `home.js` still writes the pill.
- **Footer:** replace the `<footer class="input-footer">…</footer>` line with `<footer class="input-footer"><span id="input-hints"></span></footer>`.

- [ ] **Step 4: `pages/play.css`**

Create `crates/reborn-ui/ui/pages/play.css`:

```css
/* Play (spec §8.1; mockup "PLAY"). Layout only: every colour, radius and duration comes from theme.css.
   The art covers the page; the launch group sits bottom-left over it; the strip runs along the bottom. */
body.page-home #main{padding:0;overflow-x:hidden;overflow-y:auto}
body.page-home #screen{position:relative;display:flex;flex-direction:column;min-height:100%;isolation:isolate}
.page-home .loading{padding:24px 42px}
.page-home #menu-pad-setup{position:absolute;top:24px;left:42px;z-index:2}

/* art: full bleed, never moves; the scrim keeps the words readable */
.page-home .play-scene{position:absolute;inset:0;z-index:-1;overflow:hidden}
.page-home .hero-art{position:absolute;inset:0}
.page-home .hero-art img{display:block;width:100%;height:100%;object-fit:cover;object-position:62% 18%}
.page-home .play-scene::after{content:"";position:absolute;inset:0;background:var(--hero-scrim);pointer-events:none}

/* account, top right */
.page-home .play-account{position:absolute;top:24px;right:24px;z-index:1;display:flex;flex-direction:column;align-items:flex-end;gap:8px;max-width:min(340px,45%)}
.page-home .play-account-chip{display:flex;align-items:center;gap:10px;min-width:0;padding:6px 14px 6px 6px;border-radius:999px;background:var(--on-art);border:1px solid var(--stroke);backdrop-filter:blur(16px);-webkit-backdrop-filter:blur(16px);color:var(--fg);text-decoration:none}
.page-home .play-account-text{display:flex;flex-direction:column;min-width:0}
.page-home .play-account-text .nm{font-size:13px;font-weight:600;line-height:1.2;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.page-home .play-account-text .pt{font-size:11.5px;color:var(--fg-2)}
.page-home .play-account-note{font-size:12px;color:var(--fg-2);text-align:right;overflow-wrap:anywhere}
.page-home .play-account-card{display:flex;flex-direction:column;align-items:flex-start;gap:8px;padding:16px;font-size:12.5px;color:var(--fg-2);overflow-wrap:anywhere}
.page-home .play-account-card p:first-child{color:var(--fg)}

/* the launch group, bottom left */
.page-home .play-content{position:relative;flex:1;display:flex;flex-direction:column;justify-content:flex-end;min-height:0;padding:96px 42px clamp(24px,10vh,96px)}
.page-home .play-wordmark{margin:0;line-height:0;font-size:0}
.page-home .play-wordmark img{display:block;width:470px;max-width:100%;height:auto}
.page-home .play-actions{display:flex;align-items:center;gap:8px;margin-top:24px}
.page-home .play-actions .play{height:52px;min-width:250px;padding:0 40px;font:700 18px Inter,system-ui,sans-serif;letter-spacing:.01em}
.page-home .play-actions .play svg{width:16px;height:16px}
.page-home .play-more{width:52px;height:52px;padding:0;color:var(--fg-2)}
.page-home .play-more:hover{color:var(--fg)}
.page-home .play-more svg{width:18px;height:18px}
.page-home .play-status{display:flex;align-items:center;gap:8px;min-height:20px;margin-top:16px;font-size:13px;font-weight:500;color:var(--fg)}
.page-home .play-status .dot{width:8px;height:8px;border-radius:50%;flex:none;background:var(--ok)}
.page-home .play-status.is-setup .dot{background:var(--warn)}
.page-home .play-status.is-error .dot{background:var(--red)}
.page-home .play-status.is-live .dot{animation:live-pulse 1.6s ease-in-out infinite}
.page-home .play-status-action{margin-left:4px;padding:0;border:0;background:none;color:var(--fg-2);font:inherit;text-decoration:underline;text-underline-offset:3px;cursor:pointer}
.page-home .play-status-action:hover{color:var(--fg)}
.page-home .play-notes{display:flex;flex-direction:column;gap:8px;margin-top:16px;max-width:480px}
.page-home .play-notes:empty{display:none}
.page-home .play-note{display:flex;align-items:center;gap:16px;padding:12px 16px;font-size:12.5px;color:var(--fg-2)}
.page-home .play-note>span:not(.dot){flex:1;min-width:0}
.page-home .play-note .dot{width:8px;height:8px;border-radius:50%;flex:none;background:var(--warn)}
.page-home .play-note a{color:var(--fg);font-weight:600;text-underline-offset:3px;white-space:nowrap}
.page-home .play-controller-setup{flex-direction:column;align-items:flex-start;gap:8px}
.page-home .play-controller-setup strong{font-size:14px;color:var(--fg)}
.page-home .play-controller-setup-actions{display:flex;gap:8px;flex-wrap:wrap}

/* the strip: one glass panel, three cells edge to edge; hover fills the whole cell; the panel's radius clips the outer corners */
.page-home .play-strip{flex:none;display:grid;grid-template-columns:repeat(3,minmax(0,1fr));height:78px;margin:0 42px 24px;padding:0;overflow:hidden}
.page-home .play-cell{display:flex;align-items:center;justify-content:space-between;gap:8px;min-width:0;height:100%;padding:0 18px;border:0;border-radius:0;background:transparent;color:var(--fg);font:inherit;text-align:left;text-decoration:none;cursor:pointer}
.page-home .play-cell+.play-cell{border-left:1px solid var(--stroke)}
.page-home .play-cell:hover:not(:disabled){background:var(--glass-1)}
.page-home .play-cell:disabled{cursor:default;opacity:.55}
.page-home .play-cell>span:first-child{min-width:0}
.page-home .play-cell .k{display:block;font-size:11.5px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--fg-3)}
.page-home .play-cell .v{display:block;margin-top:3px;font-size:15px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.page-home .play-cell svg{width:16px;height:16px;flex:none;color:var(--fg-3)}
/* the panel clips its cells, so the focus ring sits inside the cell */
body.shell.page-home .play-strip .play-cell:focus-visible,body.gamepad-input.page-home .play-strip .play-cell.pad-focus{outline-offset:-4px!important}

/* content rises in; the art only fades with #main (theme.css) */
.page-home .play-content,.page-home .play-strip,.page-home .play-account{animation:look-enter var(--d3) var(--ease) backwards}

/* Game details (the ⋯ button) */
.page-home .play-dialog{width:min(540px,calc(100vw - 48px));max-height:calc(100vh - 64px);overflow:auto}
.page-home .play-dialog-heading{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:16px}
.page-home .play-dialog h2{font-size:20px;font-weight:600;letter-spacing:-.02em}
.page-home .play-detail-links{margin-bottom:16px;border:1px solid var(--stroke);border-radius:var(--r-md);overflow:hidden}
.page-home .play-detail-links a{display:flex;align-items:center;gap:16px;min-height:56px;padding:0 16px;color:var(--fg);font-size:13px;text-decoration:none}
.page-home .play-detail-links a+a{border-top:1px solid var(--stroke)}
.page-home .play-detail-links a:hover{background:var(--glass-1)}
.page-home .play-detail-value{margin-left:auto;color:var(--fg-2);font-size:12px}
.page-home .play-detail-message{margin:0 0 8px;font-size:12.5px;line-height:1.6;color:var(--warn);overflow-wrap:anywhere}
.page-home .play-copy{margin-bottom:16px}
.page-home .play-copy-box{position:fixed;left:-9999px;top:0}
.page-home .play-help{display:inline-block;margin:0 0 16px;font-size:12.5px;color:var(--fg-2);text-underline-offset:4px}
.page-home .play-files summary{padding:8px 0;color:var(--fg-2);font-size:12.5px;cursor:pointer}
.page-home .play-files .block h2{display:none}
.page-home .play-dialog-actions{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-top:16px;padding-top:16px;border-top:1px solid var(--stroke)}
.page-home .play-dialog-actions a{color:var(--fg-2);font-size:12.5px;text-underline-offset:4px}

@media (max-width:900px),(max-height:650px){
  .page-home .play-content{padding:88px 28px 24px}
  .page-home .play-strip{height:64px;margin:0 28px 16px}
  .page-home .play-account{top:16px;right:16px}
}
```

- [ ] **Step 5: `home.js`: account, actions, status, notes, strip**

Make these edits. Every function not named here stays exactly as it is.

**Import line.** Replace the first `import` with:

```js
import { bridge, call, el, fact, facts, get, icon, markCurrent, minutes, reason, wireCoreFold } from "./shell.js";
```

**State.** After `let problem = null;` add:

```js
// One line about a strip change that did not apply (Display, Graphics). Never the red "could not
// start": that state is for the game itself.
let notice = null;
```

**PRESETS.** Replace the `const PRESETS = [ … ];` block with:

```js
// Names only (spec §8.1): the ReShade text they used to carry is stale since 3.0.20.
const PRESETS = [
  ["performance", "Performance"],
  ["balanced", "Balanced"],
  ["quality", "All-Out"],
];
```

**Icons.** Add after it:

```js
const GLYPH_PLAY = "M7 4l13 8-13 8z";
const GLYPH_MORE = "M5 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm7 0a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm7 0a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z";
const CHEVRON = "M9 6l6 6-6 6";
```

**Account.** Delete `function identityCard() { … }` (all of it, with its comments) and add in its place:

```js
// Top right (spec §8.1): avatar, name and playtime, in place of "Signed in as …". identity_status has no
// avatar picture, so the avatar is the name's first letter on the avatar colours (spec §3.4).
function accountBlock() {
  const id = state.identity;
  const box = el("div", { class: "play-account", role: "group", "aria-label": "Account" });
  if (signIn.active) {
    // The address, in every case, not only when the browser did not open: it can open on another
    // monitor, behind this window, or in a profile not signed into Discord.
    box.append(el("div", { class: "glass play-account-card", role: "status" },
      el("p", { text: `Waiting for Discord. Your code is ${signIn.userCode}.` }),
      signIn.verifyUrl ? el("p", { text: `Go to ${signIn.verifyUrl} to approve it.` }) : null,
      el("button", { type: "button", class: "btn sm", text: "Cancel", onclick: () => { stopSignIn(); draw(); } })));
  } else if (ok(id) && id.signed_in) {
    const name = id.display_name || "Player";
    const played = ok(state.home) && state.home.sessions_played ? `${minutes(state.home.minutes_played)} played` : "No games played yet";
    box.append(el("a", { class: "play-account-chip", href: "stats.html", "aria-label": `Signed in as ${name}` },
      el("span", { class: "avatar", "aria-hidden": "true", text: name.slice(0, 1).toUpperCase() }),
      el("span", { class: "play-account-text" }, el("span", { class: "nm", text: name }), el("span", { class: "pt", text: played }))));
  } else {
    box.append(el("button", { type: "button", class: "btn sm", text: "Sign in with Discord", disabled: busy, onclick: startSignIn }));
    // Never `id.error`: a failed read hands back the command's own name or a vault's wording.
    box.append(el("p", { class: "play-account-note", text: ok(id) ? "Sign in with Discord to track your stats from your games." : ACCOUNT_UNREADABLE }));
  }
  // In every state: the note written for a signed-out player (their game is waiting for sign-in).
  if (result?.stats_note) box.append(el("p", { class: "play-account-note", text: result.stats_note }));
  if (signIn.message) box.append(el("p", { class: "play-account-note", text: signIn.message }));
  return box;
}
```

**Facts.** In `factsBlock()`, after the line `if (mods.note) rows.push(fact("", mods.note));` add:

```js
  // The player's own test for a right pack is "can I see stuff, and what version is it": read off the
  // folder the launch will use. It moved here from the Play screen (decluttered, spec §8.1).
  const pack = ok(check) ? check.pack : null;
  rows.push(fact("Texture pack", pack ? (pack.reachable ? (pack.version || "loaded") : "not loading") : null));
```

**Game details buttons.** In `detailsDialog()`:
- the Close button gets `class: "btn sm"`;
- the Copy error button's `class: "play-copy"` becomes `class: "btn sm play-copy"`;
- the "Open Dolphin settings" button gets `class: "btn"`.

Nothing else in `detailsDialog()` changes.

**Draw.** Replace the whole `function draw() { … }` with the following. It also adds `playStatus`, `notes`, `strip`, `cell` and `pickDisplay`.

```js
// The one line under the button (spec §8.1): five states, in the words Play has always used. A setup
// that is not finished is "Setup required", never "could not start"; the reason is in Game details.
function playStatus(ready, inSession, error) {
  const crashed = !inSession && !error && result?.needs_recovery;
  const [tone, text] = inSession ? ["live", "Enjoy the game"]
    : error ? ["error", "Your game could not start"]
    : crashed ? ["error", "Game closed unexpectedly"]
    : !ready ? ["setup", "Setup required"]
    : ["ready", "Ready to play"];
  const action = tone === "setup" ? el("a", { class: "play-status-action", href: "onboarding.html", text: "Finish setup" })
    : tone === "error" ? el("button", { type: "button", class: "play-status-action", text: "See why", onclick: openDetails })
    : null;
  return el("div", { class: `play-status is-${tone}`, role: tone === "error" ? "alert" : "status" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("span", { class: "play-state-title", text }), action);
}

// Things the player must act on before, or instead of, playing. Quiet otherwise (empty = hidden).
function notes() {
  const box = el("div", { class: "play-notes" });
  // Play was NOT started because player one's pad cannot press Start or A. Not a failure: a setup to offer.
  if (controllerSetup) box.append(el("div", { class: "glass play-note play-controller-setup", role: "alert" },
    el("strong", { text: "Your controller needs a one-time setup" }),
    el("p", { text: controllerSetup.message }),
    el("div", { class: "play-controller-setup-actions" },
      el("a", { class: "btn primary sm", href: "controls.html", text: "Set up controller" }),
      el("button", { type: "button", id: "play-anyway", class: "btn sm", disabled: busy, text: "Play anyway", onclick: () => { playAnyway = true; play(); } }))));
  // Tier 2 of the game-image law: a disc this launcher could not verify as a Reborn build. Always a
  // fault to repair, never a preference.
  if (state.check?.game_verdict === "unconverted") box.append(el("div", { class: "glass play-note play-warning", role: "status" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("span", { text: "Not a Reborn build" }),
    el("a", { class: "play-warning-repair", href: "settings.html#files", text: "Repair" })));
  if (state.check?.pack && !state.check.pack.reachable) box.append(el("div", { class: "glass play-note play-warning", role: "status" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("span", { text: "Texture pack not loading" })));
  if (notice) box.append(el("div", { class: "glass play-note", role: "status" }, el("span", { text: notice })));
  return box;
}

function cell(tag, attrs, key, value) {
  return el(tag, { class: "play-cell", ...attrs },
    el("span", {}, el("span", { class: "k", text: key }), el("span", { class: "v", text: value })),
    icon(CHEVRON));
}

// The strip (spec §8.1): Controllers, Graphics, Display; no checkboxes or switches in it.
function strip(inSession) {
  const pads = ok(state.devices) && Array.isArray(state.devices.pads) ? state.devices.pads : null;
  const dolphin = ok(state.dolphin) ? state.dolphin : null;
  const fullscreen = String(dolphin?.values?.fullscreen ?? "True").toLowerCase() === "true";
  const displayLocked = busy || inSession || !dolphin || dolphin.editable === false;
  return el("nav", { class: "glass play-strip", "aria-label": "Game preferences" },
    cell("a", { href: "controls.html" }, "Controllers", pads ? pads.length ? `${pads.length} connected` : "Connect a controller" : "Manage devices"),
    cell("a", { href: "settings.html#graphics", id: "play-graphics" }, "Graphics", graphicsLabel()),
    cell("button", { type: "button", id: "play-display", disabled: displayLocked, onclick: () => pickDisplay(String(!fullscreen)) }, "Display", fullscreen ? "Fullscreen" : "Windowed"));
}

async function pickDisplay(value) {
  const dolphin = ok(state.dolphin) ? state.dolphin : null;
  if (!dolphin || busy) return;
  notice = null;
  try {
    await call("dolphin_preference_set", { runtime: get("runtime"), key: "fullscreen", value });
    dolphin.values = { ...(dolphin.values || {}), fullscreen: value };
  } catch (e) { notice = `Display: ${reason(e)}`; }
  draw();
}

function draw() {
  if (typeof state.active === "boolean") externalSession = state.active;
  const screen = document.getElementById("screen");
  // Background refreshes must not close an open dialog or discard its focus.
  const oldDialog = document.getElementById("game-details");
  const wasOpen = !!oldDialog?.open;
  const filesOpen = oldDialog?.querySelector?.("details")?.open;
  const focusedId = document.activeElement?.id;
  const ready = ok(state.check) && state.check.ready;
  const inSession = playing || externalSession;
  const scene = el("section", { class: "play-scene", "aria-hidden": "true" },
    el("div", { class: "hero-art" }, el("img", { src: "art/play-background.png", alt: "" })));
  // The artwork element itself survives every redraw, so it never flickers or moves.
  const art = screen.querySelector?.(".hero-art");
  if (art) scene.querySelector(".hero-art").replaceWith(art);
  const error = problem || state.check?.error || null;
  const actions = el("div", { class: "play-actions" });
  if (ready || inSession) actions.append(el("button", { type: "button", id: "play-now", "data-primary": "true", class: "btn primary play", disabled: busy || inSession, onclick: play },
    icon(GLYPH_PLAY, { fill: true }), el("span", { text: inSession ? "Playing" : busy ? "Opening…" : "Play now" })));
  else actions.append(el("a", { href: "onboarding.html", class: "btn primary play", "data-primary": "true", text: "Set up your game" }));
  actions.append(el("button", { type: "button", id: "game-details-open", class: "btn play-more", title: "Game details", "aria-label": "Game details", "aria-haspopup": "dialog", onclick: openDetails },
    icon(GLYPH_MORE, { fill: true })));
  const content = el("div", { class: "play-content" },
    el("h1", { class: "play-wordmark" }, el("img", { src: "art/play-logo.png", alt: "NFL Street Reborn" })),
    actions, playStatus(ready, inSession, error), notes());
  const dialog = detailsDialog();
  screen.replaceChildren(scene, accountBlock(), content, strip(inSession), dialog);
  if (wasOpen) { dialog.showModal(); if (filesOpen) dialog.querySelector("details").open = true; }
  else if (focusedId) { const target = document.getElementById(focusedId); if (target && !target.disabled) target.focus({ preventScroll: true }); }
}
```

Also, in `play()`, add `notice = null;` right after `problem = null;`.

- [ ] **Step 6: Run the checks**

- `node Tools/check-theme.mjs` and `node Tools/check-theme-tokens.mjs`: PASS (1 page stylesheet).
- `node Tools/check-shell-pages.mjs`: PASS. Play draws in all three modes, facts are named, the sign-in flow is correct.
- `node Tools/check-play-controller-gate.mjs`: PASS. It uses `.play-controller-setup`, `#play-anyway` and `.play-state-title`.
- `node Tools/check-session-state.mjs`: PASS.
- The rest of the standing checks: PASS. `run-browser-tests` saves `target/browser-review/reborn-launcher-home.png` and `reborn-launcher-800.png`.

Open both screenshots and compare them with the mockup's PLAY section. Check:
- the wordmark is 470px wide with its own shape;
- the account sits top right;
- the 52px Play now has a square ⋯ beside it;
- the status line is alone under them;
- one glass strip of three cells runs along the bottom;
- there is no hint bar.

- [ ] **Step 7: Commit**

```bash
git add crates/reborn-ui/ui/index.html crates/reborn-ui/ui/pages/play.css crates/reborn-ui/ui/home.js crates/reborn-ui/ui/shell.js Tools/check-theme.mjs Tools/check-shell-pages.mjs Tools/check-session-state.mjs Tools/run-browser-tests.mjs
git commit -m "play: theme-only page; account top right, Play now + game details, five-state status, glass strip

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Play's Graphics and Display menus, and the Play check

**Files:**
- Modify: `crates/reborn-ui/ui/home.js` (`strip()`, add `pickGraphics`)
- Create: `Tools/check-play-page.mjs`
- Modify: `Tools/check-launch-polish.mjs` (its Play step used the fullscreen checkbox)

**Interfaces:**
- Consumes:
  - `openMenu` (Task 3);
  - `strip`, `cell`, `pickDisplay`, `notice`, `PRESETS`, `preset()`, `graphicsLabel()` (Task 4);
  - `set` from `shell.js`.
- Produces:
  - `#play-graphics` and `#play-display` as `button[aria-haspopup=menu]`.
  - Graphics writes `setup_save_graphics_options({mode:"preset",resolution:3,msaa:1,ssaa:false})`, then `set("preset", value)`. This is the same call Settings has always made.
  - Display writes `dolphin_preference_set({runtime, key:"fullscreen", value:"true"|"false"})`.

- [ ] **Step 1: Write the failing check**

Create `Tools/check-play-page.mjs`:

```js
// Play (spec §8.1): the five status states, the account block, the ⋯ Game details, and the strip's
// Graphics and Display menus, against a stubbed core; also the minimum window.
//     node Tools/check-play-page.mjs
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';

const root = path.resolve(import.meta.dirname, '..'), ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.join(root, 'target/glass-review'); mkdirSync(out, { recursive: true });
const source = readFileSync(path.join(root, 'Tools/check-shell-pages.mjs'), 'utf8');
const FULL = vm.runInNewContext(source.slice(source.indexOf('const FULL ='), source.indexOf('// Every field null')) + '\nFULL');
Object.assign(FULL, {
  session_active: false, netplay_pending_join: null, ui_notices: [],
  onboarding_progress: { complete: true, next: { kind: 'already_complete' }, summary: { graphics: 'balanced' } },
  setup_graphics_options: { current: null, previous: null },
  dolphin_preferences: { values: { fullscreen: 'True' }, editable: true, live_editable: [] },
  update_policy: { automatic: false }, update_check: { state: 'current' }, update_activity: { downloading: false, ready: null },
  content_status: { phase: 'up_to_date', components: [] },
  launcher_look: { windowGlass: false, slowAnimations: false, glassSupported: true, glassApplied: false, glassNote: null },
});
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
async function open(variant = {}, viewport = { width: 1280, height: 800 }) {
  const ctx = await browser.newContext({ viewport });
  await ctx.addInitScript(({ data, variant }) => {
    Object.assign(data, variant);
    window.calls = []; window.launchAnswer = null;
    window.__TAURI__ = { event: { listen: async () => () => {} }, core: { invoke: async (cmd, args = {}) => {
      window.calls.push({ cmd, args: JSON.parse(JSON.stringify(args)) });
      if (cmd === 'launch') return window.launchAnswer || new Promise((resolve) => { window.finishLaunch = resolve; });
      if (cmd === 'window_glass_apply') return data.launcher_look;
      if (cmd === 'dolphin_preference_set') { data.dolphin_preferences.values[args.key] = args.value; return null; }
      if (cmd in data) return data[cmd];
      return null;
    } } };
  }, { data: JSON.parse(JSON.stringify(FULL)), variant });
  const page = await ctx.newPage(); page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(base + '/index.html');
  return page;
}
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height, r: r.right, b: r.bottom }; });
const overlap = (a, b) => a.x < b.r && b.x < a.r && a.y < b.b && b.y < a.b;
try {
  { // Ready, at 1280×800
    const page = await open();
    await page.getByRole('button', { name: 'Play now', exact: true }).waitFor();
    assert.equal(await page.locator('.play-status .play-state-title').innerText(), 'Ready to play');
    assert.equal(await page.locator('.play-status .play-status-action').count(), 0, 'no action when ready');
    const chip = page.getByRole('link', { name: 'Signed in as Adam' });
    assert.match(await chip.innerText(), /Adam[\s\S]*12 min played/);
    assert.doesNotMatch(await page.locator('#screen').innerText(), /Signed in as/, 'no "Signed in as" text');
    const main = await rect(page, '#main'), acct = await rect(page, '.play-account-chip');
    assert.ok(Math.abs(main.r - acct.r - 24) <= 2 && Math.abs(acct.y - main.y - 24) <= 2, 'account top right: ' + JSON.stringify({ main, acct }));
    const play = await rect(page, '#play-now'), more = await rect(page, '#game-details-open');
    assert.equal(Math.round(play.h), 52); assert.ok(play.w >= 250, 'Play now is at least 250px');
    assert.deepEqual([Math.round(more.w), Math.round(more.h)], [52, 52], '⋯ is 52×52');
    assert.ok(Math.abs(play.y - more.y) < 1, '⋯ sits beside Play now');
    const status = await rect(page, '.play-status');
    assert.ok(status.y >= play.b + 8 && Math.abs(status.x - play.x) < 2, 'the status line sits alone under the button');
    const logo = await rect(page, '.play-wordmark img');
    assert.ok(logo.w <= 470.5 && Math.abs(logo.h / logo.w - 1110 / 2834) < 0.01, 'wordmark keeps its own shape: ' + JSON.stringify(logo));
    const strip = await rect(page, '.play-strip');
    const cells = await page.locator('.play-strip .play-cell').evaluateAll((ns) => ns.map((n) => { const r = n.getBoundingClientRect(); return { x: r.x, w: r.width, h: r.height }; }));
    assert.equal(cells.length, 3);
    assert.ok(cells.every((c) => Math.abs(c.w - cells[0].w) <= 1 && Math.abs(c.h - (strip.h - 2)) <= 1), 'three equal cells, full height: ' + JSON.stringify(cells));
    assert.ok(Math.abs(cells[0].x - strip.x) <= 1.5, 'cells run edge to edge');
    assert.equal(await page.locator('.input-footer').evaluate((n) => getComputedStyle(n).display), 'none', 'no hint bar for a mouse user');
    await page.screenshot({ path: path.join(out, 'play-ready-1280.png') });

    // Graphics menu: opens above its cell, check on the current preset, a pick saves the preset.
    await page.locator('#play-graphics').click();
    await page.locator('dialog.menu[open]').waitFor();
    assert.equal(await page.evaluate(() => document.querySelector('dialog.menu').parentElement === document.body), true, 'menus mount on body');
    assert.equal(await page.getByRole('menuitemradio', { name: 'Balanced' }).getAttribute('aria-checked'), 'true');
    const m = await rect(page, 'dialog.menu'), g = await rect(page, '#play-graphics');
    assert.ok(m.b <= g.y, 'the menu opens above its cell');
    await page.screenshot({ path: path.join(out, 'play-graphics-menu-1280.png') });
    // Review Focus 3: a background redraw while the menu is open.
    await page.evaluate(() => document.dispatchEvent(new Event('refresh-devices')));
    await page.waitForTimeout(200);
    assert.equal(await page.locator('dialog.menu[open]').count(), 1, 'the menu survives a redraw');
    await page.getByRole('menuitemradio', { name: 'All-Out' }).click();
    await page.waitForFunction(() => localStorage.getItem('h:preset') === 'quality');
    assert.deepEqual(await page.evaluate(() => calls.find((c) => c.cmd === 'setup_save_graphics_options')?.args), { mode: 'preset', resolution: 3, msaa: 1, ssaa: false });
    await page.waitForFunction(() => /All-Out/.test(document.getElementById('play-graphics').innerText));
    await page.waitForFunction(() => document.activeElement?.id === 'play-graphics');

    // Display menu: Esc closes; Windowed writes fullscreen=false.
    await page.locator('#play-display').click();
    assert.equal(await page.getByRole('menuitemradio', { name: 'Fullscreen' }).getAttribute('aria-checked'), 'true');
    await page.keyboard.press('Escape');
    await page.waitForFunction(() => !document.querySelector('dialog.menu'));
    assert.equal(await page.evaluate(() => document.activeElement?.id), 'play-display');
    await page.locator('#play-display').click();
    await page.getByRole('menuitemradio', { name: 'Windowed' }).click();
    await page.waitForFunction(() => calls.some((c) => c.cmd === 'dolphin_preference_set' && c.args.key === 'fullscreen' && c.args.value === 'false'));
    await page.waitForFunction(() => /Windowed/.test(document.getElementById('play-display').innerText));

    // ⋯ opens Game details.
    await page.getByRole('button', { name: 'Game details', exact: true }).click();
    await page.locator('#game-details[open]').waitFor(); await page.keyboard.press('Escape');

    // In session: "Playing", dimmed, "Enjoy the game" with a pulsing dot.
    await page.getByRole('button', { name: 'Play now', exact: true }).click();
    await page.getByRole('button', { name: 'Playing', exact: true }).waitFor();
    assert.equal(await page.getByRole('button', { name: 'Playing', exact: true }).isDisabled(), true);
    assert.equal(await page.locator('.play-status .play-state-title').innerText(), 'Enjoy the game');
    assert.equal(await page.locator('.play-status').evaluate((n) => n.classList.contains('is-live')), true);

    // Could not start: "See why" opens the reason.
    await page.evaluate(() => finishLaunch({ started: false, message: 'The emulator could not start.' }));
    const failed = page.getByRole('alert').filter({ hasText: 'Your game could not start' });
    await failed.waitFor();
    await failed.getByRole('button', { name: 'See why', exact: true }).click();
    await page.locator('#game-details[open]').getByText('The emulator could not start.').waitFor();
    await page.keyboard.press('Escape');

    // Closed unexpectedly: "See why", never "Recover".
    await page.getByRole('button', { name: 'Play now', exact: true }).click();
    await page.evaluate(() => finishLaunch({ started: true, message: 'The game closed unexpectedly.', needs_recovery: true, minutes: 1, stats_note: null, cards_note: null }));
    await page.getByRole('alert').filter({ hasText: 'Game closed unexpectedly' }).waitFor();
    assert.equal(await page.getByRole('button', { name: 'See why', exact: true }).count(), 1);
    assert.doesNotMatch(await page.locator('#screen').innerText(), /Recover/);
    await page.context().close();
  }
  { // Setup required: amber, "Finish setup", and the primary becomes "Set up your game".
    const page = await open({ launch_check: { ready: false, problem: 'No game image was found.' } });
    await page.getByRole('link', { name: 'Set up your game', exact: true }).waitFor();
    assert.equal(await page.locator('.play-status .play-state-title').innerText(), 'Setup required');
    assert.equal(await page.getByRole('link', { name: 'Finish setup', exact: true }).getAttribute('href'), 'onboarding.html');
    assert.equal(await page.locator('.play-status').evaluate((n) => n.classList.contains('is-setup')), true);
    await page.context().close();
  }
  { // Signed out: the sign-in button takes the account's place, top right.
    const page = await open({ identity_status: { signed_in: false, display_name: null, player_id: null, show_offline: null } });
    await page.getByRole('button', { name: 'Sign in with Discord', exact: true }).waitFor();
    const acct = await rect(page, '.play-account'), main = await rect(page, '#main');
    assert.ok(main.r - acct.r <= 26, 'top right');
    await page.context().close();
  }
  { // Review Focus 5: the minimum window with a controller-setup note.
    const page = await open({}, { width: 800, height: 560 });
    await page.getByRole('button', { name: 'Play now', exact: true }).waitFor();
    await page.evaluate(() => { window.launchAnswer = { started: false, message: 'Your controller needs a one-time setup before it can be used in the game.', controller_setup: { device: 'WGInput/0/Pro Controller', controls: ['Start', 'A'] } }; });
    await page.getByRole('button', { name: 'Play now', exact: true }).click();
    await page.locator('.play-controller-setup').waitFor();
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
    const acct = await rect(page, '.play-account'), logo = await rect(page, '.play-wordmark img'), notes = await rect(page, '.play-notes'), strip = await rect(page, '.play-strip');
    assert.ok(!overlap(acct, logo), 'the account never covers the wordmark');
    assert.ok(notes.b <= strip.y, 'the notes end above the strip');
    await page.screenshot({ path: path.join(out, 'play-setup-note-800.png'), fullPage: true });
    await page.context().close();
  }
  assert.deepEqual(errors, []);
  console.log('check-play-page: PASS');
} finally { await browser.close(); server.close(); }
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `node Tools/check-play-page.mjs`
Expected: FAIL at `dialog.menu[open]` (timeout). `#play-graphics` is still a link to Settings.

- [ ] **Step 3: Menus in the strip (`home.js`)**

**Imports.**
- Change the `shell.js` import to include `set`: `import { bridge, call, el, fact, facts, get, icon, markCurrent, minutes, reason, set, wireCoreFold } from "./shell.js";`.
- Add `import { openMenu } from "./menu.js";` below it.

**Strip.** Replace `function strip(inSession) { … }` with:

```js
// The strip (spec §8.1): Controllers, Graphics, Display. Graphics and Display open a small menu
// (names only); no checkboxes, switches or segmented controls in the strip.
function strip(inSession) {
  const pads = ok(state.devices) && Array.isArray(state.devices.pads) ? state.devices.pads : null;
  const dolphin = ok(state.dolphin) ? state.dolphin : null;
  const fullscreen = String(dolphin?.values?.fullscreen ?? "True").toLowerCase() === "true";
  const displayLocked = busy || inSession || !dolphin || dolphin.editable === false;
  return el("nav", { class: "glass play-strip", "aria-label": "Game preferences" },
    cell("a", { href: "controls.html" }, "Controllers", pads ? pads.length ? `${pads.length} connected` : "Connect a controller" : "Manage devices"),
    cell("button", { type: "button", id: "play-graphics", "aria-haspopup": "menu", "aria-expanded": "false", disabled: busy || inSession,
      onclick: (e) => openMenu(e.currentTarget, { label: "Graphics", options: PRESETS.map(([value, label]) => ({ value, label })),
        // A saved custom or previous-install setting is not one of the three: nothing is checked.
        current: ok(state.graphicsOptions) && state.graphicsOptions.current ? null : preset(), onPick: pickGraphics }) },
      "Graphics", graphicsLabel()),
    cell("button", { type: "button", id: "play-display", "aria-haspopup": "menu", "aria-expanded": "false", disabled: displayLocked,
      onclick: (e) => openMenu(e.currentTarget, { label: "Display", options: [{ value: "true", label: "Fullscreen" }, { value: "false", label: "Windowed" }],
        current: String(fullscreen), onPick: pickDisplay }) },
      "Display", fullscreen ? "Fullscreen" : "Windowed"));
}

// The same write Settings' graphics preset has always made; the preset name is kept where Play reads it.
async function pickGraphics(value) {
  if (busy) return;
  notice = null;
  try {
    await call("setup_save_graphics_options", { mode: "preset", resolution: 3, msaa: 1, ssaa: false });
    set("preset", value);
    if (ok(state.graphicsOptions)) state.graphicsOptions = { ...state.graphicsOptions, current: null };
  } catch (e) { notice = `Graphics: ${reason(e)}`; }
  draw();
}
```

**check-launch-polish.** In `Tools/check-launch-polish.mjs`, replace

```js
 await page.goto(base+'/index.html');await page.getByRole('checkbox',{name:'Fullscreen',exact:true}).waitFor();
 await page.getByRole('checkbox',{name:'Fullscreen',exact:true}).uncheck();
```

with

```js
 await page.goto(base+'/index.html');await page.locator('#play-display').waitFor();
 await page.locator('#play-display').click();await page.getByRole('menuitemradio',{name:'Windowed',exact:true}).click();
```

That checker still fails earlier, at its pre-existing baseline failure. This keeps its Play step true to the page.

- [ ] **Step 4: Run the checks**

- `node Tools/check-play-page.mjs`: PASS. Screens land in `target/glass-review/`.
- The standing checks: PASS.
- `node Tools/check-launch-polish.mjs`: still FAIL, at the same place as on the baseline (stats paging), not at Play.

- [ ] **Step 5: Commit**

Append `await import('./check-play-page.mjs');` to the end of `Tools/run-browser-tests.mjs`.

```bash
git add crates/reborn-ui/ui/home.js Tools/check-play-page.mjs Tools/check-launch-polish.mjs Tools/run-browser-tests.mjs
git commit -m "play: Graphics and Display menus in the strip; Play page check

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Segmented tabs, and the pending-change list

**Files:**
- Create: `crates/reborn-ui/ui/tabs.js`, `crates/reborn-ui/ui/settings-draft.js`, `crates/reborn-ui/ui/settings-draft.test.mjs`
- Modify: `crates/reborn-ui/ui/theme.css` (append)
- Create: `Tools/check-tabs.mjs`

**Interfaces:**
- Produces:
  - `tabs(group: string, names: string[], current: string, onPick: (name) => void, { disabled = false, label } = {}) → HTMLElement`.
    - The result is a `div.tabs` holding `button.tab[aria-pressed]` items and one `span.tab-ind`.
    - The highlight starts where this `group`'s last one was and slides to `current` on the next frames.
    - It is safe without a real DOM: it returns the element and places nothing.
  - `createDraft() → { put(key, entry), drop(key), has(key), value(key, fallback), changes(): string[], saveAll(), discardAll() }`, and `HELD` (a string).
    - `entry = { label: string, value, same = false, save: async (value) => void|HELD, revert?: async () => void }`.
    - `put` with `same: true` removes the key (the change was undone). A `put` moves the key to the end, so the newest change is listed last.
    - `saveAll() → { saved: number, stopped: null | { key, label, reason } }`. It saves in order. It stops at the first throw (reason = the message) or at the first `HELD` (reason = `HELD`). The stopped entry and everything after it stay pending.
    - `discardAll() → string[]`. It runs `revert` newest first and clears the list. It never saves. It returns `"<label>: <reason>"` for each revert that failed.

- [ ] **Step 1: Write the failing tests**

Create `crates/reborn-ui/ui/settings-draft.test.mjs`:

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createDraft, HELD } from './settings-draft.js';

const noop = async () => {};

test('changes are listed by label, newest last; undoing one removes it', () => {
  const d = createDraft();
  d.put('a', { label: 'Alpha', value: 1, save: noop });
  d.put('b', { label: 'Beta', value: 2, save: noop });
  d.put('a', { label: 'Alpha', value: 3, save: noop });
  assert.deepEqual(d.changes(), ['Beta', 'Alpha']);
  assert.equal(d.value('a', 0), 3);
  assert.equal(d.value('missing', 9), 9);
  d.put('b', { label: 'Beta', value: 0, same: true, save: noop });
  assert.deepEqual(d.changes(), ['Alpha']);
  assert.equal(d.has('b'), false);
  d.drop('a');
  assert.deepEqual(d.changes(), []);
});

test('saveAll saves in the order the changes were made and empties the list', async () => {
  const d = createDraft(), order = [];
  d.put('a', { label: 'Alpha', value: 1, save: async (v) => { order.push(['a', v]); } });
  d.put('b', { label: 'Beta', value: 2, save: async (v) => { order.push(['b', v]); } });
  assert.deepEqual(await d.saveAll(), { saved: 2, stopped: null });
  assert.deepEqual(order, [['a', 1], ['b', 2]]);
  assert.deepEqual(d.changes(), []);
});

test('saveAll stops at the first failure and keeps it and everything after it', async () => {
  const d = createDraft(), order = [];
  d.put('a', { label: 'Alpha', value: 1, save: async () => { order.push('a'); } });
  d.put('b', { label: 'Beta', value: 2, save: async () => { throw new Error('Dolphin is open'); } });
  d.put('c', { label: 'Gamma', value: 3, save: async () => { order.push('c'); } });
  assert.deepEqual(await d.saveAll(), { saved: 1, stopped: { key: 'b', label: 'Beta', reason: 'Dolphin is open' } });
  assert.deepEqual(order, ['a']);
  assert.deepEqual(d.changes(), ['Beta', 'Gamma']);
});

test('a string rejection is its own reason', async () => {
  const d = createDraft();
  d.put('a', { label: 'Alpha', value: 1, save: async () => { throw 'settings file is locked'; } });
  assert.equal((await d.saveAll()).stopped.reason, 'settings file is locked');
});

test('a held save stops without an error and stays pending', async () => {
  const d = createDraft();
  d.put('g', { label: 'Game image', value: 'x.iso', save: async () => HELD });
  d.put('e', { label: 'Emulator', value: 'y.exe', save: async () => { throw new Error('should not run'); } });
  assert.deepEqual(await d.saveAll(), { saved: 0, stopped: { key: 'g', label: 'Game image', reason: HELD } });
  assert.deepEqual(d.changes(), ['Game image', 'Emulator']);
});

test('discardAll reverts newest first, never saves, and reports revert failures', async () => {
  const d = createDraft(), order = [];
  d.put('a', { label: 'Window glass', value: false, save: async () => { throw new Error('no save on discard'); }, revert: async () => { order.push('a'); } });
  d.put('b', { label: 'Pause', value: 'true', save: async () => { throw new Error('no save on discard'); } });
  d.put('c', { label: 'Slow animations', value: false, save: async () => {}, revert: async () => { order.push('c'); throw new Error('could not'); } });
  assert.deepEqual(await d.discardAll(), ['Slow animations: could not']);
  assert.deepEqual(order, ['c', 'a']);
  assert.deepEqual(d.changes(), []);
});
```

Create `Tools/check-tabs.mjs`:

```js
// Segmented tabs (spec §3.4, §5.2): one highlight that slides to the selected option at --d2.
//     node Tools/check-tabs.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const html=`<!doctype html><html lang="en" data-glass="off" data-motion="quick"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css"></head>
<body class="shell themed page-test"><main id="main"><section id="screen"></section></main>
<script type="module">
import {tabs} from './tabs.js';
let current='General';
window.draw=()=>document.getElementById('screen').replaceChildren(tabs('check',['General','Graphics','Files & storage'],current,n=>{current=n;window.draw();},{label:'Sections'}));
window.draw();window.ready=true;
</script></body></html>`;
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/tabs.html'){res.setHeader('Content-Type','text/html');res.end(html);return;}
 const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
try{
 const page=await browser.newPage({viewport:{width:1280,height:800}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.__TAURI__={core:{invoke:async()=>null}};});
 await page.goto(`http://127.0.0.1:${server.address().port}/tabs.html`);await page.waitForFunction(()=>window.ready);
 const gap=()=>page.evaluate(()=>{const i=document.querySelector('.tab-ind').getBoundingClientRect(),t=document.querySelector('.tab[aria-pressed="true"]').getBoundingClientRect();return Math.max(Math.abs(i.x-t.x),Math.abs(i.width-t.width),Math.abs(i.height-t.height));});
 await page.waitForTimeout(400);
 assert.ok(await gap()<1,'the highlight sits on the selected tab');
 assert.match(await page.locator('.tab-ind').evaluate(n=>getComputedStyle(n).transitionDuration),/^0\.18s/,'quick: --d2');
 await page.evaluate(()=>document.documentElement.dataset.motion='slow');
 const from=await page.locator('.tab[aria-pressed="true"]').evaluate(n=>n.getBoundingClientRect().x);
 await page.getByRole('button',{name:'Files & storage'}).click();
 await page.waitForTimeout(200);
 const mid=await page.locator('.tab-ind').evaluate(n=>n.getBoundingClientRect().x);
 const to=await page.locator('.tab[aria-pressed="true"]').evaluate(n=>n.getBoundingClientRect().x);
 assert.ok(mid>from+1&&mid<to-1,`it slides: mid-way x ${mid} lies between ${from} and ${to}`);
 await page.waitForTimeout(900);
 assert.ok(await gap()<1,'it settles on Files & storage');
 assert.deepEqual(await page.locator('.tab').evaluateAll(ns=>ns.map(n=>n.getAttribute('aria-pressed'))),['false','false','true']);
 assert.equal(await page.locator('.tab-ind').count(),1,'one highlight');
 await page.emulateMedia({reducedMotion:'reduce'});
 assert.match(await page.locator('.tab-ind').evaluate(n=>getComputedStyle(n).transitionDuration),/^0s/,'reduced motion');
 assert.deepEqual(errors,[]);
 console.log('check-tabs: PASS');
}finally{await browser.close();server.close();}
```

- [ ] **Step 2: Run them and confirm they fail**

- `node --test crates/reborn-ui/ui/settings-draft.test.mjs`. Expected: FAIL (`Cannot find module … settings-draft.js`).
- `node Tools/check-tabs.mjs`. Expected: FAIL (`window.ready` never true, because `tabs.js` returns 404).

- [ ] **Step 3: Write `settings-draft.js`**

```js
// Pending Settings changes (spec §7, §8.2): every change waits here until Save changes or Discard.
// Pure: no DOM, no commands. Each entry brings its own save (and, for a live preview, its own revert).
export const HELD = "held";

export function createDraft() {
  const entries = new Map();
  return {
    // Record a change; `same` means it now equals what is saved, which removes it. Re-inserted, so the
    // newest change is listed last (the bar names the last change).
    put(key, { label, value, same = false, save, revert = null }) {
      entries.delete(key);
      if (!same) entries.set(key, { label, value, save, revert });
    },
    drop(key) { entries.delete(key); },
    has(key) { return entries.has(key); },
    value(key, fallback) { return entries.has(key) ? entries.get(key).value : fallback; },
    changes() { return [...entries.values()].map((e) => e.label); },
    // In the order the changes were made. Stops at the first that fails (its message) or is held for a
    // choice the player must make (HELD); that one and every later one stay pending.
    async saveAll() {
      let saved = 0;
      for (const [key, e] of [...entries]) {
        let outcome;
        try { outcome = await e.save(e.value); }
        catch (err) { return { saved, stopped: { key, label: e.label, reason: message(err) } }; }
        if (outcome === HELD) return { saved, stopped: { key, label: e.label, reason: HELD } };
        entries.delete(key);
        saved++;
      }
      return { saved, stopped: null };
    },
    // Put every previewed change back, newest first. Nothing is saved. Returns what could not be put back.
    async discardAll() {
      const failed = [];
      for (const [, e] of [...entries].reverse()) {
        if (!e.revert) continue;
        try { await e.revert(); } catch (err) { failed.push(`${e.label}: ${message(err)}`); }
      }
      entries.clear();
      return failed;
    },
  };
}

function message(e) {
  if (e === null || e === undefined) return "unknown failure";
  if (typeof e === "string") return e;
  return e.message || JSON.stringify(e);
}
```

- [ ] **Step 4: Write `tabs.js`**

```js
// Segmented tabs with one sliding highlight (spec §3.4, §5.2). Buttons with aria-pressed, as before, so
// the pad and the checks find them by name. A page that redraws gets a new element each time; the
// highlight starts where this group's last one was and glides to the selected option.
import { el } from "./shell.js";

const last = new Map();

export function tabs(group, names, current, onPick, { disabled = false, label } = {}) {
  const ind = el("span", { class: "tab-ind still", "aria-hidden": "true" });
  const bar = el("div", { class: "tabs", role: "group", "aria-label": label || group }, ind,
    ...names.map((name) => el("button", { type: "button", class: "tab", text: name, "aria-pressed": String(name === current), disabled,
      onclick: () => { if (name !== current) onPick(name); } })));
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(() => place(group, bar, ind));
  return bar;
}

function place(group, bar, ind) {
  const on = bar.querySelector('[aria-pressed="true"]');
  if (!bar.isConnected || !on) return;
  const to = { x: on.offsetLeft, w: on.offsetWidth };
  const from = last.get(group) || to;
  ind.style.transform = `translateX(${from.x}px)`;
  ind.style.width = `${from.w}px`;
  last.set(group, to);
  requestAnimationFrame(() => {
    ind.classList.remove("still");
    ind.style.transform = `translateX(${to.x}px)`;
    ind.style.width = `${to.w}px`;
  });
}
```

- [ ] **Step 5: The tabs styles**

Append to `theme.css`:

```css
/* segmented tabs (tabs.js): a dark track, one highlight that slides and resizes to the selected option (spec §3.4, §5.2).
   The highlight is out of flow, so its width change moves nothing else; it is the one non-transform property animated. */
.tabs{position:relative;display:inline-flex;gap:2px;max-width:100%;padding:4px;overflow-x:auto;scrollbar-width:none;border-radius:12px;background:rgba(0,0,0,.22);border:1px solid var(--stroke)}
.tabs .tab{position:relative;z-index:1;height:32px;padding:0 14px;border:0;border-radius:var(--r-sm);background:transparent;color:var(--fg-2);font:500 13px Inter,system-ui,sans-serif;white-space:nowrap;cursor:pointer}
.tabs .tab:hover{color:var(--fg)}
.tabs .tab[aria-pressed="true"]{color:var(--fg)}
.tab-ind{position:absolute;top:4px;bottom:4px;left:0;width:0;border-radius:var(--r-sm);background:var(--glass-3);box-shadow:var(--sheen),0 0 0 1px var(--stroke);transition:transform var(--d2) var(--ease),width var(--d2) var(--ease);pointer-events:none}
.tab-ind.still{transition:none}
```

- [ ] **Step 6: Run the checks**

- `node --test crates/reborn-ui/ui/settings-draft.test.mjs`: 6 pass.
- `node Tools/check-tabs.mjs`: PASS.
- The standing checks: PASS.

- [ ] **Step 7: Commit**

Append `await import('./check-tabs.mjs');` to the end of `Tools/run-browser-tests.mjs`.

```bash
git add crates/reborn-ui/ui/tabs.js crates/reborn-ui/ui/settings-draft.js crates/reborn-ui/ui/settings-draft.test.mjs crates/reborn-ui/ui/theme.css Tools/check-tabs.mjs Tools/run-browser-tests.mjs
git commit -m "ui: sliding segmented tabs; one pending-change list for Settings

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Settings on the design system, every change through the save bar

**Files:**
- Modify: `crates/reborn-ui/ui/settings.html`
- Create: `crates/reborn-ui/ui/pages/settings.css`
- Rewrite: `crates/reborn-ui/ui/settings.js`
- Modify: `crates/reborn-ui/ui/theme.css` (remove the plan-1 `.look-*` layout rules; add `.field`)
- Modify: `Tools/check-theme.mjs` (`THEMED.settings`)
- Modify: `Tools/check-shell-pages.mjs` (fake DOM + the Tier 2 driven test)
- Modify: `Tools/run-browser-tests.mjs` (Settings flows)
- Create: `Tools/check-settings-page.mjs`

**Interfaces:**
- Consumes:
  - `mountSaveBar` with its failure line (Task 2), `tabs` (Task 6), `createDraft`/`HELD` (Task 6);
  - `icon` (Task 4), `applyLook`/`refreshLookResult` (plan 1);
  - `.btn`, `.glass`, `.look-switch`, `.field`, `--dolphin`, `--field-bg`.
- Produces what Plan 3 copies:
  - the `pref-section`/`pref-row` layout;
  - the "stage a change, save it in order" pattern.
- These ids and labels are what the checks use:
  - tab buttons named `General … Files & storage`;
  - `#graphics-preset` (label "Graphics preset");
  - `#update-policy`;
  - `#preference-<key>` (switches `role=switch` named by the row label; sliders);
  - `#path-<key>` readonly inputs labelled by the row name, `#path-<key>-auto`;
  - buttons `Change <name>` and `Reset <name> to automatic`;
  - `#look-glass`, `#look-motion`, `.look-section`;
  - `#dolphin-status`, `.dolphin-card`.

What is a setting and what is an action:
- **Settings wait for Save changes:**
  - every Dolphin option;
  - the graphics preset;
  - the update preference;
  - the four locations;
  - the two Saves-tab file locations;
  - Window glass and Slow animations.
- **Actions happen when pressed, as today:**
  - Open Dolphin, Check for updates, Download, Install and restart, Patch notes, Uninstall;
  - the game-content buttons;
  - Repair my game, Open save folder, Refresh settings.
- **The one exception (DECISION):** while a game is running, volume and mute are `live_editable`, and they still reach the game at once. That is what they have always done (spec §10), and a volume knob that waits for a Save button would be broken. They never open the bar.

- [ ] **Step 1: Update the expectations that change on purpose (they fail now)**

`Tools/check-theme.mjs`: `const THEMED = { index: "pages/play.css", settings: "pages/settings.css" };`

`Tools/check-shell-pages.mjs`:

**(a)** In `fakeEl(tag)`, add these members to the returned object, after `listeners: {},`:

```js
    classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
    contains() { return false; },
    focus() {},
```

**(b)** Replace the Tier 2 driven block from `const saveButton=findByAttr(page.screen(),"aria-label","Save Game image");` down to and including its closing `}` of the `else{ … }`. That is everything before `page.done();`. Replace it with:

```js
 const changeButton=findByAttr(page.screen(),"aria-label","Change Game image");
 if(!changeButton){check("Settings has a Change… button for the game image",false);}
 else{
   await press(changeButton);
   check("Choosing an image waits for Save changes", !seen.some(([cmd])=>cmd==="settings_set_path"), JSON.stringify(seen.filter(([c])=>c==="settings_set_path")));
   const saveButton=findByText(byId["save-lane"],"Save changes");
   if(!saveButton){check("The save bar offers Save changes",false);}
   else{
     await press(saveButton);
     const t=text(page.screen());
     check("Choosing an unconverted image puts the choice to the player", t.includes(CHOICE.headline)&&t.includes("Repair my game")&&t.includes("Play it anyway"), t.slice(0,400));
     check("Nothing is acknowledged before the player answers", !seen.some(([cmd,args])=>cmd==="settings_set_path"&&args.acknowledgeUnverified===true), JSON.stringify(seen.filter(([c])=>c==="settings_set_path")));
     check("The held change stays pending", text(byId["save-lane"]).includes("Game image"), text(byId["save-lane"]).slice(0,200));
     const anyway=findByText(page.screen(),"Play it anyway");
     if(!anyway){check("The escape hatch is offered",false);}
     else{
       await press(anyway);
       check("Play it anyway is the only thing that acknowledges", seen.some(([cmd,args])=>cmd==="settings_set_path"&&args.acknowledgeUnverified===true), JSON.stringify(seen.filter(([c])=>c==="settings_set_path")));
     }
   }
 }
```

**(c)** In that block's `invoke`, add as its first line after `seen.push(...)`:

```js
   if(cmd==="pick_path")return "C:\\x\\NSR2.iso";
```

`Tools/run-browser-tests.mjs`:

**(d)** Replace

```js
await page.getByLabel('Graphics preset',{exact:true}).selectOption('quality');
await page.waitForFunction(()=>localStorage.getItem('h:preset')==='quality');
```

with

```js
await page.getByLabel('Graphics preset',{exact:true}).selectOption('quality');
await page.getByRole('button',{name:'Save changes',exact:true}).click();
await page.waitForFunction(()=>localStorage.getItem('h:preset')==='quality');
```

**(e)** Replace the block from `await page.getByRole('button',{name:'Browse for Game image',exact:true}).click();` through `assert.match(await page.evaluate(()=>localStorage.getItem('h:game')),/NSR.iso/);` (the Settings path flow) with:

```js
// Change… stages the path; nothing is saved before Save changes (spec §8.2).
await page.getByRole('button',{name:'Change Game image',exact:true}).click();
await page.locator('.save-lane.open').waitFor();
assert.match(await page.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Game image/);
assert.equal(await page.evaluate(()=>localStorage.getItem('h:game')),null,'nothing is saved before Save changes');
await page.getByRole('button',{name:'Save changes',exact:true}).click();
await page.waitForFunction(()=>localStorage.getItem('h:game')?.includes('NSR.iso'));
await page.reload();await page.getByRole('button',{name:'Files & storage',exact:true}).click();await page.getByLabel('Game image',{exact:true}).waitFor();
assert.match(await page.getByLabel('Game image',{exact:true}).inputValue(),/NSR.iso/);
// A save that fails says which and why in the bar, and changes nothing.
await page.evaluate(()=>window.__reject='settings_set_path');
await page.getByRole('button',{name:'Reset Game image to automatic',exact:true}).click();
await page.getByRole('button',{name:'Save changes',exact:true}).click();
await page.locator('.save-bar [role=alert]').waitFor();
assert.match(await page.locator('.save-bar [role=alert]').innerText(),/Game image: Test failure: settings_set_path/);
assert.match(await page.evaluate(()=>localStorage.getItem('h:game')),/NSR.iso/);
```

Run:
- `node Tools/check-theme.mjs`. Expected: FAIL (`settings: must load exactly theme.css + pages/settings.css`).
- `node Tools/check-shell-pages.mjs`. Expected: FAIL (`Settings has a Change… button for the game image`).

- [ ] **Step 2: Write `Tools/check-settings-page.mjs` and confirm it fails**

```js
// Settings (spec §8.2, §7): the Dolphin card once, seven sliding tabs, and every change waiting in the
// save bar until Save changes, except volume and mute while a game is running.
//     node Tools/check-settings-page.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const out=path.join(root,'target/glass-review');mkdirSync(out,{recursive:true});
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const f=path.resolve(ui,'.'+decodeURIComponent(new URL(req.url,'http://x').pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const VALUES={confirm_stop:'True',pause_unfocused:'False',osd_messages:'True',internal_resolution:'3',msaa:'1',video_backend:'D3D',sync_gpu:'False',vsync:'False',show_fps:'False',fullscreen:'True',show_ping:'False',always_on_top:'False',audio_backend:'Cubeb',volume:'40',muted:'False',audio_stretch:'False',audio_latency:'20',slot_a:'8',card_a:'',gci_a:'',slot_b:'255',card_b:'',gci_b:''};
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
const errors=[];
async function open(hash='',variant={},viewport={width:1280,height:800}){
 const ctx=await browser.newContext({viewport});
 await ctx.addInitScript(({values,variant})=>{
  window.calls=[];window.reject=null;
  const data={
   dolphin_preferences:{values,editable:true,live_editable:[],user_dir:'C:\\Users\\A\\Dolphin User'},
   settings_paths:{runtime:null,game:null,userDir:null,legacyRoot:null},
   launch_check:{ready:true,runtime:'C:\\rt\\Dolphin.exe',game:'C:\\g\\NSR.iso',user_dir_note:'u',problem:null},
   setup_graphics_options:{current:null,previous:null},update_policy:{automatic:true},
   paths:{install_root:'i',data_root:'d',saves_survive_uninstall:true,directories:[],channel:'beta',channel_note:'beta channel.'},
   content_status:{phase:'up_to_date',components:[],note:'Everything is current.'},
   launcher_look:{windowGlass:false,slowAnimations:false,glassSupported:true,glassApplied:false,glassNote:null},
   session_active:false,ui_notices:[],onboarding_progress:{complete:true,summary:{graphics:'balanced'}},...variant};
  window.data=data;
  window.__TAURI__={core:{invoke:async(cmd,args={})=>{
   calls.push({cmd,args:JSON.parse(JSON.stringify(args))});
   if(window.reject===cmd)throw 'Dolphin is open';
   if(cmd==='pick_path')return 'C:\\Games\\Other.iso';
   if(cmd==='settings_set_path'){const k={game:'game',runtime:'runtime',user_dir:'userDir',legacy_root:'legacyRoot'}[args.which];data.settings_paths={...data.settings_paths,[k]:args.value||null};return {which:args.which,value:args.value||null};}
   if(cmd==='dolphin_preference_set'){data.dolphin_preferences.values[args.key]=args.value;return null;}
   if(cmd==='window_glass_apply')return {...data.launcher_look,glassApplied:args.enabled};
   return cmd in data?data[cmd]:null;
  }}};
 },{values:VALUES,variant});
 const page=await ctx.newPage();page.on('pageerror',e=>errors.push(e.message));
 await page.goto(base+'/settings.html'+hash);await page.locator('.dolphin-card').waitFor();
 return page;
}
const sets=page=>page.evaluate(()=>calls.filter(c=>c.cmd==='dolphin_preference_set').map(c=>[c.args.key,c.args.value]));
try{
 {
  const page=await open();
  assert.equal(await page.locator('.dolphin-card').count(),1,'one Dolphin card, not one per tab');
  await page.getByRole('button',{name:'Open Dolphin',exact:true}).waitFor();
  assert.deepEqual(await page.locator('.tabs .tab').allInnerTexts(),['General','Graphics','Display','Audio','Saves','Updates','Files & storage']);
  const ind=async()=>{await page.waitForTimeout(450);return page.evaluate(()=>{const i=document.querySelector('.tab-ind').getBoundingClientRect(),t=document.querySelector('.tabs .tab[aria-pressed="true"]').getBoundingClientRect();return Math.max(Math.abs(i.x-t.x),Math.abs(i.width-t.width));});};
  assert.ok(await ind()<1.5,'the highlight sits on General');
  await page.getByRole('button',{name:'Audio',exact:true}).click();
  assert.ok(await ind()<1.5,'the highlight slid to Audio');
  assert.ok(await page.locator('#screen').evaluate(n=>n.getBoundingClientRect().width)<=942.5,'form column ≤ 942px (mockup .inner 1014 minus its padding)');
  await page.getByRole('button',{name:'General',exact:true}).click();
  const pause=()=>page.getByRole('switch',{name:'Pause when unfocused'});
  assert.equal(await pause().getAttribute('aria-checked'),'false');
  await pause().click();
  await page.locator('.save-lane.open').waitFor();
  assert.match(await page.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Pause when unfocused/);
  assert.equal(await pause().getAttribute('aria-checked'),'true','the switch shows the pending value');
  assert.deepEqual(await sets(page),[],'nothing written before Save');
  await page.waitForTimeout(400);
  const edge=await page.evaluate(()=>({main:document.getElementById('main').getBoundingClientRect().bottom,lane:document.getElementById('save-lane').getBoundingClientRect().top}));
  assert.ok(Math.abs(edge.main-edge.lane)<1,'the bar never covers content');
  await page.getByRole('button',{name:'Discard',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.equal(await pause().getAttribute('aria-checked'),'false','Discard puts it back');
  await pause().click();
  await page.getByRole('switch',{name:'Game notifications'}).click();
  assert.match(await page.locator('.save-bar').innerText(),/2 unsaved changes[\s\S]*Game notifications/);
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.deepEqual(await sets(page),[['pause_unfocused','true'],['osd_messages','false']],'both written, in order');
  await page.evaluate(()=>window.reject='dolphin_preference_set');
  await page.getByRole('switch',{name:'Confirm before stopping'}).click();
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.locator('.save-bar [role=alert]').waitFor();
  assert.match(await page.locator('.save-bar [role=alert]').innerText(),/Confirm before stopping: Dolphin is open/);
  assert.equal(await page.locator('.save-lane.open').count(),1,'a failed change stays pending');
  await page.evaluate(()=>window.reject=null);
  await page.getByRole('button',{name:'Discard',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  await page.context().close();
 }
 {
  const page=await open('#graphics');
  await page.getByLabel('Graphics preset',{exact:true}).selectOption('performance');
  assert.match(await page.locator('.save-bar').innerText(),/Graphics preset/);
  assert.equal(await page.evaluate(()=>calls.some(c=>c.cmd==='setup_save_graphics_options')),false);
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.waitForFunction(()=>localStorage.getItem('h:preset')==='performance');
  await page.context().close();
 }
 {
  const page=await open('#audio',{dolphin_preferences:{values:VALUES,editable:false,live_editable:['volume','muted'],readonly_reason:'Close Dolphin to change these.',user_dir:'x'}});
  await page.getByRole('slider',{name:'Game volume',exact:true}).evaluate(n=>{n.value='37';n.dispatchEvent(new Event('change',{bubbles:true}));});
  await page.waitForFunction(()=>calls.some(c=>c.cmd==='dolphin_preference_set'&&c.args.key==='volume'&&c.args.value==='37'));
  assert.equal(await page.locator('.save-lane.open').count(),0,'live volume never waits for the bar');
  await page.context().close();
 }
 {
  const page=await open('#files');
  const field=page.getByLabel('Game image',{exact:true});
  assert.match(await field.inputValue(),/NSR\.iso/,'shows the detected image');
  assert.equal(await page.locator('#path-game-auto').innerText(),'Auto');
  await page.getByRole('button',{name:'Change Game image',exact:true}).click();
  await page.locator('.save-lane.open').waitFor();
  assert.match(await field.inputValue(),/Other\.iso/,'shows the pending path');
  assert.equal(await page.evaluate(()=>calls.some(c=>c.cmd==='settings_set_path')),false);
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.waitForFunction(()=>calls.some(c=>c.cmd==='settings_set_path'&&c.args.which==='game'&&c.args.value==='C:\\Games\\Other.iso'));
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.equal(await page.locator('#path-game-auto').count(),0,'a chosen path is not Auto');
  await page.screenshot({path:path.join(out,'settings-files-1280.png')});
  await page.context().close();
 }
 {
  const page=await open('',{},{width:800,height:560});
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no sideways scroll at 800x560');
  await page.screenshot({path:path.join(out,'settings-800.png')});
  await page.context().close();
 }
 assert.deepEqual(errors,[]);
 console.log('check-settings-page: PASS');
}finally{await browser.close();server.close();}
```

Run: `node Tools/check-settings-page.mjs`
Expected: FAIL (timeout waiting for `.dolphin-card`).

- [ ] **Step 3: `settings.html` becomes theme-only**

Keep the `<nav class="shell-nav">…</nav>` block and the `<details class="core">…</details>` line byte-identical, and change the rest:

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Settings | NFL Street Reborn</title>
  <script src="look-boot.js"></script>
  <link rel="icon" href="art/cover.png">
  <link rel="stylesheet" href="theme.css"><link rel="stylesheet" href="pages/settings.css">
</head>
<body class="shell themed page-settings has-save-lane">
  <a class="skip-link" href="#main">Skip to content</a>
  <header class="app-header"><span id="bridge" class="pill">Connecting</span></header>
  <!-- the existing <nav class="shell-nav"> … </nav> block, unchanged -->
  <main id="main" tabindex="-1">
    <div class="page-heading"><h1>Settings</h1><p class="purpose">Your installation, updates, and preferences.</p></div>
    <section id="screen"><p class="loading">Loading…</p></section>
    <!-- the existing <details class="core"> … </details> line, unchanged -->
  </main>
  <div id="save-lane" inert></div>
  <footer class="input-footer"><span id="input-hints"></span></footer>
  <script type="module" src="app-shell.js"></script><script type="module" src="settings.js"></script>
</body>
</html>
```

`inert` in the markup keeps the closed lane unfocusable before the bar is first mounted. The bar mounts on the first change.

- [ ] **Step 4: `pages/settings.css`**

```css
/* Settings (spec §8.2; mockup "SETTINGS"): header, the Dolphin card, the tabs, then glass sections of rows.
   Layout only: colours, radii and durations come from theme.css. The form column is 942px, the mockup's
   .inner (max-width 1014 including 36px padding each side). */
body.page-settings #main{padding:36px 36px 28px}
.page-settings .page-heading,.page-settings #screen{max-width:942px}
.page-settings #screen{display:flex;flex-direction:column}
.page-settings #screen>.refusal,.page-settings .loading{margin-top:24px}
.page-settings .settings-status{margin-top:16px;font-size:12.5px;color:var(--fg-2)}
body.page-settings .save-bar{margin:10px 36px 18px}

.dolphin-card{display:flex;align-items:center;gap:16px;margin-top:24px;padding:14px 16px 14px 18px}
.dolphin-icon{display:grid;place-items:center;flex:none;width:38px;height:38px;border-radius:var(--r-md);color:var(--dolphin);background:linear-gradient(135deg,color-mix(in srgb,var(--dolphin) 35%,transparent),color-mix(in srgb,var(--dolphin) 8%,transparent));border:1px solid color-mix(in srgb,var(--dolphin) 35%,transparent)}
.dolphin-icon svg{width:18px;height:18px}
.dolphin-text{flex:1;min-width:0}
.dolphin-text h2{font-size:14px;font-weight:600}
.dolphin-text p{margin-top:2px;font-size:12.5px;color:var(--fg-2)}

.page-settings .tabs{align-self:flex-start;margin-top:24px}

.pref-section{margin-top:24px;padding:6px 0}
.pref-section>h2{padding:16px 22px 2px;font-size:15px;font-weight:600}
.pref-lead{padding:0 22px 10px;font-size:12.5px;color:var(--fg-2)}
.pref-section>.partial,.pref-section>.refusal{margin:8px 22px 14px}
.pref-row{display:grid;grid-template-columns:150px minmax(0,1fr) auto;align-items:center;gap:8px 18px;padding:14px 22px;border-top:1px solid var(--stroke)}
.pref-label{font-size:13px;font-weight:600}
.pref-value{min-width:0;font-size:13px;color:var(--fg-2);overflow-wrap:anywhere}
.pref-control{display:flex;align-items:center;justify-content:flex-end;gap:8px;flex-wrap:wrap}
.pref-help{grid-column:2/4;margin-top:-6px;font-size:12px;color:var(--fg-3)}
.pref-state{font-size:13px;color:var(--fg-2)}
.pref-badge{display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;font-size:11px;font-weight:600;color:var(--ok);background:color-mix(in srgb,var(--ok) 10%,transparent);border:1px solid color-mix(in srgb,var(--ok) 25%,transparent)}
.pref-row select{width:220px}
.pref-row input[type=range]{width:100%}
.pref-list{margin:0 22px 14px;padding-left:18px;font-size:12.5px;color:var(--fg-2)}
.pref-section .settings-progress{display:block;width:calc(100% - 44px);height:8px;margin:0 22px 14px}
.look-section .look-error{padding:0 22px 14px}
.page-settings .refresh{align-self:flex-start;margin-top:24px}

.settings-choice{width:min(520px,calc(100vw - 48px))}
.settings-choice h2{margin-bottom:8px;font-size:18px;font-weight:600}
.settings-choice p{font-size:13px;color:var(--fg-2)}
.settings-choice .settings-actions{display:flex;justify-content:flex-end;gap:8px;flex-wrap:wrap;margin-top:24px}

@media (max-width:900px){
  body.page-settings #main{padding:24px 24px 16px}
  body.page-settings .save-bar{margin:8px 24px 16px}
  .pref-row{grid-template-columns:120px minmax(0,1fr) auto;padding:14px 16px}
  .pref-row select{width:180px}
}
```

- [ ] **Step 5: `theme.css`: drop the plan-1 look layout, add `.field`**

Delete these lines. `settings.js` now draws the Launcher window section with the shared `pref-*` rows.

```css
/* Launcher window section (settings > Display) */
.look-section{padding:6px 0;margin:16px 0}
.look-section h2{font:600 15px Inter;padding:16px 22px 2px}
.look-lead{color:var(--fg-2);font-size:12.5px;padding:0 22px 10px}
.look-row{display:grid;grid-template-columns:150px 1fr auto;gap:8px 16px;align-items:center;padding:14px 22px;border-top:1px solid var(--stroke)}
.look-label{font-weight:600;font-size:13px}
.look-help{grid-column:1/-1;font-size:12px;color:var(--fg-3);margin:0}
```

and the line:

```css
body.shell .look-section h2{text-transform:none;letter-spacing:0;color:var(--fg)}
```

Keep the `body.shell button.look-switch…` lines and `.look-error`. Then append:

```css
/* field (spec §8.2 path rows): a value in a recessed box, with the green Auto tag while nothing was chosen */
.field{display:flex;align-items:center;gap:8px;height:38px;min-width:0;padding:0 12px;border-radius:var(--r-md);background:var(--field-bg);border:1px solid var(--stroke);box-shadow:inset 0 1px 2px rgba(0,0,0,.25)}
body.shell .field input{flex:1;min-width:0;height:100%;padding:0;border:0;border-radius:0;background:transparent;color:var(--fg-2);font:500 13px Inter,system-ui,sans-serif;text-overflow:ellipsis;box-shadow:none}
.field .auto{flex:none;margin-left:auto;padding:2px 8px;border-radius:999px;font-size:11px;font-weight:600;color:var(--ok);background:rgba(61,220,151,.1);border:1px solid rgba(61,220,151,.25)}
```

- [ ] **Step 6: Rewrite `settings.js`**

First run `git log -3 --format="%h %s" -- crates/reborn-ui/ui/settings.js`. If there is a commit newer than `54288f0`, read its diff (`git show <hash> -- crates/reborn-ui/ui/settings.js`) and carry that fix into the file below.

Then replace the whole file with:

```js
// Settings, as a page: where things live, and the few paths a person can set.
//
// Every setting waits in one list until Save changes (spec §7, §8.2): Dolphin's options, the graphics
// preset, the update preference, the locations, and the launcher window's look. Actions (Open Dolphin,
// check for updates, repair, uninstall) still happen when pressed; they are not settings. The one
// exception is live audio: while a game is running, volume and mute reach it at once, which is what
// they have always done.

import { bridge, call, el, get, icon, markCurrent, partial, reason, refusal, set, wireCoreFold } from "./shell.js";
import { applyLook, refreshLookResult } from "./look.js";
import { mountSaveBar } from "./savebar.js";
import { tabs } from "./tabs.js";
import { createDraft, HELD } from "./settings-draft.js";

const state = {};
// What changed in each release, on the site. Opened in the browser.
const PATCH_NOTES_URL = "https://nflstreetreborn.com/blog";
const TABS = ["General", "Graphics", "Display", "Audio", "Saves", "Updates", "Files & storage"];
const tabForHash = () => ({ general: "General", graphics: "Graphics", display: "Display", audio: "Audio", saves: "Saves", updates: "Updates", files: "Files & storage" })[location.hash?.slice(1).toLowerCase()] || "General";
let settingsTab = tabForHash();
window.addEventListener?.("hashchange", () => { if (!busy) { settingsTab = tabForHash(); draw(); } });
let problem = null;
let busy = false;
// The update flow's last answers, shown in place.
let update = null; // update_check view
let downloaded = null; // update_download view
// What Open Dolphin last said, in the Dolphin card.
let dolphinSaid = null;
// The Tier 2 choice, while it is open. Set by a save of the game path that stored nothing because the
// chosen image is NFL Street 2 but not a Reborn build. The words come from the backend so the launch
// refusal, the Play badge and this card all describe one disc. Never "custom": this is a fault to
// repair, not a preference anybody expressed.
let unverified = null;
// Every change not saved yet (spec §7), and the bar that shows them.
const draft = createDraft();
let saveBar = null, lastChangedId = "";
// Launcher window (spec §4.3, §5.3): previews live, saved through the same bar.
let look = null, lookError = null, lookSaved = null, lookDraft = null, lookBusy = false, lookMsg = "";

function ok(x) {
  return x && !x.error;
}

async function load() {
  const reads = {
    dolphin: call("dolphin_preferences", { runtime: get("runtime") }),
    graphics: call("setup_graphics_options"),
    hardware: call("setup_hardware_info"),
    progress: call("onboarding_progress"),
    preferences: call("settings_paths"),
    installation: call("setup_install_info"),
    paths: call("paths"),
    check: call("launch_check", { runtime: get("runtime"), game: get("game"), userDir: get("userDir") }),
    identity: call("identity_status"),
    policy: call("update_policy"),
    queue: call("stats_queue"),
    content: call("content_status"),
  };
  const keys = Object.keys(reads);
  const settled = await Promise.allSettled(Object.values(reads));
  keys.forEach((k, i) => {
    state[k] = settled[i].status === "fulfilled" ? settled[i].value : { error: reason(settled[i].reason) };
  });
}

// ---- the save bar -------------------------------------------------------------------------------

function refreshBar() {
  const changes = draft.changes();
  if (!saveBar && !changes.length) return;
  if (!saveBar) {
    const lane = document.getElementById("save-lane");
    if (!lane) return;
    saveBar = mountSaveBar(lane, { onSave: saveAll, onDiscard: discardAll, returnFocus: () => document.getElementById(lastChangedId) });
  }
  saveBar.update(changes);
}

// Record one change; `focusId` is the control that made it, so focus goes back to it after the redraw.
function stage(key, focusId, entry) {
  draft.put(key, entry);
  if (focusId) lastChangedId = focusId;
  draw();
  refreshBar();
  if (focusId) document.getElementById(focusId)?.focus({ preventScroll: true });
}

// Save everything, in the order it changed. A failure is thrown so the bar says which and why; a
// change held for the Tier 2 choice is not a failure (the dialog asks), and it stays pending.
async function saveAll() {
  window.setLauncherBusy?.(true);
  let result;
  try { result = await draft.saveAll(); }
  finally {
    await load();
    window.setLauncherBusy?.(false);
    draw();
    refreshBar();
  }
  if (result.stopped && result.stopped.reason !== HELD) throw new Error(`${result.stopped.label}: ${result.stopped.reason}`);
}

async function discardAll() {
  const failed = await draft.discardAll();
  unverified = null;
  draw();
  refreshBar();
  if (failed.length) throw new Error(failed.join(" · "));
}

// ---- building blocks ----------------------------------------------------------------------------

// One glass section: a title, an optional lead line, then rows (mockup .section).
function section(title, lead, ...children) {
  return el("section", { class: "glass pref-section" }, el("h2", { text: title }), lead ? el("p", { class: "pref-lead", text: lead }) : null, ...children);
}

// One row: label | value | control, with the help line under the value (mockup .row).
function row(label, { id = null, cls = "", value = null, control = null, help = null } = {}) {
  return el("div", { class: "pref-row" + (cls ? " " + cls : "") },
    id ? el("label", { class: "pref-label", for: id, text: label }) : el("span", { class: "pref-label", text: label }),
    el("div", { class: "pref-value" }, value),
    el("div", { class: "pref-control" }, ...[].concat(control)),
    help ? el("p", { class: "pref-help", text: help }) : null);
}

function toggle(id, label, on, disabled, onFlip) {
  return el("button", { type: "button", id, class: "look-switch", role: "switch", "aria-label": label, "aria-checked": String(on), disabled, onclick: onFlip });
}
const onOff = (on) => el("span", { class: "pref-state", text: on ? "On" : "Off" });
const mb = (b) => Math.max(1, Math.round((b || 0) / 1048576));

async function act(label, cmd, args, after) {
  if (busy) return;
  const focusedId = document.activeElement?.id;
  busy = true;
  problem = null;
  if (window.setLauncherBusy) window.setLauncherBusy(true); else draw();
  const messages = { update_check: "Checking for updates…", update_download: "Downloading and verifying…", update_apply: "Starting installer…", content_check: "Checking game files…", content_fetch: "Downloading game content…", content_promote: "Installing game content…", content_repair_image: "Repairing game files…" };
  if (messages[cmd]) document.getElementById("screen").append(el("p", { class: "settings-status", role: "status", text: messages[cmd] }));
  try {
    const r = await call(cmd, args);
    after(r);
  } catch (e) {
    problem = `${label}: ${reason(e)}`;
  }
  await load();
  busy = false;
  window.setLauncherBusy?.(false);
  draw();
  if (focusedId) document.getElementById(focusedId)?.focus({ preventScroll: true });
}

// ---- the Dolphin card ---------------------------------------------------------------------------

function dolphinCard() {
  return el("section", { class: "glass dolphin-card", "aria-label": "NFL Street Reborn Dolphin" },
    el("span", { class: "dolphin-icon", "aria-hidden": "true" }, icon(["M5 4h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z", "M8 21h8M12 17v4"])),
    el("div", { class: "dolphin-text" },
      el("h2", { text: "NFL Street Reborn Dolphin" }),
      el("p", { role: "status", text: dolphinSaid || "Everyday settings are here. Open Dolphin for its full set of tools." })),
    el("button", { type: "button", class: "btn", text: "Open Dolphin", onclick: openDolphin }));
}

async function openDolphin() {
  try {
    await call("open_runtime", { runtime: get("runtime"), userDir: get("userDir") });
    dolphinSaid = "Dolphin opened.";
    await refresh();
  } catch (e) { dolphinSaid = reason(e); draw(); }
}

// ---- Dolphin's options (General, Graphics, Display, Audio, Saves) --------------------------------

const preferenceGroups = {
  General: [["confirm_stop", "Confirm before stopping", "Ask before closing a game.", "bool"], ["pause_unfocused", "Pause when unfocused", "Pause local play when you switch to another app.", "bool"], ["osd_messages", "Game notifications", "Show Dolphin messages over the game.", "bool"]],
  Graphics: [["internal_resolution", "Internal resolution", "How sharp the game renders. 3× is the Reborn default; lower it if the game runs slowly.", "resolution"], ["msaa", "Anti-aliasing", "Smooths edges. Off is the Reborn default and is what the game has always run at; raising it costs frame rate and softens the depth-based ReShade effects.", "msaa"], ["video_backend", "Graphics backend", "Direct3D 11 is the Reborn default. Vulkan or OpenGL can run better on some drivers, but the ReShade visual effects hook Direct3D, so they do not load on the other backends.", "video_backend"], ["sync_gpu", "Synchronize GPU thread", "Off is faster. Turn it on only if you see graphical glitches.", "bool"], ["vsync", "V-Sync", "Reduce screen tearing. May add a small amount of input delay.", "bool"], ["show_fps", "Frame rate", "Show a small frames-per-second counter.", "bool"]],
  Display: [["fullscreen", "Start in fullscreen", "Open the game in fullscreen mode.", "bool"], ["show_ping", "Netplay ping", "Show connection latency during online play.", "bool"], ["always_on_top", "Keep game on top", "Keep the game window above other windows.", "bool"]],
  Audio: [["audio_backend", "Audio output", "Default shares audio with other apps. Windows exclusive gives Dolphin sole use of the output device.", "backend"], ["volume", "Game volume", "Adjust Dolphin audio output.", "range", 0, 100], ["muted", "Mute audio", "Silence game audio without changing the volume.", "bool"], ["audio_stretch", "Audio stretching", "Smooth out audio during brief slowdowns.", "bool"], ["audio_latency", "Audio buffer", "Audio buffering in milliseconds. Raise it if sound crackles.", "range", 0, 200]],
  Saves: [["slot_a", "Memory Slot A", "Choose where Player 1 saves game progress.", "slot"], ["card_a", "Slot A memory card", "Used when Slot A is set to Memory card. Leave blank for Dolphin’s default.", "card"], ["gci_a", "Slot A save folder", "Used when Slot A is set to GCI folder. Leave blank for the default folder.", "folder"], ["slot_b", "Memory Slot B", "An optional second card. Existing saves are kept when you change the type.", "slot"], ["card_b", "Slot B memory card", "Used when Slot B is set to Memory card.", "card"], ["gci_b", "Slot B save folder", "Used when Slot B is set to GCI folder.", "folder"]],
};

// The value a control shows: the pending one if there is one, else what Dolphin has.
function prefValue(key) { return String(draft.value("pref." + key, state.dolphin?.values?.[key] ?? "")); }

function stagePref(key, label, type, value, focusId) {
  const saved = String(state.dolphin.values[key] ?? "");
  stage("pref." + key, focusId, { label, value,
    same: type === "bool" ? value.toLowerCase() === saved.toLowerCase() : value === saved,
    save: async (v) => { await call("dolphin_preference_set", { runtime: get("runtime"), key, value: v }); state.dolphin.values[key] = v; } });
}

// Volume and mute reach a running game at once (spec §10: what they have always done), so while a game
// is open they are not held for the save bar.
async function saveLive(key, label, value, focusId) {
  let said;
  try { await call("dolphin_preference_set", { runtime: get("runtime"), key, value }); state.dolphin.values[key] = value; said = `${label} saved.`; }
  catch (e) { said = reason(e); }
  draw();
  const status = document.getElementById("dolphin-status");
  if (status) status.textContent = said;
  document.getElementById(focusId)?.focus({ preventScroll: true });
}

function selectFor(type, id, current, disabled) {
  const choices = {
    backend: [["Cubeb", "Default (shared audio)"], ["WASAPI (Exclusive Mode)", "Windows exclusive"]],
    slot: [["8", "GCI folder"], ["1", "Memory card"], ["255", "Nothing"]],
    resolution: [["1", "1× native (640×528)"], ["2", "2×"], ["3", "3× (Reborn default)"], ["4", "4×"], ["5", "5×"], ["6", "6×"]],
    video_backend: [["D3D", "Direct3D 11 (Reborn default)"], ["D3D12", "Direct3D 12"], ["Vulkan", "Vulkan"], ["OGL", "OpenGL"]],
    msaa: [["1", "Off (Reborn default)"], ["2", "2×"], ["4", "4×"], ["8", "8×"]],
  }[type] || [];
  const v = type === "resolution" ? String(parseInt(current) || 3) : type === "msaa" ? String(parseInt(current) || 1) : current;
  const select = el("select", { id, disabled }, ...choices.map(([value, text]) => el("option", { value, text })));
  if (!choices.some(([value]) => value === v)) select.append(el("option", { value: v, text: type === "slot" ? `Current device (${v})` : type === "resolution" || type === "msaa" ? `${v}×` : v }));
  select.value = v;
  return select;
}

function prefRow(key, label, description, type, min, max, disabled, isLive) {
  const id = "preference-" + key, current = prefValue(key);
  const commit = (value, focusId = id) => isLive ? saveLive(key, label, value, focusId) : stagePref(key, label, type, value, focusId);
  if (type === "bool") {
    const on = current.toLowerCase() === "true";
    return row(label, { id, value: onOff(on), control: toggle(id, label, on, disabled, () => commit(String(!on))), help: description });
  }
  if (type === "range") {
    const unit = key === "volume" ? "%" : " ms";
    const slider = el("input", { id, type: "range", min, max, step: 1, value: current, disabled });
    const out = el("output", { for: id, class: "pref-state", text: current + unit });
    slider.addEventListener("input", () => { out.textContent = slider.value + unit; });
    slider.addEventListener("change", () => commit(slider.value));
    return row(label, { id, value: slider, control: out, help: description });
  }
  if (type === "card" || type === "folder") {
    const pick = async () => {
      try { const p = await call("pick_path", { kind: type === "card" ? "memory_card" : "folder" }); if (p) commit(p, `change-${key}`); }
      catch (e) { problem = reason(e); draw(); }
    };
    return row(label, { id, cls: "path-entry",
      value: el("div", { class: "field" }, el("input", { id, type: "text", readonly: "", value: current, placeholder: "Use default" })),
      control: [
        el("button", { type: "button", id: `change-${key}`, class: "btn", text: "Change…", "aria-label": `Change ${label}`, disabled, onclick: pick }),
        current ? el("button", { type: "button", class: "btn quiet sm", text: "Reset", "aria-label": `Reset ${label} to the default`, disabled, onclick: () => commit("", `change-${key}`) }) : null,
      ], help: description });
  }
  const select = selectFor(type, id, current, disabled);
  select.addEventListener("change", () => commit(select.value));
  return row(label, { id, control: select, help: description });
}

async function openSaves() {
  try { await call("dolphin_saves_folder", { runtime: get("runtime") }); }
  catch (e) { const s = document.getElementById("dolphin-status"); if (s) s.textContent = reason(e); }
}

function dolphinBlock() {
  const title = settingsTab === "Saves" ? "Memory cards & save data" : settingsTab === "Graphics" ? "Rendering" : settingsTab;
  if (!ok(state.dolphin)) return section(title, null, el("p", { class: "partial", text: state.dolphin?.error || "Dolphin settings could not be read." }));
  const d = state.dolphin, values = d.values || {}, live = d.live_editable || [];
  const canEdit = (key) => d.editable !== false || live.includes(key);
  const lead = el("p", { class: "pref-lead", id: "dolphin-status", role: "status", text: settingsTab === "Audio" && live.length
    ? "Volume and mute change as you set them, even during NetPlay. Everything else waits for Save changes."
    : "Changes wait for Save changes and apply the next time you open Dolphin." });
  const rows = [];
  if (d.editable === false) rows.push(el("p", { class: "partial", text: d.readonly_reason || "Close Dolphin to edit these settings, then refresh." }));
  for (const [key, label, description, type, min, max] of preferenceGroups[settingsTab] || []) {
    if (key === "audio_latency" && !["WASAPI (Exclusive Mode)", "OpenAL"].includes(prefValue("audio_backend"))) continue;
    if (values[key] === undefined) { rows.push(el("p", { class: "partial", text: label + " is unavailable. Refresh settings to try again." })); continue; }
    rows.push(prefRow(key, label, description, type, min, max, busy || !canEdit(key), live.includes(key)));
  }
  if (settingsTab === "Saves") rows.push(row("Save folder", { control: el("button", { type: "button", class: "btn", text: "Open save folder", onclick: openSaves }) }));
  return el("section", { class: "glass pref-section" }, el("h2", { text: title }), lead, ...rows);
}

// ---- Graphics ------------------------------------------------------------------------------------

const GRAPHICS_PRESETS = [["performance", "Performance"], ["balanced", "Balanced"], ["quality", "All-Out"]];
function graphicsBlock() {
  const parts = [];
  if (!ok(state.graphics)) parts.push(section("Visual effects", null, el("p", { class: "partial", text: state.graphics?.error || "Graphics settings are unavailable. Refresh settings to try again." })));
  else {
    const saved = get("preset") || state.progress?.summary?.graphics || "balanced";
    const current = state.graphics.current;
    const selected = current ? (current.label === "Previous settings" ? "previous" : "custom") : (saved === "ultra" ? "quality" : GRAPHICS_PRESETS.some((x) => x[0] === saved) ? saved : "balanced");
    const chosen = draft.value("graphics", selected);
    const select = el("select", { id: "graphics-preset", disabled: busy || state.dolphin?.editable === false }, ...GRAPHICS_PRESETS.map(([value, text]) => el("option", { value, text })));
    if (state.graphics.previous || selected === "previous") select.append(el("option", { value: "previous", text: "Previous settings" }));
    if (selected === "custom") select.append(el("option", { value: "custom", text: "Custom", disabled: true }));
    select.value = chosen;
    select.addEventListener("change", () => stage("graphics", "graphics-preset", { label: "Graphics preset", value: select.value, same: select.value === selected,
      save: async (v) => {
        await call("setup_save_graphics_options", { mode: v === "previous" ? "previous" : "preset", resolution: 3, msaa: 1, ssaa: false });
        if (v !== "previous") set("preset", v);
      } }));
    // The ReShade descriptions were stale (off since 3.0.20); the row says what the choice does now.
    const help = chosen === "previous" ? "Use the graphics settings imported from your previous installation." : chosen === "custom" ? "Your saved custom settings are selected." : "Applies the next time you play.";
    const rows = [row("Graphics preset", { id: "graphics-preset", control: select, help })];
    if (ok(state.hardware) && state.hardware.description) {
      const recommended = GRAPHICS_PRESETS.find((x) => x[0] === state.hardware.recommended)?.[1];
      rows.push(row("Your PC", { value: state.hardware.description, control: recommended ? el("span", { class: "pref-badge", text: `${recommended} recommended` }) : null }));
    }
    parts.push(section("Visual effects", null, ...rows));
  }
  parts.push(dolphinBlock());
  return parts;
}

// ---- Display: the launcher window ------------------------------------------------------------------

// One guarded step: ignored while another is in flight; a failure is shown, never thrown.
async function lookStep(id, fn) {
  if (lookBusy) return;
  lookBusy = true; lookMsg = "";
  try { await fn(); } catch (e) { lookMsg = reason(e); }
  finally {
    lookBusy = false;
    if (id) lastChangedId = id;
    draw();
    refreshBar();
    if (id) document.getElementById(id)?.focus({ preventScroll: true });
  }
}

const glassOnNow = () => document.documentElement.dataset.glass === "on";

const flipGlass = () => lookStep("look-glass", async () => {
  const next = !lookDraft.windowGlass;
  const v = await call("window_glass_apply", { enabled: next });
  look = v; lookDraft.windowGlass = next;
  applyLook({ ...v, slowAnimations: lookDraft.slowAnimations });
  draft.put("look.glass", { label: "Window glass", value: next, same: next === lookSaved.windowGlass,
    save: async (value) => {
      const saved = await call("launcher_look_set", { windowGlass: value, slowAnimations: null });
      look = saved; lookSaved.windowGlass = saved.windowGlass;
      applyLook({ ...saved, slowAnimations: lookDraft.slowAnimations });
    },
    revert: async () => {
      const back = await call("window_glass_apply", { enabled: lookSaved.windowGlass });
      look = back; lookDraft.windowGlass = lookSaved.windowGlass;
      applyLook({ ...back, slowAnimations: lookDraft.slowAnimations });
    } });
});

const flipMotion = () => lookStep("look-motion", async () => {
  lookDraft.slowAnimations = !lookDraft.slowAnimations;
  applyLook({ glassApplied: glassOnNow(), slowAnimations: lookDraft.slowAnimations });
  draft.put("look.motion", { label: "Slow animations", value: lookDraft.slowAnimations, same: lookDraft.slowAnimations === lookSaved.slowAnimations,
    save: async (value) => {
      const saved = await call("launcher_look_set", { windowGlass: null, slowAnimations: value });
      look = saved; lookSaved.slowAnimations = saved.slowAnimations;
      applyLook({ glassApplied: glassOnNow(), slowAnimations: lookDraft.slowAnimations });
    },
    revert: async () => {
      lookDraft.slowAnimations = lookSaved.slowAnimations;
      applyLook({ glassApplied: glassOnNow(), slowAnimations: lookSaved.slowAnimations });
    } });
});

function lookBlock() {
  if (!look && lookError) return el("p", { class: "partial", role: "alert", text: "Couldn’t read the launcher window settings: " + reason(lookError) });
  if (!look) return el("p", { class: "partial", text: "Reading the launcher window settings…" });
  const glassOn = lookDraft.windowGlass && look.glassSupported;
  const failed = look.glassSupported && lookDraft.windowGlass && !look.glassApplied;
  return el("section", { class: "glass pref-section look-section", "aria-label": "Launcher window" },
    el("h2", { text: "Launcher window" }),
    el("p", { class: "pref-lead", text: "How the launcher itself looks. The game is not affected." }),
    row("Window glass", { value: onOff(glassOn), control: toggle("look-glass", "Window glass", glassOn, !look.glassSupported, flipGlass),
      help: failed ? "The window glass could not be turned on." : look.glassNote || "Frosted glass that picks up your desktop colours. Turn it off for a solid dark background." }),
    row("Slow animations", { value: onOff(lookDraft.slowAnimations), control: toggle("look-motion", "Slow animations", lookDraft.slowAnimations, false, flipMotion),
      help: "Slower, smoother movement when you change pages, open menus and expand matches. Turn it off for quick animations. Windows’ reduce-motion setting always turns movement off." }),
    lookMsg ? el("p", { class: "look-error", role: "status", text: lookMsg }) : null);
}

async function loadLook() {
  const r = await refreshLookResult();
  look = r.view; lookError = r.error;
  if (look) { lookSaved = { windowGlass: look.windowGlass, slowAnimations: look.slowAnimations }; lookDraft = { ...lookSaved }; }
  draw();
}

// ---- Updates ---------------------------------------------------------------------------------------

// Textures, the emulator, mods, game patches and the roster: the content system. The Play page checks
// on start and installs when the game is closed; this is where a person looks, checks, repairs, or
// installs now.
function contentBlock() {
  const c = state.content;
  let s;
  if (!ok(c)) s = section("Game content", null, el("p", { class: "partial", text: c?.error || "Content status is unavailable. Refresh settings to try again." }));
  else {
    const working = ["checking", "fetching", "installing"].includes(c.phase);
    const labels = { checking: "Checking for updates", fetching: "Downloading content", installing: "Installing content", available: "Update available", staged: "Ready to install", installed: "Content updated", problem: "Needs attention" };
    let action;
    if (c.phase === "available") action = el("button", { type: "button", class: "btn primary", text: `Download ${mb(c.download_bytes)} MB`, disabled: busy, onclick: () => act("Download content", "content_fetch", {}, () => {}) });
    else if (c.phase === "staged") action = el("button", { type: "button", class: "btn primary", text: "Install now", disabled: busy, onclick: () => act("Install content", "content_promote", { confirm: true }, () => {}) });
    else if (c.phase === "installed") action = el("button", { type: "button", class: "btn", text: "Dismiss summary", disabled: busy, onclick: () => act("Dismiss", "content_dismiss", {}, () => {}) });
    else action = el("button", { type: "button", class: "btn", text: working ? "Working…" : "Check game content", disabled: busy || working, onclick: () => act("Check content", "content_check", { depth: null }, () => {}) });
    const details = (c.components || []).filter((x) => x && ["available", "refused", "problem"].includes(x.state));
    s = section("Game content", null,
      row(labels[c.phase] || "Game content updates", { control: action, help: c.note || "Keep your textures, game patches, roster, and Dolphin up to date." }),
      working ? el("progress", { class: "settings-progress", "aria-label": labels[c.phase], ...(c.phase === "fetching" && c.download_bytes ? { value: String(Math.min(c.fetched_bytes || 0, c.download_bytes)), max: String(c.download_bytes) } : {}) }) : null,
      details.length ? el("ul", { class: "pref-list" }, ...details.map((comp) => el("li", { text: comp.state === "available" ? `${comp.component}: ${comp.repair ? "repair" : `${comp.installed ? comp.installed + " → " : ""}${comp.offered}`}, ${mb(comp.bytes)} MB` : `${comp.component}: ${comp.message || comp.state}` }))) : null,
      row("Verify game files", { control: el("button", { type: "button", class: "btn", text: "Verify and repair", disabled: busy || working, onclick: () => act("Verify content", "content_check", { depth: "verify" }, () => {}) }), help: "Check installed content and prepare repairs for missing or changed files." }),
      // The image itself, not the components: walks the game to the newest build and re-applies the
      // roster. 09-26: a player's game-patch component was current while his image was still an old
      // build, so his online copy never matched anyone's and he had no way to fix it from here.
      row("Repair game files", { control: el("button", { type: "button", class: "btn", text: "Repair game files", disabled: busy || working, onclick: () => act("Repair game files", "content_repair_image", {}, (r) => { problem = r?.note || null; }) }), help: "Bring the game image to the newest build and re-apply the roster, if an update left it behind." }));
  }
  s.id = "settings-game-content";
  return s;
}

function updatesBlock() {
  const parts = [];
  if (ok(state.policy)) {
    const automatic = draft.value("updates", state.policy.automatic);
    const policy = el("select", { id: "update-policy", disabled: busy },
      el("option", { value: "automatic", text: "Keep me up to date" }), el("option", { value: "ask", text: "Ask me first" }));
    policy.value = automatic ? "automatic" : "ask";
    policy.addEventListener("change", () => {
      const next = policy.value === "automatic";
      stage("updates", "update-policy", { label: "Automatic updates", value: next, same: next === state.policy.automatic,
        save: async (v) => { state.policy = await call("update_policy_set", { automatic: v }); } });
    });
    parts.push(section("Update preferences", null, row("Automatic updates", { id: "update-policy", control: policy,
      value: automatic ? el("span", { class: "pref-badge", text: "Recommended" }) : null,
      help: automatic ? "Download and install verified updates automatically. See a summary when installation is complete." : "Nothing is downloaded until you say so." })));
  } else parts.push(section("Update preferences", null, el("p", { class: "partial", text: state.policy?.error || "Your update preference is unavailable." })));
  const actions = [el("button", { type: "button", class: "btn", text: "Check for updates", disabled: busy, onclick: () => act("Check for updates", "update_check", { source: null }, (r) => { update = r; downloaded = null; }) })];
  if (downloaded?.path) actions.push(el("button", { type: "button", class: "btn primary", text: "Install and restart", disabled: busy, onclick: () => act("Install update", "update_apply", { path: downloaded.path, confirm: true }, (r) => { update = { ...update, note: r.note }; }) }));
  else if (update?.state === "available") actions.push(el("button", { type: "button", class: "btn primary", text: `Download ${update.latest}`, disabled: busy, onclick: () => act("Download update", "update_download", { source: null }, (r) => { downloaded = r; if (r.check) update = r.check; }) }));
  // What changed lives on the site, not in a paragraph here.
  actions.push(el("button", { type: "button", class: "btn quiet", text: "Patch notes", onclick: () => call("open_website", { url: PATCH_NOTES_URL }).catch(() => {}) }));
  // Two presses, on purpose: one press on a pad, or a stray Enter, must not remove the launcher. The
  // first arms the button for a few seconds and says so; the second does it.
  const uninstall = el("button", { type: "button", class: "btn", text: "Uninstall…", disabled: busy, onclick: () => {
    if (uninstall.dataset.armed) { act("Uninstall", "uninstall_launcher", { confirm: true }, () => {}); return; }
    uninstall.dataset.armed = "true"; uninstall.textContent = "Press again to remove";
    setTimeout(() => { if (!uninstall.isConnected) return; delete uninstall.dataset.armed; uninstall.textContent = "Uninstall…"; }, 8000);
  } });
  parts.push(section("Launcher", null,
    row(downloaded?.path ? "Ready to install" : update?.state === "available" ? `Version ${update.latest} available` : "Launcher updates", { control: actions,
      help: downloaded?.path ? `Downloaded and verified · ${mb(downloaded.bytes)} MB. Restart the launcher to install.` : update?.note || "Check for a new launcher version. Your games and saves are preserved." }),
    row("Remove NFL Street Reborn", { control: uninstall, help: "Removes the launcher and the game from this PC. Your saves stay." })));
  parts.push(contentBlock());
  return parts;
}

// ---- Files & storage ---------------------------------------------------------------------------------

const BOXES = [
  { key: "game", setting: "game", name: "Game image", hint: "Your game image. Reset to find it automatically." },
  { key: "legacyRoot", setting: "legacy_root", name: "Existing install", hint: "Your earlier Reborn installation, containing the User and MODS folders." },
  { key: "runtime", setting: "runtime", name: "Emulator", hint: "The bundled Reborn Dolphin is selected automatically on Windows." },
  { key: "userDir", setting: "user_dir", name: "User folder", hint: "Contains your saves and Dolphin settings. Reset to use the detected folder." },
];

// Where each location resolves when nothing is chosen, from the commands that know it.
function detected(key) {
  if (key === "game") return ok(state.check) ? state.check.game : null;
  if (key === "runtime") return ok(state.check) ? state.check.runtime : null;
  if (key === "userDir") return ok(state.dolphin) ? state.dolphin.user_dir : null;
  return null;
}
function savedPath(key) { return (ok(state.preferences) ? state.preferences[key] : get(key)) || ""; }

// Label | field (value + Auto while nothing is chosen) | Change… (spec §8.2). Saving goes through the bar.
function pathRow(b) {
  const key = "path." + b.key, id = `path-${b.key}`, saved = savedPath(b.key);
  const value = draft.value(key, saved);
  const stagePath = (v) => stage(key, `change-${b.key}`, { label: b.name, value: v, same: v === saved,
    save: async (next) => {
      const r = await call("settings_set_path", { which: b.setting, value: next });
      // Nothing was stored: the image is NFL Street 2 but not a Reborn build. The choice is put to the
      // player (Repair first), and this change stays pending until they answer.
      if (r && r.unverified) { unverified = { ...r.unverified, which: b.setting, key: b.key, value: next }; return HELD; }
      unverified = null;
      set(b.key, next);
    } });
  const change = async () => {
    if (busy) return;
    try {
      const picked = await call("pick_path", { kind: b.key === "game" ? "game" : b.key === "runtime" ? "runtime" : "folder" });
      if (picked) stagePath(picked);
    } catch (e) { problem = reason(e); draw(); }
  };
  return row(b.name, { id, cls: "path-entry",
    value: el("div", { class: "field" },
      el("input", { id, type: "text", readonly: "", value: value || detected(b.key) || "Found automatically", "aria-describedby": value ? null : `${id}-auto` }),
      value ? null : el("span", { class: "auto", id: `${id}-auto`, text: "Auto" })),
    control: [
      el("button", { type: "button", id: `change-${b.key}`, class: "btn", text: "Change…", "aria-label": `Change ${b.name}`, disabled: busy, onclick: change }),
      value ? el("button", { type: "button", class: "btn quiet sm", text: "Reset", "aria-label": `Reset ${b.name} to automatic`, disabled: busy, onclick: () => stagePath("") }) : null,
    ],
    help: b.hint });
}

// The repair every refusal points at: the same download the setup wizard offers, on the page the Play
// badge and the launch refusal send the player to.
async function repairGame() {
  await act("Repair my game", "setup_download_reborn_game", {}, () => { unverified = null; draft.drop("path.game"); refreshBar(); });
}

// "Play it anyway": the escape hatch, taken deliberately, for this one image.
// This is the ONLY call in the launcher that sets acknowledgeUnverified.
async function playItAnyway(choice) {
  await act(choice.play_anyway, "settings_set_path", { which: choice.which, value: choice.value, acknowledgeUnverified: true }, () => {
    unverified = null;
    set(choice.key, choice.value);
    draft.drop("path." + choice.key);
    refreshBar();
  });
}

// The Tier 2 choice, drawn as a modal. Repair is primary; the escape hatch is not. Cancel leaves the
// change pending (Discard drops it).
function unverifiedChoice() {
  const choice = unverified;
  if (!choice) return null;
  return el("dialog", { id: "unverified-game", class: "settings-choice", role: "alertdialog", "aria-labelledby": "unverified-game-title" },
    el("h2", { id: "unverified-game-title", text: choice.headline }),
    el("p", { text: choice.detail }),
    el("div", { class: "settings-actions" },
      el("button", { type: "button", class: "btn quiet", text: "Cancel", disabled: busy, onclick: () => { unverified = null; draw(); } }),
      el("button", { type: "button", class: "btn", text: choice.play_anyway, disabled: busy, onclick: () => playItAnyway(choice) }),
      el("button", { type: "button", class: "btn primary", "data-primary": "true", text: choice.repair, disabled: busy, onclick: () => repairGame() })));
}

// The standing repair, for a player whose saved game file is already an unconverted disc.
function repairBlock() {
  if (!ok(state.check) || state.check.game_verdict !== "unconverted") return null;
  const s = section("Your game file isn't NFL Street Reborn.", "This is NFL Street 2, but not a Reborn build, so rosters, faces and stats won't match. This usually means setup couldn't convert your disc.",
    row("Repair", { control: el("button", { type: "button", class: "btn primary", text: "Repair my game", disabled: busy, onclick: () => repairGame() }) }));
  s.id = "repair-game";
  return s;
}

function foundBlock() {
  const c = state.check;
  if (!ok(c)) return section("Detected files", null, partial(`Could not check: ${c ? c.error : "not loaded"}`));
  if (!c.ready) return section("Detected files", null, refusal(c.problem || "Your installation needs attention before you can play."));
  const release = state.installation?.installed_release?.release;
  return section("Detected files", null, ...(release ? [row("Previous release", { value: release })] : []), row("Emulator", { value: c.runtime }), row("Game file", { value: c.game }));
}

function whereBlock() {
  const p = state.paths;
  if (!ok(p)) return section("Storage", null, partial(`Could not read: ${p ? p.error : "not loaded"}`));
  const dirs = Array.isArray(p.directories) ? p.directories : [];
  const present = dirs.filter((d) => d.exists).map((d) => d.relative);
  return section("Storage", null,
    row("Channel", { value: p.channel_note || p.channel }),
    row("Program files", { value: p.install_root }),
    row("Your data", { value: p.data_root }),
    row("Save data", { value: p.saves_survive_uninstall ? "Kept separately from the application." : "Stored inside the application folder." }),
    row("Folders in use", { value: present.length ? present.join(", ") : "No data folders exist yet." }));
}

function filesBlock() {
  return [repairBlock(), unverifiedChoice(),
    section("Game & installation", "Found automatically. Choose a different location only to override it.", ...BOXES.map(pathRow),
      row("Game setup", { control: el("a", { href: "onboarding.html", class: "btn", text: "Resume game setup" }) })),
    foundBlock(), whereBlock()];
}

// ---- the page ------------------------------------------------------------------------------------------

function pickTab(name) {
  if (busy) return;
  settingsTab = name;
  draw();
  document.querySelector?.('.tabs .tab[aria-pressed="true"]')?.focus({ preventScroll: true });
}

function draw() {
  const screen = document.getElementById("screen"), parts = [];
  if (problem) parts.push(refusal(problem));
  parts.push(dolphinCard());
  parts.push(tabs("settings", TABS, settingsTab, pickTab, { disabled: busy, label: "Settings sections" }));
  const body = settingsTab === "Updates" ? updatesBlock() : settingsTab === "Graphics" ? graphicsBlock()
    : settingsTab === "Files & storage" ? filesBlock() : settingsTab === "Display" ? [lookBlock(), dolphinBlock()] : [dolphinBlock()];
  parts.push(...body);
  parts.push(el("button", { type: "button", class: "btn quiet sm refresh", text: "Refresh settings", disabled: busy, onclick: refresh }));
  // repairBlock and unverifiedChoice draw nothing most of the time, and a null handed to
  // replaceChildren becomes the word "null" on the page.
  screen.replaceChildren(...parts.filter(Boolean));
  // A real browser hides a <dialog> until it is opened.
  if (unverified) document.getElementById("unverified-game")?.showModal?.();
}

async function refresh() {
  problem = null;
  try {
    await load();
  } catch (e) {
    problem = reason(e);
  }
  draw();
}

// Dolphin can close while this page stays open, including when it was opened outside the launcher.
// Refresh availability without rebuilding controls on every poll or overwriting a slider in use.
let pollingDolphin = false;
async function refreshDolphinAvailability() {
  if (pollingDolphin || busy || document.hidden || ["Updates", "Files & storage"].includes(settingsTab) || !state.dolphin) return;
  pollingDolphin = true;
  try {
    const next = await call("dolphin_preferences", { runtime: get("runtime") });
    if (busy || !next || ["Updates", "Files & storage"].includes(settingsTab)) return;
    const availability = (value) => JSON.stringify([value.editable, value.live_editable, value.readonly_reason]);
    const changed = availability(next) !== availability(state.dolphin);
    state.dolphin = next;
    if (changed) { const focused = document.activeElement?.id; draw(); if (focused) document.getElementById(focused)?.focus({ preventScroll: true }); }
  } catch (_) { /* A failed read is not evidence that Dolphin stopped. */ }
  finally { pollingDolphin = false; }
}
document.addEventListener("session-state", refreshDolphinAvailability);
window.addEventListener("focus", refreshDolphinAvailability);

// A content check or download can outlive the page that started it. Update only its section so the
// preference being edited keeps its focus.
let pollingContent = false;
setInterval(async () => {
  if (pollingContent || busy || settingsTab !== "Updates" || document.hidden || !["checking", "fetching", "installing"].includes(state.content?.phase)) return;
  pollingContent = true;
  try {
    const next = await call("content_status");
    if (busy || settingsTab !== "Updates") return;
    if (JSON.stringify(next) !== JSON.stringify(state.content)) {
      state.content = next;
      document.getElementById("settings-game-content")?.replaceWith(contentBlock());
    }
  } catch (e) { problem = reason(e); }
  finally { pollingContent = false; }
}, 2000);

const pill = document.getElementById("bridge");
if (pill) {
  pill.textContent = bridge ? "Launcher connected" : "Preview";
  pill.className = "pill " + (bridge ? "ok" : "bad");
}
markCurrent();
wireCoreFold();
loadLook();
if (bridge) {
  refresh();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}
```

Notes for the implementer (do not skip):
- **`foundBlock` labels its row "Game file", not "Game image".** Only the path row carries the label "Game image", so `getByLabel('Game image')` stays unique.
- **Every command's arguments are written as an object literal at the call** (including inside `save:` callbacks). `check-page-call-args` reads the literal after the command name.
- **The Files tab text must not contain the word "custom"** (`check-shell-pages`: "never as a preference").

- [ ] **Step 7: Run the checks**

- `node Tools/check-settings-page.mjs`: PASS.
- `node Tools/check-look-browser.mjs`: PASS. The glass preview + Discard, motion Save + reload, unsupported/failed/rejected states, and the "Couldn’t read…" failure all still hold.
- `node Tools/check-shell-pages.mjs`: PASS. This includes the Tier 2 driven test and "Settings explains why editing is unavailable".
- `node Tools/check-page-call-args.mjs`: PASS.
- `node Tools/check-session-state.mjs`: PASS. The settings sliders still lock and unlock with external Dolphin, and focus is kept.
- `node Tools/check-theme.mjs` and `node Tools/check-theme-tokens.mjs`: PASS (2 page stylesheets).
- The rest of the standing checks: PASS. The `run-browser-tests` main body covers the graphics preset via Save changes, Change…/Save/reload, the failed Reset, and pad navigation on Files.

Open `target/glass-review/settings-files-1280.png` and `settings-800.png` against the mockup's SETTINGS section.

- [ ] **Step 8: Commit**

Append `await import('./check-settings-page.mjs');` to the end of `Tools/run-browser-tests.mjs`.

```bash
git add crates/reborn-ui/ui/settings.html crates/reborn-ui/ui/pages/settings.css crates/reborn-ui/ui/settings.js crates/reborn-ui/ui/theme.css Tools/check-theme.mjs Tools/check-shell-pages.mjs Tools/run-browser-tests.mjs Tools/check-settings-page.mjs
git commit -m "settings: theme-only page; Dolphin card once, sliding tabs, glass rows, every change through the save bar

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Spec §13 for Play and Settings: screens, alignment, motion

**Files:**
- Create: `Tools/check-glass-pages.mjs`
- Modify: `Tools/run-browser-tests.mjs` (one import)

**Interfaces:**
- Consumes everything above.
- Produces screenshots in `target/glass-review/` (or the directory given as `argv[2]`) named `play-<w>-<glass|solid>-<slow|quick>.png`, plus `play-menu-…`, `play-pad-…`, `settings-…` and `settings-bar-…`.

- [ ] **Step 1: Write the check**

```js
// Spec §13 for Play and Settings: screens at 1280×800 and 800×560 in each look (glass on and off, slow
// and quick motion, controller mode off and on, the save bar hidden and shown), and the alignment and
// motion rules those screens are judged by. Also: the sidebar logo is the same size on every page.
//     node Tools/check-glass-pages.mjs [outdir]      (default target/glass-review)
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';

const root = path.resolve(import.meta.dirname, '..'), ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.resolve(process.argv[2] || path.join(root, 'target/glass-review')); mkdirSync(out, { recursive: true });
const source = readFileSync(path.join(root, 'Tools/check-shell-pages.mjs'), 'utf8');
const FULL = vm.runInNewContext(source.slice(source.indexOf('const FULL ='), source.indexOf('// Every field null')) + '\nFULL');
const VALUES = { confirm_stop: 'True', pause_unfocused: 'False', osd_messages: 'True', internal_resolution: '3', msaa: '1', video_backend: 'D3D', sync_gpu: 'False', vsync: 'False', show_fps: 'False', fullscreen: 'True', show_ping: 'False', always_on_top: 'False', audio_backend: 'Cubeb', volume: '40', muted: 'False', audio_stretch: 'False', audio_latency: '20', slot_a: '8', card_a: '', gci_a: '', slot_b: '255', card_b: '', gci_b: '' };
Object.assign(FULL, {
  session_active: false, netplay_pending_join: null, ui_notices: [],
  onboarding_progress: { complete: true, next: { kind: 'already_complete' }, summary: { graphics: 'balanced' } },
  setup_graphics_options: { current: null, previous: null },
  dolphin_preferences: { values: VALUES, editable: true, live_editable: [], user_dir: 'C:\\Users\\A\\Dolphin User' },
  settings_paths: { runtime: null, game: null, userDir: null, legacyRoot: null },
  update_policy: { automatic: true }, update_check: { state: 'current' }, update_activity: { downloading: false, ready: null },
  content_status: { phase: 'up_to_date', components: [], note: 'Everything is current.' },
});
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
      if (cmd === 'launch') return new Promise(() => {});
      if (cmd in data) return data[cmd];
      return null;
    } } };
  }, { data: JSON.parse(JSON.stringify(FULL)), glass, slow });
  const page = await ctx.newPage(); page.on('pageerror', (e) => errors.push(`${file}: ${e.message}`));
  await page.goto(base + '/' + file);
  await page.waitForFunction((g) => document.documentElement.dataset.glass === (g ? 'on' : 'off'), glass);
  await page.evaluate(() => document.fonts.ready);
  return page;
}
const press = async (page, b) => { await page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; }, b); await page.waitForTimeout(90); await page.evaluate((b) => { pads[0].buttons[b] = { pressed: false, value: 0 }; }, b); await page.waitForTimeout(90); };
const style = (page, sel, prop) => page.locator(sel).first().evaluate((n, p) => getComputedStyle(n)[p], prop);
const noSideways = async (page, what) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${what}: sideways scroll`);
const settle = (page) => page.waitForFunction(() => { const m = document.querySelector('.nav-marker'); return m && !m.classList.contains('still'); });
try {
  for (const viewport of [{ width: 1280, height: 800 }, { width: 800, height: 560 }]) for (const glass of [true, false]) for (const slow of [true, false]) {
    const tag = `${viewport.width}-${glass ? 'glass' : 'solid'}-${slow ? 'slow' : 'quick'}`;
    const t = slow ? { d2: '0.72s', d3: '0.96s' } : { d2: '0.18s', d3: '0.24s' };
    { // Play
      const page = await open('index.html', { glass, slow, viewport });
      await page.getByRole('button', { name: 'Play now', exact: true }).waitFor();
      await noSideways(page, `play ${tag}`);
      assert.equal(await page.evaluate(() => getComputedStyle(document.body).backgroundColor), 'rgba(0, 0, 0, 0)', 'body stays transparent over the material');
      const bg = await page.evaluate(() => getComputedStyle(document.documentElement).backgroundImage + '|' + getComputedStyle(document.documentElement).backgroundColor);
      if (glass) assert.match(bg, /rgba\(18, 19, 23, 0\.66\)/, 'glass: the tint over the OS material'); else assert.match(bg, /radial-gradient/, 'solid: the fallback');
      await settle(page);
      assert.equal(await style(page, '.nav-marker', 'transitionDuration'), t.d3, 'nav marker at --d3');
      const [play, more, status] = await Promise.all(['#play-now', '#game-details-open', '.play-status'].map((s) => page.locator(s).evaluate((n) => { const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, h: r.height }; })));
      assert.ok(Math.abs(play.y - more.y) < 1 && Math.abs(play.h - more.h) < 1, 'Play now and ⋯ share a top and a height');
      assert.ok(Math.abs(status.x - play.x) < 2, 'the status line starts under Play now');
      await page.screenshot({ path: path.join(out, `play-${tag}.png`) });
      await page.locator('#play-graphics').click(); await page.locator('dialog.menu[open]').waitFor();
      assert.equal(await style(page, 'dialog.menu', 'animationDuration'), t.d2, 'menus at --d2');
      await page.screenshot({ path: path.join(out, `play-menu-${tag}.png`) });
      await page.keyboard.press('Escape'); await page.waitForFunction(() => !document.querySelector('dialog.menu'));
      // Controller mode (spec §6.3): a pad connected, mouse in use: no prompts. Pad input: prompts + white ring. Mouse: gone.
      assert.equal(await style(page, '.input-footer', 'display'), 'none', 'pad connected, mouse user: no hint bar');
      await page.locator('#play-now').focus();
      await press(page, 15);
      await page.waitForFunction(() => document.body.classList.contains('gamepad-input'));
      assert.equal(await style(page, '.input-footer', 'display'), 'flex', 'pad input: the hint bar');
      const ring = await page.evaluate(() => { const s = getComputedStyle(document.activeElement); return `${s.outlineStyle} ${s.outlineWidth} ${s.outlineColor}`; });
      assert.equal(ring, 'solid 2px rgb(255, 255, 255)', 'a plain white focus ring, no glow');
      await noSideways(page, `play pad ${tag}`);
      await page.screenshot({ path: path.join(out, `play-pad-${tag}.png`) });
      await page.mouse.move(300, 300); await page.mouse.move(360, 340);
      await page.waitForFunction(() => !document.body.classList.contains('gamepad-input'));
      assert.equal(await style(page, '.input-footer', 'display'), 'none', 'mouse: the hint bar goes');
      await page.context().close();
    }
    { // Settings
      const page = await open('settings.html#display', { glass, slow, viewport });
      await page.getByRole('switch', { name: 'Keep game on top' }).waitFor();
      await noSideways(page, `settings ${tag}`);
      await page.waitForTimeout(slow ? 1000 : 350);
      assert.match(await style(page, '.tab-ind', 'transitionDuration'), new RegExp('^' + t.d2.replace('.', '\\.')), 'tab highlight at --d2');
      const lefts = await page.evaluate(() => ['.dolphin-card', '.tabs', '.pref-section'].map((s) => Math.round(document.querySelector(s).getBoundingClientRect().x)));
      assert.ok(lefts.every((v) => v === lefts[0]), 'card, tabs and sections share one left edge: ' + lefts);
      await page.screenshot({ path: path.join(out, `settings-${tag}.png`) });
      await page.getByRole('switch', { name: 'Keep game on top' }).click();
      await page.locator('.save-lane.open').waitFor();
      assert.equal(await style(page, '.save-lane', 'transitionDuration'), t.d3, 'the save lane opens at --d3');
      await page.waitForTimeout(slow ? 1100 : 400);
      const lane = await page.evaluate(() => ({ main: document.getElementById('main').getBoundingClientRect().bottom, lane: document.getElementById('save-lane').getBoundingClientRect().top, bar: Math.round(document.querySelector('.save-bar').getBoundingClientRect().x), card: Math.round(document.querySelector('.dolphin-card').getBoundingClientRect().x) }));
      assert.ok(Math.abs(lane.main - lane.lane) < 1, 'the bar never covers content');
      assert.equal(lane.bar, lane.card, 'the bar lines up with the content column');
      await noSideways(page, `settings bar ${tag}`);
      await page.screenshot({ path: path.join(out, `settings-bar-${tag}.png`) });
      await page.getByRole('button', { name: 'Discard', exact: true }).click();
      await page.context().close();
    }
  }
  { // Reduced motion: every state change is instant.
    const page = await open('settings.html#display');
    await page.emulateMedia({ reducedMotion: 'reduce' });
    for (const sel of ['.nav-marker', '.tab-ind', '.save-lane']) assert.match(await style(page, sel, 'transitionDuration'), /^0s/, `${sel} under reduced motion`);
    await page.context().close();
    const play = await open('index.html');
    await play.emulateMedia({ reducedMotion: 'reduce' });
    await play.locator('#play-graphics').click(); await play.locator('dialog.menu[open]').waitFor();
    assert.equal(await style(play, 'dialog.menu', 'animationName'), 'none', 'menu under reduced motion');
    await play.context().close();
  }
  { // Adam's logos: the sidebar logo is the same size on every page, themed or not.
    const rects = [];
    for (const f of ['index.html', 'settings.html', 'mods.html']) {
      const p = await open(f);
      await p.locator('.shell-nav .brand img').waitFor();
      rects.push(await p.locator('.shell-nav .brand img').evaluate((n) => { const r = n.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map((v) => Math.round(v * 2) / 2).join(','); }));
      await p.context().close();
    }
    assert.ok(rects.every((r) => r === rects[0]), 'sidebar logo size differs between pages: ' + rects.join(' | '));
  }
  // mods.html still runs its legacy page script against this stub; only Play and Settings errors count here.
  assert.deepEqual(errors.filter((e) => !e.startsWith('mods.html')), []);
  console.log(`check-glass-pages: PASS (screens in ${out})`);
} finally { await browser.close(); server.close(); }
```

- [ ] **Step 2: Run it**

Run: `node Tools/check-glass-pages.mjs`
Expected: PASS, with 64+ screenshots in `target/glass-review/`.

If an assertion fails, the fix belongs in the task that owns the code:
- Play layout: `pages/play.css`;
- Settings layout: `pages/settings.css`;
- durations: `theme.css`.

Fix it there, re-run the owning task's own check, then this one.

Open these pairs side by side with the mockup:
- `play-1280-glass-slow.png`
- `play-800-solid-quick.png`
- `play-pad-1280-glass-slow.png`
- `settings-bar-1280-glass-slow.png`
- `settings-800-solid-quick.png`

Check each against the spec's rulings:
- no glows;
- plain white focus ring;
- no chips for status or name;
- red accent;
- the logos untouched;
- the hint bar only in the `pad` shots.

- [ ] **Step 3: Commit**

Append `await import('./check-glass-pages.mjs');` to the end of `Tools/run-browser-tests.mjs`.

```bash
git add Tools/check-glass-pages.mjs Tools/run-browser-tests.mjs
git commit -m "checks: Play and Settings screens in every look, alignment and motion (spec §13)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Whole-plan verification and the real window

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
node Tools/check-look-browser.mjs
node Tools/check-controller-navigation.mjs
node Tools/check-controller-hints.mjs
node Tools/check-play-controller-gate.mjs
node Tools/check-session-state.mjs
node Tools/check-update-notice.mjs
node Tools/check-content-notice.mjs
node Tools/test-menu-pad.mjs
node Tools/test-controller-report.mjs
node Tools/check-controller-report.mjs
node Tools/review-players.mjs
node Tools/run-browser-tests.mjs target/browser-review
```

Expected:
- every one PASSes;
- `run-browser-tests` prints its main-body PASS, then aborts at `review-finish`;
- the three baseline failures (`review-finish`, `check-launch-polish`, `check-online-mm`) fail exactly as before, and nothing else does.

Unlink Playwright.

- [ ] **Step 2: Rust stays green**

This plan touches no Rust, but the UI files are compiled into the binary.

```bash
taskkill /IM reborn-launcher.exe /F 2>NUL
cargo test --workspace -j 2
cargo build -p reborn-ui
```

Expected:
- the test count equals the plan-1 figure (1380 passed, 6 ignored);
- the build succeeds.

- [ ] **Step 3: The real window (dev build, never the installed beta)**

The dev build shares its identifier and data folder with the installed beta, so:

1. Back up `%APPDATA%\NFL Street Reborn\config\settings.json`. Record its SHA256 with `Get-FileHash`.
2. Close any running launcher: `taskkill /IM reborn-launcher.exe /F`.
3. Start `target\debug\reborn-launcher.exe`.

Save this screenshot helper as `target\glass-real\shot.ps1`:

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

WebView2 exposes no UI Automation items, so drive the window with clicks at window coordinates. Use `[W]::SetCursorPos(x,y); [W]::mouse_event(2,0,0,0,0); [W]::mouse_event(4,0,0,0,0)` in a one-off PowerShell call. Screenshot each item with `powershell -File target\glass-real\shot.ps1 <name>`:

1. **Play** (`01-play`):
   - Mica behind the sidebar;
   - the hero art;
   - the wordmark at its own size;
   - the account top right, with the first letter, name and "N min played";
   - the 52px red Play now with the square ⋯ beside it;
   - "Ready to play" alone underneath;
   - one glass strip of three cells;
   - no hint bar.
2. **Graphics menu** (`02-graphics-menu`): it opens above the cell, with a check on the current preset. Press Esc. Do not change Adam's preset.
3. **Display menu** (`03-display-menu`): same check. Press Esc.
4. **⋯ Game details** (`04-details`): it opens. Press Esc, and focus returns to ⋯.
5. **Settings** (`05-settings`):
   - one Dolphin card;
   - the tabs' highlight slides slowly between General → Display → Files & storage;
   - the Files rows show the fields with Auto and Change….
6. **Save bar, Discard only** (`06-bar`): flip General › "Pause when unfocused". The bar rises in its own lane and the content above shrinks. Press **Discard**. **Never Save Adam's Dolphin options.**
7. **Glass** (`07-glass-off`, `08-restart`):
   - Display › Window glass off: the window goes solid at once and the bar says "Window glass". Save.
   - Close the app and start it again: it opens solid, with no glass flash.
   - Turn glass back on and Save.
8. **Minimum window** (`09-play-800`, `10-settings-800`): resize the window to its minimum (800×560). Play and Settings fit without a sideways scroll, and the strip and save bar are fully visible.
9. **Needs Adam** (do not fake these):
   - pad hints and the white ring with a real pad;
   - Play now launching the game;
   - Graphics/Display menus by D-pad.

Close the app. Restore `settings.json` from the backup and confirm the SHA256 matches.

- [ ] **Step 4: Hand off to Adam (GATE)**

Report:
- the screenshots (`target/glass-real/`, `target/glass-review/`);
- every check with PASS/FAIL;
- the three baseline failures by name;
- the "needs Adam" items;
- the decisions this plan made (listed below).

**Do not build or publish a beta or an installer.** Adam said "finish it all": plan 3 (Mods / Music / Controllers) follows on this branch.

---

## Decisions this plan made (surface to Adam at handoff)

1. **Themed pages carry `body.themed`.** It scopes the new base so pages still on legacy CSS are untouched. The shell (sidebar geometry, hint bar) applies to every page. It is written to outrank the legacy sheets, so moving between pages never shifts the sidebar.
2. **The hint bar collapses to zero on every page when there is no prompt** (spec §6.3). This reverses plan 1's Ruling 15, which kept an empty 34px strip. On Play the art simply reflows.
3. **The sidebar logo slot stays at today's 124px.** The spec says 84px, but shrinking the slot would resize Adam's logo.
4. **Live volume and mute bypass the save bar** while a game is running (`live_editable`). Every other Settings change waits for Save changes.
5. **The account avatar is the name's first letter.** `identity_status` returns no avatar picture, and no Rust change was made. The account chip links to Stats.
6. **A not-ready install shows "Setup required · Finish setup"**, not "could not start". The failure detail moved from the Play screen into Game details, reached through "See why". The texture pack version moved into Game details too.
7. **The mockup's Storage sizes and "Clear shader cache" are omitted.** No command provides them. Storage keeps its existing facts as rows.
8. **The graphics preset help drops the stale ReShade text** and now reads "Applies the next time you play".
9. **The tab highlight animates its `width`** as well as `transform`. It is out of flow, so nothing else moves. This is the one exception to "opacity/transform only".
10. **New theme tokens:** `--dolphin` (the approved teal Dolphin tile), `--hero-scrim`, `--on-art`, `--avatar-bg`, `--field-bg`.
11. **The Settings form column is 942px.** That is the mockup's `.inner`: 1014px including 36px padding on each side.
