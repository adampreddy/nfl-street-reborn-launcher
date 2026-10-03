import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { matchSnapshot } from './game-snapshot.js';
import { renderStrip, cardInput } from './site/live-render.js';
import { lineScore, seriesPill } from './site/site-model.js';

// The site's own 644488b4 game as stats_game answers it (Bills 8, Eagles 36; adamknowsball and Playz).
const G = JSON.parse(readFileSync(new URL('../../../Tools/fixtures/game-644488b4.json', import.meta.url), 'utf8'));
const other = (id) => { const g = structuredClone(G); g.game.id = id; return g; };

test('one game: a one-game match, complete, best of 1, full team names, both players seated', () => {
  const { snap, totalsOnly } = matchSnapshot(G.game.id, [G]);
  assert.deepEqual(snap.match, { id: G.game.id, mode: 'casual', status: 'complete', best_of: 1 });
  assert.equal(snap.match.single, undefined, 'never the /games/<id> layout');
  assert.deepEqual(totalsOnly, []);
  assert.deepEqual(snap.seats.map((s) => s.display_name), ['adamknowsball', 'Playz']);
  const [g] = snap.games;
  assert.equal(g.game_seq, 1);
  assert.equal(g.game_id, G.game.id);
  assert.deepEqual(g.teams.map((t) => `${t.city} ${t.name}`), ['Buffalo Bills', 'Philadelphia Eagles']);
  assert.deepEqual(g.score, [8, 36]);
  assert.equal(g.plays.length, G.plays.length);
  assert.ok(g.plays.every((p) => Array.isArray(p.score_after)), 'every play has its running score');
  assert.deepEqual(g.sides, [{ seat: 0 }, { seat: 1 }]);
  const input = cardInput(snap);
  assert.deepEqual(lineScore(input).cols, ['G1'], 'the line score is G1 alone');
  assert.equal(seriesPill(input), null, 'no series banner for one game');
});

test('a match of several games: one seat per player across games, G1..Gn, a game that failed to read stays a gap', () => {
  const second = other('bbbbbbbb-2222-4333-8444-555555555555');
  const { snap } = matchSnapshot(second.game.id, [G, null, second]);
  assert.equal(snap.match.best_of, 3);
  assert.deepEqual(snap.games.map((g) => g.game_seq), [1, 3]);
  assert.equal(snap.seats.length, 2);
  assert.deepEqual(snap.games.map((g) => g.sides.map((s) => s.seat)), [[0, 1], [0, 1]]);
  assert.deepEqual(lineScore(cardInput(snap)).cols, ['G1', 'G2', 'G3']);
});

test('an unverified game is totals only: no plays, lines or drives, its number reported', () => {
  const t = structuredClone(G);
  t.game.plays_status = 'unverified';
  const { snap, totalsOnly } = matchSnapshot(t.game.id, [t]);
  assert.deepEqual(totalsOnly, [1]);
  assert.equal(snap.games[0].plays.length, 0);
  assert.equal(snap.games[0].box.lines.length, 0);
  assert.equal(snap.games[0].drives.length, 0);
  assert.deepEqual(snap.games[0].score, [8, 36], 'the captured score still shows');
});

test('nobody attributed: no seats and no sides (the sides-unknown layout); the strip still draws', () => {
  const t = structuredClone(G);
  t.game.participants = [];
  const { snap } = matchSnapshot(t.game.id, [t]);
  assert.deepEqual(snap.seats, []);
  assert.deepEqual(snap.games[0].sides, []);
  const doc = { createElement: () => { const n = { children: [], style: { setProperty() {} }, setAttribute() {}, appendChild(k) { this.children.push(k); return k; }, className: '', textContent: '' }; return n; } };
  assert.ok(renderStrip(snap, snap.games[0], doc));
});
