// Owner 10-08, four asks, checked in a real browser on the harness (Tools/lib/page-states.mjs):
//   1. "the watch button should be live feed": Live now's cards say "Live feed", never "Watch";
//   2. a live game opens in the site's style: a Live now card (its Live feed, or a click on the card) opens match.html,
//      the site's /live/<match> page, and the page follows the match while it runs: new plays show up without a reload;
//      Back returns to Online;
//   3. every player's name opens their profile in the launcher: the leaderboard card, a Live now card, room members,
//      chat, the matchup card, the post-game screen and the match page's strip; Back on the profile returns;
//   4. the full leaderboard is a launcher page: 50 a page, your row marked, "Jump to me", rows opening profiles, at
//      1280x800 and 800x560, and by controller.
// Screens in target/live-feed (or the folder given).
//     node Tools/check-live-feed.mjs [outdir]
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { serve, openState, root, MATCH_5C8, LEADERBOARD } from './lib/page-states.mjs';
import * as F from './fixtures/stats-fixtures.mjs';

const out = path.resolve(process.argv[2] || path.join(root, 'target/live-feed'));
mkdirSync(out, { recursive: true });
const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const shots = [];
const shot = async (page, name) => { const file = path.join(out, `${name}.png`); await page.screenshot({ path: file }); shots.push(file); };
let passed = 0;
const ok = (what) => { passed++; console.log(`ok   ${what}`); };

// ---- a live match: the real 5c8d58ca best of 3 with its newest plays not played yet ----
const MID = MATCH_5C8.match.id;
const revs = MATCH_5C8.games.flatMap((g) => g.plays.map((p) => p.rev)).sort((a, b) => a - b);
const CUT = revs[revs.length - 6];   // the five newest plays come later
function liveSnap(upTo) {
  const s = structuredClone(MATCH_5C8);
  s.match.status = 'running';
  s.match.outcome = { kind: 'open', winner_seat: null, official: false, left_seat: null };
  delete s.match.ended_at;
  for (const g of s.games) g.plays = g.plays.filter((p) => p.rev <= upTo);
  return s;
}
const LIVE_BEFORE = liveSnap(CUT);
const LIVE_AFTER = liveSnap(Infinity);
// Live now's list, naming each seat's account (as GET /v1/live does).
const LIST_ITEM = { id: MID, mode: 'ranked', best_of: 3, series: [1, 1], started_at: '2026-10-03T04:00:00.000Z', game_seq: 3, score: [7, 14],
  seats: MATCH_5C8.seats, sides: MATCH_5C8.games[2].sides, teams: MATCH_5C8.games[2].teams };

const mm = (snapshot) => ({ link: 'connected', error: null, presence_on: true, relay_error: null, snapshot });
const empty = { player: { name: 'adamknowsball', player_id: F.ME, region: 'na-east' }, ticket: null, room: null, match: null, chat: [], queue_depth: { ranked: 0, casual: 0 },
  online: { count: 2, players: [{ name: 'Jock', player_id: '3d4e5f60-7182-4d9e-8f10-2b3c4d5e6f70', activity: 'idle' }, { name: 'Tuna', activity: 'idle' }] } };
const JOCK = '3d4e5f60-7182-4d9e-8f10-2b3c4d5e6f70';
const PICK = "try{localStorage.setItem('mm:mode','ranked')}catch{}";

/** Wait for the address to end in `re`, then say so. */
async function landed(page, re, what) {
  await page.waitForURL(re, { timeout: 5000 });
  ok(what);
}

// ================================================================== 1 + 2: Live feed opens the site-styled page, live
{
  const { page, errors, close } = await openState(browser, srv.base, { name: 'live-feed-online', url: 'online.html', scope: 'main',
    data: { mm_state: mm(empty), live_matches: [LIST_ITEM], live_match: LIVE_BEFORE, leaderboard: LEADERBOARD } }, { init: PICK });
  const card = page.locator('.live-row').first();
  assert.equal(await card.locator('.live-watch').textContent(), 'Live feed');
  assert.equal(await page.locator('#live-slot').getByText('Watch', { exact: true }).count(), 0);
  ok('a Live now card says "Live feed", never "Watch"');
  const names = await card.locator('a.player-link').evaluateAll((ns) => ns.map((n) => [n.textContent, n.getAttribute('href')]));
  assert.deepEqual(names.map(([, h]) => h).sort(), MATCH_5C8.seats.map((s) => `profile.html?player=${s.player_id}`).sort());
  ok(`both players on the card are links to their profiles (${names.map(([n]) => n).join(', ')})`);
  await shot(page, 'online-live-now');
  // A click on the card itself (its score), not on a link, opens the match.
  await card.locator('.live-score-num').first().click();
  await landed(page, new RegExp(`/match\\.html\\?match=${MID}&from=online\\.html$`), 'a click on the card opens match.html?match=<id>&from=online.html');
  await page.locator('#strip .strip').waitFor();
  assert.equal(await page.locator('#site-game .tabs [role="tab"]').count(), 3);
  assert.match(await page.locator('#strip .st-status').innerText(), /live/i);
  ok('the match page is the site\'s page (strip, Live Feed / Box Score / Team Stats), saying LIVE');
  const latestBefore = await page.locator('#latest').textContent();
  await shot(page, 'match-live-before');
  // The match goes on: five more plays. The page reads them on its own within a few seconds (no reload).
  await page.evaluate((snap) => {
    const core = window.__TAURI__.core, inv = core.invoke;
    window.__liveReads = 0;
    core.invoke = async (cmd, args) => (cmd === 'live_match' ? (window.__liveReads++, structuredClone(snap)) : inv(cmd, args));
  }, LIVE_AFTER);
  await page.waitForFunction((t) => document.getElementById('latest')?.textContent !== t, latestBefore, { timeout: 12000 });
  const latestAfter = await page.locator('#latest').textContent();
  const reads = await page.evaluate(() => window.__liveReads);
  assert.ok(reads >= 1, `live_match read again while the match runs (${reads} reads)`);
  assert.doesNotMatch(await page.locator('#strip .st-status').innerText(), /final/i, 'the match is not over');
  ok(`new plays appear while the match runs, without a reload ("${latestBefore.trim().slice(0, 50)}" -> "${latestAfter.trim().slice(0, 50)}")`);
  await shot(page, 'match-live-after');
  // The strip's names open profiles.
  const strip = await page.locator('#strip a.player-link').evaluateAll((ns) => ns.map((n) => n.getAttribute('href')));
  assert.equal(strip.length, 2);
  assert.ok(strip.every((h) => /^profile\.html\?player=[0-9a-f-]{36}$/.test(h)));
  ok('both players in the match page\'s strip are profile links');
  // Back (B / Esc click #back) returns to Online.
  await page.keyboard.press('Escape');
  await landed(page, /\/online\.html$/, 'Esc on the match page returns to Online');
  assert.deepEqual(errors, []);
  await close();
}

// The match page's strip: a name opens the profile, and the profile's Back returns to the match.
{
  const { page, errors, close } = await openState(browser, srv.base, { name: 'live-feed-strip', url: 'online.html', scope: 'main',
    data: { mm_state: mm(empty), live_matches: [LIST_ITEM], live_match: MATCH_5C8 } }, { init: PICK });
  await page.locator('.live-row .live-watch').first().click();
  await page.locator('#strip a.player-link').first().waitFor();
  const who = await page.locator('#strip a.player-link').first().textContent();
  await page.locator('#strip a.player-link').first().click();
  await landed(page, /\/profile\.html\?player=[0-9a-f-]{36}$/, `the strip's "${who}" opens their profile`);
  await page.locator('#back').waitFor();
  await page.locator('#back').click();
  await landed(page, /\/match\.html\?match=/, 'Back on that profile returns to the match page');
  assert.deepEqual(errors, []);
  await close();
}

// ================================================================== 3: names everywhere
async function nameOpens(stateName, data, sel, what, { url = 'online.html', click = null, back = /\/online\.html$/ } = {}) {
  const { page, errors, close } = await openState(browser, srv.base, { name: stateName, url, scope: 'main', data, click }, { init: PICK });
  const link = page.locator(sel).first();
  await link.waitFor({ timeout: 5000 });
  const href = await link.getAttribute('href');
  assert.match(href, /^profile\.html\?player=[0-9a-f-]{36}$/, `${what}: ${href}`);
  await shot(page, stateName);
  await link.focus();
  assert.equal(await page.evaluate(() => document.activeElement?.classList.contains('player-link')), true, `${what}: focusable`);
  await page.keyboard.press('Enter');
  await landed(page, /\/profile\.html\?player=/, `${what}: Enter opens the profile`);
  await page.locator('#back').waitFor();
  await page.keyboard.press('Escape');
  await landed(page, back, `${what}: Esc on the profile comes back`);
  assert.deepEqual(errors, []);
  await close();
}
await nameOpens('names-leaderboard', { mm_state: mm(empty), leaderboard: LEADERBOARD }, '#leaderboard-slot a.player-link', 'a leaderboard card name');
const say = (seq, name, text, player_id) => ({ seq, name, text, at: `2026-09-30T19:5${seq}:00Z`, player_id });
const ROOM = { ...empty, room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open', you_host: true,
  members: [{ name: 'adamknowsball', player_id: F.ME, ready: true, field: null, you: true }, { name: 'Jock', player_id: JOCK, ready: false, field: null, you: false }] },
  chat: [say(1, 'Jock', 'ready when you are', JOCK), say(2, 'adamknowsball', 'on my way', F.ME)] };
await nameOpens('names-room-member', { mm_state: mm(ROOM) }, `.on-members a.player-link[href$="${JOCK}"]`, 'a room member');
await nameOpens('names-chat', { mm_state: mm(ROOM) }, `.mm-chat a.player-link[href$="${JOCK}"]`, 'a chat author');
await nameOpens('names-chat-me', { mm_state: mm(ROOM) }, `.mm-chat a.player-link[href$="${F.ME}"]`, 'your own chat line (your own profile)');
const MATCH = { ...empty, match: { id: 'm5', mode: 'ranked', status: 'pairing', relay_state: 'pairing', relay_deadline: null, best_of: 3, you_host: true, opponent: 'Jock', opponent_id: JOCK,
  field: null, your_field_pick: null, your_accepted: false, your_ready: false, series: [0, 0], wins_needed: 2, winner_seat: null, hold_reason: null, seat: 0, opponent_elo: 1320, opponent_rank: 1 } };
await nameOpens('names-matchup', { mm_state: mm(MATCH) }, `.on-vs a.player-link[href$="${JOCK}"]`, 'the matchup card\'s opponent');
await nameOpens('names-matchup-me', { mm_state: mm(MATCH) }, `.on-vs a.player-link[href$="${F.ME}"]`, 'your own face on the matchup card');
// The post-game screen: you (adamknowsball) in seat 0, the 49ers' seat, so you won it.
{
  const youSeat0 = structuredClone(MATCH_5C8);
  youSeat0.seats = [MATCH_5C8.seats[1], MATCH_5C8.seats[0]];
  const over = { ...empty, match: { ...MATCH.match, id: MID, status: 'complete', relay_state: 'complete', your_accepted: true, your_ready: true, opponent: 'Playz',
    opponent_id: MATCH_5C8.seats[0].player_id, series: [2, 1], series_live: [2, 1], outcome: { kind: 'win', winner_seat: 0, official: true, left_seat: null } } };
  await nameOpens('names-post-game', { mm_state: mm(over), live_match: youSeat0 }, `.on-vs a.player-link[href$="${MATCH_5C8.seats[0].player_id}"]`, 'the post-game screen\'s opponent');
}
// The players-online menu.
{
  const { page, errors, close } = await openState(browser, srv.base, { name: 'names-online-menu', url: 'online.html', scope: 'main', data: { mm_state: mm(empty) } }, { init: PICK });
  await page.locator('#on-players').click();
  const link = page.locator(`.on-who a.player-link[href$="${JOCK}"]`);
  await link.waitFor();
  assert.equal(await page.locator('.on-who li').nth(1).locator('a').count(), 0, 'a player with no id stays plain');
  await shot(page, 'names-online-menu');
  await link.click();
  await landed(page, /\/profile\.html\?player=/, 'a name in the players-online menu opens their profile');
  assert.deepEqual(errors, []);
  await close();
}
// Stats: an opponent in Your matches; the row itself still opens the match.
await nameOpens('names-stats-match', {}, '#st-matches a.player-link', 'an opponent in Stats\' Your matches', { url: 'stats.html', back: /\/stats\.html$/ });
{
  const { page, errors, close } = await openState(browser, srv.base, { name: 'names-stats-row', url: 'stats.html', scope: 'main' });
  const row = page.locator('#st-matches .m-row.m-open-row').first();
  assert.equal(await row.locator('button a').count(), 0, 'no link inside a button');
  await row.locator('.m-elo').click({ force: true }); // a mouse click on the Elo cell: the row's button under it takes it
  await landed(page, /\/match\.html\?/, 'a click elsewhere on that row still opens the match');
  assert.deepEqual(errors, []);
  await close();
}

// ================================================================== 4: the full leaderboard
// The board as the server pages it: 130 ranked, you (adamknowsball) at #87.
const BOARD_INIT = `(() => {
  const ME = ${JSON.stringify(F.ME)};
  const names = ['Jock','Playz','Tuna','Mikel','Riley','Sauce','Bones','Vick7','Kaz','Dime','Rook','Blaze'];
  const row = (rank) => ({ rank, player_id: rank === 87 ? ME : '00000000-0000-4000-8000-' + String(rank).padStart(12, '0'),
    display_name: rank === 87 ? 'adamknowsball' : names[rank % names.length] + (rank > 12 ? rank : ''), nickname: rank === 2 ? 'Slick' : null,
    avatar_url: rank % 3 ? null : 'https://cdn.discordapp.com/avatars/12345678901234567' + (rank % 10) + '/0123456789abcdef0123456789abcdef.png?size=64',
    team: rank % 2 ? null : { tgid: 14, name: 'Falcons', abbrev: 'ATL', logo_url: '/live-assets/teams/14-128.png', primary: '#A71930' },
    wins: 40 - Math.floor(rank / 4), losses: 3 + Math.floor(rank / 5), elo: 1500 - rank * 4, delta_week: rank % 4 === 0 ? 16 : rank % 4 === 1 ? -9 : 0, you: rank === 87 });
  window.__board = ({ limit = 10, offset = 0 } = {}) => {
    if (window.__oldServer) return { ok: true, enabled: true, total: 130, rows: Array.from({ length: Math.min(limit, 50) }, (_, i) => row(i + 1)).map((r) => ({ ...r })), you: row(87) };
    const rows = []; for (let r = offset + 1; r <= Math.min(130, offset + limit); r++) rows.push(row(r));
    return { ok: true, enabled: true, total: 130, offset, rows, you: rows.some((r) => r.you) ? null : row(87) };
  };
  let t;
  Object.defineProperty(window, '__TAURI__', { configurable: true, get() { return t; }, set(v) {
    const inv = v.core.invoke;
    v.core.invoke = async (cmd, args = {}) => (cmd === 'leaderboard' ? window.__board(args) : inv(cmd, args));
    t = v;
  } });
})();`;
const LB = (name, url = 'leaderboard.html', extra = {}) => ({ name, url, scope: 'main', ...extra });
{
  const { page, errors, close } = await openState(browser, srv.base, LB('leaderboard-1280'), { init: BOARD_INIT });
  assert.equal(await page.locator('h1').textContent(), 'Leaderboard');
  assert.equal(await page.locator('.lb-rows a.lb-row:not(.you)').count(), 50);
  assert.equal(await page.locator('.lb-total').textContent(), '130 ranked');
  assert.equal(await page.locator('.lb-pager span').textContent(), 'Page 1 of 3');
  assert.equal(await page.locator('a.lb-row.you').count(), 1, 'your row pinned under page 1');
  assert.equal(await page.locator('a.lb-row.you .lb-rank').textContent(), '87');
  assert.equal(await page.locator('.lb-rows a.lb-row').nth(1).locator('.lb-discord').textContent(), '@Tuna', 'a nickname shows its Discord name under it');
  for (const s of await page.locator('img').evaluateAll((ns) => ns.map((n) => n.getAttribute('src'))))
    assert.match(s, /^(art\/|https:\/\/(api\.nflstreetreborn\.com\/live-assets\/|cdn\.discordapp\.com\/avatars\/))/, s);
  ok('the full leaderboard: 50 rows, "130 ranked", Page 1 of 3, your #87 pinned, Discord names under nicknames, allowed images');
  await shot(page, 'leaderboard-1280');
  await page.locator('[data-key="lb-jump"]').click();
  await page.waitForFunction(() => document.querySelector('.lb-pager span')?.textContent === 'Page 2 of 3');
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), `lb-${F.ME}`);
  assert.match(page.url(), /leaderboard\.html\?page=2$/);
  assert.equal(await page.locator('[data-key="lb-jump"]').count(), 0, 'no Jump to me once you are on the page');
  ok('"Jump to me" opens page 2, marks and focuses your row, and the address keeps the page');
  await shot(page, 'leaderboard-jump-to-me');
  // A row opens the profile; Back there returns to page 2 with that row focused.
  const target = '00000000-0000-4000-8000-000000000060';
  await page.locator(`[data-key="lb-${target}"]`).click();
  await landed(page, new RegExp(`/profile\\.html\\?player=${target}$`), 'a row opens that player\'s profile');
  await page.locator('#back').waitFor();
  await page.locator('#back').click();
  await landed(page, /\/leaderboard\.html\?page=2$/, 'Back on the profile returns to the same leaderboard page');
  await page.waitForFunction((k) => document.activeElement?.getAttribute('data-key') === k, `lb-${target}`, { timeout: 5000 });
  ok('...with the row that opened it focused again');
  await page.locator('[data-key="lb-next"]').click();
  await page.waitForFunction(() => document.querySelector('.lb-pager span')?.textContent === 'Page 3 of 3');
  assert.equal(await page.locator('.lb-rows a.lb-row:not(.you)').count(), 30);
  assert.equal(await page.locator('.lb-rows a.lb-row').first().locator('.lb-rank').textContent(), '101');
  assert.ok(await page.locator('[data-key="lb-next"]').isDisabled());
  ok('Next reaches the last page (ranks 101-130), Next then disabled');
  assert.deepEqual(errors, []);
  await close();
}
{ // 800x560: no sideways scroll, names keep their room
  const { page, errors, close } = await openState(browser, srv.base, LB('leaderboard-800'), { init: BOARD_INIT, viewport: { width: 800, height: 560 } });
  const over = await page.evaluate(() => [document.documentElement.scrollWidth - document.documentElement.clientWidth, document.getElementById('main').scrollWidth - document.getElementById('main').clientWidth]);
  assert.ok(over.every((x) => x <= 1), `no sideways scroll at 800x560: ${over}`);
  const nameW = await page.locator('.lb-rows a.lb-row .lb-name').first().evaluate((n) => n.getBoundingClientRect().width);
  assert.ok(nameW > 150, `a name has room at 800x560 (${nameW}px)`);
  ok('800x560: no sideways scroll, names have room');
  await shot(page, 'leaderboard-800');
  assert.deepEqual(errors, []);
  await close();
}
{ // by controller: Down walks the rows, A opens one, B comes back; B on the page returns to Online
  const { page, errors, close } = await openState(browser, srv.base, LB('leaderboard-pad'), { init: BOARD_INIT, pad: true });
  const press = async (b) => {
    await page.evaluate((b) => { pads[0].buttons[b] = { pressed: true, value: 1 }; }, b);
    await page.waitForTimeout(90);
    await page.evaluate((b) => { pads[0].buttons[b] = { pressed: false, value: 0 }; }, b);
    await page.waitForTimeout(120);
  };
  const key = () => page.evaluate(() => document.activeElement?.getAttribute('data-key'));
  await page.locator('.lb-rows a.lb-row').first().focus();
  const first = await key();
  await press(13);
  await press(13);
  const third = await key();
  assert.equal(third, 'lb-00000000-0000-4000-8000-000000000003', `Down twice from #1 is #3 (${first} -> ${third})`);
  await shot(page, 'leaderboard-pad-focus');
  await press(0);
  await landed(page, /\/profile\.html\?player=00000000-0000-4000-8000-000000000003$/, 'controller: Down walks the rows, A opens the focused player\'s profile');
  await page.locator('#back').waitFor();
  await press(1);
  await landed(page, /\/leaderboard\.html$/, 'controller: B on the profile returns to the leaderboard');
  await page.waitForFunction((k) => document.activeElement?.getAttribute('data-key') === k, third, { timeout: 5000 });
  ok('...on the same row');
  assert.deepEqual(errors, []);
  await close();
}
{ // A server that does not page yet: its top 50, one page, no Jump to me
  const { page, errors, close } = await openState(browser, srv.base, LB('leaderboard-old-server'), { init: `${BOARD_INIT};window.__oldServer=true;` });
  assert.equal(await page.locator('.lb-rows a.lb-row:not(.you)').count(), 50);
  assert.equal(await page.locator('.lb-pager').count(), 0);
  assert.equal(await page.locator('[data-key="lb-jump"]').count(), 0);
  assert.equal(await page.locator('.lb-note').textContent(), 'Showing the top 50.');
  ok('an older server (no paging): the top 50 as one page, saying so');
  assert.deepEqual(errors, []);
  await close();
}
{ // Elo off
  const { page, errors, close } = await openState(browser, srv.base, LB('leaderboard-soon', 'leaderboard.html', { data: { leaderboard: { ok: true, enabled: false, total: 0, rows: [], you: null } } }));
  assert.equal(await page.locator('.lb-soon').textContent(), 'Rankings drop soon.');
  ok('Elo off: "Rankings drop soon."');
  await shot(page, 'leaderboard-soon');
  assert.deepEqual(errors, []);
  await close();
}
{ // From Online: Full leaderboard opens it; Back returns to Online
  const { page, errors, close } = await openState(browser, srv.base, { name: 'leaderboard-from-online', url: 'online.html', scope: 'main', data: { mm_state: mm(empty) } }, { init: `${PICK};${BOARD_INIT}` });
  await page.getByRole('button', { name: 'Full leaderboard ›', exact: true }).click();
  await landed(page, /\/leaderboard\.html$/, 'Online\'s "Full leaderboard ›" opens the launcher\'s own page');
  await page.locator('.lb-rows a.lb-row').first().waitFor();
  await page.keyboard.press('Escape');
  await landed(page, /\/online\.html$/, 'Esc on the leaderboard returns to Online');
  assert.deepEqual(errors, []);
  await close();
}

await browser.close();
await srv.close();
console.log(`check-live-feed: PASS (${passed} checks, ${shots.length} screens in ${out})`);
