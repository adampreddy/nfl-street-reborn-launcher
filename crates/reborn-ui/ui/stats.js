// The Stats tab (spec §8.6): the signed-in player's own dashboard (`stats_dashboard`). Left: Your matches (a match of
// one game opens game.html?game=<id>; a best of 3 opens in place into its games). Right: You (Elo, trend, Record ·
// Win % · Last 10 · Streak) and Your players. Under both, one line: the local results queue on the left and
// Profile & sync on the right (offline visibility, sign out, send results now).
// Everything here is server data (player-typed team and player names among it): drawn with el()'s `text`
// (textContent) only, never markup. Images only through assetUrl() (our /live-assets/ catalog) or avatarSrc() (the
// Discord avatar CDN), the two image prefixes the CSP allows. The only script-set style is a portrait's --tc (CSSOM).
import { call, el, markCurrent, partial, quietCall, reason, wireCoreFold } from './shell.js';
import { queueLine, unsentCount } from './stats-rows.js';
import { assetUrl } from './live-common.js';
import { dropPrivate, eloChange, fmtElo, resultsOf, roleCards, trendPoints, youNumbers } from './stats-model.js';
import { matchHeader, matchRows, pager, storeGameList } from './match-rows.js';
import { tabs } from './tabs.js';

const PAGE_SIZE = 10;
const REFRESH_MS = 30000;
const SIGNED_OUT = 'Sign in with Discord on the Play page to see your games.';
const EMPTY = 'No games yet. Play one and it shows up here.';
const FILTERS = [['all', 'All'], ['online', 'Online'], ['offline', 'Offline']];
// Which part of the profile each player opens: their role's Full stats table, or Career.
const ROLE_HASH = { passer: 'passing', rusher: 'rushing', receiver: 'receiving', most_played: 'career', dl_lb: 'defense', ballhawk: 'defense' };
const SVG_NS = 'http://www.w3.org/2000/svg';
const PAGER_OTHER = { older: 'newer', newer: 'older' };

const state = { signedIn: false, answered: false, dash: null, queue: [], filter: 'all', page: 0, error: null, loaded: false,
  // Which best-of-3 rows are open (by match id), kept across the 30 s refreshes.
  expanded: new Set(),
  // Profile & sync: the identity_status and stats_queue answers, or { error } when one failed.
  identity: null, queueView: null };
let busy = false, said = null;
const ok = (value) => value && !value.error;
const screen = document.getElementById('screen');
let generation = 0;

// The first load goes to the diagnostic log; the 30 s refreshes do not (they would push everything else out of it).
async function load(quiet = false) {
  const mine = ++generation;
  const ask = quiet ? quietCall : call;
  const [dash, queue, identity] = await Promise.allSettled([ask('stats_dashboard'), ask('stats_queue'), ask('identity_status')]);
  // A later load answered first (a sign-out mid-request, say): the newest request's answer wins, never an older one.
  if (mine !== generation) return;
  if (dash.status === 'fulfilled') {
    const v = dash.value;
    state.signedIn = Boolean(v?.signed_in);
    state.answered = true;
    if (!state.signedIn) { state.dash = null; state.error = null; }
    else if (v.ok === false) state.error = typeof v.error === 'string' && v.error ? v.error : 'Your stats could not be loaded.';
    else { state.dash = v; state.error = null; }
  } else {
    state.error = reason(dash.reason);
  }
  state.queue = queue.status === 'fulfilled' && Array.isArray(queue.value?.entries) ? queue.value.entries : [];
  state.queueView = queue.status === 'fulfilled' ? queue.value : { error: reason(queue.reason) };
  state.identity = identity.status === 'fulfilled' ? identity.value : { error: reason(identity.reason) };
  state.loaded = true;
  draw();
}

const profileHref = (id, hash = '') => `profile.html?player=${encodeURIComponent(id)}${hash ? `#${hash}` : ''}`;
const gameHref = (id) => `game.html?game=${encodeURIComponent(id)}`;

function svg(tag, attrs = {}) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  return node;
}

// ---- Your matches ----

function filtered(matches) {
  return matches.filter((m) => state.filter === 'all' || (state.filter === 'online' ? m.mode === 'online' : m.mode !== 'online'));
}

function matchesBlock(matches) {
  const bar = tabs('stats-matches', FILTERS.map(([, label]) => label), FILTERS.find(([k]) => k === state.filter)[1],
    (label) => { state.filter = FILTERS.find(([, l]) => l === label)[0]; state.page = 0; draw(); }, { label: 'Show matches' });
  if (typeof bar.querySelectorAll === 'function') bar.querySelectorAll('.tab').forEach((b, i) => b.setAttribute('data-key', `filter-${FILTERS[i][0]}`));
  const card = el('div', { class: 'glass m-list st-list' }, matchHeader());
  const list = filtered(matches);
  if (!list.length) card.append(el('p', { class: 'm-empty', text: 'No matches of that kind yet.' }));
  else {
    const pages = Math.max(1, Math.ceil(list.length / PAGE_SIZE));
    state.page = Math.max(0, Math.min(state.page, pages - 1));
    const openGame = (id) => { storeGameList(list, 'Your matches', 'stats.html'); location.href = gameHref(id); };
    card.append(...list.slice(state.page * PAGE_SIZE, (state.page + 1) * PAGE_SIZE).flatMap((m) => matchRows(m, { where: 'stats', open: state.expanded, openGame })));
    if (pages > 1) card.append(pager({ page: state.page, pages, onNewer: () => { state.page--; draw(); }, onOlder: () => { state.page++; draw(); }, olderDisabled: state.page >= pages - 1 }));
  }
  return el('section', { class: 'st-col', 'aria-labelledby': 'st-matches-h' },
    el('div', { class: 'sec-head' }, el('h2', { id: 'st-matches-h', text: 'Your matches' }), bar), card);
}

// ---- You ----

function youBlock(you, recentForm) {
  const e = fmtElo(you.elo);
  // The change since the last ranked match: eloChange decides up / down / nothing; the card draws it as ▲ 16 / ▼ 18.
  const delta = you.elo?.rating == null ? null : you.elo?.last_delta;
  const change = eloChange(delta);
  const hist = Array.isArray(you.elo?.history) ? you.elo.history : [];
  const trend = trendPoints(hist, 200, 54);
  const link = you.player_id ? el('a', { class: 'sec-link', href: profileHref(you.player_id), 'data-key': 'profile', text: 'Your profile ›' }) : null;
  const chg = el('div', { class: 'st-chg' });
  if (change.kind) chg.append(el('span', { class: change.kind, 'aria-label': `${change.kind === 'up' ? 'Up' : 'Down'} ${Math.abs(delta)} since your last ranked match`, text: `${change.kind === 'up' ? '▲' : '▼'} ${Math.abs(delta)}` }));
  if (change.kind && e.rank) chg.append(el('span', { 'aria-hidden': 'true', text: ' · ' }));
  if (e.rank) chg.append(el('span', { text: e.rank }));
  const top = el('div', { class: 'st-you-top' }, el('div', {}, el('span', { class: 'k-label', text: 'Elo' }),
    el('div', { class: `st-elo-big${you.elo?.rating == null ? ' unranked' : ''}`, text: e.value }), chg));
  if (trend && typeof document.createElementNS === 'function') {
    const chart = svg('svg', { class: 'st-spark', viewBox: '0 0 200 54', preserveAspectRatio: 'none', role: 'img', 'aria-label': `Elo trend over your last ${hist.length} ranked matches` });
    chart.append(svg('polyline', { fill: 'none', stroke: 'currentColor', 'stroke-width': 2, 'stroke-linejoin': 'round', 'stroke-linecap': 'round', 'vector-effect': 'non-scaling-stroke', points: trend }));
    top.append(chart);
  }
  const nums = el('div', { class: 'st-nums' }, ...youNumbers(you.summary, recentForm).map((n) =>
    el('div', {}, el('span', { class: 'k-label', text: n.label }), el('b', { class: n.kind || null, text: n.value }))));
  return el('section', { 'aria-labelledby': 'st-you-h' }, el('div', { class: 'sec-head' }, el('h2', { id: 'st-you-h', text: 'You' }), link),
    el('div', { class: 'glass st-you' }, top, nums));
}

// ---- Your players ----

function playerRow(c, playerId) {
  const face = el('span', { class: 'player-face', 'aria-hidden': 'true' });
  if (c.primary && face.style?.setProperty) face.style.setProperty('--tc', c.primary);
  const letter = () => el('span', { class: 'face letter', text: c.initial || '' });
  const src = assetUrl(c.portrait);
  if (src) { const img = el('img', { class: 'face', alt: '', src }); img.addEventListener('error', () => img.replaceWith(letter()), { once: true }); face.append(img); }
  else face.append(letter());
  const badge = assetUrl(c.logo);
  if (badge) { const img = el('img', { class: 'badge', alt: '', src: badge }); img.addEventListener('error', () => img.remove(), { once: true }); face.append(img); }
  const line = el('div', { class: 'st-line' });
  c.line.forEach((s, i) => {
    if (i) line.append(el('span', { class: 'sep', 'aria-hidden': 'true', text: '·' }));
    line.append(el('span', {}, el('b', { text: s.value }), ` ${s.unit}`));
  });
  const tx = el('div', { class: 'st-ptx' },
    el('div', { class: 'st-phead' }, el('span', { class: 'st-pname', text: c.name }), el('span', { class: 'k-label', text: c.label })),
    c.line.length ? line : null);
  if (!c.line.length) return el('div', { class: 'st-player none' }, face, tx);
  if (!playerId) return el('div', { class: 'st-player' }, face, tx);
  return el('a', { class: 'st-player', href: profileHref(playerId, ROLE_HASH[c.role] || 'career'), 'data-key': `card-${c.role}` }, face, tx);
}

function playersBlock(best, playerId) {
  const links = playerId ? el('nav', { class: 'sec-links', 'aria-label': 'Your profile sections' },
    ...[['Offense', 'career'], ['Defense', 'defense'], ['Records', 'records']].map(([label, hash]) =>
      el('a', { href: profileHref(playerId, hash), 'data-key': `more-${hash}`, text: label }))) : null;
  return el('section', { 'aria-labelledby': 'st-players-h' }, el('div', { class: 'sec-head' }, el('h2', { id: 'st-players-h', text: 'Your players' }), links),
    el('div', { class: 'glass st-plist' }, ...roleCards(best || {}).map((c) => playerRow(c, playerId))));
}

// ---- Profile & sync (the only place the offline-visibility toggle lives, and one of the sign-out buttons) ----

// One action at a time; afterwards the whole page state is read again.
async function act(label, command, args, after) {
  if (busy) return;
  busy = true; state.error = null; draw();
  try { after(await call(command, args)); }
  catch (error) { state.error = `${label}: ${reason(error)}`; }
  await load();
  busy = false;
  draw();
}

function accountBlock() {
  const identity = state.identity, queue = state.queueView;
  const status = !ok(identity) ? 'Account unavailable' : identity.signed_in ? `Signed in as ${identity.display_name || 'your Discord account'}` : 'Not signed in';
  const profile = el('div', {}, el('h3', { text: 'Profile' }), el('p', { text: status }));
  if (!ok(identity)) profile.append(partial('Your account could not be loaded. Try refreshing.'));
  else if (identity.signed_in) {
    profile.append(el('label', { for: 'show-offline' },
      el('input', { type: 'checkbox', id: 'show-offline', checked: !!identity.show_offline, disabled: busy,
        onchange: (event) => act('Offline visibility', 'identity_set_show_offline', { show: !!event.target.checked }, () => { said = 'Profile settings saved.'; }) }),
      el('span', { text: 'Show my offline games on my profile' })),
      el('button', { type: 'button', class: 'btn sm', id: 'stats-sign-out', text: 'Sign out', disabled: busy, onclick: () => act('Sign out', 'identity_sign_out', {}, () => { said = 'Signed out.'; }) }));
  } else profile.append(el('p', { text: 'Connect your Discord account to add these results to your profile.' }), el('a', { href: 'index.html', class: 'btn sm', text: 'Sign in on Play' }));
  const sync = el('div', {}, el('h3', { text: 'Result sync' }));
  if (!ok(queue)) sync.append(partial('Result sync status could not be loaded.'));
  else sync.append(el('p', { text: queue.depth ? `${queue.depth} result${queue.depth === 1 ? ' is' : 's are'} waiting to sync.` : 'No results waiting to sync.' }));
  sync.append(el('p', { text: 'Results sync automatically after each game.' }),
    el('button', { type: 'button', class: 'btn sm', id: 'stats-send', text: 'Send results now', disabled: busy, onclick: () => act('Sync', 'stats_submit', {}, (result) => { said = result?.note || 'Sync complete.'; }) }));
  return el('details', { class: 'st-account' }, el('summary', { text: 'Profile & sync ›' }),
    el('div', { class: 'glass st-account-body' }, profile, sync, said ? el('p', { class: 'said', role: 'status', text: said }) : null));
}

// ---- drawing ----

// Redraws replace everything (every refresh): put the focus back on the same control, by data-key (spec §10).
function draw() {
  if (!screen) return;
  // Guarded: the Node page checker (Tools/check-shell-pages.mjs) draws into a minimal fake DOM without these.
  const active = typeof screen.contains === 'function' && screen.contains(document.activeElement) ? document.activeElement : null;
  const focusKey = active?.getAttribute('data-key') ?? null, focusId = active?.id || null;
  const accountOpen = Boolean(screen.querySelector?.('.st-account')?.open);
  paint(accountOpen);
  if (focusKey) {
    // "Older" on the last page (or "Newer" on the first) is now disabled and can't take focus: use the other one.
    const byKey = (k) => Array.from(screen.querySelectorAll('[data-key]')).find((n) => n.getAttribute('data-key') === k) ?? null;
    let target = byKey(focusKey);
    if (target?.disabled && PAGER_OTHER[focusKey]) target = byKey(PAGER_OTHER[focusKey]) ?? target;
    target?.focus({ preventScroll: true });
  } else if (focusId) document.getElementById(focusId)?.focus({ preventScroll: true });
}

function paint(accountOpen) {
  if (!state.loaded) { screen.replaceChildren(el('p', { class: 'loading', text: 'Loading…' })); return; }
  const account = accountBlock();
  account.open = accountOpen;
  const foot = (note) => el('div', { class: 'st-foot' }, note ? el('span', { class: 'st-queue', text: note }) : null, account);
  const q = queueLine(state.queue);
  const parts = [];
  if (state.error) parts.push(el('p', { class: 'refusal', role: 'alert', text: state.error }));
  if (!state.signedIn) {
    // Signed out: no rows, only the way in (and what is waiting locally). Never answered at all: only the error.
    if (state.answered) parts.push(el('p', { class: 'note', text: SIGNED_OUT }));
    const n = unsentCount(state.queue);
    screen.replaceChildren(...parts, foot(n ? `${n} ${n === 1 ? 'game' : 'games'} will upload when you sign in.` : null));
    return;
  }
  const d = state.dash;
  if (!d) { screen.replaceChildren(...parts, foot(q)); return; }
  // Stats never lists or counts a private room (rulings P4-3, P4-6, P4-22): dropped before anything is drawn.
  const matches = dropPrivate(d.matches);
  if (!matches.length) {
    parts.push(el('p', { class: 'note', text: EMPTY }));
    screen.replaceChildren(...parts, foot(q));
    return;
  }
  const you = d.you || {};
  parts.push(el('div', { class: 'st-cols' }, matchesBlock(matches),
    // Record and Win % are the server's career totals; Last 10 comes from the listed matches (private rooms dropped),
    // not the server's recent_form, which counts private rooms (P4-22).
    el('div', { class: 'st-side' }, youBlock(you, { results: resultsOf(matches) }), playersBlock(d.best_players, you.player_id))));
  screen.replaceChildren(...parts, foot(q));
}

markCurrent();
wireCoreFold();
draw();
load();
// No refresh while a game is running (app-shell.js repeats `session-state` every 1.5 s): a match in progress
// needs the network. When the session ends, one quiet reload picks up the game that just finished.
let sessionActive = false;
document.addEventListener('session-state', (e) => {
  const next = Boolean(e?.detail);
  const ended = sessionActive && !next;
  sessionActive = next;
  if (ended && document.visibilityState !== 'hidden') load(true);
});
setInterval(() => { if (!sessionActive && document.visibilityState !== 'hidden') load(true); }, REFRESH_MS);
