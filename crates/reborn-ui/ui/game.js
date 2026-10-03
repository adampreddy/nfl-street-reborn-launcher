// The game page (Stats or a profile, then a match that is NOT matchmade -- an offline or unlinked game; a matchmade
// match opens match.html, the site's /live/<match> page, stats-model.js matchHref): the site's /games/<id> page inside
// the launcher, for a WHOLE match (owner 10-03, "the box score is the whole match"). A match of several games is one page: the site's own game picker
// (Game 1 · Game 2 · Game 3, renderPicker) and the series sit above the scoreboard, and picking a game swaps the page
// below in place. A match of one game (an offline game among them) is just that game, with no picker.
// Each game comes from the server through the `stats_game` command (Rust does the requests, with the player's token, so
// a player's own offline game opens too) and is drawn by the SITE's own renderers, vendored unchanged into site/
// (Tools/sync-site-render.mjs): the scoreboard strip, then Live Feed (the latest-play panel over All plays by drive), Box
// Score and Team Stats. This file is the site's game-page.js controller with the launcher's data source, images and
// controls around it:
//   - the snapshot is built exactly as game-page.js builds it (gameSnapshot, fieldFromBody, rescore);
//   - the site's renderers only ever set an <img> src that passed their own rules (a root-relative /live-assets/r/...
//     catalog path, or Discord's avatar CDN). The launcher is not that origin, so the document handed to them prefixes
//     a /live-assets/ path with the backend origin the CSP img-src allows (LIVE_ORIGIN); nothing else is rewritten;
//   - untrusted server text reaches the page through textContent only (the renderers build with createElement);
//   - the launcher keeps ← Back and Open on web above the page.
// Opened as game.html?game=<id>[&games=<id>,<id>,...&series=2-1]&from=<page> (stats-model.js matchHref).
import { bridge, call, el, quietCall, reason } from './shell.js';
import { LIVE_ORIGIN, PAGE_ORIGIN } from './live-common.js';
import { webLinkShown } from './stats-rows.js';
import { readMatchAddress } from './stats-model.js';
import { fieldFromBody, renderStrip, renderPlayPanel, renderAllPlays, renderBoxScore, renderTeamStats, renderPicker, gameSnapshot, rescore, letterBadge, panelStepSeq, panelPickSeq } from './site/live-render.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
// The site's own words (game-page.js).
const TOTALS_ONLY = 'Totals only for this game. It was played before play-by-play existed (or its plays could not be checked), and older totals may be inaccurate.';
const NOT_FOUND = "That game couldn't be found.";
const TABS = ['feed', 'box', 'team'];
const address = readMatchAddress(typeof location !== 'undefined' ? location.search : '');
const matchGames = address.games;              // oldest first; one id for a one-game match
const BACK = address.back;
const $ = (id) => document.getElementById(id);
const screen = $('screen'), bar = $('game-bar'), strip = $('strip'), tabList = document.querySelector('#site-game .tabs');
const pick = $('game-pick'), body = $('game-body');
const panelSlot = $('panel'), allSlot = $('allplays'), note = $('note');
const panels = { feed: $('tab-feed'), box: $('tab-box'), team: $('tab-team') };

let gameId = address.game;     // the game on screen
let tab = 'feed';
let snap = null;
let verified = false;
let online = false;            // only an online game gets "Open on web" (webLinkShown, stats-rows.js)
let panelSeq = null;           // the play the panel shows; null = the last play
let open = new Map();          // "g1:d<first_seq>" -> the viewer's open/closed choice for a drive (per game)
let apMode = 'all';            // All plays or Scoring plays
const badImages = new Set();   // keyed by the SITE's src (the root-relative path), as the renderers check it
const badAvatars = new Set();
const loaded = new Map();      // game id -> Promise of { snap, verified, online } or { gone, alert }

/** A site image src as the launcher loads it: our catalog's root-relative path under the backend origin. */
export function siteImageSrc(src) {
  const s = String(src);
  return s.startsWith('/live-assets/') ? LIVE_ORIGIN + s : s;
}
// The document the site's renderers build with: the real one, except that an <img>'s src goes through siteImageSrc
// (and the site's own value is kept in data-site-src, so a failed image is remembered under the key they check).
const siteDoc = {
  createElement(tag) {
    const n = document.createElement(tag);
    if (n.tagName === 'IMG') {
      const set = n.setAttribute.bind(n);
      n.setAttribute = (k, v) => {
        if (String(k).toLowerCase() !== 'src') { set(k, v); return; }
        set('data-site-src', String(v));
        set('src', siteImageSrc(v));
      };
    }
    return n;
  },
};

// ---- the launcher's row: Back, Open on web ----

async function openOnWeb() {
  try { await call('open_website', { url: `${PAGE_ORIGIN}/games/${gameId}` }); } catch (e) { showError(reason(e)); }
}

function drawBar() {
  // id="back": app-shell.js's controller B / Esc clicks a visible #back, so both return where we came from.
  const back = el('a', { class: 'button lv-back', id: 'back', 'data-key': 'back', href: BACK, text: '← Back' });
  const end = el('div', { class: 'game-bar-end' });
  if (online) end.append(el('button', { type: 'button', 'data-key': 'web', text: 'Open on web', onclick: openOnWeb }));
  bar.replaceChildren(back, end);
}

function showError(text) {
  bar.querySelector('.game-error')?.remove();
  bar.append(el('p', { class: 'game-error', role: 'alert', text }));
}

// ---- the match: the site's game picker and the series, above the scoreboard ----

function drawPicker() {
  if (matchGames.length < 2) { pick.hidden = true; return; }
  const at = matchGames.indexOf(gameId) + 1;
  const nav = renderPicker(matchGames.map((_, i) => ({ game_seq: i + 1 })), at, document, { name: 'match' });
  nav.setAttribute('aria-label', 'Games in this match');
  nav.querySelectorAll('button[data-pick]').forEach((b) => b.setAttribute('data-key', `game-${b.dataset.pick}`));
  // Hyphens on the site's page, never an en dash (Tools/check-game-site.mjs).
  const series = address.series ? el('span', { class: 'game-series', text: `Series ${address.series}` }) : null;
  pick.replaceChildren(...[nav, series].filter(Boolean));
  pick.hidden = false;
}

pick.addEventListener('click', (e) => {
  const b = e.target instanceof Element ? e.target.closest('button[data-pick]') : null;
  if (!b) return;
  const id = matchGames[Number(b.dataset.pick) - 1];
  if (id && id !== gameId) switchTo(id);
});
// Left / Right walk the picker (as the site's tabs do); the pad's D-pad moves by position.
pick.addEventListener('keydown', (e) => {
  const all = [...pick.querySelectorAll('button[data-pick]')];
  const i = all.indexOf(document.activeElement);
  if (i < 0 || (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight')) return;
  e.preventDefault();
  all[Math.max(0, Math.min(all.length - 1, i + (e.key === 'ArrowRight' ? 1 : -1)))]?.focus();
});

// ---- the site's page (game-page.js) ----

function para(className, text) {
  const n = document.createElement('p');
  n.className = className;
  n.textContent = text;
  return n;
}

function draw() {
  if (!snap) return;
  const game = snap.games[0];
  strip.replaceChildren(renderStrip(snap, game, siteDoc, { badImages, badAvatars }));
  if (!verified) {
    panelSlot.replaceChildren(para('empty', TOTALS_ONLY));
    allSlot.replaceChildren();
    for (const k of ['box', 'team']) panels[k].replaceChildren(para('empty', TOTALS_ONLY));
    return;
  }
  panelSlot.replaceChildren(renderPlayPanel(snap, game, siteDoc, { seq: panelSeq, badImages }));
  allSlot.replaceChildren(renderAllPlays(snap, game, siteDoc, { badImages, open, mode: apMode }));
  panels.box.replaceChildren(renderBoxScore(snap, game.game_seq, siteDoc, { badImages }));
  panels.team.replaceChildren(renderTeamStats(snap, game.game_seq, siteDoc, { badImages }));
}

/** No game to show (a bad id, a game the server does not have, or a failed read): the site's words in the strip and
 *  no tabs (there is nothing under them). The match's picker stays, so its other games are still one press away. */
function drawGone(text, alert = false) {
  const p = para('empty', text);
  if (alert) p.setAttribute('role', 'alert');
  strip.replaceChildren(p);
  note.textContent = '';
  tabList.hidden = true;
  for (const t of TABS) panels[t].hidden = true;
}

/** « ◀ ▶ »: step the panel through the game's plays; the last play is the default. */
function stepPanel(how) {
  const game = snap?.games[0];
  if (!game?.plays?.length) return;
  panelSeq = panelStepSeq(game, panelSeq, how);
  draw();
  const again = panelSlot.querySelector(`.pp-step[data-step="${how}"]`);
  (again && !again.disabled ? again : panelSlot.querySelector('.pp-step[data-step="live"]'))?.focus();
}

screen.addEventListener('click', (e) => {
  const t = e.target instanceof Element ? e.target : null;
  if (!t?.closest('#game-body')) return;
  const step = t.closest('.pp-step');
  if (step) { if (!step.disabled) stepPanel(step.dataset.step); return; }
  const mode = t.closest('.ap-seg button[data-mode]');
  if (mode) {
    apMode = mode.dataset.mode === 'scoring' ? 'scoring' : 'all';
    draw();
    allSlot.querySelector(`.ap-seg button[data-mode="${apMode}"]`)?.focus();
    return;
  }
  const drive = t.closest('.dv-hd');
  if (drive) {
    const k = drive.dataset.drive;
    open.set(k, drive.getAttribute('aria-expanded') !== 'true');
    draw();
    allSlot.querySelector(`.dv-hd[data-drive="${CSS.escape(k)}"]`)?.focus();
    return;
  }
  const row = t.closest('.ap-btn');
  if (row) {
    panelSeq = panelPickSeq(snap?.games[0], Number(row.dataset.seq));
    draw();
    panelSlot.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }
});

// A failed image: an avatar becomes its letter badge; a logo or portrait is remembered (under the site's own src) and
// the page redrawn with its fallback. `error` does not bubble: capture phase.
document.addEventListener('error', (e) => {
  const img = e.target;
  if (!(img instanceof HTMLImageElement) || !img.closest('#site-game')) return;
  const src = img.getAttribute('data-site-src') ?? img.getAttribute('src');
  if (img.classList.contains('avatar')) { badAvatars.add(src); img.replaceWith(letterBadge(document, img.dataset.initial)); return; }
  badImages.add(src);
  img.remove();
  draw();
}, true);

// The site's tabs. Every tab stays in the tab order (the site moves tabindex to the picked one only): a controller
// reaches a control only when it is focusable (app-shell.js), and Box Score must be reachable by pad.
function showTab(name, { focus = false } = {}) {
  tab = name;
  for (const t of TABS) {
    panels[t].hidden = t !== name;
    const b = $(`tabbtn-${t}`);
    b.setAttribute('aria-selected', String(t === name));
    if (t === name && focus) b.focus();
  }
}

for (const t of TABS) $(`tabbtn-${t}`).addEventListener('click', () => showTab(t));
tabList?.addEventListener('keydown', (e) => {
  const i = TABS.indexOf(tab);
  const to = { ArrowRight: (i + 1) % TABS.length, ArrowLeft: (i + TABS.length - 1) % TABS.length, Home: 0, End: TABS.length - 1 }[e.key];
  if (to === undefined) return;
  e.preventDefault();
  showTab(TABS[to], { focus: true });
});

/** One game's record, read once per page (the picker goes back and forth without asking again). */
function fetchGame(id) {
  if (!loaded.has(id)) loaded.set(id, (async () => {
    if (!UUID_RE.test(id)) return { gone: NOT_FOUND };
    let record;
    try { record = await quietCall('stats_game', { gameId: id }); } catch (e) { return { gone: reason(e), alert: true }; }
    const game = record && typeof record === 'object' ? record.game : null;
    if (!game || typeof game !== 'object' || !game.id) return { gone: NOT_FOUND };
    // The server's own verdict, not whether any rows came back: only a verified game's plays are trusted.
    const ok = game.plays_status === 'verified';
    const list = (v) => (Array.isArray(v) ? v : []);
    const s = gameSnapshot(game, ok ? list(record.plays) : [], ok ? list(record.lines) : [], ok ? list(record.drives) : []);
    // The /plays route sends { body, situation } only: give every play its field and its running score_after (the
    // server's rule, rescore) so All plays shows scores; the strip keeps the capture's own final score.
    const g0 = s.games[0];
    for (const p of g0.plays) p.field ??= fieldFromBody(p.body, p.situation);
    const captured = g0.score;
    rescore(g0);
    if (Array.isArray(captured)) g0.score = captured;
    return { snap: s, verified: ok, online: webLinkShown(game) };
  })());
  return loaded.get(id);
}

/** Show one game of the match: its strip and tabs; the tab the player was on stays picked. */
async function show(id) {
  const got = await fetchGame(id);
  if (id !== gameId) return; // another game was picked while this one loaded
  if (got.gone) { snap = null; online = false; drawBar(); drawGone(got.gone, got.alert); return; }
  ({ snap, verified, online } = got);
  tabList.hidden = false;
  showTab(tab);
  drawBar();
  draw();
  note.textContent = verified ? 'Final' : '';
}

/** The picker: swap the page below it to another game of the match, in place (the address follows, without a history
 *  entry, so Back still leaves the match). The panel, drives and All plays start fresh for that game. */
function switchTo(id) {
  gameId = id;
  panelSeq = null; open = new Map(); apMode = 'all';
  try {
    const q = new URLSearchParams(location.search);
    q.set('game', id);
    history.replaceState(history.state, '', `${location.pathname}?${q}`);
  } catch { /* the address is a convenience only */ }
  drawPicker();
  pick.querySelector('button[aria-pressed="true"]')?.focus({ preventScroll: true });
  body.classList.remove('game-swap');
  void body.offsetWidth; // restart the swap animation
  body.classList.add('game-swap');
  return show(id);
}

async function load() {
  drawBar();
  if (!UUID_RE.test(gameId)) { drawGone(NOT_FOUND); return; }
  if (!bridge) { drawGone('Open this page inside the launcher window.', true); return; }
  drawPicker();
  await show(gameId);
  // The match's other games load behind the one on screen, so the picker swaps at once.
  for (const id of matchGames) if (id !== gameId) fetchGame(id);
}

body.addEventListener('animationend', () => body.classList.remove('game-swap'));
// Leaving for another launcher page: mark the cross-document view transition, so the page we return to plays the
// match page's closing half (theme.css, :active-view-transition-type(from-game)).
window.addEventListener('pageswap', (e) => { try { e.viewTransition?.types?.add('from-game'); } catch { /* no types: the plain transition */ } });
showTab(tab);
load().catch(() => drawGone(NOT_FOUND));
