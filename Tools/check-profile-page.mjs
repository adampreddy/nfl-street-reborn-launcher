// The Profile page, the online player hub (spec §8.7, §10, §13), the approved mv2 mockup (owner 10-03): no tabs, one
// short scroll. Back first; the hero (status | Plays as | Joined, Share profile / View on web, no Challenge, the live
// card, the Elo panel or a clean Unranked state); then the v2 grid, every row's cards sharing their edges: Elo history
// with the leaderboard around them · Record · Last 10; Career highlights; Their / Your players (box-score numbers)
// · Recent matches; Head to head · Win rate by team, each left out when its data is. The fuller views (#matches,
// #stats, #records, #h2h) open in place from the "All ..." links, each a history entry (owner 10-03: the mouse's back
// button returns to the profile); Back of any kind returns to the profile first, the hero never changing size. Matches: the queue filter and the pager; Stats: Full stats and the rest; Records:
// Single game and Career side by side; Head to head. W/L only, never "Best of 3", "ladder" or "Private"; the deployed
// server's answers; Elo off; the small window; the D-pad; the links from Stats (#passing...) landing on their table.
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
const AT = (tab) => ({ ...STATE, url: `${STATE.url}#${tab}` });
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, h: r.height }; });
const texts = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => n.textContent.trim()));
const noSideways = async (page) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
const tabOn = (page) => page.locator('.pf-body').getAttribute('data-tab');
/** Open a fuller view by its address (as the "All ..." links do). */
const go = async (page, v) => { await page.evaluate((h) => { location.hash = h; }, `#${v}`); await page.waitForFunction((t) => document.querySelector('.pf-body')?.dataset.tab === t, v); };
const spans = (page) => page.locator('#screen .vgrid').evaluateAll((gs) => gs.map((g) => [...g.children].map((c) => `${c.id}:${c.className.match(/\bs(\d+)\b/)[1]}`).join(' ')));
/** Every grid row: its cards start and end on one line, 16px apart, and fill the row's width. */
const rowsAligned = (page) => page.evaluate(() => [...document.querySelectorAll('#screen .vgrid')].map((g) => {
  const cs = [...g.children].map((c) => c.getBoundingClientRect()), gr = g.getBoundingClientRect(), bad = [];
  for (const c of cs) if (Math.abs(c.top - cs[0].top) > 0.5 || Math.abs(c.bottom - cs[0].bottom) > 0.5) bad.push('edges');
  for (let i = 1; i < cs.length; i++) if (Math.abs(cs[i].left - cs[i - 1].right - 16) > 0.6) bad.push(`gap ${cs[i].left - cs[i - 1].right}`);
  if (Math.abs(cs[0].left - gr.left) > 0.5 || Math.abs(cs.at(-1).right - gr.right) > 0.5) bad.push('width');
  return bad.length ? `${[...g.children].map((c) => c.id).join('+')}: ${bad.join(', ')}` : '';
}).filter(Boolean));
const contentRules = async (page) => {
  const body = await page.locator('#screen').innerText();
  assert.doesNotMatch(body, /best of 3/i, 'never "Best of 3"');
  assert.doesNotMatch(body, /ladder/i, '"Leaderboard", never "ladder"');
  assert.doesNotMatch(body, /private/i, 'no "Private" on Profile');
  const results = await texts(page, '.m-res b');
  if (await page.locator('#history .m-list > .m-row:not(.m-hd)').count()) assert.ok(results.length > 0, 'match rows show their result letters');
  assert.ok(results.every((t) => t === 'W' || t === 'L' || t === ''), 'results are W or L');
  // no ties: no record anywhere reads W–L–T
  assert.doesNotMatch(body, /\b\d+[–-]\d+[–-]\d+\b/, 'records are W-L only, never a tie');
};
// Best QB → receiver reads (owner 10-03, "you can't even read the text"): each word's contrast against the card behind it,
// sampled from the rendered pixels so the glass layers count. `pairs` for the names and numbers, `heads` for the headings.
async function combosReadable(page, pairs, heads) {
  await page.evaluate(() => document.getElementById('combos').scrollIntoView({ block: 'center' }));
  await settle(page);
  const at = await page.locator('#combos').evaluate((n) => { const r = n.getBoundingClientRect(); return { x: r.right - 12, y: r.top + 6 }; });
  const px = await page.screenshot({ clip: { x: at.x, y: at.y, width: 1, height: 1 } });
  const bg = await page.evaluate(async (b64) => { const bm = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
    const c = new OffscreenCanvas(1, 1).getContext('2d'); c.drawImage(bm, 0, 0); return [...c.getImageData(0, 0, 1, 1).data.slice(0, 3)]; }, px.toString('base64'));
  const colours = await page.locator('#combos').evaluate((root) => [...root.querySelectorAll('h3, thead th, tbody th .who > span:last-child, tbody td')]
    .map((n) => ({ text: n.textContent.trim(), head: n.matches('thead th'), rgb: getComputedStyle(n).color.match(/\d+/g).slice(0, 3).map(Number) })));
  const lum = (rgb) => { const c = rgb.map((v) => v / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)); return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; };
  const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
  for (const c of colours) {
    const need = c.head ? heads : pairs;
    assert.ok(ratio(c.rgb, bg) >= need, `combos "${c.text}" reads at ${ratio(c.rgb, bg).toFixed(2)}:1 on rgb(${bg}), needs ${need}:1`);
  }
}
const NONE = 'No online matches to show yet.';
// History rows keep their natural height (56px): the card never stretches its rows (P4-24).
const rowHeights = (page) => page.locator('#history .m-list > .m-row:not(.m-hd)').evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().height));
const rowsNatural = async (page, what) => { const hs = await rowHeights(page); assert.ok(hs.length && hs.every((h) => h <= 56.5), `${what}: every history row is at most 56px: ${hs}`); };
try {
  { // 1280×800, the redesign's full answers, the owner looking at their own profile: the hero and the profile (mv2-3/4)
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: (() => { const p = F.profile(); p.elo.week_delta = 39; return p; })() } });
    await page.locator('.pf-card').waitFor();
    await page.locator('#recent').waitFor();
    await page.screenshot({ path: path.join(out, 'profile-1280.png') });
    assert.equal(await page.locator('#screen > :first-child').getAttribute('id'), 'back', 'Back comes first');
    assert.equal(await page.locator('.pf-name').textContent(), 'adamknowsball');
    assert.equal(await page.locator('.pf-status').textContent(), '● In a match');
    assert.equal(await page.locator('.pf-pic .presence-dot').count(), 1);
    // status | Plays as | Joined; Share profile and View on web, never a Challenge button
    assert.deepEqual(await page.locator('.pf-meta > :not(.pf-dot)').evaluateAll((ns) => ns.map((n) => n.className)), ['pf-status', 'pf-team', '']);
    assert.match(await page.locator('.pf-meta').textContent(), /Joined Sep 2026$/);
    assert.deepEqual(await texts(page, '.pf-acts .btn'), ['Share profile', 'View on web']);
    assert.equal(await page.getByRole('button', { name: /challenge/i }).count(), 0, 'no Challenge button');
    assert.equal(await page.locator('#plays-as').count(), 1, 'the owner picks Plays as');
    const pic = await rect(page, '.pf-pic'), id = await rect(page, '.pf-id');
    assert.equal(Math.round(pic.h), 96);
    assert.ok(id.left > pic.right, 'the name beside the avatar');
    // the Elo panel on the right: rating, the week's change, Rank of total, Peak, the ranked record, the sparkline
    assert.equal(await page.locator('.pf-elo .k-label').textContent(), 'Elo · Ranked');
    assert.equal(await page.locator('.pf-elo-v').textContent(), '1,184+39 this week');
    assert.deepEqual(await texts(page, '.pf-elo-facts > *'), ['Rank', '#7 of 214', 'Peak', '1,212', 'Record', '8-5']);
    assert.equal(await page.locator('.pf-elo svg polyline').count(), 1);
    { const e = await rect(page, '.pf-elo'), c = await rect(page, '.pf-card'); assert.ok(Math.abs(e.right - c.right) < 2 && Math.abs(e.top - c.top) < 2, 'the Elo panel fills the hero\'s right'); }
    // the live card, under Share / View on web
    assert.equal(await page.locator('.pf-live .q').textContent(), 'Ranked · Game 2 of 3');
    assert.equal(await page.locator('.pf-live strong').textContent(), '14 – 7');
    assert.equal(await page.locator('.pf-live-bot > span').textContent(), 'vs Mikel · Series 1–0');
    assert.ok((await rect(page, '.pf-live')).top > (await rect(page, '.pf-acts')).bottom, 'the live card sits under the buttons');
    // no tabs: one short scroll of the v2 grid
    assert.equal(await page.locator('.pf-tabbar, #full-stats, #history').count(), 0, 'no tabs; the fuller views are not drawn');
    assert.equal(await tabOn(page), 'overview');
    assert.deepEqual(await spans(page), ['elo-history:6 record:3 last10:3', 'career:12', 'players:7 recent:5', 'h2h-card:7 pf-teams:5']);
    assert.deepEqual(await rowsAligned(page), []);
    assert.deepEqual([...new Set(await page.locator('.vc-h').evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().height)))], [48]);
    // Elo history: the curve and the leaderboard around them, their own row marked ("You" on their own profile)
    assert.equal(await page.locator('#elo-history .vc-aside').textContent(), '13 ranked games');
    assert.equal(await page.locator('#elo-history .pf-lad .k-label').textContent(), 'Around #7');
    assert.deepEqual(await texts(page, '.pf-lad .vc-tr b'), ['Kay', 'Lo', 'You', 'Playz', 'Jock']);
    assert.equal(await page.locator('.pf-lad .vc-tr.me b').textContent(), 'You');
    assert.equal(await page.locator('#elo-history .ch-peak').textContent(), 'Peak 1,212');
    // Record (online only, Ranked and Casual apart) and Last 10
    assert.equal(await page.locator('#record .pf-big').textContent(), '14-9');
    assert.equal(await page.locator('#record .vc-sub').textContent(), '60.9% online');
    assert.deepEqual(await page.locator('#record .pf-q').evaluateAll((ns) => ns.map((n) => [...n.children].map((c) => c.textContent).join(' '))), ['Ranked 8-5 61.5%', 'Casual 6-4 60.0%']);
    assert.equal(await page.locator('#last10 .pf-big').textContent(), '6-4');
    assert.equal(await page.locator('#last10 .wl-strip span').count(), 10);
    assert.deepEqual(await texts(page, '#last10 .vc-kv'), ['Scored / allowed31.4 / 25.0', 'Avg margin+6.4']);
    // Career highlights: eight spelled-out cells, rank and per game
    assert.deepEqual(await texts(page, '#career .k-label'), ['Passing Yards', 'Passing TD', 'Passer Rating', 'Rushing Yards', 'Receiving Yards', 'Total TD', 'Tackles', 'Interceptions']);
    assert.equal(await page.locator('#career .vc-cells b').first().textContent(), '3,021');
    assert.equal(await page.locator('#career .vc-cells .n').first().textContent(), '#3 · 125.9 / game');
    assert.deepEqual(await page.locator('#career .k-label').evaluateAll((ns) => ns.filter((n) => n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent)), [], 'no label cut off');
    // Your players (their own profile): five rows, box-score numbers from each player's career row
    assert.equal(await page.locator('#players-h').textContent(), 'Your players');
    assert.deepEqual(await page.locator('#players .pl-row').evaluateAll((ns) => ns.map((n) => [...n.querySelectorAll('.pl-cells > span')].map((s) => `${s.firstChild.textContent} ${s.querySelector('small').textContent}`))), [
      ['141/238 C/ATT', '2,210 YDS', '31 TD', '11 INT', '110.5 RTG'],
      ['212 CAR', '1,180 YDS', '5.6 AVG', '12 TD', '61 LONG'],
      ['98 REC', '1,402 YDS', '14.3 AVG', '14 TD', '74 LONG'],
      ['71 TKL', '5 SCK', '9 TFL', '2 FF', '6 PBU'],
      ['7 INT', '14 PBU', '33 TKL', '0 FF', '1 TD']]);
    // Recent matches: Result · Opponent · Queue · Date, seven rows
    assert.deepEqual(await texts(page, '#recent .vc-th span'), ['Result', 'Opponent', 'Queue', 'Date']);
    assert.equal(await page.locator('#recent .vc-tr').count(), 7);
    assert.deepEqual(await page.locator('[data-key="recent-m-01"] > *').evaluateAll((ns) => ns.map((n) => n.textContent.trim())), ['W 2–1', 'Playz', 'Ranked', 'Sep 26']);
    // Head to head and Win rate by team (in place of Top connections, Adam 10-03)
    assert.deepEqual(await page.locator('#h2h-card .vc-tr').first().locator(':scope > *').evaluateAll((ns) => ns.map((n) => n.textContent.trim())), ['Playz', '5-3', '212–180', 'W Sep 26', '+21']);
    assert.equal(await page.locator('#pf-teams h2').textContent(), 'Win rate by team');
    assert.ok(await page.locator('#pf-teams .vc-tr').count() > 0, 'a team row');
    assert.equal(await page.locator('#connections').count(), 0, 'Top connections is gone');
    await contentRules(page);
    await noSideways(page);
    await page.evaluate(() => { document.getElementById('main').scrollTop = 1e6; });
    await page.waitForTimeout(200);
    await page.screenshot({ path: path.join(out, 'profile-1280-lower.png') });
    await page.evaluate(() => { document.getElementById('main').scrollTop = 0; });
    // "All matches ›" opens the match history in place as a history entry; the mouse's back button (page.goBack) and the
    // page's Back both return to the profile, focus on the link that opened the view
    const len = await page.evaluate(() => history.length);
    await page.locator('[data-key="all-matches"]').click();
    assert.equal(await tabOn(page), 'matches');
    assert.equal(await page.evaluate(() => location.hash), '#matches');
    assert.equal(await page.evaluate(() => history.length), len + 1, 'a history entry');
    assert.equal(await page.locator('#players').count(), 0, 'the fuller view replaces the profile\'s grid');
    await page.goBack();
    await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'overview');
    assert.equal(await page.evaluate(() => location.hash), '');
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'all-matches', 'focus back on the link that opened it');
    await page.goForward();
    await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'matches');
    await page.locator('#back').click();
    await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'overview');
    assert.equal(await page.evaluate(() => location.hash), '');
    for (const [k, v] of [['all-stats', 'stats'], ['all-players', 'stats'], ['all-h2h', 'h2h']]) {
      if (!(await page.locator(`[data-key="${k}"]`).count())) continue;
      await page.locator(`[data-key="${k}"]`).click();
      assert.equal(await tabOn(page), v);
      await page.goBack();
      await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'overview');
    }
    // a view opened from a view (More stats' Records ›) steps back one view at a time
    await page.locator('[data-key="all-stats"]').click();
    if (await page.locator('[data-key="to-records"]').count()) {
      await page.locator('[data-key="to-records"]').click();
      assert.equal(await tabOn(page), 'records');
      await page.goBack();
      await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'stats');
    }
    await page.goBack();
    await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'overview');
    assert.deepEqual(errors, []);
    // a recent match opens the whole match, Back returning to the profile
    await page.locator('[data-key="recent-m-01"]').click();
    await page.waitForURL(/game\.html\?/);
    const u = new URL(page.url());
    assert.deepEqual([u.searchParams.get('game'), u.searchParams.get('games'), u.searchParams.get('from')], ['g-013', 'g-011,g-012,g-013', `profile.html?player=${F.ME}`]);
    await close();
  }
  { // Live feed opens the native live view
    const { page, errors, close } = await openState(browser, srv.base, STATE);
    await page.getByRole('button', { name: 'Live feed', exact: true }).click();
    await page.waitForURL(/live\.html\?match=dc5226af-d3b8-4c0b-89e8-1c53b2972a30$/);
    assert.deepEqual(errors, []);
    await close();
  }
  { // Matches: the queue as the second line, the pager in the card, more pages loaded as needed, a row opening the match
    const { page, errors, close } = await openState(browser, srv.base, AT('matches'));
    await page.locator('#history .m-list').waitFor();
    assert.equal(await tabOn(page), 'matches');
    const rows = page.locator('#history .m-list > .m-row:not(.m-hd)');
    assert.equal(await rows.count(), 10);
    await rowsNatural(page, 'ten rows');
    assert.equal(await page.locator('[data-key="match-m-01"] small').textContent(), 'Ranked');
    assert.equal(await page.locator('[data-key="match-m-02"] small').textContent(), 'Casual');
    assert.equal(await page.locator('#history [aria-expanded]').count(), 0, 'nothing opens in place');
    // letter avatars (no Discord picture) are coloured per player, the same player always the same colour (mockup)
    const letters = await page.locator('span.avatar').evaluateAll((ns) => ns.map((n) => ({ who: n.textContent, bg: getComputedStyle(n).backgroundImage })));
    const byWho = new Map();
    for (const l of letters) { if (byWho.has(l.who)) assert.equal(byWho.get(l.who), l.bg, `one colour per player (${l.who})`); byWho.set(l.who, l.bg); }
    assert.equal(await page.locator('#history .m-pager span').textContent(), 'Page 1');
    await page.screenshot({ path: path.join(out, 'profile-1280-matches.png') });
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
    await page.screenshot({ path: path.join(out, 'profile-1280-ranked.png') });
    await contentRules(page);
    await noSideways(page);
    await page.locator('[data-key="match-m-04"]').click();
    await page.waitForURL(/game\.html\?/);
    const u = new URL(page.url());
    assert.deepEqual([u.searchParams.get('game'), u.searchParams.get('games'), u.searchParams.get('series'), u.searchParams.get('from')],
      ['g-042', 'g-041,g-042', '0-2', `profile.html?player=${F.ME}#matches`], 'the whole match; Back returns to Matches');
    assert.deepEqual(errors, []);
    await close();
  }
  { // Records: Single game and Career side by side, Team records, "vs <opponent>" in each record's label
    const { page, errors, close } = await openState(browser, srv.base, AT('records'));
    await page.locator('#records #rec-single').waitFor();
    assert.equal(await tabOn(page), 'records');
    assert.deepEqual(await texts(page, '#records .vgrid > .vc h2'), ['Single game', 'Career', 'Team records']);
    assert.deepEqual(await rowsAligned(page), []);
    assert.deepEqual((await texts(page, '#rec-single .rec-k')).slice(0, 3), ['Passing Yards', 'Passing TD', 'Rushing Yards']);
    assert.doesNotMatch(await page.locator('#records').textContent(), /\byds\b/i, 'spelled-out labels say Yards');
    assert.match(await page.locator('#rec-single a.rec-row').first().getAttribute('aria-label'), /vs Playz/);
    assert.match(await page.locator('#rec-single a.rec-row').first().getAttribute('href'), new RegExp(`^game\\.html\\?game=g-011&from=profile\\.html%3Fplayer%3D${F.ME}%23records$`));
    assert.deepEqual((await texts(page, '#rec-career .rec-k')).slice(0, 3), ['Passing Yards', 'Passing TD', 'Passer Rating']);
    await page.screenshot({ path: path.join(out, 'profile-1280-records.png') });
    await contentRules(page);
    await noSideways(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // Stats: Full stats, one table at a time, top five, "Show all N players", a Total row; every table on one column grid
    const { page, errors, close } = await openState(browser, srv.base, AT('stats'));
    await page.locator('#full-stats .tbl').waitFor();
    assert.equal(await tabOn(page), 'stats');
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
    // the rest of the numbers: Your players, per game, by team, QB → receiver and the others (P4-5, P4-25)
    assert.deepEqual(await texts(page, '#more h3'), ['Your players', 'Per game', 'By team', 'Best QB → receiver', 'Laterals & pitches', 'In wins vs losses', 'By game type']);
    assert.deepEqual(await texts(page, '#more-players .pl-name'), ['M. Penix Jr.', 'S. Barkley', 'D. London', 'N. Bolton', 'A.J. Terrell', 'B. Robinson']);
    // each player's numbers from its own career row (owner 10-03), box-score labels (Adam 10-03); Most played its
    // own role's four after GAMES (a rusher here: never another role's numbers)
    assert.deepEqual(await page.locator('#more-players .pl-row').evaluateAll((ns) => ns.map((n) => [...n.querySelectorAll('.pl-cells > span')].map((s) => `${s.firstChild.textContent} ${s.querySelector('small').textContent}`))), [
      ['141/238 C/ATT', '2,210 YDS', '31 TD', '11 INT', '110.5 RTG'],
      ['212 CAR', '1,180 YDS', '5.6 AVG', '12 TD', '61 LONG'],
      ['98 REC', '1,402 YDS', '14.3 AVG', '14 TD', '74 LONG'],
      ['71 TKL', '5 SCK', '9 TFL', '2 FF', '6 PBU'],
      ['7 INT', '14 PBU', '33 TKL', '0 FF', '1 TD'],
      ['23 GAMES', '96 CAR', '535 YDS', '5.6 AVG', '5 TD']]);
    assert.doesNotMatch(await page.locator('#more-players').textContent(), /NaN|null|undefined/);
    await page.locator('#more-players').screenshot({ path: path.join(out, 'profile-1280-players-lines.png') });
    assert.deepEqual(await texts(page, '#by-type tbody th'), ['Normal']);
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
    await combosReadable(page, 4.5, 4.5);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the links from Stats' Your players: #passing / #rushing / #receiving / #defense open Stats on that table,
    // #career the Overview's Career
    for (const [hash, tab] of [['passing', 'Passing'], ['rushing', 'Rushing'], ['receiving', 'Receiving'], ['defense', 'Defense']]) {
      const { page, errors, close } = await openState(browser, srv.base, AT(hash));
      await page.locator('#full-stats .tbl').waitFor();
      assert.equal(await tabOn(page), 'stats');
      assert.equal(await page.locator('#full-stats .tab[aria-pressed="true"]').textContent(), tab);
      await page.waitForFunction(() => { const t = document.querySelector('.pf-body').getBoundingClientRect().top; return t >= -2 && t < 120; });
      assert.deepEqual(errors, []);
      await close();
    }
    const { page, errors, close } = await openState(browser, srv.base, AT('career'));
    await page.locator('#career').waitFor();
    assert.equal(await tabOn(page), 'overview');
    assert.deepEqual(errors, []);
    await close();
  }
  { // Head to head: its own tab; long lists show five rows and "Show all N" (as the old profile capped them); "Last"
    // shows a one-game match's score, a series' date (P4-26)
    const p = F.profile();
    const base = p.head_to_head[0];
    p.head_to_head = Array.from({ length: 7 }, (_, i) => ({ ...structuredClone(base), player_id: `${'0123456'[i]}b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e`, display_name: `Rival ${i + 1}`, avatar_url: null }));
    Object.assign(p.head_to_head[0].last, { game_id: 'g:0b2c3d4e', score: [35, 14] });
    p.by_team = Array.from({ length: 6 }, (_, i) => ({ ...structuredClone(p.by_team[0]), key: `tgid:${i}`, name: `Team ${i + 1}` }));
    p.combos = Array.from({ length: 8 }, (_, i) => ({ ...structuredClone(p.combos[0]), rec: 60 - i }));
    const { page, errors, close } = await openState(browser, srv.base, AT('h2h'), { data: { player_profile: p } });
    await page.locator('#h2h').waitFor();
    assert.equal(await tabOn(page), 'h2h');
    assert.equal(await page.locator('#h2h a.who').first().getAttribute('href'), '0b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e'.replace(/^/, 'profile.html?player='), 'an opponent opens their profile');
    const check = async (id, n, what) => {
      const rows = page.locator(`#${id} tbody tr:not(.more)`);
      assert.equal(await rows.filter({ visible: true }).count(), 5, `${id}: five rows first`);
      const b = page.locator(`#${id} .pf-showall`);
      assert.equal(await b.textContent(), `Show all ${n} ${what}`);
      await b.click();
      assert.equal(await rows.filter({ visible: true }).count(), n, `${id}: every row after Show all`);
      assert.equal(await b.textContent(), 'Show top 5');
      assert.equal(await b.evaluate((x) => document.activeElement === x), true, `${id}: focus stays on the button`);
    };
    // letter avatars (no Discord picture) are coloured per player, never one colour for all (mockup)
    const bgs = await page.locator('#h2h span.avatar').evaluateAll((ns) => ns.map((n) => getComputedStyle(n).backgroundImage));
    assert.ok(new Set(bgs).size >= 2, `letter avatars vary per player: ${bgs}`);
    await check('h2h', 7, 'opponents');
    assert.equal(await page.locator('#h2h tbody tr').nth(0).locator('td').nth(3).textContent(), 'W 35–14', 'a one-game match shows its score');
    assert.equal(await page.locator('#h2h tbody tr').nth(1).locator('td').nth(3).textContent(), 'W Sep 26', 'a series shows its date');
    await page.screenshot({ path: path.join(out, 'profile-1280-h2h.png') });
    await go(page, 'stats');
    await page.locator('#more').waitFor();
    await check('by-team', 6, 'teams');
    await check('combos', 8, 'combos');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // 800×560, a pad: Right across Full stats' headings keeps each one wholly in view, clear of the sticky Player
    // column; Head to head fits without scrolling (P4-26)
    const { page, errors, close } = await openState(browser, srv.base, AT('stats'), { viewport: { width: 800, height: 560 }, pad: true });
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
    await page.screenshot({ path: path.join(out, 'profile-800-full-stats-pad.png') });
    await go(page, 'h2h');
    await page.locator('#h2h').waitFor();
    const h2h = await page.locator('#h2h').evaluate((n) => [n.scrollWidth, n.clientWidth]);
    assert.ok(h2h[0] <= h2h[1] + 1, `Head to head does not scroll at 800: ${h2h}`);
    assert.deepEqual(await texts(page, '#h2h thead th'), ['Opponent', 'Record', 'PTS', 'Allowed', 'Last', 'Best win', 'Worst loss']);
    assert.deepEqual(await page.locator('#h2h thead th').evaluateAll((ns) => ns.filter((n) => n.offsetParent).map((n) => n.textContent)), ['Opponent', 'Record', 'PTS', 'Last', 'Best win']);
    assert.deepEqual(errors, []);
    await close();
  }
  { // a private room is never drawn in Stats (P4-3): not in By game type, not as a team
    const p = F.profile();
    p.by_game_type.push({ key: 'private', name: 'Private room', games: 3, wins: 2, losses: 1, win_pct: 0.667, pts_per_game: 30 });
    const { page, errors, close } = await openState(browser, srv.base, AT('stats'), { data: { player_profile: p } });
    await page.locator('#more').waitFor();
    assert.deepEqual(await texts(page, '#by-type tbody th'), ['Normal']);
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window: Full stats and the rest in one column, no sideways page scroll, no number cut off (a wide
    // table scrolls inside its own card)
    const { page, errors, close } = await openState(browser, srv.base, AT('stats'), { viewport: { width: 800, height: 560 } });
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
    await go(page, 'stats');
    await page.locator('#combos').waitFor();
    // over the glass the pairs and numbers still read at 4.5:1; the small uppercase headings at least 2.5:1 (they were 1.45)
    await combosReadable(page, 4.5, 2.5);
    await page.locator('#combos').screenshot({ path: path.join(out, 'profile-1280-combos-glass.png') });
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
  { // Back returns to the profile from a fuller view, then to the page the profile was opened from: the page's Back,
    // then the mouse's (page.goBack)
    const { page, errors, close } = await openState(browser, srv.base, { name: 'stats', url: 'stats.html', scope: 'main' });
    // Your passer opens the profile at its top (Adam 10-03); All stats › opens Full stats; Back steps out of it
    await page.locator('[data-key="card-passer"]').click();
    await page.locator('#players').waitFor();
    assert.equal(await tabOn(page), 'overview');
    await page.locator('[data-key="all-stats"]').click();
    await page.locator('#full-stats').waitFor();
    assert.equal(await tabOn(page), 'stats');
    await page.locator('#back').click();
    await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'overview');
    await page.locator('#back').click();
    await page.waitForURL(/stats\.html$/);
    await page.locator('#screen').first().waitFor();
    // the same with the mouse's back button: Match history, back to the profile, back to Stats
    await page.locator('[data-key="card-passer"]').click();
    await page.locator('#players').waitFor();
    await page.locator('[data-key="all-matches"]').click();
    await page.locator('#history').waitFor();
    await page.goBack();
    await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'overview');
    await page.goBack();
    await page.waitForURL(/stats\.html$/);
    await page.locator('#st-record, #screen').first().waitFor();
    // a deep link (Stats' All players ›, #stats) still lands on its view; Back there returns to the profile in place
    await page.locator('[data-key="all-players"]').click();
    await page.locator('#full-stats').waitFor();
    assert.equal(await tabOn(page), 'stats');
    await page.locator('#back').click();
    await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'overview');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the hero never changes size between the profile and its views (owner 10-03, "the bar at the top changes size when
    // you click match history"). Real scrollbars (the test browser hides them) and a tall window, where one view
    // scrolls and another does not.
    const real = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true, ignoreDefaultArgs: ['--hide-scrollbars'] });
    try {
      for (const viewport of [{ width: 1280, height: 800 }, { width: 1600, height: 1200 }]) {
        const { page, errors, close } = await openState(real, srv.base, STATE, { viewport });
        await page.locator('#recent').waitFor();
        const box = () => page.evaluate(() => ['#back', '.pf-card', '.pf-elo', '.pf-row'].map((s) => { const r = document.querySelector(s).getBoundingClientRect();
          return `${s} ${[r.left, r.top, r.width, r.height].map((x) => Math.round(x * 10) / 10).join(',')}`; }));
        const at = await box();
        const clip = { x: 180, y: 0, width: viewport.width - 180, height: 380 };
        await page.screenshot({ path: path.join(out, `profile-head-overview-${viewport.height}.png`), clip });
        for (const [k, v] of [['all-matches', 'matches'], ['all-stats', 'stats'], ['all-players', 'stats']]) {
          await page.locator(`[data-key="${k}"]`).click();
          await page.waitForFunction((t) => document.querySelector('.pf-body')?.dataset.tab === t, v);
          await page.evaluate(() => { document.getElementById('main').scrollTop = 0; });
          await page.waitForTimeout(300);
          if (v === 'matches') await page.screenshot({ path: path.join(out, `profile-head-matches-${viewport.height}.png`), clip });
          assert.deepEqual(await box(), at, `the hero keeps its size in ${v} (${viewport.width}x${viewport.height})`);
          await page.goBack();
          await page.waitForFunction(() => document.querySelector('.pf-body')?.dataset.tab === 'overview');
          await page.evaluate(() => { document.getElementById('main').scrollTop = 0; });
          await page.waitForTimeout(200);
          assert.deepEqual(await box(), at, `and on the profile again (${viewport.width}x${viewport.height})`);
        }
        assert.deepEqual(errors, []);
        await close();
      }
    } finally { await real.close(); }
  }
  { // someone else's profile: only the owner picks Plays as; their players read "Their players" (and "Players" in
    // More stats); the leaderboard names the owner, never "You"
    const who = { identity_status: { signed_in: true, display_name: 'Playz', player_id: '1b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e', show_offline: false } };
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: who });
    await page.locator('#players').waitFor();
    assert.equal(await page.locator('#plays-as').count(), 0);
    assert.equal(await page.locator('#players-h').textContent(), 'Their players');
    assert.equal(await page.locator('.pf-lad .vc-tr.me b').textContent(), 'adamknowsball');
    await go(page, 'stats');
    assert.equal(await page.locator('#more-players h3').textContent(), 'Players');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the deployed server: everything it does not send is left out, never faked; its private room is dropped
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: F.oldProfile(), player_profile_all: F.allMatches().filter((m) => m.mode === 'online') } });
    await page.locator('#record').waitFor();
    assert.equal(await page.locator('.pf-live').count(), 0);
    assert.equal(await page.locator('.pf-status').textContent(), '● Online now');
    // no recent_form: no Last 10; no elo.around: no leaderboard beside the curve; Elo history and Record take the row
    assert.deepEqual(await spans(page), ['elo-history:8 record:4', 'career:12', 'players:7 recent:5', 'h2h-card:7 pf-teams:5']);
    assert.equal(await page.locator('.pf-lad').count(), 0);
    assert.deepEqual(await rowsAligned(page), []);
    assert.deepEqual(await texts(page, '.pf-elo-facts > *'), ['Rank', '#7', 'Peak', '1,212', 'Record', '8-5'], 'no board size: "#7", never a made-up total');
    assert.equal(await page.locator('.pf-elo-v small').count(), 0, 'no week_delta: no week change');
    assert.equal(await page.locator('#record .pf-big').textContent(), '14-9', 'the private room never counts in the record');
    assert.deepEqual(await page.locator('#record .pf-q .qm').evaluateAll((ns) => ns.map((n) => n.textContent)), ['Ranked', 'Casual']);
    assert.ok((await texts(page, '#career .n')).every((t) => !t.includes('#')), 'no ranks without career_rank');
    assert.equal(await page.locator('[data-key="match-m-05"], [data-key="recent-m-05"]').count(), 0, 'the private room is dropped');
    await go(page, 'stats');
    await page.getByRole('button', { name: 'Rushing', exact: true }).click();
    assert.equal(await page.locator('#full-stats tr.total > *').last().textContent(), '770', 'YCO from the older server\'s yac');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // Elo off until launch: the hero says Unranked / Rankings start soon, no numbers; no Elo history card
    const p = F.profile({ live: false });
    p.elo = F.ELO_OFF; delete p.career_rank;
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p } });
    await page.locator('#record').waitFor();
    assert.equal(await page.locator('.pf-elo-v').textContent(), 'Unranked');
    assert.equal(await page.locator('.pf-elo .vc-sub').textContent(), 'Rankings start soon.');
    assert.doesNotMatch(await page.locator('.pf-elo').innerText(), /\d/);
    assert.deepEqual((await spans(page))[0], 'record:6 last10:6');
    assert.deepEqual(await rowsAligned(page), []);
    await page.screenshot({ path: path.join(out, 'profile-1280-elo-off.png') });
    assert.deepEqual(errors, []);
    await close();
  }
  { // model edge cases (P4-23): a casual live game says no "of N"; an Elo with no history draws no curve, its peak the server's
    const p = F.profile();
    Object.assign(p.header.live, { queue: 'casual', best_of: 3, game_number: 2 });
    p.elo.history = [];
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-live .q').textContent(), 'Casual', 'a casual live game shows its queue only');
    assert.doesNotMatch(await page.locator('.pf-live').innerText(), /of \d|Series/, 'no "of N" and no series for casual');
    assert.equal(await page.locator('.pf-elo svg').count(), 0, 'no history: no sparkline');
    assert.deepEqual((await texts(page, '.pf-elo-facts > *')).slice(2, 4), ['Peak', '1,212'], 'the peak the server sent');
    assert.equal(await page.locator('#elo-history .ch').count(), 0, 'no curve without history; the leaderboard alone');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // only private rooms played: never "no games", never "Private"; no tabs drawn from rooms that don't count
    const p = F.profile({ live: false });
    const room = F.allMatches().find((m) => m.queue === 'private');
    Object.assign(p, { recent_matches: [room], matches_total: 1 });
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p, player_profile_all: [room] } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-none').textContent(), NONE);
    assert.equal(await page.locator('.pf-body, .vgrid').count(), 0);
    assert.doesNotMatch(await page.locator('#screen').innerText(), /never|no online games/i, 'never claims they have not played');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // a first page that is mostly, or entirely, private rooms (the deployed server still sends them) tops up to a full
    // page at once; the empty words show only when no public match exists at all
    const room = F.allMatches().find((m) => m.queue === 'private');
    const rooms = (n) => Array.from({ length: n }, (_, i) => ({ ...structuredClone(room), id: `p-0${i}`, match_id: `p-0${i}`, at: `2026-09-27T${String(19 - i).padStart(2, '0')}:00:00.000Z` }));
    const pub = F.publicMatches();
    const rowKeys = (page) => page.locator('#history .m-list > .m-row:not(.m-hd)').evaluateAll((ns) => ns.map((n) => n.dataset.key));
    for (const [what, first, total, all, want] of [
      ['mostly private', [...rooms(8), ...pub.slice(0, 2)], 20, pub, pub.slice(0, 10).map((m) => `match-${m.id}`)],
      ['entirely private, more pages', rooms(10), 22, pub, pub.slice(0, 10).map((m) => `match-${m.id}`)],
      ['entirely private, nothing public', rooms(10), 15, [], []],
    ]) {
      const p = F.profile({ live: false });
      Object.assign(p, { recent_matches: first, matches_total: total });
      const { page, errors, close } = await openState(browser, srv.base, AT('matches'), { data: { player_profile: p, player_profile_all: all } });
      await page.locator('.pf-card').waitFor();
      if (await page.locator('.pf-none').count()) {
        assert.deepEqual(want, [], `${what}: the honest empty state only when nothing public exists`);
        await close();
        continue;
      }
      await page.waitForFunction(() => !/Loading/.test(document.querySelector('#history .m-list')?.textContent || ''));
      assert.deepEqual(await rowKeys(page), want, `${what}: the first page fills to ${want.length} public rows`);
      if (want.length) {
        assert.equal(await page.locator('#history .m-empty').count(), 0, `${what}: no empty words`);
        assert.equal(await page.locator('#history .m-pager span').textContent(), 'Page 1');
        assert.equal(await page.locator('#history [data-key="older"]').isDisabled(), false, `${what}: Older still offered`);
      } else {
        assert.equal(await page.locator('#history .m-empty').textContent(), NONE, `${what}: the honest empty state`);
        assert.equal(await page.locator('#history .m-pager').count(), 0, `${what}: no Older under the empty words`);
      }
      await contentRules(page);
      assert.deepEqual(errors, []);
      await close();
    }
  }
  { // the 30 s refresh redraws the header only: live -> not live drops the live card and reads "● Online now"; focus,
    // scroll, the tab and the history page stay; it uses the first load's arguments; it skips while the window is hidden
    const init = () => {
      const real = window.setInterval;
      window.__ticks = [];
      window.setInterval = (fn, ms, ...a) => { if (ms === 30000) { window.__ticks.push(fn); return 0; } return real.call(window, fn, ms, ...a); };
    };
    const { page, errors, close } = await openState(browser, srv.base, AT('matches'), { init });
    await page.locator('.pf-live').waitFor();
    assert.equal(await page.evaluate(() => window.__ticks.length), 1, 'one 30 s timer');
    await page.locator('#history [data-key="older"]').click();
    await page.waitForFunction(() => document.querySelector('#history .m-pager span')?.textContent === 'Page 2 of 2');
    await page.locator('[data-key="share"]').focus();
    await page.evaluate(() => { document.getElementById('main').scrollTop = 40; });
    const scroll = await page.evaluate(() => document.getElementById('main').scrollTop);
    const tick = () => page.evaluate(() => Promise.all(window.__ticks.map((f) => f())));
    const profileCalls = () => page.evaluate(() => window.__calls.filter((c) => c.cmd === 'player_profile' && !c.args.before).map((c) => JSON.stringify(c.args)));
    const before = await profileCalls();
    // the stub now answers "not in a match"
    await page.evaluate(() => {
      const inv = window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke = (cmd, args = {}) => (cmd === 'player_profile' && !args.before
        ? inv(cmd, args).then((p) => ({ ...p, header: { ...p.header, live: null } })) : inv(cmd, args));
    });
    await page.evaluate(() => Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' }));
    await tick();
    assert.deepEqual(await profileCalls(), before, 'no refresh while the window is hidden');
    assert.equal(await page.locator('.pf-live').count(), 1);
    await page.evaluate(() => { delete document.visibilityState; });
    await tick();
    const after = await profileCalls();
    assert.equal(after.length, before.length + 1, 'one quiet refresh');
    assert.equal(after.at(-1), before[0], `the refresh sends the first load's arguments: ${after.at(-1)}`);
    assert.equal(await page.locator('.pf-live').count(), 0, 'the live card is gone');
    assert.equal(await page.locator('.pf-status').textContent(), '● Online now');
    assert.equal(await page.locator('.pf-pic .presence-dot').count(), 1, 'still online: the presence dot stays');
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'share', 'focus does not move');
    assert.equal(await page.evaluate(() => document.getElementById('main').scrollTop), scroll, 'scroll does not move');
    assert.equal(await tabOn(page), 'matches', 'the tab stays');
    assert.equal(await page.locator('#history .m-pager span').textContent(), 'Page 2 of 2', 'the history page stays');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // a refresh that ends the live card while Live feed has focus hands focus to View on web, never the page top
    const init = () => { window.__ticks = []; const real = window.setInterval; window.setInterval = (fn, ms, ...a) => (ms === 30000 ? (window.__ticks.push(fn), 0) : real.call(window, fn, ms, ...a)); };
    const { page, errors, close } = await openState(browser, srv.base, STATE, { init });
    await page.locator('[data-key="live-feed"]').focus();
    await page.evaluate(() => {
      const inv = window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke = (cmd, args = {}) => (cmd === 'player_profile' && !args.before
        ? inv(cmd, args).then((p) => ({ ...p, header: { ...p.header, live: null } })) : inv(cmd, args));
    });
    await page.evaluate(() => Promise.all(window.__ticks.map((f) => f())));
    assert.equal(await page.locator('.pf-live').count(), 0);
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'web');
    assert.deepEqual(errors, []);
    await close();
  }
  { // a player with three matches: Recent matches shows three, the row's cards still end together
    const p = F.profile({ live: false });
    const three = F.publicMatches().slice(0, 3);
    Object.assign(p, { recent_matches: three, matches_total: 3 });
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p, player_profile_all: three } });
    await page.locator('#recent').waitFor();
    assert.equal(await page.locator('#recent .vc-tr').count(), 3);
    assert.deepEqual(await rowsAligned(page), []);
    await go(page, 'matches');
    await rowsNatural(page, 'three matches');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // a brand-new player: no matches at all
    const p = F.profile({ live: false });
    Object.assign(p, { recent_matches: [], matches_total: 0, summary: { games: 0, wins: 0, losses: 0, win_pct: null }, by_queue: [] });
    const { page, errors, close } = await openState(browser, srv.base, STATE, { data: { player_profile: p, player_profile_all: [] } });
    await page.locator('.pf-card').waitFor();
    assert.equal(await page.locator('.pf-none').textContent(), NONE);
    assert.equal(await page.locator('#record').count(), 0, 'no "0-0 record"');
    assert.match(await page.locator('.pf-elo-v').textContent(), /^1,184/, 'the hero still shows their Elo');
    await contentRules(page);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the D-pad walks the hero in reading order, then the profile's cards; the fuller views; B is Back
    const { page, errors, close } = await openState(browser, srv.base, STATE, { pad: true });
    await page.locator('#recent').waitFor();
    const key = () => page.evaluate(() => document.activeElement?.getAttribute('data-key') || document.activeElement?.textContent?.trim());
    const press = async (b) => {
      await page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; }, b);
      await page.waitForTimeout(90);
      await page.evaluate((b) => { pads[0].buttons[b] = { pressed: false, value: 0 }; }, b);
      await page.waitForTimeout(90);
    };
    await page.locator('#back').focus();
    const seen = [await key()];
    for (let i = 0; i < 8 && ['back', 'share', 'web', 'plays-as', 'live-feed'].includes(String(seen.at(-1))); i++) { await press(13); seen.push(await key()); }
    console.log(`pad walk: ${seen.join(' > ')}`);
    // the hero top-down (mv2-3): Plays as on the status line, then Share profile and View on web, then the live card
    assert.deepEqual(seen.slice(0, 5), ['back', 'plays-as', 'share', 'web', 'live-feed'], `Down reads the hero in order: ${seen}`);
    assert.ok(!['back', 'share', 'web', 'plays-as', 'live-feed'].includes(String(seen.at(-1))), `Down from the hero reaches the profile's cards: ${seen}`);
    await page.locator('[data-key="live-feed"]').focus();
    await press(12); await press(12);
    assert.equal(await key(), 'share', 'Up walks back through the hero in order');
    await press(15);
    assert.equal(await key(), 'web', 'Right from Share profile is View on web');
    // every link and row of the profile is reachable: All matches, then down the history into the pager
    await page.locator('[data-key="all-matches"]').focus();
    await press(0);
    assert.equal(await tabOn(page), 'matches');
    await page.locator('[data-key="match-m-01"]').focus();
    const lastRow = page.locator('#history .m-list > .m-row:not(.m-hd)').last();
    await lastRow.focus();
    await press(13);
    assert.equal(await key(), 'older', 'the last row goes down to the pager');
    await press(0);
    await page.waitForFunction(() => document.querySelector('#history .m-pager span')?.textContent === 'Page 2 of 2');
    assert.equal(await key(), 'newer', 'focus stays in the pager after Older (Older is now disabled)');
    // Stats: the table's headings, its tabs above, Show all below
    await go(page, 'stats');
    await page.locator('[data-key="sort-passing-long"]').focus();
    await press(12);
    { const k = await key(); assert.ok(['Passing', 'Rushing', 'Receiving', 'Defense'].includes(k), `Up from the last heading reaches the Full stats tabs: ${k}`); }
    await press(13);
    { const k = await key(); assert.match(String(k), /^sort-passing-/, `Down from the tabs comes back to the table: ${k}`); }
    // from a middle heading (the rightmost one's column holds "Records ›" further down)
    await page.locator('[data-key="sort-passing-int"]').focus();
    for (let i = 0; i < 3 && !String(await key()).startsWith('showall-'); i++) await press(13);
    assert.equal(await key(), 'showall-passing', 'Down through the table reaches "Show all 7 players"');
    await press(0);
    await page.waitForFunction(() => !document.querySelector('#full-stats .pf-showall'));
    assert.equal(await key(), 'sort-passing-name', 'after Show all, focus moves to the table\'s first heading, not the page top');
    await page.locator('#back').click();
    assert.equal(await tabOn(page), 'overview');
    await page.locator('#back').focus();
    await press(1);
    await page.waitForURL(/stats\.html$/);
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window: the grid stacks in one column
    const { page, errors, close } = await openState(browser, srv.base, STATE, { viewport: { width: 800, height: 560 } });
    await page.locator('#recent').waitFor();
    await noSideways(page);
    assert.ok((await rect(page, '#recent')).top > (await rect(page, '#players')).bottom - 1, 'one column in a small window');
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
