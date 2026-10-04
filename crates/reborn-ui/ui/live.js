// The native match view: one live (or just finished) match from the backend's public
// `GET /v1/live/:id` snapshot, read through the `live_match` command (Rust does the request; this page
// never fetches the backend itself). Opened from "Live now" on the Online page as live.html?match=<id>.
//
// The rules the backend's own web page (reborn-backend src/web/live-render.js) already follows, re-stated
// here rather than shared, because the launcher ships its own files:
//   - A game's score is the snapshot's own `score` (the server rescored it from every play in seq order);
//     a DISPUTED play's points never count, and a disputed play never sets the header's situation line,
//     never gets a score or turnover highlight and never gets a portrait: only its own card and badge.
//   - The header follows the game the viewer picked (the newest by default).
//   - When the sides are unknown (an older runtime sends no ports), no seat can be credited with a score,
//     so the header adds the score by SIDE: "Side 1 x-y Side 2" (team names when those are known).
//
// Everything the backend returns is untrusted player-typed data (display names, team names, play text)
// and is drawn with `el()`'s `text` (textContent) only. An image `src` is set only from our own
// `/live-assets/...` catalog under the default origin, or from Discord's avatar CDN -- the only two
// prefixes the launcher's CSP `img-src` allows. A custom `backend_origin` (Settings) cannot widen that
// static CSP, so its images would simply fail to load and draw their badges instead.
// The renderers (header, the three tabs, "not found") live in live-game-render.js.
import { bridge, call, el, quietCall, reason } from './shell.js';
import { PAGE_ORIGIN, makePoller } from './live-common.js';
import { TABS, header, newestGame, notFound } from './live-game-render.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const POLL_MS = 3000;
// A match in one of these states will never change again: draw it once more, then stop asking.
const ENDED = new Set(['complete', 'abandoned', 'void']);

const str = (v) => (typeof v === 'string' ? v : '');

// ---- the page ----

const screen = document.getElementById('screen');
const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const matchId = params.get('match') ?? '';

function showNotFound() {
  screen?.replaceChildren(...notFound('online.html', 'Back to Online'));
}

const state = { snap: null, error: null, picked: null, tab: 'plays', done: false };
let lastText = null;

function pickedGame() {
  const games = Array.isArray(state.snap?.games) ? state.snap.games.filter((g) => g && Number.isInteger(g.game_seq)) : [];
  return games.find((g) => g.game_seq === state.picked) ?? newestGame(state.snap);
}

function draw() {
  if (!screen) return;
  const focusKey = document.activeElement?.getAttribute?.('data-key') ?? null;
  const parts = [];
  parts.push(el('div', { class: 'lv-actions' },
    // id="back": app-shell.js's controller B / Esc clicks a visible #back, so both return to Online.
    el('a', { class: 'button lv-back', id: 'back', 'data-key': 'back', href: 'online.html', text: '← Back' }),
    el('button', { type: 'button', class: 'lv-watch', 'data-key': 'watch', text: 'Watch on web', onclick: watchOnWeb })));
  if (state.error) parts.push(el('p', { class: 'lv-error', role: 'alert', text: state.error }));
  const snap = state.snap;
  if (snap) {
    const game = pickedGame();
    parts.push(header(snap, game));
    const games = (Array.isArray(snap.games) ? snap.games : []).filter((g) => g && Number.isInteger(g.game_seq)).sort((a, b) => a.game_seq - b.game_seq);
    if (games.length > 1) {
      parts.push(el('div', { class: 'lv-picker', role: 'group', 'aria-label': 'Game' }, ...games.map((g) => el('button', {
        type: 'button', 'data-key': `game-${g.game_seq}`, 'aria-pressed': String(g === game), text: `Game ${g.game_seq}`,
        onclick: () => { state.picked = g.game_seq; draw(); },
      }))));
    }
    parts.push(el('div', { class: 'lv-tabs', role: 'group', 'aria-label': 'View' }, ...TABS.map(([key, label]) => el('button', {
      type: 'button', 'data-key': `tab-${key}`, 'aria-pressed': String(state.tab === key), text: label,
      onclick: () => { state.tab = key; draw(); },
    }))));
    const [, , render] = TABS.find(([key]) => key === state.tab) ?? TABS[0];
    parts.push(el('div', { class: 'lv-panel' }, game ? render(snap, game) : el('p', { class: 'lv-empty', text: 'No plays yet.' })));
  }
  screen.replaceChildren(...parts);
  screen.removeAttribute?.('aria-busy');
  if (focusKey) {
    for (const n of screen.querySelectorAll?.('[data-key]') ?? []) if (n.getAttribute('data-key') === focusKey) { n.focus(); break; }
  }
}

async function watchOnWeb() {
  try { await call('open_website', { url: `${PAGE_ORIGIN}/live/${matchId}` }); } catch (e) { state.error = reason(e); draw(); }
}

// One request at a time; errors back off from 5 s up to 60 s (a rate limit waits a full minute)
// and the normal 3 s resumes after the first good answer -- see live-common.js.
const poller = makePoller(poll, POLL_MS);

async function poll() {
  if (state.done) { poller.stop(); return; }
  let snap;
  try {
    snap = await quietCall('live_match', { matchId });
  } catch (e) {
    // The one line the core already words for a player, over the last good view (which stays).
    const was = state.error;
    state.error = reason(e);
    if (was !== state.error) draw();
    throw e;
  }
  if (!snap || typeof snap !== 'object' || !Array.isArray(snap.games)) {
    state.done = true;
    poller.stop();
    showNotFound();
    return;
  }
  const text = JSON.stringify(snap);
  const hadError = state.error !== null;
  state.error = null;
  if (text !== lastText || hadError) { lastText = text; state.snap = snap; draw(); }
  if (ENDED.has(str(snap.match?.status))) { state.done = true; poller.stop(); }
}

// Nothing polls while the page is hidden, after navigating away, once the match has ended, or
// while a game is running (app-shell.js's `session-state`): a match in progress needs the network.
let sessionActive = false;
let away = false;
function update() {
  if (state.done || away || sessionActive || document.visibilityState === 'hidden') poller.stop();
  else poller.start();
}

if (!UUID_RE.test(matchId)) {
  showNotFound();
} else {
  document.addEventListener('visibilitychange', update);
  document.addEventListener('session-state', (e) => { sessionActive = Boolean(e?.detail); update(); });
  window.addEventListener?.('pagehide', () => { away = true; update(); });
  window.addEventListener?.('pageshow', (e) => { if (e?.persisted) { away = false; update(); } });
  if (!bridge) {
    state.error = 'Open this page inside the launcher window.';
    draw();
  } else {
    update();
  }
}
