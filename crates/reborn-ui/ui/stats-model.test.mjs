import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fmtInt, fmtPct, fmtElo, gameRowModel, matchRowModel, roleCards, totalLine, trendPoints, backHref, matchHref, gameHref, readMatchAddress, onlineRecord } from './stats-model.js';

test('number formats', () => {
  assert.equal(fmtInt(1284), '1,284'); assert.equal(fmtInt(null), '—'); assert.equal(fmtInt(NaN), '—');
  assert.equal(fmtPct(0.63), '.630'); assert.equal(fmtPct(1), '1.000'); assert.equal(fmtPct(null), '—');
});

test('fmtElo: rated and unranked', () => {
  assert.deepEqual(fmtElo({ rating: 1184, last_delta: 16, rank: 7 }), { value: '1,184', change: '▲ 16', rank: '#7 on the leaderboard' });
  assert.deepEqual(fmtElo({ rating: 990, last_delta: -18, rank: 30 }).change, '▼ 18');
  assert.deepEqual(fmtElo({ rating: null }), { value: 'Unranked', change: '', rank: '' });
  assert.deepEqual(fmtElo(undefined), { value: 'Unranked', change: '', rank: '' });
});

test('gameRowModel: ranked Elo, offline tag, CPU', () => {
  const base = { id: 'g', at: '2026-09-27T01:18:00Z', mode: 'online', score: [36, 14], result: 'win', opponent: { display_name: 'Playz', avatar_url: 'https://cdn.discordapp.com/avatars/1/a.png?size=64' }, opponent_kind: 'human',
    me: { name: 'Falcons', logo_url: '/live-assets/r/v/teams/14-256.png' }, opp: { name: 'Eagles', logo_url: null } };
  const r = gameRowModel({ ...base, elo_delta: 16 });
  assert.equal(r.score, '36–14'); assert.equal(r.result, 'W'); assert.equal(r.tag, '+16 Elo'); assert.equal(r.tagKind, 'up'); assert.equal(r.opponent, 'Playz');
  assert.equal(gameRowModel({ ...base, elo_delta: -18 }).tag, '−18 Elo');
  const off = gameRowModel({ ...base, mode: 'offline', opponent: null, opponent_kind: 'cpu', elo_delta: null });
  assert.equal(off.tag, 'Offline'); assert.equal(off.tagKind, 'offline'); assert.equal(off.opponent, 'CPU');
  assert.equal(gameRowModel({ ...base, score: null, result: null }).score, '—');
  const guest = gameRowModel({ ...base, opponent: null, opponent_kind: 'guest', elo_delta: null });
  assert.equal(guest.opponent, 'Guest');
});

test('matchRowModel: a series shows 2–1 and expands; one game shows its score and opens it', () => {
  const g1 = { id: 'a', at: '2026-09-27T01:00:00Z', mode: 'online', score: [14, 10], result: 'win', elo_delta: 16, opponent: { display_name: 'Playz' }, opponent_kind: 'human', me: {}, opp: {} };
  const m = matchRowModel({ id: 'm1', at: '2026-09-27T02:00:00Z', mode: 'online', series: [2, 1], score: [42, 52], result: 'win', elo_delta: 16,
    opponent: { display_name: 'Playz' }, opponent_kind: 'human', me: {}, opp: {}, games: [g1, { ...g1, id: 'b' }, { ...g1, id: 'c' }] });
  assert.equal(m.score, '2–1'); assert.equal(m.expandable, true); assert.equal(m.onlyGame, null);
  assert.equal(m.tag, '+16 Elo'); assert.equal(m.result, 'W');
  assert.equal(m.games.length, 3); assert.equal(m.games[0].tag, '', 'Elo shows once, on the match');
  const one = matchRowModel({ id: 'g:a', at: g1.at, mode: 'offline', series: null, score: [36, 14], result: 'win', elo_delta: null,
    opponent: null, opponent_kind: 'cpu', me: {}, opp: {}, games: [{ ...g1, mode: 'offline' }] });
  assert.equal(one.score, '36–14'); assert.equal(one.expandable, false); assert.equal(one.onlyGame, 'a'); assert.equal(one.tag, 'Offline');
  const guest = matchRowModel({ id: 'g:b', at: g1.at, mode: 'online', series: null, score: [28, 21], result: 'win', elo_delta: null,
    opponent: null, opponent_kind: 'guest', me: {}, opp: {}, games: [{ ...g1, id: 'b', opponent_kind: 'guest' }] });
  assert.equal(guest.opponent, 'Guest');
});

test('roleCards: six cards in order, None yet when empty', () => {
  const c = roleCards({ passer: { name: 'Penix', team: 'Falcons', logo_url: '/l', portrait_url: null, value: 2104, unit: 'pass_yds' }, rusher: null, receiver: null, dl_lb: null, ballhawk: null, most_played: null });
  assert.deepEqual(c.map((x) => x.label), ['Best passer', 'Best rusher', 'Best receiver', 'Best DL/LB', 'Ballhawk', 'Most played']);
  assert.equal(c[0].stat, '2,104 passing yards'); assert.equal(c[0].initial, 'P');
  assert.equal(c[1].name, 'None yet'); assert.equal(c[1].stat, '');
});

test('totalLine is one plain line, empty without a total', () => {
  assert.equal(totalLine({ total: { cmp: 212, att: 371, yds: 3108, td: 41, int: 18, rtg: 104.2 } }, 'passing'), 'Total  212/371 · 3,108 yds · 41 TD · 18 INT · 104.2 rtg');
  assert.equal(totalLine({ total: null }, 'passing'), '');
});

test('trendPoints', () => {
  assert.equal(trendPoints([{ rating: 1000 }]), '');
  assert.equal(trendPoints([{ rating: 1000 }, { rating: 1100 }], 100, 50), '0,50 100,0');
});

test('backHref: only stats.html or a profile of a real player id, each with a short #view, else Stats', () => {
  const id = '0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d';
  assert.equal(backHref('stats.html'), 'stats.html');
  assert.equal(backHref('stats.html#records'), 'stats.html#records');
  assert.equal(backHref(`profile.html?player=${id}`), `profile.html?player=${id}`);
  assert.equal(backHref(`profile.html?player=${id}#matches`), `profile.html?player=${id}#matches`);
  for (const bad of [undefined, null, 42, '', 'profile.html', 'profile.html?player=nope', `profile.html?player=${id}&x=1`,
    `profile.html?player=${id.toUpperCase()}`, 'https://evil.example/', 'javascript:alert(1)', '//evil.example/stats.html',
    'game.html?game=' + id, 'stats.html#X', 'stats.html#a b', `stats.html#${'x'.repeat(30)}`, ` stats.html`, `profile.html?player=${id}#<b>`])
    assert.equal(backHref(bad), 'stats.html', String(bad));
});

import { eloChange, matchSubline, lastTen, youNumbers, statLine, teamColour, dropPrivate, resultsOf, recordOf } from './stats-model.js';

test('result letters are W and L only: ties do not exist', () => {
  const g = { id: 'g', at: '2026-09-27T01:18:00Z', mode: 'online', score: [21, 21], opponent: null, opponent_kind: 'cpu', me: {}, opp: {} };
  assert.equal(gameRowModel({ ...g, result: 'tie' }).result, '');
  assert.equal(gameRowModel({ ...g, result: 'LOSS' }).result, 'L');
  assert.equal(gameRowModel({ ...g, result: null }).result, '');
});

test('eloChange: +16, −18, or nothing', () => {
  assert.deepEqual(eloChange(16), { text: '+16', kind: 'up' });
  assert.deepEqual(eloChange(-18), { text: '−18', kind: 'down' });
  for (const x of [0, null, undefined, NaN]) assert.deepEqual(eloChange(x), { text: '', kind: '' });
});

test('matchSubline: Stats says Offline only, Profile says the queue, never Best of 3 or Private', () => {
  assert.equal(matchSubline({ mode: 'offline' }), 'Offline');
  assert.equal(matchSubline({ mode: 'online', opponent_kind: 'cpu' }), 'Offline');
  assert.equal(matchSubline({ mode: 'online', queue: 'private' }), '');
  assert.equal(matchSubline({ mode: 'online', queue: 'ranked', best_of: 3 }), '');
  assert.equal(matchSubline({ queue: 'ranked', best_of: 3 }, 'profile'), 'Ranked');
  assert.equal(matchSubline({ queue: 'casual' }, 'profile'), 'Casual');
  assert.equal(matchSubline({ queue: 'private' }, 'profile'), '');
});

const G = (n) => `00000000-0000-4000-8000-00000000000${n}`;
test('matchHref: one game opens that game; a match of several opens one page with all of them, the last one shown', () => {
  assert.equal(matchHref({ games: [{ id: G(1) }] }, 'stats.html'), `match.html?game=${G(1)}&from=stats.html`);
  const three = matchHref({ games: [{ id: G(1) }, { id: G(2) }, { id: G(3) }], series: [2, 1] }, 'stats.html');
  assert.equal(three, `match.html?game=${G(3)}&games=${encodeURIComponent([G(1), G(2), G(3)].join(','))}&series=2-1&from=stats.html`);
  assert.equal(matchHref({ games: [] }, 'stats.html'), null);
  assert.equal(matchHref({}, 'stats.html'), null);
  assert.match(matchHref({ games: [{ id: G(1) }] }, 'https://evil.example/'), /from=stats\.html$/, 'Back is validated on the way out too');
  assert.equal(gameHref(G(4), 'stats.html#records'), `match.html?game=${G(4)}&from=stats.html%23records`);
});

test("matchHref: a matchmade match (a match id) opens the match page, the site's /live/<match>; an unlinked one the same page by its games", () => {
  const M = 'dc5226af-d3b8-4c0b-89e8-1c53b2972a30';
  assert.equal(matchHref({ match_id: M, games: [{ id: G(1) }, { id: G(2) }], series: [1, 1] }, 'stats.html#matches'), `match.html?match=${M}&from=stats.html%23matches`);
  assert.equal(matchHref({ match_id: M, games: [{ id: G(1) }] }, 'https://evil.example/'), `match.html?match=${M}&from=stats.html`, 'Back is validated');
  // no match id, or one that is not a lowercase UUID (an offline game's list key): the same page, by its games
  assert.match(matchHref({ match_id: null, games: [{ id: G(1) }] }, 'stats.html'), /^match\.html\?game=/);
  assert.match(matchHref({ match_id: 'g:g-031', games: [{ id: G(1) }] }, 'stats.html'), /^match\.html\?game=/);
  assert.match(matchHref({ match_id: M.toUpperCase(), games: [{ id: G(1) }] }, 'stats.html'), /^match\.html\?game=/);
  // no recorded game: no link at all
  assert.equal(matchHref({ match_id: M, games: [] }, 'stats.html'), null);
});

test('readMatchAddress: the shown game, the match, the series, Back; anything odd falls back to one game', () => {
  const a = readMatchAddress(new URL('http://x/' + matchHref({ games: [{ id: G(1) }, { id: G(2) }], series: [1, 1] }, 'stats.html#records')).search);
  assert.deepEqual(a, { game: G(2), games: [G(1), G(2)], series: '1-1', back: 'stats.html#records' });
  assert.deepEqual(readMatchAddress(`?game=${G(1)}`), { game: G(1), games: [G(1)], series: null, back: 'stats.html' });
  // the shown game is not in the list: just that game
  assert.deepEqual(readMatchAddress(`?game=${G(5)}&games=${G(1)},${G(2)}&series=1-1`).games, [G(5)]);
  // only UUIDs make the list; a bad series is dropped; a bad Back is Stats
  assert.deepEqual(readMatchAddress(`?game=${G(2)}&games=${G(1)},nope,${G(2)}&series=2–1&from=javascript:x`), { game: G(2), games: [G(1), G(2)], series: null, back: 'stats.html' });
});

test('onlineRecord: online only, Ranked and Casual apart, offline, CPU and private rooms never count', () => {
  const m = (result, extra) => ({ result, mode: 'online', queue: 'ranked', opponent_kind: 'human', ...extra });
  const r = onlineRecord([m('win'), m('win', { queue: 'casual' }), m('loss', { mode: 'offline', queue: null, opponent_kind: 'cpu' }),
    m('loss'), m('win', { queue: 'private' }), m('loss', { queue: 'casual' }), m('loss', { queue: 'casual', opponent_kind: 'cpu' })]);
  assert.deepEqual([r.all.text, r.ranked.text, r.casual.text], ['2-2', '1-1', '1-1']);
  assert.deepEqual(r.results, ['W', 'W', 'L', 'L']);
  assert.equal(r.streak, 'W2');
  const none = onlineRecord(null);
  assert.deepEqual([none.all.text, none.streak, none.all.pct], ['—', '—', null]);
});

test('lastTen and youNumbers', () => {
  assert.equal(lastTen(['W', 'W', 'L', 'W', 'L', 'W', 'W', 'L', 'W', 'L', 'W']), '6-4');
  assert.equal(lastTen([]), '—');
  assert.equal(lastTen(['T', 'W']), '1-0');
  const on = (result, queue) => ({ result, queue, mode: 'online', opponent_kind: 'human' });
  const n = youNumbers(onlineRecord([on('win', 'ranked'), on('win', 'casual'), on('loss', 'ranked')]));
  assert.deepEqual(n.nums.map((x) => [x.label, x.value, x.kind]), [['Record', '2-1', ''], ['Win %', '.667', ''], ['Last 10', '2-1', ''], ['Streak', 'W2', 'up']]);
  assert.deepEqual(n.queues, [{ label: 'Ranked', value: '1-1', pct: '.500' }, { label: 'Casual', value: '1-0', pct: '1.000' }]);
  assert.deepEqual(youNumbers(onlineRecord([])).nums.map((x) => x.value), ['—', '—', '—', '—']);
  assert.equal(youNumbers(onlineRecord([on('loss', 'ranked')])).nums[3].kind, '');
});

test('statLine: the fuller line of a role from the fields the server sends, missing ones left out, else its one number', () => {
  assert.deepEqual(statLine('passer', { cmp: 141, att: 238, yds: 3108, td: 31, int: 11, rtg: 108.9, value: 3108, unit: 'pass_yds' }),
    [{ value: '141/238', unit: 'C/ATT' }, { value: '3,108', unit: 'YDS' }, { value: '31', unit: 'TD' }, { value: '11', unit: 'INT' }, { value: '108.9', unit: 'RTG' }]);
  // no cmp/att sent: C/ATT is left out, never "—/—" or NaN
  assert.deepEqual(statLine('passer', { yds: 3108, td: 31, int: 11, rtg: 108.9 }).map((s) => s.unit), ['YDS', 'TD', 'INT', 'RTG']);
  assert.deepEqual(statLine('passer', { cmp: 1, att: null, yds: 3, td: 0, rtg: NaN }).map((s) => `${s.value} ${s.unit}`), ['3 YDS', '0 TD']);
  assert.deepEqual(statLine('rusher', { att: 212, yds: 1715, td: 12, avg: 5.6, long: 61 }).map((s) => `${s.value} ${s.unit}`), ['212 CAR', '1,715 YDS', '5.6 AVG', '12 TD', '61 LNG']);
  assert.deepEqual(statLine('receiver', { rec: 98, yds: 1402, avg: 14.3, td: 14 }).map((s) => s.unit), ['REC', 'YDS', 'AVG', 'TD']);
  assert.deepEqual(statLine('dl_lb', { tkl: 71, sck: 5, tfl: 9, ff: 2, pbu: 6 }).map((s) => s.unit), ['TKL', 'SCK', 'TFL', 'FF', 'PBU']);
  assert.deepEqual(statLine('ballhawk', { int: 7, pbu: 14, tkl: 33, td: 1 }).map((s) => s.unit), ['INT', 'PBU', 'TKL', 'TD']);
  // one field alone is not a line: the one number instead
  assert.deepEqual(statLine('passer', { td: 3, value: 40, unit: 'pass_yds' }), [{ value: '40', unit: 'YDS' }]);
  assert.deepEqual(statLine('most_played', { games: 23, att: 96, yds: 535, td: 5 }).map((s) => `${s.value} ${s.unit}`), ['23 GAMES', '96 CAR', '535 YDS', '5 TD']);
  assert.deepEqual(statLine('most_played', { games: 23, rec: 40, yds: 535, td: 5 })[1], { value: '40', unit: 'REC' });
  assert.deepEqual(statLine('passer', { value: 3108, unit: 'pass_yds' }), [{ value: '3,108', unit: 'YDS' }]);
  assert.deepEqual(statLine('dl_lb', { value: 96, unit: 'tkl_sck' }), [{ value: '96', unit: 'TKL + SCK' }]);
  assert.deepEqual(statLine('passer', null), []);
});

test('teamColour, and role cards carry the colour and the statline', () => {
  assert.equal(teamColour('#A71930'), '#A71930');
  for (const x of ['red', '#fff', 'url(x)', null, '#A7193']) assert.equal(teamColour(x), null);
  const c = roleCards({ passer: { name: 'Penix', team: 'Falcons', logo_url: '/l', portrait_url: null, value: 2104, unit: 'pass_yds', primary: '#A71930' }, rusher: null });
  assert.equal(c[0].primary, '#A71930');
  assert.deepEqual(c[0].line, [{ value: '2,104', unit: 'YDS' }]);
  assert.equal(c[1].primary, null);
  assert.deepEqual(c[1].line, []);
});

// P4-3 / P4-6: private matches never reach Stats.
const PRIV = { id: 'p', at: '2026-09-29T01:00:00Z', mode: 'online', queue: 'private', result: 'win', elo_delta: null, opponent: { display_name: 'Friend' }, opponent_kind: 'human', score: [7, 0], series: null, games: [{ id: 'p-g1', at: '2026-09-29T01:00:00Z', result: 'win', score: [7, 0], me: {}, opp: {} }], me: {}, opp: {} };
const PUB = (id, result, extra = {}) => ({ id, at: '2026-09-28T01:00:00Z', mode: 'online', queue: 'ranked', result, elo_delta: result === 'win' ? 12 : -12, opponent: { display_name: 'Playz' }, opponent_kind: 'human', score: [14, 7], series: null, games: [{ id: `${id}-g1`, at: '2026-09-28T01:00:00Z', result, score: [14, 7], me: {}, opp: {} }], me: {}, opp: {}, ...extra });

test('dropPrivate removes private matches and keeps order', () => {
  const list = [PRIV, PUB('a', 'win'), PUB('b', 'loss')];
  assert.deepEqual(dropPrivate(list).map((m) => m.id), ['a', 'b']);
  assert.deepEqual(dropPrivate(null), []);
});

test('no Stats output counts or mentions a private match', () => {
  const kept = dropPrivate([PRIV, PUB('a', 'win'), PUB('b', 'loss'), PUB('c', 'win')]);
  assert.deepEqual(resultsOf(kept), ['W', 'L', 'W']);
  assert.deepEqual(recordOf(kept), { wins: 2, losses: 1 });
  assert.equal(lastTen(resultsOf(kept)), '2-1');
  const nums = youNumbers(onlineRecord(kept));
  const rows = kept.map(matchRowModel);
  const keptOut = JSON.stringify([nums, rows, kept.map((m) => matchSubline(m)), kept.map((m) => matchSubline(m, 'profile'))]);
  assert.ok(!/private/i.test(keptOut));
  assert.ok(!keptOut.includes('Friend'));
  const PV = { ...PRIV, value: 1, unit: 'games' };
  const PT = { ...PRIV, cmp: 1, att: 1, yds: 1, td: 1, int: 1, rtg: 1, avg: 1, rec: 1, tkl: 1, sck: 1, pbu: 1 };
  // The output functions called ON the private match itself never say "Private" either.
  const onPriv = JSON.stringify([
    matchRowModel(PRIV),
    PRIV.games.map(gameRowModel),
    gameRowModel(PRIV),
    matchSubline(PRIV), matchSubline(PRIV, 'profile'),
    roleCards({ passer: PV, rusher: PV, receiver: PV, dl_lb: PV, ballhawk: PV, most_played: PV }),
    totalLine({ total: PT }, 'passing'), totalLine({ total: PT }, 'rushing'), totalLine({ total: PT }, 'receiving'), totalLine({ total: PT }, 'defense'),
    fmtElo({ ...PRIV, rating: 1000, last_delta: 5, rank: 3 }), youNumbers(onlineRecord([PRIV])),
  ]);
  assert.ok(!/private/i.test(onPriv));
  assert.equal(matchSubline(PRIV), '');
});

test('ties and unknown results count for nothing', () => {
  assert.deepEqual(recordOf([{ result: 'tie' }, { result: 'win' }]), { wins: 1, losses: 0 });
  assert.deepEqual(resultsOf([{ result: 'tie' }, {}, { result: 'draw' }, null, { result: 'win' }, { result: 'loss' }]), ['W', 'L']);
  assert.ok(!resultsOf([{ result: 'tie' }, { result: 'T' }]).includes('T'));
  assert.deepEqual(eloChange('16'), { text: '', kind: '' });
});

test('fmtInt: a missing number (undefined, a string) is a dash, never a throw', () => {
  assert.equal(fmtInt(undefined), '—'); assert.equal(fmtInt('12'), '—');
  assert.equal(roleCards({ passer: { name: 'A. Passer', yds: 10, unit: 'pass_yds' } })[0].stat, '— passing yards');
});

test('spellYards: a spelled-out label says Yards; a bare YDS heading stays', async () => {
  const { spellYards } = await import('./profile-model.js');
  assert.equal(spellYards('Passing yds'), 'Passing Yards');
  assert.equal(spellYards('Rushing YDS'), 'Rushing Yards');
  assert.equal(spellYards('YDS'), 'YDS');
  assert.equal(spellYards('Tackles'), 'Tackles');
  assert.equal(spellYards(null), '');
});

// ---- Stats + Profile v2 (mv2 mockup) ----
import * as V2 from './stats-model.js';
const isoDay = (d, h = 20) => new Date(Date.UTC(2026, 8, d, h)).toISOString();
const g = (id, d, score, extra = {}) => ({ id, at: isoDay(d), score, result: score[0] > score[1] ? 'win' : 'loss', ...extra });
const m = (id, games, extra = {}) => {
  const w = games.filter((x) => x.result === 'win').length;
  return { id, at: games.at(-1).at, mode: 'online', queue: 'ranked', opponent_kind: 'human', games, result: w * 2 > games.length ? 'win' : 'loss', ...extra };
};

test('recordCard: by_queue wins when sent; else counted from the online matches; offline, CPU and private never count', () => {
  const list = [m('a', [g('a1', 20, [28, 21])]), m('b', [g('b1', 19, [10, 30])], { queue: 'casual' }), m('c', [g('c1', 18, [49, 7])], { mode: 'offline', queue: null, opponent_kind: 'cpu' }),
    m('d', [g('d1', 17, [31, 30])], { queue: 'private' })];
  const fromList = V2.recordCard(undefined, list);
  assert.equal(fromList.source, 'list');
  assert.equal(fromList.text, '1-1');
  assert.deepEqual([fromList.ranked.text, fromList.ranked.rate, fromList.ranked.pts], ['1-0', '100.0%', '28.0']);
  assert.deepEqual([fromList.casual.text, fromList.casual.pts], ['0-1', '10.0']);
  const server = V2.recordCard([{ key: 'ranked', wins: 8, losses: 5, pts_per_game: 31.4 }, { key: 'casual', wins: 6, losses: 4, pts_per_game: 28.9 }, { key: 'private', wins: 9, losses: 0 }], list);
  assert.equal(server.source, 'server');
  assert.deepEqual([server.text, server.rate, server.games], ['14-9', '60.9%', 23]);
  assert.deepEqual([server.ranked.text, server.ranked.rate, server.ranked.pts, server.casual.rate], ['8-5', '61.5%', '31.4', '60.0%']);
  assert.ok(Math.abs(server.ranked.share - 8 / 13) < 1e-9);
  assert.equal(V2.recordCard([], []), null, 'no online match: no card');
  assert.equal(V2.recordCard(null, [list[2], list[3]]), null, 'only offline and private: no card');
});

test('lastTenStrip, streakOf, lastTenGames', () => {
  const list = Array.from({ length: 12 }, (_, i) => m(`x${i}`, [g(`x${i}`, 28 - i, i % 3 === 2 ? [10, 20] : [30, 20])]));
  const s = V2.lastTenStrip(list);
  assert.deepEqual(s.results, ['W', 'W', 'L', 'W', 'W', 'L', 'W', 'W', 'L', 'W']);
  assert.deepEqual([s.text, s.streak, s.streakKind, s.scored, s.allowed], ['7-3', 'W2', 'up', '24.0', '20.0']);
  assert.equal(V2.lastTenStrip([]), null);
  assert.equal(V2.streakOf(['L', 'L', 'W']), 'L2');
  assert.equal(V2.streakOf([]), '—');
  // a best of 3 adds each of its games, newest first
  assert.deepEqual(V2.lastTenGames([m('s', [g('s1', 20, [7, 14]), { ...g('s2', 20, [21, 14]), at: isoDay(20, 21) }])]), ['W', 'L']);
});

test('eloCard: rated, unranked, off; nothing invented', () => {
  const e = V2.eloCard({ rating: 1077, rank: 7, total: 42, games: 13, peak: 1080, week_delta: 12, history: [{ at: isoDay(12), rating: 1025, delta: 25 }, { at: isoDay(13), rating: 1013, delta: -12 }, { at: isoDay(14), rating: 1077, delta: 64 }] });
  assert.deepEqual([e.state, e.value, e.rank, e.rankOf, e.peak, e.week.text, e.week.kind, e.games], ['rated', '1,077', '#7', '#7 of 42', '1,080', '+12', 'up', 13]);
  assert.deepEqual(e.values, [1000, 1025, 1013, 1077], 'the curve starts where the first match began');
  assert.deepEqual(e.labels.map((l) => l.text), ['Sep 12', 'Sep 12', 'Sep 14']);
  const bare = V2.eloCard({ rating: 1184, rank: 7, last_delta: 16 });
  assert.deepEqual([bare.week, bare.rankOf, bare.peak, bare.values], [null, '#7', null, []], 'no week_delta, total or history: nothing made up');
  assert.equal(V2.eloCard({ rating: 990, week_delta: -9 }).week.text, '−9');
  assert.deepEqual(V2.eloCard({ rating: null, rank: null, total: 0, history: [] }), { state: 'off', title: 'Unranked', note: 'Rankings start soon.' });
  assert.equal(V2.eloCard(undefined).state, 'off');
  assert.equal(V2.eloCard({ rating: null, total: 38 }).state, 'unranked');
});

test('statCells: five numbers per role from the full row, box-score labels (C/ATT, CAR); the old value/unit shape falls back to one', () => {
  const cells = (r, d) => V2.statCells(r, d).map((c) => `${c.value} ${c.label}`);
  assert.deepEqual(cells('passer', { cmp: 41, att: 68, yds: 1284, td: 14, int: 3, rtg: 118.6, value: 1284, unit: 'pass_yds' }), ['41/68 C/ATT', '1,284 YDS', '14 TD', '3 INT', '118.6 RTG']);
  assert.deepEqual(cells('rusher', { att: 52, yds: 611, avg: 11.75, td: 7, long: 74 }), ['52 CAR', '611 YDS', '11.8 AVG', '7 TD', '74 LONG']);
  assert.deepEqual(cells('receiver', { rec: 23, yds: 688, avg: 29.9, td: 8, long: 81 }), ['23 REC', '688 YDS', '29.9 AVG', '8 TD', '81 LONG']);
  assert.deepEqual(cells('dl_lb', { tkl: 19, sck: 6, tfl: 8, ff: 3, pbu: 2 }), ['19 TKL', '6 SCK', '8 TFL', '3 FF', '2 PBU']);
  assert.deepEqual(cells('ballhawk', { int: 5, pbu: 7, tkl: 14, ff: 2, td: 2 }), ['5 INT', '7 PBU', '14 TKL', '2 FF', '2 TD']);
  // most played: games, then the picked row's line (a passer here), four numbers
  assert.deepEqual(cells('most_played', { games: 11, cmp: 38, att: 61, yds: 1102, td: 12, int: 2, rtg: 109.1, value: 11, unit: 'games' }), ['11 GAMES', '38/61 C/ATT', '1,102 YDS', '12 TD', '109.1 RTG']);
  assert.deepEqual(cells('most_played', { att: 96, yds: 535, td: 5, value: 23, unit: 'games' }), ['23 GAMES', '96 CAR', '535 YDS', '5 TD']);
  // a field not sent is left out, never a dash or NaN
  assert.deepEqual(cells('passer', { yds: 3108, td: 31, rtg: NaN, cmp: 1 }), ['3,108 YDS', '31 TD']);
  // the deployed server: value + unit only
  assert.deepEqual(cells('passer', { value: 3108, unit: 'pass_yds' }), ['3,108 PASS YDS']);
  assert.deepEqual(cells('dl_lb', { value: 96, unit: 'tkl_sck' }), ['96 TKL + SCK']);
  assert.deepEqual(cells('most_played', { value: 23, unit: 'games' }), ['23 GAMES']);
  assert.deepEqual(V2.statCells('passer', null), []);
  assert.deepEqual(V2.statCells('passer', { name: 'x' }), []);
});

test('pointsSeries: the newest online games, oldest first, axis rounded to its step', () => {
  const list = Array.from({ length: 20 }, (_, i) => m(`p${i}`, [g(`p${i}`, 28 - i, [20 + i, 15])])).concat([m('off', [g('off', 29, [70, 0])], { mode: 'offline', opponent_kind: 'cpu' })]);
  const s = V2.pointsSeries(list);
  assert.equal(s.games.length, 16);
  assert.deepEqual(s.games.map((x) => x.for).slice(0, 3), [35, 34, 33], 'oldest of the newest sixteen first; the offline 70 left out');
  assert.equal(s.games.at(-1).label, 'Sep 28');
  assert.deepEqual([s.max, s.step, s.allowed], [40, 10, '15.0']);
  assert.equal(V2.pointsSeries([list[0]]), null);
});

test('resultsByDay: 24 days ending on the newest match, a day stacks oldest first, the best day', () => {
  const list = [m('a', [g('a', 26, [1, 0])]), m('b', [{ ...g('b', 19, [0, 1]), at: isoDay(19, 21) }]), m('c', [g('c', 19, [1, 0])]), m('d', [{ ...g('d', 19, [1, 0]), at: isoDay(19, 22) }])];
  const r = V2.resultsByDay(list);
  assert.equal(r.days.length, 24);
  assert.equal(r.days.at(-1).label, 'Sep 26');
  assert.deepEqual(r.days.find((d) => d.label === 'Sep 19').results, ['W', 'L', 'W']);
  assert.deepEqual([r.tallest, r.best], [3, 'Sep 19 · 2-1']);
  assert.equal(V2.resultsByDay([]), null);
});

test('teamRates, yardsSplit, teamPerGame, winsLosses, queueMark, rowSpans', () => {
  const t = V2.teamRates([{ key: 'tgid:1', name: 'Ravens', logo_url: '/l', wins: 7, losses: 3 }, { key: 'pickup', name: 'Pickup', wins: 1, losses: 1 }, { key: 'tgid:2', name: 'Rams', wins: 0, losses: 0 }]);
  assert.deepEqual(t.map((x) => [x.name, x.record, x.pct]), [['Ravens', '7-3', '70%'], ['Pickup', '1-1', '50%']]);
  assert.deepEqual(t[1].team, { abbrev: 'PU' });
  assert.deepEqual(V2.yardsSplit({ pass: 136.4, rush: 93.8 }), { total: '230', pass: '136.4', rush: '93.8', passShare: 136.4 / 230.2, passPct: '59%', rushPct: '41%' });
  assert.equal(V2.yardsSplit({ pass: 0, rush: 0 }), null);
  assert.equal(V2.yardsSplit({ pass: 10 }), null);
  assert.deepEqual(V2.teamPerGame({ games: 23, yds: 230.2, pass: 136.4, biggest_lead: 36, turnovers: null }).map((c) => `${c.label} ${c.value}${c.unit ? ` ${c.unit}` : ''}`),
    ['Yards / game 230.2', 'Passing 136.4 Yards', 'Biggest lead 36']);
  assert.deepEqual(V2.teamPerGame(null), []);
  assert.deepEqual(V2.winsLosses({ wins: { games: 14, pts: 38.2, yds: 284, to: 0.9 }, losses: { games: 9, pts: 19.6, yds: 161.4, to: 2.3 } }).rows,
    [{ label: 'Points', win: '38.2', loss: '19.6' }, { label: 'Yards', win: '284', loss: '161' }, { label: 'Turnovers', win: '0.9', loss: '2.3' }]);
  assert.equal(V2.winsLosses({ wins: { games: 3 }, losses: { games: 0 } }), null);
  assert.deepEqual([V2.queueMark({ mode: 'online', queue: 'ranked' }), V2.queueMark({ mode: 'online', queue: 'casual' }), V2.queueMark({ mode: 'offline' })].map((q) => q.kind), ['ranked', 'casual', 'off']);
  assert.deepEqual(V2.rowSpans([['a', 7], ['b', 5]]), [['a', 7], ['b', 5]]);
  assert.deepEqual(V2.rowSpans([['a', 6], [null, 3], ['c', 3]]), [['a', 8], ['c', 4]]);
  assert.deepEqual(V2.rowSpans([[null, 7], ['b', 5]]), [['b', 12]]);
  assert.deepEqual(V2.rowSpans([]), []);
});

test('most played: Games, then four numbers of that player’s role only (Adam 10-03, position-relevant)', () => {
  const labels = (d) => V2.statCells('most_played', d).map((c) => c.label);
  // the live server's rows (api /v1/players/6d8ed15e.../profile, 10-03): a passing row, a rushing row, a receiving row, a defensive row
  const qb = { key: 'p11', games: 3, cmp: 8, att: 16, yds: 407, td: 5, int: 1, air: 214, yac: 193, ypa: 25.4, rtg: 109.4, long: 73, sacked: 5, value: 3, unit: 'games' };
  const rb = { key: 'p44', games: 2, att: 8, yds: 150, avg: 18.8, td: 1, long: 79, btk: 9, yco: 12, value: 2, unit: 'games' };
  const wr = { key: 'p4', games: 3, rec: 4, yds: 250, avg: 62.5, yac: 120, td: 4, long: 73, drops: 0, value: 3, unit: 'games' };
  const db = { key: 'p53', games: 2, tkl: 8, tfl: 2, sck: 0, int: 3, pbu: 1, ff: 2, fr: 2, td: 1, value: 2, unit: 'games' };
  assert.deepEqual(labels(qb), ['GAMES', 'C/ATT', 'YDS', 'TD', 'RTG']);
  assert.deepEqual(labels(rb), ['GAMES', 'CAR', 'YDS', 'AVG', 'TD']);
  assert.deepEqual(labels(wr), ['GAMES', 'REC', 'YDS', 'AVG', 'TD']);
  assert.deepEqual(labels(db), ['GAMES', 'TKL', 'SCK', 'INT', 'FF']);
  assert.deepEqual(V2.statCells('most_played', qb).map((c) => c.value), ['3', '8/16', '407', '5', '109.4']);
  assert.deepEqual([qb, rb, wr, db].map(V2.playerKind), ['passer', 'rusher', 'receiver', 'defender']);
  // a position field, when a server sends one, decides
  assert.equal(V2.playerKind({ position: 'te', att: 1, rec: 3 }), 'receiver');
  assert.deepEqual(labels({ position: 'LB', tkl: 9, sck: 1, int: 0, ff: 1, games: 4 }), ['GAMES', 'TKL', 'SCK', 'INT', 'FF']);
  // never another role's numbers: a quarterback shows no tackles
  assert.ok(!labels({ ...qb, tkl: 4 }).includes('TKL'));
});
