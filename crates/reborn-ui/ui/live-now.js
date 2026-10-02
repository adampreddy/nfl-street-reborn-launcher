// "Live now" on the Online page: every match currently running on the web game, from the
// backend's public `GET /v1/live` (no sign-in). Inserted above the private-room section the same
// way online-mm.js inserts its own panel: a fresh <section> placed right before #screen.
//
// Everything the backend returns is untrusted player-typed data (a display name, a team name) and
// is drawn with `el()`'s `text` (textContent), never innerHTML -- see shell.js. An image `src` is
// set only from a URL under our own live-assets catalog or Discord's avatar CDN (the CSP `img-src`
// this launcher ships only ever allows those two prefixes); anything else draws its badge
// instead of an <img>, which is also what a custom `backend_origin` (Settings) falls back to,
// since that setting can never widen a static CSP -- see docs/superpowers/sdd's constraints for
// this feature.
import { bridge, call, el, quietCall, reason } from './shell.js';
import { PAGE_ORIGIN, assetUrl, makePoller } from './live-common.js';

// The list changes slowly (a match starts or a score moves); 10 s keeps a LAN party of launchers
// behind one address well inside the backend's rate limit.
const POLL_MS = 10000;
const HEX_RE = /^#[0-9a-fA-F]{6}$/;

/** Shows `img`; swaps to `badge` if the image never loads (wrong origin, 404, offline). */
function withImageFallback(img, badge) {
  badge.hidden = true;
  img.addEventListener('error', () => { img.hidden = true; badge.hidden = false; });
  return el('span', { class: 'live-image' }, img, badge);
}

function teamBadge(team) {
  return el('span', { class: 'live-badge', text: team && team.abbrev ? String(team.abbrev) : '?' });
}

/** A team's 128 px crest, or its abbreviation badge when there is no usable logo. */
function teamLogo(team) {
  const url = team ? assetUrl(team.logo_url) : null;
  const badge = teamBadge(team);
  if (!url) return badge;
  const img = el('img', { class: 'live-logo', src: url, width: 128, height: 128, alt: '' });
  return withImageFallback(img, badge);
}

/** The player under a team, muted beside its name; a side with no controller is the CPU. */
function playerLine(seat, cpu) {
  if (cpu) return el('em', { class: 'live-player-name live-cpu', text: 'CPU' });
  const name = seat && typeof seat.display_name === 'string' && seat.display_name ? seat.display_name : 'Unknown player';
  return el('em', { class: 'live-player-name', text: name });
}

/** One team row of a card (spec 2026-10-02 §5): crest (or its badge), team name with the player muted, score, on a faint
    fade of the team's own primary (validated, never trusted as is). The losing side is dimmed. */
function sideRow(team, score, seat, cpu, losing) {
  const attrs = { class: `live-side${losing ? ' lose' : ''}` };
  if (team && HEX_RE.test(team.primary)) attrs.style = `--tc:${team.primary}`;
  return el('div', attrs,
    teamLogo(team),
    el('span', { class: 'live-team-name' }, team && team.name ? String(team.name) : 'Team TBD', ' ', playerLine(seat, cpu)),
    el('b', { class: 'live-score-num', text: String(Number.isFinite(score) ? score : 0) }));
}

/**
 * Which seat sits under side `side`, laid out BY SIDE like the scoreboard above it: `sides[side].seat` when a real
 * (non-empty) `sides` array names one, else `{ cpu: true }` -- a side with no controller (CPU, a disconnect, or an
 * older runtime that sends no ports at all) never borrows the other side's seat. With no `sides` array at all,
 * this falls back to the seats by raw position, exactly as before ports existed.
 */
function seatOfSide(seats, sides, side) {
  if (!sides) return { seat: seats[side], cpu: false };
  const idx = sides[side]?.seat;
  return (idx === 0 || idx === 1) ? { seat: seats[idx], cpu: false } : { seat: null, cpu: true };
}

function watchLink(match) {
  const id = typeof match.id === 'string' ? match.id : '';
  const teams = Array.isArray(match.teams) ? match.teams : [null, null];
  const label = (t) => (t && t.name ? t.name : 'a team');
  const link = el('a', {
    class: 'btn sm live-watch', href: `live.html?match=${encodeURIComponent(id)}`, 'data-key': `watch-${id}`,
    'aria-label': `Watch ${label(teams[0])} vs ${label(teams[1])}`, text: 'Watch',
  });
  watchLinks.set(`watch-${id}`, link);
  return link;
}

/** The card's whole content, rebuilt fresh every poll; only the <li> that holds it is reused. */
function rowContent(match) {
  const teams = Array.isArray(match.teams) ? match.teams : [null, null];
  const score = Array.isArray(match.score) ? match.score : [0, 0];
  const seats = Array.isArray(match.seats) ? match.seats : [null, null];
  // Each player sits under the team they have (side 0 first, then side 1); a side with no seat shows CPU rather than
  // the other side's player.
  const sides = Array.isArray(match.sides) && match.sides.length ? match.sides : null;
  const s0 = seatOfSide(seats, sides, 0);
  const s1 = seatOfSide(seats, sides, 1);
  const a = Number.isFinite(score[0]) ? score[0] : 0, b = Number.isFinite(score[1]) ? score[1] : 0;
  const gameNo = Number.isInteger(match.game_seq) ? match.game_seq : 1;
  return [
    sideRow(teams[0], score[0], s0.seat, s0.cpu, a < b),
    sideRow(teams[1], score[1], s1.seat, s1.cpu, b < a),
    el('div', { class: 'live-foot' },
      el('span', { class: 'live-dot', 'aria-hidden': 'true' }),
      el('span', { class: 'live-mode-line', text: match.mode === 'ranked' ? `Ranked · Game ${gameNo}` : 'Casual' }),
      watchLink(match)),
  ];
}

// ---- the section, its keyed rows, and polling ----

const rows = new Map(); // match id -> <li>, reused across polls so a refresh never flickers
const watchLinks = new Map(); // data-key -> the Watch link drawn last, to keep its focus across a redraw

function rowFor(match) {
  const id = String(match.id);
  let li = rows.get(id);
  if (!li) {
    li = el('li', { class: 'live-row glass', 'data-match-id': id });
    rows.set(id, li);
  }
  li.replaceChildren(...rowContent(match));
  return li;
}

const screen = document.getElementById('screen');
const section = el('section', { class: 'live-now', 'aria-label': 'Live now' });
// Live now's slot under the mode area (online.html, spec 2026-10-02 §2); without one (Tools/check-live-now.mjs's
// stand-in DOM) it sits right before #screen as it always did.
const liveSlot = document.getElementById('live-slot');
if (liveSlot) liveSlot.append(section); else screen?.parentNode?.insertBefore(section, screen);

// No Tauri core attached (a bare preview, or a launcher build with no live views): nothing to
// poll, and the section stays empty, the same way online-mm.js stays out of the way without one.
if (bridge) {
  // Two fixed parts: the error line (if any) and the rows. A failed poll only ever touches the
  // first, so the last good rows -- and a Watch link that has focus -- stay exactly as they were.
  const note = el('div', { class: 'live-now-note' });
  const content = el('div', { class: 'live-now-rows' });
  const body = el('div', { class: 'live-now-body' }, note, content);
  section.append(el('div', { class: 'sec-head live-now-heading' }, el('h2', { text: 'Live now' }),
    el('button', { type: 'button', class: 'sec-link', 'data-key': 'live-all', text: 'See all ›',
      onclick: () => call('open_website', { url: `${PAGE_ORIGIN}/live` }).catch(() => {}) })), body);
  // The heading's "● {n} live games" (spec §2); hidden until the first good answer.
  const countBox = document.getElementById('on-live-count');
  function setCount(n) {
    const words = countBox?.querySelector('span');
    if (!words) return;
    words.textContent = `${n} live ${n === 1 ? 'game' : 'games'}`;
    countBox.hidden = false;
  }
  const list = el('ul', { class: 'live-rows' });

  let lastJson = null; // the list last drawn, so an unchanged poll touches nothing
  let alertText = null; // the error line on screen, replaced only when its words change

  function drawRows(matches) {
    const focusKey = [...watchLinks].find(([, n]) => n === document.activeElement)?.[0] ?? null;
    const ids = new Set(matches.map((m) => String(m?.id)));
    for (const id of [...rows.keys()]) if (!ids.has(id)) rows.delete(id);
    watchLinks.clear();
    list.replaceChildren(...matches.map((m) => rowFor(m)));
    content.replaceChildren(list);
    if (focusKey) watchLinks.get(focusKey)?.focus();
  }

  /** A good answer: clear the error line, and redraw the rows only when the list itself changed. */
  function showMatches(matches) {
    if (alertText !== null) { alertText = null; note.replaceChildren(); }
    const json = JSON.stringify(matches);
    if (json === lastJson) return;
    lastJson = json;
    setCount(matches.length);
    // Up to three cards (spec §5); "See all ›" opens the rest on the web.
    if (matches.length) { drawRows(matches.slice(0, 3)); return; }
    rows.clear();
    watchLinks.clear();
    content.replaceChildren(el('p', { class: 'live-empty', text: 'No live games right now.' }));
  }

  /** A failed poll: its one line above the last good rows, which stay. The same words twice
      leave the alert alone, so a screen reader hears it once. */
  function showError(text) {
    if (text === alertText) return;
    alertText = text;
    note.replaceChildren(el('p', { class: 'live-error', role: 'alert', text }));
  }

  async function poll() {
    let matches;
    try {
      matches = await quietCall('live_matches');
    } catch (e) {
      // The line the backend's own error already gives a player, nothing more; the poller backs off.
      showError(reason(e));
      throw e;
    }
    showMatches(Array.isArray(matches) ? matches : []);
  }

  const poller = makePoller(poll, POLL_MS);
  // Nothing polls while the page is hidden or while a game is running (the launcher's own
  // `session-state` signal from app-shell.js): a match in progress needs the network and the CPU.
  let sessionActive = false;
  function update() {
    if (document.visibilityState === 'hidden' || sessionActive) poller.stop();
    else poller.start();
  }
  document.addEventListener('visibilitychange', update);
  document.addEventListener('session-state', (e) => { sessionActive = Boolean(e?.detail); update(); });
  update();
}
