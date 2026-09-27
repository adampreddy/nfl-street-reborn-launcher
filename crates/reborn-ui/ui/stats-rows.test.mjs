import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rowModel, queueLine, unsentCount, participantSeats, snapshotOf, webLinkShown } from './stats-rows.js';

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
