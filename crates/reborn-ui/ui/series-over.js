// The series-over screen (series spec 2026-10-02 §3.3): both players, the series score, each game's score, an
// "Unofficial" tag until the server says the result is official, and Full game / Find another game / Rematch.
// PURE: no imports, so node --test can load it; the page hands in its own `el`. Only the Online page's existing
// classes are used (plan ruling R21: another session owns launcher styling).

const PAGE = 'https://online.nflstreetreborn.com';
const OVER = new Set(['win', 'forfeit', 'split']);

/** Everything the screen says, or null when there is no finished series to show. */
export function seriesOverModel(match, live) {
  const o = match?.outcome;
  if (!o || !OVER.has(o.kind)) return null;
  const me = match.seat === 1 ? 1 : 0;
  const wins = Array.isArray(match.series_live) && match.series_live.length === 2 ? match.series_live
    : Array.isArray(match.series) ? match.series : [0, 0];
  const names = [live?.seats?.[0]?.display_name, live?.seats?.[1]?.display_name];
  const you = names[me] || 'You';
  const them = names[1 - me] || match.opponent || 'Your opponent';
  const mine = wins[me] ?? 0, theirs = wins[1 - me] ?? 0;
  const winner = o.winner_seat === 0 || o.winner_seat === 1 ? o.winner_seat : null;
  const headline = o.kind === 'split' ? `Split ${mine}-${theirs}`
    : winner === me ? `You won ${mine}-${theirs}` : `${them} won ${theirs}-${mine}`;
  const note = o.kind === 'forfeit' ? (o.left_seat === me ? 'You left the series' : `${them} left the series`) : null;
  const games = (live?.games ?? []).filter((g) => g.decided).map((g) => {
    const side = Array.isArray(g.sides) ? g.sides.findIndex((s) => s?.seat === me) : -1;
    const sc = Array.isArray(g.score) ? g.score : [0, 0];
    return side === -1 ? { n: g.game_seq, mine: sc[0], theirs: sc[1] } : { n: g.game_seq, mine: sc[side], theirs: sc[1 - side] };
  });
  return { kind: o.kind, title: 'Series over', headline, note, you, them, games, unofficial: o.official !== true,
    fullUrl: `${PAGE}/live/${match.id}` };
}

/** The screen's nodes, built with the page's `el` (shell.js). */
export function drawSeriesOver(m, { el, onFind, onRematch, onFull }) {
  return [
    el('h2', { text: m.title }),
    el('p', { class: 'mm-state', text: m.headline }),
    m.unofficial ? el('p', { class: 'note', text: 'Unofficial' }) : null,
    m.note ? el('p', { class: 'note', text: m.note }) : null,
    el('p', { class: 'mm-series', text: `${m.you} vs ${m.them}` }),
    ...m.games.map((g) => el('p', { class: 'note', text: `Game ${g.n}: ${g.mine}-${g.theirs}` })),
    el('div', { class: 'mm-actions' },
      el('button', { type: 'button', text: 'Full game ↗', onclick: onFull }),
      el('button', { type: 'button', class: 'primary', text: 'Find another game', onclick: onFind }),
      el('button', { type: 'button', text: 'Rematch', onclick: onRematch })),
  ].filter(Boolean);
}
