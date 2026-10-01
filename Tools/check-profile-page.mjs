// The Profile page, the online player hub (spec §8.7, §10, §13): Back first; the card (status, summary line, Share /
// View on web, the live card); the overview (three columns, big numbers on one line, visuals ending on one line); Match
// history beside Career / Leaderboard / Records (first cards start and end on one line; 40px headers); the queue filter
// and the pager; W/L only, never "Best of 3", "ladder" or "Private"; the deployed server's answers; the small window;
// the model edge cases (P4-23); letter avatars coloured per player; the D-pad through the card, history and tabs.
//     node Tools/check-profile-page.mjs [outdir]      (screens in target/p4-after by default)
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
const STATE = { name: 'profile', url: `profile.html?player=${F.ME}`, scope: 'main' };
const rect = (page, sel) => page.locator(sel).first().evaluate((n) => { const r = n.getBoundingClientRect(); return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, h: r.height }; });
const texts = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => n.textContent.trim()));
const noSideways = async (page) => assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
const contentRules = async (page) => {
  const body = await page.locator('#screen').innerText();
  assert.doesNotMatch(body, /best of 3/i, 'never "Best of 3"');
  assert.doesNotMatch(body, /ladder/i, '"Leaderboard", never "ladder"');
  assert.doesNotMatch(body, /private/i, 'no "Private" on Profile');
  assert.ok((await texts(page, '.m-res b')).every((t) => t === 'W' || t === 'L' || t === ''), 'results are W or L');
  // no ties: no record anywhere reads W–L–T
  assert.doesNotMatch(body, /\b\d+–\d+–\d+\b/, 'records are W–L only, never a tie');
};
const NONE = 'No online matches to show yet.';
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
  // (Task 7 adds the Full stats checks here)
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
    // (Task 7 adds the older server's rushing table here)
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
    assert.ok(seen.includes('share') || seen.includes('plays-as'), `Down from Back reaches the card's controls: ${seen}`);
    assert.ok(seen.some((k) => ['All', 'Ranked', 'Casual'].includes(k)), `Down reaches the history tabs: ${seen}`);
    assert.equal(seen.at(-1), 'match-m-01', `Down then reaches the first match: ${seen}`);
    await page.locator('[data-key="share"]').focus();
    await press(15);
    assert.equal(await key(), 'web', 'Right from Share profile is View on web');
    await press(15);
    assert.equal(await key(), 'live-feed', 'Right from View on web is the live card');
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
