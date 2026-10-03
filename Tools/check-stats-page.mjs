// The Stats page (spec §8.6, §10, §13), the approved mv2 mockup (owner 10-03): Matches | Records at the top right; one
// 12-column grid whose rows' cards share their top and bottom edges with 16px gutters. Matches: Your Record (online
// only, Ranked and Casual halves from by_queue or the matches, Last 10) beside Elo (or Unranked while Elo is off);
// Your matches (All / Ranked / Casual / Offline, a queue column, a row opening the WHOLE match, focus kept across a
// refresh) beside Your players (five box-score numbers per role; the old value/unit shape as one number); the charts,
// each left out when its data is absent. Records: Single game and Career side by side, Team records, no queue switch
// (the data does not split by queue). W/L only, never "Best of 3", "ladder" or "Private". The small window, signed
// out, a refusal, and the deployed server's answers.
//     node Tools/check-stats-page.mjs [outdir]      (screens in target/p4-after by default)
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { serve, openState, root } from './lib/page-states.mjs';
import * as F from './fixtures/stats-fixtures.mjs';

const out = path.resolve(process.argv[2] || path.join(root, 'target/p4-after'));
mkdirSync(out, { recursive: true });
const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const STATE = { name: 'stats', url: 'stats.html', scope: 'main' };
const FULL = { stats_dashboard: F.dashboard({ lines: true, byQueue: true, week: 16 }) };
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, h: r.height }; });
const texts = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => n.textContent.trim()));
const noSideways = async (page) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
/** Every grid row: its cards start and end on one line, 16px apart, and fill the row's width. */
const rowsAligned = (page) => page.evaluate(() => [...document.querySelectorAll('#screen .vgrid')].map((g) => {
  const cs = [...g.children].map((c) => c.getBoundingClientRect());
  const gr = g.getBoundingClientRect();
  const ids = [...g.children].map((c) => c.id).join('+');
  const bad = [];
  for (const c of cs) { if (Math.abs(c.top - cs[0].top) > 0.5 || Math.abs(c.bottom - cs[0].bottom) > 0.5) bad.push('edges'); }
  for (let i = 1; i < cs.length; i++) if (Math.abs(cs[i].left - cs[i - 1].right - 16) > 0.6) bad.push(`gap ${cs[i].left - cs[i - 1].right}`);
  if (Math.abs(cs[0].left - gr.left) > 0.5 || Math.abs(cs.at(-1).right - gr.right) > 0.5) bad.push('width');
  return bad.length ? `${ids}: ${bad.join(', ')}` : '';
}).filter(Boolean));
/** No card leaves empty space under its last row: each card's content reaches its bottom (within 20px). */
const noDeadSpace = (page) => page.evaluate(() => [...document.querySelectorAll('#screen .vgrid > .vc')].map((c) => {
  const r = c.getBoundingClientRect();
  let bottom = r.top;
  const walk = (n) => { for (const k of n.children) { const b = k.getBoundingClientRect(); if (b.height && getComputedStyle(k).display !== 'contents') { const own = [...k.childNodes].some((x) => x.nodeType === 3 && x.textContent.trim()) || !k.children.length;
    if (own || k.matches('svg, .ch-area, .vc-tr, .pl-row, .rec-row, .m-row, .share, .wl-strip, .st-cmp-r, .vc-cells > div, .ch-days')) bottom = Math.max(bottom, b.bottom); else walk(k); } } };
  walk(c);
  return r.bottom - bottom > 20 ? `${c.id}: ${Math.round(r.bottom - bottom)}px empty` : '';
}).filter(Boolean));
const contentRules = async (page) => {
  const body = await page.locator('#screen').innerText();
  assert.doesNotMatch(body, /best of 3/i);
  assert.doesNotMatch(body, /ladder/i);
  assert.doesNotMatch(body, /private/i);
  assert.doesNotMatch(body, /NaN|undefined|null/);
  assert.doesNotMatch(await page.locator('#screen').evaluate((n) => [...n.querySelectorAll('[aria-label]')].map((x) => x.getAttribute('aria-label')).join(' ')), /private|best of 3|\btie\b/i, 'nor in the labels');
  assert.ok((await texts(page, '.m-res b')).every((t) => t === 'W' || t === 'L' || t === ''), 'results are W or L');
};
try {
  { // 1280×800, the live server's answers (full player rows, by_queue, week_delta)
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: FULL });
    await page.locator('#st-record').waitFor();
    await page.locator('#st-pergame').waitFor();
    assert.equal(await page.locator('.page-heading .purpose').textContent(), 'Every match you’ve played, where your Elo stands, and who’s carrying your team.');
    // the grid (mv2): rows of 7+5, 7+5, 8+4, 6+6, 8+4; each row's cards share their edges, 16px apart; no dead space
    assert.deepEqual(await page.locator('#screen .vgrid').evaluateAll((gs) => gs.map((g) => [...g.children].map((c) => `${c.id}:${c.className.match(/\bs(\d+)\b/)[1]}`).join(' '))),
      ['st-record:7 st-elo:5', 'st-matches:7 st-players:5', 'st-points:8 st-yards:4', 'st-days:6 st-teams:6', 'st-pergame:8 st-wl:4']);
    assert.deepEqual(await rowsAligned(page), []);
    assert.deepEqual(await noDeadSpace(page), []);
    assert.deepEqual([...new Set(await page.locator('.vc-h').evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().height)))], [48], 'every card header is 48px');
    // Your Record: online overall from by_queue (ranked 1-1 + casual 7-3), the halves with the server's points per game
    // the way to the player's own profile sits at the top, beside Matches | Records
    assert.equal(await page.locator('#st-record .vc-aside').textContent(), 'Online games only');
    const mine = page.locator('#stats-view [data-key="your-profile"]');
    assert.equal(await mine.textContent(), 'Your profile ›');
    assert.match(await mine.getAttribute('href'), /^profile\.html\?player=[0-9a-f-]+$/);
    assert.equal(await page.locator('.st-overall .vc-big').textContent(), '8-4');
    assert.deepEqual(await texts(page, '#st-record .st-half:not(.st-overall) .vc-mid'), ['1-1', '7-3']);
    assert.deepEqual(await texts(page, '#st-record .st-half:not(.st-overall) .vc-kv'), ['Elo1,184 · #7', 'Points / game23.8', 'Games10', 'Points / game21.6']);
    assert.equal(await page.locator('#st-record .k-label.qm.ranked').count(), 1, 'Ranked has its red dot');
    assert.equal(await page.locator('.st-streak').textContent(), 'Streak W2 · 12 games');
    assert.deepEqual(await texts(page, '.st-l10 .wl-strip span'), ['W', 'W', 'L', 'L', 'W', 'W', 'L', 'W', 'W', 'L']);
    assert.match(await page.locator('.st-l10-sum').textContent(), /^6-4 · \d+\.\d scored\d+\.\d allowed$/);
    // Elo: the value, Rank · Peak · This week, the curve with its peak
    assert.equal(await page.locator('.st-elo-v').textContent(), '1,184');
    assert.deepEqual(await texts(page, '.st-elo-meta > div'), ['Rank#7', 'Peak1,212', 'This week+16']);
    assert.equal(await page.locator('#st-elo .vc-aside').textContent(), 'Ranked · 13 games');
    assert.equal(await page.locator('#st-elo svg polyline.ch-stroke').count(), 1);
    assert.equal(await page.locator('#st-elo .ch-peak').textContent(), 'Peak 1,212');
    // Your matches: the queue column, eight rows that fill the card, the pager inside it
    assert.deepEqual(await texts(page, '#st-matches .m-hd span'), ['Result', 'Matchup', 'Opponent', 'Queue', 'Elo', 'Date']);
    const rows = page.locator('#st-matches .m-list > .m-row:not(.m-hd)');
    assert.equal(await rows.count(), 8);
    assert.equal(await page.locator('#st-matches .m-pager span').textContent(), 'Page 1 of 2');
    const row = (id) => page.locator(`[data-key="match-${id}"]`);
    assert.equal(await row('m-01').locator('.m-res').textContent(), 'W2–1');
    assert.equal(await row('m-01').locator('.qm').textContent(), 'Ranked');
    assert.match(await row('m-01').locator('.qm').getAttribute('class'), /\branked\b/);
    assert.equal(await row('m-01').locator('.m-elo').textContent(), '+16');
    assert.equal(await row('m-02').locator('.qm').textContent(), 'Casual');
    assert.equal(await row('m-02').locator('.m-elo').textContent(), '—');
    assert.equal(await row('g:g-031').locator('.qm').textContent(), 'Offline');
    assert.equal(await row('g:g-031').locator('.m-opp-n').textContent(), 'CPU');
    assert.equal(await page.locator('#st-matches small').count(), 0, 'no second line under a name: the queue is its own column');
    assert.equal(await row('m-04').locator('.m-elo').textContent(), '−18');
    assert.equal(await row('m-05').count(), 0, 'a private room is dropped from the list');
    assert.equal(await page.locator('#st-matches .chip, #st-matches [class*="chip"]').count(), 0, 'no chips');
    // Your players: five box-score numbers per role (Adam 10-03: C/ATT, CAR); Most played its role's own four
    const cells = await page.locator('#st-players .pl-row').evaluateAll((ns) => ns.map((n) => [n.querySelector('.pl-role').textContent, ...[...n.querySelectorAll('.pl-cells > span')].map((s) => `${s.firstChild.textContent} ${s.querySelector('small').textContent}`)]));
    assert.deepEqual(cells, [
      ['Best passer', '198/331 C/ATT', '3,108 YDS', '31 TD', '11 INT', '108.9 RTG'],
      ['Best rusher', '212 CAR', '1,715 YDS', '5.6 AVG', '12 TD', '61 LONG'],
      ['Best receiver', '98 REC', '1,714 YDS', '14.3 AVG', '14 TD', '74 LONG'],
      ['Best DL/LB', '71 TKL', '5 SCK', '9 TFL', '2 FF', '6 PBU'],
      ['Ballhawk', '7 INT', '14 PBU', '33 TKL', '1 TD'],
      ['Most played', '23 GAMES', '96 CAR', '535 YDS', '5.6 AVG', '5 TD']]);
    assert.equal(await page.locator('#st-players .pl-row').first().locator('.player-face').evaluate((n) => getComputedStyle(n).getPropertyValue('--tc').trim()), '#A71930');
    assert.match(await page.locator('[data-key="card-passer"]').getAttribute('href'), /^profile\.html\?player=[0-9a-f-]+$/);
    assert.match(await page.locator('[data-key="all-players"]').getAttribute('href'), /#stats$/);
    // the charts
    assert.equal(await page.locator('#st-points svg rect').count(), 30, 'fifteen online games with a score, two bars each');
    assert.match(await page.locator('#st-points .ch-legend').textContent(), /^Scored · \d+\.\dAllowed · \d+\.\dLast 15 online games$/);
    assert.equal(await page.locator('#st-yards .ch-donut .mid b').textContent(), '412');
    assert.deepEqual(await texts(page, '#st-yards .st-yv'), ['251.261%', '161.139%']);
    assert.equal(await page.locator('#st-days .ch-days > div').count(), 24);
    assert.equal(await page.locator('#st-days .ch-days i.W').count() + await page.locator('#st-days .ch-days i.L').count(), 12, 'one square per online match');
    assert.deepEqual(await texts(page, '#st-teams .st-team b'), ['Falcons', 'Pickup']);
    assert.deepEqual(await texts(page, '#st-pergame .k-label'), ['Yards / game', 'Passing', 'Rushing', 'Points / drive', 'First downs', 'Turnovers', 'Three-and-outs', 'Biggest lead']);
    assert.deepEqual(await texts(page, '#st-wl .st-cmp-r:not(.st-cmp-h) > span'), ['Points', 'Yards', 'Turnovers']);
    assert.deepEqual(await texts(page, '#st-wl .st-cmp-h b'), ['In 14 wins', 'In 9 losses']);
    // Matches | Records at the top right, beside the title, Matches picked
    assert.deepEqual(await texts(page, '#stats-view .tab'), ['Matches', 'Records']);
    assert.equal(await page.locator('#stats-view .tab[aria-pressed="true"]').textContent(), 'Matches');
    { const v = await rect(page, '#stats-view .tabs'), h = await rect(page, '.page-heading h1'), c = await rect(page, '#st-elo');
      assert.ok(Math.abs(v.right - c.right) < 1.5, `the switch ends on the content's right edge: ${v.right} vs ${c.right}`);
      assert.ok(v.top < h.bottom + 30, 'the switch sits beside the title'); }
    await contentRules(page);
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'stats-1280.png') });
    await page.evaluate(() => { document.getElementById('main').scrollTop = 700; });
    await page.waitForTimeout(200);
    await page.screenshot({ path: path.join(out, 'stats-1280-lower.png') });
    await page.evaluate(() => { document.getElementById('main').scrollTop = 0; });
    // a refresh (the 30 s timer, or a game ending) keeps the focus on the same row
    await row('m-02').focus();
    const before = await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'stats_dashboard').length);
    await page.evaluate(() => { document.dispatchEvent(new CustomEvent('session-state', { detail: true })); document.dispatchEvent(new CustomEvent('session-state', { detail: false })); });
    await page.waitForFunction((n) => window.__calls.filter((c) => c.cmd === 'stats_dashboard').length > n, before);
    await page.waitForTimeout(250);
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'match-m-02', 'focus kept across a refresh');
    // the pager
    await page.locator('[data-key="older"]').click();
    assert.equal(await page.locator('#st-matches .m-pager span').textContent(), 'Page 2 of 2');
    assert.equal(await rows.count(), 5, 'page 2: 13 listed matches (the private room dropped), 5 on the second page');
    assert.equal(await page.locator('[data-key="older"]').isDisabled(), true);
    await page.locator('[data-key="newer"]').click();
    // filters: Ranked, Casual, Offline (no chips; a plain segmented control)
    assert.deepEqual(await texts(page, '#st-matches .tabs .tab'), ['All', 'Ranked', 'Casual', 'Offline']);
    await page.locator('[data-key="filter-ranked"]').click();
    assert.deepEqual(await rows.evaluateAll((ns) => ns.map((n) => n.dataset.key)), ['match-m-01', 'match-m-04']);
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'filter-ranked', 'focus stays on the filter');
    await page.locator('[data-key="filter-offline"]').click();
    assert.deepEqual(await rows.evaluateAll((ns) => ns.map((n) => n.dataset.key)), ['match-g:g-031']);
    await page.locator('[data-key="filter-casual"]').click();
    assert.equal(await rows.count(), 8);
    assert.deepEqual(await rowsAligned(page), [], 'a short page keeps the row aligned');
    await page.locator('[data-key="filter-all"]').click();
    assert.deepEqual(errors, []);
    await close();
  }
  { // the deployed server before cc79302: best players send value + unit only, no by_queue, no week_delta. The record is
    // counted from the online matches; each player shows its one number; nothing invented.
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: F.oldDashboard() } });
    await page.locator('#st-record').waitFor();
    assert.equal(await page.locator('.st-overall .vc-big').textContent(), '8-4');
    assert.deepEqual(await texts(page, '#st-record .st-half:not(.st-overall) .vc-mid'), ['1-1', '7-3']);
    assert.deepEqual(await texts(page, '.st-elo-meta > div'), ['Rank#7', 'Peak1,212'], 'no week_delta: no This week');
    const cells = await page.locator('#st-players .pl-row').evaluateAll((ns) => ns.map((n) => [...n.querySelectorAll('.pl-cells > span')].map((s) => `${s.firstChild.textContent} ${s.querySelector('small').textContent}`)));
    assert.deepEqual(cells, [['3,108 PASS YDS'], ['1,715 RUSH YDS'], ['1,714 REC YDS'], ['96 TKL + SCK'], ['7 INT'], ['23 GAMES']]);
    assert.equal(await page.locator('.player-face').first().evaluate((n) => getComputedStyle(n).getPropertyValue('--tc').trim()), '', 'no colour sent, none invented');
    assert.deepEqual(await rowsAligned(page), []);
    await contentRules(page);
    await page.screenshot({ path: path.join(out, 'stats-1280-old-server.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // Elo off until launch: the Elo card says Unranked / Rankings start soon, no numbers, no curve; the ranked half no Elo
    const d = F.dashboard({ lines: true });
    d.you.elo = F.ELO_OFF;
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: d } });
    await page.locator('#st-record').waitFor();
    assert.equal(await page.locator('#st-elo .vc-mid').textContent(), 'Unranked');
    assert.equal(await page.locator('#st-elo .vc-sub').textContent(), 'Rankings start soon.');
    assert.equal(await page.locator('#st-elo svg').count(), 0);
    assert.doesNotMatch(await page.locator('#st-elo').innerText(), /\d/, 'no number on an unranked Elo card');
    assert.deepEqual((await texts(page, '#st-record .st-half:not(.st-overall) .vc-kv')).slice(0, 1), ['Games2']);
    assert.deepEqual(await rowsAligned(page), []);
    await page.screenshot({ path: path.join(out, 'stats-1280-elo-off.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // a profile without per game, by team or wins vs losses (and no player rows): those cards leave, partners take the row
    const p = F.profile();
    delete p.per_game; delete p.by_team; delete p.wins_vs_losses;
    const d = F.dashboard();
    d.best_players = {};
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: d, player_profile: p } });
    await page.locator('#st-record').waitFor();
    await page.waitForTimeout(300);
    assert.deepEqual(await page.locator('#screen .vgrid').evaluateAll((gs) => gs.map((g) => [...g.children].map((c) => `${c.id}:${c.className.match(/\bs(\d+)\b/)[1]}`).join(' '))),
      ['st-record:7 st-elo:5', 'st-matches:12', 'st-points:12', 'st-days:12']);
    assert.deepEqual(await rowsAligned(page), []);
    assert.deepEqual(errors, []);
    await close();
  }
  { // a row opens the WHOLE match on one page (owner 10-03): an offline game just that game, a best of 3 all of its games
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.locator('[data-key="match-g:g-031"]').click();
    await page.waitForURL(/game\.html\?/);
    const one = new URL(page.url());
    assert.deepEqual([one.searchParams.get('game'), one.searchParams.get('games'), one.searchParams.get('from')], ['g-031', null, 'stats.html']);
    await page.goto(`${srv.base}/stats.html`);
    await page.locator('[data-key="match-m-01"]').click();
    await page.waitForURL(/game\.html\?.*games=/);
    const u = new URL(page.url());
    assert.deepEqual([u.searchParams.get('game'), u.searchParams.get('games'), u.searchParams.get('series'), u.searchParams.get('from')],
      ['g-013', 'g-011,g-012,g-013', '2-1', 'stats.html'], 'every game of the match on one page');
    assert.deepEqual(errors, []);
    await close();
  }
  { // Records: Single game and Career side by side, Team records under them; no Ranked | Casual switch (the records
    // do not split by queue); the switch follows the address (#records); Back from a record's game
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: FULL });
    await page.locator('#st-record').waitFor();
    await page.locator('[data-key="view-records"]').click();
    await page.locator('#rec-single').waitFor();
    assert.equal(await page.evaluate(() => location.hash), '#records');
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'view-records', 'focus stays on the switch');
    assert.equal(await page.locator('#st-record').count(), 0, 'Records replaces the matches');
    assert.equal(await page.locator('.st-records .tabs, .rec-pill, [class*="chip"]').count(), 0, 'no queue switch and no chips: the data does not split by queue');
    assert.deepEqual(await page.locator('.st-records .vgrid').evaluateAll((gs) => gs.map((g) => [...g.children].map((c) => c.id).join(' '))), ['rec-single rec-career', 'rec-team']);
    assert.deepEqual(await rowsAligned(page), []);
    assert.deepEqual(await texts(page, '#rec-single .vc-th span'), ['Record', 'Player', 'Best', 'Date']);
    assert.deepEqual(await texts(page, '#rec-career .vc-th span'), ['Record', 'Player', 'Total', 'Per game']);
    assert.deepEqual(await texts(page, '#rec-single .rec-k'), ['Passing Yards', 'Passing TD', 'Rushing Yards', 'Rushing TD', 'Receptions', 'Receiving Yards', 'Receiving TD', 'Tackles', 'Sacks', 'Interceptions', 'Longest run']);
    assert.doesNotMatch(await page.locator('.st-records').textContent(), /\byds\b/i, '"Yards" spelled out in record names');
    const first = page.locator('#rec-single .rec-row').first();
    assert.deepEqual(await first.locator(':scope > *').evaluateAll((ns) => ns.map((n) => n.textContent.trim())), ['Passing Yards', 'M. Penix Jr.', '288', 'Sep 19']);
    assert.match(await first.getAttribute('aria-label'), /vs Playz/, 'the game it was set in, in the label');
    assert.match(await first.getAttribute('href'), /^game\.html\?game=g-011&from=stats\.html%23records$/, 'a record opens its game, Back returns to Records');
    assert.deepEqual(await page.locator('#rec-career .rec-row').evaluateAll((ns) => ns.map((n) => [...n.children].map((c) => c.textContent.trim()).join(' | '))), [
      'Passing Yards | M. Penix Jr. | 2,210 | 110.5', 'Passing TD | M. Penix Jr. | 31 | 1.6', 'Passer Rating | M. Penix Jr. | 110.5 | —',
      'Rushing Yards | S. Barkley | 1,180 | 59.0', 'Rushing TD | S. Barkley | 12 | 0.6', 'Receptions | D. London | 98 | 4.9', 'Receiving Yards | D. London | 1,402 | 70.1',
      'Receiving TD | D. London | 14 | 0.7', 'Tackles | N. Bolton | 71 | 3.6', 'Sacks | N. Bolton | 5 | 0.3', 'Interceptions | A.J. Terrell | 7 | 0.4', 'Total TD | D. London | 14 | 0.7']);
    assert.deepEqual(await texts(page, '#rec-team .k-label'), ['Longest TD', 'Most points', 'Biggest win', 'Biggest comeback']);
    assert.deepEqual(await texts(page, '#rec-team b'), ['99', '49', '+42', '14']);
    assert.deepEqual(await texts(page, '#rec-team .n'), ['Sep 22 vs Jock', 'Sep 24 vs CPU', 'Sep 24 vs CPU', 'Sep 26 vs Playz']);
    await contentRules(page);
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'stats-1280-records.png') });
    // a refresh keeps the view and the focus
    await page.locator('#rec-single .rec-row').nth(2).focus();
    const key = await page.evaluate(() => document.activeElement?.getAttribute('data-key'));
    await page.evaluate(() => { document.dispatchEvent(new CustomEvent('session-state', { detail: true })); document.dispatchEvent(new CustomEvent('session-state', { detail: false })); });
    await page.waitForTimeout(500);
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), key);
    await page.locator('[data-key="view-matches"]').click();
    await page.locator('#st-record').waitFor();
    assert.equal(await page.evaluate(() => location.hash), '');
    assert.deepEqual(errors, []);
    await close();
  }
  { // opened at #records (Back from a record's game) it shows Records
    const { page, errors, close } = await openState(browser, srv.base, { ...STATE, url: 'stats.html#records' });
    await page.locator('#rec-single').waitFor();
    assert.equal(await page.locator('#stats-view .tab[aria-pressed="true"]').textContent(), 'Records');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window: one column, long names end in an ellipsis, nothing scrolls sideways
    const { page, errors, close } = await openState(browser, srv.base, STATE, { viewport: { width: 800, height: 560 }, data: FULL });
    await page.locator('#st-record').waitFor();
    await noSideways(page);
    assert.ok((await rect(page, '#st-elo')).top > (await rect(page, '#st-record')).bottom - 1, 'one column in a small window');
    await page.evaluate(() => { document.querySelector('[data-key="match-m-01"] .m-opp-n').textContent = 'A player with an unusually long display name that goes on'; });
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'stats-800.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // a controller reaches the filters, the rows, the pager and the players
    const { page, errors, close } = await openState(browser, srv.base, STATE, { pad: true, data: FULL });
    await page.locator('#st-record').waitFor();
    const press = async (b) => {
      await page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; }, b);
      await page.waitForTimeout(90);
      await page.evaluate((b) => { pads[0].buttons[b] = { pressed: false, value: 0 }; }, b);
      await page.waitForTimeout(90);
    };
    const key = () => page.evaluate(() => document.activeElement?.getAttribute('data-key'));
    await page.locator('[data-key="match-m-01"]').focus();
    await press(12);
    assert.match(String(await key()), /^filter-/, 'Up from the first match reaches the filters');
    await page.locator('#st-matches .m-list > .m-row').last().focus();
    await press(13);
    assert.equal(await key(), 'older', 'the last row goes down to the pager');
    await page.locator('[data-key="match-m-01"]').focus();
    await press(15);
    assert.match(String(await key()), /^card-|^all-players$/, `Right from a match reaches Your players: ${await key()}`);
    assert.deepEqual(errors, []);
    await close();
  }
  { // signed out: the way in, never an empty grid
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { signed_in: false }, identity_status: { signed_in: false },
      identity_sign_in_start: { device_code: 'dev-1', user_code: 'ABCD-1234', verify_url: 'https://discord.com/activate', interval: 60 } } });
    await page.getByRole('heading', { name: 'Sign in to see your stats', exact: true }).waitFor();
    const h = await page.locator('.st-signin h2').evaluate((n) => { const c = getComputedStyle(n); return { size: parseFloat(c.fontSize), weight: +c.fontWeight }; });
    assert.ok(h.size >= 28 && h.weight >= 700, `sign-in heading is big and bold (${h.size}px/${h.weight})`);
    const go = page.locator('.st-signin-go');
    assert.equal(await go.textContent(), 'Sign in with Discord');
    assert.ok((await go.boundingBox()).height >= 52, 'sign-in button is the big one');
    assert.equal(await page.locator('.vgrid').count(), 0);
    assert.equal(await page.locator('.st-account').count(), 0, 'no Profile & sync before there is an account');
    await page.getByText('2 games will upload when you sign in.', { exact: true }).waitFor();
    { const card = await page.locator('.st-signin').boundingBox(), main = await page.locator('#main').boundingBox();
      assert.ok(Math.abs((card.x + card.width / 2) - (main.x + main.width / 2)) <= 2, 'sign-in card is centred'); }
    await page.screenshot({ path: path.join(out, 'stats-signed-out.png') });
    await go.click();
    await page.getByText('Your code is ABCD-1234', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.getByRole('heading', { name: 'Sign in to see your stats', exact: true }).waitFor();
    assert.deepEqual(errors, []);
    await close();
  }
  { // the server refused: its reason, and nothing invented
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { signed_in: true, ok: false, error: 'The server is busy.' } } });
    await page.getByText('The server is busy.', { exact: true }).waitFor();
    assert.equal(await page.locator('.vgrid').count(), 0);
    assert.equal(await page.locator('.st-account').count(), 1);
    assert.deepEqual(errors, []);
    await close();
  }
  { // no games yet
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { ...F.dashboard(), matches: [] } } });
    await page.getByText('No games yet. Play one and it shows up here.', { exact: true }).waitFor();
    assert.equal(await page.locator('.vgrid').count(), 0);
    assert.deepEqual(errors, []);
    await close();
  }
  { // only private rooms played (all dropped): neutral words, never "No games yet" and never "Private"
    const room = F.allMatches().find((m) => m.queue === 'private');
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { ...F.dashboard(), matches: [room] } } });
    await page.getByText('No games to show yet.', { exact: true }).waitFor();
    const body = await page.locator('#screen').innerText();
    assert.doesNotMatch(body, /No games yet/);
    assert.doesNotMatch(body, /private/i);
    assert.equal(await page.locator('.vgrid').count(), 0);
    assert.deepEqual(errors, []);
    await close();
  }
  { // only offline games: no online record (the card leaves), Elo takes the row, the list still shows them
    const off = F.allMatches().filter((m) => m.mode === 'offline');
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { ...F.dashboard(), matches: off } } });
    await page.locator('#st-matches').waitFor();
    assert.equal(await page.locator('#st-record').count(), 0);
    assert.match(await page.locator('#st-elo').getAttribute('class'), /\bs12\b/);
    assert.deepEqual(await rowsAligned(page), []);
    assert.deepEqual(errors, []);
    await close();
  }
  console.log(`check-stats-page: PASS (screens in ${out})`);
} finally { await browser.close(); await srv.close(); }
