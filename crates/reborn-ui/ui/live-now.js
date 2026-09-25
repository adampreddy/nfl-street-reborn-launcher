// "Live now" on the Online page: every match currently running on the web game, from the
// backend's public `GET /v1/live` (no sign-in). Inserted above the private-room section the same
// way online-mm.js inserts its own panel: a fresh <section> placed right before #screen.
//
// Everything the backend returns is untrusted player-typed data (a display name, a team name) and
// is drawn with `el()`'s `text` (textContent), never innerHTML -- see shell.js. An image `src` is
// set only from a URL under our own live-assets catalog or Discord's avatar CDN (the CSP `img-src`
// this launcher ships only ever allows those two prefixes); anything else draws its badge/initial
// instead of an <img>, which is also what a custom `backend_origin` (Settings) falls back to,
// since that setting can never widen a static CSP -- see docs/superpowers/sdd's constraints for
// this feature.
import { bridge, el, quietCall, reason } from './shell.js';
import { inkFor } from './live-color.js';
import { assetUrl, avatarSrc, makePoller, seatsBySide } from './live-common.js';

// The list changes slowly (a match starts or a score moves); 10 s keeps a LAN party of launchers
// behind one address well inside the backend's rate limit.
const POLL_MS = 10000;
const HEX_RE = /^#[0-9a-fA-F]{6}$/;

function initialOf(name) {
  const s = String(name ?? '');
  return s ? Array.from(s)[0].toUpperCase() : '?';
}

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

/** The team's abbreviation as a chip: the catalog's own primary colour as its background (validated,
    never trusted as is), with black or white text picked by luminance for 4.5:1 contrast or better --
    the same treatment live.js's header gives a team's name, via the shared live-color.js. */
function teamAbbrev(team) {
  const attrs = { class: 'live-abbrev', text: team && team.abbrev ? String(team.abbrev) : '?' };
  if (team && HEX_RE.test(team.primary)) attrs.style = `background:${team.primary};color:${inkFor(team.primary)}`;
  return el('span', attrs);
}

function teamBlock(team, score, side) {
  return el('div', { class: `live-team live-team-${side}` },
    teamLogo(team),
    el('div', { class: 'live-team-id' },
      teamAbbrev(team),
      el('span', { class: 'live-team-name', text: team && team.name ? String(team.name) : 'Team TBD' })),
    el('span', { class: 'live-score-num', text: String(Number.isFinite(score) ? score : 0) }));
}

/** A seat's avatar, or its display name's initial when there is none (or it fails to load). */
function playerAvatar(seat) {
  const badge = el('span', { class: 'live-avatar-badge', text: initialOf(seat && seat.display_name) });
  const url = avatarSrc(seat);
  if (!url) return badge;
  const img = el('img', { class: 'live-avatar', src: url, width: 24, height: 24, alt: '' });
  return withImageFallback(img, badge);
}

function playerLine(seat) {
  const name = seat && typeof seat.display_name === 'string' && seat.display_name ? seat.display_name : 'Unknown player';
  return el('span', { class: 'live-player' }, playerAvatar(seat), el('span', { class: 'live-player-name', text: name }));
}

function watchLink(match) {
  const id = typeof match.id === 'string' ? match.id : '';
  const teams = Array.isArray(match.teams) ? match.teams : [null, null];
  const label = (t) => (t && t.name ? t.name : 'a team');
  const link = el('a', {
    class: 'button live-watch', href: `live.html?match=${encodeURIComponent(id)}`, 'data-key': `watch-${id}`,
    'aria-label': `Watch ${label(teams[0])} vs ${label(teams[1])}`, text: 'Watch',
  });
  watchLinks.set(`watch-${id}`, link);
  return link;
}

/** The row's whole content, rebuilt fresh every poll; only the <li> that holds it is reused. */
function rowContent(match) {
  const teams = Array.isArray(match.teams) ? match.teams : [null, null];
  const score = Array.isArray(match.score) ? match.score : [0, 0];
  const seats = Array.isArray(match.seats) ? match.seats : [null, null];
  // Each player sits under the team they have: side 0's seat first, then side 1's (seat order
  // when the sides are not known), so the names line up with the scoreboard above them.
  const [first, second] = seatsBySide(match.sides);
  const gameNo = Number.isInteger(match.game_seq) ? match.game_seq : 1;
  const bestOf = Number.isInteger(match.best_of) ? match.best_of : 1;
  return [
    el('div', { class: 'live-scoreboard' },
      teamBlock(teams[0], score[0], 0),
      el('span', { class: 'live-score-sep', 'aria-hidden': 'true', text: '–' }),
      teamBlock(teams[1], score[1], 1)),
    el('div', { class: 'live-meta' },
      el('span', { class: 'live-game-no', text: `Game ${gameNo} of ${bestOf}` }),
      el('span', { class: 'live-mode', text: match.mode === 'ranked' ? 'Ranked' : 'Casual' })),
    el('div', { class: 'live-players' }, playerLine(seats[first]), playerLine(seats[second])),
    watchLink(match),
  ];
}

// ---- the section, its keyed rows, and polling ----

const rows = new Map(); // match id -> <li>, reused across polls so a refresh never flickers
const watchLinks = new Map(); // data-key -> the Watch link drawn last, to keep its focus across a redraw

function rowFor(match) {
  const id = String(match.id);
  let li = rows.get(id);
  if (!li) {
    li = el('li', { class: 'live-row', 'data-match-id': id });
    rows.set(id, li);
  }
  li.replaceChildren(...rowContent(match));
  return li;
}

const screen = document.getElementById('screen');
const section = el('section', { class: 'live-now', 'aria-label': 'Live now' });
screen?.parentNode?.insertBefore(section, screen);

// No Tauri core attached (a bare preview, or a launcher build with no live views): nothing to
// poll, and the section stays empty, the same way online-mm.js stays out of the way without one.
if (bridge) {
  // Two fixed parts: the error line (if any) and the rows. A failed poll only ever touches the
  // first, so the last good rows -- and a Watch link that has focus -- stay exactly as they were.
  const note = el('div', { class: 'live-now-note' });
  const content = el('div', { class: 'live-now-rows' });
  const body = el('div', { class: 'live-now-body' }, note, content);
  section.append(el('div', { class: 'live-now-heading' }, el('h2', { text: 'Live now' })), body);
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
    if (matches.length) { drawRows(matches); return; }
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
