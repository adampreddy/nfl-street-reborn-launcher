// One match list for Stats (Your matches) and Profile (Match history), spec §8.6 and §8.7: a header RESULT · MATCHUP ·
// OPPONENT · ELO · DATE, one row per match (a best of 3 is one row with its series and a single Elo change, and opens
// in place into its games), and the pager inside the card. Matches first (spec §10).
// Server text only through el()'s `text`; images only through assetUrl() / avatarSrc() (the CSP's two image prefixes).
import { el } from './shell.js';
import { assetUrl, avatarSrc } from './live-common.js';
import { eloChange, matchRowModel, matchSubline } from './stats-model.js';

const RESULT_WORD = { W: 'win', L: 'loss' };

export function initial(name) {
  const first = Array.from(String(name || '').trim())[0];
  return first ? first.toUpperCase() : '?';
}

/** A team's full-colour backend logo, or its abbreviation in a small round badge when there is none or it fails. */
export function teamLogo(team, cls = '') {
  const c = `team-logo${cls ? ` ${cls}` : ''}`;
  const badge = () => el('span', { class: `${c} text`, 'aria-hidden': 'true', text: team?.abbrev || initial(team?.name) });
  const src = assetUrl(team?.logo_url);
  if (!src) return badge();
  const img = el('img', { class: c, alt: '', src, title: team?.name || null });
  img.addEventListener('error', () => img.replaceWith(badge()), { once: true });
  return img;
}

/** A player's Discord avatar, or the first letter of their name in the same circle. */
export function avatar(url, name, cls = '') {
  const c = `avatar${cls ? ` ${cls}` : ''}`;
  const letter = () => el('span', { class: c, 'aria-hidden': 'true', text: initial(name) });
  const src = avatarSrc({ avatar_url: url });
  if (!src) return letter();
  const img = el('img', { class: c, alt: '', src });
  img.addEventListener('error', () => img.replaceWith(letter()), { once: true });
  return img;
}

/** The list's column header (the rows carry full aria-labels, so it is hidden from screen readers). */
export function matchHeader() {
  return el('div', { class: 'm-row m-hd', 'aria-hidden': 'true' }, ...['Result', 'Matchup', 'Opponent', 'Elo', 'Date', ''].map((t) => el('span', { text: t })));
}

/** The ids of a set of matches' games, newest first (the server sends each match's games oldest first). This is the
 *  ordered list game.html's ‹ Previous / Next › bar steps through. */
export function gameIdsOf(matches) {
  const ids = [];
  for (const m of Array.isArray(matches) ? matches : []) {
    const games = Array.isArray(m?.games) ? m.games : [];
    for (let i = games.length - 1; i >= 0; i--) if (games[i] && typeof games[i].id === 'string') ids.push(games[i].id);
  }
  return ids;
}

/** Remembered right before opening a game, so the game view can step through this list and come back (`from`). */
export function storeGameList(matches, label, from) {
  try { sessionStorage.setItem('nsr.gameList', JSON.stringify({ ids: gameIdsOf(matches), label, from })); } catch { /* best effort */ }
}

const resultCell = (result, score) => el('span', { class: 'm-res' }, el('b', { class: result || null, text: result }), el('span', { text: score }));
const matchup = (me, opp) => el('span', { class: 'm-mu' }, teamLogo(me), el('i', { text: 'vs' }), teamLogo(opp));

function label(m, r, sub, elo) {
  const teams = `${m?.me?.name || 'Your team'} vs ${m?.opp?.name || 'their team'}`;
  const who = m?.opponent_kind === 'cpu' ? 'against the CPU' : r.opponent !== '—' ? `against ${r.opponent}` : '';
  const score = r.expandable ? `series ${r.score}` : r.score !== '—' ? `score ${r.score}` : 'no score';
  const parts = [`${r.date} ${r.time}`, [teams, who].filter(Boolean).join(', '), [score, RESULT_WORD[r.result]].filter(Boolean).join(', ')];
  if (sub) parts.push(sub);
  if (elo.text) parts.push(`${elo.text} Elo`);
  if (r.expandable) parts.push(`${r.games.length} games`);
  return `${parts.join('. ')}.`;
}

/**
 * A match's row, and for a best of 3 its games under it. `where` picks the second line ('stats': Offline / Private
 * room; 'profile': Ranked / Casual). `open` (a Set of match ids) is read and updated, so it survives redraws; opening
 * toggles classes in place (the reveal animates), closed games are inert. `openGame(id)` opens one game.
 */
export function matchRows(m, { where, open, openGame }) {
  const r = matchRowModel({ ...m, games: Array.isArray(m?.games) ? m.games : [] });
  const sub = matchSubline(m, where);
  const elo = eloChange(m?.elo_delta);
  const kids = [
    resultCell(r.result, r.score),
    matchup(r.me, r.opp),
    // The CPU (or an unknown opponent) gets the same circle with its first letter, so every name starts on one line.
    el('span', { class: 'm-opp' }, avatar(m?.opponent_kind === 'cpu' || !m?.opponent ? null : r.avatar, r.opponent),
      el('span', { class: 'm-opp-tx' }, el('span', { class: 'm-opp-n', text: r.opponent }), sub ? el('small', { text: sub }) : null)),
    el('span', { class: `m-elo${elo.kind ? ` ${elo.kind}` : ''}`, text: elo.text }),
    el('span', { class: 'm-date', text: r.date }),
    el('span', { class: r.expandable ? 'chev' : null, 'aria-hidden': 'true' }),
  ];
  const attrs = { class: 'm-row', 'data-key': `match-${r.id}`, 'aria-label': label(m, r, sub, elo) };
  if (!r.expandable) {
    if (r.onlyGame) return [el('button', { ...attrs, type: 'button', onclick: () => openGame(r.onlyGame) }, ...kids)];
    // A match with no recorded game has nothing to open.
    return [el('div', { ...attrs, class: 'm-row m-static', role: 'group' }, ...kids)];
  }
  const isOpen = open.has(r.id);
  const id = `games-${String(r.id).replace(/[^A-Za-z0-9_-]/g, '_')}`;
  const inner = el('div', { inert: isOpen ? null : '' }, ...r.games.map((g, i) => el('button', {
    type: 'button', class: 'm-row m-game', 'data-key': `game-${g.id}`, onclick: () => openGame(g.id),
    'aria-label': `Game ${i + 1}. ${g.score !== '—' ? `Score ${g.score}` : 'No score'}${g.result ? `, ${RESULT_WORD[g.result]}` : ''}. ${g.date} ${g.time}.`,
  }, resultCell(g.result, g.score), matchup(g.me, g.opp), el('span', { class: 'm-opp', text: `Game ${i + 1} · ${g.time}` }), el('span'), el('span'), el('span'))));
  const reveal = el('div', { class: `rows-reveal${isOpen ? ' open' : ''}`, id }, inner);
  const row = el('button', { ...attrs, type: 'button', 'aria-expanded': String(isOpen), 'aria-controls': id }, ...kids);
  row.addEventListener('click', () => {
    const now = !open.has(r.id);
    if (now) open.add(r.id); else open.delete(r.id);
    row.setAttribute('aria-expanded', String(now));
    reveal.classList?.toggle('open', now);
    if (now) inner.removeAttribute?.('inert'); else inner.setAttribute('inert', '');
  });
  return [row, reveal];
}

/** Newer / "Page N of M" / Older, inside the card as its last row. `pages` null draws "Page N" (the total is not known). */
export function pager({ page, pages, onNewer, onOlder, olderDisabled }) {
  return el('div', { class: 'm-pager' },
    el('button', { type: 'button', class: 'btn sm quiet', 'data-key': 'newer', text: 'Newer', disabled: page === 0, onclick: onNewer }),
    el('span', { text: pages ? `Page ${page + 1} of ${pages}` : `Page ${page + 1}` }),
    el('button', { type: 'button', class: 'btn sm', 'data-key': 'older', text: 'Older', disabled: Boolean(olderDisabled), onclick: onOlder }));
}
