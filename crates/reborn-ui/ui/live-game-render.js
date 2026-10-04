// The renderers for one game's view: the live match view (live.js), and notFound() for Profile. (The Stats tab's
// match page, match.js, draws with the site's own renderers, vendored in site/.) Pure drawing from a snapshot:
// no requests, no page state. The rules they follow are stated at the top of live.js.
//
// Everything drawn here is untrusted player-typed data (display names, team names, play text) and goes through
// `el()`'s `text` (textContent) only. An image `src` is set only from our own `/live-assets/...` catalog under the
// default origin (assetUrl) or from Discord's avatar CDN (avatarSrc) -- the two prefixes the launcher's CSP allows.
import { el } from './shell.js';
import { inkFor } from './live-color.js';
import { assetUrl, avatarSrc, sideOfSeats } from './live-common.js';

const HEX_RE = /^#[0-9a-fA-F]{6}$/;
const DASH = '–'; // a box-score cell with no number
const SEP = '-'; // scores and series: a plain hyphen, "36-12", "Series 2-1" (owner ruling, launch audit M11)
const BIG_YARDS = 20;
const NOT_FOUND = "That game couldn't be found.";
const STATUS_WORD = { running: 'Live', complete: 'Final', held: 'Under review', abandoned: 'Abandoned', void: 'Void', pairing: 'Starting', lobby: 'Starting' };

// ---- small, pure helpers ----

const num = (v) => (Number.isFinite(v) ? v : 0);
const seqOf = (p) => (Number.isInteger(p?.body?.seq) ? p.body.seq : p?.seq);
const str = (v) => (typeof v === 'string' ? v : '');

export function initialOf(name) {
  const s = String(name ?? '');
  return s ? Array.from(s)[0].toUpperCase() : '?';
}

export function newestGame(snap) {
  let top = null;
  for (const g of Array.isArray(snap?.games) ? snap.games : []) if (g && Number.isInteger(g.game_seq) && (!top || g.game_seq > top.game_seq)) top = g;
  return top;
}

export function playsOf(game) {
  return (Array.isArray(game?.plays) ? game.plays : []).filter((p) => p && Number.isInteger(seqOf(p)));
}

/** A by-side score as `[seat0, seat1]`, or null when the sides are not known. */
export function seatScore(score, sides) {
  if (!Array.isArray(score)) return null;
  const [s0, s1] = sideOfSeats(sides);
  if (s0 === null || s1 === null) return null;
  return [num(score[s0]), num(score[s1])];
}

/** A side's team, only when it has a name. */
export function teamOf(game, side) {
  const t = side === 0 || side === 1 ? (Array.isArray(game?.teams) ? game.teams[side] : null) : null;
  return t && typeof t.name === 'string' && t.name ? t : null;
}

/** What names a side in a table: its team, else the seat that has it, else "Side N". */
export function sideLabel(game, seats, side) {
  const team = teamOf(game, side);
  if (team) return team.name;
  const seat = Array.isArray(game?.sides) ? game.sides[side]?.seat : null;
  const name = (seat === 0 || seat === 1) ? str(seats?.[seat]?.display_name) : '';
  return name || (side === 0 || side === 1 ? `Side ${side + 1}` : 'Unknown');
}

/** Live (the newest game of a running match, not yet recorded) or Final, and friends. */
export function gameStatus(snap, game) {
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
export function withFallback(img, badge) {
  const src = img.getAttribute('src');
  if (failed.has(src)) return badge;
  img.addEventListener('error', () => { failed.add(src); img.hidden = true; if (badge) badge.hidden = false; });
  if (!badge) return img;
  badge.hidden = true;
  return el('span', { class: 'lv-image' }, img, badge);
}

export function teamLogo(team) {
  const url = assetUrl(team.logo_url);
  const badge = el('span', { class: 'lv-logo-badge', text: str(team.abbrev) || initialOf(team.name) });
  if (!url) return badge;
  return withFallback(el('img', { class: 'lv-logo', src: url, width: 256, height: 256, alt: '' }), badge);
}

export function seatAvatar(seat) {
  const badge = el('span', { class: 'lv-avatar-badge', text: initialOf(seat?.display_name) });
  const url = avatarSrc(seat);
  if (!url) return badge;
  return withFallback(el('img', { class: 'lv-avatar', src: url, width: 32, height: 32, alt: '' }), badge);
}

export function face(url, cls, size) {
  const src = assetUrl(url);
  if (!src || !/\/portraits\//.test(src)) return null;
  return withFallback(el('img', { class: cls, src, width: size, height: size, alt: '' }), null);
}

// ---- the header ----

export function teamName(team) {
  const attrs = { class: 'lv-team-name', text: team.name };
  if (HEX_RE.test(str(team.primary))) attrs.style = `background:${team.primary};color:${inkFor(team.primary)}`;
  return el('span', attrs);
}

// A seat's name; a link to the player's profile when the seat carries their player id.
const PLAYER_ID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
function seatName(seat) {
  const attrs = { class: 'lv-player-name', text: str(seat?.display_name) || 'Unknown player' };
  const id = str(seat?.player_id);
  return PLAYER_ID_RE.test(id) ? el('a', { ...attrs, href: `profile.html?player=${encodeURIComponent(id.toLowerCase())}` }) : el('span', attrs);
}

export function seatCard(i, seat, team, points, cpu) {
  const body = cpu
    ? el('span', { class: 'lv-player lv-cpu' }, el('span', { class: 'lv-player-name', text: 'CPU' }))
    : el('span', { class: 'lv-player' }, seatAvatar(seat), seatName(seat));
  return el('div', { class: `lv-seat lv-seat-${i}${team ? ' has-team' : ''}` },
    team ? teamLogo(team) : null,
    el('div', { class: 'lv-seat-id' },
      team ? teamName(team) : null,
      body),
    points === null ? null : el('span', { class: 'lv-points', text: String(points) }));
}

/**
 * Laid out BY SIDE: side 0 and side 1 each always get a card, from `game.teams` and `game.score` (by side) -- a
 * side is never dropped for having no controller. A card's Discord seat is whichever seat maps to that side
 * (`sides[side].seat`); a side with no seat -- CPU, a disconnect, or an older runtime with no ports at all -- shows
 * "CPU", never another side's seat. When a real `sides` array names an actual seat for one of the two cards, that
 * card keeps its historical seat-number position (a seat's card never jumps left/right mid-match); a side left
 * without a claimed seat fills whatever position remains. With no `sides` array at all, cards fall back to the
 * snapshot's seats by raw position, exactly as before ports existed.
 */
export function header(snap, game) {
  const match = snap.match ?? {};
  const seats = Array.isArray(snap.seats) ? snap.seats : [];
  const plays = playsOf(game);
  const sides = Array.isArray(game?.sides) ? game.sides : null;
  const hasSides = Boolean(sides && sides.length);
  const sideOf = sideOfSeats(sides); // [side owning seat 0, side owning seat 1], each 0|1|null
  const anySeatKnown = hasSides && sideOf.some((s) => s === 0 || s === 1);
  const sideScore = Array.isArray(game?.score) ? [num(game.score[0]), num(game.score[1])] : null;
  const hasAny = plays.length > 0 || Boolean(sideScore && (sideScore[0] || sideScore[1]));

  // cardSide[i]: which SIDE the card at position i shows. seatOfCard[i]: which seats[] index sits under it (or
  // null for CPU). With no sides array, both default to the legacy raw-position behaviour (team always unknown).
  let cardSide = [null, null];
  let seatOfCard = [0, 1];
  if (hasSides) {
    const claimed = new Set(sideOf.filter((s) => s === 0 || s === 1));
    const leftover = [0, 1].filter((s) => !claimed.has(s));
    let li = 0;
    cardSide = [0, 1].map((i) => (sideOf[i] === 0 || sideOf[i] === 1) ? sideOf[i] : leftover[li++]);
    seatOfCard = [0, 1].map((i) => (sideOf[i] === 0 || sideOf[i] === 1) ? i : null);
  }
  // Unknown sides everywhere: no seat is credited with anything; the side-labelled line below carries the score.
  const pointsOf = (i) => (anySeatKnown && sideScore ? sideScore[cardSide[i]] : null);

  const root = el('div', { class: 'lv-header' });
  root.append(el('div', { class: 'lv-scoreboard' },
    seatCard(0, seats[seatOfCard[0]], teamOf(game, cardSide[0]), pointsOf(0), hasSides && seatOfCard[0] === null),
    el('span', { class: 'lv-vs', 'aria-hidden': 'true', text: anySeatKnown ? SEP : 'vs' }),
    seatCard(1, seats[seatOfCard[1]], teamOf(game, cardSide[1]), pointsOf(1), hasSides && seatOfCard[1] === null)));
  if (!anySeatKnown && sideScore && hasAny) {
    const label = (side) => teamOf(game, side)?.name ?? `Side ${side + 1}`;
    root.append(el('p', { class: 'lv-side-score', text: `${label(0)} ${sideScore[0]}${SEP}${sideScore[1]} ${label(1)}` }));
  }

  const st = gameStatus(snap, game);
  const bestOf = Number.isInteger(match.best_of) ? match.best_of : 1;
  const meta = el('div', { class: 'lv-meta' },
    el('span', { class: `lv-status ${st.live ? 'live' : 'done'}`, text: st.word }),
    el('span', { class: 'lv-game-no', text: `Game ${game?.game_seq ?? 1} of ${bestOf}` }));
  if (bestOf > 1) {
    const series = Array.isArray(match.series) ? match.series : [0, 0];
    meta.append(el('span', { class: 'lv-series', text: `Series ${num(series[0])}${SEP}${num(series[1])}` }));
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
export function portraitsFor(snap, game) {
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
export function starOf(s) {
  for (const d of [s.receiver, s.rusher, s.passer]) if (Number.isInteger(d) && d >= 0) return d;
  return null;
}

export function playsTab(snap, game) {
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
    if (after) top.append(el('span', { class: 'lv-play-after', text: `${after[0]}${SEP}${after[1]}` }));
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
  // A rusher shows up for carries OR broken tackles OR yards after (first) contact: a change-of-pace back can log
  // BTK/YCO on a stuffed carry, or (in principle) draw the stat off a carry credited to someone else. AVG is the
  // backend's own rush_ypc, already null with 0 carries, so it reads as a dash rather than a divide-by-zero.
  { title: 'Rushing', has: (s) => num(s.rush_att) > 0 || num(s.broken_tackles) > 0 || num(s.yds_after_contact) > 0, sort: (s) => num(s.rush_yds),
    cols: [['ATT', (s) => num(s.rush_att)], ['YDS', (s) => num(s.rush_yds)], ['AVG', (s) => (Number.isFinite(s.rush_ypc) ? s.rush_ypc : DASH)],
      ['TD', (s) => num(s.rush_td)], ['LONG', (s) => (Number.isFinite(s.rush_long) ? s.rush_long : DASH)],
      ['BTK', (s) => num(s.broken_tackles)], ['YCO', (s) => num(s.yds_after_contact)]] },
  // A receiver shows up for receptions OR drops: a drop with no catch still belongs on the table.
  { title: 'Receiving', has: (s) => num(s.rec) > 0 || num(s.drops) > 0, sort: (s) => num(s.rec_yds),
    cols: [['REC', (s) => num(s.rec)], ['YDS', (s) => num(s.rec_yds)], ['TD', (s) => num(s.rec_td)],
      ['LONG', (s) => (Number.isFinite(s.rec_long) ? s.rec_long : DASH)], ['DROP', (s) => num(s.drops)]] },
  { title: 'Defense', has: (s) => num(s.tackles) + num(s.sacks) + num(s.def_int) + num(s.ff) + num(s.fr) > 0,
    sort: (s) => num(s.tackles) + num(s.sacks) * 2 + num(s.def_int) * 3,
    cols: [['TKL', (s) => num(s.tackles)], ['SCK', (s) => num(s.sacks)], ['INT', (s) => num(s.def_int)], ['FF', (s) => num(s.ff)],
      ['PBU', (s) => num(s.pass_breakups)]] },
];

export function boxTab(snap, game) {
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
    const table = el('table', { class: `lv-table ${t.title.toLowerCase()}` }, el('caption', { text: t.title }), el('thead', {}, head), body);
    root.append(el('div', { class: 'lv-table-scroll' }, table));
  }
  if (!any) root.append(el('p', { class: 'lv-empty', text: 'No stats yet.' }));
  return root;
}

const RESULT_WORD = { touchdown: 'Touchdown', turnover: 'Turnover', downs: 'Downs', end: 'End of game' };

export function drivesTab(snap, game) {
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

export const TABS = [['plays', 'Plays', playsTab], ['box', 'Box score', boxTab], ['drives', 'Drives', drivesTab]];

/** The "couldn't be found" line and a way back (`href`, `label`), as nodes for the page to place. */
export function notFound(href = 'online.html', label = 'Back to Online') {
  return [
    el('p', { class: 'lv-empty', role: 'alert', text: NOT_FOUND }),
    el('a', { class: 'button', id: 'back', 'data-key': 'back', href, text: label }),
  ];
}
