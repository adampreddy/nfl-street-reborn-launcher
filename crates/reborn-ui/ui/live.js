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
//     so the header adds the score by SIDE: "Side 1 x – y Side 2" (team names when those are known).
//
// Everything the backend returns is untrusted player-typed data (display names, team names, play text)
// and is drawn with `el()`'s `text` (textContent) only. An image `src` is set only from our own
// `/live-assets/...` catalog under the default origin, or from Discord's avatar CDN -- the only two
// prefixes the launcher's CSP `img-src` allows. A custom `backend_origin` (Settings) cannot widen that
// static CSP, so its images would simply fail to load and draw their badges instead.
import { bridge, call, el, reason } from './shell.js';

const LIVE_ORIGIN = 'https://api.nflstreetreborn.com';
const AVATAR_PREFIX = 'https://cdn.discordapp.com/avatars/';
const ASSET_RE = /^\/live-assets\/[A-Za-z0-9._/-]+\.png$/;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const HEX_RE = /^#[0-9a-fA-F]{6}$/;
const DASH = '–';
const BIG_YARDS = 20;
const POLL_MS = 3000;
const NOT_FOUND = "That game couldn't be found.";
// A match in one of these states will never change again: draw it once more, then stop asking.
const ENDED = new Set(['complete', 'abandoned', 'void']);
const STATUS_WORD = { running: 'Live', complete: 'Final', held: 'Under review', abandoned: 'Abandoned', void: 'Void', pairing: 'Starting', lobby: 'Starting' };

// ---- small, pure helpers ----

const num = (v) => (Number.isFinite(v) ? v : 0);
const seqOf = (p) => (Number.isInteger(p?.body?.seq) ? p.body.seq : p?.seq);
const str = (v) => (typeof v === 'string' ? v : '');

/** A `/live-assets/...png` path as an absolute URL under the default origin, or null. */
function assetUrl(path) {
  return typeof path === 'string' && ASSET_RE.test(path) && !path.includes('..') ? LIVE_ORIGIN + path : null;
}

function avatarSrc(seat) {
  const u = str(seat?.avatar_url);
  return u.startsWith(AVATAR_PREFIX) ? u : null;
}

function initialOf(name) {
  const s = String(name ?? '');
  return s ? Array.from(s)[0].toUpperCase() : '?';
}

/** WCAG relative luminance of a validated #rrggbb. */
function luminance(hex) {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

/** Black or white text on `hex`, whichever contrasts more (the better of the two is always 4.58:1 or more). */
function inkFor(hex) {
  const l = luminance(hex);
  return (l + 0.05) / 0.05 >= 1.05 / (l + 0.05) ? '#000000' : '#ffffff';
}

function newestGame(snap) {
  let top = null;
  for (const g of Array.isArray(snap?.games) ? snap.games : []) if (g && Number.isInteger(g.game_seq) && (!top || g.game_seq > top.game_seq)) top = g;
  return top;
}

function playsOf(game) {
  return (Array.isArray(game?.plays) ? game.plays : []).filter((p) => p && Number.isInteger(seqOf(p)));
}

/** Which side each seat has: `[sideOfSeat0, sideOfSeat1]`, each 0|1|null. */
function sideOfSeats(sides) {
  const out = [null, null];
  (Array.isArray(sides) ? sides : []).forEach((s, side) => { if (s && (s.seat === 0 || s.seat === 1) && side < 2) out[s.seat] = side; });
  return out;
}

/** A by-side score as `[seat0, seat1]`, or null when the sides are not known. */
function seatScore(score, sides) {
  if (!Array.isArray(score)) return null;
  const [s0, s1] = sideOfSeats(sides);
  if (s0 === null || s1 === null) return null;
  return [num(score[s0]), num(score[s1])];
}

/** A side's team, only when it has a name. */
function teamOf(game, side) {
  const t = side === 0 || side === 1 ? (Array.isArray(game?.teams) ? game.teams[side] : null) : null;
  return t && typeof t.name === 'string' && t.name ? t : null;
}

/** What names a side in a table: its team, else the seat that has it, else "Side N". */
function sideLabel(game, seats, side) {
  const team = teamOf(game, side);
  if (team) return team.name;
  const seat = Array.isArray(game?.sides) ? game.sides[side]?.seat : null;
  const name = (seat === 0 || seat === 1) ? str(seats?.[seat]?.display_name) : '';
  return name || (side === 0 || side === 1 ? `Side ${side + 1}` : 'Unknown');
}

/** Live (the newest game of a running match, not yet recorded) or Final, and friends. */
function gameStatus(snap, game) {
  const status = str(snap?.match?.status);
  const newest = newestGame(snap);
  if (game && (game.game_id || (newest && game.game_seq < newest.game_seq))) return { word: 'Final', live: false };
  return { word: STATUS_WORD[status] ?? (status || 'Live'), live: status === 'running' };
}

// ---- images, each with its badge fallback ----

// URLs that already failed to load on this page: drawn as their badge straight away on every redraw, rather than
// a fresh <img> that asks again and sits empty until it fails again.
const failed = new Set();

/** Shows `img`; swaps to `badge` (when given) if the image never loads (wrong origin, 404, offline). */
function withFallback(img, badge) {
  const src = img.getAttribute('src');
  if (failed.has(src)) return badge;
  img.addEventListener('error', () => { failed.add(src); img.hidden = true; if (badge) badge.hidden = false; });
  if (!badge) return img;
  badge.hidden = true;
  return el('span', { class: 'lv-image' }, img, badge);
}

function teamLogo(team) {
  const url = assetUrl(team.logo_url);
  const badge = el('span', { class: 'lv-logo-badge', text: str(team.abbrev) || initialOf(team.name) });
  if (!url) return badge;
  return withFallback(el('img', { class: 'lv-logo', src: url, width: 256, height: 256, alt: '' }), badge);
}

function seatAvatar(seat) {
  const badge = el('span', { class: 'lv-avatar-badge', text: initialOf(seat?.display_name) });
  const url = avatarSrc(seat);
  if (!url) return badge;
  return withFallback(el('img', { class: 'lv-avatar', src: url, width: 32, height: 32, alt: '' }), badge);
}

function face(url, cls, size) {
  const src = assetUrl(url);
  if (!src || !/\/portraits\//.test(src)) return null;
  return withFallback(el('img', { class: cls, src, width: size, height: size, alt: '' }), null);
}

// ---- the header ----

function teamName(team) {
  const attrs = { class: 'lv-team-name', text: team.name };
  if (HEX_RE.test(str(team.primary))) attrs.style = `background:${team.primary};color:${inkFor(team.primary)}`;
  return el('span', attrs);
}

function seatCard(i, seat, team, points) {
  const name = str(seat?.display_name) || 'Unknown player';
  return el('div', { class: `lv-seat lv-seat-${i}${team ? ' has-team' : ''}` },
    team ? teamLogo(team) : null,
    el('div', { class: 'lv-seat-id' },
      team ? teamName(team) : null,
      el('span', { class: 'lv-player' }, seatAvatar(seat), el('span', { class: 'lv-player-name', text: name }))),
    points === null ? null : el('span', { class: 'lv-points', text: String(points) }));
}

function header(snap, game) {
  const match = snap.match ?? {};
  const seats = Array.isArray(snap.seats) ? snap.seats : [];
  const plays = playsOf(game);
  const sideOf = sideOfSeats(game?.sides);
  const byGame = seatScore(game?.score, game?.sides);
  const sideScore = Array.isArray(game?.score) ? [num(game.score[0]), num(game.score[1])] : null;
  const hasAny = plays.length > 0 || Boolean(sideScore && (sideScore[0] || sideScore[1]));
  // Unknown sides: no seat is credited with anything; the side-labelled line below carries the score.
  const pointsOf = (i) => (byGame ? byGame[i] : null);

  const root = el('div', { class: 'lv-header' });
  root.append(el('div', { class: 'lv-scoreboard' },
    seatCard(0, seats[0], teamOf(game, sideOf[0]), pointsOf(0)),
    el('span', { class: 'lv-vs', 'aria-hidden': 'true', text: byGame ? DASH : 'vs' }),
    seatCard(1, seats[1], teamOf(game, sideOf[1]), pointsOf(1))));
  if (!byGame && sideScore && hasAny) {
    const label = (side) => teamOf(game, side)?.name ?? `Side ${side + 1}`;
    root.append(el('p', { class: 'lv-side-score', text: `${label(0)} ${sideScore[0]} ${DASH} ${sideScore[1]} ${label(1)}` }));
  }

  const st = gameStatus(snap, game);
  const bestOf = Number.isInteger(match.best_of) ? match.best_of : 1;
  const meta = el('div', { class: 'lv-meta' },
    el('span', { class: `lv-status ${st.live ? 'live' : 'done'}`, text: st.word }),
    el('span', { class: 'lv-game-no', text: `Game ${game?.game_seq ?? 1} of ${bestOf}` }));
  if (bestOf > 1) {
    const series = Array.isArray(match.series) ? match.series : [0, 0];
    meta.append(el('span', { class: 'lv-series', text: `Series ${num(series[0])}${DASH}${num(series[1])}` }));
  }
  if (match.mode === 'ranked' || match.mode === 'casual') meta.append(el('span', { class: 'lv-mode', text: match.mode === 'ranked' ? 'Ranked' : 'Casual' }));
  root.append(meta);

  if (st.live) {
    let top = null;
    for (const p of plays) if (p.status !== 'disputed' && (!top || seqOf(p) > seqOf(top))) top = p;
    if (top && str(top.situation)) root.append(el('p', { class: 'lv-situation', text: top.situation }));
  }
  return root;
}

// ---- the tabs ----

/** DIGP -> portrait path from this game's lines, then (same DIGP and same name) any other game's in the match. */
function portraitsFor(snap, game) {
  const own = new Map();
  const other = new Map();
  for (const g of Array.isArray(snap.games) ? snap.games : []) {
    for (const l of Array.isArray(g?.box?.lines) ? g.box.lines : []) {
      if (!l || !Number.isInteger(l.digp) || !assetUrl(l.portrait_url)) continue;
      if (g === game) own.set(l.digp, l.portrait_url);
      else other.set(`${l.digp}|${str(l.name)}`, l.portrait_url);
    }
  }
  return (line) => (assetUrl(line?.portrait_url) ? line.portrait_url : own.get(line?.digp) ?? other.get(`${line?.digp}|${str(line?.name)}`) ?? null);
}

const isBig = (s) => s.touchdown === true || s.turnover === true || (s.yards_known === true && Number.isFinite(s.yards) && s.yards >= BIG_YARDS);
function starOf(s) {
  for (const d of [s.receiver, s.rusher, s.passer]) if (Number.isInteger(d) && d >= 0) return d;
  return null;
}

function playsTab(snap, game) {
  const plays = playsOf(game).sort((a, b) => seqOf(b) - seqOf(a));
  if (!plays.length) return el('p', { class: 'lv-empty', text: game?.game_id ? 'No live plays reached us for this game.' : 'No plays yet.' });
  const portraitOf = portraitsFor(snap, game);
  const list = el('ol', { class: 'lv-plays' });
  for (const p of plays) {
    const s = p.body?.summary ?? {};
    const disputed = p.status === 'disputed';
    const scored = !disputed && (s.touchdown === true || (Number.isFinite(s.points) && s.points > 0));
    const turnover = !disputed && s.turnover === true;
    const star = !disputed && isBig(s) ? face(portraitOf({ digp: starOf(s) }), 'lv-play-face', 48) : null;
    const cls = ['lv-play'];
    if (scored) cls.push('score');
    if (turnover) cls.push('turnover');
    if (disputed) cls.push('disputed');
    if (star) cls.push('has-portrait');

    const top = el('div', { class: 'lv-play-top' }, el('span', { class: 'lv-play-seq', text: `#${seqOf(p)}` }));
    if (str(p.situation)) top.append(el('span', { class: 'lv-play-sit', text: p.situation }));
    const after = seatScore(p.score_after, game?.sides);
    if (after) top.append(el('span', { class: 'lv-play-after', text: `${after[0]}${DASH}${after[1]}` }));
    if (disputed) top.append(el('span', { class: 'lv-badge disputed', text: 'Disputed' }));
    else if (turnover) top.append(el('span', { class: 'lv-badge turnover', text: 'Turnover' }));
    const words = str(s.text) || str(s.kind) || 'Play';
    list.append(el('li', { class: cls.join(' ') }, star, el('div', { class: 'lv-play-body' }, top, el('p', { class: 'lv-play-text', text: words }))));
  }
  return list;
}

const TABLES = [
  { title: 'Passing', has: (s) => num(s.pass_att) > 0, sort: (s) => num(s.pass_yds),
    cols: [['C/ATT', (s) => `${num(s.pass_cmp)}/${num(s.pass_att)}`], ['YDS', (s) => num(s.pass_yds)], ['TD', (s) => num(s.pass_td)],
      ['INT', (s) => num(s.pass_int)], ['RTG', (s) => (Number.isFinite(s.pass_rtg) ? s.pass_rtg : DASH)]] },
  { title: 'Rushing', has: (s) => num(s.rush_att) > 0, sort: (s) => num(s.rush_yds),
    cols: [['ATT', (s) => num(s.rush_att)], ['YDS', (s) => num(s.rush_yds)], ['TD', (s) => num(s.rush_td)],
      ['LONG', (s) => (Number.isFinite(s.rush_long) ? s.rush_long : DASH)]] },
  { title: 'Receiving', has: (s) => num(s.rec) > 0, sort: (s) => num(s.rec_yds),
    cols: [['REC', (s) => num(s.rec)], ['YDS', (s) => num(s.rec_yds)], ['TD', (s) => num(s.rec_td)],
      ['LONG', (s) => (Number.isFinite(s.rec_long) ? s.rec_long : DASH)]] },
  { title: 'Defense', has: (s) => num(s.tackles) + num(s.sacks) + num(s.def_int) + num(s.ff) + num(s.fr) > 0,
    sort: (s) => num(s.tackles) + num(s.sacks) * 2 + num(s.def_int) * 3,
    cols: [['TKL', (s) => num(s.tackles)], ['SCK', (s) => num(s.sacks)], ['INT', (s) => num(s.def_int)], ['FF', (s) => num(s.ff)]] },
];

function boxTab(snap, game) {
  const box = game?.box ?? {};
  const lines = (Array.isArray(box.lines) ? box.lines : []).filter((l) => l && l.stats && typeof l.stats === 'object');
  const portraitOf = portraitsFor(snap, game);
  const teamCol = (i) => { const t = teamOf(game, i); return t ? (str(t.abbrev) || t.name) : `Side ${i === 1 ? 2 : 1}`; };
  const root = el('div', { class: 'lv-box' },
    el('h3', { class: 'lv-box-title', text: box.provisional === false ? 'Final box score' : 'Box score (provisional)' }));
  let any = false;
  for (const t of TABLES) {
    const rows = lines.filter((l) => t.has(l.stats))
      .sort((a, b) => num(a.team_index) - num(b.team_index) || t.sort(b.stats) - t.sort(a.stats));
    if (!rows.length) continue;
    any = true;
    const head = el('tr', {}, el('th', { class: 'lv-who', scope: 'col', text: 'Player' }), el('th', { scope: 'col', text: 'Team' }),
      ...t.cols.map(([h]) => el('th', { scope: 'col', text: h })));
    const body = el('tbody', {}, ...rows.map((l) => el('tr', { class: `t${l.team_index === 1 ? 1 : 0}` },
      el('td', { class: 'lv-who' }, face(portraitOf(l), 'lv-face', 28), el('span', { text: str(l.name) || `#${l.digp}` })),
      el('td', { class: 'lv-team-col', text: teamCol(l.team_index) }),
      ...t.cols.map(([, f]) => el('td', { text: String(f(l.stats)) })))));
    root.append(el('table', { class: `lv-table ${t.title.toLowerCase()}` }, el('caption', { text: t.title }), el('thead', {}, head), body));
  }
  if (!any) root.append(el('p', { class: 'lv-empty', text: 'No stats yet.' }));
  return root;
}

const RESULT_WORD = { touchdown: 'Touchdown', turnover: 'Turnover', downs: 'Downs', end: 'End of game' };

function drivesTab(snap, game) {
  const list = Array.isArray(game?.drives) ? game.drives.filter((d) => d && typeof d === 'object') : [];
  if (!list.length) return el('p', { class: 'lv-empty lv-drives', text: 'No drives yet.' });
  const seats = Array.isArray(snap.seats) ? snap.seats : [];
  const head = el('tr', {}, ...['#', 'Offense', 'Plays', 'Yards', 'Result'].map((h) => el('th', { scope: 'col', class: h === 'Offense' ? 'lv-who' : null, text: h })));
  const body = el('tbody', {}, ...list.map((d, i) => el('tr', { class: d.result === 'touchdown' ? 'score' : d.result === 'turnover' ? 'turnover' : null },
    el('td', { text: String(i + 1) }),
    el('td', { class: 'lv-who', text: sideLabel(game, seats, d.side) }),
    el('td', { text: String(num(d.plays)) }),
    el('td', { text: String(num(d.yards)) }),
    el('td', { text: RESULT_WORD[d.result] ?? str(d.result) }))));
  return el('table', { class: 'lv-table lv-drives' },
    el('caption', { text: list.some((d) => d.provisional) ? 'Drives (provisional)' : 'Drives' }), el('thead', {}, head), body);
}

const TABS = [['plays', 'Plays', playsTab], ['box', 'Box score', boxTab], ['drives', 'Drives', drivesTab]];

// ---- the page ----

const screen = document.getElementById('screen');
const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const matchId = params.get('match') ?? '';

function notFound() {
  screen?.replaceChildren(
    el('p', { class: 'lv-empty', role: 'alert', text: NOT_FOUND }),
    el('a', { class: 'button', href: 'online.html', text: 'Back to Online' }));
}

const state = { snap: null, error: null, picked: null, tab: 'plays', done: false };
let lastText = null;
let timer = null;

function pickedGame() {
  const games = Array.isArray(state.snap?.games) ? state.snap.games.filter((g) => g && Number.isInteger(g.game_seq)) : [];
  return games.find((g) => g.game_seq === state.picked) ?? newestGame(state.snap);
}

function draw() {
  if (!screen) return;
  const focusKey = document.activeElement?.getAttribute?.('data-key') ?? null;
  const parts = [];
  parts.push(el('div', { class: 'lv-actions' },
    el('a', { class: 'button lv-back', href: 'online.html', text: '← Back' }),
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
  try { await call('open_website', { url: `${LIVE_ORIGIN}/live/${matchId}` }); } catch (e) { state.error = reason(e); draw(); }
}

function stop() {
  if (timer) { clearInterval(timer); timer = null; }
}
function start() {
  stop();
  if (!state.done && document.visibilityState !== 'hidden') timer = setInterval(poll, POLL_MS);
}

let inflight = false;
async function poll() {
  if (inflight || state.done) return;
  inflight = true;
  try {
    const snap = await call('live_match', { matchId });
    if (!snap || typeof snap !== 'object' || !Array.isArray(snap.games)) {
      state.done = true;
      stop();
      notFound();
      return;
    }
    const text = JSON.stringify(snap);
    const hadError = state.error !== null;
    state.error = null;
    if (text !== lastText || hadError) { lastText = text; state.snap = snap; draw(); }
    if (ENDED.has(str(snap.match?.status))) { state.done = true; stop(); }
  } catch (e) {
    // Quiet retries: the one line the core already words for a player, over the last good view.
    const was = state.error;
    state.error = reason(e);
    if (was !== state.error) draw();
  } finally {
    inflight = false;
  }
}

if (!UUID_RE.test(matchId)) {
  notFound();
} else {
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') { stop(); return; }
    if (state.done) return;
    poll();
    start();
  });
  window.addEventListener?.('pagehide', stop);
  window.addEventListener?.('pageshow', (e) => { if (e?.persisted && !state.done) { poll(); start(); } });
  if (!bridge) {
    state.error = 'Open this page inside the launcher window.';
    draw();
  } else {
    await poll();
    start();
  }
}
