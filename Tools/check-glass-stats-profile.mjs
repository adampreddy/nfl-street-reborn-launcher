// Spec §13 for Stats and Profile: screens at 1280×800 and 800×560 in each look (glass on and off, slow and quick
// motion), Stats › Records, Profile live and not live; the motion each uses (nav marker --d3, tab highlights --d2;
// reduced motion instant); controller mode (spec §6.3): a mouse user with a pad connected sees no prompts, pad input
// brings the prompts and a plain white ring, the D-pad walks the match rows, A opens a whole match, and a mouse move
// hides the prompts again.
//     node Tools/check-glass-stats-profile.mjs [outdir]      (default target/glass-review)
import { chromium } from 'playwright';
import { mkdirSync, readdirSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { serve, openState, settle, root } from './lib/page-states.mjs';
import * as F from './fixtures/stats-fixtures.mjs';

const out = path.resolve(process.argv[2] || path.join(root, 'target/glass-review'));
mkdirSync(out, { recursive: true });
const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const STATS = { name: 'stats', url: 'stats.html', scope: 'main' };
const PROFILE = { name: 'profile', url: `profile.html?player=${F.ME}`, scope: 'main' };
const style = (page, sel, prop) => page.locator(sel).first().evaluate((n, p) => getComputedStyle(n)[p], prop);
const noSideways = async (page, what) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${what}: sideways scroll`);
const look = (page, glass) => page.waitForFunction((g) => document.documentElement.dataset.glass === (g ? 'on' : 'off'), glass);
const press = async (page, b) => {
  await page.evaluate((i) => { window.pads[0].buttons[i] = { pressed: true, value: 1 }; }, b); await page.waitForTimeout(90);
  await page.evaluate((i) => { window.pads[0].buttons[i] = { pressed: false, value: 0 }; }, b); await page.waitForTimeout(150);
};
const activeKey = (page) => page.evaluate(() => document.activeElement?.getAttribute('data-key'));
const shots = [];
const shot = async (page, name) => { await page.screenshot({ path: path.join(out, name) }); shots.push(name); };
const fixedDur = (d) => new RegExp(`^${d.replace('.', '\\.')}`);
try {
  for (const viewport of [{ width: 1280, height: 800 }, { width: 800, height: 560 }]) for (const glass of [true, false]) for (const slow of [true, false]) {
    const tag = `${viewport.width}-${glass ? 'glass' : 'solid'}-${slow ? 'slow' : 'quick'}`;
    const t = slow ? { d2: '0.72s', d3: '0.96s' } : { d2: '0.18s', d3: '0.24s' };
    const opts = { viewport, glass, slow, reducedMotion: 'no-preference' };
    { // Stats
      const { page, errors, close } = await openState(browser, srv.base, STATS, opts);
      await page.locator('#st-matches').waitFor();
      await look(page, glass);
      await settle(page);
      assert.equal(await style(page, '.nav-marker', 'transitionDuration'), t.d3, `nav marker at --d3 (${tag})`);
      assert.match(await style(page, '#st-matches .tab-ind', 'transitionDuration'), fixedDur(t.d2), `tab highlight at --d2 (${tag})`);
      assert.match(await style(page, '#stats-view .tab-ind', 'transitionDuration'), fixedDur(t.d2), `Matches | Records highlight at --d2 (${tag})`);
      await noSideways(page, `stats ${tag}`);
      await shot(page, `stats-${tag}.png`);
      await page.locator('[data-key="view-records"]').click();
      await page.locator('#rec-single').waitFor();
      await page.waitForTimeout(slow ? 1100 : 350);
      await settle(page);
      await noSideways(page, `stats records ${tag}`);
      await shot(page, `stats-records-${tag}.png`);
      assert.deepEqual(errors, []);
      await close();
    }
    for (const live of [true, false]) { // Profile, in a match and not
      const { page, errors, close } = await openState(browser, srv.base, PROFILE, { ...opts, data: { player_profile: F.profile({ live }) } });
      await page.locator('.pf-card').waitFor();
      await look(page, glass);
      await settle(page);
      assert.equal(await page.locator('.pf-live').count(), live ? 1 : 0, `profile live card (${tag}, live=${live})`);
      assert.equal(await page.locator('.pf-tabbar').count(), 0, `no profile tabs (mv2) (${tag})`);
      await noSideways(page, `profile ${tag}`);
      await shot(page, `profile-${live ? 'live' : 'idle'}-${tag}.png`);
      assert.deepEqual(errors, []);
      await close();
    }
  }
  { // reduced motion: every state change is instant
    const { page, close } = await openState(browser, srv.base, STATS);
    await page.locator('#st-matches').waitFor();
    for (const sel of ['.nav-marker', '#st-matches .tab-ind', '#stats-view .tab-ind']) assert.match(await style(page, sel, 'transitionDuration'), /^0s/, `${sel} under reduced motion`);
    await close();
  }
  { // controller mode on Stats (spec §6.3)
    const { page, errors, close } = await openState(browser, srv.base, STATS, { pad: true });
    await page.locator('#st-matches').waitFor();
    assert.equal(await style(page, '.input-footer', 'display'), 'none', 'a pad connected, a mouse user: no prompts');
    await page.locator('[data-key="match-m-02"]').focus();
    await press(page, 12);                                   // the first pad input turns controller mode on
    await page.waitForFunction(() => document.body.classList.contains('gamepad-input'));
    assert.equal(await style(page, '.input-footer', 'display'), 'flex', 'pad input: the prompts');
    await page.locator('[data-key="match-m-01"]').focus();
    await press(page, 13);                                   // D-pad Down
    assert.equal(await activeKey(page), 'match-m-02', 'Down walks to the next match (nothing opens in place)');
    await press(page, 12);                                   // D-pad Up
    assert.equal(await activeKey(page), 'match-m-01');
    const ring = await page.evaluate(() => { const s = getComputedStyle(document.activeElement); return `${s.outlineStyle} ${s.outlineWidth} ${s.outlineColor}`; });
    assert.equal(ring, 'solid 2px rgb(255, 255, 255)', 'a plain white focus ring, no glow');
    await shot(page, 'stats-pad-1280.png');
    await page.mouse.move(300, 300);
    await page.mouse.move(360, 340);
    await page.waitForFunction(() => !document.body.classList.contains('gamepad-input'));
    assert.equal(await style(page, '.input-footer', 'display'), 'none', 'mouse: the prompts go');
    await page.locator('[data-key="match-m-01"]').focus();
    await press(page, 0);                                    // A opens the whole match
    await page.waitForURL(/match\.html\?game=g-013&games=g-011%2Cg-012%2Cg-013/);
    assert.deepEqual(errors, []);
    await close();
  }
  assert.equal(shots.length, 33, `screenshot count: 8 looks x 4 screens (stats, stats-records, profile-live, profile-idle) + the pad shot; got ${shots.length}`);
  console.log(`check-glass-stats-profile: PASS (${shots.length} screens in ${out}; ${readdirSync(out).filter((f) => /^(stats|profile)-.*\.png$/.test(f)).length} Stats/Profile files present)`);
} finally { await browser.close(); await srv.close(); }
