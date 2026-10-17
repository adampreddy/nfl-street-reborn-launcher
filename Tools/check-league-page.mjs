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
    // hosting a friendly hands over to the Online page, which starts a fresh call log: keep this one's in sessionStorage
    await page.evaluate(() => { const push = window.__calls.push.bind(window.__calls); window.__calls.push = (c) => {
      if (c.cmd === 'league_host_friendly') sessionStorage.setItem('friendly', c.args.ruleset); return push(c); }; });
    await page.click('[data-key="league-friendly-menu"]');
    await page.locator('dialog.menu[open] .menu-item', { hasText: 'Standard rules' }).click();
    await page.waitForTimeout(400);
    const rules = await page.evaluate(() => sessionStorage.getItem('friendly'));
    if (rules !== 'raw') p.push(`Standard rules hosted ${rules}`);
  }
  if (s.name === 'league-stale' && (t.match(/Can't reach the league/g) || []).length !== 1) p.push('stale data is not said exactly once');
  if (s.name === 'league-stale' && (await page.locator('[data-key="league-join"], [data-key="league-cancel"]').count())) p.push('stale data offers a room button');
  if (s.name === 'league-catchup' && !/week 2 game/i.test(t)) p.push('catch-up game is not labelled Week 2 game');
  if (s.name === 'league-no-best' && !t.includes('No games played yet')) p.push('no week_best does not say No games played yet');
  if (s.name === 'league-no-logo' && !(await page.locator('.lg-bt .team-logo.text').count())) p.push('a missing logo has no letter fallback');
  if (p.length) fail(s.name, p.join('; ')); else ok(s.name);
  await done();
}
await browser.close();
await close();
console.log(failures ? `FAIL: ${failures}` : `PASS (${STATES.league.length} League states in ${out})`);
process.exit(failures ? 1 : 0);
