import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  pageMode, headerLine, bannerLine, teamColor, weekTitle, cardState, installState, playGate, isUpdateRequired, updateRequiredLine, friendlyRuleset,
  scoreCells, standingsRows, recentRows, leaderCols, rosterRows, ordinal,
} from './league-model.js';

const FIX = JSON.parse(readFileSync(new URL('../../../Tools/fixtures/league-home.json', import.meta.url), 'utf8'));
const ED = JSON.parse(readFileSync(new URL('../../../Tools/fixtures/league-edition-status.json', import.meta.url), 'utf8'));
const clone = () => structuredClone(FIX);
const withMatchup = (patch) => { const h = clone(); h.matchup = { ...h.matchup, ...patch }; return h; };
const OPP = '480510938571735040';
const ROOM = { room_id: 'r1', code: 'K7QP2M', members: 1, expires_at: null, state: 'waiting' };
const MINE = { ...ROOM, host_discord_user_id: '111' };      // I host (the host is not the opponent)
const THEIRS = { ...ROOM, host_discord_user_id: OPP };     // the opponent hosts

test('pageMode: signed out, non-member redirects, member', () => {
  assert.equal(pageMode({ signed_in: false }), 'signed_out');
  assert.equal(pageMode({ ok: true, signed_in: true, member: false }), 'redirect');
  assert.equal(pageMode(null), 'redirect');
  assert.equal(pageMode(FIX), 'member');
});

test('ordinal, header line (week_label wins), banner only when stale', () => {
  assert.deepEqual([1, 2, 3, 4, 11, 12, 13, 21, 22].map(ordinal), ['1st', '2nd', '3rd', '4th', '11th', '12th', '13th', '21st', '22nd']);
  assert.equal(headerLine(FIX), 'Season 4 · Week 6 · 4-1 · 2nd');
  const playoff = clone(); playoff.league.week_label = 'Wild Card'; playoff.team.standing = null; playoff.league.season = null;
  assert.equal(headerLine(playoff), 'Wild Card · 4-1');
  assert.equal(bannerLine(FIX), null);
  assert.equal(bannerLine({ ...FIX, stale: true }), 'League data unavailable. Showing what was last loaded.');
});

test('teamColor: only #RRGGBB passes', () => {
  assert.equal(teamColor(FIX.team), '#00A3FF');
  for (const bad of ['red', '#FFF', 'url(x)', '#00A3FF;x:y', null, 7]) assert.equal(teamColor({ color: bad }), null, String(bad));
});

test('weekTitle: the league week, or the matchup week when a postponed game is still open', () => {
  assert.equal(weekTitle(FIX), 'This week · Week 6');
  const late = withMatchup({ week: 5 });
  assert.equal(weekTitle(late), 'Catch-up game · Week 5');
  const noLabel = clone(); noLabel.league.week_label = ''; assert.equal(weekTitle(noLabel), 'This week · Week 6');
});

test('cardState: play, waiting, join, lobby, final, bye', () => {
  const p = cardState(FIX);
  assert.deepEqual([p.kind, p.action, p.line], ['play', 'Play league game', '@BLONDE is online']);

  const waiting = cardState(withMatchup({ room: MINE }));
  assert.deepEqual([waiting.kind, waiting.line, waiting.action, waiting.cancel], ['waiting', 'Waiting for @BLONDE…', 'Open room', true]);

  const join = cardState(withMatchup({ room: THEIRS }));
  assert.deepEqual([join.kind, join.action, join.cancel, join.room.code], ['join', "Join @BLONDE's game", false, 'K7QP2M']);

  const hostLobby = cardState(withMatchup({ room: { ...MINE, state: 'lobby', members: 2 } }));
  assert.deepEqual([hostLobby.kind, hostLobby.line, hostLobby.action, hostLobby.cancel], ['lobby', '@BLONDE joined', 'Open room', true]);
  const guestLobby = cardState(withMatchup({ room: { ...THEIRS, state: 'lobby', members: 2 } }));
  assert.deepEqual([guestLobby.kind, guestLobby.line, guestLobby.action, guestLobby.cancel], ['lobby', "You're in @BLONDE's room", 'Open room', false]);

  const won = cardState(withMatchup({ status: 'final', series: { home: 2, away: 1 }, winner_team_id: 493 }));
  assert.deepEqual([won.kind, won.line, won.action], ['final', 'Final · Won the series 2-1', null]);
  const lost = cardState(withMatchup({ status: 'final', home: false, series: { home: 2, away: 0 }, winner_team_id: 486 }));
  assert.equal(lost.line, 'Final · Lost the series 0-2');
  assert.equal(cardState(withMatchup({ status: 'final', series: null, winner_team_id: null })).line, 'Final');

  assert.deepEqual([cardState(withMatchup({ status: 'bye', opponent: null })).kind, cardState(withMatchup({ status: 'bye' })).line], ['bye', 'No game this week']);
  const none = clone(); none.matchup = null;
  assert.deepEqual([cardState(none).kind, cardState(none).action], ['bye', null]);
});

test('cardState: forfeits read as forfeits', () => {
  const ffWon = cardState(withMatchup({ status: 'final', forfeit: true, series: null, score: null, winner_team_id: 493 }));
  assert.deepEqual([ffWon.kind, ffWon.line, ffWon.action], ['final', 'Final · Won by forfeit', null]);
  const ffLost = cardState(withMatchup({ status: 'final', forfeit: true, series: null, winner_team_id: 486 }));
  assert.equal(ffLost.line, 'Final · Lost by forfeit');
  assert.equal(cardState(withMatchup({ status: 'final', forfeit: true, series: null, winner_team_id: null })).line, 'Final · Forfeit');
});

test('cardState: no schedule yet is not a bye', () => {
  const h = clone(); h.schedule_published = false; h.matchup = { ...h.matchup, status: 'bye', fixture_id: null, opponent: null, home: null };
  const s = cardState(h);
  assert.deepEqual([s.kind, s.line, s.action], ['unscheduled', 'Schedule not published yet', null]);
  const old = clone(); delete old.schedule_published; old.matchup = { ...old.matchup, status: 'bye', opponent: null };
  assert.equal(cardState(old).kind, 'bye', 'an older backend without the flag still reads as a bye');
});

test('cardState: in_progress keeps the series score and still plays game 2', () => {
  const s = cardState(withMatchup({ status: 'in_progress', series: { home: 1, away: 0 }, winner_team_id: 493 }));
  assert.deepEqual([s.kind, s.line, s.action], ['play', 'Series · You lead 1-0', 'Play league game'], 'winner_team_id mid-series is only the leader');
  assert.equal(cardState(withMatchup({ status: 'in_progress', home: false, series: { home: 1, away: 0 } })).line, 'Series · You trail 0-1');
  const j = cardState(withMatchup({ status: 'in_progress', series: { home: 1, away: 1 }, room: THEIRS }));
  assert.deepEqual([j.kind, j.line], ['join', 'Series · Tied 1-1 · @BLONDE is waiting']);
});

test('cardState: live (backend merge) or a room already in its series', () => {
  const l = cardState(withMatchup({ status: 'live', series: { home: 1, away: 0 } }));
  assert.deepEqual([l.kind, l.line, l.action], ['live', 'LIVE · You lead 1-0', 'Open room']);
  assert.equal(cardState(withMatchup({ room: { room_id: null, code: null, host_discord_user_id: OPP, members: 2, expires_at: null, state: 'in_series' } })).kind, 'live');
  assert.equal(cardState(withMatchup({ status: 'live', series: null })).line, 'LIVE');
});

test('cardState: an opponent without Discord gets no button', () => {
  const h = clone(); h.matchup.opponent.discord_user_id = null;
  const s = cardState(h);
  assert.deepEqual([s.kind, s.line, s.action], ['unlinked', "Opponent hasn't linked Discord", null]);
});

test('cardState: a room with no code is no room; names fall back', () => {
  assert.equal(cardState(withMatchup({ room: { room_id: 'r1', host_discord_user_id: OPP, state: 'waiting' } })).kind, 'play');
  const h = withMatchup({ room: THEIRS });
  h.matchup.opponent.owner_display = '';
  assert.equal(cardState(h).action, "Join @Football Furies's game");
});

test('installState: checking, install, update, progress, current', () => {
  assert.deepEqual(installState(null, null), { label: 'Checking league files…', disabled: true, primary: false, percent: null });
  assert.deepEqual(installState({ ...ED, installed: false, version: null }, null), { label: 'Install league files (1.5 GB)', disabled: false, primary: true, percent: null });
  assert.deepEqual(installState({ ...ED, latest: '2026.10.09', update_available: true }, null), { label: 'Update league files (1.5 GB)', disabled: false, primary: true, percent: null });
  assert.deepEqual(installState(ED, { phase: 'download', done: 512, total: 2048 }), { label: 'Downloading league files… 25%', disabled: true, primary: false, percent: 25 });
  assert.deepEqual(installState(ED, { phase: 'verify', done: 0, total: 0 }), { label: 'Checking downloaded files…', disabled: true, primary: false, percent: null });
  assert.deepEqual(installState(ED, { phase: 'compose', done: 3, total: 4 }), { label: 'Setting up textures… 75%', disabled: true, primary: false, percent: 75 });
  assert.equal(installState(ED, null), null, 'installed and current: no button');
  assert.equal(installState({ ...ED, size_bytes: 0, installed: false }, null).label, 'Install league files');
});

test('playGate: buttons only when installed, current and idle', () => {
  assert.deepEqual(playGate(ED, null), { ready: true, hint: null });
  assert.deepEqual(playGate(null, null), { ready: false, hint: 'Checking league files…' });
  assert.deepEqual(playGate({ ...ED, installed: false }, null), { ready: false, hint: 'Install league files to play.' });
  assert.deepEqual(playGate({ ...ED, update_available: true }, null), { ready: false, hint: 'Update league files to play.' });
  assert.deepEqual(playGate(ED, { phase: 'download', done: 1, total: 2 }), { ready: false, hint: 'League files are installing…' });
});

test('isUpdateRequired and friendlyRuleset', () => {
  assert.equal(isUpdateRequired({ kind: 'league_update_required', message: 'x' }), true);
  assert.equal(isUpdateRequired({ kind: 'unavailable', message: 'league_version_mismatch' }), true, 'a raw code from the existing join path');
  assert.equal(isUpdateRequired('league_version_mismatch'), true);
  assert.equal(isUpdateRequired({ kind: 'unavailable', message: 'nope' }), false);
  assert.equal(isUpdateRequired(null), false);
  assert.equal(friendlyRuleset(true), 'league');
  assert.equal(friendlyRuleset(false), 'raw');
});

test('scoreCells: mine first, never a time, series and forfeit lines, winners only when final', () => {
  const cells = scoreCells(FIX);
  assert.deepEqual(cells.map((c) => c.key), [1234, 1230, 1231, 1232]);
  const [mine, fin, prog, ff] = cells;
  assert.deepEqual([mine.mine, mine.status, mine.statusText, mine.home.score], [true, 'scheduled', 'This week', null]);
  assert.deepEqual([fin.status, fin.statusText, fin.home.score, fin.away.score, fin.home.winner, fin.away.winner], ['final', 'FINAL · 0-2', 49, 21, true, false]);
  assert.deepEqual([prog.status, prog.statusText, prog.home.winner, prog.away.winner], ['in_progress', 'SERIES · 1-0', false, false], 'the leader is not a winner');
  assert.deepEqual([ff.statusText, ff.home.winner, ff.away.winner, ff.home.score], ['FORFEIT', true, false, null]);
  assert.equal(mine.home.color, '#00A3FF');
  assert.equal(fin.home.logo_url, null);
});

test('scoreCells: live text, junk rows dropped, no week_games is []', () => {
  const h = clone(); h.week_games[3].status = 'live'; h.week_games[3].series = { home: 1, away: 0 }; h.week_games.push(null, { fixture_id: 9 });
  const cells = scoreCells(h);
  assert.deepEqual([cells[0].status, cells[0].statusText], ['live', 'LIVE · 0-1']);
  assert.equal(cells.length, 4);
  assert.deepEqual(scoreCells({ member: true }), []);
});

test('rows: standings mark mine; recent with forfeits; TD leaders; roster', () => {
  assert.deepEqual(standingsRows(FIX).map((r) => [r.rank, r.abbrev, r.record, r.mine]), [[1, 'GHO', '5-0', false], [2, 'ADAM', '4-1', true], [3, 'BLONDE', '3-2', false]]);
  assert.deepEqual(recentRows(FIX), [
    { result: 'W', text: 'W 28-21 @ JOCK · Week 5' },
    { result: 'L', text: 'L 0-2 vs GHO · Week 4' },
    { result: null, text: 'Forfeit vs RAIL · Week 3' },
    { result: 'W', text: 'W by forfeit @ DUNE · Week 2' },
  ]);
  assert.deepEqual(leaderCols(FIX).map((c) => [c.label, c.rows[0]?.name ?? null, c.rows[0]?.value ?? null]),
    [['Passing TD', 'Michael Vick', '14 TD'], ['Rushing TD', 'Barry Sanders', '1,204 TD'], ['Receiving TD', null, null]]);
  const bare = clone(); bare.leaders_unit = undefined;
  assert.equal(leaderCols(bare)[0].rows[0].value, '14', 'no unit from the server: the bare number');
  assert.deepEqual(rosterRows(FIX)[0], { key: 12, number: '7', name: 'Michael Vick', position: 'QB' });
  assert.deepEqual([recentRows({}), leaderCols({}).map((c) => c.rows.length), rosterRows({})], [[], [0, 0, 0], []]);
});

test('updateRequiredLine: our files are behind unless the room wants an older version', () => {
  const err = (required) => ({ kind: 'league_update_required', message: 'x', required });
  assert.equal(updateRequiredLine(err('2026.10.09'), ED), 'Update league files to play.');
  assert.equal(updateRequiredLine(err(undefined), ED), 'Update league files to play.', 'no required: the update sentence');
  assert.equal(updateRequiredLine(err(null), ED), 'Update league files to play.');
  assert.equal(updateRequiredLine(err('2026.10.07'), ED), 'Your opponent needs to update their league files.');
  assert.equal(updateRequiredLine(err('2026.9.30'), ED), 'Your opponent needs to update their league files.', 'numeric, not string, compare');
  assert.equal(updateRequiredLine(err('2026.10.08'), ED), 'Update league files to play.');
  assert.equal(updateRequiredLine(err('2026.10.07'), { ...ED, installed: false, version: null }), 'Update league files to play.');
  assert.equal(updateRequiredLine('league_version_mismatch', ED), 'Update league files to play.');
  assert.equal(updateRequiredLine({ kind: 'unavailable', message: 'nope' }, ED), null);
});

test('fixture edition game ID is the league ID', () => {
  assert.equal(ED.game_id, 'NSRS04');
});
