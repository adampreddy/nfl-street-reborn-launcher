// League page v3, layout F (spec docs/superpowers/specs/2026-10-17-league-page-v3-design.md): the banner states, the
// Week card's order, Week N's best and the site links. A Week 3 answer in the real Season 4 shape (2026-10-17).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { seasonLine, teamCardLine, bannerState, weekGames, weekBest, siteUrl } from './league-model.js';

const OPP_ID = '553962568008335382';
const READY = { installed: true, version: '2026.10.17.2', latest: '2026.10.17.2', update_available: false, size_bytes: 1503238553, installing: false };
const team = (id, name, abbr, record) => ({ team_id: id, name, abbr, record, logo_url: null });
const H = () => ({
  ok: true, signed_in: true, member: true, schedule_published: true,
  league: { id: 36, name: 'Reborn Season 4', season: 4, week: 3, week_label: 'Week 3' },
  team: { id: 493, name: 'Subway Surfers', abbr: 'ADAM', record: '2-0', standing: 2 },
  matchup: { fixture_id: 2614, week: 3, home: true, status: 'scheduled', forfeit: false, score: null, series: null, winner_team_id: null,
    opponent: { team_id: 484, name: 'Wolves Reign', abbr: 'SNIPER', record: '2-0', owner_display: 'SNIPER', discord_user_id: OPP_ID, online: true },
    room: null, match_id: null },
  week_games: [
    { fixture_id: 2614, home: team(493, 'Subway Surfers', 'ADAM', '2-0'), away: team(484, 'Wolves Reign', 'SNIPER', '2-0'), status: 'scheduled', score: null, series: null, winner_team_id: null },
    { fixture_id: 2615, home: team(488, 'Kings', 'PROPEL', '2-1'), away: team(490, 'Patriots', 'DREAD', '1-2'), status: 'final', score: { home: 28, away: 14 }, series: { home: 2, away: 0 }, winner_team_id: 488 },
    { fixture_id: 2616, home: team(486, 'Football Furies', 'BLONDE', '2-1'), away: team(489, 'Commandments', 'MOSES', '2-1'), status: 'scheduled', score: null, series: null, winner_team_id: null },
    { fixture_id: 2617, home: team(485, 'Wildcats', 'JACOB', '0-3'), away: team(491, 'Team Korra', 'MIKEL', '1-2'), status: 'final', score: { home: 17, away: 21 }, series: { home: 0, away: 2 }, winner_team_id: 491 },
  ],
  standings: [{ rank: 1, team_id: 492, name: 'Big Tunas', abbr: 'TUNA', record: '3-0' }],
  week_best: { week: 3, groups: [
    { label: 'Passing', rows: [{ poid: 12, name: 'P. Mahomes', team_id: 492, line: '318 yds · 4 TD' }, { poid: 13, name: 'C. Stroud', team_id: 491, line: '274 yds · 3 TD' },
      { poid: 14, name: 'J. Goff', team_id: 488, line: '251 yds · 2 TD' }, { poid: 15, name: 'Fourth', team_id: 485, line: '1 yds · 0 TD' }] },
    { label: 'Rushing', rows: [{ poid: 22, name: 'D. Henry', team_id: 492, line: '142 yds · 2 TD' }] },
    { label: 'Receiving', rows: [] },
  ] },
});
const withMu = (patch) => { const h = H(); h.matchup = { ...h.matchup, ...patch }; return h; };
const acts = (s) => s.actions.map((a) => [a.key, a.label, a.primary === true, a.disabled === true]);
const ROOM = { room_id: 'r1', code: 'K7QP2M', members: 1, expires_at: null, state: 'waiting' };

test('seasonLine: league name and week label', () => {
  assert.equal(seasonLine(H()), 'Reborn Season 4 · Week 3');
  assert.equal(seasonLine({ league: { name: 'Reborn Season 4', week: 3 } }), 'Reborn Season 4 · Week 3');
  assert.equal(seasonLine({ league: { week: 3 } }), 'Week 3');
  assert.equal(seasonLine({}), '');
});

test('teamCardLine: record and place', () => {
  assert.equal(teamCardLine(H()), '2-0 · 2nd');
  assert.equal(teamCardLine({ team: { record: '0-0', standing: 11 } }), '0-0 · 11th');
  assert.equal(teamCardLine({ team: { record: '1-0' } }), '1-0');
});

test('bannerState: before the game: Play offline, Friendly, Play {opponent team}', () => {
  const s = bannerState(H(), { edition: READY });
  assert.equal(s.kind, 'play');
  assert.equal(s.mid, 'VS');
  assert.equal(s.scores, null);
  assert.equal(s.text, '');
  assert.deepEqual(acts(s), [['offline', 'Play offline', false, false], ['friendly', 'Friendly', false, false], ['play', 'Play Wolves Reign', true, false]]);
});

test('bannerState: rooms: waiting, join, lobby', () => {
  const waiting = bannerState(withMu({ room: { ...ROOM, host_discord_user_id: '111' } }), { edition: READY });
  assert.equal(waiting.text, 'Waiting for SNIPER to join');
  assert.deepEqual(acts(waiting), [['cancel', 'Cancel', false, false]]);
  const join = bannerState(withMu({ room: { ...ROOM, host_discord_user_id: OPP_ID } }), { edition: READY });
  assert.equal(join.text, 'SNIPER opened the room');
  assert.deepEqual(acts(join), [['join', 'Join', true, false]]);
  const lobby = bannerState(withMu({ room: { ...ROOM, members: 2, state: 'lobby', host_discord_user_id: OPP_ID } }), { edition: READY });
  assert.equal(lobby.text, 'SNIPER is in the room');
  assert.deepEqual(acts(lobby), [['open', 'Open room', true, false]]);
});

test('bannerState: live shows the game score and the series', () => {
  const s = bannerState(withMu({ status: 'live', score: { home: 14, away: 7 }, series: { home: 1, away: 0 } }), { edition: READY });
  assert.equal(s.kind, 'live');
  assert.equal(s.mid, 'LIVE');
  assert.deepEqual(s.scores, [14, 7]);
  assert.equal(s.text, 'Game 2 of 3 · you lead 1-0');
  assert.deepEqual(acts(s), [['back', 'Back to the game', true, false]]);
  const away = bannerState(withMu({ home: false, status: 'live', score: { home: 14, away: 7 }, series: { home: 1, away: 0 } }), { edition: READY });
  assert.deepEqual(away.scores, [7, 14], 'you first, from the away side');
  assert.equal(away.text, 'Game 2 of 3 · you trail 0-1');
});

test('bannerState: between games of a series', () => {
  const s = bannerState(withMu({ status: 'in_progress', series: { home: 1, away: 0 } }), { edition: READY });
  assert.equal(s.kind, 'play');
  assert.deepEqual(s.scores, [1, 0]);
  assert.equal(s.text, 'Game 2 next · you lead 1-0');
  assert.equal(acts(s).at(-1)[1], 'Play Wolves Reign');
  assert.equal(bannerState(withMu({ status: 'in_progress', series: { home: 1, away: 1 } }), { edition: READY }).text, 'Game 3 next · tied 1-1');
});

test('bannerState: final, lost, forfeit', () => {
  const won = bannerState(withMu({ status: 'final', series: { home: 2, away: 1 }, winner_team_id: 493 }), { edition: READY });
  assert.equal(won.mid, 'FINAL');
  assert.deepEqual(won.scores, [2, 1]);
  assert.equal(won.text, 'You won the series 2-1');
  assert.equal(won.tone, 'ok');
  assert.deepEqual(acts(won), [['offline', 'Play offline', false, false], ['friendly', 'Friendly', false, false]]);
  const lost = bannerState(withMu({ status: 'final', series: { home: 1, away: 2 }, winner_team_id: 484 }), { edition: READY });
  assert.equal(lost.text, 'You lost the series 1-2');
  assert.equal(lost.tone, 'bad');
  const ff = bannerState(withMu({ status: 'final', forfeit: true, winner_team_id: 493 }), { edition: READY });
  assert.equal(ff.text, 'You won by forfeit');
  assert.equal(ff.scores, null);
  assert.equal(bannerState(withMu({ status: 'final', forfeit: true, winner_team_id: 484 }), { edition: READY }).text, 'You lost by forfeit');
});

test('bannerState: bye, no schedule, unlinked opponent', () => {
  const bye = bannerState(withMu({ status: 'bye', opponent: null }), { edition: READY });
  assert.equal(bye.kind, 'bye');
  assert.equal(bye.text, 'No game this week');
  assert.deepEqual(acts(bye).map((a) => a[0]), ['offline', 'friendly']);
  const h = H(); h.schedule_published = false; h.matchup = null;
  assert.equal(bannerState(h, { edition: READY }).text, 'No schedule yet');
  const unl = bannerState(withMu({ opponent: { ...H().matchup.opponent, discord_user_id: null } }), { edition: READY });
  assert.equal(unl.text, "SNIPER hasn't linked Discord on the league site");
  assert.deepEqual(acts(unl).at(-1), ['play', 'Play Wolves Reign', true, true]);
});

test('bannerState: league files missing or behind take the action slot, except live and waiting', () => {
  const none = bannerState(H(), { edition: { ...READY, installed: false, version: null } });
  assert.deepEqual(acts(none), [['install', 'Install league files (1.4 GB)', true, false]]);
  const upd = bannerState(H(), { edition: { ...READY, update_available: true } });
  assert.deepEqual(acts(upd), [['install', 'Update league files (1.4 GB)', true, false]]);
  const busy = bannerState(H(), { edition: READY, progress: { phase: 'download', done: 50, total: 100 } });
  assert.deepEqual(acts(busy), [['install', 'Downloading league files… 50%', true, true]]);
  assert.equal(busy.actions[0].percent, 50);
  const live = bannerState(withMu({ status: 'live', series: { home: 0, away: 0 } }), { edition: { ...READY, update_available: true } });
  assert.deepEqual(acts(live).map((a) => a[0]), ['back']);
  const waiting = bannerState(withMu({ room: { ...ROOM, host_discord_user_id: '111' } }), { edition: { ...READY, update_available: true } });
  assert.deepEqual(acts(waiting).map((a) => a[0]), ['cancel']);
  const unknown = bannerState(H(), { edition: null });
  assert.deepEqual(acts(unknown), [['install', 'Checking league files…', true, true]]);
});

test('bannerState: a catch-up game says its own week; stale data offers no room buttons', () => {
  assert.equal(bannerState(withMu({ week: 2 }), { edition: READY }).mid, 'Week 2 game');
  const st = bannerState(withMu({ room: { ...ROOM, host_discord_user_id: OPP_ID } }), { edition: READY, stale: true });
  assert.equal(st.kind, 'stale');
  assert.deepEqual(acts(st).map((a) => a[0]), ['offline', 'friendly']);
  const stPlay = bannerState(H(), { edition: READY, stale: true });
  assert.equal(stPlay.kind, 'play', 'Play still asks the server');
});

test('weekGames: finals first, then yours, then the rest; markers and sides', () => {
  const g = weekGames(H());
  assert.deepEqual(g.map((x) => x.fixture_id), [2615, 2617, 2614, 2616]);
  assert.deepEqual(g.map((x) => x.marker), ['Final', 'Final', 'You', '']);
  assert.deepEqual(g.map((x) => x.mine), [false, false, true, false]);
  assert.deepEqual([g[0].away.name, g[0].away.score, g[0].away.winner, g[0].home.score, g[0].home.winner], ['Patriots', 14, false, 28, true]);
  assert.equal(g[2].home.score, null, 'no score before a game is played');
  assert.equal(g[2].home.team_id, 493);
  const h = H(); h.week_games[2].status = 'live'; h.week_games[2].score = { home: 7, away: 3 };
  assert.equal(weekGames(h).find((x) => x.fixture_id === 2616).marker, 'Live');
  const catchup = withMu({ fixture_id: 9999, week: 2 });
  assert.equal(weekGames(catchup).filter((x) => x.mine).length, 0, 'a catch-up fixture is not in this week');
  assert.deepEqual(weekGames({}), []);
});

test("weekBest: title, top 3, the leader marked; empty and fallback", () => {
  const b = weekBest(H());
  assert.equal(b.title, "Week 3's Top Performers");
  assert.deepEqual(b.groups.map((x) => x.label), ['Passing', 'Rushing', 'Receiving']);
  assert.equal(b.groups[0].rows.length, 3);
  assert.deepEqual(b.groups[0].rows.map((r) => r.top), [true, false, false]);
  assert.deepEqual(b.groups[0].rows[0], { poid: 12, name: 'P. Mahomes', team_id: 492, line: '318 yds · 4 TD', top: true });
  const last = H(); last.week_best.week = 2;
  assert.equal(weekBest(last).title, "Week 2's Top Performers");
  const none = H(); none.week_best = null;
  assert.deepEqual(weekBest(none), { title: "Week 3's Top Performers", empty: 'No games played yet' });
  const missing = H(); delete missing.week_best;
  assert.deepEqual(weekBest(missing), { title: "Week 3's Top Performers", empty: 'No games played yet' });
  const blank = H(); blank.week_best.groups = blank.week_best.groups.map((x) => ({ ...x, rows: [] }));
  assert.equal(weekBest(blank).empty, 'No games played yet');
});

test('siteUrl: only numeric ids, under the league site', () => {
  const h = { league: { id: 36, week: 3 } };
  assert.equal(siteUrl('team', h, 493), 'https://nflstreetreborn.com/league/team.php?league_id=36&team_id=493');
  assert.equal(siteUrl('game', h, 2614), 'https://nflstreetreborn.com/league/stats/box.php?league_id=36&game_id=2614');
  assert.equal(siteUrl('player', h, 12), 'https://nflstreetreborn.com/league/stats/player.php?league_id=36&poid=12');
  assert.equal(siteUrl('standings', h), 'https://nflstreetreborn.com/league/standings.php?league_id=36');
  assert.equal(siteUrl('schedule', h), 'https://nflstreetreborn.com/league/schedule.php?league_id=36&week=3');
  assert.equal(siteUrl('leaders', h), 'https://nflstreetreborn.com/league/stats/leaders.php?league_id=36');
  assert.equal(siteUrl('team', h, '493&x=1'), null);
  assert.equal(siteUrl('team', h, -1), null);
  assert.equal(siteUrl('team', { league: {} }, 493), null);
  assert.equal(siteUrl('nope', h, 1), null);
  assert.equal(siteUrl('schedule', { league: { id: 36 } }), 'https://nflstreetreborn.com/league/schedule.php?league_id=36');
});

test('bannerState: a room that refused our league files offers Update even before the release check knows the build', () => {
  const s = bannerState(H(), { edition: READY, needsUpdate: true });
  assert.deepEqual(acts(s), [['install', 'Update league files', true, false]]);
  assert.deepEqual(acts(bannerState(H(), { edition: READY })).map((a) => a[0]), ['offline', 'friendly', 'play'], 'no refusal, no update');
});
