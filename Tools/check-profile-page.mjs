// The Profile page, the online player hub (spec §8.7, §10, §13): Back first; the card (status, summary line, Share /
// View on web, the live card); the overview (three columns, big numbers on one line, visuals ending on one line); Match
// history beside Career / Leaderboard / Records (first cards start and end on one line; 40px headers); the queue filter
// and the pager; W/L only, never "Best of 3", "ladder" or "Private"; the deployed server's answers; the small window;
// the model edge cases (P4-23); letter avatars coloured per player; the D-pad through the card, history and tabs;
// Full stats (one table at a time, top five, Show all, Total, shared column edges, sorting) and More stats; the links
// from Stats (#passing, #rushing, #receiving, #defense, #career) landing on their table or section.
//     node Tools/check-profile-page.mjs [outdir]      (screens in target/p4-after by default)
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { serve, openState, root, settle } from './lib/page-states.mjs';
import * as F from './fixtures/stats-fixtures.mjs';

const out = path.resolve(process.argv[2] || path.join(root, 'target/p4-after'));
mkdirSync(out, { recursive: true });
const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const STATE = { name: 'profile', url: `profile.html?player=${F.ME}`, scope: 'main' };
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, h: r.height }; });
const texts = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => n.textContent.trim()));
const noSideways = async (page) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
const contentRules = async (page) => {
  const body = await page.locator('#screen').innerText();
  assert.doesNotMatch(body, /best of 3/i, 'never "Best of 3"');
  assert.doesNotMatch(body, /ladder/i, '"Leaderboard", never "ladder"');
  assert.doesNotMatch(body, /private/i, 'no "Private" on Profile');
  const results = await texts(page, '.m-res b');
  if (await page.locator('#history .m-list > .m-row:not(.m-hd)').count()) assert.ok(results.length > 0, 'match rows show their result letters');
  assert.ok(results.every((t) => t === 'W' || t === 'L' || t === ''), 'results are W or L');
  // no ties: no record anywhere reads W–L–T
  assert.doesNotMatch(body, /\b\d+–\d+–\d+\b/, 'records are W–L only, never a tie');
};
const NONE = 'No online matches to show yet.';
// History rows keep their natural height (56px; a best of 3's games 40px): the card fills, never the rows (P4-24).
const rowHeights = (page) => page.locator('#history .m-list > .m-row:not(.m-hd)').evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().height));
const rowsNatural = async (page, what) => { const hs = await rowHeights(page); assert.ok(hs.length && hs.every((h) => h <= 56.5), `${what}: every history row is at most 56px: ${hs}`); };
try {
  { // 1280×800, the redesign's full answers, the owner looking at their own profile
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.locator('.pf-card').waitFor();
    await page.screenshot({ path: path.join(out, 'profile-1280.png') });
    await page.evaluate(() => document.getElementById('history').scrollIntoView({ block: 'start' }));
    await page.screenshot({ path: path.join(out, 'profile-1280-lower.png') });
    await page.evaluate(() => { document.getElementById('main').scrollTop = 0; });
    assert.equal(await page.locator('#screen > :first-child').getAttribute('id'), 'back', 'Back comes first');
    assert.equal(await page.locator('.pf-name').textContent(), 'adamknowsball');
    assert.equal(await page.locator('.pf-status').textContent(), '● In a match');
    assert.equal(await page.locator('.pf-pic .presence-dot').count(), 1);
    assert.deepEqual(await texts(page, '.pf-sum > span'), ['#7 on the leaderboard', '1,184 Elo', '60.9% win rate']);
    assert.deepEqual(await texts(page, '.pf-acts .btn'), ['Share profile', 'View on web']);
    assert.equal(await page.locator('#plays-as').count(), 1, 'the owner picks Plays as');
    const pic = await rect(page, '.pf-pic'), id = await rect(page, '.pf-id');
    assert.equal(Math.round(pic.h), 104);
    assert.ok(Math.abs((pic.top + pic.bottom) / 2 - (id.top + id.bottom) / 2) < 2, 'the avatar is centred on the identity block');
    // the live card
    assert.equal(await page.locator('.pf-live .q').textContent(), 'Ranked · Game 2 of 3');
    assert.equal(await page.locator('.pf-live strong').textContent(), '14 – 7');
    assert.equal(await page.locator('.pf-live-bot > span').textContent(), 'vs Mikel · Series 1–0');
    const live = await rect(page, '.pf-live'), card = await rect(page, '.pf-card');
    assert.ok(live.top > card.top && live.bottom < card.bottom, 'the live card sits inside the profile card');
    // overview: three identical columns; big numbers on one line; the visuals end on one line
    assert.deepEqual(await texts(page, '.pf-ovh h2'), ['Elo', 'Record', 'Last 10 games']);
    assert.deepEqual(await texts(page, '.pf-big'), ['1,184+16', '14–960.9%', '6–4+6.4']);
    assert.equal(await page.locator('.pf-ov').count(), 1, 'one glass panel');
    const bigSize = await page.locator('.pf-big > span').evaluateAll((ns) => [...new Set(ns.map((n) => getComputedStyle(n).fontSize))]);
    assert.deepEqual(bigSize, ['34px'], 'the big numbers are 34px');
    const bigs = await page.locator('.pf-big').evaluateAll((ns) => ns.map((n) => Math.round(n.getBoundingClientRect().top)));
    assert.equal(new Set(bigs).size, 1, `the big numbers share a line: ${bigs}`);
    const ends = await page.evaluate(() => ['.pf-spark', '.pf-bars', '.pf-pair'].map((s) => Math.round(document.querySelector(s).getBoundingClientRect().bottom)));
    assert.ok(Math.max(...ends) - Math.min(...ends) <= 1, `the visuals end on one line: ${ends}`);
    const widths = await page.locator('.pf-ovc').evaluateAll((ns) => ns.map((n) => Math.round(n.getBoundingClientRect().width)));
    assert.ok(Math.max(...widths) - Math.min(...widths) <= 1, `three identical columns: ${widths}`);
    assert.deepEqual(await texts(page, '.pf-bar-row > :first-child'), ['Ranked', 'Casual']);
    assert.deepEqual(await texts(page, '.pf-bar-row b'), ['8–5', '6–4']);
    // two columns: first cards start together and the columns end together; 40px section headers
    const hl = await rect(page, '#history .m-list');
    const first = await rect(page, '.pf-side > section:first-child > .glass');
    const last = await rect(page, '.pf-side > section:last-child > .glass');
    assert.ok(Math.abs(hl.top - first.top) < 1, `first cards start together: ${hl.top} vs ${first.top}`);
    assert.ok(Math.abs(hl.bottom - last.bottom) < 1, `the columns end together: ${hl.bottom} vs ${last.bottom}`);
    assert.deepEqual([...new Set(await page.locator('.sec-head').evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().height)))], [40]);
    await rowsNatural(page, 'ten rows');
    const recH = await page.locator('#records .pf-rec:not([hidden])').evaluateAll((ns) => ns.map((n) => Math.round(n.getBoundingClientRect().height)));
    assert.deepEqual([...new Set(recH)], [38], `Records rows keep their natural height: ${recH}`);
    // letter avatars (no Discord picture) are coloured per player, the same player always the same colour (mockup)
    const letters = await page.locator('span.avatar').evaluateAll((ns) => ns.map((n) => ({ who: n.textContent, bg: getComputedStyle(n).backgroundImage })));
    const byWho = new Map();
    for (const l of letters) { if (byWho.has(l.who)) assert.equal(byWho.get(l.who), l.bg, `one colour per player (${l.who})`); byWho.set(l.who, l.bg); }
    assert.ok(new Set(byWho.values()).size >= 2, `letter avatars vary per player: ${JSON.stringify([...byWho])}`);
    // Match history: the queue as the second line, the pager in the card, more pages loaded as needed
    const rows = page.locator('#history .m-list > .m-row:not(.m-hd)');
    assert.equal(await rows.count(), 10);
    assert.equal(await page.locator('[data-key="match-m-01"] small').textContent(), 'Ranked');
    assert.equal(await page.locator('[data-key="match-m-02"] small').textContent(), 'Casual');
    assert.equal(await page.locator('#history .m-pager span').textContent(), 'Page 1');
    await page.locator('#history [data-key="older"]').click();
    await page.waitForFunction(() => document.querySelector('#history .m-pager span')?.textContent === 'Page 2 of 2');
    assert.equal(await rows.count(), 2);
    assert.equal(await page.locator('#history [data-key="older"]').isDisabled(), true);
    await page.locator('#history [data-key="newer"]').click();
    await page.getByRole('button', { name: 'Ranked', exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll('#history .m-list > .m-row:not(.m-hd)').length === 2);
    assert.deepEqual(await rows.evaluateAll((ns) => ns.map((n) => n.dataset.key)), ['match-m-01', 'match-m-04']);
    assert.equal(await page.locator('#history .m-pager').count(), 0, 'one page: no pager');
    await rowsNatural(page, 'the Ranked filter');
    const hl2 = await rect(page, '#history .m-list'), last2 = await rect(page, '.pf-side > section:last-child > .glass');
    assert.ok(Math.abs(hl2.bottom - last2.bottom) < 1, `the card still meets the right column: ${hl2.bottom} vs ${last2.bottom}`);
    await page.evaluate(() => document.getElementById('history').scrollIntoView({ block: 'start' }));
    await page.screenshot({ path: path.join(out, 'profile-1280-ranked.png') });
    await page.getByRole('button', { name: 'All', exact: true }).click();
    // Career: nine spelled-out tiles, rank among the player base, per game
    assert.deepEqual(await texts(page, '#career .k-label'), ['Passing Yards', 'Passing TD', 'Passer Rating', 'Rushing Yards', 'Receiving Yards', 'Total TD', 'Tackles', 'Sacks', 'Interceptions']);
    assert.equal(await page.locator('#career .pf-car > div b.num').first().textContent(), '3,021');
    assert.equal(await page.locator('#career .pf-car-s').first().textContent(), '#3 · 125.9 / G');
    assert.equal(await page.locator('#career .pf-note').textContent(), 'Rank among 214 online players');
    // Leaderboard: #5 to #9 around the player; their own row reads You
    assert.deepEqual(await texts(page, '.pf-lb-rank'), ['#5', '#6', '#7', '#8', '#9']);
    assert.equal(await page.locator('.pf-lb-row.me .pf-lb-name').textContent(), 'You');
    // Records: five, then All ›
    assert.equal(await page.locator('#records .pf-rec:not([hidden])').count(), 5);
    assert.equal(await page.locator('#records .sec-link').textContent(), 'All ›');
    await contentRules(page);
    await noSideways(page);
    assert.deepEqual(errors, []);
    // an opponent's name opens their profile (through the game list; a Leaderboard row links straight to it)
    assert.equal(await page.locator('.pf-lb-row[data-key="lb-1b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e"]').getAttribute('href'), 'profile.html?player=1b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e');
    // Live feed opens the native live view
    await page.getByRole('button', { name: 'Live feed', exact: true }).click();
    await page.waitForURL(/live\.html\?match=dc5226af-d3b8-4c0b-89e8-1c53b2972a30$/);
    await close();
  }
  { // Full stats: one table at a time, top five, "Show all N players", a Total row; every table on one column grid
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.locator('#full-stats .tbl').waitFor();
    assert.deepEqual(await texts(page, '#full-stats .tab'), ['Passing', 'Rushing', 'Receiving', 'Defense']);
    assert.deepEqual(await texts(page, '#full-stats thead th'), ['Player', 'C/ATT', 'YDS', 'TD', 'INT', 'AIR', 'YAC', 'Y/A', 'RTG', 'LNG']);
    const body = page.locator('#full-stats tbody tr:not(.more):not(.total)');
    assert.equal(await body.count(), 5);
    assert.equal(await page.locator('#full-stats .pf-showall').textContent(), 'Show all 7 players');
    assert.deepEqual(await texts(page, '#full-stats tr.total > *'), ['Total', '212/371', '3,021', '40', '18', '1,900', '1,121', '8.1', '104.2', '74']);
    const grid = async () => { await settle(page); return page.evaluate(() => { const ths = [...document.querySelectorAll('#full-stats thead th')]; return [Math.round(ths[0].getBoundingClientRect().width), Math.round(ths[1].getBoundingClientRect().left)]; }); };
    const passing = await grid();
    assert.equal(passing[0], 220, 'the shared 220px name column');
    await page.locator('#full-stats .pf-showall').click();
    assert.equal(await body.count(), 7);
    assert.equal(await page.locator('#full-stats .pf-showall').count(), 0, 'the reveal row leaves once every player shows');
    for (const t of ['Rushing', 'Receiving', 'Defense']) {
      await page.getByRole('button', { name: t, exact: true }).click();
      assert.deepEqual(await grid(), passing, `the ${t} table shares the Passing table's column edges`);
    }
    assert.deepEqual(await texts(page, '#full-stats thead th'), ['Player', 'TKL', 'TFL', 'SCK', 'INT', 'PBU', 'FF', 'FR', 'TD']);
    await page.getByRole('button', { name: 'Receiving', exact: true }).click();
    assert.deepEqual(await texts(page, '#full-stats thead th'), ['Player', 'REC', 'YDS', 'AVG', 'YAC', 'TD', 'LNG', 'DRP']);
    await page.getByRole('button', { name: 'Rushing', exact: true }).click();
    assert.deepEqual(await texts(page, '#full-stats thead th'), ['Player', 'CAR', 'YDS', 'AVG', 'TD', 'LNG', 'BTK', 'YCO']);
    assert.deepEqual(await grid(), passing, 'the tables share their column edges');
    assert.deepEqual(await texts(page, '#full-stats tr.total > *'), ['Total', '308', '1,715', '5.6', '17', '61', '45', '770']);
    // sorting by a column still works (it did before the redesign)
    const firstName = () => page.locator('#full-stats tbody tr').first().locator('.who > span:last-child').textContent();
    await page.locator('[data-key="sort-rushing-yds"]').click();
    assert.equal(await firstName(), 'S. Barkley');
    assert.equal(await page.locator('[data-key="sort-rushing-yds"]').evaluate((n) => document.activeElement === n), true, 'focus stays on the sort button');
    await page.locator('[data-key="sort-rushing-yds"]').click();
    assert.equal(await firstName(), 'B. Robinson');
    // More stats keeps every number the old profile showed, and the old profile's Best players (P4-5, P4-25)
    assert.deepEqual(await texts(page, '#more h3'), ['Head to head', 'Your players', 'By team', 'Per game', 'Best QB → receiver', 'Laterals & pitches', 'In wins vs losses', 'By game type']);
    assert.equal(await page.locator('#h2h a.who').getAttribute('href'), 'profile.html?player=1b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e', 'an opponent opens their profile');
    assert.deepEqual(await texts(page, '#more-players .pf-pl-name'), ['M. Penix Jr.', 'S. Barkley', 'D. London', 'N. Bolton', 'A.J. Terrell', 'B. Robinson']);
    assert.deepEqual(await texts(page, '#by-type tbody th'), ['Normal']);
    // Career's "Full stats ›" goes there
    await page.locator('#career .sec-link').click();
    await page.waitForFunction(() => { const t = document.getElementById('full-stats').getBoundingClientRect().top; return t >= -2 && t < 120; });
    await contentRules(page);
    await noSideways(page);
    await page.getByRole('button', { name: 'Passing', exact: true }).click();
    await settle(page);
    await page.screenshot({ path: path.join(out, 'profile-1280-full-stats.png') });
    for (const t of ['Rushing', 'Receiving', 'Defense']) {
      await page.getByRole('button', { name: t, exact: true }).click();
      await settle(page);
      await page.screenshot({ path: path.join(out, `profile-1280-full-stats-${t.toLowerCase()}.png`) });
    }
    await page.evaluate(() => document.getElementById('more').scrollIntoView({ block: 'start' }));
    await settle(page);
    await page.screenshot({ path: path.join(out, 'profile-1280-more.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // a link from Stats (#defense) opens that table and scrolls to Full stats
    const { page, errors, close } = await openState(browser, srv.base, { ...STATE, url: `${STATE.url}#defense` });
    await page.locator('#full-stats .tbl').waitFor();
    assert.equal(await page.locator('#full-stats .tab[aria-pressed="true"]').textContent(), 'Defense');
    await page.waitForFunction(() => { const t = document.getElementById('full-stats').getBoundingClientRect().top; return t >= -2 && t < 120; });
    assert.deepEqual(errors, []);
    await close();
  }
  { // every link Stats' Your players draws lands on its table (passing, rushing, receiving) or section (career)
    for (const [hash, tab] of [['passing', 'Passing'], ['rushing', 'Rushing'], ['receiving', 'Receiving']]) {
      const { page, errors, close } = await openState(browser, srv.base, { ...STATE, url: `${STATE.url}#${hash}` });
      await page.locator('#full-stats .tbl').waitFor();
      assert.equal(await page.locator('#full-stats .tab[aria-pressed="true"]').textContent(), tab);
      await page.waitForFunction(() => { const t = document.getElementById('full-stats').getBoundingClientRect().top; return t >= -2 && t < 120; });
      assert.deepEqual(errors, []);
      await close();
    }
    const { page, errors, close } = await openState(browser, srv.base, { ...STATE, url: `${STATE.url}#career` });
    await page.locator('#career').waitFor();
    await page.waitForFunction(() => { const t = document.getElementById('career').getBoundingClientRect().top; return t >= -2 && t < 120; });
    assert.deepEqual(errors, []);
    await close();
  }
  { // More stats' long lists show five rows and "Show all N" (as the old profile capped them); H2H "Last" shows a
    // one-game match's score, a series' date (P4-26)
    const p = F.profile();
    const base = p.head_to_head[0];
    p.head_to_head = Array.from({ length: 7 }, (_, i) => ({ ...structuredClone(base), player_id: `${'0123456'[i]}b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e`, display_name: `Rival ${i + 1}` }));
    Object.assign(p.head_to_head[0].last, { game_id: 'g:0b2c3d4e', score: [35, 14] });
    p.by_team = Array.from({ length: 6 }, (_, i) => ({ ...structuredClone(p.by_team[0]), key: `tgid:${i}`, name: `Team ${i + 1}` }));
    p.combos = Array.from({ length: 8 }, (_, i) => ({ ...structuredClone(p.combos[0]), rec: 60 - i }));
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p } });
    await page.locator('#more').waitFor();
    for (const [id, n, what] of [['h2h', 7, 'opponents'], ['by-team', 6, 'teams'], ['combos', 8, 'combos']]) {
      const rows = page.locator(`#${id} tbody tr:not(.more)`);
      assert.equal(await rows.filter({ visible: true }).count(), 5, `${id}: five rows first`);
      const b = page.locator(`#${id} .pf-showall`);
      assert.equal(await b.textContent(), `Show all ${n} ${what}`);
      await b.click();
      assert.equal(await rows.filter({ visible: true }).count(), n, `${id}: every row after Show all`);
      assert.equal(await b.textContent(), 'Show top 5');
      assert.equal(await b.evaluate((x) => document.activeElement === x), true, `${id}: focus stays on the button`);
    }
    assert.equal(await page.locator('#h2h tbody tr').nth(0).locator('td').nth(3).textContent(), 'W 35–14', 'a one-game match shows its score');
    assert.equal(await page.locator('#h2h tbody tr').nth(1).locator('td').nth(3).textContent(), 'W Sep 26', 'a series shows its date');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // 800×560, a pad: Right across Full stats' headings keeps each one wholly in view, clear of the sticky Player
    // column; Head to head fits without scrolling (P4-26)
    const { page, errors, close } = await openState(browser, srv.base, STATE, { viewport: { width: 800, height: 560 }, pad: true });
    await page.locator('#full-stats .tbl').waitFor();
    const press = async (b) => {
      await page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; }, b);
      await page.waitForTimeout(90);
      await page.evaluate((b) => { pads[0].buttons[b] = { pressed: false, value: 0 }; }, b);
      await page.waitForTimeout(90);
    };
    const key = () => page.evaluate(() => document.activeElement?.getAttribute('data-key'));
    const inView = () => page.evaluate(() => {
      const box = document.querySelector('#full-stats .pf-tbl').getBoundingClientRect(), r = document.activeElement.closest('th').getBoundingClientRect();
      const name = document.querySelector('#full-stats thead th').getBoundingClientRect();
      return r.left >= Math.max(box.left, name.right) - 1 && r.right <= box.right + 1 ? '' : `${document.activeElement.textContent} ${Math.round(r.left)}..${Math.round(r.right)} not in ${Math.round(name.right)}..${Math.round(box.right)}`;
    });
    await page.locator('[data-key="sort-passing-name"]').focus();
    const seen = [];
    for (let i = 0; i < 12 && (await key()) !== 'sort-passing-long'; i++) {
      await press(15);
      await settle(page);
      seen.push(await key());
      assert.equal(await inView(), '', `the focused heading is wholly in view: ${seen}`);
    }
    assert.equal(await key(), 'sort-passing-long', `Right reaches the last heading: ${seen}`);
    assert.ok(await page.evaluate(() => document.querySelector('#full-stats .pf-tbl').scrollLeft > 0), 'the table scrolled to show it');
    await page.screenshot({ path: path.join(out, 'profile-800-full-stats-pad-right.png') });
    // back Left to C/ATT: it comes out from under the sticky Player column
    for (let i = 0; i < 12 && (await key()) !== 'sort-passing-cmp'; i++) { await press(14); await settle(page); assert.equal(await inView(), '', 'Left keeps the heading in view'); }
    assert.equal(await key(), 'sort-passing-cmp');
    const h2h = await page.locator('#h2h').evaluate((n) => [n.scrollWidth, n.clientWidth]);
    assert.ok(h2h[0] <= h2h[1] + 1, `Head to head does not scroll at 800: ${h2h}`);
    assert.deepEqual(await texts(page, '#h2h thead th'), ['Opponent', 'Record', 'PTS', 'Allowed', 'Last', 'Best win', 'Worst loss']);
    assert.deepEqual(await page.locator('#h2h thead th').evaluateAll((ns) => ns.filter((n) => n.offsetParent).map((n) => n.textContent)), ['Opponent', 'Record', 'PTS', 'Last', 'Best win']);
    await page.screenshot({ path: path.join(out, 'profile-800-full-stats-pad.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // a private room is never drawn in More stats (P4-3): not in By game type, not as a team
    const p = F.profile();
    p.by_game_type.push({ key: 'private', name: 'Private room', games: 3, wins: 2, losses: 1, win_pct: 0.667, pts_per_game: 30 });
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p } });
    await page.locator('#more').waitFor();
    assert.deepEqual(await texts(page, '#by-type tbody th'), ['Normal']);
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window: Full stats and More stats in one column, no sideways page scroll, no number cut off (a wide
    // table scrolls inside its own card)
    const { page, errors, close } = await openState(browser, srv.base, STATE, { viewport: { width: 800, height: 560 } });
    const clipped = () => page.locator('#full-stats td, #full-stats thead th, #more td, #more thead th').evaluateAll((ns) =>
      ns.filter((n) => n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent.trim()));
    await page.locator('#full-stats .tbl').waitFor();
    await page.evaluate(() => document.getElementById('full-stats').scrollIntoView({ block: 'start' }));
    await settle(page);
    await noSideways(page);
    assert.deepEqual(await clipped(), [], 'Passing: no number or heading cut off');
    await page.screenshot({ path: path.join(out, 'profile-800-full-stats.png') });
    for (const t of ['Rushing', 'Receiving', 'Defense']) {
      await page.getByRole('button', { name: t, exact: true }).click();
      await settle(page);
      await noSideways(page);
      assert.deepEqual(await clipped(), [], `${t}: no number or heading cut off`);
      await page.screenshot({ path: path.join(out, `profile-800-full-stats-${t.toLowerCase()}.png`) });
    }
    await page.evaluate(() => document.getElementById('more').scrollIntoView({ block: 'start' }));
    await settle(page);
    const cards = await page.locator('#more .pf-more-card').evaluateAll((ns) => ns.map((n) => Math.round(n.getBoundingClientRect().left)));
    assert.ok(cards.every((x) => x === cards[0]), `More stats is one column: ${cards}`);
    await page.screenshot({ path: path.join(out, 'profile-800-more.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // glass on: the same page over the window glass
    const { page, errors, close } = await openState(browser, srv.base, STATE, { glass: true });
    await page.locator('.pf-card').waitFor();
    await page.screenshot({ path: path.join(out, 'profile-1280-glass.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // online, not in a match: no live card; Back with nothing behind goes to Stats
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: F.profile({ live: false }) } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-live').count(), 0);
    assert.equal(await page.locator('.pf-status').textContent(), '● Online now');
    await page.screenshot({ path: path.join(out, 'profile-1280-idle.png') });
    await page.locator('#back').click();
    await page.waitForURL(/stats\.html$/);
    assert.deepEqual(errors, []);
    await close();
  }
  { // Back returns to the page this one was opened from
    const { page, errors, close } = await openState(browser, srv.base, { name: 'stats', url: 'stats.html', scope: 'main' });
    await page.locator(`a[href^="profile.html?player=${F.ME}"]`).first().click();
    await page.locator('.pf-card').waitFor();
    await page.locator('#back').click();
    await page.waitForURL(/stats\.html$/);
    await page.locator('.st-cols, #screen').first().waitFor();
    assert.deepEqual(errors, []);
    await close();
  }
  { // Career's "Full stats ›" scrolls in place: one Back still leaves the page (P4-26); the href stays for a middle-click
    const { page, errors, close } = await openState(browser, srv.base, { name: 'stats', url: 'stats.html', scope: 'main' });
    await page.locator(`a[href^="profile.html?player=${F.ME}"]`).first().click();
    await page.locator('#full-stats .tbl').waitFor();
    assert.equal(await page.locator('#career .sec-link').getAttribute('href'), '#full-stats');
    await page.locator('#career .sec-link').click();
    await page.waitForFunction(() => { const t = document.getElementById('full-stats').getBoundingClientRect().top; return t >= -2 && t < 120; });
    assert.equal(await page.evaluate(() => location.hash), '', 'no history entry for the in-page link');
    await page.locator('#back').click();
    await page.waitForURL(/stats\.html$/);
    assert.deepEqual(errors, []);
    await close();
  }
  { // someone else's profile: their row keeps their name, and only the owner picks Plays as
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { identity_status: { signed_in: true, display_name: 'Playz', player_id: '1b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e', show_offline: false } } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-lb-row.me .pf-lb-name').textContent(), 'adamknowsball');
    assert.equal(await page.locator('#plays-as').count(), 0);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the deployed server: everything it does not send is left out, never faked; its private room is dropped
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: F.oldProfile(), player_profile_all: F.allMatches().filter((m) => m.mode === 'online') } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-live').count(), 0);
    assert.equal(await page.locator('.pf-status').textContent(), '● Online now');
    assert.deepEqual(await texts(page, '.pf-ovh h2'), ['Elo', 'Record'], 'no Last 10 games without recent_form');
    assert.ok((await texts(page, '#career .pf-car-s')).every((t) => !t.includes('#')), 'no ranks without career_rank');
    assert.equal(await page.locator('#career .pf-note').count(), 0);
    assert.equal(await page.locator('#leaderboard').count(), 0, 'no Leaderboard without elo.around');
    assert.deepEqual(await texts(page, '.pf-bar-row > :first-child'), ['Ranked', 'Casual']);
    assert.equal(await page.locator('[data-key="match-m-05"]').count(), 0, 'the private room is dropped');
    await page.getByRole('button', { name: 'Rushing', exact: true }).click();
    assert.equal(await page.locator('#full-stats tr.total > *').last().textContent(), '770', 'YCO from the older server\'s yac');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // model edge cases (P4-23): a casual live game says no "of N"; an Elo with no history shows no note and no "started at"
    const p = F.profile();
    Object.assign(p.header.live, { queue: 'casual', best_of: 3, game_number: 2 });
    p.elo.history = [];
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-live .q').textContent(), 'Casual', 'a casual live game shows its queue only');
    assert.doesNotMatch(await page.locator('.pf-live').innerText(), /of \d|Series/, 'no "of N" and no series for casual');
    const elo = page.locator('.pf-ovc').first();
    assert.equal(await elo.locator('.pf-ovh span').count(), 0, 'no "0 ranked matches" note');
    assert.doesNotMatch(await elo.innerText(), /started at/, 'no "started at" without history');
    assert.equal(await elo.locator('.pf-spark').count(), 0, 'no chart without history');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // only private rooms played: never "no games", never "Private"; the overview is not drawn from rooms that don't count
    const p = F.profile({ live: false });
    const room = F.allMatches().find((m) => m.queue === 'private');
    Object.assign(p, { recent_matches: [room], matches_total: 1 });
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p, player_profile_all: [room] } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-none').textContent(), NONE);
    assert.equal(await page.locator('[data-key="match-m-05"]').count(), 0);
    assert.doesNotMatch(await page.locator('#screen').innerText(), /never|no online games/i, 'never claims they have not played');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // a player with three matches: rows stay 56px, the card still fills to the right column
    const p = F.profile({ live: false });
    const three = F.publicMatches().slice(0, 3);
    Object.assign(p, { recent_matches: three, matches_total: 3 });
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p, player_profile_all: three } });
    await page.locator('.pf-card').waitFor();
    await rowsNatural(page, 'three matches');
    const hl = await rect(page, '#history .m-list'), last = await rect(page, '.pf-side > section:last-child > .glass');
    assert.ok(Math.abs(hl.bottom - last.bottom) < 1, `the card meets the right column: ${hl.bottom} vs ${last.bottom}`);
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // a brand-new player: no matches at all
    const p = F.profile({ live: false });
    Object.assign(p, { recent_matches: [], matches_total: 0, summary: { games: 0, wins: 0, losses: 0, win_pct: null } });
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p, player_profile_all: [] } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-none').textContent(), NONE);
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the D-pad walks the card, the history tabs and rows, and the pager; B is Back
    const { page, errors, close } = await openState(browser, srv.base, STATE, { pad: true });
    await page.locator('.pf-card').waitFor();
    const key = () => page.evaluate(() => document.activeElement?.getAttribute('data-key') || document.activeElement?.textContent?.trim());
    const press = async (b) => {
      await page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; }, b);
      await page.waitForTimeout(90);
      await page.evaluate((b) => { pads[0].buttons[b] = { pressed: false, value: 0 }; }, b);
      await page.waitForTimeout(90);
    };
    await page.locator('#back').focus();
    const seen = [await key()];
    for (let i = 0; i < 8 && !String(seen.at(-1)).startsWith('match-'); i++) { await press(13); seen.push(await key()); }
    console.log(`pad walk: ${seen.join(' > ')}`);
    // the card reads in order: Back, Plays as, Share profile, View on web (share before the live card), then the tabs
    assert.deepEqual(seen.slice(0, 4), ['back', 'plays-as', 'share', 'web'], `Down reads the card in order: ${seen}`);
    assert.ok(!seen.includes('live-feed') || seen.indexOf('share') < seen.indexOf('live-feed'), `Share comes before Live feed: ${seen}`);
    assert.ok(['All', 'Ranked', 'Casual'].includes(seen[4]), `Down from View on web reaches the history tabs: ${seen}`);
    assert.equal(seen.at(-1), 'match-m-01', `Down then reaches the first match: ${seen}`);
    await press(12); await press(12); await press(12);
    assert.equal(await key(), 'share', 'Up walks back through the card in order');
    await press(15);
    assert.equal(await key(), 'web', 'Right from Share profile is View on web');
    await press(15);
    assert.equal(await key(), 'live-feed', 'Right from View on web is the live card');
    await press(13);
    // the live card sits over the right column: Down lands on Career's "Full stats ›" (Task 7), Left from it on the history tabs
    assert.equal(await key(), 'full-stats', 'Down from Live feed reaches the first stop of the right column');
    await press(14);
    { const k = await key(); assert.ok(['All', 'Ranked', 'Casual'].includes(k), `Left from Full stats › reaches the history tabs: ${k}`); }
    await page.locator('[data-key="live-feed"]').focus();
    await press(14);
    assert.equal(await key(), 'web', 'Left from Live feed comes back to View on web');
    await page.locator('[data-key="match-m-18"]').scrollIntoViewIfNeeded().catch(() => {});
    const lastRow = page.locator('#history .m-list > .m-row:not(.m-hd)').last();
    await lastRow.focus();
    await press(13);
    assert.equal(await key(), 'older', 'the last row goes down to the pager');
    await press(0);
    await page.waitForFunction(() => document.querySelector('#history .m-pager span')?.textContent === 'Page 2 of 2');
    assert.equal(await key(), 'newer', 'focus stays in the pager after Older (Older is now disabled)');
    // past the pager, Down goes on into Full stats' table; the tabs sit over its right-hand columns
    await press(13);
    { const k = await key(); assert.match(String(k), /^sort-passing-/, `Down from the pager reaches the Passing table: ${k}`); }
    await page.locator('[data-key="sort-passing-long"]').focus();
    await press(12);
    { const k = await key(); assert.ok(['Passing', 'Rushing', 'Receiving', 'Defense'].includes(k), `Up from the last heading reaches the Full stats tabs: ${k}`); }
    await press(13);
    { const k = await key(); assert.match(String(k), /^sort-passing-/, `Down from the tabs comes back to the table: ${k}`); }
    for (let i = 0; i < 3 && !String(await key()).startsWith('showall-'); i++) await press(13);
    assert.equal(await key(), 'showall-passing', 'Down through the table reaches "Show all 7 players"');
    await press(0);
    await page.waitForFunction(() => !document.querySelector('#full-stats .pf-showall'));
    assert.equal(await key(), 'sort-passing-name', 'after Show all, focus moves to the table\'s first heading, not the page top');
    await press(1);
    await page.waitForURL(/stats\.html$/);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window
    const { page, errors, close } = await openState(browser, srv.base, STATE, { viewport: { width: 800, height: 560 } });
    await page.locator('.pf-card').waitFor();
    await noSideways(page);
    assert.ok((await rect(page, '.pf-side')).top > (await rect(page, '#history')).bottom - 1, 'one column in a small window');
    await contentRules(page);
    await page.screenshot({ path: path.join(out, 'profile-800.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window, not live, glass on
    const { page, errors, close } = await openState(browser, srv.base, STATE, { viewport: { width: 800, height: 560 }, glass: true, data: { player_profile: F.profile({ live: false }) } });
    await page.locator('.pf-card').waitFor();
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'profile-800-idle-glass.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // a profile that does not exist
    const { page, errors, close } = await openState(browser, srv.base, { ...STATE, url: 'profile.html?player=nope' });
    await page.getByText("That profile couldn't be found.").waitFor();
    assert.equal(await page.locator('#back').textContent(), 'Back to Stats');
    assert.deepEqual(errors, []);
    await close();
  }
  console.log(`check-profile-page: PASS (screens in ${out})`);
} finally { await browser.close(); await srv.close(); }
