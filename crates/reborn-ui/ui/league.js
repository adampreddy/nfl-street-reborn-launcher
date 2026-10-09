// The League tab (spec 2026-10-08 v2 §3-4): the league site's homepage for your team, plus the League edition's
// Install/Update button and play panel. Members only: league_home answers member:false for anyone else, and this
// page sends them to Play. Everything is server data, drawn with el()'s `text` only; images only through
// teamLogo()/avatar() (assetUrl / avatarSrc, the two prefixes the CSP allows). Team colours reach CSS only as the
// `--tc` / `--oc` custom properties, and only as an exact #RRGGBB (league-model.js teamColor).
// The League edition commands (league_edition_*, league_play_offline, league_host_friendly, league_room_open) are
// Plan 3b's; none of them navigates. Rooms run through the existing private-room path and this page hands over to
// the Online page itself.
import { call, el, get, markCurrent, quietCall, reason, set, wireCoreFold } from './shell.js';
import { makePoller } from './live-common.js';
import { card } from './stats-cards.js';
import { avatar, teamLogo } from './match-rows.js';
import { createSignIn } from './sign-in.js';
import {
  bannerLine, cardState, friendlyRuleset, headerLine, installState, isUpdateRequired, leaderCols, pageMode, playGate,
  recentRows, rosterRows, scoreCells, standingsRows, teamColor, updateRequiredLine, weekTitle,
} from './league-model.js';

const REFRESH_MS = 60000;
const FRIENDLY_KEY = 'league-friendly-league-settings';
const REFRESH_FAILED = "Can't reach the league right now. Showing the last league data we have and retrying. Joining or leaving a room may not work until it's back.";
const screen = document.getElementById('screen');
// busy: which control is working ('primary', 'cancel', 'offline', 'friendly'), or null.
// controllerSetup: the controller check's sentence after Play offline, until the player answers it.
const state = { home: null, error: null, busy: null, actionError: null, installError: null, controllerSetup: null,
  edition: null, progress: null, needsUpdate: false, trackLeft: 0 };
const signIn = createSignIn({ onChange: () => draw(), onDone: () => load().catch(() => {}) });
const logoTeam = (t) => ({ name: t?.name || t?.abbrev || t?.abbr, abbrev: t?.abbrev || t?.abbr, logo_url: t?.logo_url });
const tinted = (node, prop, color) => { if (color && node?.style?.setProperty) node.style.setProperty(prop, color); return node; };
const leagueSettingsOn = () => get(FRIENDLY_KEY) !== 'off'; // default ON
const go = (href) => { location.href = href; };

async function loadEdition() {
  try { state.edition = await quietCall('league_edition_status'); }
  catch (_) { state.edition = null; }
}

// One load at a time: a load asked for while one runs (a refresh during the post-Cancel reload, say) runs once more
// after it, so the page always ends on an answer read after the latest change. Every caller shares the same promise.
let loading = null, again = false;
function load() {
  if (loading) { again = true; return loading; }
  loading = (async () => {
    try { do { again = false; await loadOnce(); } while (again); }
    finally { loading = null; }
  })();
  return loading;
}

async function loadOnce() {
  try {
    const home = await quietCall('league_home');
    state.home = home;
    state.error = null;
    if (pageMode(home) === 'redirect') {
      draw();
      if (typeof location?.replace === 'function') location.replace('index.html');
      return;
    }
  } catch (e) {
    state.error = reason(e);
    draw();
    throw e; // makePoller backs off (and waits a full minute after a rate limit)
  }
  if (pageMode(state.home) === 'member' && !state.progress) await loadEdition();
  draw();
}
const poller = makePoller(load, REFRESH_MS);

// INTEGRATION SEAM (Plan 2/3b): a league room is joined like any private room, by its code. Its refusal for a
// different league build is `{kind: "league_update_required", required}` (online_mm.rs join_error).
async function joinLeagueRoom(code) {
  await call('mm_room_join', { code });
}

async function act(key, fn) {
  if (state.busy) return;
  state.busy = key; state.actionError = null; state.controllerSetup = null; draw();
  try { await fn(); }
  catch (e) {
    const update = updateRequiredLine(e, state.edition);
    if (update) {
      state.actionError = update;
      if (isUpdateRequired(e) && update.startsWith('Update')) { state.needsUpdate = true; await loadEdition(); }
    } else if (e && typeof e === 'object' && e.kind === 'controller_setup') {
      state.controllerSetup = reason(e);
    } else state.actionError = reason(e);
  }
  finally { state.busy = null; draw(); }
}

async function install() {
  if (state.progress) return;
  let unlisten = null;
  state.progress = { phase: 'download', done: 0, total: 0 }; state.installError = null; draw();
  try {
    if (window.__TAURI__?.event?.listen) unlisten = await window.__TAURI__.event.listen('league-install-progress', (e) => { state.progress = e.payload; draw(); });
    await call('league_edition_install');
    state.needsUpdate = false;
    if (state.actionError && /update/i.test(state.actionError)) state.actionError = null;
  } catch (e) {
    state.installError = reason(e);
  } finally {
    if (typeof unlisten === 'function') unlisten();
    state.progress = null;
    await loadEdition();
    draw();
  }
}

const play = () => act('primary', async () => {
  const r = await call('league_room_open');
  const code = r?.code || r?.room?.code;
  if (r?.action === 'join' && code) await joinLeagueRoom(code);
  go('online.html');
});
const join = (room) => act('primary', async () => { await joinLeagueRoom(room.code); go('online.html'); });
const cancel = (room) => act('cancel', async () => { await call('mm_room_action', { roomId: room.room_id ?? room.id, action: 'leave' });
  // The leave went through. A reload that fails now is a refresh problem (the banner), never Cancel's.
  await load().catch(() => {});
});
const offline = (playAnyway = false) => act('offline', async () => { await call('league_play_offline', playAnyway ? { playAnyway: true } : {}); });
const friendly = () => act('friendly', async () => { await call('league_host_friendly', { ruleset: friendlyRuleset(leagueSettingsOn()) }); go('online.html'); });

// ---- the team header: who you are, and the League edition's Install / Update button ----

function installArea() {
  // A room asked for a newer build than the release check has seen yet: still offer the update (install fetches it).
  const s = installState(state.edition, state.progress)
    || (state.needsUpdate ? { label: 'Update league files', disabled: false, primary: true, percent: null } : null);
  if (!s) {
    const v = state.edition?.version;
    return v ? el('span', { class: 'lg-files' }, el('span', { class: 'lg-ok', 'aria-hidden': 'true' }), `League files ${v}`) : null;
  }
  const btn = el('button', { type: 'button', class: `btn${s.primary ? ' primary' : ''}${state.needsUpdate ? ' attention' : ''}`, 'data-key': 'league-install',
    text: s.label, disabled: s.disabled || Boolean(state.busy), onclick: install });
  const bar = s.percent === null ? null : el('progress', { max: '100', value: s.percent, 'aria-label': s.label });
  return el('div', { class: 'lg-install' }, btn, bar);
}

function teamHeader(home) {
  const t = home.team || {};
  const site = typeof home.site_url === 'string' && home.site_url.startsWith('https://nflstreetreborn.com/')
    ? el('button', { type: 'button', class: 'btn quiet sm lg-site', 'data-key': 'league-site', text: 'View on site ›',
      onclick: () => call('open_website', { url: home.site_url }).catch(() => {}) })
    : null;
  return tinted(el('header', { class: 'glass lg-head' },
    teamLogo(logoTeam(t), 'lg-head-logo'),
    el('div', { class: 'lg-head-text' },
      el('span', { class: 'k-label', text: home.league?.name || 'League' }),
      el('h1', { text: t.name || 'Your team' }),
      el('p', { class: 'lg-head-line num', text: headerLine(home) })),
    el('div', { class: 'lg-head-actions' }, installArea(), site)), '--tc', teamColor(t));
}

// ---- the week scoreboard: every game this week, yours first ----

function sideRow(s) {
  return tinted(el('div', { class: `lg-side${s.winner ? ' won' : ''}` },
    el('span', { class: 'lg-chip', 'aria-hidden': 'true' }), teamLogo(logoTeam(s), 'lg-cell-logo'),
    el('b', { class: 'lg-abbr', text: s.abbrev }), el('span', { class: 'lg-rec num', text: s.record }),
    el('span', { class: 'lg-pts num', text: s.score === null ? '' : String(s.score) })), '--tc', s.color);
}

function cellLabel(c) {
  const side = (s) => `${s.name || s.abbrev}${s.score === null ? '' : ` ${s.score}`}`;
  return `${c.mine ? 'Your game: ' : ''}${side(c.away)} at ${side(c.home)}, ${c.mine && c.status === 'scheduled' ? 'this week' : c.statusText.toLowerCase()}`;
}

function scoreboard(home) {
  const cells = scoreCells(home);
  if (!cells.length) return null;
  const toWeek = () => {
    const btn = document.querySelector?.('[data-key="league-primary"]:not(:disabled)');
    if (btn) btn.focus(); else document.getElementById('lg-week')?.scrollIntoView?.({ block: 'nearest' });
  };
  const track = el('ol', { class: 'lg-track', 'aria-label': `${home.league?.week_label || 'This week'}'s games` }, ...cells.map((c) => {
    const status = el('span', { class: `lg-status ${c.status}` }, c.status === 'live' ? el('span', { class: 'live-dot', 'aria-hidden': 'true' }) : null,
      c.mine && c.status === 'scheduled' ? 'Your game' : c.statusText);
    const kids = [status, sideRow(c.away), sideRow(c.home)];
    const cls = `lg-cell ${c.status}${c.mine ? ' mine' : ''}`;
    const body = c.mine
      ? el('button', { type: 'button', class: cls, 'data-key': 'league-my-cell', 'aria-label': `${cellLabel(c)}. Go to This week`, onclick: toWeek }, ...kids)
      : el('div', { class: cls, 'aria-label': cellLabel(c), role: 'group' }, ...kids);
    return el('li', {}, body);
  }));
  const prev = el('button', { type: 'button', class: 'btn quiet sm lg-step', 'data-key': 'league-earlier', 'aria-label': 'Earlier games', text: '‹' });
  const next = el('button', { type: 'button', class: 'btn quiet sm lg-step', 'data-key': 'league-later', 'aria-label': 'Later games', text: '›' });
  const board = el('section', { class: 'glass lg-board', 'aria-label': `${home.league?.week_label || 'Week'} scoreboard` }, prev, track, next);
  const steps = () => {
    const max = (track.scrollWidth || 0) - (track.clientWidth || 0);
    const left = track.scrollLeft || 0;
    prev.disabled = left <= 1;
    next.disabled = left >= max - 1;
    board.classList.toggle('fits', max <= 1);
  };
  const still = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  const step = (dir) => () => track.scrollBy?.({ left: dir * (track.clientWidth || 0) * 0.8, behavior: still() ? 'auto' : 'smooth' });
  prev.addEventListener('click', step(-1));
  next.addEventListener('click', step(1));
  track.addEventListener('scroll', () => { state.trackLeft = track.scrollLeft || 0; steps(); }, { passive: true });
  // A mouse wheel scrolls the strip sideways, but only while there is somewhere to go (the page keeps its own scroll).
  track.addEventListener('wheel', (e) => {
    if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
    const max = track.scrollWidth - track.clientWidth;
    if (max <= 1 || (e.deltaY < 0 && track.scrollLeft <= 0) || (e.deltaY > 0 && track.scrollLeft >= max - 1)) return;
    track.scrollLeft += e.deltaY; e.preventDefault();
  }, { passive: false });
  // After layout: back where the strip was before this redraw, and the arrows set for it.
  if (typeof requestAnimationFrame === 'function') requestAnimationFrame(() => { track.scrollLeft = state.trackLeft; steps(); });
  return board;
}

// ---- This Week: your game, and the one button that plays it ----

function vsSide(t, { opp = false, online = true } = {}) {
  const owner = opp && t?.owner_display ? el('div', { class: 'lg-owner' }, avatar(t.avatar_url, t.owner_display, 'lg-av'),
    el('span', { class: 'lg-handle', text: `@${t.owner_display}` }),
    online && t.online === true ?el('span', { class: 'lg-online' }, el('span', { class: 'lg-ok', 'aria-hidden': 'true' }), 'Online') : null) : null;
  return el('div', { class: `lg-vs-team${opp ? ' opp' : ''}` }, teamLogo(logoTeam(t), 'lg-vs-logo'),
    el('b', { class: 'lg-vs-name', text: t?.name || t?.abbr || '' }),
    el('span', { class: 'lg-vs-rec num', text: [t?.abbr, t?.record].filter(Boolean).join(' · ') }), owner);
}

const BUSY_WORDS = { play: 'Opening room…', join: 'Joining…', cancel: 'Leaving…' };

function thisWeek(home) {
  const s = cardState(home);
  const gate = playGate(state.edition, state.progress);
  const m = home.matchup || {};
  const opp = m.opponent || {};
  const noGame = s.kind === 'bye' || s.kind === 'unscheduled';
  const vs = noGame ? null : el('div', { class: 'lg-vs' }, vsSide(home.team), el('span', { class: 'lg-vs-at', text: m.home ? 'vs' : '@' }), vsSide(opp, { opp: true, online: !s.line.endsWith('is online') }));
  // Opening a room that already exists (waiting / lobby / live) needs no league files; starting or joining one does.
  const needsFiles = s.kind === 'play' || s.kind === 'join';
  const onClick = s.kind === 'play' ? play : s.kind === 'join' ? () => join(s.room) : (s.kind === 'waiting' || s.kind === 'lobby' || s.kind === 'live') ? () => go('online.html') : null;
  const blocked = Boolean(state.busy) || (needsFiles && !gate.ready);
  const label = state.busy === 'primary' ? (s.kind === 'join' ? BUSY_WORDS.join : BUSY_WORDS.play) : s.action;
  const buttons = s.action || s.cancel ? el('div', { class: 'lg-actions' },
    s.action ? el('button', { type: 'button', class: 'btn primary lg-go', 'data-key': 'league-primary', 'data-primary': 'true', text: label, disabled: blocked, onclick: onClick }) : null,
    s.cancel ? el('button', { type: 'button', class: 'btn', 'data-key': 'league-cancel', text: state.busy === 'cancel' ? BUSY_WORDS.cancel : 'Cancel', disabled: Boolean(state.busy), onclick: () => cancel(s.room) }) : null) : null;
  const hint = s.action && needsFiles && !gate.ready ? el('p', { class: 'lg-hint', text: gate.hint })
    : s.kind === 'unlinked' ? el('p', { class: 'lg-hint', text: 'They need to link Discord on the league site before you can play.' }) : null;
  const line = s.line ? el('p', { class: `lg-line ${s.kind}` }, s.kind === 'live' ? el('span', { class: 'live-dot', 'aria-hidden': 'true' }) : null, s.line) : null;
  const node = card('lg-week', weekTitle(home), [vs, line, buttons, hint,
    state.actionError ? el('p', { class: 'refusal', role: 'alert', text: state.actionError }) : null],
  { aside: noGame ? null : (m.home ? 'Home' : 'Away'), cls: `lg-week ${s.kind}${noGame ? ' empty' : ''}` });
  tinted(node, '--tc', teamColor(home.team));
  return tinted(node, '--oc', teamColor(opp));
}

// ---- the League edition: offline and friendlies ----

function playPanel() {
  const gate = playGate(state.edition, state.progress);
  const on = leagueSettingsOn();
  const toggle = el('button', { type: 'button', class: 'look-switch', role: 'switch', 'aria-checked': on ? 'true' : 'false', 'aria-labelledby': 'lg-ls-label',
    'data-key': 'league-settings', disabled: Boolean(state.busy),
    onclick: () => { set(FRIENDLY_KEY, on ? 'off' : 'on'); draw(); } });
  const setup = state.controllerSetup ? el('div', { class: 'partial lg-setup', role: 'alert' },
    el('p', { text: state.controllerSetup }),
    el('div', { class: 'lg-row' },
      el('a', { class: 'btn primary sm', href: 'controls.html', 'data-key': 'league-setup', text: 'Set up controller' }),
      el('button', { type: 'button', class: 'btn sm', 'data-key': 'league-anyway', text: 'Play anyway', disabled: Boolean(state.busy), onclick: () => offline(true) }))) : null;
  return card('lg-play', 'League build', [
    el('div', { class: 'lg-play-btns' },
      el('button', { type: 'button', class: 'btn', 'data-key': 'league-offline', text: state.busy === 'offline' ? 'Playing…' : 'Play offline', disabled: Boolean(state.busy) || !gate.ready, onclick: () => offline() }),
      el('button', { type: 'button', class: 'btn', 'data-key': 'league-friendly', text: state.busy === 'friendly' ? 'Opening room…' : 'Host friendly', disabled: Boolean(state.busy) || !gate.ready, onclick: friendly })),
    el('div', { class: 'lg-toggle' },
      el('span', { id: 'lg-ls-label', class: 'lg-toggle-text' }, el('b', { text: 'League settings' }), el('span', { text: on ? 'Friendlies play Hard, GameBreakers off' : 'Friendlies play the game as it comes' })),
      toggle),
    gate.ready ? null : el('p', { class: 'lg-hint', text: gate.hint }),
    setup,
  ], { cls: 'lg-play' });
}

// ---- the quiet cards: standings, results, leaders, roster ----

function standings(home) {
  const rows = standingsRows(home);
  return card('lg-standings', 'Standings', rows.length ? el('ol', { class: 'lg-list lg-stand' }, ...rows.map((r) =>
    el('li', { class: r.mine ? 'mine' : null, 'aria-current': r.mine ? 'true' : null },
      el('span', { class: 'num lg-rank', text: r.rank === null ? '' : String(r.rank) }), el('span', { class: 'lg-name', text: r.name || r.abbrev }),
      el('span', { class: 'num r', text: r.record })))) : el('p', { class: 'vc-none', text: 'No standings yet.' }), { flush: rows.length > 0 });
}

function recent(home) {
  const rows = recentRows(home);
  return card('lg-recent', 'Recent results', rows.length ? el('ol', { class: 'lg-list lg-results' }, ...rows.map((r) => {
    let rest = r.result && r.text.startsWith(r.result) ? r.text.slice(r.result.length).trim() : r.text;
    if (rest.startsWith('by forfeit')) rest = `${r.result === 'W' ? 'Won' : 'Lost'} ${rest}`;
    return el('li', {}, el('span', { class: `lg-res ${r.result === 'W' ? 'w' : r.result === 'L' ? 'l' : 'f'}`, 'aria-hidden': 'true', text: r.result || '–' }),
      el('span', { class: 'lg-name', text: rest, 'aria-label': r.text }));
  })) : el('p', { class: 'vc-none', text: 'No results yet.' }), { flush: rows.length > 0 });
}

function leaders(home) {
  const colsData = leaderCols(home);
  return card('lg-leaders', 'Team leaders', el('div', { class: 'lg-leaders' }, ...colsData.map((c) => el('div', { class: 'lg-lcol' },
    el('span', { class: 'k-label', text: c.label }),
    ...(c.rows.length ? c.rows.map((r) => el('div', { class: 'lg-leader' }, el('span', { class: 'lg-name', text: r.name }), el('b', { class: 'num', text: r.value })))
      : [el('p', { class: 'vc-none', text: 'No one yet' })])))));
}

function roster(home) {
  const rows = rosterRows(home);
  if (!rows.length) return null;
  return el('details', { class: 'glass lg-roster' }, el('summary', { 'data-key': 'league-roster', text: `Roster · ${rows.length} ${rows.length === 1 ? 'player' : 'players'}` }),
    el('ol', { class: 'lg-list' }, ...rows.map((r) => el('li', { 'data-poid': r.key === null ? null : String(r.key) },
      el('span', { class: 'num lg-rank', text: r.number }), el('span', { class: 'lg-name', text: r.name }), el('span', { class: 'r lg-pos', text: r.position })))));
}

// ---- drawing ----

function signInCard() {
  const si = signIn.state;
  if (si.active) {
    return el('section', { class: 'glass lg-signin', 'aria-label': 'Sign in' }, el('h2', { text: 'Waiting for Discord' }),
      el('p', { role: 'status', text: `Your code is ${si.userCode}` }),
      si.verifyUrl ? el('p', { text: `Go to ${si.verifyUrl} to approve it.` }) : null,
      el('button', { type: 'button', class: 'btn', 'data-key': 'signin-cancel', text: 'Cancel', onclick: () => signIn.cancel() }));
  }
  return el('section', { class: 'glass lg-signin', 'aria-label': 'Sign in' }, el('h2', { text: 'Sign in to see your league' }),
    el('p', { text: 'Connect your Discord account to see your team and play your league games.' }),
    el('button', { type: 'button', class: 'btn primary', 'data-key': 'signin', 'data-primary': 'true', text: si.starting ? 'Opening Discord…' : 'Sign in with Discord', disabled: si.starting, onclick: () => signIn.start() }),
    si.message ? el('p', { class: 'refusal', role: 'alert', text: si.message }) : null);
}

// Redraws replace everything (every refresh): put the focus back on the same control, by data-key.
function draw() {
  if (!screen) return;
  const active = typeof screen.contains === 'function' && screen.contains(document.activeElement) ? document.activeElement : null;
  const focusKey = active?.getAttribute?.('data-key') ?? null;
  const rosterOpen = Boolean(screen.querySelector?.('.lg-roster')?.open);
  screen.replaceChildren(...view());
  if (rosterOpen) { const r = screen.querySelector?.('.lg-roster'); if (r) r.open = true; }
  if (focusKey) screen.querySelector?.(`[data-key="${focusKey}"]`)?.focus?.({ preventScroll: true });
}

function view() {
  const home = state.home;
  if (!home) {
    if (!state.error) return [el('p', { class: 'loading', text: 'Loading…' })];
    return [el('div', { class: 'lg-error' }, el('p', { class: 'refusal', role: 'alert', text: state.error }),
      el('button', { type: 'button', class: 'btn', 'data-key': 'league-retry', 'data-primary': 'true', text: 'Try again', onclick: () => load().catch(() => {}) }))];
  }
  const mode = pageMode(home);
  if (mode === 'signed_out') return [signInCard()];
  if (mode !== 'member') return [el('p', { class: 'loading', text: 'The League tab is for league members. Taking you to Play…' })];
  // A refresh that failed after a good load: the old answer stays on screen, said so plainly, room buttons included.
  const banner = state.error ? REFRESH_FAILED : bannerLine(home);
  return [
    teamHeader(home),
    state.installError ? el('p', { class: 'refusal', role: 'alert', text: state.installError }) : null,
    banner ? el('p', { class: 'partial', role: 'status', text: banner }) : null,
    scoreboard(home),
    el('div', { class: 'lg-grid' },
      el('div', { class: 'lg-col lg-main' }, thisWeek(home), el('div', { class: 'lg-pair' }, recent(home), leaders(home))),
      el('div', { class: 'lg-col lg-side-col' }, playPanel(), standings(home))),
    roster(home),
  ].filter(Boolean);
}

markCurrent();
wireCoreFold();
poller.start();
document.addEventListener('visibilitychange', () => (document.hidden ? poller.stop() : poller.start()));
