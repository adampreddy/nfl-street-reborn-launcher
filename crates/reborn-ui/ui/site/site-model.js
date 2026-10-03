// Vendored from Reborn Backend src/web/site-model.js @ca3fdf5, do not edit: run Tools/sync-site-render.mjs.
// The launcher match page (match.js, game-snapshot.js) draws with it exactly as the site does.
// Pure data for the restyled site pages (spec 2026-10-02 site restyle): who sits where, the strip's status and line
// score, the series pill's words, drives, box score columns and team stats. No DOM, no I/O, no clocks. Web-only: the
// shared card core (live-card-core.js, a byte copy of the Live bot's) is imported, never edited. Served byte-for-byte
// by webRouter.js, which versions its one import (keep it ONE import line from ./live-card-core.js).
// Every `input` is a CardInput (live-render.js cardInput): games ascending by game_seq, plays ascending by seq, and
// each game's seatOfSide filled, so every seat has a side. Everything is laid out BY SEAT (plan ruling R2).
import { teamFor, sideColours, finalScoreOf, seriesOutcome, gameDecided, clean, cut, teamColour } from "./live-card-core.js";

// The core's team colour for the profile strip (profile-page.js imports this module, never the core itself).
export { teamColour };

const num = (v) => (Number.isFinite(v) ? v : 0);
const ABBREV = /^[A-Z0-9]{2,4}$/;
const MAX_GAME_COLUMNS = 5;
const DASHES = /[\u2010-\u2015\u2212]/g;

/** Our own text, hyphens only (spec section 11): every dash-like character becomes "-". */
export const hy = (s) => String(s ?? "").replace(DASHES, "-");

/** The newest game (highest game_seq) of a CardInput, or null. */
export function newestOf(input) {
  let top = null;
  for (const g of input?.games ?? []) if (!top || g.game_seq > top.game_seq) top = g;
  return top;
}

/** The side `seat` plays in `game`. cardInput's seatOfSide always holds both slots, so this is always 0 or 1. */
export function sideOfSeat(game, seat) {
  const i = Array.isArray(game?.seatOfSide) ? game.seatOfSide.indexOf(seat) : -1;
  return i === -1 ? seat : i;
}

/** A game's score by seat, [seat 0, seat 1]: the core's final score (newest non-disputed play), moved off sides. */
export function scoreBySeat(game) {
  const sc = finalScoreOf(game);
  return [num(sc[sideOfSeat(game, 0)]), num(sc[sideOfSeat(game, 1)])];
}

/**
 * Each seat as the page shows it: its team in `game` (else the newest game), the player's name and a short label
 * (the team abbrev, else the name). A seat past the humans is the CPU; with no humans at all the player is "" and
 * the label falls back to the team name, else "Side N". Never undefined (Review Focus 1).
 */
export function seatTeams(input, game = null) {
  const g = game ?? newestOf(input);
  const seats = Array.isArray(input?.seats) ? input.seats : [];
  return [0, 1].map((seat) => {
    const side = g ? sideOfSeat(g, seat) : seat;
    const t = g ? teamFor(g.teams, side) : null;
    const team = t && typeof t.name === "string" && t.name ? t : null;
    const typed = clean(seats[seat]?.display_name);
    const player = typed || (!seats.length ? "" : seat >= seats.length ? "CPU" : `Seat ${seat + 1}`);
    const abbrev = clean(team?.abbrev).toUpperCase();
    const label = ABBREV.test(abbrev) ? abbrev : cut(player || team?.name || `Side ${side + 1}`, 14);
    return { seat, side, team, player, label };
  });
}

/** The two seats' colours: the core's card colours for their teams (team B swaps to its secondary when they clash). */
export const seatColours = (st) => sideColours(st[0].team, st[1].team);

/** `game` is the one being played right now: the newest game of a running match, not yet decided. */
export function gameIsLive(input, game) {
  const newest = newestOf(input);
  return input?.match?.status === "running" && Boolean(game && newest) && game.game_seq === newest.game_seq && !gameDecided(game);
}

/** The strip's status line: "LIVE · Game 2", "Between games", "Final · Game 1" (an earlier game of a running match) or "Final". */
export function stripStatus(input, game) {
  const m = input?.match ?? {};
  const final = () => (unofficial(m) ? { text: "Final", live: false, note: "Unofficial" } : { text: "Final", live: false });
  if (!game) return m.status === "running" ? { text: "Starting", live: false } : final();
  if (gameIsLive(input, game)) return { text: `LIVE · Game ${game.game_seq}`, live: true };
  if (m.status === "running" && game.game_seq === newestOf(input)?.game_seq) return { text: "Between games", live: false };
  return m.status === "running" ? { text: `Final · Game ${game.game_seq}`, live: false } : final();
}

/** A decided series whose result is not official (the Discord card's FINAL · UNOFFICIAL): never while open, and never
 *  held or abandoned (no result to be unofficial; live-card-core.js keeps the same rule). */
function unofficial(m) {
  const o = m?.outcome;
  return Boolean(o) && o.kind !== "open" && o.kind !== "held" && o.kind !== "abandoned" && o.official === false;
}

/**
 * The line score (spec section 3): G1..Gn, n = max(best_of, games played), the newest MAX_GAME_COLUMNS of them (R3); one
 * row per seat labelled like the pill; "-" for a game not played. `current` is the index of the picked game's column, or -1.
 */
export function lineScore(input, current = null) {
  const games = input?.games ?? [];
  const bestOf = Number.isInteger(input?.match?.best_of) ? input.match.best_of : 1;
  const n = Math.max(bestOf, 1, ...games.map((g) => g.game_seq));
  const seqs = Array.from({ length: n }, (_, i) => i + 1).slice(-MAX_GAME_COLUMNS);
  const st = seatTeams(input);
  const rows = [0, 1].map((seat) => ({
    seat,
    label: st[seat].label,
    cells: seqs.map((gs) => { const g = games.find((x) => x.game_seq === gs); return g ? String(scoreBySeat(g)[seat]) : "-"; }),
  }));
  return { cols: seqs.map((gs) => `G${gs}`), seqs, rows, current: current ? seqs.indexOf(current.game_seq) : -1 };
}

/**
 * The series pill's words (spec section 3, ruling R4): null when there is nothing to say. `winner` is the seat whose
 * colour the pill takes (a won or forfeited series), else null (the plain grey pill).
 */
export function seriesPill(input) {
  const out = seriesOutcome(input);
  const [a, b] = out.wins;
  const ranked = input?.match?.mode === "ranked";
  const games = input?.games ?? [];
  const st = seatTeams(input);
  const L = (seat) => st[seat].label;
  switch (out.kind) {
    case "win": {
      const w = out.winner ?? (a >= b ? 0 : 1);
      if (!ranked && games.length < 2) return null;
      const score = `${out.wins[w]}-${out.wins[1 - w]}`;
      return { text: ranked ? `${L(w)} wins the series ${score}` : `${L(w)} wins ${score}`, winner: w };
    }
    case "forfeit": { const w = out.winner ?? 0; return { text: `${L(w)} wins by forfeit`, winner: w }; }
    case "split":
      if (a + b === 0) return null;
      return { text: `Series split ${a}-${b}`, winner: null };
    case "held": return { text: "Under review", winner: null };
    case "abandoned": return { text: "No result", winner: null };
    default: {
      if (a + b === 0) return null;
      if (a === b) return { text: ranked ? `Series tied ${a}-${b}` : `Tied ${a}-${b}`, winner: null };
      const lead = a > b ? 0 : 1;
      const score = `${Math.max(a, b)}-${Math.min(a, b)}`;
      return { text: ranked ? `${L(lead)} leads the series ${score}` : `${L(lead)} leads ${score}`, winner: null };
    }
  }
}

const bit = (v) => v === 0 || v === 1;

/** The words under a drive header (R9). */
export const DRIVE_WORDS = Object.freeze({ touchdown: "Touchdown", turnover: "Turnover", downs: "Turnover on downs", end: "End of game", live: "In progress" });

/**
 * A game's drives from its plays, oldest first (R9): the rule of src/pbp/drives.js (a drive ends when the other side
 * has the ball on the next real play; a conversion try belongs to the drive that scored), computed here so streamed
 * plays group at once. A disputed play is LISTED in the drive it falls in (`seqs`) but never counted (plays, yards,
 * result), like the server, whose drives never see it. The last drive of a live game with no result is "live".
 */
export function driveGroups(game, { live = false } = {}) {
  const out = [];
  let cur = null;
  for (const p of game?.plays ?? []) {
    const b = p.body ?? {};
    const s = b.summary ?? {};
    const conv = s.kind === "conversion" || b.conversion === true;
    const disputed = p.status === "disputed";
    const side = bit(b.offense_side) ? b.offense_side : null;
    if (cur && (conv || disputed)) { cur.seqs.push(p.seq); continue; }
    if (!cur || side !== cur.side) {
      cur = { side, first_seq: p.seq, seqs: [], plays: 0, yards: 0, result: null };
      out.push(cur);
    }
    cur.seqs.push(p.seq);
    if (conv || disputed) continue;
    cur.plays += 1;
    if (s.yards_known === true && Number.isFinite(s.yards)) cur.yards += s.yards;
    if (s.touchdown === true) cur.result = "touchdown";
    else if (s.turnover === true && cur.result !== "touchdown") cur.result = "turnover";
  }
  out.forEach((d, i) => { if (!d.result) d.result = i + 1 < out.length ? "downs" : live ? "live" : "end"; });
  return out;
}

// ---- Box score (spec §6, ruling R10): columns are SEATS; one game or every game added together. ----------------

const SUM_KEYS = ["pass_att", "pass_cmp", "pass_yds", "pass_td", "pass_int", "sacked", "pass_yac", "rush_att", "rush_yds", "rush_td",
  "rec", "rec_yds", "rec_td", "rec_yac", "tackles", "tfl", "sacks", "def_int", "ff", "fr", "broken_tackles", "yds_after_contact", "drops", "pass_breakups"];
const LONG_KEYS = ["pass_long", "rush_long", "rec_long"];
// src/pbp/boxscore.js round1 and rtg, duplicated: this module ships to the browser with its one core import.
const round1 = (x) => Math.round(Number((x * 10).toPrecision(12))) / 10;
const clamp = (v) => Math.max(0, Math.min(2.375, v));
/** The passer rating, exactly the server's (src/pbp/boxscore.js rtg); null with no attempts. */
export function rtg({ att, cmp, yds, td, int }) {
  if (!att) return null;
  const a = clamp((cmp / att - 0.3) * 5), b = clamp((yds / att - 3) * 0.25);
  const c = clamp((td / att) * 20), d = clamp(2.375 - (int / att) * 25);
  return round1(((a + b + c + d) / 6) * 100);
}
function emptyStats() {
  const s = {};
  for (const k of SUM_KEYS) s[k] = 0;
  for (const k of LONG_KEYS) s[k] = null;
  return s;
}
function addInto(into, from) {
  for (const k of SUM_KEYS) into[k] += num(from?.[k]);
  for (const k of LONG_KEYS) { const v = from?.[k]; if (Number.isFinite(v)) into[k] = into[k] === null ? v : Math.max(into[k], v); }
  return into;
}
function finish(s) {
  return { ...s, pass_avg: s.pass_att ? round1(s.pass_yds / s.pass_att) : null, rush_ypc: s.rush_att ? round1(s.rush_yds / s.rush_att) : null,
    rec_avg: s.rec ? round1(s.rec_yds / s.rec) : null, pass_rtg: rtg({ att: s.pass_att, cmp: s.pass_cmp, yds: s.pass_yds, td: s.pass_td, int: s.pass_int }) };
}
const f1 = (v) => (Number.isFinite(v) ? v.toFixed(1) : "-");
const fLong = (v) => (Number.isFinite(v) ? String(v) : "-");
const n = (k) => (s) => num(s[k]);

/** The five tables and their columns (mockups game-espn-2 / boxscore-9): what the game records, nothing else. */
export const BOX_TABLES = Object.freeze([
  { key: "passing", title: "Passing", has: (s) => num(s.pass_att) > 0, sort: n("pass_yds"),
    cols: [["C/Att", (s) => `${num(s.pass_cmp)}/${num(s.pass_att)}`], ["Yds", n("pass_yds")], ["Avg", (s) => f1(s.pass_avg)], ["TD", n("pass_td")], ["Int", n("pass_int")], ["Rtg", (s) => f1(s.pass_rtg)]] },
  { key: "rushing", title: "Rushing", has: (s) => num(s.rush_att) > 0 || num(s.broken_tackles) > 0, sort: n("rush_yds"),
    cols: [["Car", n("rush_att")], ["Yds", n("rush_yds")], ["Avg", (s) => f1(s.rush_ypc)], ["TD", n("rush_td")], ["Long", (s) => fLong(s.rush_long)], ["BTK", n("broken_tackles")]] },
  { key: "receiving", title: "Receiving", has: (s) => num(s.rec) > 0 || num(s.drops) > 0, sort: n("rec_yds"),
    cols: [["Rec", n("rec")], ["Yds", n("rec_yds")], ["Avg", (s) => f1(s.rec_avg)], ["TD", n("rec_td")], ["Long", (s) => fLong(s.rec_long)], ["Drop", n("drops")]] },
  { key: "defense", title: "Defense", has: (s) => num(s.tackles) + num(s.sacks) + num(s.def_int) + num(s.ff) + num(s.pass_breakups) > 0,
    sort: (s) => num(s.tackles) + num(s.sacks) * 2 + num(s.def_int) * 3,
    cols: [["Tkl", n("tackles")], ["Sck", n("sacks")], ["Int", n("def_int")], ["FF", n("ff")], ["PBU", n("pass_breakups")]] },
  { key: "fumbles", title: "Fumbles", has: (s) => num(s.ff) + num(s.fr) > 0, sort: (s) => num(s.ff) + num(s.fr),
    cols: [["FF", n("ff")], ["FR", n("fr")]] },
]);

/** A box line's play side: its own `side` (snapshot lines), else team_index (gameSnapshot lines are already moved onto sides). */
const lineSide = (l) => (bit(l?.side) ? l.side : bit(l?.team_index) ? l.team_index : null);

/**
 * The box score for `pick` (a game_seq, or "total" for every game added together): two columns by SEAT, each the
 * five tables with a TEAM row, or the empty words ("No Buffalo fumbles"). Over a series a player is one row per seat,
 * keyed by roster pid, else name. A line with no known side is left out (R10). `any` is false when no game has lines.
 */
export function boxScore(input, pick) {
  const games = input?.games ?? [];
  const total = pick === "total";
  const chosen = total ? games : games.filter((g) => g.game_seq === pick);
  const st = seatTeams(input, total ? null : chosen[0] ?? null);
  const players = [new Map(), new Map()];
  const teamsUsed = [new Set(), new Set()];
  for (const g of chosen) {
    const lines = Array.isArray(g.box?.lines) ? g.box.lines : [];
    for (const seat of [0, 1]) { const t = teamFor(g.teams, sideOfSeat(g, seat)); if (t?.name) teamsUsed[seat].add(t.name); }
    for (const l of lines) {
      if (!l || !l.stats || typeof l.stats !== "object") continue;
      const side = lineSide(l);
      const seat = side === null ? null : (g.seatOfSide ?? [0, 1])[side];
      if (!bit(seat)) continue;
      const key = Number.isInteger(l.pid) ? `p${l.pid}` : `n${clean(l.name)}`;
      if (!players[seat].has(key)) players[seat].set(key, { name: clean(l.name) || "Player", stats: emptyStats() });
      addInto(players[seat].get(key).stats, l.stats);
    }
  }
  const columns = [0, 1].map((seat) => {
    const s = st[seat];
    const one = !total || teamsUsed[seat].size <= 1;
    const prefix = (one && s.team ? clean(s.team.city) || s.team.name : "") || s.player || s.label;
    const lines = [...players[seat].values()].map((p) => ({ name: p.name, stats: finish(p.stats) }));
    const tables = BOX_TABLES.map((t) => {
      const rows = lines.filter((l) => t.has(l.stats)).sort((a, b) => t.sort(b.stats) - t.sort(a.stats) || a.name.localeCompare(b.name));
      const sum = rows.length ? finish(rows.reduce((acc, r) => addInto(acc, r.stats), emptyStats())) : null;
      return {
        key: t.key, title: `${prefix} ${t.title}`, heads: t.cols.map(([h]) => h),
        rows: rows.map((r) => ({ name: r.name, cells: t.cols.map(([, f]) => String(f(r.stats))) })),
        total: sum ? t.cols.map(([, f]) => String(f(sum))) : null,
        empty: rows.length ? null : `No ${prefix} ${t.title.toLowerCase()}`,
      };
    });
    const sum = finish(lines.reduce((acc, l) => addInto(acc, l.stats), emptyStats()));
    return { seat, label: s.label, prefix, team: one ? s.team : null, tables, sum };
  });
  return { kind: total ? "total" : "game", any: players.some((m) => m.size > 0), columns };
}

/** Rows where the LOWER number is the better one (spec §3; R8 adds Drops). */
export const LOWER_IS_BETTER = Object.freeze(new Set(["Turnovers", "Interceptions thrown", "Drops"]));

/** Which side of a row reads brighter (R8): [a, b] booleans; a tie is both, a row with no number is neither. */
export function betterOf(a, b, lower = false) {
  if (!Number.isFinite(a) || !Number.isFinite(b)) return [false, false];
  if (a === b) return [true, true];
  return (lower ? a < b : a > b) ? [true, false] : [false, true];
}

/**
 * Team Stats, the ESPN table (spec §3, ruling R8): one row per stat, one value per SEAT, and `better` [a, b] saying
 * which value is drawn bright. Box numbers come from the box score's per-seat sums; First downs, Turnovers and Plays
 * come from the plays (a disputed play or a conversion try never counts) and read "-" when no chosen game has plays
 * (a quiet recorded game), never a false 0. Comp/Att is compared by completion rate.
 */
export function teamStats(input, pick) {
  const m = boxScore(input, pick);
  const games = (input?.games ?? []).filter((g) => pick === "total" || g.game_seq === pick);
  const withPlays = games.filter((g) => (g.plays ?? []).length);
  const firstDowns = [0, 0], snaps = [0, 0], turnovers = [0, 0];
  for (const g of withPlays) for (const p of g.plays) {
    const b = p.body ?? {};
    const s = b.summary ?? {};
    if (p.status === "disputed" || s.kind === "conversion" || b.conversion === true || !bit(b.offense_side)) continue;
    const seat = (g.seatOfSide ?? [0, 1])[b.offense_side];
    if (!bit(seat)) continue;
    snaps[seat] += 1;
    if (s.first_down === true) firstDowns[seat] += 1;
    if (s.turnover === true) turnovers[seat] += 1;
  }
  const fromPlays = (a) => (withPlays.length ? { values: a.map(String), cmp: a } : { values: ["-", "-"], cmp: null });
  const v = (f) => ({ values: m.columns.map((c) => String(f(c.sum))), cmp: m.columns.map((c) => num(f(c.sum))) });
  const rate = (c, a) => (a > 0 ? c / a : null);
  const rows = [
    ["Total yards", v((x) => x.pass_yds + x.rush_yds)],
    ["Passing yards", v((x) => x.pass_yds)],
    ["Comp/Att", { values: m.columns.map((c) => `${c.sum.pass_cmp}/${c.sum.pass_att}`), cmp: m.columns.map((c) => rate(c.sum.pass_cmp, c.sum.pass_att)) }],
    ["Rushing yards", v((x) => x.rush_yds)],
    ["Carries", v((x) => x.rush_att)],
    ["Yards per carry", { values: m.columns.map((c) => f1(c.sum.rush_ypc)), cmp: m.columns.map((c) => c.sum.rush_ypc) }],
    ["Touchdowns", v((x) => x.pass_td + x.rush_td)],
    ["Turnovers", fromPlays(turnovers)],
    ["Interceptions thrown", v((x) => x.pass_int)],
    ["Fumbles recovered", v((x) => x.fr)],
    ["Sacks", v((x) => x.sacks)],
    ["Tackles", v((x) => x.tackles)],
    ["Broken tackles", v((x) => x.broken_tackles)],
    ["Drops", v((x) => x.drops)],
    ["First downs", fromPlays(firstDowns)],
    ["Plays", fromPlays(snaps)],
  ];
  return {
    any: m.any,
    heads: m.columns.map((c) => ({ seat: c.seat, label: c.label, team: c.team })),
    rows: rows.map(([label, r]) => ({ label, values: r.values, better: r.cmp ? betterOf(r.cmp[0], r.cmp[1], LOWER_IS_BETTER.has(label)) : [false, false] })),
  };
}

// ---- Game page v2 (spec 2026-10-03): the drive list, its chart, the scoring filter ----------------------------

/** The detailed words of a drive's end (R3). NFL Street has no kicking, so there is no punt. */
export const DRIVE_END_WORDS = Object.freeze({ touchdown: "Touchdown", interception: "Interception", fumble: "Fumble", safety: "Safety",
  downs: "Turnover on downs", end: "End of game", live: "Current drive" });

const isDigp = (d) => Number.isInteger(d) && d >= 0;
const clampYd = (v, L) => Math.min(L, Math.max(0, v));
/** A play gave the defense a safety: the decoder's flag, or the game's own team `tsst` stat rising (live-render safetyOf). */
const safetyPlay = (b) => b?.summary?.safety === true
  || (Array.isArray(b?.stat_changes) && b.stat_changes.some((c) => c?.team && c.field === "tsst" && Number.isFinite(c.before) && Number.isFinite(c.after) && c.after > c.before));

/** How a drive ended (R3), from its last counted play: a turnover first (a defensive touchdown is still an
 *  interception or a fumble), then the server result, a safety read off a drive that otherwise ended on downs. */
function endOf(d, last) {
  const s = last?.body?.summary ?? {};
  if (s.turnover === true) return s.kind === "interception" || isDigp(s.interceptor) ? "interception" : "fumble";
  if ((d.result === "downs" || d.result === "end") && last && safetyPlay(last.body)) return "safety";
  return d.result;
}

/** "ATL 7", "PIT 3" or "midfield": the core's spot rule (its private spotLabel), on our own labels. */
export function spotOf(yd, L, offLabel, defLabel) {
  if (!Number.isFinite(yd) || !Number.isFinite(L) || L <= 0) return null;
  const half = L / 2;
  if (yd < half - 0.5) return `${offLabel} ${Math.round(yd)}`;
  if (yd > half + 0.5) return `${defLabel} ${Math.round(L - yd)}`;
  return "midfield";
}

/**
 * The All plays drive list of `game` (a CardInput game), NEWEST FIRST (spec §1, rulings R3, R4, R6, R7). Each drive:
 * driveGroups' fields plus `key` ("d<first_seq>"), `end` (a DRIVE_END_WORDS key), `current` (the newest drive of a live
 * game), `score` [seat 0, seat 1] after the drive and `before` (the score it started from), `scored`, `newest` (its
 * newest non-disputed seq), and `chart` { start, end, field_yds, from, to } (own yards and spot words) or null.
 */
export function driveList(input, game, { live = false } = {}) {
  const groups = driveGroups(game, { live });
  const bySeq = new Map((game?.plays ?? []).map((p) => [p.seq, p]));
  const st = seatTeams(input, game);
  const label = (side) => st.find((s) => s.side === side)?.label ?? `Side ${side + 1}`;
  const L = (game?.plays ?? []).find((p) => Number.isFinite(p.field?.field_yds) && p.field.field_yds > 0)?.field.field_yds ?? null;
  const toSeat = (sc) => [num(sc[sideOfSeat(game, 0)]), num(sc[sideOfSeat(game, 1)])];
  let running = [0, 0];
  const out = groups.map((d, i) => {
    const plays = d.seqs.map((q) => bySeq.get(q)).filter(Boolean);
    const ok = plays.filter((p) => p.status !== "disputed");
    const counted = ok.filter((p) => !(p.body?.summary?.kind === "conversion" || p.body?.conversion === true));
    const withScore = [...ok].reverse().find((p) => Array.isArray(p.score_after));
    const before = running;
    const score = withScore ? toSeat(withScore.score_after) : before;
    running = score;
    const start = counted[0]?.field?.own_yd;
    const endWord = endOf(d, counted.at(-1));
    let chart = null;
    if (bit(d.side) && Number.isFinite(start) && Number.isFinite(L)) {
      const td = d.result === "touchdown" && counted.at(-1)?.body?.summary?.turnover !== true;
      const end = td ? L : clampYd(start + d.yards, L);
      chart = { start: clampYd(start, L), end, field_yds: L,
        from: spotOf(clampYd(start, L), L, label(d.side), label(1 - d.side)),
        to: td || endWord === "safety" ? "the end zone" : spotOf(end, L, label(d.side), label(1 - d.side)) };
    }
    return { ...d, key: `d${d.first_seq}`, end: endWord, current: live && i === groups.length - 1,
      before, score, scored: score[0] !== before[0] || score[1] !== before[1], newest: ok.at(-1)?.seq ?? null, chart };
  });
  return out.reverse();
}

/** Scoring plays (R1): only the drives that changed the score. */
export const scoringDrives = (drives) => drives.filter((d) => d.scored);

// ---- Spots with team abbrevs (spec §5, ruling R9) -------------------------------------------------------------

const MAX_COORD = 200;   // src/pbp/situation.js coord(): a field coordinate is trusted within 200 yards of midfield
const coord = (v) => (typeof v === "number" && Number.isFinite(v) && Math.abs(v) <= MAX_COORD ? v : null);
const r2 = (v) => Math.round(v * 100) / 100;

/**
 * A play's numeric field from its stored body, for /games/:id (whose /plays route sends { body, situation } only):
 * src/pbp/situation.js's ownYd and field_yds exactly, the down from the body, the distance from the server's text
 * ("1st & 10 at opp 32", "2nd & goal ..."). Null when the snap is unknown. A conversion keeps down and distance null.
 */
export function fieldFromBody(body, text = null) {
  const z = coord(body?.snap?.z);
  if (z === null) return null;
  const goalZ = coord(body?.goal_z);
  const goal = goalZ > 0 ? goalZ : 40;
  const conv = body?.summary?.kind === "conversion" || body?.conversion === true;
  const m = typeof text === "string" ? /&\s*(goal|\d+)/i.exec(text) : null;
  const toGo = conv || !m ? null : /goal/i.test(m[1]) ? "goal" : Number(m[1]);
  return { own_yd: r2(Math.min(2 * goal, Math.max(0, z + goal))), field_yds: r2(2 * goal),
    down: conv || !Number.isInteger(body?.down) ? null : body.down, to_go: toGo };
}

/** The server's situation text with team abbrevs (R9): "own 20" -> "ATL 20", "opp 3" -> "PIT 3", "conversion" ->
 *  "Conversion try". `offense` is the play's offense side; `teams` by side. Unknown teams keep the server's words. */
export function spotWords(text, offense, teams) {
  const t = clean(text);
  if (/^conversion$/i.test(t)) return "Conversion try";
  if (!bit(offense)) return t;
  const ab = (side) => { const a = clean(teams?.[side]?.abbrev).toUpperCase(); return ABBREV.test(a) ? a : null; };
  const off = ab(offense), def = ab(1 - offense);
  return t.replace(/\bown (\d+)/, (w, n) => (off ? `${off} ${n}` : w)).replace(/\bopp (\d+)/, (w, n) => (def ? `${def} ${n}` : w))
    .replace(/& goal\b/, "& Goal");
}
