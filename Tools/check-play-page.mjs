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
async function open(variant = {}, viewport = { width: 1280, height: 800 }, { pad = false } = {}) {
  const ctx = await browser.newContext({ viewport, reducedMotion: pad ? 'reduce' : 'no-preference' });
  // A stubbed pad (as check-menu): frames run only on tick(), so each press is exactly one frame down, one up.
  if (pad) await ctx.addInitScript(() => {
    window.pads = [{ id: 'Test pad', index: 0, connected: true, mapping: 'standard', axes: [0, 0, 0, 0], buttons: Array.from({ length: 18 }, () => ({ pressed: false, value: 0 })) }];
    Object.defineProperty(navigator, 'getGamepads', { value: () => pads });
    document.hasFocus = () => true;
    window.frames = []; window.clock = 0; window.requestAnimationFrame = (fn) => (frames.push(fn), frames.length); window.cancelAnimationFrame = () => {};
    window.tick = () => { clock += 16; frames.splice(0).forEach((fn) => fn(clock)); };
  });
  await ctx.addInitScript(({ data, variant }) => {
    Object.assign(data, variant);
    window.calls = []; window.launchAnswer = null;
    window.__TAURI__ = { event: { listen: async () => () => {} }, core: { invoke: async (cmd, args = {}) => {
      window.calls.push({ cmd, args: JSON.parse(JSON.stringify(args)) });
      if (cmd === 'launch') return window.launchAnswer || new Promise((resolve) => { window.finishLaunch = resolve; });
      if (cmd === 'window_glass_apply') return data.launcher_look;
      if (cmd === 'dolphin_preference_set') { data.dolphin_preferences.values[args.key] = args.value; return window.holdPref ? new Promise((r) => { window.releasePref = () => r(null); }) : null; }
      if (cmd in data) return data[cmd];
      return null;
    } } };
  }, { data: JSON.parse(JSON.stringify(FULL)), variant });
  const page = await ctx.newPage(); page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(base + '/index.html');
  return page;
}
// The first draw rises in (look-enter); geometry is read once every finite animation has finished.
const settled = (page) => page.evaluate(() => Promise.all(document.getAnimations().filter((a) => a.effect?.getComputedTiming().iterations !== Infinity).map((a) => a.finished.catch(() => {}))));
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height, r: r.right, b: r.bottom }; });
const overlap = (a, b) => a.x < b.r && b.x < a.r && a.y < b.b && b.y < a.b;
try {
  { // Ready, at 1280×800
    const page = await open();
    await page.getByRole('button', { name: 'Play now', exact: true }).waitFor(); await settled(page);
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
    await settled(page); await page.screenshot({ path: path.join(out, 'play-graphics-menu-1280.png') });
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
    await page.locator('dialog.menu[open]').waitFor();
    assert.equal(await page.locator('#play-display').getAttribute('aria-expanded'), 'true');
    await settled(page); await page.screenshot({ path: path.join(out, 'play-display-menu-1280.png') });
    await page.getByRole('menuitemradio', { name: 'Windowed' }).click();
    await page.waitForFunction(() => calls.some((c) => c.cmd === 'dolphin_preference_set' && c.args.key === 'fullscreen' && c.args.value === 'false'));
    await page.waitForFunction(() => /Windowed/.test(document.getElementById('play-display').innerText));
    // Picking the value already set sends nothing (a checkbox could not change to its own state).
    await page.locator('#play-display').click();
    await page.getByRole('menuitemradio', { name: 'Windowed' }).click();
    await page.waitForTimeout(100);
    const prefCalls = () => page.evaluate(() => calls.filter((c) => c.cmd === 'dolphin_preference_set').length);
    assert.equal(await prefCalls(), 1, 'the same value is not written again');
    // In-flight lock: while a Display write is out, the cell does not open and a second write is never sent.
    await page.evaluate(() => { window.holdPref = true; });
    await page.locator('#play-display').click();
    await page.getByRole('menuitemradio', { name: 'Fullscreen' }).click();
    await page.waitForFunction(() => calls.filter((c) => c.cmd === 'dolphin_preference_set').length === 2);
    await page.locator('#play-display').click();
    await page.waitForTimeout(150);
    assert.equal(await page.locator('dialog.menu').count(), 0, 'no menu while a Display write is out');
    assert.equal(await prefCalls(), 2, 'one write in flight, never two');
    assert.equal(await page.evaluate(() => document.activeElement?.id), 'play-display', 'the cell keeps focus while its write is out');
    await page.evaluate(() => { window.holdPref = false; window.releasePref(); });
    await page.waitForFunction(() => /Fullscreen/.test(document.getElementById('play-display').innerText));

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
    await page.getByRole('button', { name: 'Sign in with Discord', exact: true }).waitFor(); await settled(page);
    const acct = await rect(page, '.play-account'), main = await rect(page, '#main');
    assert.ok(main.r - acct.r <= 26, 'top right');
    await page.context().close();
  }
  { // Review Focus 5: the minimum window with a controller-setup note.
    const page = await open({}, { width: 800, height: 560 }, { pad: true });
    await page.getByRole('button', { name: 'Play now', exact: true }).waitFor(); await settled(page);
    // Both menus fit the minimum window.
    for (const id of ['graphics', 'display']) {
      await page.locator(`#play-${id}`).click();
      await page.locator('dialog.menu[open]').waitFor();
      const m = await rect(page, 'dialog.menu');
      assert.ok(m.y >= 0 && m.b <= 560 && m.x >= 0 && m.r <= 800, `the ${id} menu stays inside the window: ` + JSON.stringify(m));
      await settled(page); await page.screenshot({ path: path.join(out, `play-${id}-menu-800.png`) });
      await page.keyboard.press('Escape');
      await page.waitForFunction(() => !document.querySelector('dialog.menu'));
    }
    await page.evaluate(() => { window.launchAnswer = { started: false, message: 'Your controller needs a one-time setup before it can be used in the game.', controller_setup: { device: 'WGInput/0/Pro Controller', controls: ['Start', 'A'] } }; });
    await page.getByRole('button', { name: 'Play now', exact: true }).click();
    await page.locator('.play-controller-setup').waitFor();
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
    const acct = await rect(page, '.play-account'), logo = await rect(page, '.play-wordmark img'), notes = await rect(page, '.play-notes'), strip = await rect(page, '.play-strip');
    assert.ok(!overlap(acct, logo), 'the account never covers the wordmark');
    assert.ok(notes.b <= strip.y, 'the notes end above the strip');
    await page.screenshot({ path: path.join(out, 'play-setup-note-800.png'), fullPage: true });
    // The wordmark (never resized) plus the note do not fit 560px with the strip: #main scrolls. A pad
    // player must still reach the strip, and it scrolls into view when it takes focus.
    const press = (b) => page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; tick(); pads[0].buttons[b] = { pressed: false, value: 0 }; tick(); }, b);
    await page.evaluate(() => tick());
    for (let i = 0; i < 8 && !(await page.evaluate(() => !!document.activeElement?.closest('.play-strip'))); i++) await press(13);
    assert.ok(await page.evaluate(() => !!document.activeElement?.closest('.play-strip')), 'D-pad Down reaches the strip');
    for (let i = 0; i < 5; i++) await page.evaluate(() => tick());
    const cellNow = await page.evaluate(() => { const r = document.activeElement.getBoundingClientRect(); return { y: r.y, b: r.bottom, scrolled: document.getElementById('main').scrollTop }; });
    assert.ok(cellNow.y >= 0 && cellNow.b <= 560 && cellNow.scrolled > 0, 'the strip scrolls into view: ' + JSON.stringify(cellNow));
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'still no sideways scroll');
    await page.screenshot({ path: path.join(out, 'play-setup-note-800-strip-focused.png') });
    await page.context().close();
  }
  assert.deepEqual(errors, []);
  console.log('check-play-page: PASS');
} finally { await browser.close(); server.close(); }
