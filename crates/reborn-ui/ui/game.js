// The native game view: one finished game from the server through the `stats_game` command (Rust does the
// request, with the player's token, so a player's own offline game opens). Opened from the Stats tab as
// game.html?game=<id>. Drawn by the same renderers as the live view (live-game-render.js), so the rules stated at
// the top of live.js hold here too: untrusted server data is drawn with textContent only, and images only ever
// come from our own /live-assets/ catalog.
import { bridge, call, el, quietCall, reason } from './shell.js';
import { LIVE_ORIGIN } from './live-common.js';
import { TABS, header, notFound } from './live-game-render.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const TOTALS_ONLY = 'Totals only for this game. It was played before play-by-play existed (or its plays could not be checked), and older totals may be inaccurate.';
const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const gameId = (params.get('game') || '').toLowerCase();
const screen = document.getElementById('screen');
const state = { snap: null, verified: false, error: null, tab: 'plays', missing: !UUID_RE.test(gameId) };

/**
 * The snapshot shape the live renderers read, from one finished game. Mirrors the backend's
 * `gameSnapshot` (reborn-backend src/web/live-render.js), which draws the same game on the web.
 */
function snapshotOf(record) {
  const g = record.game ?? {};
  const plays = Array.isArray(record.plays) ? record.plays : [];
  const fromPlays = plays.find((p) => Array.isArray(p?.body?.teams) && p.body.teams.length === 2)?.body.teams;
  const teams = [0, 1].map((side) => ({
    side,
    tgid: Number.isInteger(fromPlays?.[side]?.tgid) ? fromPlays[side].tgid : null,
    tmc: fromPlays?.[side]?.tmc ?? null,
    name: g.teams?.[side]?.name ?? `Side ${side + 1}`,
    abbrev: g.teams?.[side]?.abbrev ?? null,
  }));
  const score = [0, 1].map((i) => (Number.isFinite(g.teams?.[i]?.score) ? g.teams[i].score : 0));
  return {
    match: { status: 'complete', best_of: 1 },
    seats: [],
    games: [{ game_seq: 1, game_id: g.id, score, teams, sides: [], plays,
      box: { provisional: false, lines: Array.isArray(record.lines) ? record.lines : [] },
      drives: Array.isArray(record.drives) ? record.drives : [] }],
  };
}

function draw() {
  if (!screen) return;
  const focusKey = document.activeElement?.getAttribute?.('data-key') ?? null;
  if (state.missing) { screen.replaceChildren(...notFound('stats.html', 'Back to Stats')); return; }
  const parts = [el('div', { class: 'lv-actions' },
    // id="back": app-shell.js's controller B / Esc clicks a visible #back, so both return to Stats.
    el('a', { class: 'button lv-back', id: 'back', 'data-key': 'back', href: 'stats.html', text: '← Back' }),
    el('button', { type: 'button', class: 'lv-watch', 'data-key': 'web', text: 'Open on web', onclick: openOnWeb }))];
  if (state.error) parts.push(el('p', { class: 'lv-error', role: 'alert', text: state.error }));
  const snap = state.snap;
  if (!snap && !state.error) parts.push(el('p', { class: 'loading', text: 'Loading the game…' }));
  if (snap) {
    const game = snap.games[0];
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

async function openOnWeb() {
  try { await call('open_website', { url: `${LIVE_ORIGIN}/games/${gameId}` }); } catch (e) { state.error = reason(e); draw(); }
}

async function load() {
  if (state.missing) { draw(); return; }
  if (!bridge) { state.error = 'Open this page inside the launcher window.'; draw(); return; }
  draw();
  try {
    const record = await quietCall('stats_game', { gameId });
    if (!record || typeof record !== 'object' || !record.game) state.missing = true;
    else {
      state.snap = snapshotOf(record);
      // The server's own verdict, not whether any rows came back: only a verified game's plays are trusted.
      state.verified = record.game.plays_status === 'verified';
      state.error = null;
    }
  } catch (e) { state.error = reason(e); }
  draw();
}

load();
