import { test } from 'node:test';
import assert from 'node:assert/strict';
import { seriesOverModel, drawSeriesOver } from './series-over.js';

const BILLS = { name: 'Bills', abbrev: 'BUF', logo_url: '/live-assets/r/v/teams/2-256.png' };
const EAGLES = { name: 'Eagles', abbrev: 'PHI', logo_url: '/live-assets/r/v/teams/22-256.png' };
const live = {
  seats: [{ display_name: 'Playz', avatar_url: 'https://cdn.discordapp.com/avatars/1/a.png' }, { display_name: 'Adam', avatar_url: null }],
  games: [
    { game_seq: 1, sides: [{ seat: 1 }, { seat: 0 }], teams: [BILLS, EAGLES], score: [8, 36], decided: { winner_side: 1, official: false } },
    { game_seq: 2, sides: [{ seat: 1 }, { seat: 0 }], teams: [BILLS, EAGLES], score: [6, 36], decided: { winner_side: 1, official: false } },
    { game_seq: 3, sides: [{ seat: 1 }, { seat: 0 }], teams: [BILLS, EAGLES], score: [6, 6], decided: null },
  ],
};
const ID = '644488b4-16f1-4b76-b207-20403fbaea90';
const match = (outcome, seat = 1, extra = {}) => ({ id: ID, mode: 'casual', best_of: 3, seat, opponent: 'Playz',
  relay_state: 'complete', series_live: [2, 0], series: [0, 0], outcome, ...extra });

test('a loss: who won, the final series from my side, each decided game with W/L, teams, Unofficial, the box score', () => {
  const m = seriesOverModel(match({ kind: 'win', winner_seat: 0, official: false, left_seat: null }), live);
  assert.equal(m.title, 'Series over');
  assert.equal(m.headline, 'Playz won the series');
  assert.equal(m.score, '0-2');
  assert.equal(m.winner, 'them');
  assert.deepEqual(m.games, [{ n: 1, mine: 8, theirs: 36, won: false }, { n: 2, mine: 6, theirs: 36, won: false }]);
  assert.equal(m.you.name, 'Adam');
  assert.equal(m.you.team, BILLS);
  assert.equal(m.them.team, EAGLES);
  assert.equal(m.them.avatar, 'https://cdn.discordapp.com/avatars/1/a.png');
  assert.equal(m.unofficial, true);
  assert.equal(m.mode, 'Casual · first to two wins', 'casual is a best of 3 too');
  assert.equal(m.boxHref, `match.html?match=${ID}&from=online.html`);
  const won = seriesOverModel(match({ kind: 'win', winner_seat: 0, official: true }, 0), live);
  assert.equal(won.headline, 'You won the series');
  assert.equal(won.score, '2-0');
  assert.equal(won.winner, 'you');
  assert.equal(won.games[0].won, true);
});

test('a split and a forfeit', () => {
  const s = seriesOverModel(match({ kind: 'split', winner_seat: null, official: true }, 1, { series_live: [1, 1] }), live);
  assert.equal(s.headline, 'Series split');
  assert.equal(s.score, '1-1');
  assert.equal(s.winner, null, 'nobody is marked the winner of a split');
  assert.equal(s.unofficial, false);
  const f = seriesOverModel(match({ kind: 'forfeit', winner_seat: 1, official: false, left_seat: 0 }, 1, { series_live: [1, 0] }), live);
  assert.equal(f.headline, 'You win by forfeit');
  assert.equal(f.note, 'Playz left the series');
  assert.equal(f.winner, 'you');
  assert.equal(f.games.length, 2, 'the per-game chips stay');
  const g = seriesOverModel(match({ kind: 'forfeit', winner_seat: 0, official: false, left_seat: 1 }, 1, { series_live: [0, 1] }), live);
  assert.equal(g.headline, 'Playz wins by forfeit');
  assert.equal(g.note, 'You left the series');
});

test('Elo change when the server sends it, your own name and picture when the snapshot has none, no box score without a real id', () => {
  const withElo = structuredClone(live);
  withElo.seats[1].elo_delta = 18;
  withElo.seats[0].elo_delta = -18;
  const m = seriesOverModel(match({ kind: 'win', winner_seat: 1, official: true }, 1), withElo);
  assert.deepEqual(m.you.elo, { dir: 'up', text: '▲ 18 Elo' });
  assert.deepEqual(m.them.elo, { dir: 'down', text: '▼ 18 Elo' });
  assert.equal(seriesOverModel(match({ kind: 'win', winner_seat: 1, official: true }, 1), live).you.elo, null, 'no Elo sent: none shown');
  const bare = seriesOverModel(match({ kind: 'win', winner_seat: 1, official: true }, 1, { id: 'm1', opponent_avatar_url: 'x' }), null, { name: 'adamknowsball', avatar: 'me.png' });
  assert.equal(bare.you.name, 'adamknowsball');
  assert.equal(bare.you.avatar, 'me.png');
  assert.equal(bare.them.name, 'Playz');
  assert.equal(bare.them.avatar, 'x');
  assert.deepEqual(bare.games, []);
  assert.equal(bare.boxHref, null);
  assert.equal(seriesOverModel(match({ kind: 'win', winner_seat: 1 }, 1, { mode: 'ranked' }), live).mode, 'Ranked · first to two wins');
});

test('no screen while the series is open, held or abandoned', () => {
  for (const kind of ['open', 'held', 'abandoned']) assert.equal(seriesOverModel(match({ kind }), live), null);
  assert.equal(seriesOverModel({ relay_state: 'running' }, null), null);
});

test('the screen: both faces, the winner tagged, the games, and Play again / Box score / Rematch / Back to Online', () => {
  const made = [];
  const el = (tag, attrs = {}, ...kids) => { const n = { tag, attrs, kids: kids.flat().filter((k) => k !== null && k !== undefined) }; made.push(n); return n; };
  const avatar = (url, name, cls) => el('img', { class: `avatar ${cls}`, 'data-url': url, alt: name });
  const teamLogo = (t, cls) => el('img', { class: `team-logo ${cls}`, 'data-team': t.abbrev });
  const calls = [];
  const nodes = drawSeriesOver(seriesOverModel(match({ kind: 'win', winner_seat: 1, official: false }, 1, { series_live: [0, 2] }), live),
    { el, avatar, teamLogo, onAgain: () => calls.push('again'), onBox: () => calls.push('box'), onRematch: () => calls.push('rematch'), onBack: () => calls.push('back') });
  assert.equal(nodes.length, 2, 'the card and the action row');
  const buttons = made.filter((n) => n.tag === 'button');
  assert.deepEqual(buttons.map((b) => b.attrs.text), ['Play again', 'Box score ›', 'Rematch', 'Back to Online']);
  assert.deepEqual(buttons.map((b) => b.attrs['data-key']), ['over-again', 'over-box', 'over-rematch', 'over-back']);
  assert.equal(buttons[0].attrs['data-primary'], 'true', 'A on the card goes to Play again');
  buttons.forEach((b) => b.attrs.onclick());
  assert.deepEqual(calls, ['again', 'box', 'rematch', 'back']);
  const faces = made.filter((n) => n.tag === 'div' && /\bon-p\b/.test(n.attrs.class));
  assert.deepEqual(faces.map((f) => f.attrs.class), ['on-p won', 'on-p lost']);
  assert.equal(made.filter((n) => n.attrs.class === 'on-win').length, 1, 'one Winner tag');
  assert.deepEqual(made.filter((n) => /team-logo/.test(n.attrs.class)).map((n) => n.attrs['data-team']), ['BUF', 'PHI']);
  assert.ok(made.some((n) => n.attrs.class === 'on-final num you' && n.attrs.text === '2-0'));
  assert.equal(made.filter((n) => n.tag === 'li').length, 2, 'one chip per decided game');
  assert.ok(made.some((n) => n.attrs.class === 'on-unoff'), 'Unofficial until the server confirms');
  const noBox = [];
  const el2 = (tag, attrs = {}, ...kids) => { const n = { tag, attrs, kids }; noBox.push(n); return n; };
  drawSeriesOver(seriesOverModel(match({ kind: 'win', winner_seat: 1 }, 1, { id: 'm1' }), live), { el: el2, avatar, teamLogo });
  assert.ok(!noBox.some((n) => n.attrs['data-key'] === 'over-box'), 'no Box score without a match page to open');
});

test('no rematch when the server says none is offered (Adam 10-09); an older server that says nothing still offers one', () => {
  const draw = (extra) => {
    const made = [];
    const el = (tag, attrs = {}, ...kids) => { const n = { tag, attrs, kids }; made.push(n); return n; };
    const avatar = () => el('img', {});
    const teamLogo = () => el('img', {});
    drawSeriesOver(seriesOverModel(match({ kind: 'win', winner_seat: 1 }, 1, extra), live), { el, avatar, teamLogo });
    return made.filter((n) => n.tag === 'button').map((b) => b.attrs['data-key']);
  };
  assert.deepEqual(draw({ rematch_available: false }), ['over-again', 'over-box', 'over-back']);
  assert.ok(draw({ rematch_available: true }).includes('over-rematch'));
  assert.ok(draw({}).includes('over-rematch'), 'absent: keep showing it');
  assert.equal(seriesOverModel(match({ kind: 'win', winner_seat: 1 }, 1, { rematch_available: false }), live).rematch, false);
});
