// The Stats tab: the signed-in player's games from the server (stats_my_games), each opening the native game
// view (game.html?game=<id>). The local results queue is only counted ("N games not uploaded yet").
// Team names are player-typed server data: drawn with el()'s `text` (textContent) only, and a logo only ever
// through assetUrl() (our own /live-assets/ catalog, the one image prefix the CSP allows besides Discord's).
import { call, el, markCurrent, quietCall, reason, wireCoreFold } from './shell.js';
import { rowModel, queueLine } from './stats-rows.js';
import { assetUrl } from './live-common.js';

const PAGE_SIZE = 6;
const REFRESH_MS = 30000;
const SIGNED_OUT = 'Sign in with Discord on the Play page to see your games.';
const EMPTY = 'No games yet. Play one and it shows up here.';
const state = { signedIn: false, answered: false, games: [], queue: [], filter: 'all', page: 0, error: null, loaded: false };
const screen = document.getElementById('screen');
let generation = 0;

// The first load goes to the diagnostic log; the 30 s refreshes do not (they would push everything else out of it).
async function load(quiet = false) {
  const mine = ++generation;
  const ask = quiet ? quietCall : call;
  const [games, queue] = await Promise.allSettled([ask('stats_my_games', { limit: 200 }), ask('stats_queue')]);
  // A later load answered first (a sign-out mid-request, say): the newest request's answer wins, never an older one.
  if (mine !== generation) return;
  if (games.status === 'fulfilled') {
    state.signedIn = Boolean(games.value?.signed_in);
    state.answered = true;
    // Signed out: no rows at all, whatever an earlier answer drew.
    state.games = state.signedIn && Array.isArray(games.value?.games) ? games.value.games : [];
    state.error = null;
  } else {
    state.error = reason(games.reason);
  }
  state.queue = queue.status === 'fulfilled' && Array.isArray(queue.value?.entries) ? queue.value.entries : [];
  state.loaded = true;
  draw();
}

function abbrevOf(t) {
  return t.abbrev || Array.from(t.name).slice(0, 3).join('').toUpperCase();
}

function teamCell(t, score, mine, side) {
  const cell = el('span', { class: `stats-team${mine === side ? ' stats-team-mine' : ''}` });
  const src = assetUrl(t.logo);
  const badge = () => el('span', { class: 'stats-abbrev', 'aria-hidden': 'true', text: abbrevOf(t) });
  let logo;
  if (src) {
    logo = el('img', { class: 'stats-logo', alt: '', width: 24, height: 24, src });
    logo.addEventListener('error', () => logo.replaceWith(badge()));
  } else logo = badge();
  cell.append(logo, el('span', { class: 'stats-team-name', text: t.name }),
    el('strong', { class: 'stats-team-score', text: score === null ? '–' : String(score) }));
  return cell;
}

function row(game) {
  const r = rowModel(game);
  const s = (i) => (r.score ? r.score[i] : '–');
  const label = `${r.home.name} ${s(0)}, ${r.away.name} ${s(1)}. ${r.date}, ${r.mode}${r.result ? `, ${ { W: 'win', L: 'loss', T: 'tie' }[r.result] }` : ''}, ${r.badge}.`;
  const btn = el('button', { type: 'button', class: 'stats-game', 'data-key': `game-${r.id}`, 'aria-label': label,
    onclick: () => { location.href = `game.html?game=${encodeURIComponent(r.id)}`; } });
  btn.append(
    el('span', { class: 'stats-game-date' }, el('span', { text: r.date }), el('small', { text: r.time })),
    el('span', { class: 'stats-matchup' }, teamCell(r.home, r.score ? r.score[0] : null, r.mine, 0), teamCell(r.away, r.score ? r.score[1] : null, r.mine, 1)),
    el('span', { class: `stats-result stats-result-${(r.result || 'none').toLowerCase()}`, text: r.result || '' }),
    el('span', { class: 'stats-game-mode', text: r.mode }),
    el('span', { class: `stats-badge stats-badge-${r.badge.replace(/\s+/g, '-').toLowerCase()}`, text: r.badge }),
  );
  return btn;
}

function filtered() {
  return state.games.filter((g) => state.filter === 'all' || (state.filter === 'online' ? g.mode === 'online' : g.mode !== 'online'));
}

// Redraws replace the rows (every refresh): put the focus back on the same control, as live.js does.
function draw() {
  if (!screen) return;
  const focusKey = screen.contains(document.activeElement) ? document.activeElement.getAttribute('data-key') : null;
  paint();
  if (focusKey) for (const n of screen.querySelectorAll('[data-key]')) if (n.getAttribute('data-key') === focusKey) { n.focus({ preventScroll: true }); break; }
}

function paint() {
  if (!state.loaded) { screen.replaceChildren(el('p', { class: 'loading', text: 'Loading…' })); return; }
  const parts = [];
  const q = queueLine(state.queue);
  if (state.error) parts.push(el('p', { class: 'refusal', role: 'alert', text: state.error }));
  if (!state.signedIn) {
    // Signed out: no rows, only the way in (and what is waiting locally). Never answered at all: only the error.
    if (state.answered) parts.push(el('p', { class: 'note', text: SIGNED_OUT }));
    if (q) parts.push(el('p', { class: 'note', text: `${q.replace(' not uploaded yet', '')} will upload when you sign in.` }));
    screen.replaceChildren(...parts);
    return;
  }
  if (q) parts.push(el('p', { class: 'note', text: q }));
  parts.push(el('div', { class: 'stats-filters', role: 'group', 'aria-label': 'Show' }, ...[['all', 'All'], ['online', 'Online'], ['offline', 'Offline']].map(([k, label]) =>
    el('button', { type: 'button', class: `stats-filter${state.filter === k ? ' current' : ''}`, 'data-key': `filter-${k}`, 'aria-pressed': String(state.filter === k), text: label,
      onclick: () => { state.filter = k; state.page = 0; draw(); } }))));
  const list = filtered();
  if (!list.length) {
    parts.push(el('p', { class: 'note', text: state.games.length ? 'No games of that kind yet.' : EMPTY }));
    screen.replaceChildren(...parts);
    return;
  }
  const pages = Math.max(1, Math.ceil(list.length / PAGE_SIZE));
  state.page = Math.max(0, Math.min(state.page, pages - 1));
  parts.push(el('h2', { text: 'Your games' }));
  parts.push(el('div', { class: 'stats-list' }, ...list.slice(state.page * PAGE_SIZE, (state.page + 1) * PAGE_SIZE).map(row)));
  if (pages > 1) parts.push(el('div', { class: 'stats-pager' },
    el('button', { type: 'button', 'data-key': 'newer', text: 'Newer', disabled: state.page === 0, onclick: () => { state.page--; draw(); } }),
    el('span', { text: `Page ${state.page + 1} of ${pages}` }),
    el('button', { type: 'button', 'data-key': 'older', text: 'Older', disabled: state.page >= pages - 1, onclick: () => { state.page++; draw(); } })));
  screen.replaceChildren(...parts);
}

markCurrent();
wireCoreFold();
draw();
load();
setInterval(() => { if (document.visibilityState !== 'hidden') load(true); }, REFRESH_MS);
