// One match list for Stats (Your matches) and Profile (Match history), spec §8.6 and §8.7: a header RESULT · MATCHUP ·
// OPPONENT · ELO · DATE, one row per match (a best of 3 is one row with its series and a single Elo change), and the
// pager inside the card. Matches first (spec §10). A row opens the WHOLE match on one page (owner 10-03, stats-model.js
// matchHref): the site's /live/<match> page (match.html), a matchmade match by its id, any other by its games (a
// one-game match just that game; game-snapshot.js).
// Server text only through el()'s `text`; images only through assetUrl() / avatarSrc() (the CSP's two image prefixes).
import { el } from './shell.js';
import { assetUrl, avatarSrc } from './live-common.js';
import { eloChange, matchHref, matchRowModel, matchSubline, queueMark } from './stats-model.js';

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

/** The letter circle's colour for a name: one of theme.css's eight `.av-N`, the same name always the same one. */
export function letterColour(name) {
  let h = 0;
  for (const ch of Array.from(String(name || ''))) h = (h * 31 + ch.codePointAt(0)) >>> 0;
  return `av-${h % 8}`;
}

/** A player's Discord avatar, or the first letter of their name in a circle coloured from the name (`cpu: true` = the
 *  CPU's neutral circle). */
export function avatar(url, name, cls = '', { cpu = false } = {}) {
  const c = `avatar${cls ? ` ${cls}` : ''}`;
  const letter = () => el('span', { class: `${c} ${cpu ? 'av-cpu' : letterColour(name)}`, 'aria-hidden': 'true', text: initial(name) });
  const src = avatarSrc({ avatar_url: url });
  if (!src) return letter();
  const img = el('img', { class: c, alt: '', src });
  img.addEventListener('error', () => img.replaceWith(letter()), { once: true });
  return img;
}

/** The list's column header (the rows carry full aria-labels, so it is hidden from screen readers). */
export function matchHeader({ queue = false } = {}) {
  const words = queue ? ['Result', 'Matchup', 'Opponent', 'Queue', 'Elo', 'Date'] : ['Result', 'Matchup', 'Opponent', 'Elo', 'Date', ''];
  return el('div', { class: 'm-row m-hd', 'aria-hidden': 'true' }, ...words.map((t) => el('span', { text: t })));
}

/** The queue column (the v2 Stats list, owner 10-03 mockup): Ranked with a red dot, Casual with a ring, Offline plain. */
export function queueCell(m) {
  const q = queueMark(m);
  return el('span', { class: `qm${q.kind ? ` ${q.kind}` : ''}`, text: q.text });
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
 * A match's row. `where` picks the second line ('stats': Offline only; 'profile': Ranked / Casual). `from` is the page
 * Back on the match page returns to (stats-model.js backHref). The row opens the whole match; a match with no recorded
 * game has nothing to open. Returned as a one-row list (the lists flatMap their rows).
 */
export function matchRows(m, { where, from, queue = false }) {
  const r = matchRowModel({ ...m, games: Array.isArray(m?.games) ? m.games : [] });
  // With the queue column the queue (and Offline) is its own cell, never a second line under the name.
  const sub = queue ? '' : matchSubline(m, where);
  const elo = eloChange(m?.elo_delta);
  const kids = [
    resultCell(r.result, r.score),
    matchup(r.me, r.opp),
    // The CPU (or an unknown opponent) gets the same circle with its first letter, so every name starts on one line.
    el('span', { class: 'm-opp' }, avatar(m?.opponent_kind === 'cpu' || !m?.opponent ? null : r.avatar, r.opponent, '', { cpu: m?.opponent_kind === 'cpu' }),
      el('span', { class: 'm-opp-tx' }, el('span', { class: 'm-opp-n', text: r.opponent }), sub ? el('small', { text: sub }) : null)),
    ...(queue ? [queueCell(m)] : []),
    el('span', { class: `m-elo${elo.kind ? ` ${elo.kind}` : queue ? ' none' : ''}`, text: elo.text || (queue ? '—' : '') }),
    el('span', { class: 'm-date', text: r.date }),
    ...(queue ? [] : [el('span', { 'aria-hidden': 'true' })]),
  ];
  const q = queue ? queueMark(m).text : '';
  const attrs = { class: 'm-row', 'data-key': `match-${r.id}`, 'aria-label': label(m, r, sub || q, elo) };
  const href = matchHref(m, from);
  if (!href) return [el('div', { ...attrs, class: 'm-row m-static', role: 'group' }, ...kids)];
  return [el('button', { ...attrs, type: 'button', onclick: () => { location.href = href; } }, ...kids)];
}

/** Newer / "Page N of M" / Older, inside the card as its last row. `pages` null draws "Page N" (the total is not known). */
export function pager({ page, pages, onNewer, onOlder, olderDisabled }) {
  return el('div', { class: 'm-pager' },
    el('button', { type: 'button', class: 'btn sm quiet', 'data-key': 'newer', text: 'Newer', disabled: page === 0, onclick: onNewer }),
    el('span', { text: pages ? `Page ${page + 1} of ${pages}` : `Page ${page + 1}` }),
    el('button', { type: 'button', class: 'btn sm', 'data-key': 'older', text: 'Older', disabled: Boolean(olderDisabled), onclick: onOlder }));
}
