// node --test crates/reborn-ui/ui/online-model.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { int, fmtInt, clock, modeLine, seriesLine, acceptWithin, eloBits, standing, seekLine, heldNotice, everyoneReady, inviteLink, inviteLine, leaderboardView, boardStanding } from './online-model.js';

test('numbers and the clock', () => {
  assert.equal(int(3), 3); assert.equal(int('3'), null); assert.equal(int(NaN), null); assert.equal(int(null), null);
  assert.equal(fmtInt(1184), '1,184');
  assert.equal(clock(42000), '0:42'); assert.equal(clock(61000), '1:01'); assert.equal(clock(-5), '0:00');
});

test('the mode line never says best of', () => {
  assert.equal(modeLine('ranked'), 'Ranked · first to two wins');
  assert.equal(modeLine('casual'), 'Casual');
  assert.equal(modeLine(undefined), 'Casual');
  assert.equal(modeLine('casual', 3), 'Casual · first to two wins', 'casual is a best of 3 too (Adam 10-04)');
  assert.equal(modeLine('casual', 1), 'Casual');
  assert.equal(modeLine('casual', 20), 'Casual', 'an open-ended casual match names no series');
  assert.equal(modeLine('ranked', 3), 'Ranked · first to two wins');
});

test('the series line is from this seat, only in a series', () => {
  assert.equal(seriesLine({ best_of: 3, series: [1, 0], seat: 0 }), 'Game 2 · Series 1-0');
  assert.equal(seriesLine({ best_of: 3, series: [0, 1], seat: 1 }), 'Game 2 · Series 1-0');
  assert.equal(seriesLine({ best_of: 3, series: [1, 1], seat: 0 }), 'Game 3 · Series 1-1');
  assert.equal(seriesLine({ best_of: 1, series: [0, 0], seat: 0 }), null);
  assert.equal(seriesLine({ best_of: null, series: null }), null);
  assert.equal(seriesLine(null), null);
});

test('accept within counts down to the deadline and hides when there is none', () => {
  const now = Date.UTC(2026, 8, 30, 20, 0, 0);
  assert.equal(acceptWithin('2026-09-30T20:00:14.000Z', now), '0:14 to accept');
  assert.equal(acceptWithin('2026-09-30T20:00:00.500Z', now), '0:00 to accept');
  assert.equal(acceptWithin('2026-09-30T20:00:00.000Z', now), null, 'the deadline has come: hidden');
  assert.equal(acceptWithin('2026-09-30T19:59:00.000Z', now), null, 'the deadline has passed: hidden');
  assert.equal(acceptWithin(null, now), null);
  assert.equal(acceptWithin('not a date', now), null);
});

test('Elo bits hide whatever the server did not send', () => {
  assert.deepEqual(eloBits({ rating: 1184, rank: 7, week_delta: 16 }), { rank: '#7', elo: '1,184 Elo', week: { dir: 'up', text: '▲ 16 this week' } });
  assert.deepEqual(eloBits({ rating: 1184, rank: 7, week_delta: -9 }).week, { dir: 'down', text: '▼ 9 this week' });
  assert.equal(eloBits({ rating: 1184, rank: 7, week_delta: 0 }).week, null);
  assert.equal(eloBits({ rating: 1184, rank: 7 }).week, null);
  assert.deepEqual(eloBits({ rating: null, rank: null, week_delta: null }), { rank: null, elo: null, week: null });
  assert.deepEqual(eloBits({ rating: null, rank: 3, week_delta: 5 }), { rank: null, elo: null, week: null }, 'Elo off: nothing, even if a rank leaked');
  assert.deepEqual(eloBits(undefined), { rank: null, elo: null, week: null });
  assert.equal(eloBits({ rating: 1000, rank: 0 }).rank, null);
});

test('standing joins only what exists', () => {
  assert.equal(standing(9, 1162), '#9 · 1,162 Elo');
  assert.equal(standing(null, 1162), '1,162 Elo');
  assert.equal(standing(9, null), '#9');
  assert.equal(standing(0, null), '');
  assert.equal(standing(undefined, 'x'), '');
});

test('the searching pill', () => {
  assert.equal(seekLine('ranked', { rating: 1184 }), 'Finding players near 1,184 Elo');
  assert.equal(seekLine('ranked', { rating: null }), 'Looking for a ranked game');
  assert.equal(seekLine('ranked', null), 'Looking for a ranked game');
  assert.equal(seekLine('casual', { rating: 1184 }), 'Finding a casual game');
});

test('the held notice keeps each reason\'s words', () => {
  assert.equal(heldNotice({ hold_reason: 'x' }).title, 'Your last match is held for review');
  assert.equal(heldNotice({ hold_reason: 'series-too-fast' }).text, 'Nothing changed on the leaderboard. Someone will look at it. You can play again now.');
  assert.match(heldNotice({ hold_reason: 'content-stale' }).text, /^A game update never reached one of your game discs, so that game was not started\./);
  assert.equal(heldNotice({ hold_reason: 'content-mismatch', opponent: 'Jock' }).text,
    "Your game files differ from Jock's, so that game was not started. Both of you: Settings → Check game content, install what it offers, then find a game again.");
  assert.match(heldNotice({ hold_reason: 'content-mismatch', opponent: null }).text, /your opponent's/);
  for (const opponent of ['', undefined, 42]) {
    const t = heldNotice({ hold_reason: 'content-mismatch', opponent }).text;
    assert.match(t, /^Your game files differ from your opponent's,/, `opponent ${JSON.stringify(opponent)}`);
  }
});

test('everyone ready needs two players, all ready', () => {
  assert.equal(everyoneReady({ members: [{ ready: true }, { ready: true }] }), true);
  assert.equal(everyoneReady({ members: [{ ready: true }, { ready: false }] }), false);
  assert.equal(everyoneReady({ members: [{ ready: true }] }), false);
  assert.equal(everyoneReady(null), false);
  // 2v2 rooms: three or four players, Start only when every one of them is ready.
  assert.equal(everyoneReady({ members: [{ ready: true }, { ready: true }, { ready: true }, { ready: true }] }), true);
  assert.equal(everyoneReady({ members: [{ ready: true }, { ready: true }, { ready: false }] }), false);
});

test('invite link and line', () => {
  assert.equal(inviteLink('K7QP2M'), 'https://nflstreetreborn.com/join.php?r=K7QP2M');
  assert.equal(inviteLink('a b&c'), 'https://nflstreetreborn.com/join.php?r=a%20b%26c');
  assert.deepEqual(inviteLine('Jock', 'K7QP2M'), { who: 'Jock', rest: ' wants you in the room · K7QP2M' });
  assert.deepEqual(inviteLine(null, 'K7QP2M'), { who: null, rest: "You've got a room invite · K7QP2M" });
  for (const bad of [null, undefined, '', 42]) {
    assert.equal(inviteLink(bad), null, `no link for ${bad}`);
    assert.equal(inviteLine('Jock', bad), null, `no line for ${bad}`);
  }
  assert.deepEqual(inviteLine('', 'K7QP2M'), { who: null, rest: "You've got a room invite · K7QP2M" });
});

test('the leaderboard model', () => {
  assert.deepEqual(leaderboardView(null), { soon: true });
  assert.deepEqual(leaderboardView({ enabled: false, rows: [] }), { soon: true });
  assert.deepEqual(leaderboardView({ enabled: true, rows: [] }), { soon: true });
  const rows = Array.from({ length: 12 }, (_, i) => ({ rank: i + 1, display_name: `P${i}`, avatar_url: null, team: null, wins: 5, losses: 2, elo: 1300 - i, delta_week: i === 0 ? 16 : i === 1 ? -9 : 0, you: false }));
  const v = leaderboardView({ enabled: true, total: 38, rows, you: { rank: 14, display_name: 'Me', wins: 1, losses: 1, elo: 1001, delta_week: 0 } });
  assert.equal(v.soon, false); assert.equal(v.total, 38); assert.equal(v.rows.length, 10, 'top 10 only');
  assert.deepEqual(v.rows[0], { rank: 1, name: 'P0', avatar: null, team: null, record: '5-2', elo: '1,300', change: { dir: 'up', text: '▲ 16' }, you: false });
  assert.deepEqual(v.rows[1].change, { dir: 'down', text: '▼ 9' });
  assert.equal(v.rows[2].change, null);
  assert.equal(v.you.rank, 14); assert.equal(v.you.you, true);
  const inTop = leaderboardView({ enabled: true, total: 3, rows: [{ ...rows[0], you: true }], you: { rank: 1, display_name: 'P0' } });
  assert.equal(inTop.you, null, 'you are already on the card');
  const unflagged = leaderboardView({ enabled: true, total: 12, rows, you: { rank: 7, display_name: 'P6', elo: 1294 } });
  assert.equal(unflagged.you, null, 'your rank is on the card even without the you flag: you are not drawn twice');
  const bare = leaderboardView({ enabled: true, rows: [{ rank: null, display_name: null, wins: null, losses: 2, elo: null, delta_week: null }] });
  assert.deepEqual(bare.rows[0], { rank: null, name: '', avatar: null, team: null, record: null, elo: null, change: null, you: false });
});

test('your standing comes from the leaderboard answer, in the shape eloBits reads', () => {
  const team = { tgid: 14, name: 'Falcons', abbrev: 'ATL', logo_url: '/live-assets/teams/14-128.png' };
  const rows = [{ rank: 1, elo: 1320, delta_week: 16, team: null, you: false }, { rank: 2, elo: 1302, delta_week: -9, team, you: true }];
  assert.deepEqual(boardStanding({ enabled: true, rows, you: null }), { elo: { rating: 1302, rank: 2, week_delta: -9 }, team }, 'your flagged row on the card');
  assert.deepEqual(boardStanding({ enabled: true, rows: rows.map((r) => ({ ...r, you: false })), you: { rank: 14, elo: 1104, delta_week: 0, team: null } }),
    { elo: { rating: 1104, rank: 14, week_delta: 0 }, team: null }, 'your pinned row further down');
  assert.deepEqual(eloBits(boardStanding({ enabled: true, rows, you: null }).elo), { rank: '#2', elo: '1,302 Elo', week: { dir: 'down', text: '▼ 9 this week' } });
  assert.deepEqual(boardStanding({ enabled: true, rows: [{ rank: 3, elo: 'x', delta_week: null, team: 'ATL', you: true }] }),
    { elo: { rating: null, rank: 3, week_delta: null }, team: null }, 'what does not read is null');
  for (const none of [null, undefined, { enabled: false, total: 0, rows: [], you: null }, { enabled: true, rows: rows.map((r) => ({ ...r, you: false })), you: null }, { enabled: true }]) {
    assert.equal(boardStanding(none), null, `no standing in ${JSON.stringify(none)}`);
  }
});
