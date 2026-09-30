// The Stats tab: the signed-in player's dashboard from the server (stats_dashboard). Left: your matches (a match of
// one game opens game.html?game=<id>; a longer match expands to its games), recent form and head to head. Right:
// You (Elo, trend, record grid), your players, and the links into your public profile. The local results queue is
// only counted ("N games not uploaded yet").
// Everything here is server data (player-typed team and player names among it): drawn with el()'s `text`
// (textContent) only, never markup. An image only ever comes through assetUrl() (our own /live-assets/ catalog) or
// avatarSrc() (the Discord avatar CDN): the two image prefixes the CSP allows.
import { call, el, markCurrent, partial, quietCall, reason, wireCoreFold } from './shell.js';
import { queueLine, unsentCount } from './stats-rows.js';
import { assetUrl, avatarSrc } from './live-common.js';
import { fmtElo, fmtInt, fmtRecord, matchRowModel, roleCards, trendPoints, youGrid } from './stats-model.js';

const PAGE_SIZE = 8;
const H2H_ROWS = 5;
const REFRESH_MS = 30000;
const SIGNED_OUT = 'Sign in with Discord on the Play page to see your games.';
const EMPTY = 'No games yet. Play one and it shows up here.';
const RESULT_WORD = { W: 'win', L: 'loss', T: 'tie' };
const RESULT_LABEL = { W: 'Win', L: 'Loss', T: 'Tie' };
// Which profile section each player card opens: offense cards the career tables, defense cards the defense table.
const ROLE_HASH = { passer: 'career', rusher: 'career', receiver: 'career', most_played: 'career', dl_lb: 'defense', ballhawk: 'defense' };
const SVG_NS = 'http://www.w3.org/2000/svg';

const state = { signedIn: false, answered: false, dash: null, queue: [], filter: 'all', page: 0, error: null, loaded: false,
  // Which match rows are open (by match id), kept across the 30 s refreshes; and whether head to head shows all rows.
  expanded: new Set(), h2hAll: false,
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

// ---- small pieces ----

const profileHref = (id, hash = '') => `profile.html?player=${encodeURIComponent(id)}${hash ? `#${hash}` : ''}`;
const gameHref = (id) => `game.html?game=${encodeURIComponent(id)}`;
const go = (href) => () => { location.href = href; };

function initial(name) {
  const first = Array.from(String(name || '').trim())[0];
  return first ? first.toUpperCase() : '?';
}

/** A team's logo (22px in rows), or its abbreviation in a small round badge when there is no logo or it fails. */
function teamLogo(t, cls = 'stats-logo') {
  const badge = () => el('span', { class: `${cls} stats-logo-text`, 'aria-hidden': 'true', text: t?.abbrev || initial(t?.name) });
  const src = assetUrl(t?.logo_url);
  if (!src) return badge();
  const img = el('img', { class: cls, alt: '', src, title: t?.name || null });
  img.addEventListener('error', () => img.replaceWith(badge()));
  return img;
}

/** A player's Discord avatar, or the first letter of their name in the same circle. */
function avatar(url, name) {
  const fallback = () => el('span', { class: 'stats-av', 'aria-hidden': 'true', text: initial(name) });
  const src = avatarSrc({ avatar_url: url });
  if (!src) return fallback();
  const img = el('img', { class: 'stats-av', alt: '', src });
  img.addEventListener('error', () => img.replaceWith(fallback()));
  return img;
}

function svg(tag, attrs = {}) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  return node;
}

function section(title, link) {
  return el('h2', { class: 'stats-sec' }, el('span', { text: title }), link || null);
}

// ---- Your matches ----

function filtered(matches) {
  return matches.filter((m) => state.filter === 'all' || (state.filter === 'online' ? m.mode === 'online' : m.mode !== 'online'));
}

function matchLabel(m, r) {
  const teams = `${m.me?.name || 'Your team'} vs ${m.opp?.name || 'their team'}`;
  const who = m.opponent_kind === 'cpu' ? 'against the CPU' : r.opponent !== '—' ? `against ${r.opponent}` : '';
  const score = r.expandable ? `series ${r.score}` : r.score !== '—' ? `score ${r.score}` : 'no score';
  const parts = [`${r.date} ${r.time}`, [teams, who].filter(Boolean).join(', '),
    [score, RESULT_WORD[r.result]].filter(Boolean).join(', ')];
  if (r.tag) parts.push(r.tag);
  if (r.expandable) parts.push(`${r.games.length} games`);
  return `${parts.join('. ')}.`;
}

function matchRow(m) {
  const r = matchRowModel(m);
  const key = `match-${r.id}`;
  const open = r.expandable && state.expanded.has(r.id);
  const gamesId = `games-${String(r.id).replace(/[^A-Za-z0-9_-]/g, '_')}`;
  const opponent = el('span', { class: 'stats-opp' },
    m.opponent_kind === 'cpu' || !m.opponent ? null : avatar(r.avatar, r.opponent),
    el('span', { class: 'stats-opp-name', text: r.opponent }));
  const cells = [
    el('span', { class: 'stats-when' }, el('b', { text: r.date }), el('span', { text: r.time })),
    el('span', { class: 'stats-mu' }, teamLogo(r.me), el('span', { class: 'stats-vs', text: 'vs' }), teamLogo(r.opp), opponent),
    el('span', { class: 'stats-sc', text: r.score }),
    el('span', { class: `stats-res${r.result ? ` ${r.result}` : ''}`, text: r.result }),
    el('span', { class: `stats-badge${r.tagKind ? ` ${r.tagKind}` : ''}`, text: r.tag }),
    el('span', { class: 'stats-caret', 'aria-hidden': 'true', text: r.expandable ? '›' : '' }),
  ];
  const attrs = { class: `stats-row${r.expandable ? ' exp' : ''}`, 'data-key': key, 'aria-label': matchLabel(m, r) };
  let row;
  if (r.expandable) {
    row = el('button', { ...attrs, type: 'button', 'aria-expanded': String(open), 'aria-controls': gamesId,
      onclick: () => { if (state.expanded.has(r.id)) state.expanded.delete(r.id); else state.expanded.add(r.id); draw(); } }, ...cells);
  } else if (r.onlyGame) {
    row = el('button', { ...attrs, type: 'button', onclick: go(gameHref(r.onlyGame)) }, ...cells);
  } else {
    // A match with no recorded game has nothing to open.
    row = el('div', { ...attrs, role: 'group' }, ...cells);
  }
  if (!r.expandable) return [row];
  const games = el('div', { class: 'stats-games', id: gamesId, hidden: open ? null : '' },
    ...r.games.map((g, i) => {
      const label = `Game ${i + 1}. ${g.score !== '—' ? `Score ${g.score}` : 'No score'}${g.result ? `, ${RESULT_WORD[g.result]}` : ''}. ${g.date} ${g.time}.`;
      return el('button', { type: 'button', class: 'stats-row game', 'data-key': `game-${g.id}`, 'aria-label': label, onclick: go(gameHref(g.id)) },
        el('span', { class: 'stats-when' }, el('b', { text: `Game ${i + 1}` })),
        el('span', { class: 'stats-mu' }, teamLogo(g.me), el('span', { class: 'stats-vs', text: 'vs' }), teamLogo(g.opp),
          el('span', { class: 'stats-game-time', text: g.time })),
        el('span', { class: 'stats-sc', text: g.score }),
        el('span', { class: `stats-res${g.result ? ` ${g.result}` : ''}`, text: g.result }),
        el('span', { class: 'stats-badge' }),
        el('span', { class: 'stats-caret' }));
    }));
  return [row, games];
}

function matchesBlock(matches) {
  const out = [section('Your matches')];
  out.push(el('div', { class: 'stats-filters', role: 'group', 'aria-label': 'Show matches' }, ...[['all', 'All'], ['online', 'Online'], ['offline', 'Offline']].map(([k, label]) =>
    el('button', { type: 'button', class: 'stats-filter', 'data-key': `filter-${k}`, 'aria-pressed': String(state.filter === k), text: label,
      onclick: () => { state.filter = k; state.page = 0; draw(); } }))));
  const list = filtered(matches);
  if (!list.length) {
    out.push(el('div', { class: 'stats-glass stats-list' }, el('p', { class: 'stats-empty', text: 'No matches of that kind yet.' })));
    return out;
  }
  const pages = Math.max(1, Math.ceil(list.length / PAGE_SIZE));
  state.page = Math.max(0, Math.min(state.page, pages - 1));
  out.push(el('div', { class: 'stats-glass stats-list' }, ...list.slice(state.page * PAGE_SIZE, (state.page + 1) * PAGE_SIZE).flatMap(matchRow)));
  if (pages > 1) out.push(el('div', { class: 'stats-pager' },
    el('button', { type: 'button', 'data-key': 'newer', text: 'Newer', disabled: state.page === 0, onclick: () => { state.page--; draw(); } }),
    el('span', { text: `Page ${state.page + 1} of ${pages}` }),
    el('button', { type: 'button', 'data-key': 'older', text: 'Older', disabled: state.page >= pages - 1, onclick: () => { state.page++; draw(); } })));
  return out;
}

// ---- Recent form and head to head ----

function formBlock(form) {
  const results = Array.isArray(form?.results) ? form.results.filter((x) => RESULT_LABEL[x]) : [];
  const box = el('div', { class: 'stats-glass stats-mod' }, el('h3', { class: 'stats-label', text: `Recent form · last ${results.length || 10}` }));
  if (!results.length) { box.append(el('p', { class: 'stats-empty', text: 'No finished matches yet.' })); return box; }
  box.append(el('div', { class: 'stats-dots', role: 'list', 'aria-label': 'Newest first' },
    ...results.map((x) => el('span', { class: `stats-dot ${x}`, role: 'listitem', 'aria-label': RESULT_LABEL[x], text: x }))));
  box.append(el('p', { class: 'stats-pts' },
    el('span', { text: 'Points for ' }), el('b', { class: 'for', text: fmtInt(form.pts_for ?? null) }),
    el('span', { text: ' · against ' }), el('b', { class: 'against', text: fmtInt(form.pts_against ?? null) })));
  return box;
}

function h2hBlock(rows) {
  const list = Array.isArray(rows) ? rows : [];
  const box = el('div', { class: 'stats-glass stats-mod' });
  const head = el('div', { class: 'stats-mod-head' }, el('h3', { class: 'stats-label', text: 'Head to head' }));
  if (list.length > H2H_ROWS) head.append(el('button', { type: 'button', class: 'stats-link', 'data-key': 'h2h-all', 'aria-expanded': String(state.h2hAll),
    text: state.h2hAll ? 'Fewer ‹' : 'All ›', 'aria-label': state.h2hAll ? 'Show fewer opponents' : `Show all ${list.length} opponents`,
    onclick: () => { state.h2hAll = !state.h2hAll; draw(); } }));
  box.append(head);
  if (!list.length) { box.append(el('p', { class: 'stats-empty', text: 'No matches against other players yet.' })); return box; }
  for (const h of state.h2hAll ? list : list.slice(0, H2H_ROWS)) {
    const wins = h.wins || 0, losses = h.losses || 0, ties = h.ties || 0, total = wins + losses + ties;
    const name = h.display_name || 'Player';
    const who = h.player_id
      ? el('a', { class: 'stats-rv-name', href: profileHref(h.player_id), 'data-key': `h2h-${h.player_id}` }, avatar(h.avatar_url, name), el('span', { text: name }))
      : el('span', { class: 'stats-rv-name' }, avatar(h.avatar_url, name), el('span', { text: name }));
    const share = total ? Math.round((wins / total) * 100) : 0;
    const desc = `${wins} ${wins === 1 ? 'win' : 'wins'}, ${losses} ${losses === 1 ? 'loss' : 'losses'}${ties ? `, ${ties} ${ties === 1 ? 'tie' : 'ties'}` : ''}`;
    const fill = el('i');
    fill.style.width = `${share}%`;
    const bar = el('span', { class: 'stats-bar', role: 'img', 'aria-label': desc }, fill);
    box.append(el('div', { class: 'stats-rv' }, who, el('b', { class: 'stats-rv-rec', 'aria-label': `Record ${desc}`, text: fmtRecord({ wins, losses, ties }) }), bar));
  }
  return box;
}

// ---- You ----

function youBlock(you) {
  const e = fmtElo(you.elo);
  const delta = you.elo?.last_delta;
  const trend = trendPoints(Array.isArray(you.elo?.history) ? you.elo.history : []);
  const link = you.player_id ? el('a', { class: 'stats-link', href: profileHref(you.player_id), 'data-key': 'profile', text: 'Your profile ›' }) : null;
  const ed = el('span', { class: 'stats-ed' });
  if (e.change) ed.append(el('span', { class: delta > 0 ? 'up' : 'down', 'aria-label': `${delta > 0 ? 'Up' : 'Down'} ${Math.abs(delta)} since your last ranked match`, text: e.change }));
  if (e.change && e.rank) ed.append(el('span', { 'aria-hidden': 'true', text: ' · ' }));
  if (e.rank) ed.append(el('span', { text: e.rank }));
  const top = el('div', { class: 'stats-ytop' },
    el('div', {}, el('span', { class: 'stats-label', text: 'Elo' }),
      el('span', { class: `stats-ev${you.elo?.rating == null ? ' unranked' : ''}`, text: e.value }), ed));
  if (trend) {
    const n = you.elo.history.length;
    const chart = svg('svg', { viewBox: '0 0 160 50', preserveAspectRatio: 'none', role: 'img', 'aria-label': `Elo trend over your last ${n} ranked matches` });
    chart.append(svg('polyline', { fill: 'none', stroke: 'currentColor', 'stroke-width': 2, 'vector-effect': 'non-scaling-stroke', points: trend }));
    top.append(chart);
  }
  const grid = el('div', { class: 'stats-ygrid' }, ...youGrid(you.summary || {}, you.plays_as || null).map((c) => {
    const v = el('b', { text: c.value });
    if (c.logo) { const src = assetUrl(c.logo); if (src) { const img = el('img', { alt: '', src }); img.addEventListener('error', () => img.remove()); v.prepend(img, ' '); } }
    return el('div', {}, el('span', { class: 'stats-label', text: c.label }), v);
  }));
  return [section('You', link), el('div', { class: 'stats-glass stats-youm' }, top, grid)];
}

function playerCard(c, playerId) {
  const face = el('span', { class: 'stats-face' });
  const src = assetUrl(c.portrait);
  const letter = () => el('span', { class: 'stats-face-initial', 'aria-hidden': 'true', text: c.initial || '' });
  if (src) { const img = el('img', { class: 'stats-face-img', alt: '', src }); img.addEventListener('error', () => img.replaceWith(letter())); face.append(img); }
  else face.append(letter());
  const badge = assetUrl(c.logo);
  if (badge) { const img = el('img', { class: 'stats-face-badge', alt: c.team || '', src: badge }); img.addEventListener('error', () => img.remove()); face.append(img); }
  const text = el('span', { class: 'stats-lead-text' }, el('span', { class: 'stats-label', text: c.label }),
    el('span', { class: 'stats-lead-n', text: c.name }), c.stat ? el('span', { class: 'stats-lead-s', text: c.stat }) : null);
  if (!c.stat || !playerId) return el('div', { class: 'stats-glass stats-lead none' }, face, text);
  return el('a', { class: 'stats-glass stats-lead', href: profileHref(playerId, ROLE_HASH[c.role] || 'career'), 'data-key': `card-${c.role}` }, face, text);
}

function playersBlock(best, playerId) {
  return [section('Your players'),
    el('div', { class: 'stats-pgrid' }, ...roleCards(best || {}).map((c) => playerCard(c, playerId))),
    playerId ? el('nav', { class: 'stats-more', 'aria-label': 'Your profile sections' },
      ...[['Offense', 'career'], ['Defense', 'defense'], ['Records', 'records']].map(([label, hash]) =>
        el('a', { href: profileHref(playerId, hash), 'data-key': `more-${hash}`, text: label }))) : null];
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

// ---- drawing ----

// Redraws replace everything (every refresh): put the focus back on the same control, as live.js does.
function draw() {
  if (!screen) return;
  // Guarded: the Node page checker (Tools/check-shell-pages.mjs) draws into a minimal fake DOM without these.
  const active = typeof screen.contains === 'function' && screen.contains(document.activeElement) ? document.activeElement : null;
  const focusKey = active?.getAttribute('data-key') ?? null, focusId = active?.id || null;
  const accountOpen = Boolean(screen.querySelector?.('.stats-account')?.open);
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
  const d = state.dash;
  if (!d) { screen.replaceChildren(...parts, account); return; }
  const matches = Array.isArray(d.matches) ? d.matches : [];
  if (!matches.length) {
    parts.push(el('p', { class: 'note', text: EMPTY }));
    if (q) parts.push(el('p', { class: 'note', text: q }));
    screen.replaceChildren(...parts, account);
    return;
  }
  const you = d.you || {};
  const left = el('div', { class: 'stats-col' }, ...matchesBlock(matches),
    el('div', { class: 'stats-mods' }, formBlock(d.recent_form), h2hBlock(d.head_to_head)));
  const right = el('div', { class: 'stats-col' }, ...youBlock(you), ...playersBlock(d.best_players, you.player_id),
    q ? el('p', { class: 'note stats-queue', text: q }) : null);
  parts.push(el('div', { class: 'stats-two' }, left, right));
  screen.replaceChildren(...parts, account);
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
