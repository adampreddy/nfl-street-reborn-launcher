import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rowModel, queueLine, unsentCount, participantSeats, snapshotOf, sideTeams, webLinkShown } from './stats-rows.js';

test('rowModel turns a games-list entry into what a row draws', () => {
  const r = rowModel({ id: 'g1', mode: 'online', status: 'accepted', captured_at: '2026-09-27T01:41:00Z',
    teams: [{ name: 'Eagles', score: 36 }, { name: 'Falcons', score: 6 }], team_index: 1, score: [36, 6], result: 'loss', plays_status: 'verified' });
  assert.equal(r.id, 'g1');
  assert.equal(r.home.name, 'Eagles');
  assert.deepEqual(r.score, [36, 6]);
  assert.equal(r.mine, 1);
  assert.equal(r.result, 'L');
  assert.equal(r.badge, 'Verified');
  assert.equal(r.mode, 'Online');
});

test('rowModel: totals-only, held, and missing pieces', () => {
  assert.equal(rowModel({ id: 'a', mode: 'offline', status: 'accepted', teams: [{ name: 'A' }, { name: 'B' }], plays_status: 'absent' }).badge, 'Totals only');
  assert.equal(rowModel({ id: 'b', mode: 'offline', status: 'accepted', teams: [{ name: 'A' }, { name: 'B' }], plays_status: 'failed' }).badge, 'Totals only');
  const held = rowModel({ id: 'c', mode: 'online', status: 'held', hold_reason: 'left-before-start', teams: [{ name: 'A' }, { name: 'B' }], plays_status: 'absent' });
  assert.equal(held.badge, 'Held for review');
  assert.equal(held.result, null);
  const bare = rowModel({ id: 'd', mode: 'offline', status: 'accepted', teams: null, plays_status: 'absent' });
  assert.equal(bare.home.name, 'Side 1');
  assert.equal(bare.score, null);
  assert.equal(bare.mode, 'Offline');
});

test('rowModel carries the logo path the server gave as data, never builds one', () => {
  const r = rowModel({ id: 'e', teams: [{ name: 'A', logo_url: '/live-assets/v1/teams/3-256.png' }, { name: 'B', logo_url: 'https://evil.example/x.png' }] });
  assert.equal(r.home.logo, '/live-assets/v1/teams/3-256.png');
  assert.equal(r.away.logo, 'https://evil.example/x.png'); // kept as data; assetUrl() refuses it when drawn
  assert.equal(rowModel({ id: 'f', teams: [{ name: 'A' }, { name: 'B' }] }).home.logo, null);
});

test('queueLine counts what has not uploaded, in the spec\'s words', () => {
  assert.equal(queueLine([]), null);
  assert.equal(queueLine(undefined), null);
  assert.equal(queueLine([{ state: 'queued' }]), '1 game not uploaded yet');
  assert.equal(queueLine([{ state: 'queued' }, { state: 'stalled' }, { state: 'accepted' }]), '2 games not uploaded yet');
  assert.equal(queueLine([{ state: 'accepted' }, { state: 'held' }, { state: 'rejected' }, { state: 'quarantined' }]), null);
});

test('queueLine carries the queue reason from the first unsent entry that has one', () => {
  assert.equal(queueLine([{ state: 'queued' }, { state: 'stalled', reason: 'The server could not be reached.' }]),
    '2 games not uploaded yet: The server could not be reached.');
  assert.equal(queueLine([{ state: 'stalled', reason: '  ' }]), '1 game not uploaded yet');
  assert.equal(queueLine([{ state: 'held', reason: 'review' }, { state: 'queued' }]), '1 game not uploaded yet'); // a held entry's reason is not an upload problem
  assert.equal(unsentCount([{ state: 'queued' }, { state: 'stalled' }, { state: 'accepted' }]), 2);
  assert.equal(unsentCount(null), 0);
});

test('snapshotOf seats the human participants on their sides; CPU and guests stay seatless', () => {
  const avatar = 'https://cdn.discordapp.com/avatars/1001/0123456789abcdef0123456789abcdef.png?size=64';
  const snap = snapshotOf({ game: { id: 'g', mode: 'offline', teams: [{ name: 'Eagles', score: 36 }, { name: 'Falcons', score: 6 }],
    participants: [
      { kind: 'cpu', pad: null, team_index: 0, player_id: null, display_name: null, avatar_url: null },
      { kind: 'guest', pad: 1, team_index: null, player_id: null, display_name: null, avatar_url: null },
      { kind: 'human', pad: 0, team_index: 1, player_id: 'p1', display_name: 'Alpha', avatar_url: avatar },
    ] }, plays: [], lines: [], drives: [] });
  assert.deepEqual(snap.seats, [{ display_name: 'Alpha', avatar_url: avatar }]);
  assert.deepEqual(snap.games[0].sides, [{ seat: null }, { seat: 0 }]);
  assert.deepEqual(snap.games[0].score, [36, 6]);

  const two = participantSeats([
    { kind: 'human', team_index: 1, player_id: 'b', display_name: 'Bravo' },
    { kind: 'human', team_index: 0, player_id: 'a', display_name: 'Alpha' },
  ]);
  assert.deepEqual(two.seats.map((s) => s.display_name), ['Alpha', 'Bravo']);
  assert.deepEqual(two.sides, [{ seat: 0 }, { seat: 1 }]);
  assert.deepEqual(participantSeats([{ kind: 'human', team_index: null, player_id: 'a' }]), { seats: [], sides: [] });
  assert.deepEqual(participantSeats(undefined), { seats: [], sides: [] });
});

test('snapshotOf takes logos and colours from teams_view, and tolerates an older server without it', () => {
  const logo = '/live-assets/r/2026.09.23-seq-7.abc/teams/13-256.png';
  const game = { id: 'g', teams: [{ name: 'Eagles', score: 7 }, { name: 'Customs', score: 3 }],
    teams_view: [{ side: 0, tgid: 13, name: 'Eagles', abbrev: 'PHI', primary: '#032E2F', secondary: '#878787', logo_url: logo },
      { side: 1, tgid: null, name: null, abbrev: null, primary: null, secondary: null, logo_url: null }] };
  const t = snapshotOf({ game }).games[0].teams;
  assert.equal(t[0].logo_url, logo);
  assert.equal(t[0].primary, '#032E2F');
  assert.equal(t[0].tgid, 13);
  assert.equal(t[0].name, 'Eagles');
  assert.equal(t[1].logo_url, null);
  assert.equal(t[1].name, 'Customs');
  // Plays' own teams win for the tgid.
  const withPlays = snapshotOf({ game, plays: [{ body: { teams: [{ tgid: 14 }, { tgid: 8 }] } }] }).games[0].teams;
  assert.equal(withPlays[0].tgid, 14);
  // Older server: no teams_view, no participants.
  const { teams_view, ...older } = game;
  const plain = snapshotOf({ game: older });
  assert.equal(plain.games[0].teams[0].logo_url, null);
  assert.equal(plain.games[0].teams[0].primary, null);
  assert.deepEqual(plain.seats, []);
  assert.deepEqual(plain.games[0].sides, []);
});

test('webLinkShown: only an online game offers Open on web', () => {
  assert.equal(webLinkShown({ mode: 'online' }), true);
  assert.equal(webLinkShown({ mode: 'offline' }), false);
  assert.equal(webLinkShown({}), false);
  assert.equal(webLinkShown(null), false);
});

// ---- side order: a finished game's capture order is not its play side order (game a51767ab, 2026-09-27) ----

// The live game: capture teams [Eagles 36, Falcons 14]; plays side 0 = tgid 14 Falcons, side 1 = tgid 13 Eagles; the
// fork's side_to_team was the swap, so every capture player carries side AND team_index.
const swapped = () => ({
  id: 'a51767ab-9bb0-42b0-89ab-f10902065a1f', mode: 'online', plays_status: 'verified',
  teams: [{ name: 'Eagles', score: 36 }, { name: 'Falcons', score: 14 }],
  players: [
    { name: 'Michael Penix Jr.', team: 'Falcons', side: 0, team_index: 1, digp: 16 },
    { name: 'Jalen Hurts', team: 'Eagles', side: 1, team_index: 0, digp: 36 },
  ],
  teams_view: [
    { side: 0, tgid: 14, name: 'Falcons', abbrev: 'ATL', primary: '#141414', secondary: '#7D0D19', logo_url: '/live-assets/r/x/teams/14-256.png' },
    { side: 1, tgid: 13, name: 'Eagles', abbrev: 'PHI', primary: '#032E2F', secondary: '#878787', logo_url: '/live-assets/r/x/teams/13-256.png' },
  ],
  participants: [
    { kind: 'human', pad: 0, team_index: 1, player_id: 'adam', display_name: 'adamknowsball', avatar_url: null },
    { kind: 'human', pad: 1, team_index: 0, player_id: 'playz', display_name: 'Playz', avatar_url: null },
  ],
});
const swappedPlays = [{ body: { seq: 1, teams: [{ side: 0, tgid: 14, tmc: [1, 2, 3] }, { side: 1, tgid: 13, tmc: [4, 5, 6] }] } }];
const swappedLines = [
  { digp: 36, name: 'Jalen Hurts', team_index: 0, stats: { pass_att: 1 } },
  { digp: 16, name: 'Michael Penix Jr.', team_index: 1, stats: { pass_att: 1 } },
];

test('snapshotOf: capture [Eagles 36, Falcons 14] with side 0 = Falcons puts Falcons 14 and its player on side 0', () => {
  const game = swapped();
  assert.deepEqual(sideTeams(game, game.teams_view), { team: [1, 0], key: 'players' });
  const snap = snapshotOf({ game, plays: swappedPlays, lines: swappedLines, drives: [] });
  const g = snap.games[0];
  assert.deepEqual(g.teams.map((t) => [t.name, t.tgid, t.logo_url]),
    [['Falcons', 14, '/live-assets/r/x/teams/14-256.png'], ['Eagles', 13, '/live-assets/r/x/teams/13-256.png']]);
  assert.deepEqual(g.score, [14, 36], 'each side\'s score is its own team\'s');
  assert.deepEqual(snap.seats.map((s) => s.display_name), ['adamknowsball', 'Playz'], 'team_index 1 (Falcons) sits on side 0');
  assert.deepEqual(g.sides, [{ seat: 0 }, { seat: 1 }]);
  assert.deepEqual(g.box.lines.map((l) => [l.name, l.team_index]), [['Jalen Hurts', 1], ['Michael Penix Jr.', 0]],
    'box lines move from capture order onto sides');
  // The Stats list row stays in the server's capture order, as the server's score/result are.
  const row = rowModel({ ...game, score: [36, 14], team_index: 1, status: 'accepted', result: 'loss' });
  assert.deepEqual([row.home.name, row.away.name, row.score, row.mine, row.result], ['Eagles', 'Falcons', [36, 14], 1, 'L']);
});

test('snapshotOf: without players\' team_index the side names match the capture names; without names it keeps capture order', () => {
  const byTeam = { ...swapped(), players: [{ side: 0, team: 'Falcons' }, { side: 1, team: 'Eagles' }] };
  assert.deepEqual(sideTeams(byTeam, byTeam.teams_view), { team: [1, 0], key: 'names' });
  const byView = { ...swapped(), players: [] };
  assert.deepEqual(sideTeams(byView, byView.teams_view), { team: [1, 0], key: 'names' });
  const snap = snapshotOf({ game: byView, plays: swappedPlays, lines: swappedLines });
  assert.deepEqual(snap.games[0].teams.map((t) => t.name), ['Falcons', 'Eagles']);
  assert.deepEqual(snap.games[0].score, [14, 36]);
  assert.deepEqual(snap.seats.map((s) => s.display_name), ['adamknowsball', 'Playz']);
  const clash = { ...swapped(), players: [{ side: 0, team_index: 1, team: 'Falcons' }, { side: 0, team_index: 0 }] };
  assert.deepEqual(sideTeams(clash, clash.teams_view).team, [1, 0]);
  // An older server (no teams_view) and a capture with no players: capture order, exactly as before.
  const { teams_view, players, ...older } = swapped();
  assert.deepEqual(sideTeams(older), { team: [0, 1], key: 'order' });
  const plain = snapshotOf({ game: older, lines: swappedLines });
  assert.deepEqual(plain.games[0].teams.map((t) => t.name), ['Eagles', 'Falcons']);
  assert.deepEqual(plain.games[0].score, [36, 14]);
  assert.deepEqual(plain.seats.map((s) => s.display_name), ['Playz', 'adamknowsball']);
  assert.deepEqual(plain.games[0].box.lines.map((l) => l.team_index), [0, 1]);
});

test('snapshotOf: capture order == side order is unchanged', () => {
  const game = {
    id: 'g', teams: [{ name: 'Falcons', score: 14 }, { name: 'Eagles', score: 36 }],
    players: [{ side: 0, team_index: 0, team: 'Falcons' }, { side: 1, team_index: 1, team: 'Eagles' }],
    teams_view: swapped().teams_view,
    participants: [{ kind: 'human', pad: 0, team_index: 0, player_id: 'a', display_name: 'Alpha', avatar_url: null }],
  };
  const snap = snapshotOf({ game, plays: swappedPlays, lines: [{ digp: 1, name: 'X', team_index: 1, stats: {} }] });
  assert.deepEqual(snap.games[0].teams.map((t) => [t.name, t.tgid]), [['Falcons', 14], ['Eagles', 13]]);
  assert.deepEqual(snap.games[0].score, [14, 36]);
  assert.deepEqual(snap.games[0].sides, [{ seat: 0 }, { seat: null }]);
  assert.equal(snap.games[0].box.lines[0].team_index, 1);
});
