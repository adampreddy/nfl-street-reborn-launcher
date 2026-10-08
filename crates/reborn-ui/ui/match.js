// The match page (Stats or a profile, then a match or a game): the site's /live/<match> page inside the launcher, the
// whole match on one page as the site draws it -- the scoreboard strip with full team names, both players, the G1 / G2 /
// G3 line score and the series banner, then Live Feed (Game 1 · Game 2 · Game 3, landing on the last game; the
// latest-play panel with the portrait over All plays by drive), Box Score and Team Stats (each with its own picker and
// Series total).
//
// Nothing of the site's page is ported: its controller (site/live-page.js) and renderers (site/live-render.js) are
// VENDORED unchanged (Tools/sync-site-render.mjs) and run here as they run on the site. This file only gives them what
// the site's server gives them, and keeps the launcher's chrome around them:
//   - the match id: the site's server writes it into <meta name="nsr-match">; here it comes from the address
//     (match.html?match=<id>&from=<page>, stats-model.js matchHref) and is written into the same meta before the
//     controller loads;
//   - the snapshot: the controller fetches /v1/live/<id> from its own origin. The launcher's CSP has no connect-src for
//     the backend, so that one request is answered by the `live_match` command (Rust makes the request; the snapshot is
//     the backend's own JSON, carrying team names, seats, portraits, box lines and drives -- nothing else to fetch).
//     Every other fetch goes through untouched;
//   - the stream: the site follows a running match with an EventSource of `play` frames. The launcher cannot open one
//     (no connect-src), so a live match's stream is PollingStream: every 3 s it reads the snapshot with `live_match`
//     and hands the controller each play newer than the last it handed over, as the `play` events the site's stream
//     sends (same frame, same rev), so the page follows a live game play by play exactly as the site does (owner
//     10-08: a live game opens here, in the site's style). When the match ends the stream closes, and the controller
//     reads the final snapshot. A finished match never opens a stream (the controller only opens one while it runs);
//     a game's page (below) keeps the stream that never opens;
//   - player names: each seat's name in the strip opens that player's profile in the launcher (player-link.js);
//   - images: the renderers only ever set an <img> src that passed their own rules (a root-relative /live-assets/r/...
//     catalog path, or Discord's avatar CDN). The launcher is not that origin, so an <img> created on this page loads a
//     /live-assets/ path from the backend origin the CSP img-src allows (LIVE_ORIGIN). The site's value stays what the
//     page reads back (getAttribute('src')), so a failed image is remembered under the key the renderers check;
//   - the tabs: the site's roving tabindex leaves only the picked tab focusable, but a controller reaches a control only
//     when it is focusable (app-shell.js), so every tab stays in the tab order;
//   - Back (B / Esc) and Open on web (the site's /live/<match>) above the page.
//
// A game that is not part of a matchmade match (an offline game, an unlinked online one, a record's game) opens here
// too (owner 10-03: one style for every box score), as match.html?game=<id>[&games=<id>,...]&from=<page>
// (stats-model.js matchHref / gameHref): each game is read with `stats_game` (Rust, with the player's token, so an
// offline game opens too), game-snapshot.js turns them into the snapshot the controller reads, and the controller's
// request for it is answered with that. A game the server has no plays for says so above the page (totals only).
import { bridge, call, el, quietCall, reason } from './shell.js';
import { LIVE_ORIGIN, PAGE_ORIGIN } from './live-common.js';
import { backHref, readMatchAddress } from './stats-model.js';
import { webLinkShown } from './stats-rows.js';
import { matchSnapshot } from './game-snapshot.js';
import { playerIdOf, profileHref } from './player-link.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const NOT_FOUND = 'No such match.';   // the site's words (live-page.js) for a match the server does not have
const query = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const matchId = (query.get('match') || '').toLowerCase();
const BACK = backHref(query.get('from'));
// A game (no match id): the game opened, and the match's games oldest first (just it, for a one-game match).
const games = !query.has('match') && query.has('game') ? readMatchAddress(typeof location !== 'undefined' ? location.search : '') : null;
const GAME_NOT_FOUND = "That game couldn't be found.";
// The site's own words (its game page) for a game it has no verified plays for.
const TOTALS_ONLY = 'Totals only for this game. It was played before play-by-play existed (or its plays could not be checked), and older totals may be inaccurate.';
let gameSnap = null;     // a game's snapshot, built once (the controller asks again on its tick)
let webUrl = UUID_RE.test(matchId) ? `${PAGE_ORIGIN}/live/${matchId}` : null;
const $ = (id) => document.getElementById(id);

/** A site image src as the launcher loads it: our catalog's root-relative path under the backend origin. */
export function siteImageSrc(src) {
  const s = String(src);
  return s.startsWith('/live-assets/') ? LIVE_ORIGIN + s : s;
}

/** The snapshot request the site's controller makes, as a Response, answered by the `live_match` command: the
 *  backend's JSON (200), no such match (404, as the site's server answers), or a failed read (503, which the
 *  controller shows as "Could not refresh; trying again shortly."). */
export async function liveMatchResponse(id, ask = quietCall) {
  try {
    const snap = await ask('live_match', { matchId: id });
    if (snap === null || snap === undefined) return new Response('', { status: 404 });
    return new Response(JSON.stringify(snap), { status: 200, headers: { 'content-type': 'application/json' } });
  } catch {
    return new Response('', { status: 503 });
  }
}

/** The site's stream, which never opens in the launcher (no connect-src): readyState stays CONNECTING, so the
 *  controller neither shows "Reconnecting" nor opens another, and its 30-second tick refetches the snapshot. A game's
 *  page (no live match behind it) uses this one. */
export class LauncherStream extends EventTarget {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 2;
  constructor(url) { super(); this.url = String(url); this.readyState = LauncherStream.CONNECTING; }
  close() { this.readyState = LauncherStream.CLOSED; }
}

const ENDED = new Set(['complete', 'abandoned', 'void']);
/** A snapshot whose match will never change again (the controller's own rule: an outcome, else the status). */
export function snapshotEnded(snap) {
  const kind = snap?.match?.outcome?.kind;
  return typeof kind === 'string' && kind ? kind !== 'open' : ENDED.has(snap?.match?.status);
}

/** Every play of `snap` newer than `since`, as the site's stream frames them ({ game_seq, seq, status, body,
 *  situation, field, score_after } with the play's rev as its event id), oldest rev first. */
export function framesSince(snap, since) {
  const out = [];
  for (const g of Array.isArray(snap?.games) ? snap.games : []) {
    if (!Number.isInteger(g?.game_seq)) continue;
    for (const p of Array.isArray(g.plays) ? g.plays : []) {
      const rev = Number(p?.rev);
      const seq = Number.isInteger(p?.seq) ? p.seq : p?.body?.seq;
      if (!Number.isFinite(rev) || rev <= since || !Number.isInteger(seq)) continue;
      out.push({ rev, frame: { game_seq: g.game_seq, seq, status: p.status, body: p.body, situation: p.situation ?? null,
        field: p.field ?? null, score_after: p.score_after ?? null } });
    }
  }
  return out.sort((a, b) => a.rev - b.rev);
}

const POLL_MS = 3000;   // the old live view's pace: a play shows within a few seconds of being played
const POLL_BACKOFF_MAX_MS = 30000;

/**
 * A live match's stream in the launcher: `live_match` read every 3 s, each new play handed to the controller as the
 * site's `play` event. OPEN after the first good read; a failed read is an `error` while CONNECTING (the controller
 * says "Reconnecting…") and the next read waits longer (up to 30 s); the match gone or over CLOSES it with an `error`
 * (the controller then reads the final snapshot). Paused while the window is hidden or a game is running on this PC.
 */
export class PollingStream extends EventTarget {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 2;
  constructor(url, { ask = quietCall, schedule = (f, ms) => setTimeout(f, ms), cancel = (t) => clearTimeout(t), paused = () => false } = {}) {
    super();
    this.url = String(url);
    this.readyState = PollingStream.CONNECTING;
    const u = new URL(this.url, 'https://launcher.invalid/');
    this.matchId = decodeURIComponent((/^\/v1\/live\/([^/]+)\/stream$/.exec(u.pathname) || [])[1] || '');
    this.since = Number(u.searchParams.get('since')) || 0;
    this.ask = ask; this.schedule = schedule; this.cancel = cancel; this.paused = paused;
    this.failures = 0; this.timer = null; this.busy = false;
    PollingStream.live.add(this);
    this.arm(0);
  }
  arm(ms) {
    if (this.readyState === PollingStream.CLOSED) return;
    if (this.timer !== null) this.cancel(this.timer);
    this.timer = this.schedule(() => { this.timer = null; this.tick(); }, ms);
  }
  /** Read now if nothing is out and the page is not paused (the window shown again, a game ended). */
  wake() { if (!this.busy && this.readyState !== PollingStream.CLOSED) this.arm(0); }
  async tick() {
    if (this.readyState === PollingStream.CLOSED) return;
    if (this.paused()) return;          // wake() re-arms it when the page can read again
    this.busy = true;
    let snap;
    try {
      snap = await this.ask('live_match', { matchId: this.matchId });
    } catch {
      this.busy = false;
      if (this.readyState === PollingStream.CLOSED) return;
      this.failures++;
      this.readyState = PollingStream.CONNECTING;
      this.dispatchEvent(new Event('error'));
      this.arm(Math.min(POLL_BACKOFF_MAX_MS, POLL_MS * 2 ** this.failures));
      return;
    }
    this.busy = false;
    if (this.readyState === PollingStream.CLOSED) return;
    this.failures = 0;
    if (!snap || typeof snap !== 'object' || !Array.isArray(snap.games)) { this.finish(); return; }
    if (this.readyState !== PollingStream.OPEN) { this.readyState = PollingStream.OPEN; this.dispatchEvent(new Event('open')); }
    for (const { rev, frame } of framesSince(snap, this.since)) {
      this.since = Math.max(this.since, rev);
      if (this.readyState === PollingStream.CLOSED) return;
      this.dispatchEvent(new MessageEvent('play', { data: JSON.stringify(frame), lastEventId: String(rev) }));
    }
    if (snapshotEnded(snap)) { this.finish(); return; }
    this.arm(POLL_MS);
  }
  /** Over (or gone): CLOSED, then the `error` the site's stream gives when it ends for good. */
  finish() {
    this.close();
    this.dispatchEvent(new Event('error'));
  }
  close() {
    this.readyState = PollingStream.CLOSED;
    if (this.timer !== null) { this.cancel(this.timer); this.timer = null; }
    PollingStream.live.delete(this);
  }
}
PollingStream.live = new Set();

// Nothing reads while the window is hidden or while a game runs on this PC (app-shell.js's `session-state`): a
// match in progress needs the network and the CPU. Showing again (or the game ending) reads at once.
let sessionActive = false;
const streamPaused = () => sessionActive || document.visibilityState === 'hidden';
function wakeStreams() { if (!streamPaused()) for (const s of PollingStream.live) s.wake(); }
document.addEventListener('visibilitychange', wakeStreams);
document.addEventListener('session-state', (e) => { sessionActive = Boolean(e?.detail); wakeStreams(); });
window.addEventListener('pagehide', () => { for (const s of [...PollingStream.live]) s.close(); });

/** The latest snapshot the page was given (the controller's read, or the game's): whose seat is whose, for the names. */
let shownSnap = null;

function installDataSource() {
  const realFetch = window.fetch.bind(window);
  const SNAPSHOT = /^\/v1\/live\/([^/?#]+)$/;
  window.fetch = (input, init) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input?.url ?? '';
    const m = SNAPSHOT.exec(url);
    if (!m) return realFetch(input, init);
    const id = decodeURIComponent(m[1]);
    if (gameSnap) { shownSnap = gameSnap; return Promise.resolve(id === gameSnap.match.id ? new Response(JSON.stringify(gameSnap), { status: 200, headers: { 'content-type': 'application/json' } }) : new Response('', { status: 404 })); }
    return UUID_RE.test(id) ? liveMatchResponse(id, (cmd, args) => quietCall(cmd, args).then((snap) => { if (snap && typeof snap === 'object') shownSnap = snap; return snap; }))
      : Promise.resolve(new Response('', { status: 404 }));
  };
  // A game's page has no live match to follow; a match's page follows it while it runs.
  window.EventSource = gameSnap ? LauncherStream
    : class extends PollingStream { constructor(url) { super(url, { paused: streamPaused }); } };
}

/** Each seat's player name in the site's strip opens their profile (seat 0 is the strip's left team, seat 1 its
 *  right, as the site lays it out): the name element becomes a link with the same class and words. Run after every
 *  render of the strip (the controller rebuilds it), so the names are always links. */
function linkStripNames() {
  const strip = $('strip');
  if (!strip) return;
  const seats = Array.isArray(shownSnap?.seats) ? shownSnap.seats : [];
  [['a', 0], ['b', 1]].forEach(([cls, seat]) => {
    const box = strip.querySelector(`.st-team.${cls}`);
    if (!box) return;
    // The player's name: under the team (.st-player), or the strip's big name when the seat has no team (an avatar).
    const name = box.querySelector('.st-player') || (box.querySelector('.avatar') ? box.querySelector('.st-name') : null);
    const id = playerIdOf(seats[seat]?.player_id);
    if (!name || !id || name.tagName === 'A') return;
    const a = document.createElement('a');
    a.className = `${name.className} player-link`;
    a.href = profileHref(id);
    a.setAttribute('data-key', `strip-player-${seat}`);
    a.textContent = name.textContent;
    a.setAttribute('aria-label', `${name.textContent}, profile`);
    name.replaceWith(a);
  });
}

function watchStrip() {
  const strip = $('strip');
  if (!strip) return;
  new MutationObserver(linkStripNames).observe(strip, { childList: true });
  linkStripNames();
}

/** Every <img> this page creates loads a /live-assets/ path from the backend origin; getAttribute('src') still answers
 *  the site's own value (kept in data-site-src). */
function installImageOrigin() {
  const create = document.createElement.bind(document);
  document.createElement = (tag, options) => {
    const n = create(tag, options);
    if (n.tagName === 'IMG') {
      const set = n.setAttribute.bind(n), get = n.getAttribute.bind(n);
      n.setAttribute = (k, v) => {
        if (String(k).toLowerCase() !== 'src') { set(k, v); return; }
        set('data-site-src', String(v));
        set('src', siteImageSrc(v));
      };
      n.getAttribute = (k) => (String(k).toLowerCase() === 'src' && n.hasAttribute('data-site-src') ? get('data-site-src') : get(k));
    }
    return n;
  };
}

/** Keep every tab focusable (the site's roving tabindex sets the others to -1): a controller must reach Box Score. */
function keepTabsReachable() {
  const tabs = [...document.querySelectorAll('#site-game .tabs [role="tab"]')];
  const fix = () => { for (const b of tabs) if (b.tabIndex !== 0) b.tabIndex = 0; };
  new MutationObserver(fix).observe(document.querySelector('#site-game .tabs'), { subtree: true, attributes: true, attributeFilter: ['tabindex'] });
  fix();
}

// ---- the launcher's row: Back, Open on web ----

async function openOnWeb() {
  try { await call('open_website', { url: webUrl }); } catch (e) { showError(reason(e)); }
}

function drawBar(note = null) {
  // id="back": app-shell.js's controller B / Esc clicks a visible #back, so both return where we came from.
  const back = el('a', { class: 'button lv-back', id: 'back', 'data-key': 'back', href: BACK, text: '← Back' });
  const end = el('div', { class: 'game-bar-end' });
  if (webUrl) end.append(el('button', { type: 'button', 'data-key': 'web', text: 'Open on web', onclick: openOnWeb }));
  $('game-bar').replaceChildren(...[back, end, note ? el('p', { class: 'game-note', id: 'totals-note', text: note }) : null].filter(Boolean));
}

/** A game's (or a non-matchmade match's) snapshot from its games' records; null when the game opened is not there. */
async function loadGames() {
  const ids = games.games;
  const records = await Promise.all(ids.map((id) => quietCall('stats_game', { gameId: id }).then((r) => r, () => null)));
  const opened = records[ids.indexOf(games.game)];
  if (!opened?.game?.id) return null;
  const { snap, totalsOnly } = matchSnapshot(games.game, records);
  if (webLinkShown(opened.game)) webUrl = `${PAGE_ORIGIN}/games/${games.game}`;
  const note = !totalsOnly.length ? null : ids.length === 1 ? TOTALS_ONLY
    : `Totals only for Game ${totalsOnly.join(', ')}.${TOTALS_ONLY.slice(TOTALS_ONLY.indexOf(' It was'))}`;
  return { snap, note };
}

function showError(text) {
  const bar = $('game-bar');
  bar.querySelector('.game-error')?.remove();
  bar.append(el('p', { class: 'game-error', role: 'alert', text }));
}

/** No page to run: the words in the strip, no tabs under it. */
function drawGone(text, alert = false) {
  const p = el('p', { class: 'empty', text });
  if (alert) p.setAttribute('role', 'alert');
  $('strip').replaceChildren(p);
  document.querySelector('#site-game .tabs').hidden = true;
  for (const t of ['feed', 'box', 'team']) $(`tab-${t}`).hidden = true;
}

/** The sidebar marks the page the match was opened from: Online for a live game (Live now, the post-game screen),
 *  Stats otherwise (match.html's own markup). */
function markNav() {
  if (!BACK.startsWith('online.html')) return;
  for (const a of document.querySelectorAll('.shell-nav a')) {
    const on = a.getAttribute('href') === 'online.html';
    a.classList.toggle('current', on);
    if (on) a.setAttribute('aria-current', 'page'); else a.removeAttribute('aria-current');
  }
}

async function start() {
  markNav();
  drawBar();
  const id = games ? games.game : matchId;
  if (!UUID_RE.test(id)) { drawGone(games ? GAME_NOT_FOUND : NOT_FOUND); return; }
  if (!bridge) { drawGone('Open this page inside the launcher window.', true); return; }
  if (games) {
    const got = await loadGames();
    if (!got) { drawGone(GAME_NOT_FOUND); return; }
    gameSnap = got.snap;
    drawBar(got.note);
  }
  const meta = document.createElement('meta');
  meta.name = 'nsr-match';
  meta.content = id;
  document.head.append(meta);
  installDataSource();
  installImageOrigin();
  keepTabsReachable();
  watchStrip();
  await import('./site/live-page.js');
}

// Leaving for another launcher page: mark the cross-document view transition, so the page we return to plays the
// match page's closing half (theme.css, :active-view-transition-type(from-game)).
window.addEventListener('pageswap', (e) => { try { e.viewTransition?.types?.add('from-game'); } catch { /* no types: the plain transition */ } });
start().catch((e) => drawGone(reason(e) || NOT_FOUND, true));
