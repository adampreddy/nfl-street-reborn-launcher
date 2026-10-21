// League game day (spec docs/superpowers/specs/2026-10-21-league-game-day-design.md): the season stat line, the lineup
// order, the tale of the tape, and when the view applies. Real Season 4 names (Week 1, Subway Surfers v Wolves Reign).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { statLine, lineup, tape, gameDay, TAPE_LABELS } from './league-gameday-model.js';

const Z = { gp: 0, pass_yds: 0, pass_td: 0, thrown_int: 0, rush_yds: 0, rush_td: 0, rec_yds: 0, rec_td: 0, tackle: 0, sack: 0, int: 0, def_td: 0 };
const S = (o) => ({ ...Z, ...o });
const TS0 = { gp: 0, pts: 0, pts_allowed: 0, yds: 0, pass_yds: 0, rush_yds: 0, turnovers: 0, sacks: 0, def_td: 0 };
const TS = (o) => ({ ...TS0, ...o });
const P = (poid, name, position, number, season = Z, portrait_url = null) => ({ poid, name, position, number, season, portrait_url });

test('statLine: the first group a player has, non-zero parts only, season totals', () => {
  assert.equal(statLine(S({ pass_yds: 482, pass_td: 5, thrown_int: 1, rush_yds: 30 }), 'QB'), '482 yds · 5 TD · 1 INT');
  assert.equal(statLine(S({ pass_yds: 241 }), 'QB'), '241 yds');
  assert.equal(statLine(S({ rush_yds: 242, rush_td: 3 }), 'RB'), '242 yds · 3 TD');
  assert.equal(statLine(S({ rec_yds: 1204, rec_td: 5 }), 'WR'), '1,204 yds · 5 TD');
  assert.equal(statLine(S({ sack: 3, tackle: 8 }), 'DL'), '3 sk · 8 tkl');
  assert.equal(statLine(S({ int: 1, tackle: 10 }), 'DB'), '1 INT · 10 tkl');
  assert.equal(statLine(S({ def_td: 1, tackle: 2 }), 'LB'), '2 tkl');
});

test('statLine: nothing recorded is blank, an OL says no stats are kept, a missing season is blank', () => {
  assert.equal(statLine(Z, 'OL'), 'No stats kept');
  assert.equal(statLine(Z, 'DB'), '');
  assert.equal(statLine(undefined, 'QB'), '');
});

test('lineup: QB, RB, WR, OL, DL, LB, DB, ties by name, with the line and the portrait', () => {
  const rows = lineup([P(1, 'Zed', 'DB', 2), P(2, 'Purdy', 'QB', 13), P(3, 'Dawkins', 'OL', 73), P(4, 'Egbuka', 'WR', 2),
    P(5, 'Robinson', 'RB', 7), P(6, 'Oluokun', 'LB', 23), P(7, 'Jones', 'DL', 95), P(8, 'Adams', 'DB', 24, Z, 'https://api.test/live-assets/league/portrait/8.png')], { blank: false });
  assert.deepEqual(rows.map((r) => r.name), ['Purdy', 'Robinson', 'Egbuka', 'Dawkins', 'Jones', 'Oluokun', 'Adams', 'Zed']);
  assert.equal(rows[3].line, 'No stats kept');
  assert.equal(rows[6].portraitUrl, 'https://api.test/live-assets/league/portrait/8.png');
  assert.equal(rows[0].pos, 'QB'); assert.equal(rows[0].number, 13);
  assert.deepEqual(lineup(null, { blank: false }), []);
});

test('lineup: Week 1 blanks every line, OL included', () => {
  const rows = lineup([P(3, 'Dawkins', 'OL', 73), P(2, 'Purdy', 'QB', 13, S({ pass_yds: 10 }))], { blank: true });
  assert.deepEqual(rows.map((r) => r.line), ['', '']);
});

test('tape: Week 1 (neither side played) is null; one side playing is enough; 8 rows in order', () => {
  assert.equal(tape(TS0, TS0), null);
  assert.equal(tape(undefined, undefined), null);
  const rows = tape(TS({ gp: 2, pts: 63 }), TS0);
  assert.equal(rows.length, 8);
  assert.deepEqual(rows.map((r) => r.label), TAPE_LABELS);
  assert.deepEqual(TAPE_LABELS, ['Points', 'Points allowed', 'Total yards', 'Passing yards', 'Rushing yards', 'Turnovers', 'Sacks', 'Defensive TDs']);
  assert.equal(rows[0].theirs, 0, 'a team that has not played reads 0, not blank');
});

test('tape: higher is better, except Points allowed and Turnovers; a tie favours nobody; bar widths scale to the larger', () => {
  const rows = tape(TS({ gp: 2, pts: 63, pts_allowed: 45, yds: 824, turnovers: 3, sacks: 6 }), TS({ gp: 2, pts: 54, pts_allowed: 48, yds: 824, turnovers: 5, sacks: 4 }));
  const by = Object.fromEntries(rows.map((r) => [r.label, r]));
  assert.equal(by.Points.mineBetter, true); assert.equal(by.Points.theirsBetter, false);
  assert.equal(by.Points.mineWidth, 100); assert.equal(Math.round(by.Points.theirsWidth), 86);
  assert.equal(by['Points allowed'].mineBetter, true);
  assert.equal(by.Turnovers.mineBetter, true); assert.equal(by.Turnovers.theirsBetter, false);
  assert.equal(by['Total yards'].mineBetter, false); assert.equal(by['Total yards'].theirsBetter, false);
  assert.equal(by['Defensive TDs'].mineWidth, 0, 'zero against zero draws no bar');
});

const HOME = () => ({
  ok: true, member: true, league: { id: 36, week: 1 },
  team: { id: 493, name: 'Subway Surfers', abbr: 'ADAM', color: '#2563A4', record: '0-0', logo_url: 'x' },
  team_season: TS0,
  roster: [P(11, 'Brock Purdy', 'QB', 13), P(50, 'Dion Dawkins', 'OL', 73)],
  matchup: { fixture_id: 2614, week: 1, home: true, status: 'scheduled',
    opponent: { team_id: 484, name: 'Wolves Reign', abbr: 'SNIPER', owner_display: 'SNIPER', color: '#D15619', record: '0-0', logo_url: 'y',
      roster: [P(361, 'Lamar Jackson', 'QB', 8)], team_season: TS0 } },
});
const SNAP = (room = { id: 'r1', code: 'WHRF58', league: { fixture_id: 2614, week: 1 }, members: [] }) => ({ room, match: null, chat: [] });

test('gameDay: only this fixture\'s room; the title names away at home', () => {
  assert.equal(gameDay(HOME(), SNAP({ id: 'r9', league: { fixture_id: 9999, week: 1 } })), null, 'another fixture');
  assert.equal(gameDay(HOME(), SNAP({ id: 'r9', edition: 'league', league: null })), null, 'a friendly');
  assert.equal(gameDay(HOME(), { room: null, match: null }), null, 'no room');
  const g = gameDay(HOME(), SNAP());
  assert.equal(g.title, 'Week 1 · Wolves Reign at Subway Surfers');
  const away = HOME(); away.matchup.home = false;
  assert.equal(gameDay(away, SNAP()).title, 'Week 1 · Subway Surfers at Wolves Reign');
  assert.equal(g.me.name, 'Subway Surfers'); assert.equal(g.me.owner, 'ADAM'); assert.equal(g.me.color, '#2563A4');
  assert.equal(g.opp.name, 'Wolves Reign'); assert.equal(g.opp.owner, 'SNIPER');
  assert.equal(g.week, 1);
});

test('gameDay: Week 1 is blank lines and no tape; a running match of this fixture keeps the view', () => {
  const g = gameDay(HOME(), SNAP());
  assert.equal(g.tape, null);
  assert.deepEqual(g.me.lineup.map((r) => r.line), ['', '']);
  const m = gameDay(HOME(), { room: null, match: { id: 'm1', league: { fixture_id: 2614 }, relay_state: 'running', series: [1, 0] }, chat: [] });
  assert.equal(m?.match?.id, 'm1');
});

test('gameDay: an older site without the opponent lineup; stale data is marked', () => {
  const h = HOME(); delete h.matchup.opponent.roster; delete h.matchup.opponent.team_season; h.stale = true;
  const g = gameDay(h, SNAP());
  assert.equal(g.oppMissing, true); assert.deepEqual(g.opp.lineup, []);
  assert.equal(g.tape, null);
  assert.equal(g.stale, true);
});

test('matchState: the words and whether the series is over (final review: a cancelled match is not "Starting…")', async () => {
  const { matchState } = await import('./league-gameday-model.js');
  assert.deepEqual(matchState({ relay_state: 'room_formed', series: [0, 0] }), { words: 'Starting…', done: false });
  assert.deepEqual(matchState({ relay_state: 'running', series: [1, 0] }), { words: 'Game 2 of 3', done: false });
  assert.deepEqual(matchState({ relay_state: 'intermission', series: [1, 1] }), { words: 'Game 3 of 3', done: false });
  assert.deepEqual(matchState({ relay_state: 'complete', series: [2, 1] }), { words: 'Final', done: true });
  assert.deepEqual(matchState({ relay_state: 'void', series: [0, 0] }), { words: 'Match cancelled', done: true });
  assert.deepEqual(matchState({ relay_state: 'abandoned', series: [0, 0] }), { words: 'Match cancelled', done: true });
  assert.deepEqual(matchState({ relay_state: 'held', series: [1, 0] }), { words: 'Held for review', done: true });
});

test('gameDayKey: presence and queue counts never redraw; the room, the match and the chat do (final review)', async () => {
  const { gameDayKey } = await import('./league-gameday-model.js');
  const base = { room: { id: 'r1', members: [{ name: 'Adam', ready: false }] }, match: null, chat: [], online: { count: 3, players: [] }, queue_depth: { ranked: 0 } };
  const k = gameDayKey(base);
  assert.equal(gameDayKey({ ...base, online: { count: 4, players: [{ name: 'Tuna', activity: 'queued' }] }, queue_depth: { ranked: 2 } }), k);
  assert.notEqual(gameDayKey({ ...base, room: { ...base.room, members: [{ name: 'Adam', ready: true }] } }), k);
  assert.notEqual(gameDayKey({ ...base, chat: [{ seq: 1, text: 'yo' }] }), k);
  assert.notEqual(gameDayKey({ ...base, room: null, match: { id: 'm1', relay_state: 'running' } }), k);
  assert.equal(gameDayKey(null), gameDayKey(undefined));
});
