import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fmtInt, fmtPct, fmtRecord, fmtElo, gameRowModel, matchRowModel, roleCards, totalLine, youGrid, trendPoints, neighbours, backHref } from './stats-model.js';

test('number formats', () => {
  assert.equal(fmtInt(1284), '1,284'); assert.equal(fmtInt(null), '—'); assert.equal(fmtInt(NaN), '—');
  assert.equal(fmtPct(0.63), '.630'); assert.equal(fmtPct(1), '1.000'); assert.equal(fmtPct(null), '—');
  assert.equal(fmtRecord({ wins: 14, losses: 9, ties: 1 }), '14–9–1'); assert.equal(fmtRecord({ wins: 3, losses: 0, ties: 0 }), '3–0');
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
  assert.equal(c[0].stat, '2,104 pass yds'); assert.equal(c[0].initial, 'P');
  assert.equal(c[1].name, 'None yet'); assert.equal(c[1].stat, '');
});

test('totalLine is one plain line, empty without a total', () => {
  assert.equal(totalLine({ total: { cmp: 212, att: 371, yds: 3108, td: 41, int: 18, rtg: 104.2 } }, 'passing'), 'Total  212/371 · 3,108 yds · 41 TD · 18 INT · 104.2 rtg');
  assert.equal(totalLine({ total: null }, 'passing'), '');
});

test('youGrid and trendPoints', () => {
  const g = youGrid({ wins: 14, losses: 9, ties: 1, win_pct: 0.63, streak: 'W2', pts_per_game: 31.4, yds_per_game: 412 }, { abbrev: 'ATL', logo_url: '/l' });
  assert.deepEqual(g.map((x) => x.label), ['Record', 'Win %', 'Streak', 'Pts / gm', 'Yds / gm', 'Plays as']);
  assert.equal(g[5].value, 'ATL');
  assert.equal(youGrid({ wins: 0, losses: 0, ties: 0, win_pct: null, streak: null, pts_per_game: null, yds_per_game: null }, null)[2].value, '—');
  assert.equal(trendPoints([{ rating: 1000 }]), '');
  assert.equal(trendPoints([{ rating: 1000 }, { rating: 1100 }], 100, 50), '0,50 100,0');
});

test('neighbours in a newest-first list', () => {
  assert.deepEqual(neighbours(['a', 'b', 'c'], 'b'), { prev: 'c', next: 'a' });
  assert.deepEqual(neighbours(['a', 'b'], 'a'), { prev: 'b', next: null });
  assert.deepEqual(neighbours(['a'], 'x'), { prev: null, next: null });
  assert.deepEqual(neighbours(null, 'a'), { prev: null, next: null });
});

test('backHref: only stats.html or a profile of a real player id, else Stats', () => {
  const id = '0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d';
  assert.equal(backHref('stats.html'), 'stats.html');
  assert.equal(backHref(`profile.html?player=${id}`), `profile.html?player=${id}`);
  for (const bad of [undefined, null, 42, '', 'profile.html', 'profile.html?player=nope', `profile.html?player=${id}&x=1`,
    `profile.html?player=${id.toUpperCase()}`, 'https://evil.example/', 'javascript:alert(1)', '//evil.example/stats.html',
    'game.html?game=' + id, 'stats.html#x', ` stats.html`])
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

test('lastTen and youNumbers', () => {
  assert.equal(lastTen(['W', 'W', 'L', 'W', 'L', 'W', 'W', 'L', 'W', 'L', 'W']), '6–4');
  assert.equal(lastTen([]), '—');
  assert.equal(lastTen(['T', 'W']), '1–0');
  const n = youNumbers({ wins: 14, losses: 9, ties: 0, win_pct: 0.609, streak: 'W2' }, { results: ['W', 'L'] });
  assert.deepEqual(n.map((x) => [x.label, x.value, x.kind]), [['Record', '14–9', ''], ['Win %', '.609', ''], ['Last 10', '1–1', ''], ['Streak', 'W2', 'up']]);
  assert.deepEqual(youNumbers({}, null).map((x) => x.value), ['—', '—', '—', '—']);
  assert.equal(youNumbers({ wins: 1, losses: 3, streak: 'L3' }, null)[3].kind, '');
});

test('statLine: the four numbers of a role when the server sends them, else its one number', () => {
  assert.deepEqual(statLine('passer', { yds: 3108, td: 31, int: 11, rtg: 108.9, value: 3108, unit: 'pass_yds' }),
    [{ value: '3,108', unit: 'YDS' }, { value: '31', unit: 'TD' }, { value: '11', unit: 'INT' }, { value: '108.9', unit: 'RTG' }]);
  assert.deepEqual(statLine('rusher', { att: 212, yds: 1715, td: 12, avg: 5.6 }).map((s) => s.unit), ['CAR', 'YDS', 'TD', 'AVG']);
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
  assert.equal(lastTen(resultsOf(kept)), '2–1');
  const nums = youNumbers({ wins: 2, losses: 1, win_pct: 0.667, streak: 'W1' }, { results: resultsOf(kept) });
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
    fmtElo({ ...PRIV, rating: 1000, last_delta: 5, rank: 3 }), youGrid({ ...PRIV, wins: 1, losses: 0, ties: 0 }, null), youNumbers({ ...PRIV, wins: 1, losses: 0 }, null),
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
  assert.equal(roleCards({ passer: { name: 'A. Passer', yds: 10, unit: 'pass_yds' } })[0].stat, '— pass yds');
});
