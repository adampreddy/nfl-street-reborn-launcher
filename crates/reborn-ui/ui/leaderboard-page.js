// The full leaderboard in the launcher (owner 10-08: "i also would like to be able to view the full leaderboard in the
// launcher"), opened from the Online page's "Full leaderboard ›" as leaderboard.html[?page=N]. The Online card's look,
// full width: rank, picture (with the plays-as team), name (the Discord name under a nickname), record, Elo and the
// week's change, 50 rows a page. Your row is marked; when it is on another page it is pinned under this one, and
// "Jump to me" opens your page. Every row opens that player's profile (player-link.js); Back there returns here, to
// the same page and the same row (the page number is in the address, the row in the history entry).
//
// Read with the `leaderboard` command ({ limit, offset }; the backend's public GET /v1/leaderboard, with your token so
// it can mark your row). A server that does not page yet (no `offset` in its answer) shows its top 50 as one page.
// Server text only through el()'s `text`; images only through match-rows.js's avatar() and teamLogo().
import { bridge, el, quietCall, reason } from './shell.js';
import { avatar, teamLogo } from './match-rows.js';
import { BOARD_PAGE, boardPage, pageOfRank } from './online-model.js';
import { playerIdOf, profileHref } from './player-link.js';

const screen = document.getElementById('screen');
const bar = document.getElementById('lb-bar');
const headEnd = document.getElementById('lb-head-end');
const query = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const asked = Math.max(1, Math.trunc(Number(query.get('page'))) || 1);   // 1-based in the address
const state = { page: asked - 1, view: null, error: null, busy: false };
// The profile the player opened last from this page (its row's data-key), kept in this page's history entry so Back
// from that profile lands on it again.
const returnTo = (() => { try { return typeof history.state?.lbFocus === 'string' ? history.state.lbFocus : null; } catch { return null; } })();

/** Back (and B / Esc, which click a visible #back): the launcher page this one was opened from, else Online. */
function backButton() {
  const b = el('button', { type: 'button', class: 'lb-back', id: 'back', 'data-key': 'back' },
    el('span', { class: 'chev', 'aria-hidden': 'true' }), el('span', { text: 'Back' }));
  b.addEventListener('click', () => {
    let fromLauncher = false;
    try { fromLauncher = Boolean(document.referrer) && new URL(document.referrer).origin === location.origin; } catch { /* no referrer */ }
    if (fromLauncher && history.length > 1) history.back(); else location.href = 'online.html';
  });
  return b;
}

function rowKey(r) { return r.player_id ? `lb-${r.player_id}` : `lb-rank-${r.rank}`; }

/** One player's row: the whole row is the link to their profile (a row with no account id is plain). */
function row(r) {
  const label = [r.rank !== null ? `#${r.rank}` : null, r.name || 'Player', r.discord ? `(@${r.discord})` : null, r.record ? `${r.record} record` : null,
    r.elo ? `${r.elo} Elo` : null, r.change ? `${r.change.dir === 'up' ? 'up' : 'down'} ${r.change.text.slice(2)} this week` : null, r.you ? 'you' : null]
    .filter(Boolean).join(', ');
  const kids = [
    el('span', { class: 'lb-rank num', text: r.rank === null ? '' : String(r.rank) }),
    el('span', { class: 'lb-face' }, avatar(r.avatar, r.name), r.team ? teamLogo(r.team) : null),
    el('span', { class: 'lb-who' },
      el('span', { class: 'lb-name' }, r.name || 'Player', r.you ? el('small', { class: 'lb-you', text: 'You' }) : null),
      r.discord ? el('small', { class: 'lb-discord', text: `@${r.discord}` }) : null),
    el('span', { class: 'lb-rec num', text: r.record || '—' }),
    el('span', { class: 'lb-elo num', text: r.elo || '—' }),
    el('span', { class: `lb-week num${r.change ? ` ${r.change.dir}` : ''}`, text: r.change ? r.change.text : '—' }),
  ];
  const cls = `lb-row${r.rank !== null && r.rank <= 3 ? ' top' : ''}${r.you ? ' you' : ''}`;
  const id = playerIdOf(r.player_id);
  if (!id) return el('li', {}, el('div', { class: cls, role: 'group', 'aria-label': label }, ...kids));
  const a = el('a', { class: `${cls} player-link`, href: profileHref(id), 'data-key': rowKey(r), 'aria-label': `${label}. Open profile` }, ...kids);
  a.addEventListener('click', () => {
    try { history.replaceState({ ...(history.state || {}), lbFocus: rowKey(r) }, ''); } catch { /* the return spot is a convenience */ }
  });
  return el('li', {}, a);
}

function pagerBar(v) {
  if (!v.paged || v.pages <= 1) return null;
  return el('div', { class: 'lb-pager' },
    el('button', { type: 'button', class: 'btn sm quiet', 'data-key': 'lb-prev', text: 'Previous', disabled: v.page === 0, onclick: () => go(v.page - 1, 'lb-prev') }),
    el('span', { class: 'num', text: `Page ${v.page + 1} of ${v.pages}` }),
    el('button', { type: 'button', class: 'btn sm', 'data-key': 'lb-next', text: 'Next', disabled: v.page >= v.pages - 1, onclick: () => go(v.page + 1, 'lb-next') }));
}

/** "Jump to me": your page, when your row is pinned under another page (signed in, ranked, and not on this page). */
function jumpButton(v) {
  const p = v?.you && v.paged ? pageOfRank(v.you.rank) : null;
  if (p === null || p === v.page) return null;
  return el('button', { type: 'button', class: 'btn sm', 'data-key': 'lb-jump', text: 'Jump to me', onclick: () => go(p, null, true) });
}

function draw(focusKey = null) {
  const v = state.view;
  const was = focusKey ?? document.activeElement?.getAttribute?.('data-key') ?? null;
  bar.replaceChildren(backButton());
  headEnd.replaceChildren(...[v && !v.soon && v.total !== null ? el('span', { class: 'lb-total', text: `${v.total.toLocaleString('en-US')} ranked` }) : null,
    v && !v.soon ? jumpButton(v) : null].filter(Boolean));
  const parts = [];
  if (state.error) parts.push(el('p', { class: 'refusal', role: 'alert', text: state.error }));
  if (!v) {
    if (!state.error) parts.push(el('p', { class: 'loading', text: 'Loading…' }));
    else parts.push(el('button', { type: 'button', class: 'btn sm', 'data-key': 'lb-retry', text: 'Try again', onclick: () => load(state.page) }));
  } else if (v.soon) {
    parts.push(el('p', { class: 'lb-soon', text: 'Rankings drop soon.' }));
  } else {
    parts.push(el('div', { class: 'lb-hd', 'aria-hidden': 'true' }, el('span', { text: '#' }), el('span', { text: 'Player' }), el('span', { text: 'Record' }),
      el('span', { text: 'Elo' }), el('span', { text: 'Week' })));
    const list = el('ol', { class: 'lb-rows', 'aria-label': `Ranks ${v.page * BOARD_PAGE + 1} to ${v.page * BOARD_PAGE + v.rows.length}` }, ...v.rows.map(row));
    if (!v.rows.length) list.append(el('li', { class: 'lb-empty', text: 'Nobody on this page.' }));
    if (v.you) list.append(el('li', { class: 'lb-gap', 'aria-hidden': 'true', text: '···' }), row(v.you));
    parts.push(list);
    if (!v.paged && v.total !== null && v.total > v.rows.length) parts.push(el('p', { class: 'lb-note', text: `Showing the top ${v.rows.length}.` }));
    parts.push(pagerBar(v));
  }
  screen.replaceChildren(...parts.filter(Boolean));
  screen.removeAttribute('aria-busy');
  if (was) {
    const n = [...document.querySelectorAll('[data-key]')].find((x) => x.getAttribute('data-key') === was && !x.disabled);
    n?.focus({ preventScroll: false });
  }
}

/** Page `p` (0-based): the address says it, the rows are read, and focus goes to `focusKey` (or the first row). */
async function go(p, focusKey = null, toMe = false) {
  if (state.busy) return;
  try { history.replaceState({ ...(history.state || {}), lbFocus: null }, '', p > 0 ? `?page=${p + 1}` : location.pathname); } catch { /* a convenience */ }
  await load(p, focusKey, toMe);
}

async function load(p, focusKey = null, toMe = false) {
  state.busy = true;
  try {
    const answer = await quietCall('leaderboard', { limit: BOARD_PAGE, offset: p * BOARD_PAGE });
    state.view = boardPage(answer, p * BOARD_PAGE);
    state.page = state.view.soon ? 0 : state.view.page;
    state.error = null;
  } catch (e) {
    state.error = reason(e) || 'The leaderboard could not be loaded.';
  }
  state.busy = false;
  // Jump to me lands on your row; Previous / Next keep their button (the first row once it is disabled at an end).
  const mine = state.view?.rows?.find((r) => r.you);
  const first = state.view?.rows?.[0] ? rowKey(state.view.rows[0]) : null;
  draw(toMe && mine ? rowKey(mine) : focusKey);
  if (focusKey && document.activeElement?.getAttribute?.('data-key') !== focusKey && first) draw(first);
  if (toMe && mine) document.querySelector(`[data-key="${rowKey(mine)}"]`)?.scrollIntoView({ block: 'center' });
}

if (!bridge) {
  bar.replaceChildren(backButton());
  screen.replaceChildren(el('p', { class: 'refusal', role: 'alert', text: 'Open this page inside the launcher window.' }));
  screen.removeAttribute('aria-busy');
} else {
  draw();
  load(state.page).then(() => {
    // Back from a profile: the row that opened it, as it was.
    if (!returnTo) return;
    const n = [...document.querySelectorAll('[data-key]')].find((x) => x.getAttribute('data-key') === returnTo);
    if (n) { n.focus({ preventScroll: true }); n.scrollIntoView({ block: 'center' }); }
  });
}
