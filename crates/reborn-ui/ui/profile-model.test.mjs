import { test } from 'node:test';
import assert from 'node:assert/strict';
import { FULL_STATS, areaPoints, careerTiles, cellValue, eloOverview, isPublic, lastTenOverview, leaderboardRows, liveCard, publicQueues,
  recordOverview, splitTop, statusLine, summaryFacts, winRate, profileRecord, singleGameRecords, careerRecords } from './profile-model.js';

test('private rooms never pass', () => {
  assert.equal(isPublic({ queue: 'private' }), false);
  assert.equal(isPublic({ queue: 'ranked' }), true);
  assert.equal(isPublic({ queue: null }), true);
  assert.equal(isPublic(null), false);
  assert.deepEqual(publicQueues([{ key: 'ranked' }, { key: 'private', name: 'Private room' }, { key: 'x', name: 'Private' }, { key: 'casual' }]).map((r) => r.key), ['ranked', 'casual']);
});

test('summary line: rank, Elo and the online record, each only when the server sent it', () => {
  const byQueue = [{ key: 'ranked', wins: 8, losses: 5 }, { key: 'casual', wins: 6, losses: 4 }, { key: 'private', name: 'Private room', wins: 2, losses: 1 }];
  assert.deepEqual(summaryFacts({ elo: { rank: 7, rating: 1184 }, summary: { wins: 16, losses: 10, win_pct: 0.609 }, by_queue: byQueue }).map((f) => `${f.strong} ${f.text}`),
    ['#7 on the leaderboard', '1,184 Elo', '14-9 record'], 'ranked + casual: the private room never counts');
  assert.deepEqual(summaryFacts({ summary: { wins: 3, losses: 1 } }).map((f) => f.strong), ['3-1'], 'no by_queue: the summary');
  assert.deepEqual(summaryFacts({ summary: { wins: 0, losses: 0 } }), [], 'a new player has no record to show');
  assert.deepEqual(summaryFacts({ elo: { rank: null, rating: null }, summary: { win_pct: null } }), []);
  assert.deepEqual(summaryFacts({}), []);
  assert.equal(winRate(0.5), '50.0%');
  assert.equal(winRate(null), null);
});

test('live card: game N of 3 and the series for a best of 3; never a private room', () => {
  const c = liveCard({ match_id: 'm1', queue: 'ranked', best_of: 3, game_number: 2, series: [1, 0], score: [14, 7], me: { abbrev: 'ATL' }, opp: { abbrev: 'PHI' }, opponent: { display_name: 'Mikel' } });
  assert.equal(c.head, 'Ranked · Game 2 of 3');
  assert.equal(c.score, '14 – 7');
  assert.equal(c.line, 'vs Mikel · Series 1–0');
  assert.equal(c.href, 'live.html?match=m1');
  assert.deepEqual([c.me, c.opp], [{ abbrev: 'ATL' }, { abbrev: 'PHI' }]);
  const one = liveCard({ match_id: 'm2', queue: 'casual', best_of: 1, game_number: 1, series: [0, 0], score: null, opponent: null });
  assert.equal(one.head, 'Casual');
  assert.equal(one.score, null);
  assert.equal(one.line, '');
  assert.equal(liveCard({ match_id: 'm3', queue: 'private' }), null);
  assert.equal(liveCard(null), null);
  assert.equal(liveCard({ queue: 'ranked' }), null);
});

test('status: in a match, online now, or nothing', () => {
  assert.deepEqual(statusLine({ live: { match_id: 'x', queue: 'ranked' }, online: true }), { text: 'In a match', kind: 'live' });
  assert.deepEqual(statusLine({ live: null, online: true }), { text: 'Online now', kind: 'online' });
  assert.deepEqual(statusLine({ live: { match_id: 'x', queue: 'private' }, online: true }), { text: 'Online now', kind: 'online' });
  assert.equal(statusLine({ online: null }), null);
});

test('area chart points', () => {
  assert.deepEqual(areaPoints([1, 3], 100, 50, 0), { line: '0,50 100,0', area: 'M0,50 L100,0 L100,50 L0,50Z' });
  assert.equal(areaPoints([5]), null);
  assert.equal(areaPoints([2, 2], 100, 50, 0).line, '0,25 100,25');
});

test('Elo overview: value, change, peak and start; the peak from history when the server sends none', () => {
  const e = eloOverview({ rating: 1184, last_delta: 16, peak: 1212, history: [{ rating: 1000, delta: 0 }, { rating: 1212, delta: 212 }, { rating: 1184, delta: -28 }] });
  assert.equal(e.value, '1,184');
  assert.deepEqual(e.change, { text: '+16', kind: 'up' });
  assert.equal(e.sub, 'Peak 1,212 · started at 1,000');
  assert.equal(e.note, '3 ranked matches');
  assert.equal(e.points.line.split(' ').length, 4);
  const old = eloOverview({ rating: 1184, last_delta: -18, history: [{ rating: 1202, delta: 2 }, { rating: 1184, delta: -18 }] });
  assert.equal(old.sub, 'Peak 1,202 · started at 1,200');
  assert.deepEqual(old.change, { text: '−18', kind: 'down' });
  assert.equal(eloOverview({ rating: null }), null);
  assert.equal(eloOverview(undefined), null);
});

test('record overview: online only, the headline is ranked + casual, Ranked and Casual two separate figures', () => {
  const r = recordOverview({ wins: 16, losses: 10, win_pct: 0.615 }, [{ key: 'ranked', wins: 8, losses: 5 }, { key: 'casual', wins: 6, losses: 4 }, { key: 'private', name: 'Private room', wins: 2, losses: 1 }]);
  assert.equal(r.value, '14-9');
  assert.equal(r.pct, '60.9%');
  assert.equal(r.note, '23 matches');
  assert.deepEqual(r.queues, [{ label: 'Ranked', record: '8-5', pct: '.615' }, { label: 'Casual', record: '6-4', pct: '.600' }]);
  assert.equal(recordOverview({}, []), null);
  // an older server without by_queue: the summary, no queue figures
  assert.deepEqual(recordOverview({ wins: 2, losses: 1 }, null), { value: '2-1', pct: '66.7%', note: '3 matches', queues: [] });
  assert.deepEqual(profileRecord(null, [{ key: 'casual', wins: 1, losses: 0 }]).ranked, null, 'no ranked row: no ranked figure');
});

test('records: single game rows with "vs <opponent>" from the match list; career leaders from the career tables', () => {
  const at = '2026-09-19T20:00:00.000Z';
  const recs = [{ key: 'pass_yds', label: 'Passing yds', value: 288, game_id: 'g-1', at, name: 'M. Penix Jr.', team: 'Falcons', logo_url: '/live-assets/teams/14-128.png' },
    { key: 'most_points', label: 'Most points', value: 49, game_id: 'g-9', at, name: null },
    { key: 'x', label: 'Tackles', value: 12, game_id: 'g-2', at, name: 'N. Bolton', opponent: { display_name: 'Jock' } }, null, { value: 3 }];
  const matches = [{ id: 'm-1', queue: 'ranked', opponent: { display_name: 'Playz' }, games: [{ id: 'g-0' }, { id: 'g-1' }] },
    { id: 'g:g-9', queue: null, opponent: null, opponent_kind: 'cpu', games: [] }, { id: 'm-p', queue: 'private', opponent: { display_name: 'Friend' }, games: [{ id: 'g-2' }] }];
  const rows = singleGameRecords(recs, matches);
  assert.deepEqual(rows.map((r) => [r.label, r.name, r.value, r.game, r.date, r.game_id]), [
    ['Passing Yards', 'M. Penix Jr.', '288', 'vs Playz', 'Sep 19', 'g-1'],
    ['Most points', null, '49', 'vs CPU', 'Sep 19', 'g-9'],
    ['Tackles', 'N. Bolton', '12', 'vs Jock', 'Sep 19', 'g-2']]);
  assert.equal(rows[0].team.logo_url, '/live-assets/teams/14-128.png');
  assert.equal(singleGameRecords([{ label: 'Rushing yds', value: 3, game_id: 'nope' }], [])[0].game, '—', 'no opponent known: a dash');
  assert.doesNotMatch(JSON.stringify(rows), /Friend|private/i, 'a private room never names its opponent');
  const row = (name, games, cols) => ({ key: name, name, team: 'Falcons', logo_url: null, games, ...cols });
  const career = { passing: { rows: [row('A', 20, { yds: 2210, td: 31 }), row('B', 3, { yds: 561, td: 40 })] },
    rushing: { rows: [row('C', 20, { yds: 1180, td: 12 })] }, receiving: { rows: [row('D', 20, { yds: 1402, td: 14 }), row('C', 20, { yds: 10, td: 3 })] },
    defense: { rows: [row('E', 1, { tkl: 71, sck: 0, int: 2, td: 1 })] } };
  assert.deepEqual(careerRecords(career).map((r) => [r.label, r.name, r.value, r.game]), [
    ['Passing Yards', 'A', '2,210', '20 games'], ['Passing TD', 'B', '40', '3 games'], ['Rushing Yards', 'C', '1,180', '20 games'],
    ['Receiving Yards', 'D', '1,402', '20 games'], ['Total TD', 'C', '15', '20 games'], ['Tackles', 'E', '71', '1 game'], ['Interceptions', 'E', '2', '1 game']]);
  assert.deepEqual(careerRecords(null), []);
});

test('last 10 games: record, average margin, points; nothing from an older server', () => {
  assert.deepEqual(lastTenOverview({ games: 10, wins: 6, losses: 4, avg_margin: 6.4, pts_for_pg: 31.4, pts_against_pg: 25 }),
    { value: '6-4', margin: { text: '+6.4', kind: 'up' }, scored: '31.4', allowed: '25.0' });
  assert.deepEqual(lastTenOverview({ games: 3, wins: 1, losses: 2, avg_margin: -3, pts_for_pg: null, pts_against_pg: 8 }),
    { value: '1-2', margin: { text: '−3.0', kind: 'down' }, scored: '—', allowed: '8.0' });
  assert.equal(lastTenOverview(null), null);
  assert.equal(lastTenOverview({ games: 0, wins: 0, losses: 0 }), null);
  assert.equal(lastTenOverview({ results: ['W'] }), null, 'the dashboard shape is not the profile block');
});

test('career tiles: nine spelled-out labels; the rank and per game only when sent', () => {
  const career = { passing: { total: { yds: 3021, td: 40, rtg: 104.2 } }, rushing: { total: { yds: 1715, td: 17 } }, receiving: { total: { yds: 1714, td: 17 } }, defense: { total: { tkl: 104, sck: 5, int: 9, td: 2 } } };
  const { tiles, footnote } = careerTiles(career, { pass_yds: 3, pass_td: 2, pass_rtg: 5, rush_yds: 4, rec_yds: 6, total_td: 2, tkl: 8, sck: 5, def_int: null, pool: 214 }, 24);
  assert.deepEqual(tiles.map((t) => t.label), ['Passing Yards', 'Passing TD', 'Passer Rating', 'Rushing Yards', 'Receiving Yards', 'Total TD', 'Tackles', 'Sacks', 'Interceptions']);
  assert.deepEqual(tiles[0], { key: 'pass_yds', label: 'Passing Yards', value: '3,021', rank: '#3', perGame: '125.9 / game' });
  assert.deepEqual(tiles[2], { key: 'pass_rtg', label: 'Passer Rating', value: '104.2', rank: '#5', perGame: null });
  assert.equal(tiles[5].value, '36', 'Total TD = rushing + receiving + defensive');
  assert.equal(tiles[8].rank, null);
  assert.equal(footnote, 'Rank among 214 online players');
  const bare = careerTiles(career, undefined, undefined);
  assert.ok(bare.tiles.every((t) => t.rank === null && t.perGame === null));
  assert.equal(bare.footnote, null);
  assert.equal(careerTiles({}, null, 10).tiles[0].value, '—');
});

test('leaderboard rows: "You" only on your own profile', () => {
  const around = [{ rank: 6, display_name: 'Lo', rating: 1197, is_player: false, player_id: 'a' }, { rank: 7, display_name: 'adam', rating: 1184, is_player: true, player_id: 'b' }];
  assert.deepEqual(leaderboardRows(around, { self: true }).map((r) => [r.rank, r.name, r.elo, r.me]), [['#6', 'Lo', '1,197', false], ['#7', 'You', '1,184', true]]);
  assert.equal(leaderboardRows(around, { self: false })[1].name, 'adam');
  assert.deepEqual(leaderboardRows(undefined), []);
});

test('full stats: columns, values, an older server\'s yac for YCO, top five', () => {
  assert.deepEqual(FULL_STATS.passing.cols.map((c) => c.label), ['C/ATT', 'YDS', 'TD', 'INT', 'AIR', 'YAC', 'Y/A', 'RTG', 'LNG']);
  assert.deepEqual(FULL_STATS.rushing.cols.map((c) => c.label), ['CAR', 'YDS', 'AVG', 'TD', 'LNG', 'BTK', 'YCO']);
  assert.deepEqual(FULL_STATS.receiving.cols.map((c) => c.label), ['REC', 'YDS', 'AVG', 'YAC', 'TD', 'LNG', 'DRP']);
  assert.deepEqual(FULL_STATS.defense.cols.map((c) => c.label), ['TKL', 'TFL', 'SCK', 'INT', 'PBU', 'FF', 'FR', 'TD']);
  const [ca, yds] = FULL_STATS.passing.cols;
  assert.equal(cellValue(ca, { cmp: 141, att: 238 }), '141/238');
  assert.equal(cellValue(yds, { yds: 2210 }), '2,210');
  assert.equal(cellValue(FULL_STATS.passing.cols[7], { rtg: 110.5 }), '110.5');
  assert.equal(cellValue(FULL_STATS.passing.cols[7], {}), '—');
  const yco = FULL_STATS.rushing.cols[6];
  assert.equal(cellValue(yco, { yco: 540 }), '540');
  assert.equal(cellValue(yco, { yac: 530 }), '530', 'an older server still calls it yac');
  const { top, rest } = splitTop([1, 2, 3, 4, 5, 6, 7]);
  assert.deepEqual([top.length, rest.length], [5, 2]);
});

// Every output function, fed a private match and tie results: nothing it draws may say Private, ladder, Best of 3, or show a T.
test('no output ever carries Private, ladder, Best of 3 or a T result, even for private and tied input', () => {
  const live = { match_id: 'mp', queue: 'private', best_of: 3, game_number: 2, series: [1, 1], score: [7, 7], opponent: { display_name: 'Mikel' } };
  const tieLive = { match_id: 'mt', queue: 'ranked', best_of: 3, game_number: 3, series: [1, 1], score: [7, 7], opponent: { display_name: 'Mikel' } };
  const byQueue = [{ key: 'ranked', wins: 8, losses: 5, ties: 2 }, { key: 'casual', wins: 6, losses: 4, ties: 1 },
    { key: 'private', name: 'Private room', wins: 2, losses: 1, ties: 3 }, { key: 'pr', name: 'Private', wins: 1, losses: 1 }];
  const career = { passing: { total: { yds: 1, td: 1, rtg: 1 } }, rushing: { total: { yds: 1, td: 1 } }, receiving: { total: { yds: 1, td: 1 } }, defense: { total: { tkl: 1, sck: 1, int: 1, td: 1 } } };
  const out = [
    summaryFacts({ elo: { rank: 7, rating: 1184 }, summary: { win_pct: 0.5, ties: 3 } }),
    liveCard(live), liveCard(tieLive), statusLine({ live, online: true }), statusLine({ live: tieLive, online: true }),
    eloOverview({ rating: 1000, last_delta: 0, history: [{ rating: 1000, delta: 0 }, { rating: 1000, delta: 0 }] }),
    recordOverview({ wins: 14, losses: 9, ties: 4, win_pct: 0.6 }, byQueue),
    lastTenOverview({ games: 10, wins: 5, losses: 4, ties: 1, avg_margin: 0, pts_for_pg: 10, pts_against_pg: 10, results: ['W', 'T'] }),
    careerTiles(career, { pass_yds: 1, pool: 3 }, 5),
    leaderboardRows([{ rank: 1, display_name: 'Lo', rating: 1, is_player: true }], { self: true }),
    publicQueues(byQueue).map((r) => r.key),
  ];
  const text = JSON.stringify(out);
  assert.doesNotMatch(text, /private/i);
  assert.doesNotMatch(text, /ladder/i);
  assert.doesNotMatch(text, /best of 3/i);
  assert.doesNotMatch(text, /tie/i);
  assert.doesNotMatch(text, /"T"|–T|\bT\b/);
  assert.equal(liveCard(live), null);
  assert.equal(statusLine({ live, online: true }).kind, 'online');
  assert.equal(recordOverview({ wins: 14, losses: 9, ties: 4 }, byQueue).value, '14-9');
  assert.equal(recordOverview({ wins: 14, losses: 9, ties: 4 }, byQueue).queues.length, 2);
});

test('absent shapes: an old server gets nulls and empty lists, never a throw', () => {
  assert.equal(lastTenOverview(undefined), null);
  assert.equal(liveCard(undefined), null);
  assert.equal(statusLine(undefined), null);
  assert.deepEqual(leaderboardRows(null), []);
  assert.deepEqual(publicQueues(undefined), []);
  assert.deepEqual(summaryFacts(undefined), []);
  assert.equal(careerTiles(undefined, undefined, undefined).footnote, null);
});

test('withCareerLines: each best player takes its own career row by key; most played never takes pass attempts', async () => {
  const { withCareerLines } = await import('./profile-model.js');
  const career = {
    passing: { rows: [{ key: 'qb', cmp: 10, att: 20, yds: 150, td: 2, int: 1, rtg: 99.1, air: 80 }] },
    rushing: { rows: [{ key: 'rb', att: 30, yds: 120, avg: 4, td: 1, long: 20 }, { key: 'qb', att: 3, yds: 9, td: 0 }] },
    receiving: { rows: [{ key: 'rb', rec: 5, yds: 60, avg: 12, td: 0 }] },
    defense: { rows: [] },
  };
  const b = withCareerLines({ passer: { key: 'qb', value: 150, unit: 'pass_yds' }, rusher: { key: 'rb', value: 120, unit: 'rush_yds' },
    most_played: { key: 'rb', value: 7, unit: 'games' }, dl_lb: null, ballhawk: { key: 'nobody', value: 1, unit: 'int' } }, career);
  assert.deepEqual(b.passer, { cmp: 10, att: 20, yds: 150, td: 2, int: 1, rtg: 99.1, key: 'qb', value: 150, unit: 'pass_yds' });
  assert.equal(b.rusher.long, 20);
  assert.deepEqual(b.most_played, { att: 30, yds: 120, td: 1, games: 7, key: 'rb', value: 7, unit: 'games' });
  assert.equal(b.dl_lb, null);
  assert.deepEqual(b.ballhawk, { key: 'nobody', value: 1, unit: 'int' });
  // no career sent (an older server): the players stay as they came
  assert.deepEqual(withCareerLines({ passer: { key: 'qb', value: 1, unit: 'pass_yds' } }, undefined), { passer: { key: 'qb', value: 1, unit: 'pass_yds' } });
  assert.deepEqual(withCareerLines(null, career), {});
});
