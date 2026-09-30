import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fmtInt, fmtPct, fmtRecord, fmtElo, gameRowModel, matchRowModel, roleCards, totalLine, youGrid, trendPoints } from './stats-model.js';

test('number formats', () => {
  assert.equal(fmtInt(1284), '1,284'); assert.equal(fmtInt(null), '—'); assert.equal(fmtInt(NaN), '—');
  assert.equal(fmtPct(0.63), '.630'); assert.equal(fmtPct(1), '1.000'); assert.equal(fmtPct(null), '—');
  assert.equal(fmtRecord({ wins: 14, losses: 9, ties: 1 }), '14–9–1'); assert.equal(fmtRecord({ wins: 3, losses: 0, ties: 0 }), '3–0');
});

test('fmtElo: rated and unranked', () => {
  assert.deepEqual(fmtElo({ rating: 1184, last_delta: 16, rank: 7 }), { value: '1,184', change: '▲ 16', rank: '#7 on the ladder' });
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
