// Records (owner 10-03, mv2-2): Single game and Career as two tables side by side, then Team records, for Stats ›
// Records and the Profile's Records view. Single game: Record · Player (with the team logo) · Best · Date, a row
// opening its game. Career: Record · Player · Total · Per game. Team records (the records with no player: most
// points, biggest win...): one strip of cells. A piece with no rows is left out. The rows come from profile-model.js
// (singleGameRecords, careerRecords, teamRecords); this file only draws them.
// Server text only through el()'s `text`; images only through teamLogo() (the CSP's image prefixes).
import { el } from './shell.js';
import { teamLogo } from './match-rows.js';
import { gameHref } from './stats-model.js';
import { card, cellGrid, tableHead } from './stats-cards.js';

const COLS = 'minmax(0,1.15fr) minmax(0,1.45fr) 56px 70px';

/** One records table as a v2 card. `kind` 'single' | 'career'; `from` is where Back on a record's game returns to. */
export function recordsTable(rows, { kind, from, empty, span = 6 }) {
  const career = kind === 'career';
  const list = el('div', { class: 'rec-rows' });
  if (!rows.length) list.append(el('p', { class: 'rec-empty', text: empty }));
  for (const r of rows) {
    const cells = [el('span', { class: 'rec-k', text: r.label }),
      r.name ? el('span', { class: 'rec-who' }, teamLogo(r.team || {}), el('span', { class: 'rec-name', text: r.name })) : el('span', { class: 'rec-who none', text: '—' }),
      el('b', { class: 'rec-v num', text: r.value }), el('span', { class: 'rec-d num', text: career ? r.perGame : r.date })];
    const label = [`${r.label}: ${r.value}`, r.name, career ? (r.perGame !== '—' ? `${r.perGame} per game` : null) : r.game !== '—' ? r.game : null, career ? r.game : r.date || null]
      .filter((x) => x && x !== '—').join(', ');
    const row = r.game_id
      ? el('a', { class: 'rec-row', href: gameHref(r.game_id, from), 'data-key': `rec-${kind}-${r.key}`, 'aria-label': `${label}. Open the game`, title: r.game !== '—' ? r.game : null }, ...cells)
      : el('div', { class: 'rec-row', role: 'group', 'aria-label': label }, ...cells);
    row.style && (row.style.gridTemplateColumns = COLS);
    list.append(row);
  }
  return card(`rec-${kind}`, career ? 'Career' : 'Single game', [tableHead(['Record', 'Player', career ? 'Total' : 'Best', career ? 'Per game' : 'Date'], COLS, [2, 3]), list],
    { aside: career ? 'Leader across all games' : 'Best in one game', flush: true, span, cls: 'rec-table' });
}

/** The whole Records view: the two tables side by side, Team records under them when there are any. */
export function recordsView({ single, career, team }, { from, aside = null }) {
  const rows = [];
  const top = [single.length ? recordsTable(single, { kind: 'single', from }) : null, career.length ? recordsTable(career, { kind: 'career', from }) : null].filter(Boolean);
  if (top.length === 1) top[0].className = top[0].className.replace(/\bs6\b/, 's12');
  if (top.length) rows.push(el('div', { class: 'vgrid' }, ...top));
  if (team.length) rows.push(el('div', { class: 'vgrid' }, card('rec-team', 'Team records', cellGrid(team, Math.min(6, team.length)), { aside, flush: true })));
  return rows;
}
