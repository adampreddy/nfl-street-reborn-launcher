import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rowModel, queueLine, unsentCount } from './stats-rows.js';

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
