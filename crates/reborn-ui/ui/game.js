// The game page (Stats, then a game): the site's /games/<id> page inside the launcher. One finished game from the
// server through the `stats_game` command (Rust does the requests, with the player's token, so a player's own offline
// game opens too), drawn by the SITE's own renderers, vendored unchanged into site/ (Tools/sync-site-render.mjs): the
// scoreboard strip, then Live Feed (the latest-play panel over All plays by drive), Box Score and Team Stats. This
// file is the site's game-page.js controller with the launcher's data source, images and controls around it:
//   - the snapshot is built exactly as game-page.js builds it (gameSnapshot, fieldFromBody, rescore);
//   - the site's renderers only ever set an <img> src that passed their own rules (a root-relative /live-assets/r/...
//     catalog path, or Discord's avatar CDN). The launcher is not that origin, so the document handed to them prefixes
//     a /live-assets/ path with the backend origin the CSP img-src allows (LIVE_ORIGIN); nothing else is rewritten;
//   - untrusted server text reaches the page through textContent only (the renderers build with createElement);
//   - the launcher keeps ← Back, the game list's ‹ Previous / Next › and Open on web above the page.
// Opened as game.html?game=<id>.
import { bridge, call, el, quietCall, reason } from './shell.js';
import { LIVE_ORIGIN, PAGE_ORIGIN } from './live-common.js';
import { webLinkShown } from './stats-rows.js';
import { backHref, neighbours } from './stats-model.js';
import { fieldFromBody, renderStrip, renderPlayPanel, renderAllPlays, renderBoxScore, renderTeamStats, gameSnapshot, rescore, letterBadge, panelStepSeq, panelPickSeq } from './site/live-render.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
// The site's own words (game-page.js).
const TOTALS_ONLY = 'Totals only for this game. It was played before play-by-play existed (or its plays could not be checked), and older totals may be inaccurate.';
const NOT_FOUND = "That game couldn't be found.";
const TABS = ['feed', 'box', 'team'];
const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const gameId = (params.get('game') || '').toLowerCase();
const $ = (id) => document.getElementById(id);
const screen = $('screen'), bar = $('game-bar'), strip = $('strip'), tabList = document.querySelector('#site-game .tabs');
const panelSlot = $('panel'), allSlot = $('allplays'), note = $('note');
const panels = { feed: $('tab-feed'), box: $('tab-box'), team: $('tab-team') };

let tab = 'feed';
let snap = null;
let verified = false;
let online = false;            // only an online game gets "Open on web" (webLinkShown, stats-rows.js)
let panelSeq = null;           // the play the panel shows; null = the last play
const open = new Map();        // "g1:d<first_seq>" -> the viewer's open/closed choice for a drive
let apMode = 'all';            // All plays or Scoring plays
const badImages = new Set();   // keyed by the SITE's src (the root-relative path), as the renderers check it
const badAvatars = new Set();

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

// ---- the launcher's row: Back, Previous / Next, Open on web ----

// The list of game ids the page we came from was showing (stats.js / profile.js write it right before navigating
// here). Missing, unparsable, or not naming an ids array: no list to step through.
function readGameList() {
  try {
    const parsed = JSON.parse(sessionStorage.getItem('nsr.gameList'));
    return parsed && Array.isArray(parsed.ids) ? parsed : null;
  } catch { return null; }
}
const gameList = readGameList();
// "← Back" returns to the page that stored the list (a profile or Stats), validated by backHref; else Stats.
const BACK = backHref(gameList?.from);

function openGame(id) {
  if (!id) return;
  location.href = `game.html?game=${encodeURIComponent(id)}`;
}

async function openOnWeb() {
  try { await call('open_website', { url: `${PAGE_ORIGIN}/games/${gameId}` }); } catch (e) { showError(reason(e)); }
}

function drawBar() {
  // id="back": app-shell.js's controller B / Esc clicks a visible #back, so both return where we came from.
  const back = el('a', { class: 'button lv-back', id: 'back', 'data-key': 'back', href: BACK, text: '← Back' });
  const end = el('div', { class: 'game-bar-end' });
  // ‹ Previous / Next ›: only when this game is in the list we came from (neighbours() alone can't tell "not in the
  // list" from "at one end of it").
  if (gameList && gameList.ids.includes(gameId)) {
    const { prev, next } = neighbours(gameList.ids, gameId);
    end.append(el('div', { class: 'game-bar-end', role: 'group', 'aria-label': gameList.label || 'Games' },
      el('button', { type: 'button', 'data-key': 'game-prev', text: '‹ Previous', disabled: prev === null, onclick: () => openGame(prev) }),
      el('button', { type: 'button', 'data-key': 'game-next', text: 'Next ›', disabled: next === null, onclick: () => openGame(next) })));
  }
  if (online) end.append(el('button', { type: 'button', 'data-key': 'web', text: 'Open on web', onclick: openOnWeb }));
  bar.replaceChildren(back, end);
}

function showError(text) {
  bar.querySelector('.game-error')?.remove();
  bar.append(el('p', { class: 'game-error', role: 'alert', text }));
}

// Left/Right step to the neighbouring game, unless the key is meant for a text field, carries a modifier (Alt+Left
// is WebView2's own "navigate back"), or was already handled (the tab list's own arrows).
document.addEventListener('keydown', (e) => {
  if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
  if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey || e.defaultPrevented) return;
  if (!gameList || !gameList.ids.includes(gameId)) return;
  const a = document.activeElement;
  if (a && (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.tagName === 'SELECT' || a.isContentEditable)) return;
  const { prev, next } = neighbours(gameList.ids, gameId);
  openGame(e.key === 'ArrowLeft' ? prev : next);
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
 *  no tabs (there is nothing under them). */
function drawGone(text, alert = false) {
  const p = para('empty', text);
  if (alert) p.setAttribute('role', 'alert');
  strip.replaceChildren(p);
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
  if (!t?.closest('#site-game')) return;
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

async function load() {
  drawBar();
  if (!UUID_RE.test(gameId)) { drawGone(NOT_FOUND); return; }
  if (!bridge) { drawGone('Open this page inside the launcher window.', true); return; }
  let record;
  try { record = await quietCall('stats_game', { gameId }); } catch (e) { drawGone(reason(e), true); return; }
  const game = record && typeof record === 'object' ? record.game : null;
  if (!game || typeof game !== 'object' || !game.id) { drawGone(NOT_FOUND); return; }
  // The server's own verdict, not whether any rows came back: only a verified game's plays are trusted.
  verified = game.plays_status === 'verified';
  online = webLinkShown(game);
  const list = (v) => (Array.isArray(v) ? v : []);
  snap = gameSnapshot(game, verified ? list(record.plays) : [], verified ? list(record.lines) : [], verified ? list(record.drives) : []);
  // The /plays route sends { body, situation } only: give every play its field and its running score_after (the
  // server's rule, rescore) so All plays shows scores; the strip keeps the capture's own final score.
  const g0 = snap.games[0];
  for (const p of g0.plays) p.field ??= fieldFromBody(p.body, p.situation);
  const captured = g0.score;
  rescore(g0);
  if (Array.isArray(captured)) g0.score = captured;
  drawBar();
  draw();
  note.textContent = verified ? 'Final' : '';
}

showTab(tab);
load().catch(() => drawGone(NOT_FOUND));
