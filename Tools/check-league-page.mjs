// The League page, layout F (docs/superpowers/specs/2026-10-17-league-page-v3-design.md): every League state from
// lib/page-states.mjs on the real page, in Edge. Fails on a page error, banned copy, a roster, a sideways scroll,
// columns that do not end on one line, the wrong game order, more than 3 rows a stat, and links that open the wrong
// page. Screenshots land in target/league-page.
//     node Tools/check-league-page.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { STATES, root, serve, openState, settle } from './lib/page-states.mjs';

const out = path.join(root, 'target/league-page');
mkdirSync(out, { recursive: true });
let failures = 0;
const fail = (name, msg) => { failures++; console.log(`FAIL ${name}: ${msg}`); };
const ok = (name) => console.log(`ok   ${name}`);
const BANNED = ['This week', 'Your game', 'Catch-up game', 'Roster'];
const MEMBER = (n) => !['league-error', 'league-signed-out'].includes(n);

const { base, close } = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
for (const s of STATES.league) {
  const viewport = s.viewport || { width: 1280, height: 800 };
  const { page, errors, close: done } = await openState(browser, base, s, { viewport, pad: s.pad === true });
  await settle(page);
  await page.waitForTimeout(150);
  await page.screenshot({ path: path.join(out, `${s.name}.png`) });
  const p = [...errors];
  const t = await page.locator('#screen').innerText();
  for (const w of BANNED) if (t.includes(w)) p.push(`says "${w}"`);
  if (await page.locator('.lg-roster').count()) p.push('draws a roster');
  const sideways = await page.evaluate(() => { const m = document.getElementById('main'); return m.scrollWidth - m.clientWidth; });
  if (sideways > 1) p.push(`scrolls sideways by ${sideways}px`);
  if (MEMBER(s.name)) {
    const bottoms = await page.$$eval('.lg-cols > .lg-card', (n) => n.map((c) => Math.round(c.getBoundingClientRect().bottom)));
    const wide = viewport.width > 1100;
    if (wide && bottoms.length === 3 && Math.max(...bottoms) - Math.min(...bottoms) > 2) p.push(`columns end at ${bottoms.join(', ')}`);
    if (!wide && bottoms.length === 3 && Math.abs(bottoms[0] - bottoms[1]) > 2) p.push(`standings and week end at ${bottoms[0]}, ${bottoms[1]}`);
    const best = await page.$$eval('#lg-best .lg-bgrp', (g) => g.map((x) => x.querySelectorAll('.lg-brow').length));
    if (best.some((n) => n > 3)) p.push(`a stat group has ${Math.max(...best)} rows`);
  }
  if (s.name === 'league') {
    const purpose = (await page.locator('.lg-head .purpose').innerText()).trim();
    if (purpose !== 'Reborn Season 4 · Week 3') p.push(`subtitle reads "${purpose}"`);
    const marks = await page.$$eval('#lg-week .lg-game', (g) => g.map((x) => x.querySelector('.lg-mark').textContent.trim().toLowerCase()));
    const order = marks.map((m) => (m === 'final' ? 0 : m === 'you' ? 1 : 2));
    if (order.some((v, i) => i && v < order[i - 1])) p.push(`week order ${marks.join(',')}`);
    if (marks.filter((m) => m === 'you').length !== 1) p.push(`You marked ${marks.filter((m) => m === 'you').length} times`);
    if (!(await page.locator('[data-key="league-play"]').innerText()).includes('Play Wolves Reign')) p.push('primary is not Play Wolves Reign');
    // a standings row opens that team's page on the league site
    const row = page.locator('#lg-standings .lg-row.lg-link').first();
    const want = await row.getAttribute('data-url');
    await row.click();
    const opened = await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'open_website').map((c) => c.args.url));
    if (!/^https:\/\/nflstreetreborn\.com\/league\/team\.php\?league_id=36&team_id=\d+$/.test(want || '') || opened.at(-1) !== want) p.push(`standings link ${want} opened ${opened.at(-1)}`);
    // the Friendly menu: League rules is the default; Standard rules hosts one raw game
    // Friendly is league rules only (Adam 10-17: "dont let us cycle through game rules"): no rules menu
    if (await page.locator('[data-key="league-friendly-menu"]').count()) p.push('a Friendly rules menu is offered');
    const best = (await page.locator('#lg-best .vc-h h2').innerText()).trim();
    if (best !== "Week 3's Top Performers") p.push(`week's best card reads "${best}"`);
  }
  if (s.name === 'league-stale' && (t.match(/Can't reach the league/g) || []).length !== 1) p.push('stale data is not said exactly once');
  if (s.name === 'league-stale' && (await page.locator('[data-key="league-join"], [data-key="league-cancel"]').count())) p.push('stale data offers a room button');
  if (s.name === 'league-catchup' && !/week 2 game/i.test(t)) p.push('catch-up game is not labelled Week 2 game');
  if (s.name === 'league-no-best' && !t.includes('No games played yet')) p.push('no week_best does not say No games played yet');
  if (s.name === 'league-no-logo' && !(await page.locator('.lg-bt .team-logo.text').count())) p.push('a missing logo has no letter fallback');
  if (s.name === 'league-update-required' && !(await page.locator('[data-key="league-install"]', { hasText: 'Update league files' }).count())) p.push('a version refusal offers no Update league files');
  // a letter badge (no logo yet, or none) never spills out of its circle
  const spill = await page.$$eval('#screen .team-logo.text', (n) => n.filter((b) => b.scrollWidth > b.clientWidth + 1).length);
  if (spill) p.push(`${spill} letter badge(s) overflow`);
  if (p.length) fail(s.name, p.join('; ')); else ok(s.name);
  await done();
}
// The banner at three widths: nothing overflows it, normal names are never cut, and the primary button stays inside.
for (const name of ['league', 'league-unlinked', 'league-between', 'league-long-names', 'league-update-required']) {
  for (const width of [960, 1101, 1280]) {
    const s = STATES.league.find((x) => x.name === name);
    const { page, errors, close: done } = await openState(browser, base, s, { viewport: { width, height: 800 } });
    await settle(page);
    const p = [...errors];
    const r = await page.evaluate(() => {
      const row = document.querySelector('.lg-bn-row'), banner = document.querySelector('.lg-banner');
      const names = [...document.querySelectorAll('.lg-bt-text b')].map((b) => ({ t: b.textContent, cut: b.scrollWidth > b.clientWidth + 1 }));
      const subs = [...document.querySelectorAll('.lg-bt-text span')].map((x) => x.getBoundingClientRect().height);
      const prim = document.querySelector('.lg-acts .btn.primary');
      const pr = prim?.getBoundingClientRect(), br = banner?.getBoundingClientRect();
      return { over: row ? row.scrollWidth - row.clientWidth : 0, names, subs, primOut: pr && br ? pr.right > br.right + 1 || pr.left < br.left - 1 : false };
    });
    if (r.over > 1) p.push(`banner row overflows by ${r.over}px`);
    if (r.primOut) p.push('primary button outside the banner');
    if (name !== 'league-long-names' && r.names.some((n) => n.cut)) p.push(`team name cut: ${r.names.filter((n) => n.cut).map((n) => n.t).join(', ')}`);
    if (r.subs.some((h) => h > 22)) p.push('owner · record wraps');
    if (p.length) fail(`${name}@${width}`, p.join('; ')); else ok(`${name}@${width}`);
    await done();
  }
}
await browser.close();
await close();
console.log(failures ? `FAIL: ${failures}` : `PASS (${STATES.league.length} League states in ${out})`);
process.exit(failures ? 1 : 0);
