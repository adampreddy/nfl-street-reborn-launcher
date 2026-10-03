// The Stats tab (spec §8.6): the signed-in player's own dashboard (`stats_dashboard`), in two views picked at the top
// right (owner 10-03): Matches | Records. Laid out as the approved mv2 mockup (owner 10-03, "I like the mockup"): one
// 12-column grid whose rows' cards share their top and bottom edges.
//   Matches: Your Record (online only: overall, Ranked and Casual halves, Last 10) beside Elo (the curve, or a clean
//   Unranked state); Your matches (All / Ranked / Casual / Offline, a queue column; a row opens the whole match,
//   stats-model.js matchHref) beside Your players (five numbers per role); then the charts the data allows: Points per
//   game, Yards split, Results by day, Win rate by team, Team per game, Wins vs losses. A card whose data the server
//   does not send is left out and its row partner takes the width (rowSpans). Under it all, one line: the local results
//   queue on the left and Profile & sync on the right (offline visibility, sign out, send results now).
//   Records: Single game and Career side by side, Team records under them (records-view.js).
// The dashboard sends the record, the matches and the players; per game, by team, wins vs losses and the records are
// the player's own profile's (`player_profile`, read once after the dashboard and again after a game ends).
// Everything here is server data (player-typed team and player names among it): drawn with el()'s `text`
// (textContent) only, never markup. Images only through assetUrl() (our /live-assets/ catalog) or avatarSrc() (the
// Discord avatar CDN), the two image prefixes the CSP allows. Script-set styles are custom properties (CSSOM).
import { call, el, markCurrent, partial, quietCall, reason, wireCoreFold } from './shell.js';
import { queueLine, unsentCount } from './stats-rows.js';
import { dropPrivate, eloCard, lastTenStrip, pointsSeries, recordCard, resultsByDay, rowSpans, teamPerGame, teamRates, winsLosses, yardsSplit } from './stats-model.js';
import { matchHeader, matchRows, pager, teamLogo } from './match-rows.js';
import { careerRecords, playerRecords, teamRecords } from './profile-model.js';
import { recordsView } from './records-view.js';
import { card, cellGrid, cols, dayColumns, donut, eloLine, pointsBars, playerRows, share, wlStrip } from './stats-cards.js';
import { tabs } from './tabs.js';
import { createSignIn } from './sign-in.js';

const PAGE_SIZE = 8;
const signIn = createSignIn({ onChange: () => draw(), onDone: () => load() });
const REFRESH_MS = 30000;
const EMPTY = 'No games yet. Play one and it shows up here.';
// Only private rooms came back (dropped, P4-3): they did play, so never "No games yet".
const NONE_SHOWN = 'No games to show yet.';
const FILTERS = [['all', 'All'], ['ranked', 'Ranked'], ['casual', 'Casual'], ['offline', 'Offline']];
// Which part of the profile each player opens: their role's Full stats table.
const PAGER_OTHER = { older: 'newer', newer: 'older' };
const VIEWS = [['matches', 'Matches'], ['records', 'Records']];
const viewOfHash = () => (typeof location !== 'undefined' && location.hash === '#records' ? 'records' : 'matches');

const state = { signedIn: false, answered: false, dash: null, queue: [], filter: 'all', page: 0, error: null, loaded: false,
  // Profile & sync: the identity_status and stats_queue answers, or { error } when one failed.
  identity: null, queueView: null,
  // Matches | Records (the address's #records), and the player's own profile the lower cards and Records read.
  view: viewOfHash(), profile: null, profileFor: null, profileBusy: false, profileError: null, profileStale: false };
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
const here = () => (state.view === 'records' ? 'stats.html#records' : 'stats.html');
const list = (v) => (Array.isArray(v) ? v : []);
/** The profile's answer when it is this player's; the dashboard's own field wins when a later server sends it. */
const fromProfile = (d, key) => d?.[key] ?? (state.profileFor === d?.you?.player_id ? state.profile?.[key] : undefined);

// ---- Your Record and Elo (row 1) ----

function half(label, kind, t, firstRow) {
  return el('div', { class: 'st-half', role: 'group', 'aria-label': `${label}: ${t.text}, ${t.rate} win rate` },
    el('span', { class: `k-label qm ${kind}`, text: label }), el('div', { class: 'vc-mid', text: t.text }),
    el('div', { class: 'vc-sub' }, el('b', { text: t.rate }), ' win rate'), share(t.share ?? 0),
    el('div', { class: 'vc-kv' }, el('span', { text: firstRow[0] }), el('b', { text: firstRow[1] })),
    el('div', { class: 'vc-kv' }, el('span', { text: 'Points / game' }), el('b', { text: t.pts })));
}

function recordBlock(d, matches) {
  const r = recordCard(d.by_queue, matches);
  if (!r) return null;
  const e = eloCard(d.you?.elo);
  const tenth = lastTenStrip(matches);
  const overall = el('div', { class: 'st-half st-overall' }, el('span', { class: 'k-label', text: 'Online overall' }), el('div', { class: 'vc-big', text: r.text }),
    el('div', { class: 'vc-sub' }, el('b', { text: r.rate }), ' win rate'),
    el('div', { class: 'vc-sub st-streak' }, 'Streak ', el('b', { class: tenth?.streakKind || null, text: tenth?.streak ?? '—' }), ` · ${r.games} ${r.games === 1 ? 'game' : 'games'}`));
  const ranked = half('Ranked', 'ranked', r.ranked, e.state === 'rated' ? ['Elo', [e.value, e.rank].filter(Boolean).join(' · ')] : ['Games', String(r.ranked.games)]);
  const casual = half('Casual', 'casual', r.casual, ['Games', String(r.casual.games)]);
  const foot = tenth ? el('div', { class: 'st-l10' }, el('span', { class: 'k-label', text: 'Last 10' }), wlStrip(tenth.results),
    el('div', { class: 'st-l10-sum' }, el('b', { text: tenth.text }), tenth.scored ? ` · ${tenth.scored} scored` : null, tenth.allowed ? el('br') : null, tenth.allowed ? `${tenth.allowed} allowed` : null)) : null;
  return card('st-record', 'Your Record', [el('div', { class: 'st-rec' }, overall, ranked, casual), foot], { aside: 'Online games only' });
}

function eloBlock(d) {
  const e = eloCard(d.you?.elo);
  if (e.state !== 'rated') {
    return card('st-elo', 'Elo', el('div', { class: 'st-unranked' }, el('div', { class: 'vc-mid', text: e.title }), el('p', { class: 'vc-sub', text: e.note })),
      { aside: e.state === 'off' ? 'Ranked' : 'Ranked · no games yet', cls: 'st-elo-card' });
  }
  const meta = el('div', { class: 'st-elo-meta' },
    ...[['Rank', e.rank, ''], ['Peak', e.peak, ''], ['This week', e.week?.text, e.week?.kind]].filter(([, v]) => v)
      .map(([k, v, kind]) => el('div', {}, el('span', { text: k }), el('b', { class: kind || null, text: v }))));
  const chart = eloLine(e.values, { labels: e.labels, peak: e.peakValue, label: `Your Elo over your last ${e.values.length - 1} ranked matches` });
  return card('st-elo', 'Elo', [el('div', { class: 'st-elo-top' }, el('div', { class: 'vc-mid st-elo-v', text: e.value }), meta),
    chart || el('p', { class: 'vc-sub st-elo-wait', text: 'Your Elo curve starts after your first ranked match.' })],
  { aside: e.games ? `Ranked · ${e.games} ${e.games === 1 ? 'game' : 'games'}` : 'Ranked', cls: 'st-elo-card' });
}

// ---- Your matches and Your players (row 2) ----

function filtered(matches) {
  const f = state.filter;
  return matches.filter((m) => f === 'all' || (f === 'offline' ? m.mode !== 'online' : m.mode === 'online' && m.queue === f));
}

function matchesBlock(matches) {
  const bar = tabs('stats-matches', FILTERS.map(([, label]) => label), FILTERS.find(([k]) => k === state.filter)[1],
    (label) => { state.filter = FILTERS.find(([, l]) => l === label)[0]; state.page = 0; draw(); }, { label: 'Show matches' });
  bar.classList?.add('sm');
  if (typeof bar.querySelectorAll === 'function') bar.querySelectorAll('.tab').forEach((b, i) => b.setAttribute('data-key', `filter-${FILTERS[i][0]}`));
  const box = el('div', { class: 'm-list with-q st-list' }, matchHeader({ queue: true }));
  const shown = filtered(matches);
  if (!shown.length) box.append(el('p', { class: 'm-empty', text: 'No matches of that kind yet.' }));
  else {
    const pages = Math.max(1, Math.ceil(shown.length / PAGE_SIZE));
    state.page = Math.max(0, Math.min(state.page, pages - 1));
    box.append(...shown.slice(state.page * PAGE_SIZE, (state.page + 1) * PAGE_SIZE).flatMap((m) => matchRows(m, { where: 'stats', from: here(), queue: true })));
    if (pages > 1) box.append(pager({ page: state.page, pages, onNewer: () => { state.page--; draw(); }, onOlder: () => { state.page++; draw(); }, olderDisabled: state.page >= pages - 1 }));
  }
  // data-nav="column": Down from the last row is the pager at the card's foot.
  const c = card('st-matches', 'Your matches', box, { aside: bar, flush: true });
  c.setAttribute('data-nav', 'column');
  return c;
}

function playersBlock(best, playerId) {
  // A player opens the profile at its top (Adam, 10-03: deep links into Full stats read as the old page).
  const rows = playerRows(best, { href: playerId ? () => profileHref(playerId) : null });
  if (!rows.length) return null;
  const link = playerId ? el('a', { class: 'sec-link', href: profileHref(playerId, 'stats'), 'data-key': 'all-players', text: 'All players ›' }) : null;
  return card('st-players', 'Your players', el('div', { class: 'vc-rows' }, ...rows), { aside: link, flush: true });
}

// ---- the charts (rows 3 to 5): each only from data that exists ----

function pointsBlock(matches) {
  const s = pointsSeries(matches);
  const chart = pointsBars(s);
  if (!chart) return null;
  const legend = el('div', { class: 'ch-legend' }, el('span', {}, el('i', { class: 'ch-key-red' }), `Scored · ${s.scored}`),
    el('span', {}, el('i', { class: 'ch-key-grey' }), `Allowed · ${s.allowed}`), el('span', { class: 'mut', text: `Last ${s.games.length} online games` }));
  return card('st-points', 'Points per game', chart, { aside: legend });
}

function yardsBlock(pg) {
  const y = yardsSplit(pg);
  const ring = donut(y);
  if (!ring) return null;
  const part = (cls, word, v, pct) => el('div', {}, el('div', { class: 'ch-legend' }, el('span', {}, el('i', { class: cls }), word)),
    el('div', { class: 'st-yv' }, v, el('small', { text: pct })), el('div', { class: 'st-yn', text: 'Yards / game' }));
  return card('st-yards', 'Yards split', el('div', { class: 'st-donut' }, ring, el('div', { class: 'st-yparts' },
    part('ch-key-red', 'Passing', y.pass, y.passPct), part('ch-key-grey', 'Rushing', y.rush, y.rushPct))), { aside: 'Per game' });
}

function daysBlock(matches) {
  const r = resultsByDay(matches);
  const chart = dayColumns(r);
  if (!chart) return null;
  const legend = el('div', { class: 'ch-legend st-days-legend' }, el('span', {}, el('i', { class: 'ch-key-W' }), 'Win'), el('span', {}, el('i', { class: 'ch-key-L' }), 'Loss'),
    r.best ? el('span', { class: 'mut', text: `Best day ${r.best}` }) : null);
  return card('st-days', 'Results by day', [chart, legend], { aside: `Last ${r.days.length} days` });
}

function teamsBlock(byTeam) {
  const rows = teamRates(byTeam);
  if (!rows.length) return null;
  const COLS = '120px minmax(0,1fr) 42px 40px';
  return card('st-teams', 'Win rate by team', el('div', { class: 'vc-rows st-teams' }, ...rows.map((t) => cols(el('div', { class: 'vc-tr', role: 'group', 'aria-label': `${t.name}: ${t.record}, ${t.pct} wins` },
    el('span', { class: 'st-team' }, teamLogo(t.team), el('b', { text: t.name })), share(t.share, 'red'), el('span', { class: 'r num', text: t.record }), el('span', { class: 'r num st-pct', text: t.pct })), COLS))),
  { aside: 'Online games', flush: true });
}

function perGameBlock(pg) {
  const cells = teamPerGame(pg);
  if (!cells.length) return null;
  const n = Number.isFinite(pg?.games) ? pg.games : null;
  return card('st-pergame', 'Team per game', cellGrid(cells, Math.min(4, cells.length)), { aside: n ? `${n} ${n === 1 ? 'game' : 'games'}` : null, flush: true });
}

function winsLossesBlock(wvl) {
  const w = winsLosses(wvl);
  if (!w) return null;
  return card('st-wl', 'Wins vs losses', el('div', { class: 'st-cmp' },
    el('div', { class: 'st-cmp-r st-cmp-h' }, el('b', { class: 'up', text: `In ${w.wins} ${w.wins === 1 ? 'win' : 'wins'}` }), el('span'), el('b', { class: 'down', text: `In ${w.losses} ${w.losses === 1 ? 'loss' : 'losses'}` })),
    ...w.rows.map((r) => el('div', { class: 'st-cmp-r', role: 'group', 'aria-label': `${r.label}: ${r.win} in wins, ${r.loss} in losses` },
      el('b', { class: 'num', text: r.win }), el('span', { text: r.label }), el('b', { class: 'num', text: r.loss })))));
}

/** One grid row from [[card, span], ...]; null when no card of it has data. */
function gridRow(cards) {
  const present = rowSpans(cards);
  if (!present.length) return null;
  for (const [node, span] of present) node.className = node.className.replace(/\bs\d+\b/, `s${span}`);
  return el('div', { class: 'vgrid' }, ...present.map(([node]) => node));
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
  } else profile.append(el('p', { text: 'Connect your Discord account to add these results to your profile.' }), el('button', { type: 'button', class: 'btn sm', 'data-key': 'signin-foot', text: 'Sign in with Discord', disabled: signIn.state.starting || signIn.state.active, onclick: () => signIn.start() }));
  const sync = el('div', {}, el('h3', { text: 'Result sync' }));
  if (!ok(queue)) sync.append(partial('Result sync status could not be loaded.'));
  else sync.append(el('p', { text: queue.depth ? `${queue.depth} result${queue.depth === 1 ? ' is' : 's are'} waiting to sync.` : 'No results waiting to sync.' }));
  sync.append(el('p', { text: 'Results sync automatically after each game.' }),
    el('button', { type: 'button', class: 'btn sm', id: 'stats-send', text: 'Send results now', disabled: busy, onclick: () => act('Sync', 'stats_submit', {}, (result) => { said = result?.note || 'Sync complete.'; }) }));
  return el('details', { class: 'st-account' }, el('summary', { text: 'Profile & sync ›' }),
    el('div', { class: 'glass st-account-body' }, profile, sync, said ? el('p', { class: 'said', role: 'status', text: said }) : null));
}

// ---- the player's own profile (per game, by team, wins vs losses, records), read once, again after a game ends ----

async function loadProfile(playerId) {
  if (!playerId || state.profileBusy || (state.profileFor === playerId && state.profile && !state.profileStale)) return;
  state.profileBusy = true; state.profileError = null; state.profileStale = false;
  try {
    const p = await quietCall('player_profile', { playerId });
    if (!p || typeof p !== 'object' || p.ok === false) throw new Error(typeof p?.error === 'string' && p.error ? p.error : 'Your records could not be loaded.');
    state.profile = p; state.profileFor = playerId;
  } catch (e) { state.profileError = reason(e); }
  state.profileBusy = false;
  draw();
}

function recordsBlock(d) {
  const you = d.you || {};
  if (state.profileFor !== you.player_id || !state.profile) {
    if (!state.profileError) loadProfile(you.player_id);
    return [state.profileError ? el('p', { class: 'refusal', role: 'alert', text: state.profileError }) : el('p', { class: 'loading', text: 'Loading…' })];
  }
  // A game just ended: read the records again, the old ones staying on screen (and the focus with them) meanwhile.
  if (state.profileStale) loadProfile(you.player_id);
  const p = state.profile;
  // "vs <opponent>": the record names its game only; the opponent comes from the matches this page has.
  const matches = [...dropPrivate(d.matches), ...dropPrivate(p.recent_matches)];
  const parts = recordsView({ single: playerRecords(p.records, matches), career: careerRecords(p.career), team: teamRecords(p.records, matches) },
    { from: 'stats.html#records', aside: 'Online games' });
  if (!parts.length) return [el('p', { class: 'note', text: 'No records yet. Play an online game and they show up here.' })];
  return [el('section', { class: 'st-records', 'aria-label': 'Records' }, ...parts)];
}

/** Matches | Records at the top right of the page (beside the title). Signed out, or before an answer, it is not drawn. */
function drawViewSwitch(show) {
  const slot = document.getElementById?.('stats-view');
  if (!slot) return;
  if (!show) { slot.replaceChildren?.(); return; }
  const bar = tabs('stats-view', VIEWS.map(([, l]) => l), VIEWS.find(([k]) => k === state.view)[1], (label) => {
    state.view = VIEWS.find(([, l]) => l === label)[0];
    try { history.replaceState(history.state, '', state.view === 'records' ? '#records' : location.pathname + location.search); } catch { /* the address is a convenience */ }
    draw();
    slot.querySelector?.('[aria-pressed="true"]')?.focus({ preventScroll: true });
  }, { label: 'Stats view' });
  if (typeof bar.querySelectorAll === 'function') bar.querySelectorAll('.tab').forEach((b, i) => b.setAttribute('data-key', `view-${VIEWS[i][0]}`));
  // The player's own profile, at the top of the page beside the view switch (Adam, 10-03).
  const me = state.dash?.you?.player_id;
  const mine = me ? el('a', { class: 'btn sm st-profile-link', href: profileHref(me), 'data-key': 'your-profile', text: 'Your profile ›' }) : null;
  slot.replaceChildren(...[mine, bar].filter(Boolean));
}

// ---- drawing ----

// Signed out: a big sign-in card. While waiting for Discord, the code and the address to approve it.
function signInCard(waiting) {
  const si = signIn.state;
  const box = el('section', { class: 'glass st-signin', 'aria-label': 'Sign in' });
  if (si.active) {
    box.append(...[el('h2', { text: 'Waiting for Discord' }),
      el('p', { class: 'st-signin-code', role: 'status', text: `Your code is ${si.userCode}` }),
      si.verifyUrl ? el('p', { text: `Go to ${si.verifyUrl} to approve it.` }) : null,
      el('button', { type: 'button', class: 'btn', 'data-key': 'signin-cancel', text: 'Cancel', onclick: () => signIn.cancel() })].filter(Boolean));
  } else {
    box.append(...[el('h2', { text: 'Sign in to see your stats' }),
      el('p', { text: 'Connect your Discord account to play online and track your stats!' }),
      el('button', { type: 'button', class: 'btn primary st-signin-go', 'data-key': 'signin', text: si.starting ? 'Opening Discord…' : 'Sign in with Discord', disabled: si.starting, onclick: () => signIn.start() }),
      si.message ? el('p', { class: 'refusal', role: 'alert', text: si.message }) : null,
      waiting ? el('p', { class: 'st-signin-note', text: waiting }) : null].filter(Boolean));
  }
  return box;
}

// Redraws replace everything (every refresh): put the focus back on the same control, by data-key (spec §10).
function draw() {
  if (!screen) return;
  // Guarded: the Node page checker (Tools/check-shell-pages.mjs) draws into a minimal fake DOM without these.
  const slot = document.getElementById?.('stats-view');
  const inSwitch = Boolean(slot && typeof slot.contains === 'function' && slot.contains(document.activeElement));
  const active = typeof screen.contains === 'function' && screen.contains(document.activeElement) ? document.activeElement : null;
  const focusKey = active?.getAttribute('data-key') ?? null, focusId = active?.id || null;
  const accountOpen = Boolean(screen.querySelector?.('.st-account')?.open);
  paint(accountOpen);
  if (inSwitch) slot.querySelector?.('[aria-pressed="true"]')?.focus({ preventScroll: true });
  else if (focusKey) {
    // "Older" on the last page (or "Newer" on the first) is now disabled and can't take focus: use the other one.
    const byKey = (k) => Array.from(screen.querySelectorAll('[data-key]')).find((n) => n.getAttribute('data-key') === k) ?? null;
    let target = byKey(focusKey);
    if (target?.disabled && PAGER_OTHER[focusKey]) target = byKey(PAGER_OTHER[focusKey]) ?? target;
    target?.focus({ preventScroll: true });
  } else if (focusId) document.getElementById(focusId)?.focus({ preventScroll: true });
}

function paint(accountOpen) {
  drawViewSwitch(state.loaded && state.signedIn && Boolean(state.dash) && dropPrivate(state.dash.matches).length > 0);
  if (!state.loaded) { screen.replaceChildren(el('p', { class: 'loading', text: 'Loading…' })); return; }
  const account = accountBlock();
  account.open = accountOpen;
  const foot = (note) => el('div', { class: 'st-foot' }, note ? el('span', { class: 'st-queue', text: note }) : null, account);
  const q = queueLine(state.queue);
  const parts = [];
  if (state.error) parts.push(el('p', { class: 'refusal', role: 'alert', text: state.error }));
  if (!state.signedIn) {
    // Signed out: no rows, only the way in (and what is waiting locally). Never answered at all: only the error.
    const n = unsentCount(state.queue);
    const waiting = n ? `${n} ${n === 1 ? 'game' : 'games'} will upload when you sign in.` : null;
    // The account is signed in but the stats server says otherwise: keep Profile & sync (sign out, send results).
    if (ok(state.identity) && state.identity.signed_in) { screen.replaceChildren(...parts, foot(waiting)); return; }
    // No account yet: only the sign-in card, no Profile & sync. What is waiting locally goes in the card.
    if (state.answered) parts.push(signInCard(waiting));
    screen.replaceChildren(...parts);
    return;
  }
  const d = state.dash;
  if (!d) { screen.replaceChildren(...parts, foot(q)); return; }
  // Stats never lists or counts a private room (rulings P4-3, P4-6, P4-22): dropped before anything is drawn.
  const matches = dropPrivate(d.matches);
  if (!matches.length) {
    parts.push(el('p', { class: 'note', text: list(d.matches).length ? NONE_SHOWN : EMPTY }));
    screen.replaceChildren(...parts, foot(q));
    return;
  }
  if (state.view === 'records') { screen.replaceChildren(...parts, ...recordsBlock(d)); return; }
  const you = d.you || {};
  // The lower cards read the player's own profile: ask once (and again after a game), draw what is here meanwhile.
  if (you.player_id && (state.profileFor !== you.player_id || state.profileStale) && !state.profileError) loadProfile(you.player_id);
  const pg = fromProfile(d, 'per_game');
  const rows = [
    gridRow([[recordBlock(d, matches), 7], [eloBlock(d), 5]]),
    gridRow([[matchesBlock(matches), 7], [playersBlock(d.best_players, you.player_id), 5]]),
    gridRow([[pointsBlock(matches), 8], [yardsBlock(pg), 4]]),
    gridRow([[daysBlock(matches), 6], [teamsBlock(fromProfile(d, 'by_team')), 6]]),
    gridRow([[perGameBlock(pg), 8], [winsLossesBlock(fromProfile(d, 'wins_vs_losses')), 4]]),
  ].filter(Boolean);
  parts.push(el('div', { class: 'st-grid' }, ...rows));
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
  // The game that just finished may hold a new record: the profile is read again when it is next needed.
  if (ended) state.profileStale = true;
  if (ended && document.visibilityState !== 'hidden') load(true);
});
if (typeof window !== 'undefined') window.addEventListener?.('hashchange', () => { const v = viewOfHash(); if (v !== state.view) { state.view = v; draw(); } });
setInterval(() => { if (!sessionActive && document.visibilityState !== 'hidden') load(true); }, REFRESH_MS);
