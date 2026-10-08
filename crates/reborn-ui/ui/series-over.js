// The post-game screen, shown on Online when a series is over (series spec 2026-10-02 §3.3; redesign 2026-10-04: "the
// screen that shows up after a game ... needs a redesign to match the rest of the launcher"). It is the matchup card
// finished: you and your opponent with your pictures and team logos, the winner marked, the final series big in the
// middle, each game's score from your side, the Elo change when the server sends one, "Unofficial" until the server
// says the result is official; under it Play again, Box score, Rematch and Back to Online.
// PURE: only online-model.js (itself pure), so node --test can load it; the page hands in `el`, `avatar` and `teamLogo`.
import { modeLine } from './online-model.js';
import { shownName } from './names.js';

const OVER = new Set(['win', 'forfeit', 'split']);
const DASH = '-'; // scores: a plain hyphen, "36-12" (owner ruling, launch audit M11)
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const int = (n) => (typeof n === 'number' && Number.isFinite(n) ? n : null);

/** "▲ 18 Elo" / "▼ 9 Elo" from a seat's (or the match's) elo_delta; null when none was sent or it is 0. */
function eloChange(d) {
  const n = int(d);
  return n ? { dir: n > 0 ? 'up' : 'down', text: `${n > 0 ? '▲' : '▼'} ${Math.abs(Math.round(n))} Elo` } : null;
}

const team = (t) => (t && typeof t === 'object' && (t.name || t.abbrev || t.logo_url) ? t : null);

/** Everything the screen says, or null when there is no finished series to show. `self`: your own name and picture as
 *  the page knows them (the matchmaking snapshot, Play's cached Discord picture), used where the live snapshot has none. */
export function seriesOverModel(match, live, self = {}) {
  const o = match?.outcome;
  if (!o || !OVER.has(o.kind)) return null;
  const me = match.seat === 1 ? 1 : 0;
  const wins = Array.isArray(match.series_live) && match.series_live.length === 2 ? match.series_live
    : Array.isArray(match.series) ? match.series : [0, 0];
  const seats = Array.isArray(live?.seats) ? live.seats : [];
  const names = [shownName(seats[0]), shownName(seats[1])];
  const you = names[me] || (typeof self.name === 'string' && self.name) || 'You';
  const them = names[1 - me] || match.opponent || 'Your opponent';
  const mine = int(wins[me]) ?? 0, theirs = int(wins[1 - me]) ?? 0;
  const winner = o.winner_seat === 0 || o.winner_seat === 1 ? o.winner_seat : null;
  const headline = o.kind === 'split' ? 'Series split'
    : o.kind === 'forfeit' ? (winner === me ? 'You win by forfeit' : `${them} wins by forfeit`)
    : winner === me ? 'You won the series' : `${them} won the series`;
  const note = o.kind === 'forfeit' ? (o.left_seat === me ? 'You left the series' : `${them} left the series`) : null;
  // Each decided game from your side: its score, whether you won it, and the team each of you had.
  let myTeam = null, theirTeam = null;
  const games = (Array.isArray(live?.games) ? live.games : []).filter((g) => g?.decided).map((g) => {
    const side = Array.isArray(g.sides) ? g.sides.findIndex((s) => s?.seat === me) : -1;
    const sc = Array.isArray(g.score) ? g.score : [0, 0];
    const mySide = side === -1 ? 0 : side;
    const teams = Array.isArray(g.teams) ? g.teams : [];
    if (side !== -1) { myTeam = team(teams[mySide]) ?? myTeam; theirTeam = team(teams[1 - mySide]) ?? theirTeam; }
    const w = g.decided.winner_side;
    return { n: g.game_seq, mine: int(sc[mySide]) ?? 0, theirs: int(sc[1 - mySide]) ?? 0,
      won: side === -1 || (w !== 0 && w !== 1) ? null : w === mySide };
  });
  const seatDelta = (s) => eloChange(seats[s]?.elo_delta);
  return {
    kind: o.kind, title: 'Series over', headline, note, games, unofficial: o.official !== true,
    score: `${mine}${DASH}${theirs}`, mine, theirs,
    winner: winner === null || o.kind === 'split' ? null : winner === me ? 'you' : 'them',
    // `id`: whose profile the name opens (owner 10-08); null when neither the live snapshot nor the match names one.
    you: { name: you, id: seats[me]?.player_id ?? self.id ?? null, avatar: seats[me]?.avatar_url ?? self.avatar ?? null, team: myTeam, elo: seatDelta(me) ?? eloChange(match.elo_delta) },
    them: { name: them, id: seats[1 - me]?.player_id ?? match.opponent_id ?? null, avatar: seats[1 - me]?.avatar_url ?? match.opponent_avatar_url ?? null, team: theirTeam ?? team(match.opponent_team), elo: seatDelta(1 - me) },
    mode: modeLine(match.mode, match.best_of),
    // The whole match on the launcher's own match page; Back there returns to Online.
    boxHref: typeof match.id === 'string' && UUID_RE.test(match.id) ? `match.html?${new URLSearchParams({ match: match.id, from: 'online.html' })}` : null,
  };
}

/** The screen's nodes: the finished matchup card and its action row. `el` is shell.js's; `avatar` and `teamLogo` are
 *  match-rows.js's (Discord pictures and catalog logos only, each with its letter / abbreviation fallback); `playerLink`
 *  (player-link.js) makes each name a link to that player's profile, and without it the names are plain. */
export function drawSeriesOver(m, { el, avatar, teamLogo, playerLink = null, onAgain, onBox, onRematch, onBack }) {
  const face = (p, who) => {
    const won = m.winner === who, lost = m.winner && !won;
    return el('div', { class: `on-p${won ? ' won' : lost ? ' lost' : ''}` },
      el('span', { class: 'on-avw' }, avatar(p.avatar, p.name, 'on-av'), won ? el('span', { class: 'on-win', text: 'Winner' }) : null),
      playerLink ? playerLink(p.id, p.name, { cls: 'on-pname', key: `over-${who}` }) : el('span', { class: 'on-pname', text: p.name }),
      p.elo ? el('span', { class: `on-psub num on-elo ${p.elo.dir}`, text: p.elo.text }) : el('span', { class: 'on-psub', text: who === 'you' && p.name !== 'You' ? 'You' : '' }),
      p.team ? teamLogo(p.team, 'on-plogo') : null);
  };
  const games = m.games.length
    ? el('ol', { class: 'on-games', 'aria-label': 'Each game' }, ...m.games.map((g) => el('li', {
      class: g.won === true ? 'w' : g.won === false ? 'l' : null,
      'aria-label': `Game ${g.n}: ${g.mine} to ${g.theirs}${g.won === true ? ', won' : g.won === false ? ', lost' : ''}` },
    el('span', { class: 'on-gn', text: `G${g.n}` }),
    el('b', { class: 'num', text: `${g.mine}${DASH}${g.theirs}` }),
    g.won === null ? null : el('span', { class: 'on-wl', text: g.won ? 'W' : 'L' }))))
    : null;
  const mid = el('div', { class: 'on-mid on-over-mid' },
    el('span', { class: 'on-over-top' }, el('span', { class: 'k-label', text: m.title }),
      m.unofficial ? el('span', { class: 'on-unoff', text: 'Unofficial' }) : null),
    el('strong', { class: `on-final num${m.winner ? ` ${m.winner}` : ''}`, 'aria-hidden': 'true', text: m.score }),
    el('span', { class: 'on-over-head', text: m.headline }),
    m.note ? el('span', { class: 'on-over-note', text: m.note }) : null,
    el('span', { class: 'on-mode-line', text: m.mode }));
  const card = el('section', { class: 'on-vs on-over', 'aria-label': `${m.title}. ${m.headline}, ${m.mine} to ${m.theirs}.` },
    el('span', { class: 'on-art', 'aria-hidden': 'true' }),
    el('div', { class: 'on-vs-in' }, face(m.you, 'you'), mid, face(m.them, 'them'), games));
  const row = el('div', { class: 'on-row on-over-row' },
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'over-again', 'data-primary': 'true', text: 'Play again', onclick: onAgain }),
    m.boxHref ? el('button', { type: 'button', class: 'btn', 'data-key': 'over-box', text: 'Box score ›', onclick: onBox }) : null,
    el('button', { type: 'button', class: 'btn', 'data-key': 'over-rematch', text: 'Rematch', onclick: onRematch }),
    el('button', { type: 'button', class: 'btn quiet', 'data-key': 'over-back', text: 'Back to Online', onclick: onBack }));
  return [card, row];
}
