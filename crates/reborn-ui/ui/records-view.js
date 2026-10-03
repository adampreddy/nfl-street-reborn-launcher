// Records (owner 10-03, "Records get their own tab"): one table, Single game or Career, for Stats › Records and the
// Profile's Records tab. Columns: Record · Player (with the team logo) · Best · Game ("vs <opponent>", or the games
// a career leader played) · Date. A single-game record opens its game. The rows come from profile-model.js
// (singleGameRecords, careerRecords); this file only draws them.
// Server text only through el()'s `text`; images only through teamLogo() (the CSP's image prefixes).
import { el } from './shell.js';
import { teamLogo } from './match-rows.js';
import { gameHref } from './stats-model.js';

export const RECORD_KINDS = [['single', 'Single game'], ['career', 'Career']];

/** The Single game | Career pills. `onPick(kind)` redraws; the pressed pill keeps data-key rec-kind-<kind>. */
export function recordPills(kind, onPick) {
  return el('div', { class: 'rec-pills', role: 'group', 'aria-label': 'Records' }, ...RECORD_KINDS.map(([k, label]) =>
    el('button', { type: 'button', class: 'rec-pill', 'data-key': `rec-kind-${k}`, 'aria-pressed': String(k === kind), text: label,
      onclick: () => { if (k !== kind) onPick(k); } })));
}

/**
 * The records table. `rows` from singleGameRecords / careerRecords; `kind` 'single' | 'career'; `from` is where Back on
 * a record's game returns to (stats-model.js backHref). `empty` is the words when there are no rows.
 */
export function recordsTable(rows, { kind, from, empty }) {
  const career = kind === 'career';
  const head = el('div', { class: 'rec-row rec-hd', 'aria-hidden': 'true' },
    ...['Record', 'Player', career ? 'Total' : 'Best', career ? 'Games' : 'Game', career ? '' : 'Date'].map((t) => el('span', { text: t })));
  const card = el('div', { class: `glass rec-table${career ? ' career' : ''}` }, head);
  if (!rows.length) { card.append(el('p', { class: 'rec-empty', text: empty })); return card; }
  for (const r of rows) {
    const cells = [el('span', { class: 'rec-k', text: r.label }),
      r.name ? el('span', { class: 'rec-who' }, teamLogo(r.team || {}), el('span', { class: 'rec-name', text: r.name })) : el('span', { class: 'rec-who none', text: '—' }),
      el('b', { class: 'rec-v num', text: r.value }), el('span', { class: 'rec-g', text: r.game }), el('span', { class: 'rec-d num', text: r.date })];
    const label = [`${r.label}: ${r.value}`, r.name, r.game !== '—' ? r.game : null, r.date || null].filter(Boolean).join(', ');
    card.append(r.game_id
      ? el('a', { class: 'rec-row', href: gameHref(r.game_id, from), 'data-key': `rec-${r.key}`, 'aria-label': `${label}. Open the game` }, ...cells)
      : el('div', { class: 'rec-row', role: 'group', 'aria-label': label }, ...cells));
  }
  return card;
}
