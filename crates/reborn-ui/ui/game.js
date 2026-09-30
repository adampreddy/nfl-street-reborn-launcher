// The native game view: one finished game from the server through the `stats_game` command (Rust does the
// request, with the player's token, so a player's own offline game opens). Opened from the Stats tab as
// game.html?game=<id>. Drawn by the same renderers as the live view (live-game-render.js), so the rules stated at
// the top of live.js hold here too: untrusted server data is drawn with textContent only, and images only ever
// come from our own /live-assets/ catalog.
import { bridge, call, el, quietCall, reason } from './shell.js';
import { PAGE_ORIGIN } from './live-common.js';
import { TABS, header, notFound } from './live-game-render.js';
import { sideTeams, snapshotOf, webLinkShown } from './stats-rows.js';
import { neighbours } from './stats-model.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const TOTALS_ONLY = 'Totals only for this game. It was played before play-by-play existed (or its plays could not be checked), and older totals may be inaccurate.';
const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const gameId = (params.get('game') || '').toLowerCase();
const screen = document.getElementById('screen');
// online: whether the game is an online one (only those get "Open on web"; see webLinkShown in stats-rows.js).
const state = { snap: null, verified: false, online: false, error: null, tab: 'plays', missing: !UUID_RE.test(gameId) };

// The list of game ids the page we came from was showing (stats.js / profile.js write this, right before
// navigating here, as the ids of their own current match list). Missing, unparsable, or not naming an ids
// array: no list to step through, same as gameList being null throughout this file.
function readGameList() {
  try {
    const parsed = JSON.parse(sessionStorage.getItem('nsr.gameList'));
    return parsed && Array.isArray(parsed.ids) ? parsed : null;
  } catch { return null; }
}
const gameList = readGameList();

function openGame(id) {
  if (!id) return;
  location.href = `game.html?game=${encodeURIComponent(id)}`;
}

// ‹ Previous / Next › above the header: drawn only when this game is actually in the list we came from
// (neighbours() alone can't tell "not in the list" apart from "at one end of it", since both give a null
// neighbour on that side).
function prevNextBar() {
  if (!gameList || !gameList.ids.includes(gameId)) return null;
  const { prev, next } = neighbours(gameList.ids, gameId);
  return el('div', { class: 'lv-actions', role: 'group', 'aria-label': gameList.label || 'Games' },
    el('button', { type: 'button', 'data-key': 'game-prev', text: '‹ Previous', disabled: prev === null, onclick: () => openGame(prev) }),
    el('button', { type: 'button', 'data-key': 'game-next', text: 'Next ›', disabled: next === null, onclick: () => openGame(next) }));
}

// Left/Right step to the neighbouring game, same as the buttons, unless the arrow key is meant for a text
// field (there is none on this page today, but the rule holds for whatever lands here later).
document.addEventListener('keydown', (e) => {
  if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
  if (!gameList) return;
  const a = document.activeElement;
  if (a && (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.tagName === 'SELECT' || a.isContentEditable)) return;
  const { prev, next } = neighbours(gameList.ids, gameId);
  openGame(e.key === 'ArrowLeft' ? prev : next);
});

function draw() {
  if (!screen) return;
  const focusKey = document.activeElement?.getAttribute?.('data-key') ?? null;
  if (state.missing) { screen.replaceChildren(...notFound('stats.html', 'Back to Stats')); return; }
  const parts = [el('div', { class: 'lv-actions' },
    // id="back": app-shell.js's controller B / Esc clicks a visible #back, so both return to Stats.
    el('a', { class: 'button lv-back', id: 'back', 'data-key': 'back', href: 'stats.html', text: '← Back' }),
    state.online ? el('button', { type: 'button', class: 'lv-watch', 'data-key': 'web', text: 'Open on web', onclick: openOnWeb }) : null)];
  if (state.error) parts.push(el('p', { class: 'lv-error', role: 'alert', text: state.error }));
  const snap = state.snap;
  if (!snap && !state.error) parts.push(el('p', { class: 'loading', text: 'Loading the game…' }));
  if (snap) {
    const game = snap.games[0];
    const bar = prevNextBar();
    if (bar) parts.push(bar);
    parts.push(header(snap, game));
    if (!state.verified) {
      parts.push(el('p', { class: 'lv-empty', text: TOTALS_ONLY }));
    } else {
      parts.push(el('div', { class: 'lv-tabs', role: 'group', 'aria-label': 'View' }, ...TABS.map(([key, label]) => el('button', {
        type: 'button', 'data-key': `tab-${key}`, 'aria-pressed': String(state.tab === key), text: label,
        onclick: () => { state.tab = key; draw(); },
      }))));
      const [, , render] = TABS.find(([key]) => key === state.tab) ?? TABS[0];
      parts.push(el('div', { class: 'lv-panel' }, render(snap, game)));
    }
  }
  screen.replaceChildren(...parts);
  screen.removeAttribute?.('aria-busy');
  if (focusKey) {
    for (const n of screen.querySelectorAll?.('[data-key]') ?? []) if (n.getAttribute('data-key') === focusKey) { n.focus(); break; }
  }
}

// The header's seat names link to each player's profile: the seat of each side gets the player id of the same
// participant participantSeats (stats-rows.js) chose for it -- the first human with an account on that side's team.
function withPlayerIds(snap, game) {
  const { team } = sideTeams(game, Array.isArray(game.teams_view) ? game.teams_view : null);
  const people = Array.isArray(game.participants) ? game.participants : [];
  (Array.isArray(snap.sides) ? snap.sides : []).forEach((s, side) => {
    const seat = s && Number.isInteger(s.seat) ? snap.seats[s.seat] : null;
    const p = seat ? people.find((x) => x && x.kind === 'human' && x.player_id && x.team_index === team[side]) : null;
    if (p && typeof p.player_id === 'string') seat.player_id = p.player_id;
  });
  return snap;
}

async function openOnWeb() {
  try { await call('open_website', { url: `${PAGE_ORIGIN}/games/${gameId}` }); } catch (e) { state.error = reason(e); draw(); }
}

async function load() {
  if (state.missing) { draw(); return; }
  if (!bridge) { state.error = 'Open this page inside the launcher window.'; draw(); return; }
  draw();
  try {
    const record = await quietCall('stats_game', { gameId });
    if (!record || typeof record !== 'object' || !record.game) state.missing = true;
    else {
      state.snap = withPlayerIds(snapshotOf(record), record.game);
      // The server's own verdict, not whether any rows came back: only a verified game's plays are trusted.
      state.verified = record.game.plays_status === 'verified';
      state.online = webLinkShown(record.game);
      state.error = null;
    }
  } catch (e) { state.error = reason(e); }
  draw();
}

load();
