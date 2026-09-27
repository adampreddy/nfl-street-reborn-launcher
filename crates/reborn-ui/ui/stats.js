// The Stats tab: the signed-in player's games from the server (stats_my_games), each opening the native game
// view (game.html?game=<id>). The local results queue is only counted ("N games not uploaded yet").
// Team names are player-typed server data: drawn with el()'s `text` (textContent) only, and a logo only ever
// through assetUrl() (our own /live-assets/ catalog, the one image prefix the CSP allows besides Discord's).
import { call, el, markCurrent, partial, quietCall, reason, wireCoreFold } from './shell.js';
import { rowModel, queueLine, unsentCount } from './stats-rows.js';
import { assetUrl } from './live-common.js';

const PAGE_SIZE = 6;
const REFRESH_MS = 30000;
const SIGNED_OUT = 'Sign in with Discord on the Play page to see your games.';
const EMPTY = 'No games yet. Play one and it shows up here.';
const state = { signedIn: false, answered: false, games: [], queue: [], filter: 'all', page: 0, error: null, loaded: false,
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
  const [games, queue, identity] = await Promise.allSettled([ask('stats_my_games', { limit: 200 }), ask('stats_queue'), ask('identity_status')]);
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
  state.queueView = queue.status === 'fulfilled' ? queue.value : { error: reason(queue.reason) };
  state.identity = identity.status === 'fulfilled' ? identity.value : { error: reason(identity.reason) };
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

// ---- Profile & sync (the only place the offline-visibility toggle lives, and one of the sign-out buttons) ----

// One action at a time; afterwards the whole page state is read again, as the old Stats page did.
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
  const status = !ok(identity) ? "Account unavailable" : identity.signed_in ? `Signed in as ${identity.display_name || "your Discord account"}` : "Not signed in";
  const details = el("details", { class: "stats-account" }, el("summary", {},
    el("span", { text: "Profile & sync" }), el("span", { class: "stats-account-status", text: status })));
  const profile = el("div", { class: "stats-account-section" }, el("h3", { text: "Profile" }));
  if (!ok(identity)) profile.append(partial("Your account could not be loaded. Try refreshing."));
  else if (identity.signed_in) {
    profile.append(el("label", { class: "stats-visibility", for: "show-offline" },
      el("input", { type: "checkbox", id: "show-offline", checked: !!identity.show_offline, disabled: busy,
        onchange: event => act("Offline visibility", "identity_set_show_offline", { show: !!event.target.checked }, () => { said = "Profile settings saved."; }) }),
      el("span", { text: "Show my offline games on my profile" })),
      el("button", { type: "button", id: "stats-sign-out", text: "Sign out", disabled: busy, onclick: () => act("Sign out", "identity_sign_out", {}, () => { said = "Signed out."; }) }));
  } else profile.append(el("p", { text: "Connect your Discord account to add these results to your profile." }), el("a", { href: "index.html", class: "button", text: "Sign in on Play" }));
  const sync = el("div", { class: "stats-account-section" }, el("h3", { text: "Result sync" }));
  if (!ok(queue)) sync.append(partial("Result sync status could not be loaded."));
  else sync.append(el("p", { text: queue.depth ? `${queue.depth} result${queue.depth === 1 ? " is" : "s are"} waiting to sync.` : "No results waiting to sync." }));
  sync.append(el("p", { class: "stats-secondary", text: "Results sync automatically after each game." }),
    el("button", { type: "button", id: "stats-send", text: "Send results now", disabled: busy, onclick: () => act("Sync", "stats_submit", {}, result => { said = result?.note || "Sync complete."; }) }));
  details.append(el("div", { class: "stats-account-grid" }, profile, sync));
  if (said) details.append(el("p", { class: "stats-sync-note", role: "status", text: said }));
  return details;
}

// Redraws replace the rows (every refresh): put the focus back on the same control, as live.js does.
function draw() {
  if (!screen) return;
  const active = screen.contains(document.activeElement) ? document.activeElement : null;
  const focusKey = active?.getAttribute('data-key') ?? null, focusId = active?.id || null;
  const accountOpen = Boolean(screen.querySelector('.stats-account')?.open);
  paint(accountOpen);
  if (focusKey) { for (const n of screen.querySelectorAll('[data-key]')) if (n.getAttribute('data-key') === focusKey) { n.focus({ preventScroll: true }); break; } }
  else if (focusId) document.getElementById(focusId)?.focus({ preventScroll: true });
}

function paint(accountOpen) {
  if (!state.loaded) { screen.replaceChildren(el('p', { class: 'loading', text: 'Loading…' })); return; }
  const account = accountBlock();
  account.open = accountOpen;
  const parts = [];
  const q = queueLine(state.queue);
  if (state.error) parts.push(el('p', { class: 'refusal', role: 'alert', text: state.error }));
  if (!state.signedIn) {
    // Signed out: no rows, only the way in (and what is waiting locally). Never answered at all: only the error.
    if (state.answered) parts.push(el('p', { class: 'note', text: SIGNED_OUT }));
    const n = unsentCount(state.queue);
    if (n) parts.push(el('p', { class: 'note', text: `${n} ${n === 1 ? 'game' : 'games'} will upload when you sign in.` }));
    screen.replaceChildren(...parts, account);
    return;
  }
  if (q) parts.push(el('p', { class: 'note', text: q }));
  parts.push(el('div', { class: 'stats-filters', role: 'group', 'aria-label': 'Show' }, ...[['all', 'All'], ['online', 'Online'], ['offline', 'Offline']].map(([k, label]) =>
    el('button', { type: 'button', class: `stats-filter${state.filter === k ? ' current' : ''}`, 'data-key': `filter-${k}`, 'aria-pressed': String(state.filter === k), text: label,
      onclick: () => { state.filter = k; state.page = 0; draw(); } }))));
  const list = filtered();
  if (!list.length) {
    parts.push(el('p', { class: 'note', text: state.games.length ? 'No games of that kind yet.' : EMPTY }));
    screen.replaceChildren(...parts, account);
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
  screen.replaceChildren(...parts, account);
}

markCurrent();
wireCoreFold();
draw();
load();
setInterval(() => { if (document.visibilityState !== 'hidden') load(true); }, REFRESH_MS);
