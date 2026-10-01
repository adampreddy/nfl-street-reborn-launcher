// The Stats page (spec §8.6, §10, §13): the two columns' cards start and end on one line, 40px section headers, the
// match list (header, 56px rows, the pager inside the card), W/L only, never "Best of 3" or "ladder", the best-of-3
// row opening in place, focus kept across a refresh, You and Your players, the footer line, the small window, signed
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
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, h: r.height }; });
const texts = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => n.textContent.trim()));
const heights = async (page, sel) => [...new Set(await page.locator(sel).evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().height)))];
const noSideways = async (page) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
try {
  { // 1280×800, the redesign's full answers
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.locator('.st-cols').waitFor();
    assert.equal(await page.locator('.page-heading .purpose').textContent(), 'Every match you’ve played, where your Elo stands, and who’s carrying your team.');
    // the two columns: first cards start on one line and end on one line; every section header is 40px
    const list = await rect(page, '.st-list'), you = await rect(page, '.st-you'), players = await rect(page, '.st-plist');
    assert.ok(Math.abs(list.top - you.top) < 1, `first cards start together: ${list.top} vs ${you.top}`);
    assert.ok(Math.abs(list.bottom - players.bottom) < 1, `the columns end together: ${list.bottom} vs ${players.bottom}`);
    assert.deepEqual(await heights(page, '.sec-head'), [40]);
    // the list: header, ten 56px rows, the pager inside the card as its last row
    assert.deepEqual(await texts(page, '.st-list .m-hd span'), ['Result', 'Matchup', 'Opponent', 'Elo', 'Date', '']);
    const rows = page.locator('.st-list > .m-row:not(.m-hd)');
    assert.equal(await rows.count(), 10);
    assert.deepEqual(await heights(page, '.st-list > .m-row:not(.m-hd)'), [56]);
    assert.ok(await page.locator('.st-list > .m-pager').evaluate((n) => n === n.parentElement.lastElementChild), 'the pager is the card’s last row');
    assert.equal(await page.locator('.m-pager span').textContent(), 'Page 1 of 2');
    assert.equal(await page.locator('[data-key="newer"]').isDisabled(), true);
    // a row reads RESULT · MATCHUP · OPPONENT · ELO · DATE
    const row = (id) => page.locator(`[data-key="match-${id}"]`);
    assert.equal(await row('m-01').locator('.m-res').textContent(), 'W2–1');
    assert.equal(await row('m-01').locator('.m-opp-n').textContent(), 'Playz');
    assert.equal(await row('m-01').locator('small').count(), 0, 'a ranked match has no second line');
    assert.equal(await row('m-01').locator('.m-elo').textContent(), '+16');
    assert.match(await row('m-01').locator('.m-elo').getAttribute('class'), /\bup\b/);
    assert.equal(await row('m-01').locator('.m-date').textContent(), 'Sep 26');
    assert.equal(await row('m-01').locator('.m-mu .team-logo').count(), 2);
    assert.equal(await row('m-04').locator('.m-res').textContent(), 'L0–2');
    assert.equal(await row('m-04').locator('.m-elo').textContent(), '−18');
    assert.equal(await row('g:g-031').locator('small').textContent(), 'Offline');
    assert.equal(await row('g:g-031').locator('.m-opp-n').textContent(), 'CPU');
    // a private room is never listed on Stats (rulings P4-3, P4-6, P4-22): the fixture keeps one (m-05) to prove it
    assert.equal(await row('m-05').count(), 0, 'a private room is dropped from the list');
    assert.equal(await row('m-02').locator('.m-elo').textContent(), '');
    // content rules (spec §13): no "Best of 3", no "ladder", no "Private" anywhere, no ties
    const body = await page.locator('#screen').innerText();
    assert.doesNotMatch(body, /best of 3/i);
    assert.doesNotMatch(body, /ladder/i);
    assert.doesNotMatch(body, /private/i);
    assert.doesNotMatch(await page.locator('#screen').evaluate((n) => [...n.querySelectorAll('[aria-label]')].map((x) => x.getAttribute('aria-label')).join(' ')), /private|best of 3|\btie\b/i, 'nor in the labels');
    assert.ok((await texts(page, '.m-res b')).every((t) => t === 'W' || t === 'L' || t === ''), 'results are W or L');
    // a best of 3 opens in place into its games
    assert.equal(await page.locator('#games-m-01').evaluate((n) => n.firstElementChild.hasAttribute('inert')), true, 'closed games cannot take focus');
    assert.equal(Math.round((await rect(page, '#games-m-01')).h), 0);
    await row('m-01').click();
    assert.equal(await row('m-01').getAttribute('aria-expanded'), 'true');
    await page.waitForFunction(() => document.getElementById('games-m-01').getBoundingClientRect().height > 100);
    assert.deepEqual(await texts(page, '#games-m-01 .m-game .m-opp'), ['Game 1 · 5:00 PM', 'Game 2 · 5:25 PM', 'Game 3 · 5:50 PM']);
    assert.deepEqual(await texts(page, '#games-m-01 .m-game .m-res'), ['W28–21', 'L20–31', 'W35–14']);
    assert.deepEqual(await heights(page, '#games-m-01 .m-game'), [40]);
    await page.screenshot({ path: path.join(out, 'stats-1280-bo3.png') });
    // a refresh (the 30 s timer, or a game ending) keeps the row open and the focus on the same game
    await page.locator('[data-key="game-g-012"]').focus();
    const before = await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'stats_dashboard').length);
    await page.evaluate(() => {
      document.dispatchEvent(new CustomEvent('session-state', { detail: true }));
      document.dispatchEvent(new CustomEvent('session-state', { detail: false }));
    });
    await page.waitForFunction((n) => window.__calls.filter((c) => c.cmd === 'stats_dashboard').length > n, before);
    await page.waitForTimeout(150);
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'game-g-012', 'focus kept across a refresh');
    assert.equal(await row('m-01').getAttribute('aria-expanded'), 'true', 'the row stays open');
    assert.ok((await rect(page, '#games-m-01')).h > 100, 'and stays drawn open');
    // the pager
    await page.locator('[data-key="older"]').click();
    assert.equal(await page.locator('.m-pager span').textContent(), 'Page 2 of 2');
    assert.equal(await rows.count(), 3, 'page 2: 13 listed matches (the private room dropped), 3 on the second page');
    assert.equal(await page.locator('[data-key="older"]').isDisabled(), true);
    await page.locator('[data-key="newer"]').click();
    // filters
    await page.getByRole('button', { name: 'Offline', exact: true }).click();
    assert.deepEqual(await rows.evaluateAll((ns) => ns.map((n) => n.dataset.key)), ['match-g:g-031']);
    await page.getByRole('button', { name: 'All', exact: true }).click();
    assert.equal(await rows.count(), 10);
    // You: Elo, its change and the leaderboard rank, then Record · Win % · Last 10 · Streak
    assert.equal(await page.locator('.st-elo-big').textContent(), '1,184');
    assert.equal(await page.locator('.st-chg').textContent(), '▲ 16 · #7 on the leaderboard');
    assert.equal(await page.locator('.st-you svg.st-spark').count(), 1);
    assert.deepEqual(await texts(page, '.st-nums .k-label'), ['Record', 'Win %', 'Last 10', 'Streak']);
    // Record and Win % are the server's career totals (P4-22); Last 10 is the newest ten LISTED matches, the private
    // room dropped (W W W L L W W L W W = 7–3; the server's recent_form, which counts the private room, says 6–4).
    assert.deepEqual(await texts(page, '.st-nums b'), ['14–9', '.609', '7–3', 'W2']);
    assert.match(await page.locator('.st-nums b').nth(3).getAttribute('class'), /\bup\b/);
    // Your players: the portrait on the team colour, the name and role, the statline the server sent
    assert.equal(await page.locator('.st-player').count(), 6);
    const first = page.locator('.st-player').first();
    assert.equal(await first.locator('.player-face').evaluate((n) => getComputedStyle(n).getPropertyValue('--tc').trim()), '#A71930');
    assert.equal(await first.locator('.st-pname').textContent(), 'M. Penix Jr.');
    assert.equal(await first.locator('.k-label').textContent(), 'Best passer');
    assert.equal(await first.locator('.st-line').textContent(), '3,108 YDS');
    assert.match(await first.getAttribute('href'), /#passing$/);
    assert.deepEqual(await texts(page, '.sec-links a'), ['Offense', 'Defense', 'Records']);
    // the footer line: the queue note on the left, Profile & sync on the right, on the columns' right edge
    assert.equal(await page.locator('.st-foot .st-queue').textContent(), '2 games not uploaded yet: The server did not answer.');
    assert.equal(await page.locator('.st-account > summary').textContent(), 'Profile & sync ›');
    const cols = await rect(page, '.st-cols'), sync = await rect(page, '.st-account > summary');
    assert.ok(Math.abs(cols.right - sync.right) < 1.5, 'Profile & sync ends on the columns’ right edge');
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'stats-1280.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // a server that sends every role's statline
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: F.dashboard({ lines: true }) } });
    await page.locator('.st-player').first().waitFor();
    assert.equal(await page.locator('.st-player').first().locator('.st-line').textContent(), '3,108 YDS·31 TD·11 INT·108.9 RTG');
    assert.equal(await page.locator('.st-player').nth(1).locator('.st-line').textContent(), '212 CAR·1,715 YDS·12 TD·5.6 AVG');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the deployed server: no team colours, no leaderboard neighbours; nothing invented
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: F.oldDashboard() } });
    await page.locator('.st-cols').waitFor();
    assert.equal(await page.locator('.player-face').first().evaluate((n) => getComputedStyle(n).getPropertyValue('--tc').trim()), '', 'no colour sent, none invented');
    assert.equal(await page.locator('.st-chg').textContent(), '▲ 16 · #7 on the leaderboard');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window: one column, long names end in an ellipsis, nothing scrolls sideways
    const { page, errors, close } = await openState(browser, srv.base, STATE, { viewport: { width: 800, height: 560 } });
    await page.locator('.st-cols').waitFor();
    await noSideways(page);
    assert.ok((await rect(page, '.st-you')).top > (await rect(page, '.st-list')).bottom - 1, 'one column in a small window');
    await page.evaluate(() => { document.querySelector('[data-key="match-m-01"] .m-opp-n').textContent = 'A player with an unusually long display name that goes on'; });
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'stats-800.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // signed out: the way in and Profile & sync, never an empty two-column frame
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { signed_in: false } } });
    await page.getByText('Sign in with Discord on the Play page to see your games.', { exact: true }).waitFor();
    assert.equal(await page.locator('.st-cols').count(), 0);
    assert.equal(await page.locator('.st-account').count(), 1);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the server refused: its reason, and nothing invented
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { signed_in: true, ok: false, error: 'The server is busy.' } } });
    await page.getByText('The server is busy.', { exact: true }).waitFor();
    assert.equal(await page.locator('.st-cols').count(), 0);
    assert.equal(await page.locator('.st-account').count(), 1);
    assert.deepEqual(errors, []);
    await close();
  }
  { // no games yet
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { ...F.dashboard(), matches: [] } } });
    await page.getByText('No games yet. Play one and it shows up here.', { exact: true }).waitFor();
    assert.equal(await page.locator('.st-cols').count(), 0);
    assert.deepEqual(errors, []);
    await close();
  }
  { // only private rooms played (all dropped): neutral words, never "No games yet" and never "Private"
    const room = F.allMatches().find((m) => m.queue === 'private');
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { ...F.dashboard(), matches: [room] } } });
    await page.getByText('No games to show yet.', { exact: true }).waitFor();
    const body = await page.locator('#screen').innerText();
    assert.doesNotMatch(body, /No games yet/, 'a private-only player is never told they have not played');
    assert.doesNotMatch(body, /private/i);
    assert.equal(await page.locator('.st-cols').count(), 0);
    assert.deepEqual(errors, []);
    await close();
  }
  console.log(`check-stats-page: PASS (screens in ${out})`);
} finally { await browser.close(); await srv.close(); }
