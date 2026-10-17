// The League tab (spec 2026-10-08 v2 §3-4): the league site's homepage for your team, plus the League edition's
// Install/Update button and play panel. Members only: league_home answers member:false for anyone else, and this
// page sends them to Play. Everything is server data, drawn with el()'s `text` only; images only through teamLogo()
// (league_logo data: URLs, or assetUrl). Layout F (spec 2026-10-17): no team colour reaches CSS.
// The League edition commands (league_edition_*, league_play_offline, league_host_friendly, league_room_open) are
// Plan 3b's; none of them navigates. Rooms run through the existing private-room path and this page hands over to
// the Online page itself.
import { call, el, markCurrent, quietCall, reason, wireCoreFold } from './shell.js';
import { makePoller } from './live-common.js';
import { openMenu } from './menu.js';
import { teamLogo } from './match-rows.js';
import { createSignIn } from './sign-in.js';
import {
  bannerState, isUpdateRequired, pageMode, runningInstall, seasonLine, siteUrl, standingsRows, teamCardLine, updateRequiredLine,
  weekBest, weekGames, withInstalled,
} from './league-model.js';

const REFRESH_MS = 60000;
const STALE_LINE = "Can't reach the league right now.";
// While an install the page did not start runs (it was started before the page opened), how often to ask whether it
// is still running. Local (league_edition_installed), no network.
const WATCH_MS = 2000;
const screen = document.getElementById('screen');
// busy: which control is working ('primary', 'cancel', 'offline', 'friendly'), or null.
// controllerSetup: the controller check's sentence after Play offline, until the player answers it.
const state = { home: null, error: null, busy: null, actionError: null, installError: null, controllerSetup: null,
  edition: null, progress: null, needsUpdate: false, trackLeft: 0 };
const signIn = createSignIn({ onChange: () => draw(), onDone: () => load().catch(() => {}) });
// Real team logos: asked of the core once per team per visit (league_logo), drawn as soon as each arrives. The
// league answer's own logo_url is a backend /live-assets/league/ path the page cannot load from a non-production backend.
const logos = new Map();
const askedLogos = new Set();
let logoDraw = 0;
function logoFor(id) {
  const key = Number(id);
  if (!Number.isSafeInteger(key) || key <= 0) return null;
  if (!askedLogos.has(key)) {
    askedLogos.add(key);
    quietCall('league_logo', { teamId: key }).then((u) => {
      if (typeof u !== 'string') return;
      logos.set(key, u);
      if (!logoDraw) logoDraw = setTimeout(() => { logoDraw = 0; draw(); }, 50);
    }).catch(() => {});
  }
  return logos.get(key) ?? null;
}
const logoTeam = (t) => ({ name: t?.name || t?.abbrev || t?.abbr, abbrev: t?.abbrev || t?.abbr,
  logo_url: logoFor(t?.team_id ?? t?.id) ?? t?.logo_url });
const go = (href) => { location.href = href; };

// The release check (network): on page load, after an install and after a version refusal only (3a final fix M2).
async function loadEdition() {
  try { state.edition = await quietCall('league_edition_status'); }
  catch (_) { state.edition = null; }
  watchRunningInstall();
}

// Between release checks, a poll reads the install state from disk only and keeps the last check's `latest`.
async function loadInstalled() {
  try { state.edition = withInstalled(state.edition, await quietCall('league_edition_installed')); }
  catch (_) { /* keep what the page has */ }
  watchRunningInstall();
}

// An install already running when the page opened (it was started before the player left the page): show its
// progress with Install disabled, follow its events, and ask the local answer until it ends; then the release check.
let watching = false;
function watchRunningInstall() {
  const running = runningInstall(state.edition);
  if (!running || state.progress || watching) return;
  watching = true;
  state.progress = running;
  let unlisten = null;
  const listening = window.__TAURI__?.event?.listen
    ? Promise.resolve(window.__TAURI__.event.listen('league-install-progress', (e) => { if (watching) { state.progress = e.payload; draw(); } }))
      .then((u) => { unlisten = u; }).catch(() => {})
    : Promise.resolve();
  const finish = async () => {
    await listening;
    if (typeof unlisten === 'function') unlisten();
    watching = false;
    state.progress = null;
    await loadEdition();
    draw();
  };
  const tickWatch = async () => {
    let s = null;
    try { s = await quietCall('league_edition_installed'); } catch (_) { /* the next poll looks again */ }
    if (s && s.installing === true) {
      if (s.progress && typeof s.progress === 'object') { state.progress = s.progress; draw(); }
      setTimeout(tickWatch, WATCH_MS);
    } else await finish();
  };
  setTimeout(tickWatch, WATCH_MS);
}

// One load at a time: a load asked for while one runs (a refresh during the post-Cancel reload, say) runs once more
// after it, so the page always ends on an answer read after the latest change. Every caller shares the same promise.
let loading = null, again = false, editionChecked = false;
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
  if (pageMode(state.home) === 'member' && !state.progress) {
    // A release check that failed (null) is tried again; one that answered is not repeated by polls.
    if (!editionChecked || !state.edition) { editionChecked = true; await loadEdition(); } else await loadInstalled();
  }
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
  state.progress = { phase: 'verify', done: 0, total: 1, checking: true }; state.installError = null; draw();
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
// Friendly hosts on the league rules unless the ▾ menu picked standard rules for this one game (Adam 10-17).
const friendly = (ruleset = 'league') => act('friendly', async () => { await call('league_host_friendly', { ruleset: ruleset === 'raw' ? 'raw' : 'league' }); go('online.html'); });

// ---- League page v3, layout F (spec 2026-10-17-league-page-v3-design.md) ----
// The page heading with your team card, the matchup banner, then Standings · Week N · Week N's best. Everything that
// is a team, a game or a player is a button that opens its page on the league site (siteUrl: numeric ids only).

let linkN = 0;
function link(url, cls, kids, label) {
  if (!url) return el('div', { class: cls }, ...kids);
  linkN += 1;
  return el('button', { type: 'button', class: `${cls} lg-link`, 'data-url': url, 'data-key': `league-link-${linkN}`, 'aria-label': label || null }, ...kids);
}
const dotIf = (yes) => (yes ? el('span', { class: 'lg-me', 'aria-hidden': 'true' }) : null);
const teamName = (name, me) => el('span', { class: `lg-name${me ? ' me' : ''}` }, name, dotIf(me));

function pageHeading(home) {
  const t = home.team || {};
  const card = el('div', { class: 'glass lg-teamcard' }, teamLogo(logoTeam(t), 'lg-tc-logo'),
    el('div', { class: 'lg-tc-text' }, el('b', { text: t.name || 'Your team' }), el('span', { class: 'num', text: teamCardLine(home) })),
    link(siteUrl('team', home, t.id), 'btn quiet sm lg-tc-site', ['View on site ›']));
  const site = card.querySelector?.('.lg-tc-site');
  if (site) site.setAttribute('data-key', 'league-team-site');
  return el('div', { class: 'page-heading lg-head' },
    el('div', {}, el('h1', { text: 'League' }), el('p', { class: 'purpose', text: seasonLine(home) })), card);
}

function bannerTeam(home, t, me, score) {
  const sub = [me ? home.team?.abbr : (t?.owner_display || t?.abbr), t?.record].filter(Boolean).join(' · ');
  return el('div', { class: 'lg-bt' },
    link(siteUrl('team', home, t?.team_id ?? t?.id), 'lg-bt-team', [teamLogo(logoTeam(t), 'lg-bt-logo'),
      el('span', { class: 'lg-bt-text' }, el('b', { text: t?.name || '' }), el('span', { class: 'num', text: sub }))]),
    score === null || score === undefined ? null : el('span', { class: 'lg-bt-score num', text: String(score) }));
}

const ACT_BUSY = { play: 'Opening room…', join: 'Joining…', cancel: 'Leaving…', offline: 'Playing…', friendly: 'Opening room…' };

function actionButton(a, s, home) {
  const handlers = {
    play: () => play(), join: () => join(home.matchup.room), open: () => go('online.html'), back: () => go('online.html'),
    cancel: () => cancel(home.matchup.room), offline: () => offline(), friendly: () => friendly('league'), install: () => install(),
  };
  const busyKey = { play: 'primary', join: 'primary', cancel: 'cancel', offline: 'offline', friendly: 'friendly' }[a.key];
  const label = busyKey && state.busy === busyKey ? ACT_BUSY[a.key] : a.label;
  const btn = el('button', { type: 'button', class: `btn${a.primary ? ' primary' : ''}`, 'data-key': `league-${a.key}`, ...(a.primary ? { 'data-primary': 'true' } : {}),
    text: label, disabled: a.disabled || (Boolean(state.busy) && a.key !== 'back' && a.key !== 'open'), onclick: handlers[a.key] });
  if (a.key === 'install' && a.percent !== null && a.percent !== undefined) {
    return el('div', { class: 'lg-install' }, btn, el('progress', { max: '100', value: a.percent, 'aria-label': a.label }));
  }
  if (a.key !== 'friendly') return btn;
  // Friendly always plays the league rules (Adam 10-17); the ▾ offers one standard-rules game. Nothing is remembered.
  const more = el('button', { type: 'button', class: 'btn lg-more', 'data-key': 'league-friendly-menu', 'aria-label': 'Friendly rules', text: '▾',
    disabled: a.disabled || Boolean(state.busy),
    onclick: (e) => openMenu(e.currentTarget, { label: 'Friendly rules', current: 'league',
      options: [{ value: 'league', label: 'League rules' }, { value: 'raw', label: 'Standard rules' }], onPick: (v) => friendly(v) }) });
  return el('span', { class: 'lg-split' }, btn, more);
}

function banner(home) {
  const stale = Boolean(state.error) || home.stale === true;
  const s = bannerState(home, { stale, edition: state.edition, progress: state.progress });
  const m = home.matchup || {};
  const you = { ...(home.team || {}), team_id: home.team?.id };
  const bg = el('div', { class: 'lg-banner-bg', 'aria-hidden': 'true' }, teamLogo(logoTeam(you), ''), m.opponent ? teamLogo(logoTeam(m.opponent), '') : null);
  const sc = s.scores || [null, null];
  const teams = s.kind === 'bye' || s.kind === 'unscheduled'
    ? el('div', { class: 'lg-bt' }, teamLogo(logoTeam(you), 'lg-bt-logo'), el('span', { class: 'lg-bt-text' }, el('b', { text: s.text }),
      s.kind === 'bye' ? el('span', { text: `${you.name || 'Your team'} have a bye` }) : null))
    : [bannerTeam(home, you, true, sc[0]), s.mid ? el('span', { class: `lg-mid ${s.kind}` }, s.kind === 'live' ? el('span', { class: 'live-dot', 'aria-hidden': 'true' }) : null, s.mid) : null,
      bannerTeam(home, m.opponent, false, sc[1])];
  const text = s.text && s.kind !== 'bye' && s.kind !== 'unscheduled' ? el('span', { class: `lg-bn-text${s.tone ? ` ${s.tone}` : ''}`, role: 'status', text: s.text }) : null;
  return el('section', { class: 'glass lg-banner', 'aria-label': 'Your game' }, bg,
    el('div', { class: 'lg-bn-row' }, ...[teams].flat(), el('div', { class: 'lg-acts' }, text, ...s.actions.map((a) => actionButton(a, s, home)))));
}

function head(title, url, aside) {
  const h = url ? el('button', { type: 'button', class: 'lg-hlink lg-link', 'data-url': url, 'data-key': `league-head-${title.replace(/\W+/g, '-').toLowerCase()}` }, title) : el('span', { text: title });
  return el('div', { class: 'vc-h' }, el('h2', {}, h), aside);
}

function standingsCard(home) {
  const rows = standingsRows(home);
  return el('section', { id: 'lg-standings', class: 'glass vc lg-card' }, head('Standings', siteUrl('standings', home), el('span', { class: 'vc-aside', text: 'W-L' })),
    rows.length ? el('div', { class: 'lg-rows' }, ...rows.map((r) => link(siteUrl('team', home, r.team_id), `lg-row${r.mine ? ' mine' : ''}`, [
      el('span', { class: 'lg-rank num', text: r.rank === null ? '' : String(r.rank) }), teamLogo(logoTeam(r), 'lg-row-logo'), teamName(r.name || r.abbrev, r.mine),
      el('span', { class: 'lg-rec num', text: r.record })], `${r.name}, ${r.record}`)))
      : el('p', { class: 'vc-none', text: 'No standings yet.' }));
}

function weekCard(home) {
  const games = weekGames(home);
  const week = home.league?.week_label || (home.league?.week ? `Week ${home.league.week}` : 'Week');
  const finals = games.filter((g) => g.status === 'final').length;
  const side = (t, final) => el('div', { class: `lg-g-side${final && !t.winner ? ' lost' : ''}` },
    teamLogo(logoTeam(t), 'lg-g-logo'), teamName(t.name, t.team_id === home.team?.id), el('span', { class: 'lg-g-score num', text: t.score === null ? '' : String(t.score) }));
  return el('section', { id: 'lg-week', class: 'glass vc lg-card' }, head(week, siteUrl('schedule', home), games.length ? el('span', { class: 'vc-aside num', text: `${finals} of ${games.length} final` }) : null),
    games.length ? el('div', { class: 'lg-games' }, ...games.map((g) => link(siteUrl('game', home, g.fixture_id), `lg-game${g.mine ? ' mine' : ''}`, [
      el('div', { class: 'lg-g-pair' }, side(g.away, g.status === 'final'), side(g.home, g.status === 'final')),
      el('span', { class: `lg-mark ${g.marker.toLowerCase()}` }, g.marker === 'Live' ? el('span', { class: 'live-dot', 'aria-hidden': 'true' }) : null, g.marker)],
    `${g.away.name} at ${g.home.name}`)))
      : el('p', { class: 'vc-none', text: 'No games scheduled.' }));
}

function bestCard(home) {
  const b = weekBest(home);
  const all = el('button', { type: 'button', class: 'sec-link lg-link', 'data-url': siteUrl('leaders', home), 'data-key': 'league-all-stats', text: 'All stats ›' });
  return el('section', { id: 'lg-best', class: 'glass vc lg-card' }, el('div', { class: 'vc-h' }, el('h2', { text: b.title }), all),
    b.empty ? el('p', { class: 'vc-none lg-best-none', text: b.empty }) : el('div', { class: 'lg-best' }, ...b.groups.map((g) => el('div', { class: 'lg-bgrp' },
      el('span', { class: 'k-label', text: g.label }),
      ...g.rows.map((r) => link(siteUrl('player', home, r.poid), `lg-brow${r.top ? ' top' : ''}`, [teamLogo(logoTeam({ team_id: r.team_id }), 'lg-row-logo'),
        el('span', { class: 'lg-name', text: r.name }), el('span', { class: 'lg-line num', text: r.line })], `${r.name}, ${r.line}`))))));
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
  linkN = 0;
  screen.replaceChildren(...view());
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
  // One amber line for old data, whichever way it is old (the core's cached answer, or a refresh that failed).
  const stale = state.error || home.stale === true ? STALE_LINE : null;
  const setup = state.controllerSetup ? el('div', { class: 'partial lg-setup', role: 'alert' },
    el('p', { text: state.controllerSetup }),
    el('div', { class: 'lg-row-btns' },
      el('a', { class: 'btn primary sm', href: 'controls.html', 'data-key': 'league-setup', text: 'Set up controller' }),
      el('button', { type: 'button', class: 'btn sm', 'data-key': 'league-anyway', text: 'Play anyway', disabled: Boolean(state.busy), onclick: () => offline(true) }))) : null;
  return [
    pageHeading(home),
    stale ? el('p', { class: 'partial', role: 'status', text: stale }) : null,
    state.installError ? el('p', { class: 'refusal', role: 'alert', text: state.installError }) : null,
    banner(home),
    state.actionError ? el('p', { class: 'refusal', role: 'alert', text: state.actionError }) : null,
    setup,
    el('div', { class: 'lg-cols' }, standingsCard(home), weekCard(home), bestCard(home)),
  ].filter(Boolean);
}

// One handler for every link on the page: open it on the league site (open_website refuses anything else).
screen?.addEventListener?.('click', (e) => {
  const a = e.target?.closest?.('.lg-link[data-url]');
  if (!a) return;
  call('open_website', { url: a.getAttribute('data-url') }).catch(() => {});
});

markCurrent();
wireCoreFold();
poller.start();
document.addEventListener('visibilitychange', () => (document.hidden ? poller.stop() : poller.start()));
