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
//   - the stream: the site follows a running match with an EventSource. Here the stream stays "connecting" and never
//     opens, which the controller already handles: it refetches the snapshot on its 30-second tick (refetchOnTick)
//     until the match ends;
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
 *  controller neither shows "Reconnecting" nor opens another, and its 30-second tick refetches the snapshot. */
export class LauncherStream extends EventTarget {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 2;
  constructor(url) { super(); this.url = String(url); this.readyState = LauncherStream.CONNECTING; }
  close() { this.readyState = LauncherStream.CLOSED; }
}

function installDataSource() {
  const realFetch = window.fetch.bind(window);
  const SNAPSHOT = /^\/v1\/live\/([^/?#]+)$/;
  window.fetch = (input, init) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input?.url ?? '';
    const m = SNAPSHOT.exec(url);
    if (!m) return realFetch(input, init);
    const id = decodeURIComponent(m[1]);
    if (gameSnap) return Promise.resolve(id === gameSnap.match.id ? new Response(JSON.stringify(gameSnap), { status: 200, headers: { 'content-type': 'application/json' } }) : new Response('', { status: 404 }));
    return UUID_RE.test(id) ? liveMatchResponse(id) : Promise.resolve(new Response('', { status: 404 }));
  };
  window.EventSource = LauncherStream;
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

async function start() {
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
  await import('./site/live-page.js');
}

// Leaving for another launcher page: mark the cross-document view transition, so the page we return to plays the
// match page's closing half (theme.css, :active-view-transition-type(from-game)).
window.addEventListener('pageswap', (e) => { try { e.viewTransition?.types?.add('from-game'); } catch { /* no types: the plain transition */ } });
start().catch((e) => drawGone(reason(e) || NOT_FOUND, true));
