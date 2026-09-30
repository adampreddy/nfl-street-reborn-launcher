// Spec §13 for Mods, Music and Controllers: screens at 1280×800 and 800×560 in each look (glass on and off,
// slow and quick motion), Music's save bar shown and hidden, Controllers in controller mode, and the
// alignment and motion rules those screens are judged by. Also Review Focus 2 and 5 (plan 3).
//     node Tools/check-glass-mods-music-controls.mjs [outdir]      (default target/glass-review)
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, mkdirSync, readdirSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';

const root = path.resolve(import.meta.dirname, '..'), ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.resolve(process.argv[2] || path.join(root, 'target/glass-review')); mkdirSync(out, { recursive: true });

const LONG = 'Classic brown football with a very long package name that has to wrap';
const slots = Array.from({ length: 21 }, (_, i) => ({ slot: i + 1, title: `Song ${i + 1}`, artist: `Artist ${i + 1}`, album: '', duration_ms: 180000 + i * 1000, source: 'original', source_name: null, pending: null,
  original: { title: `Song ${i + 1}`, artist: `Artist ${i + 1}`, album: '', duration_ms: 180000 + i * 1000 } }));
slots[2].pending = { kind: 'your_file', title: 'Blinding Lights', artist: 'The Weeknd', album: '' };
slots[6] = { ...slots[6], title: 'Renamed on the disc' };
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
const written = new Set();
const shot = async (page, name) => { await page.screenshot({ path: path.join(out, name) }); written.add(name); };
async function open(file, { glass = true, slow = true, viewport = { width: 1280, height: 800 }, mutate = null } = {}) {
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
      if (cmd === 'controller_assign_player' && window.__holdAssign) return new Promise((r) => { window.__release = () => r({ slots: data.controller_slots.slots }); });
      if (cmd === 'controls_set') { await new Promise((r) => setTimeout(r, 300)); return { sync_note: 'Button textures saved.' }; }
      if (cmd === 'remap_detect') return new Promise(() => {});
      if (cmd in data) return data[cmd];
      return null;
    } } };
  }, { data: (() => { const d = JSON.parse(JSON.stringify(DATA)); if (mutate) mutate(d); return d; })(), glass, slow });
  const page = await ctx.newPage(); page.on('pageerror', (e) => errors.push(`${file}: ${e.message}`));
  await page.goto(base + '/' + file);
  await page.waitForFunction((g) => document.documentElement.dataset.glass === (g ? 'on' : 'off'), glass);
  await page.evaluate(() => document.fonts.ready);
  return page;
}
// Hold the button until the expected effect is seen (bounded), then release: no fixed wall-clock edge to miss.
const press = async (page, b, until) => {
  await page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; }, b);
  try { await page.waitForFunction(until, null, { timeout: 4000, polling: 'raf' }); } finally { await page.evaluate((b) => { pads[0].buttons[b] = { pressed: false, value: 0 }; }, b); }
  await page.waitForTimeout(90);
};
const style = (page, sel, prop, pseudo = null) => page.locator(sel).first().evaluate((n, [p, ps]) => getComputedStyle(n, ps)[p], [prop, pseudo]);
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height, r: r.right, b: r.bottom }; });
const rects = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => { const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; }));
// Wait for every finite animation and transition to finish; infinite ones are skipped, and never wait over 5 s.
const settle = async (page) => {
  await page.waitForFunction(() => { const m = document.querySelector('.nav-marker'); return m && !m.classList.contains('still'); });
  await page.evaluate(() => Promise.race([Promise.all(document.getAnimations().filter((a) => a.effect?.getComputedTiming().iterations !== Infinity).map((a) => a.finished.catch(() => {}))), new Promise((r) => setTimeout(r, 5000))]));
};
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
      const scr = await rect(page, '#screen');
      assert.ok(cards.every((c) => c.x >= scr.x - 1 && c.x + c.w <= scr.r + 1), 'every card stays inside the content column: ' + JSON.stringify(cards));
      assert.ok(await page.locator('.store-card').evaluateAll((ns) => ns.every((n) => n.scrollWidth <= n.clientWidth + 1)), 'the long package name wraps inside its card');
      assert.deepEqual(await page.locator('.store-card').evaluateAll((ns) => ns.map((n) => n.querySelectorAll('[role=switch]').length)), [1, 1, 1], 'one switch per card');
      assert.equal(await page.locator('.store-card.active').count(), 1);
      assert.equal(await page.locator('.store-card.active .store-check').count(), 1, 'a red check on the enabled card');
      assert.equal(await page.locator('.store-card:not(.active) .store-check').count(), 0);
      assert.equal(await page.locator('.store-art img, .store-art > svg').count(), 0, 'no drawn art in the preview');
      assert.equal(await style(page, '.store-check', 'backgroundColor'), 'rgb(224, 39, 46)', 'the check is red');
      assert.equal(await page.locator('.mods-count').innerText(), '3 footballs · 1 enabled · only one football can be on at a time');
      await settle(page);
      assert.match(await style(page, '.tab-ind', 'transitionDuration'), starts(t.d2), 'the category highlight slides at --d2');
      assert.match(await style(page, '.store-card [role=switch]', 'transitionDuration', '::after'), starts(t.d2), 'the switch thumb slides at --d2');
      await shot(page, `mods-${tag}.png`);
      await page.context().close();
    }
    { // Music (spec §8.4, §7)
      const page = await open('music.html', { glass, slow, viewport });
      await page.locator('.mz-track').first().waitFor();
      await page.locator('.save-lane.open').waitFor();
      await material(page, glass);
      assert.equal(await style(page, '.save-lane', 'transitionDuration'), t.d3, 'the save lane opens at --d3');
      await settle(page);
      const edge = await page.evaluate(() => ({ main: document.getElementById('main').getBoundingClientRect().bottom, lane: document.getElementById('save-lane').getBoundingClientRect().top,
        bar: Math.round(document.querySelector('.save-bar').getBoundingClientRect().x), table: Math.round(document.getElementById('disc').getBoundingClientRect().x) }));
      assert.ok(Math.abs(edge.main - edge.lane) < 1, 'the main area ends where the save lane begins');
      await page.evaluate(() => { const m = document.getElementById('main'); m.scrollTop = m.scrollHeight; });
      const fit = await page.evaluate(() => { const lane = document.getElementById('save-lane').getBoundingClientRect(), bar = document.querySelector('.save-bar').getBoundingClientRect(), last = Array.from(document.querySelectorAll('.mz-track')).pop().getBoundingClientRect();
        return { laneTop: lane.top, laneBottom: lane.bottom, barTop: bar.top, barBottom: bar.bottom, lastBottom: last.bottom, vh: innerHeight, vw: innerWidth, barR: bar.right }; });
      assert.ok(fit.lastBottom <= fit.laneTop + 0.5, 'scrolled to the end, the last song row ends above the bar: ' + JSON.stringify(fit));
      assert.ok(fit.barTop >= fit.laneTop - 0.5 && fit.barBottom <= fit.laneBottom + 0.5 && fit.barBottom <= fit.vh + 0.5 && fit.barR <= fit.vw, 'the bar sits inside its lane and the window: ' + JSON.stringify(fit));
      assert.ok(await page.evaluate(() => Array.from(document.querySelectorAll('.mz-col-actions button')).every((b) => { const c = b.closest('td').getBoundingClientRect(), r = b.getBoundingClientRect(); return r.left >= c.left - 0.5 && r.right <= c.right + 0.5; })), 'row action buttons stay inside their cell');
      assert.equal(edge.bar, edge.table, 'the bar lines up with the soundtrack table');
      assert.match(await page.locator('.save-bar').innerText(), /1 pending change[\s\S]*Song 3 replaced[\s\S]*Discard[\s\S]*Apply changes/);
      assert.equal(await page.locator('[data-slot="3"] .mz-src').innerText(), 'Replacing Song 3 · Pending');
      assert.equal(await style(page, '[data-slot="3"] .mz-src .pend', 'color'), 'rgb(255, 122, 127)', '· Pending is --red-ink');
      assert.deepEqual(await page.locator('[data-slot="3"] .mz-col-actions button').allInnerTexts(), ['Undo']);
      assert.deepEqual(await page.locator('[data-slot="5"] .mz-col-actions button').allInnerTexts(), ['Restore original', 'Replace']);
      assert.deepEqual(await page.locator('[data-slot="7"] .mz-col-actions button').allInnerTexts(), ['Restore original', 'Replace'], 'a renamed original can be restored');
      assert.deepEqual(await page.locator('[data-slot="1"] .mz-col-actions button').allInnerTexts(), ['Replace']);
      assert.equal(await page.getByRole('button', { name: 'Add song', exact: true }).count(), 0, 'no duplicate Add song');
      assert.equal(await page.getByRole('switch', { name: 'Custom soundtrack' }).getAttribute('aria-checked'), 'true');
      if (viewport.width === 1280) {
        const add = await rect(page, 'button:text-is("Add songs…")'), disc = await rect(page, '#disc');
        assert.ok(Math.abs(add.r - disc.r) < 1.5, 'Add songs… ends on the table\'s right edge');
      }
      await noSideways(page, `music bar ${tag}`);
      await shot(page, `music-bar-${tag}.png`);
      if (viewport.width === 1280 && glass && slow) { // Review Focus 2: the pad reaches the bar, and Back never leaves the page
        await page.locator('[aria-label="Replace song 21"]').scrollIntoViewIfNeeded();
        await page.locator('[aria-label="Replace song 21"]').focus();
        await press(page, 13, () => document.activeElement?.textContent === 'Apply changes');
        assert.equal(await page.evaluate(() => document.activeElement.textContent), 'Apply changes', 'D-pad down from the last song reaches Apply changes');
        await press(page, 1, () => !!document.activeElement?.closest('.shell-nav'));
        assert.ok(page.url().endsWith('/music.html'), 'Back from the bar stays on Music');
        assert.equal(await page.evaluate(() => !!document.activeElement.closest('.shell-nav')), true, 'Back from the bar goes to the sidebar');
        await page.mouse.move(300, 300); await page.mouse.move(360, 340);
      }
      await page.getByRole('button', { name: 'Discard', exact: true }).click();
      await page.waitForFunction(() => !document.querySelector('.save-lane.open'));
      await settle(page);
      assert.equal(await page.locator('.mz-track.staged').count(), 0);
      await noSideways(page, `music ${tag}`);
      await shot(page, `music-${tag}.png`);
      await page.context().close();
    }
    { // Controllers (spec §8.5, §6.3)
      const page = await open('controls.html', { glass, slow, viewport });
      await page.getByRole('button', { name: 'Assign A', exact: true }).waitFor();
      await settle(page);
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
      await shot(page, `controls-${tag}.png`);
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
      await press(page, 15, () => document.body.classList.contains('gamepad-input'));
      await page.waitForFunction(() => document.body.classList.contains('gamepad-input'));
      assert.equal(await style(page, '.input-footer', 'display'), 'flex', 'pad input: the hint bar');
      const ring = await page.evaluate(() => { const s = getComputedStyle(document.activeElement); return `${s.outlineStyle} ${s.outlineWidth} ${s.outlineColor}`; });
      assert.equal(ring, 'solid 2px rgb(255, 255, 255)', 'a plain white focus ring, no glow');
      await noSideways(page, `controls pad ${tag}`);
      await shot(page, `controls-pad-${tag}.png`);
      await page.mouse.move(300, 300); await page.mouse.move(360, 340);
      await page.waitForFunction(() => !document.body.classList.contains('gamepad-input'));
      assert.equal(await style(page, '.input-footer', 'display'), 'none', 'mouse: the hint bar goes');
      await page.context().close();
    }
  }
  { // Controllers: Dismiss while busy, and the refresh that locks the page under a focused Dismiss.
    const page = await open('controls.html');
    await page.getByRole('button', { name: 'Assign A', exact: true }).waitFor();
    await settle(page);
    await page.evaluate(() => { window.__holdAssign = true; });
    await page.locator('.player-device select').first().focus();
    await page.locator('.player-device select').first().selectOption({ index: 1 });
    await page.waitForFunction(() => document.querySelector('#player-tab-0').disabled);
    assert.equal(await page.locator('.ctl-note button').isDisabled(), true, 'Dismiss cannot be activated while busy');
    assert.notEqual(await page.evaluate(() => document.activeElement?.tagName), 'BODY', 'busy: focus is not on the page');
    await page.evaluate(() => window.__release());
    await page.waitForFunction(() => !document.querySelector('#player-tab-0').disabled);
    // Pad focus on Dismiss when a device refresh locks the page.
    await page.locator('.ctl-note button').focus();
    await page.evaluate(() => document.dispatchEvent(new Event('refresh-devices')));
    await page.waitForFunction(() => document.querySelector('#player-tab-0').disabled || document.activeElement?.tagName !== 'BODY');
    assert.notEqual(await page.evaluate(() => document.activeElement?.tagName), 'BODY', 'a refresh under a focused Dismiss does not drop focus to the page');
    await page.waitForFunction(() => !document.querySelector('#player-tab-0').disabled);
    await page.locator('.ctl-note button').click();
    assert.equal(await page.locator('.ctl-note').count(), 0, 'the note goes');
    assert.equal(await page.evaluate(() => document.activeElement?.id), 'player-tab-0', 'Dismiss hands focus to the player tab');
    await page.context().close();
  }
  { // Controllers: focus returns to the Button textures select once its save finishes.
    const page = await open('controls.html');
    await page.getByRole('button', { name: 'Assign A', exact: true }).waitFor();
    await settle(page);
    const tex = page.locator('#player-textures');
    await tex.focus();
    await tex.selectOption('gamecube');
    await page.waitForFunction(() => document.querySelector('#player-textures').disabled);
    assert.notEqual(await page.evaluate(() => document.activeElement?.tagName), 'BODY', 'busy: focus is not on the page');
    await page.waitForFunction(() => !document.querySelector('#player-textures').disabled);
    assert.equal(await page.evaluate(() => document.activeElement?.id), 'player-textures', 'after the save, focus is back on the Button textures select');
    await page.context().close();
  }
  { // Mods: an empty library offers the in-page import action (not just the toolbar link).
    const page = await open('mods.html', { mutate: (d) => { d.mod_packages.packages = []; d.mod_packages.activation = []; d.mod_packages.exclusive = []; } });
    await page.locator('.store-empty').waitFor();
    assert.equal(await page.locator('.store-empty h2').innerText(), 'Your mod library starts here');
    const action = page.locator('.store-empty a');
    assert.equal(await action.count(), 1, 'the empty state has its own import action');
    assert.equal(await action.innerText(), 'Import mods');
    assert.equal(await action.getAttribute('href'), 'onboarding.html');
    await page.context().close();
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
  // Five screens per look (mods, music-bar, music, controls, controls-pad) x 8 looks (2 widths x glass/solid x slow/quick).
  const EXPECTED_SHOTS = 5 * 8;
  assert.equal(written.size, EXPECTED_SHOTS, `screenshots written: ${written.size}`);
  assert.equal(readdirSync(out).filter((f) => written.has(f)).length, EXPECTED_SHOTS, 'every screenshot is on disk');
  console.log(`check-glass-mods-music-controls: PASS (${written.size} screens in ${out})`);
} finally { await browser.close(); server.close(); }
