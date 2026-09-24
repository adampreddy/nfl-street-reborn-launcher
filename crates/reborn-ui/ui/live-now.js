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
import { bridge, call, el, reason } from './shell.js';
import { inkFor } from './live-color.js';

// The backend origin the CSP's img-src is pinned to (identity::DEFAULT_ORIGIN on the Rust side).
// Deliberately NOT read from a setting: a player-configured `backend_origin` cannot change what
// the static CSP allows, so image URLs are always built against this default regardless of which
// origin `live_matches` actually talked to. A mismatch (a self-hosted backend) simply never
// produces a URL under this prefix, and the row draws its badge instead of a blocked image.
const LIVE_ORIGIN = 'https://api.nflstreetreborn.com';
const AVATAR_PREFIX = 'https://cdn.discordapp.com/avatars/';
const HEX_RE = /^#[0-9a-fA-F]{6}$/;

/** A same-origin `/live-assets/...` path, absolute under the default origin, or null. */
function assetUrl(path) {
  return typeof path === 'string' && path.startsWith('/live-assets/') ? LIVE_ORIGIN + path : null;
}

/** A seat's Discord avatar URL, only when it is already exactly that CDN's prefix. */
function avatarSrc(seat) {
  const u = seat && typeof seat.avatar_url === 'string' ? seat.avatar_url : '';
  return u.startsWith(AVATAR_PREFIX) ? u : null;
}

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
  return el('a', {
    class: 'button live-watch', href: `live.html?match=${encodeURIComponent(id)}`,
    'aria-label': `Watch ${label(teams[0])} vs ${label(teams[1])}`, text: 'Watch',
  });
}

/** The row's whole content, rebuilt fresh every poll; only the <li> that holds it is reused. */
function rowContent(match) {
  const teams = Array.isArray(match.teams) ? match.teams : [null, null];
  const score = Array.isArray(match.score) ? match.score : [0, 0];
  const seats = Array.isArray(match.seats) ? match.seats : [null, null];
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
    el('div', { class: 'live-players' }, playerLine(seats[0]), playerLine(seats[1])),
    watchLink(match),
  ];
}

// ---- the section, its keyed rows, and polling ----

const rows = new Map(); // match id -> <li>, reused across polls so a refresh never flickers

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
  const body = el('div', { class: 'live-now-body' });
  section.append(el('div', { class: 'live-now-heading' }, el('h2', { text: 'Live now' })), body);
  const list = el('ul', { class: 'live-rows' });

  function render(state) {
    if (state.error) {
      body.replaceChildren(el('p', { class: 'live-error', role: 'alert', text: state.error }));
      return;
    }
    const matches = Array.isArray(state.matches) ? state.matches : [];
    if (!matches.length) {
      rows.clear();
      body.replaceChildren(el('p', { class: 'live-empty', text: 'No live games right now.' }));
      return;
    }
    const ids = new Set(matches.map((m) => String(m?.id)));
    for (const id of [...rows.keys()]) if (!ids.has(id)) rows.delete(id);
    list.replaceChildren(...matches.map((m) => rowFor(m)));
    body.replaceChildren(list);
  }

  const state = { matches: null, error: null };
  async function poll() {
    try {
      const matches = await call('live_matches');
      state.matches = Array.isArray(matches) ? matches : [];
      state.error = null;
    } catch (e) {
      // Quiet retries: the line the backend's own error already gives a player, nothing more.
      state.error = reason(e);
    }
    render(state);
  }

  let timer = null;
  function stop() {
    if (timer) { clearInterval(timer); timer = null; }
  }
  function start() {
    stop();
    if (document.visibilityState !== 'hidden') timer = setInterval(poll, 5000);
  }

  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') { stop(); return; }
    poll();
    start();
  });

  poll();
  start();
}
