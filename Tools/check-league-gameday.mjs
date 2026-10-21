// League game day (spec docs/superpowers/specs/2026-10-21-league-game-day-design.md): every game day state from
// lib/page-states.mjs on the real League page, in Edge. Fails on a page error, a sideways scroll, a page that scrolls at
// 1280x800, a Hold up button, Start live before both are ready, lines in Week 1, the tape's rows out of order, the wrong
// match words, a Rematch, a player row too tall, or a clipped name. Screenshots land in target/league-gameday.
//     node Tools/check-league-gameday.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { STATES, root, serve, openState, settle } from './lib/page-states.mjs';

const out = path.join(root, 'target/league-gameday');
mkdirSync(out, { recursive: true });
const TAPE = ['Points', 'Points allowed', 'Total yards', 'Passing yards', 'Rushing yards', 'Turnovers', 'Sacks', 'Defensive TDs'];
let failures = 0;

const { base, close } = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
for (const s of STATES.gameday) {
  const viewport = s.viewport || { width: 1280, height: 800 };
  const { page, errors, close: done } = await openState(browser, base, s, { viewport });
  await settle(page);
  await page.waitForSelector('.gd', { timeout: 4000 }).catch(() => {});
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join(out, `${s.name}.png`) });
  const p = [...errors];
  const t = await page.locator('#screen').innerText().catch(() => '');
  if (!(await page.locator('.gd').count())) p.push('no game day view');
  const m = await page.evaluate(() => { const e = document.getElementById('main'); return { side: e.scrollWidth - e.clientWidth, down: e.scrollHeight - e.clientHeight }; });
  if (m.side > 1) p.push(`scrolls sideways by ${m.side}px`);
  if (viewport.width >= 1280 && m.down > 1) p.push(`the page scrolls by ${m.down}px at 1280x800`);
  if (/\bHold up\b/.test(t)) p.push('shows Hold up');
  const start = page.locator('[data-key="room-start"]');
  if (await start.count()) {
    const off = await start.isDisabled();
    const ready = s.name === 'league-gameday-ready';
    if (ready && off) p.push('Start is disabled with both ready');
    if (!ready && !off) p.push('Start is live before both are ready');
  }
  if (viewport.width >= 1280) {
    const rows = await page.$$eval('.gd-pl', (n) => n.map((r) => r.getBoundingClientRect().height));
    if (rows.some((h) => h > 48.5)) p.push(`a player row is ${Math.max(...rows)}px tall`);
    const cut = await page.$$eval('.gd-who b', (n) => n.filter((b) => b.scrollWidth > b.clientWidth + 1 && getComputedStyle(b).textOverflow !== 'ellipsis').length);
    if (cut) p.push(`${cut} names overflow without an ellipsis`);
  }
  const lines = await page.$$eval('.gd-pl .gd-ln', (n) => n.map((x) => x.textContent));
  const tapeLabels = await page.$$eval('.gd-st-lab', (n) => n.map((x) => x.childNodes[1]?.textContent ?? ''));
  switch (s.name) {
    case 'league-gameday-week1': case 'league-gameday-rejoin':
      if (lines.some((l) => l !== '')) p.push(`Week 1 lines are not blank: ${JSON.stringify(lines.filter(Boolean))}`);
      if (!t.includes('No games yet')) p.push('the tape does not say No games yet');
      if (tapeLabels.length) p.push('Week 1 shows tape rows');
      break;
    case 'league-gameday-season':
      if (JSON.stringify(tapeLabels) !== JSON.stringify(TAPE)) p.push(`tape rows ${JSON.stringify(tapeLabels)}`);
      if (!lines.includes('482 yds · 5 TD · 1 INT')) p.push('no season line for the QB');
      if (!lines.includes('No stats kept')) p.push('the OL does not say No stats kept');
      if (!t.includes('Week 3 · Wolves Reign at Subway Surfers')) p.push('top line');
      if (!(await page.locator('img.gd-face').count())) p.push('no portraits drawn');
      break;
    case 'league-gameday-opp-missing':
      if (!t.includes('Lineup not available')) p.push('no Lineup not available');
      break;
    case 'league-gameday-stale':
      if (!t.includes('League site unreachable, showing saved data')) p.push('no stale line');
      break;
    case 'league-gameday-match':
      if (!t.includes('Game 2 of 3')) p.push('no Game 2 of 3');
      if (await page.locator('[data-key="room-start"]').count()) p.push('the room card is still drawn during the match');
      break;
    case 'league-gameday-final':
      if (!t.includes('Final') || !t.includes('Back to League')) p.push('no Final / Back to League');
      if (/Rematch/.test(t)) p.push('offers Rematch');
      break;
    case 'league-gameday-small': {
      const [a, b] = await page.$$eval('.gd-side', (n) => n.map((x) => x.getBoundingClientRect().top));
      if (!(b > a)) p.push('the two lineups do not stack at 800x560');
      break;
    }
    default: break;
  }
  if (p.length) { failures++; console.log(`FAIL ${s.name}: ${p.join('; ')}`); } else console.log(`ok   ${s.name}`);
  await done();
}
await browser.close();
await close();
console.log(failures ? `\ncheck-league-gameday: FAIL (${failures})` : `\ncheck-league-gameday: PASS (${STATES.gameday.length} state(s))`);
process.exit(failures ? 1 : 0);
