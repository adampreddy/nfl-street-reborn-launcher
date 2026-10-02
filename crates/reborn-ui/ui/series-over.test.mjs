import { test } from 'node:test';
import assert from 'node:assert/strict';
import { seriesOverModel, drawSeriesOver } from './series-over.js';

const live = {
  seats: [{ display_name: 'Playz' }, { display_name: 'Adam' }],
  games: [
    { game_seq: 1, sides: [{ seat: 1 }, { seat: 0 }], score: [8, 36], decided: { winner_side: 1, official: false } },
    { game_seq: 2, sides: [{ seat: 1 }, { seat: 0 }], score: [6, 36], decided: { winner_side: 1, official: false } },
    { game_seq: 3, sides: [{ seat: 1 }, { seat: 0 }], score: [6, 6], decided: null },
  ],
};
const match = (outcome, seat = 1, extra = {}) => ({ id: '644488b4-16f1-4b76-b207-20403fbaea90', mode: 'casual', seat, opponent: 'Playz',
  relay_state: 'complete', series_live: [2, 0], series: [0, 0], outcome, ...extra });

test('a win: who won, the series, each decided game from my side, Unofficial, the full-game link', () => {
  const m = seriesOverModel(match({ kind: 'win', winner_seat: 0, official: false, left_seat: null }), live);
  assert.equal(m.title, 'Series over');
  assert.equal(m.headline, 'Playz won 2-0');
  assert.deepEqual(m.games, [{ n: 1, mine: 8, theirs: 36 }, { n: 2, mine: 6, theirs: 36 }]);
  assert.equal(m.unofficial, true);
  assert.equal(m.fullUrl, 'https://online.nflstreetreborn.com/live/644488b4-16f1-4b76-b207-20403fbaea90');
  assert.equal(seriesOverModel(match({ kind: 'win', winner_seat: 0, official: true }, 0), live).headline, 'You won 2-0');
});

test('a split and a forfeit', () => {
  const s = seriesOverModel(match({ kind: 'split', winner_seat: null, official: true }, 1, { series_live: [1, 1] }), live);
  assert.equal(s.headline, 'Split 1-1');
  assert.equal(s.unofficial, false);
  const f = seriesOverModel(match({ kind: 'forfeit', winner_seat: 1, official: false, left_seat: 0 }, 1, { series_live: [1, 0] }), live);
  assert.equal(f.headline, 'You won by forfeit', 'a forfeit says so, with no game score in the headline');
  assert.equal(f.note, 'Playz left the series');
  assert.deepEqual(f.games, [{ n: 1, mine: 8, theirs: 36 }, { n: 2, mine: 6, theirs: 36 }], 'the per-game lines stay');
  const g = seriesOverModel(match({ kind: 'forfeit', winner_seat: 0, official: false, left_seat: 1 }, 1, { series_live: [0, 1] }), live);
  assert.equal(g.headline, 'Playz won by forfeit');
  assert.equal(g.note, 'You left the series');
});

test('no screen while the series is open, held or abandoned', () => {
  for (const kind of ['open', 'held', 'abandoned']) assert.equal(seriesOverModel(match({ kind }), live), null);
  assert.equal(seriesOverModel({ relay_state: 'running' }, null), null);
});

test('the screen has Full game, Find another game and Rematch, and only the page\'s existing classes', () => {
  const made = [];
  const el = (tag, attrs = {}, ...kids) => { const n = { tag, attrs, kids }; made.push(n); return n; };
  const calls = [];
  const nodes = drawSeriesOver(seriesOverModel(match({ kind: 'win', winner_seat: 0, official: false }), live),
    { el, onFind: () => calls.push('find'), onRematch: () => calls.push('rematch'), onFull: () => calls.push('full') });
  const buttons = made.filter((n) => n.tag === 'button');
  assert.deepEqual(buttons.map((b) => b.attrs.text), ['Full game ↗', 'Find another game', 'Rematch']);
  buttons.forEach((b) => b.attrs.onclick());
  assert.deepEqual(calls, ['full', 'find', 'rematch']);
  const classes = new Set(made.flatMap((n) => String(n.attrs.class ?? '').split(' ').filter(Boolean)));
  for (const c of classes) assert.ok(['mm-state', 'note', 'mm-series', 'mm-actions', 'primary'].includes(c), `unexpected class ${c}`);
  assert.ok(nodes.length >= 4);
});
