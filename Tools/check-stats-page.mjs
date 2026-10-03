// The Stats page (spec §8.6, §10, §13): Matches | Records at the top right (owner 10-03). Matches: the two columns'
// cards start and end on one line, 40px section headers, the match list (header, 56px rows, the pager inside the card),
// W/L only, never "Best of 3" or "ladder", a row opening the WHOLE match on one page (never in place), focus kept across
// a refresh, You (the record online only, Ranked and Casual apart) and Your players, the footer line. Records: Single
// game | Career, one table, "Yards" spelled out, "vs <opponent>". The small window, signed out, a refusal, and the
// deployed server's answers.
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
    // nothing opens in place any more: no reveal, no aria-expanded, the column never changes size
    assert.equal(await page.locator('.st-list .rows-reveal, .st-list [aria-expanded]').count(), 0, 'no row opens in place');
    // Matches | Records at the top right, beside the title, Matches picked
    assert.deepEqual(await texts(page, '#stats-view .tab'), ['Matches', 'Records']);
    assert.equal(await page.locator('#stats-view .tab[aria-pressed="true"]').textContent(), 'Matches');
    { const v = await rect(page, '#stats-view .tabs'), h = await rect(page, '.page-heading h1'), c = await rect(page, '.st-cols');
      assert.ok(Math.abs(v.right - c.right) < 1.5, `the switch ends on the content's right edge: ${v.right} vs ${c.right}`);
      assert.ok(v.top < h.bottom, 'the switch sits beside the title'); }
    // a refresh (the 30 s timer, or a game ending) keeps the focus on the same row
    await row('m-02').focus();
    const before = await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'stats_dashboard').length);
    await page.evaluate(() => {
      document.dispatchEvent(new CustomEvent('session-state', { detail: true }));
      document.dispatchEvent(new CustomEvent('session-state', { detail: false }));
    });
    await page.waitForFunction((n) => window.__calls.filter((c) => c.cmd === 'stats_dashboard').length > n, before);
    await page.waitForTimeout(150);
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'match-m-02', 'focus kept across a refresh');
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
    // The record is ONLINE matches only (owner 10-03): the offline CPU game (W) and the private room are left out, so
    // 8-4, not the server's 14-9 (which counts offline games). Last 10 is the newest ten online matches (6-4).
    assert.deepEqual(await texts(page, '.st-nums b'), ['8-4', '.667', '6-4', 'W2']);
    assert.match(await page.locator('.st-nums b').nth(3).getAttribute('class'), /\bup\b/);
    // Ranked and Casual: two separate, labelled figures, never one blended number
    assert.deepEqual(await texts(page, '.st-q .k-label'), ['Ranked', 'Casual']);
    assert.deepEqual(await texts(page, '.st-q b'), ['1-1', '7-3']);
    assert.deepEqual(await texts(page, '.st-q-pct'), ['.500', '.700']);
    { const [a, b] = await page.locator('.st-q').evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().toJSON()));
      assert.ok(Math.abs(a.top - b.top) < 1 && b.left > a.right, 'Ranked and Casual side by side, apart'); }
    // Your players: the portrait on the team colour, the name and role, the statline the server sent
    assert.equal(await page.locator('.st-player').count(), 6);
    const first = page.locator('.st-player').first();
    assert.equal(await first.locator('.player-face').evaluate((n) => getComputedStyle(n).getPropertyValue('--tc').trim()), '#A71930');
    assert.equal(await first.locator('.st-pname').textContent(), 'M. Penix Jr.');
    assert.equal(await first.locator('.k-label').textContent(), 'Best passer');
    assert.equal(await first.locator('.st-line').textContent(), '3,108 YDS');
    assert.match(await first.getAttribute('href'), /#passing$/);
    // no Offense / Defense / Records strip beside Your players (owner ruling 10-03)
    assert.equal(await page.locator('.sec-links').count(), 0);
    assert.equal(await page.locator('#st-players-h').locator('xpath=..').locator('a, nav').count(), 0);
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
  { // a row opens the WHOLE match on one page (owner 10-03): every game of it, the last one shown, the series, Back
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.locator('[data-key="match-m-01"]').click();
    await page.waitForURL(/game\.html\?/);
    const u = new URL(page.url());
    assert.deepEqual([u.searchParams.get('game'), u.searchParams.get('games'), u.searchParams.get('series'), u.searchParams.get('from')],
      ['g-013', 'g-011,g-012,g-013', '2-1', 'stats.html']);
    await page.goto(`${srv.base}/stats.html`);
    await page.locator('[data-key="match-m-02"]').click();
    await page.waitForURL(/game\.html\?/);
    const one = new URL(page.url());
    assert.deepEqual([one.searchParams.get('game'), one.searchParams.get('games')], ['g-021', null], 'a one-game match opens just that game');
    assert.deepEqual(errors, []);
    await close();
  }
  { // Records: Single game | Career, one table; the switch follows the address (#records); Back from a record's game
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.locator('.st-cols').waitFor();
    await page.locator('[data-key="view-records"]').click();
    await page.locator('.rec-table').waitFor();
    assert.equal(await page.evaluate(() => location.hash), '#records');
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'view-records', 'focus stays on the switch');
    assert.equal(await page.locator('.st-cols').count(), 0, 'Records replaces the matches, never expands them');
    assert.equal(await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'player_profile').map((c) => c.args.playerId).join()), F.ME, 'the player\'s own profile, read once');
    assert.deepEqual(await texts(page, '.rec-pill'), ['Single game', 'Career']);
    assert.equal(await page.locator('.rec-pill[aria-pressed="true"]').textContent(), 'Single game');
    assert.deepEqual(await texts(page, '.rec-hd span'), ['Record', 'Player', 'Best', 'Game', 'Date']);
    assert.deepEqual(await texts(page, '.rec-table > .rec-row:not(.rec-hd) .rec-k'), ['Passing Yards', 'Rushing Yards', 'Receiving Yards', 'Tackles', 'Longest TD', 'Most points', 'Biggest win']);
    assert.doesNotMatch(await page.locator('.rec-table').textContent(), /\byds\b/i, '"Yards" spelled out');
    const first = page.locator('.rec-table > .rec-row:not(.rec-hd)').first();
    assert.deepEqual(await first.locator(':scope > *').evaluateAll((ns) => ns.map((n) => n.textContent.trim())), ['Passing Yards', 'M. Penix Jr.', '288', 'vs Playz', 'Sep 19']);
    assert.equal(await first.locator('.team-logo').count(), 1, 'the player\'s team logo');
    assert.deepEqual(await texts(page, '.rec-g'), ['vs Playz', 'vs Playz', 'vs Playz', 'vs Mikel', 'vs Jock', 'vs CPU', 'vs CPU']);
    assert.match(await first.getAttribute('href'), /^game\.html\?game=g-011&from=stats\.html%23records$/, 'a record opens its game, Back returns to Records');
    { const t = await rect(page, '.rec-table'), h = await rect(page, '.page-heading');
      assert.ok(t.top > h.bottom, 'the table sits under the heading'); }
    await noSideways(page);
    await page.screenshot({ path: path.join(out, 'stats-1280-records.png') });
    await page.locator('[data-key="rec-kind-career"]').click();
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'rec-kind-career', 'focus stays on the pill');
    assert.deepEqual(await texts(page, '.rec-hd span'), ['Record', 'Player', 'Total', 'Games', '']);
    assert.deepEqual(await page.locator('.rec-table > .rec-row:not(.rec-hd)').evaluateAll((ns) => ns.map((n) => [...n.children].slice(0, 4).map((c) => c.textContent.trim()).join(' | '))), [
      'Passing Yards | M. Penix Jr. | 2,210 | 20 games', 'Passing TD | M. Penix Jr. | 31 | 20 games', 'Rushing Yards | S. Barkley | 1,180 | 20 games',
      'Receiving Yards | D. London | 1,402 | 20 games', 'Total TD | D. London | 14 | 20 games', 'Tackles | N. Bolton | 71 | 20 games',
      'Sacks | N. Bolton | 5 | 20 games', 'Interceptions | A.J. Terrell | 7 | 20 games']);
    await page.screenshot({ path: path.join(out, 'stats-1280-records-career.png') });
    // a refresh keeps the view and the focus
    await page.evaluate(() => { document.dispatchEvent(new CustomEvent('session-state', { detail: true })); document.dispatchEvent(new CustomEvent('session-state', { detail: false })); });
    await page.waitForTimeout(400);
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'rec-kind-career');
    await page.locator('[data-key="view-matches"]').click();
    await page.locator('.st-cols').waitFor();
    assert.equal(await page.evaluate(() => location.hash), '');
    assert.deepEqual(errors, []);
    await close();
  }
  { // opened at #records (Back from a record's game) it shows Records
    const { page, errors, close } = await openState(browser, srv.base, { ...STATE, url: 'stats.html#records' });
    await page.locator('.rec-table').waitFor();
    assert.equal(await page.locator('#stats-view .tab[aria-pressed="true"]').textContent(), 'Records');
    assert.deepEqual(errors, []);
    await close();
  }
  { // a server that sends every role's statline
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: F.dashboard({ lines: true }) } });
    await page.locator('.st-player').first().waitFor();
    const lines = await page.locator('.st-player .st-line').evaluateAll((ns) => ns.map((n) => [...n.children].map((s) => s.textContent)));
    assert.deepEqual(lines, [
      ['198/331 C/ATT', '3,108 YDS', '31 TD', '11 INT', '108.9 RTG'],
      ['212 CAR', '1,715 YDS', '5.6 AVG', '12 TD', '61 LNG'],
      ['98 REC', '1,714 YDS', '14.3 AVG', '14 TD', '74 LNG'],
      ['71 TKL', '5 SCK', '9 TFL', '2 FF', '6 PBU'],
      ['7 INT', '14 PBU', '33 TKL', '1 TD'],
      ['23 GAMES', '96 CAR', '535 YDS', '5 TD']]);
    assert.doesNotMatch(await page.locator('.st-plist').textContent(), /NaN|null|undefined|—/);
    // a long line wraps between numbers, never inside one, and never runs out of its card
    for (const s of await page.locator('.st-line > span').evaluateAll((ns) => ns.map((n) => { const r = n.getBoundingClientRect(), c = n.closest('.st-plist').getBoundingClientRect(); return { h: r.height, inside: r.right <= c.right + 0.5 }; })))
      assert.ok(s.h < 20 && s.inside, `statline numbers stay on one line inside the card: ${JSON.stringify(s)}`);
    await page.locator('.st-plist').screenshot({ path: path.join(out, 'stats-1280-players-lines.png') });
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
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { stats_dashboard: { signed_in: false }, identity_status: { signed_in: false },
      identity_sign_in_start: { device_code: 'dev-1', user_code: 'ABCD-1234', verify_url: 'https://discord.com/activate', interval: 60 } } });
    // Big and front and centre: a 28px bold heading and the 52px red Sign in button, right on Stats.
    await page.getByRole('heading', { name: 'Sign in to see your stats', exact: true }).waitFor();
    const h = await page.locator('.st-signin h2').evaluate(n => { const c = getComputedStyle(n); return { size: parseFloat(c.fontSize), weight: +c.fontWeight }; });
    assert.ok(h.size >= 28 && h.weight >= 700, `sign-in heading is big and bold (${h.size}px/${h.weight})`);
    const go = page.locator('.st-signin-go');
    assert.equal(await go.textContent(), 'Sign in with Discord');
    assert.ok((await go.boundingBox()).height >= 52, 'sign-in button is the big one');
    assert.equal(await page.locator('.st-cols').count(), 0);
    assert.equal(await page.locator('.st-account').count(), 0, 'no Profile & sync before there is an account');
    await page.getByText('Connect your Discord account to play online and track your stats!', { exact: true }).waitFor();
    await page.getByText('2 games will upload when you sign in.', { exact: true }).waitFor();
    { // centred on the content area
      const card = await page.locator('.st-signin').boundingBox(), main = await page.locator('#main').boundingBox();
      assert.ok(Math.abs((card.x + card.width / 2) - (main.x + main.width / 2)) <= 2, 'sign-in card is centred');
    }
    await page.screenshot({ path: path.join(out, 'stats-signed-out.png') });
    assert.doesNotMatch(await page.locator('.st-signin').innerText(), /null|undefined/, 'no stray empty-slot text in the sign-in card');
    await go.click();
    await page.getByText('Your code is ABCD-1234', { exact: true }).waitFor();
    await page.getByText('Go to https://discord.com/activate to approve it.', { exact: true }).waitFor();
    assert.doesNotMatch(await page.locator('.st-signin').innerText(), /null|undefined/);
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.getByRole('heading', { name: 'Sign in to see your stats', exact: true }).waitFor();
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
