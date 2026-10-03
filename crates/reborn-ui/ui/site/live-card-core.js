// Vendored from Reborn Backend src/web/live-card-core.js @e7ca37c, do not edit: run Tools/sync-site-render.mjs.
// The launcher game page (game.js) draws with it exactly as the site does.
/**
 * The live card's pure core: everything the Discord card and the web live card show, as strings and numbers.
 * No imports, no I/O, no clocks, no DOM. The same file runs in Node (the bot, both repos' tests) and in the
 * browser (the web live page). THIS FILE IS THE SOURCE: the backend serves a byte-identical copy as
 * src/web/live-card-core.js (plan ruling R19). Never edit only one copy.
 *
 * CardInput (built by src/card/input.js in the bot and cardInput() in the backend's live-render.js):
 *   { match: { id, mode, best_of, status, series, series_live?, outcome?, rules? }, seats: [{ display_name }],
 *     games: [{ game_seq, sides, teams, score, box: { lines }, seatOfSide: [slot, slot],
 *               plays: [{ seq, status, body, situation, field, score_after }] }] }
 * games ascending by game_seq, plays ascending by seq. seatOfSide[side] is the seat SLOT (0 or 1) holding that side;
 * a CPU or unknown side takes the slot nobody else holds (fillSeats), so every per-seat sum has two slots.
 */

/** [seatOfSide0, seatOfSide1] with a null filled by the other slot; two nulls are by position. */
export function fillSeats(bySide) {
  const s = Array.isArray(bySide) ? [bySide[0], bySide[1]].map((v) => (v === 0 || v === 1 ? v : null)) : [null, null];
  if (s[0] === null && s[1] === null) return [0, 1];
  if (s[0] === null) s[0] = 1 - s[1];
  if (s[1] === null) s[1] = 1 - s[0];
  if (s[0] === s[1]) return [0, 1];
  return s;
}

// ---- text -------------------------------------------------------------------------------------------------------

const DASHES = /[‐-―−]/g;

/** One line, every dash-like character a plain hyphen (spec: hyphens only, never en or em dashes). */
export function clean(s) {
  return String(s ?? '').replace(DASHES, '-').replace(/[\s\u0000-\u001f\u007f]+/g, ' ').trim();
}

/** clean(s) cut to at most `max` characters, ending with "…" when anything was dropped. */
export function cut(s, max) {
  const t = clean(s);
  return t.length <= max ? t : `${t.slice(0, Math.max(0, max - 1)).trimEnd()}…`;
}

const SUFFIX = /^(jr|sr|ii|iii|iv|v)\.?$/i;

/** "Michael Penix Jr." -> "Penix": the last word that is not a suffix, at most 14 characters. */
export function lastName(name) {
  const parts = clean(name).split(' ').filter(Boolean);
  while (parts.length > 1 && SUFFIX.test(parts[parts.length - 1])) parts.pop();
  return cut(parts[parts.length - 1] ?? '', 14);
}

const int = (v) => { const n = Number(v); return Number.isFinite(n) ? Math.round(n) : 0; };
const bit = (v) => v === 0 || v === 1;
const r2 = (v) => Math.round(v * 100) / 100;
const r4 = (v) => Math.round(v * 10000) / 10000;

// ---- colour -----------------------------------------------------------------------------------------------------

const HEX = /^#[0-9a-f]{6}$/i;
export const NEUTRAL = '#3a3c42';
/** Colours that never follow a team (approved T2 mockup). */
export const FIXED = Object.freeze({ live: '#ff453a', result: '#7dffb0', firstDown: '#ffd60a', white: '#ffffff' });

function rgbOf(hex) { return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)); }
function hexOf(c) { return `#${c.map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0')).join('')}`; }
/** Every channel times k. */
export function scale(hex, k) { return hexOf(rgbOf(hex).map((v) => v * k)); }
/** `t` of the way from `hex` to `hex2`. */
export function mix(hex, hex2, t) { const b = rgbOf(hex2); return hexOf(rgbOf(hex).map((v, i) => v + (b[i] - v) * t)); }

function toHsl(hex) {
  const [r, g, b] = rgbOf(hex).map((v) => v / 255);
  const mx = Math.max(r, g, b), mn = Math.min(r, g, b), l = (mx + mn) / 2, d = mx - mn;
  if (d === 0) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  const h = mx === r ? ((g - b) / d) % 6 : mx === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [(h * 60 + 360) % 360, s, l];
}
function fromHsl([h, s, l]) {
  const c = (1 - Math.abs(2 * l - 1)) * s, x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = l - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return hexOf([(r + m) * 255, (g + m) * 255, (b + m) * 255]);
}
const usable = (hex) => HEX.test(hex ?? '') && toHsl(hex)[1] >= 0.25 && toHsl(hex)[2] >= 0.08;
const distance = (a, b) => { const x = rgbOf(a), y = rgbOf(b); return Math.hypot(x[0] - y[0], x[1] - y[1], x[2] - y[2]); };

/** A team's card colour (plan ruling R5): a usable primary, else a usable secondary, else the primary; lightness in [0.16, 0.42]. */
export function teamColour(team) {
  const p = team?.primary, s = team?.secondary;
  const base = usable(p) ? p : usable(s) ? s : HEX.test(p ?? '') ? p : NEUTRAL;
  const [h, sat, l] = toHsl(base);
  return fromHsl([h, sat, Math.min(0.42, Math.max(0.16, l))]).toLowerCase();
}

/** [colourA, colourB]; team B takes its secondary when the two would look the same and that helps. */
export function sideColours(teamA, teamB) {
  const a = teamColour(teamA);
  let b = teamColour(teamB);
  if (distance(a, b) < 60 && usable(teamB?.secondary)) {
    const alt = teamColour({ primary: teamB.secondary });
    if (distance(a, alt) >= 60) b = alt;
  }
  return [a, b];
}

/** Every colour the card derives from the colour of the team that has the ball (plan ruling R6). */
export function palette(c) {
  const [h, s, l] = toHsl(c);
  return {
    team: c,
    bg: [scale(c, 0.45), scale(c, 0.26), scale(c, 0.115)],
    finalBg: [scale(c, 0.35), scale(c, 0.2), scale(c, 0.1)],
    face: [scale(c, 1.18), scale(c, 0.68)],
    badge: scale(c, 0.26),
    soft: mix(c, '#f0f0f0', 0.7),
    muted: mix(c, '#c8c8c8', 0.6),
    label: mix(c, '#f0f0f0', 0.58),
    line: mix(c, '#f0f0f0', 0.82),
    bar: fromHsl([h, Math.min(s, 0.75), Math.min(0.6, Math.max(0.3, l + 0.18))]),
  };
}

/** The score band's four stops, A on the left (spec: team A's primary to team B's). */
export function bandStops(a, b) { return [a, scale(a, 0.75), scale(b, 0.78), b]; }

/** The team on `side` of a game's teams array (by its `side` field, else by position). */
export function teamFor(teams, side) {
  const list = Array.isArray(teams) ? teams : [];
  return list.find((t) => t?.side === side) ?? list[side] ?? null;
}

function abbrevOf(team, fallback) {
  const a = clean(team?.abbrev).toUpperCase();
  return /^[A-Z0-9]{2,4}$/.test(a) ? a : fallback;
}

// ---- plays ------------------------------------------------------------------------------------------------------

const summaryOf = (p) => p?.body?.summary ?? {};
const isConversion = (body) => body?.summary?.kind === 'conversion' || body?.conversion === true;
const isDigp = (d) => Number.isInteger(d) && d >= 0;

/** The backend boxscore's raw field map (src/pbp/boxscore.js STAT_FIELDS), the part the card shows. */
const STAT_FIELDS = Object.freeze({
  taag: 'pass_att', mcag: 'pass_cmp', ayag: 'pass_yds', dtag: 'pass_td', niag: 'pass_int',
  taug: 'rush_att', ayug: 'rush_yds', dtug: 'rush_td', accg: 'rec', aycg: 'rec_yds', dtcg: 'rec_td', cycg: 'rec_yac',
  atdg: 'tackles', ltdg: 'tfl', kslg: 'sacks', nisg: 'def_int', hyug: 'yds_after_contact',
});

function applyChanges(map, play) {
  if (play.status === 'disputed' || isConversion(play.body)) return;
  for (const c of Array.isArray(play.body?.stat_changes) ? play.body.stat_changes : []) {
    const key = STAT_FIELDS[c?.field];
    if (!key || c.team || !c.before_known || !Number.isInteger(c.who)) continue;
    const d = Number(c.after) - Number(c.before);
    if (!Number.isFinite(d)) continue;
    if (!map.has(c.who)) map.set(c.who, {});
    const s = map.get(c.who);
    s[key] = (s[key] ?? 0) + d;
  }
}

/** Each player's game stats through play `seq` (plan ruling R20), DIGP -> { pass_att, ... }. */
export function statsThrough(game, seq) {
  const out = new Map();
  for (const p of game?.plays ?? []) { if (p.seq > seq) break; applyChanges(out, p); }
  return out;
}

/** One player's change in one raw field on one play. */
function delta(play, who, field) {
  let d = 0;
  for (const c of Array.isArray(play.body?.stat_changes) ? play.body.stat_changes : []) {
    if (c?.who === who && c.field === field && !c.team && c.before_known) d += Number(c.after) - Number(c.before) || 0;
  }
  return Math.round(d);
}

/** DIGP -> side from who did what on the plays (offense roles -> offense side; defense roles -> the other). */
export function digpSides(game) {
  const m = new Map();
  for (const p of game?.plays ?? []) {
    const off = p.body?.offense_side;
    if (p.status === 'disputed' || !bit(off)) continue;
    const s = summaryOf(p);
    for (const d of [s.passer, s.rusher, s.receiver]) if (isDigp(d) && !m.has(d)) m.set(d, off);
    const defenders = [s.interceptor, ...(s.turnover === true ? [] : Array.isArray(s.tacklers) ? s.tacklers : [])];
    for (const d of defenders) if (isDigp(d) && !m.has(d)) m.set(d, 1 - off);
  }
  return m;
}

/** A box line's side: its own `side`, else what the plays say, else its team_index (capture order: a last resort). */
function sideOfLine(line, bySide) {
  if (bit(line?.side)) return line.side;
  if (bySide.has(line?.digp)) return bySide.get(line.digp);
  return bit(line?.team_index) ? line.team_index : null;
}

function playersOf(game) {
  const bySide = digpSides(game);
  const m = new Map();
  for (const l of game?.box?.lines ?? []) {
    if (!l || !Number.isInteger(l.digp)) continue;
    m.set(l.digp, { name: clean(l.name), side: sideOfLine(l, bySide), portrait: typeof l.portrait_url === 'string' ? l.portrait_url : null });
  }
  return m;
}

/** Who the play is about, and in what role. */
function mainPlayer(s) {
  const pick = (d, role) => (isDigp(d) ? { digp: d, role } : null);
  switch (s?.kind) {
    case 'pass': return pick(s.receiver, 'receiver') ?? pick(s.passer, 'passer');
    case 'run': return pick(s.rusher, 'rusher');
    case 'interception': return pick(s.interceptor, 'defender');
    case 'sack': return pick(Array.isArray(s.tacklers) ? s.tacklers[0] : null, 'defender') ?? pick(s.passer, 'passer');
    case 'conversion': return pick(s.receiver, 'receiver') ?? pick(s.rusher, 'rusher') ?? pick(s.passer, 'passer');
    default: return null;
  }
}

const capital = (w) => w.charAt(0).toUpperCase() + w.slice(1);
/** "1 yd", "2 yds", "-1 yds". */
const yds = (n) => (n === 1 ? 'yd' : 'yds');

/** The big line: "34-yard catch", "6-yard run", "3-yard loss", "Interception", "Sacked". Hyphens only. */
export function headline(s) {
  const y = s?.yards_known === true && Number.isFinite(s?.yards) ? Math.round(s.yards) : null;
  const gain = (noun) => (y == null ? capital(noun) : y > 0 ? `${y}-yard ${noun}` : y === 0 ? `${capital(noun)}, no gain` : `${-y}-yard loss`);
  switch (s?.kind) {
    case 'pass': return isDigp(s.receiver) ? gain('catch') : s.turnover === true ? 'Pass, turnover' : 'Incomplete pass';
    case 'run': return gain('run');
    case 'sack': return y != null && y < 0 ? `Sacked, ${-y}-yard loss` : 'Sacked';
    case 'interception': return 'Interception';
    case 'conversion': return 'Conversion try';
    case 'fg': return 'Field goal try';
    default: return 'Play';
  }
}

/** The small spaced capitals over the headline, or null when the play has no result. */
export function resultOf(s, status) {
  if (status === 'disputed') return { word: 'DISPUTED', tone: 'muted' };
  if (s?.kind === 'conversion') return s.conversion_good === true ? { word: 'GOOD', tone: 'good' } : { word: 'NO GOOD', tone: 'muted' };
  if (s?.touchdown === true) return { word: 'TOUCHDOWN', tone: 'good' };
  if (s?.turnover === true) return { word: 'TURNOVER', tone: 'good' };
  if (s?.kind === 'fg') return Number(s.points) > 0 ? { word: 'GOOD', tone: 'good' } : { word: 'NO GOOD', tone: 'muted' };
  if (s?.first_down === true) return { word: '1ST DOWN', tone: 'good' };
  return null;
}

/** "5 rec · 96 yds · 1 TD" as [value, word] pairs; the renderer draws values white and words grey. */
function statSegments(role, st = {}) {
  const v = (k) => int(st[k]);
  if (role === 'receiver') return [[v('rec'), 'rec'], [v('rec_yds'), yds(v('rec_yds'))], ...(v('rec_td') > 0 ? [[v('rec_td'), 'TD']] : [])];
  if (role === 'rusher') return [[v('rush_att'), 'car'], [v('rush_yds'), yds(v('rush_yds'))], ...(v('rush_td') > 0 ? [[v('rush_td'), 'TD']] : [])];
  if (role === 'passer') {
    return [[`${v('pass_cmp')}/${v('pass_att')}`, ''], [v('pass_yds'), yds(v('pass_yds'))], ...(v('pass_td') > 0 ? [[v('pass_td'), 'TD']] : []),
      ...(v('pass_int') > 0 ? [[v('pass_int'), 'INT']] : [])];
  }
  const out = [[v('tackles'), 'tkl']];
  if (v('sacks') > 0) out.push([v('sacks'), v('sacks') === 1 ? 'sack' : 'sacks']);
  if (v('def_int') > 0) out.push([v('def_int'), 'INT']);
  return out;
}

/** The grey line: one detail, then the passer's game line ("18 yds after the catch · **Penix** 14/20, 212 yds"). */
function detailOf(s, play, role, players, stats) {
  const parts = [];
  if (role === 'receiver') { const yac = delta(play, s.receiver, 'cycg'); if (yac > 0) parts.push({ text: `${yac} ${yds(yac)} after the catch` }); }
  if (role === 'rusher') { const yco = delta(play, s.rusher, 'hyug'); if (yco > 0) parts.push({ text: `${yco} ${yds(yco)} after contact` }); }
  const passer = role !== 'passer' && isDigp(s.passer) ? s.passer : null;
  if (passer != null && players.get(passer)?.name) {
    const st = stats.get(passer) ?? {};
    if (parts.length) parts.push({ text: ' · ' });
    parts.push({ text: lastName(players.get(passer).name), bold: true }, { text: ` ${int(st.pass_cmp)}/${int(st.pass_att)}, ${int(st.pass_yds)} yds` });
  }
  return parts.length ? parts : null;
}

/** Band 2 (and one web feed row) for one play. `stats` defaults to statsThrough(game, play.seq). */
export function playRow(game, play, colours, stats = null) {
  const s = summaryOf(play);
  const side = bit(play.body?.offense_side) ? play.body.offense_side : null;
  const res = resultOf(s, play.status);
  const row = {
    seq: play.seq, side, disputed: play.status === 'disputed', result: res?.word ?? null, tone: res?.tone ?? null,
    headline: headline(s), text: cut(s.text ?? '', 160), player: null, stats: null, detail: null,
  };
  const main = mainPlayer(s);
  const players = playersOf(game);
  const who = main ? players.get(main.digp) : null;
  if (!who?.name) return row;
  const st = stats ?? statsThrough(game, play.seq);
  const pSide = bit(who.side) ? who.side : side == null ? null : main.role === 'defender' ? 1 - side : side;
  row.player = { name: cut(who.name, 30), short: lastName(who.name), side: pSide, portrait: who.portrait, colour: bit(pSide) ? colours[pSide] : null };
  row.stats = statSegments(main.role, st.get(main.digp));
  row.detail = detailOf(s, play, main.role, players, st);
  return row;
}

/** Every play's row, newest first, in one pass over the stat changes (the web feed). */
export function playRows(game, colours) {
  const running = new Map();
  const rows = [];
  for (const p of game?.plays ?? []) {
    applyChanges(running, p);
    rows.push(playRow(game, p, colours, running));
  }
  return rows.reverse();
}

// ---- situation, drive, ruler ------------------------------------------------------------------------------------

const DOWN_WORD = Object.freeze({ 1: '1st', 2: '2nd', 3: '3rd', 4: '4th' });
const PREDICTABLE = new Set(['pass', 'run', 'sack']);
const fieldOk = (f) => Boolean(f) && Number.isFinite(f.field_yds) && f.field_yds > 0 && Number.isFinite(f.own_yd);

/**
 * Where things stand AFTER `play` (plan ruling R3). The next non-disputed, non-conversion play's own snap is exact.
 * Otherwise the newest play is predicted from its summary: pass, run or sack with known yards; a touchdown puts the
 * ball in the end zone; anything else is the play's own snap ("before"), never an invented spot. Null when the play
 * has no usable field (older data): the card then shows the text situation and no ruler.
 */
export function afterState(game, play) {
  const body = play?.body ?? {};
  const s = body.summary ?? {};
  const next = (game?.plays ?? []).find((p) => p.seq > play.seq && p.status !== 'disputed') ?? null;
  if (next && !isConversion(next.body) && fieldOk(next.field) && bit(next.body?.offense_side)) {
    const nf = next.field;
    return { source: 'next', offense: next.body.offense_side, down: nf.down ?? null, to_go: nf.to_go ?? null,
      own_yd: nf.own_yd, field_yds: nf.field_yds, label: null };
  }
  const f = play?.field;
  const off = body.offense_side;
  if (!fieldOk(f) || !bit(off)) return null;
  const L = f.field_yds;
  const before = { source: 'before', offense: off, down: f.down ?? null, to_go: f.to_go ?? null, own_yd: f.own_yd, field_yds: L, label: null };
  if (isConversion(body)) return { ...before, down: null, to_go: null, label: 'Conversion try' };
  if (s.touchdown === true) {
    return { source: 'predicted', offense: s.turnover === true ? 1 - off : off, down: null, to_go: null, own_yd: L, field_yds: L, label: 'Touchdown' };
  }
  if (s.turnover === true || !PREDICTABLE.has(s.kind)) return before;
  const incomplete = s.kind === 'pass' && !isDigp(s.receiver);
  const y = incomplete ? 0 : s.yards_known === true && Number.isFinite(s.yards) ? s.yards : null;
  if (y == null) return before;
  const ball = r2(Math.min(L, Math.max(0, f.own_yd + y)));
  const at = (down, toGo) => ({ source: 'predicted', offense: off, down, to_go: toGo, own_yd: ball, field_yds: L, label: null });
  if (s.first_down === true) {
    const step = Number(body.line_step) > 0 ? Number(body.line_step) : 20;
    const line = step * (Math.floor((ball + 1e-6) / step) + 1);
    return at(1, line >= L - 1e-6 ? 'goal' : Math.max(1, Math.round(line - ball)));
  }
  const down = Number.isInteger(f.down) ? f.down + 1 : null;
  if (down == null || down > 4) return before;
  if (f.to_go === 'goal') return at(down, 'goal');
  if (!Number.isFinite(f.to_go)) return at(down, null);
  const left = f.own_yd + f.to_go - ball;
  return left > 0 ? at(down, Math.max(1, Math.round(left))) : before;
}

/** The drive the after-state belongs to: plays so far, start (own yards) and yards gained; null after a change of possession. */
export function driveOf(game, play, after) {
  const off = play?.body?.offense_side;
  if (!after || !bit(off) || after.offense !== off || isConversion(play.body)) return null;
  const plays = (game?.plays ?? []).filter((p) => p.seq <= play.seq && p.status !== 'disputed');
  let first = null;
  let count = 0;
  for (let i = plays.length - 1; i >= 0; i -= 1) {
    const p = plays[i];
    if (isConversion(p.body) || p.body?.offense_side !== off) break;
    first = p;
    count += 1;
  }
  if (!first || !fieldOk(first.field)) return null;
  if (after.source === 'before') count -= 1; // the ball has not moved past the shown play's snap
  if (count <= 0) return null;
  return { plays: count, start: first.field.own_yd, yards: Math.round(after.own_yd - first.field.own_yd) };
}

/** Design px of the ruler track (640 - 2 x 30) and a safe average glyph width for 12 px Inter 600-700. */
export const TRACK_W = 580;
export const CHAR_W = 7.5;
const GAP = 8;

/** Labels in priority order; one that would overlap an earlier one is dropped (spec: nothing may overlap). */
export function placeLabels(cands) {
  const placed = [];
  for (const c of cands) {
    const w = c.text.length * CHAR_W;
    const at = c.x * TRACK_W;
    let lo = c.align === 'left' ? at : c.align === 'right' ? at - w : at - w / 2;
    lo = Math.max(0, Math.min(TRACK_W - w, lo));
    const hi = lo + w;
    if (placed.some((p) => lo < p.hi + GAP && hi > p.lo - GAP)) continue;
    placed.push({ ...c, lo, hi });
  }
  return placed.map((p) => ({ text: p.text, x: p.x, align: p.align, strong: Boolean(p.strong), left: r4(p.lo / TRACK_W), width: r4((p.hi - p.lo) / TRACK_W) }));
}

/** "ATL 8", "PHI 34", or the midfield number. */
function spotLabel(yd, L, offAbbr, defAbbr) {
  const half = L / 2;
  if (yd < half - 0.5) return `${offAbbr} ${Math.round(yd)}`;
  if (yd > half + 0.5) return `${defAbbr} ${Math.round(L - yd)}`;
  return String(Math.round(half));
}

/** The yard ruler, offense always on the left (spec §1). Every position is a 0..1 fraction of the track. */
export function rulerOf(after, teams, drive, colours) {
  const L = after.field_yds;
  const off = after.offense;
  const def = 1 - off;
  const x = (yd) => r4(0.06 + (0.88 * Math.min(L, Math.max(0, yd))) / L);
  const half = L / 2;
  const ticks = [];
  for (let yd = 10; yd < L - 0.5; yd += 10) if (Math.abs(yd - half) > 1) ticks.push({ x: x(yd), mid: false });
  ticks.push({ x: x(half), mid: true });
  ticks.sort((a, b) => a.x - b.x);
  const offAbbr = abbrevOf(teams?.[off], 'OWN');
  const defAbbr = abbrevOf(teams?.[def], 'OPP');
  const ball = Number.isFinite(after.own_yd) ? x(after.own_yd) : null;
  const fdYd = Number.isFinite(after.to_go) && Number.isFinite(after.own_yd) ? after.own_yd + after.to_go : null;
  const firstDown = fdYd != null && fdYd < L - 0.5 ? x(fdYd) : null;
  const start = drive && Number.isFinite(drive.start) ? x(drive.start) : null;
  const fdPx = firstDown == null ? null : firstDown * TRACK_W;
  return {
    caps: [colours[off], colours[def]],
    ticks,
    ball,
    firstDown,
    firstDownAlign: fdPx == null ? null : fdPx < 30 ? 'left' : fdPx > TRACK_W - 30 ? 'right' : 'center',
    start,
    drive: start != null && ball != null && start !== ball ? { from: Math.min(start, ball), to: Math.max(start, ball) } : null,
    labels: placeLabels([
      ball != null && { text: spotLabel(after.own_yd, L, offAbbr, defAbbr), x: ball, align: 'center', strong: true },
      { text: offAbbr, x: 0, align: 'left' },
      { text: defAbbr, x: 1, align: 'right' },
      start != null && Math.abs(drive.start - after.own_yd) >= 1 && { text: spotLabel(drive.start, L, offAbbr, defAbbr), x: start, align: 'center' },
      { text: String(Math.round(half)), x: x(half), align: 'center' },
    ].filter(Boolean)),
  };
}

/** Band 3's text line: "1st & 10" + "at PHI 34", and "Drive · 4 plays · 58 yds" (null when there is no drive). */
export function situationLine(after, teams, drive, play) {
  if (!after) {
    const t = clean(play?.situation);
    return t ? { down: t, spot: '', drive: null } : null;
  }
  const w = DOWN_WORD[after.down];
  const down = after.label ?? (w ? (after.to_go === 'goal' ? `${w} & Goal` : Number.isFinite(after.to_go) ? `${w} & ${after.to_go}` : w) : '');
  const s = spotLabel(after.own_yd, after.field_yds, abbrevOf(teams?.[after.offense], 'OWN'), abbrevOf(teams?.[1 - after.offense], 'OPP'));
  const spot = after.label === 'Touchdown' ? '' : /^\d+$/.test(s) ? 'at midfield' : `at ${s}`;
  return { down, spot, drive: drive ? { plays: drive.plays, yards: drive.yards } : null };
}

// ---- the live card ----------------------------------------------------------------------------------------------

/** The 192 px portrait for a 96 px catalog path (the catalog ships both sizes); anything else unchanged. */
export function bigPortrait(path) {
  return typeof path === 'string' ? path.replace(/-96\.png$/, '-192.png') : path;
}

/** A game's score by side: its newest non-disputed play's score_after, else the game's own score. */
export function finalScoreOf(game) {
  const plays = game?.plays ?? [];
  for (let i = plays.length - 1; i >= 0; i -= 1) {
    if (plays[i].status !== 'disputed' && Array.isArray(plays[i].score_after)) return plays[i].score_after.map(int);
  }
  return Array.isArray(game?.score) ? game.score.map(int) : [0, 0];
}

/** The live score that decides a game (backend src/mm/series.js GAME_TARGET; results.js TARGET_SCORE). */
export const GAME_TARGET = 36;

/**
 * A game is decided (series spec §1): a side reached GAME_TARGET and the newest non-disputed play is not still
 * `live` (one seat's copy only). Computed here from the plays, never read from the snapshot's games[].decided, so a
 * stream frame that confirms the deciding play turns it at once. A tie decides nothing.
 */
export function gameDecided(game) {
  const plays = game?.plays ?? [];
  let newest = null;
  for (let i = plays.length - 1; i >= 0; i -= 1) if (plays[i].status !== 'disputed') { newest = plays[i]; break; }
  if (!newest || newest.status === 'live') return false;
  const sc = finalScoreOf(game);
  return Math.max(sc[0], sc[1]) >= GAME_TARGET && sc[0] !== sc[1];
}

/** The seat that won a decided game, or null. */
function winnerSeatOf(g) {
  if (!gameDecided(g)) return null;
  const sc = finalScoreOf(g);
  const seat = (g.seatOfSide ?? [0, 1])[sc[0] > sc[1] ? 0 : 1];
  return bit(seat) ? seat : null;
}

/** Wins by side of `game` from every decided game before it (and `game` itself when `inclusive`). */
function seriesBefore(input, game, inclusive = false) {
  const wins = [0, 0];
  for (const g of input?.games ?? []) {
    if (inclusive ? g.game_seq > game.game_seq : g.game_seq >= game.game_seq) continue;
    const seat = winnerSeatOf(g);
    if (seat !== null) wins[seat] += 1;
  }
  const sos = game.seatOfSide ?? [0, 1];
  return [0, 1].map((side) => (bit(sos[side]) ? wins[sos[side]] : 0));
}

/**
 * The series outcome (spec §1): the snapshot's `outcome` when it has one; otherwise (an older backend) the match
 * status says whether it is over and the wins say who took it. Wins prefer `series_live`, then a `series` that has
 * decided something, then the decided games here -- so an official [0,0] never hides a real 2-0 (644488b4).
 */
export function seriesOutcome(input) {
  const m = input?.match ?? {};
  const tally = [0, 0];
  for (const g of input?.games ?? []) { const s = winnerSeatOf(g); if (s !== null) tally[s] += 1; }
  const pair = (v) => (Array.isArray(v) && v.length === 2 ? v.map(int) : null);
  const official = pair(m.series);
  const wins = pair(m.series_live) ?? (official && official[0] + official[1] > 0 ? official : tally);
  const o = m.outcome;
  if (o && typeof o === 'object' && typeof o.kind === 'string') {
    return { kind: o.kind, winner: bit(o.winner_seat) ? o.winner_seat : null, official: o.official === true,
      unofficial: o.official !== true, leftSeat: bit(o.left_seat) ? o.left_seat : null, wins };
  }
  const lead = wins[0] > wins[1] ? 0 : wins[1] > wins[0] ? 1 : null;
  const base = { winner: null, official: false, unofficial: false, leftSeat: null, wins };
  if (m.status === 'held') return { ...base, kind: 'held' };
  if (m.status === 'void' || m.status === 'abandoned') return { ...base, kind: 'abandoned' };
  if (m.status === 'complete') return { ...base, kind: lead === null ? 'split' : 'win', winner: lead, official: Boolean(official && official[0] + official[1] > 0) };
  return { ...base, kind: 'open' };
}

/**
 * The Discord message / web page line for a finished series (spec §5), or null while it is open. `nameOf(seat)` is
 * the caller's (bold markdown in the bot, plain text on the page). R25: a casual series of one decided game shows
 * that game's score.
 */
export function seriesLine(input, nameOf) {
  const out = seriesOutcome(input);
  const mode = input?.match?.mode === 'ranked' ? 'Ranked' : 'Casual';
  let [a, b] = out.wins;
  const games = input?.games ?? [];
  if (input?.match?.mode !== 'ranked' && games.length === 1 && a + b === 1) {
    const sc = finalScoreOf(games[0]);
    const sos = games[0].seatOfSide ?? [0, 1];
    const bySeat = [0, 0];
    [0, 1].forEach((side) => { if (bit(sos[side])) bySeat[sos[side]] = sc[side] ?? 0; });
    [a, b] = bySeat;
  }
  const by = [a, b];
  switch (out.kind) {
    case 'win': { const w = out.winner ?? (a >= b ? 0 : 1); return `${nameOf(w)} beat ${nameOf(1 - w)} ${by[w]}-${by[1 - w]} · ${mode}`; }
    case 'forfeit': return `${nameOf(out.winner ?? 0)} wins by forfeit · ${mode}`;
    case 'split': return `${nameOf(0)} and ${nameOf(1)} split ${a}-${b} · ${mode}`;
    case 'held': return `${nameOf(0)} vs ${nameOf(1)} ${a}-${b} · ${mode} · Under review`;
    case 'abandoned': return `${nameOf(0)} vs ${nameOf(1)} · ${mode} · No result`;
    default: return null;
  }
}

/**
 * The series part of the live card's sub line, by THIS game's teams (Adam 10-02: "IND leads 1-0"): " · IND leads 1-0",
 * " · Series tied 1-1", " · IND wins 2-1" once the series is won, " · Best of 3" before ranked has a result, and
 * nothing for a casual game 1 with no result. `series` is wins by side of this game; `open` is false only when the
 * card shows the series' last game after the match closed.
 */
function seriesTally(input, teams, series, ranked, open) {
  const [a, b] = series;
  if (a + b === 0) {
    const bo = int(input?.match?.best_of);
    return ranked && bo > 1 ? ` · Best of ${bo}` : '';
  }
  if (a === b) return ` · Series tied ${a}-${b}`;
  const lead = a > b ? 0 : 1;
  const won = !open && ['win', 'forfeit'].includes(seriesOutcome(input).kind);
  return ` · ${abbrevOf(teams[lead], lead ? 'B' : 'A')} ${won ? 'wins' : 'leads'} ${series[lead]}-${series[1 - lead]}`;
}

/** Everything the live card shows for (gameSeq, seq), or null when there is no such play. Strings and numbers only. */
export function liveCard(input, gameSeq, seq) {
  const games = input?.games ?? [];
  const game = games.find((g) => g.game_seq === gameSeq);
  const play = game?.plays?.find((p) => p.seq === seq);
  if (!game || !play) return null;
  const teams = [teamFor(game.teams, 0), teamFor(game.teams, 1)];
  const colours = sideColours(teams[0], teams[1]);
  const after = afterState(game, play);
  const holder = bit(after?.offense) ? after.offense : bit(play.body?.offense_side) ? play.body.offense_side : 0;
  const score = Array.isArray(play.score_after) ? play.score_after.map(int) : finalScoreOf(game);
  const newest = Math.max(...games.map((g) => g.game_seq));
  const out = seriesOutcome(input);
  const running = input?.match?.status === 'running' && game.game_seq === newest;
  const between = running && out.kind === 'open' && gameDecided(game);
  const series = seriesBefore(input, game, !running || between);
  const ranked = input?.match?.mode === 'ranked';
  const sub = `Game ${game.game_seq}${seriesTally(input, teams, series, ranked, running || between || game.game_seq < newest)}`;
  const box = (side) => ({
    abbrev: abbrevOf(teams[side], side ? 'B' : 'A'), name: cut(teams[side]?.name ?? '', 18), logo: teams[side]?.logo_url ?? null,
    colour: colours[side], value: score[side] ?? 0, trailing: (score[side] ?? 0) < (score[1 - side] ?? 0),
  });
  const row = playRow(game, play, colours);
  const drive = after ? driveOf(game, play, after) : null;
  return {
    kind: 'live',
    game: game.game_seq,
    seq: play.seq,
    holder,
    palette: palette(colours[holder]),
    band: bandStops(colours[0], colours[1]),
    score: {
      a: box(0), b: box(1),
      status: between ? 'Between games' : running ? 'LIVE' : 'FINAL',
      sub,
      unofficial: Boolean(input?.match?.outcome) && out.unofficial,
    },
    play: { ...row, logo: teams[bit(row.player?.side) ? row.player.side : holder]?.logo_url ?? null },
    situation: situationLine(after, teams, drive, play),
    ruler: after ? rulerOf(after, teams, drive, colours) : null,
  };
}

// ---- the final card ---------------------------------------------------------------------------------------------

const STATUS_LABEL = Object.freeze({ complete: 'FINAL', held: 'UNDER REVIEW', abandoned: 'ABANDONED', void: 'VOID' });
const KIND_LABEL = Object.freeze({ win: 'FINAL', forfeit: 'FINAL', split: 'FINAL', held: 'UNDER REVIEW' });
const SUMS = ['pass_att', 'pass_cmp', 'pass_yds', 'pass_td', 'pass_int', 'rush_att', 'rush_yds', 'rush_td',
  'rec', 'rec_yds', 'rec_td', 'tackles', 'sacks', 'tfl', 'def_int', 'fr'];
const MAX_GAME_COLUMNS = 5;

/** "SEP 30" in America/New_York, or null. */
export function dateLabel(date) {
  if (!(date instanceof Date) || Number.isNaN(date.getTime())) return null;
  return new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', timeZone: 'America/New_York' }).format(date).toUpperCase();
}

/** Every box line of `seat` over the whole series, summed per player (pid, else name). */
function seriesLines(results, seat) {
  const by = new Map();
  for (const r of results) {
    const side = r.sos.indexOf(seat);
    if (side === -1) continue;
    const bySide = digpSides(r.g);
    for (const l of r.g.box?.lines ?? []) {
      if (!l || sideOfLine(l, bySide) !== side) continue;
      const name = clean(l.name);
      if (!name) continue;
      const key = Number.isInteger(l.pid) ? `p${l.pid}` : `n${name.toLowerCase()}`;
      if (!by.has(key)) by.set(key, { name, portrait: null, st: Object.fromEntries(SUMS.map((k) => [k, 0])) });
      const e = by.get(key);
      if (!e.portrait && typeof l.portrait_url === 'string') e.portrait = l.portrait_url;
      for (const k of SUMS) e.st[k] += int(l.stats?.[k]);
    }
  }
  return [...by.values()];
}

function best(list, has, key) {
  return list.filter((e) => has(e.st)).sort((a, b) => key(b.st) - key(a.st) || a.name.localeCompare(b.name))[0] ?? null;
}

/** Top passer, rusher, receiver and defender (by tackles) of `seat` over the series; a category nobody has is left out. */
function leadersOf(results, seat) {
  const lines = seriesLines(results, seat);
  const out = [];
  const add = (e, line) => { if (e) out.push({ name: lastName(e.name), portrait: e.portrait, line }); };
  const p = best(lines, (s) => s.pass_att > 0, (s) => s.pass_yds);
  add(p, p && `${p.st.pass_cmp}/${p.st.pass_att}, ${p.st.pass_yds} yds, ${p.st.pass_td} TD, ${p.st.pass_int} INT`);
  const r = best(lines, (s) => s.rush_att > 0, (s) => s.rush_yds);
  add(r, r && `${r.st.rush_att} car, ${r.st.rush_yds} yds, ${r.st.rush_td} TD`);
  const c = best(lines, (s) => s.rec > 0, (s) => s.rec_yds);
  add(c, c && `${c.st.rec} rec, ${c.st.rec_yds} yds, ${c.st.rec_td} TD`);
  const d = best(lines, (s) => s.tackles + s.sacks + s.def_int > 0, (s) => s.tackles * 1e6 + s.sacks * 1e3 + s.def_int);
  if (d) {
    const parts = [`${d.st.tackles} tkl`];
    if (d.st.sacks > 0) parts.push(`${d.st.sacks} ${d.st.sacks === 1 ? 'sack' : 'sacks'}`);
    if (d.st.tfl > 0) parts.push(`${d.st.tfl} TFL`);
    if (d.st.def_int > 0) parts.push(`${d.st.def_int} INT`);
    add(d, parts.join(', '));
  }
  return out;
}

/** Team bars by seat slot: yards from the box lines; turnovers from the plays, else thrown INTs + the other side's recoveries. */
function barsOf(results) {
  const t = [0, 1].map(() => ({ pass: 0, rush: 0, to: 0 }));
  for (const r of results) {
    const bySide = digpSides(r.g);
    const lines = r.g.box?.lines ?? [];
    for (const l of lines) {
      const side = sideOfLine(l, bySide);
      const seat = bit(side) ? r.sos[side] : null;
      if (!bit(seat)) continue;
      t[seat].pass += int(l.stats?.pass_yds);
      t[seat].rush += int(l.stats?.rush_yds);
    }
    const plays = (r.g.plays ?? []).filter((p) => p.status !== 'disputed' && !isConversion(p.body));
    if (plays.length) {
      for (const p of plays) {
        const off = p.body?.offense_side;
        if (summaryOf(p).turnover === true && bit(off) && bit(r.sos[off])) t[r.sos[off]].to += 1;
      }
    } else {
      for (const l of lines) {
        const side = sideOfLine(l, bySide);
        if (!bit(side)) continue;
        t[r.sos[side]].to += int(l.stats?.pass_int);
        t[r.sos[1 - side]].to += int(l.stats?.fr);
      }
    }
  }
  return [
    { label: 'Total Yards', a: t[0].pass + t[0].rush, b: t[1].pass + t[1].rush },
    { label: 'Passing', a: t[0].pass, b: t[1].pass },
    { label: 'Rushing', a: t[0].rush, b: t[1].rush },
    { label: 'Turnovers', a: t[0].to, b: t[1].to },
  ];
}

/** Everything the series final card shows (spec §2), rows by seat slot (plan rulings R8-R11). Null with no games. */
export function finalCard(input, { date = null } = {}) {
  const games = input?.games ?? [];
  if (!games.length) return null;
  const results = games.map((g) => {
    const sc = finalScoreOf(g);
    const sos = Array.isArray(g.seatOfSide) ? g.seatOfSide : [0, 1];
    const bySeat = [null, null];
    [0, 1].forEach((side) => { if (bit(sos[side])) bySeat[sos[side]] = sc[side] ?? 0; });
    return { g, sos, bySeat };
  });
  const out = seriesOutcome(input);
  const wins = out.wins;
  const winner = out.kind === 'win' || out.kind === 'forfeit'
    ? (out.winner ?? (wins[0] > wins[1] ? 0 : wins[1] > wins[0] ? 1 : null)) : null;
  const split = out.kind === 'split';
  const teamOfSeat = (seat) => {
    for (let i = results.length - 1; i >= 0; i -= 1) {
      const side = results[i].sos.indexOf(seat);
      if (side !== -1) return teamFor(results[i].g.teams, side);
    }
    return null;
  };
  const teams = [teamOfSeat(0), teamOfSeat(1)];
  const colours = sideColours(teams[0], teams[1]);
  const n = Math.max(int(input.match?.best_of) || 1, ...games.map((g) => int(g.game_seq)));
  const cols = Array.from({ length: n }, (_, i) => i + 1).slice(-MAX_GAME_COLUMNS);
  const seatName = (seat) => clean(input.seats?.[seat]?.display_name);
  const rows = [0, 1].map((seat) => ({
    seat,
    name: cut(teams[seat]?.name || seatName(seat) || `Seat ${seat + 1}`, 18),
    abbrev: abbrevOf(teams[seat], seat ? 'B' : 'A'),
    logo: teams[seat]?.logo_url ?? null,
    colour: colours[seat],
    winner: winner === seat,
    games: cols.map((gs) => {
      const r = results.find((x) => x.g.game_seq === gs);
      return r && r.bySeat[seat] != null ? String(r.bySeat[seat]) : '-';
    }),
    wins: split ? 'SPLIT' : wins[seat],
  }));
  const label = KIND_LABEL[out.kind] ?? STATUS_LABEL[input.match?.status] ?? 'FINAL';
  const leftName = bit(out.leftSeat) ? cut(seatName(out.leftSeat) || `Seat ${out.leftSeat + 1}`, 24) : null;
  // Held ("Under review") and abandoned (no result) carry no UNOFFICIAL tag: there is no result to be unofficial.
  const tagged = out.unofficial && out.kind !== 'held' && out.kind !== 'abandoned';
  return {
    kind: 'final',
    outcome: out.kind,
    winner,
    unofficial: tagged,
    note: out.kind === 'forfeit' && leftName ? `${leftName} left the series` : null,
    palette: palette(winner == null ? NEUTRAL : colours[winner]),
    header: { label: [label, tagged ? 'UNOFFICIAL' : null, dateLabel(date)].filter(Boolean).join(' · '),
      games: cols.map((gs) => `G${gs}`) },
    rows,
    leaders: winner == null ? [] : leadersOf(results, winner),
    bars: barsOf(results),
  };
}
