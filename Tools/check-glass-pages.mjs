// Spec §13 for Play and Settings: screens at 1280×800 and 800×560 in each look (glass on and off, slow
// and quick motion, controller mode off and on, the save bar hidden and shown), and the alignment and
// motion rules those screens are judged by. Also: the sidebar logo is the same size on every page.
//     node Tools/check-glass-pages.mjs [outdir]      (default target/glass-review)
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, mkdirSync, readdirSync } from 'node:fs';
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
const written = new Set();
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
const shot = async (page, name) => { await page.screenshot({ path: path.join(out, name) }); written.add(name); };
const press = async (page, b) => { await page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; }, b); await page.waitForTimeout(90); await page.evaluate((b) => { pads[0].buttons[b] = { pressed: false, value: 0 }; }, b); await page.waitForTimeout(90); };
const style = (page, sel, prop) => page.locator(sel).first().evaluate((n, p) => getComputedStyle(n)[p], prop);
const noSideways = async (page, what) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${what}: sideways scroll`);
// Wait for every running animation and transition on the page to finish before measuring.
const settle = async (page) => {
  await page.waitForFunction(() => { const m = document.querySelector('.nav-marker'); return m && !m.classList.contains('still'); });
  // Infinite animations (live-pulse) never finish: skipped; and never wait longer than 5 s.
  await page.evaluate(() => Promise.race([Promise.all(document.getAnimations().filter(a=>a.effect?.getComputedTiming().iterations!==Infinity).map(a=>a.finished.catch(()=>{}))),new Promise(r=>setTimeout(r,5000))]));
};
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
      await shot(page, `play-${tag}.png`);
      await page.locator('#play-graphics').click(); await page.locator('dialog.menu[open]').waitFor();
      assert.equal(await style(page, 'dialog.menu', 'animationDuration'), t.d2, 'menus at --d2');
      await page.evaluate(() => Promise.race([Promise.all(document.getAnimations().filter(a=>a.effect?.getComputedTiming().iterations!==Infinity).map(a=>a.finished.catch(()=>{}))),new Promise(r=>setTimeout(r,5000))]));
      await shot(page, `play-menu-${tag}.png`);
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
      await shot(page, `play-pad-${tag}.png`);
      await page.mouse.move(300, 300); await page.mouse.move(360, 340);
      await page.waitForFunction(() => !document.body.classList.contains('gamepad-input'));
      assert.equal(await style(page, '.input-footer', 'display'), 'none', 'mouse: the hint bar goes');
      await page.context().close();
    }
    { // Settings
      const page = await open('settings.html#display', { glass, slow, viewport });
      await page.getByRole('switch', { name: 'Keep game on top' }).waitFor();
      await noSideways(page, `settings ${tag}`);
      await settle(page);
      assert.match(await style(page, '.tab-ind', 'transitionDuration'), new RegExp('^' + t.d2.replace('.', '\\.')), 'tab highlight at --d2');
      const lefts = await page.evaluate(() => ['.dolphin-card', '.tabs', '.pref-section'].map((s) => Math.round(document.querySelector(s).getBoundingClientRect().x)));
      assert.ok(lefts.every((v) => v === lefts[0]), 'card, tabs and sections share one left edge: ' + lefts);
      await shot(page, `settings-${tag}.png`);
      await page.getByRole('switch', { name: 'Keep game on top' }).click();
      await page.locator('.save-lane.open').waitFor();
      assert.equal(await style(page, '.save-lane', 'transitionDuration'), t.d3, 'the save lane opens at --d3');
      await settle(page);
      const lane = await page.evaluate(() => ({ main: document.getElementById('main').getBoundingClientRect().bottom, lane: document.getElementById('save-lane').getBoundingClientRect().top, bar: Math.round(document.querySelector('.save-bar').getBoundingClientRect().x), card: Math.round(document.querySelector('.dolphin-card').getBoundingClientRect().x) }));
      assert.ok(Math.abs(lane.main - lane.lane) < 1, 'the bar never covers content');
      assert.equal(lane.bar, lane.card, 'the bar lines up with the content column');
      await noSideways(page, `settings bar ${tag}`);
      await shot(page, `settings-bar-${tag}.png`);
      await page.getByRole('button', { name: 'Discard', exact: true }).click();
      await page.context().close();
    }
  }
  // Five screens per look (play, play-menu, play-pad, settings, settings-bar) x 8 looks (2 widths x glass/solid x slow/quick).
  const EXPECTED_SHOTS = 40;
  assert.equal(written.size, EXPECTED_SHOTS, `screenshots written: ${written.size}`);
  const onDisk = readdirSync(out).filter((f) => written.has(f));
  assert.equal(onDisk.length, EXPECTED_SHOTS, 'every screenshot is on disk');
  { // Reduced motion: every state change is instant.
    const page = await open('settings.html#display');
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.getByRole('switch', { name: 'Keep game on top' }).click(); await page.locator('.save-lane.open').waitFor(); // the lane exists only once a change is pending
    for (const sel of ['.nav-marker', '.tab-ind', '.save-lane']) assert.match(await style(page, sel, 'transitionDuration'), /^0s/, `${sel} under reduced motion`);
    await page.context().close();
    const play = await open('index.html');
    await play.emulateMedia({ reducedMotion: 'reduce' });
    await play.locator('#play-graphics').click(); await play.locator('dialog.menu[open]').waitFor();
    assert.equal(await style(play, 'dialog.menu', 'animationName'), 'none', 'menu under reduced motion');
    assert.equal(await style(play, '.nav-marker', 'transitionDuration'), '0s', 'nav marker under reduced motion');
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
  console.log(`check-glass-pages: PASS (${written.size} screens in ${out})`);
} finally { await browser.close(); server.close(); }
