// Pure view models for the public profile, the online player hub (spec §8.7). No DOM. Each function takes the server's
// answer as it is and returns strings ready to draw, or null / [] when the server did not send what a piece needs, so
// the page leaves that piece out: nothing is invented (spec §8.7, "Degrades gracefully until the backend work ships").
// An older server's shapes are handled here too: private rooms (backend §1) and rushing `yac` for YCO (backend §9).
import { eloChange, fmtInt, fmtPct, playerKind } from './stats-model.js';

// Win-loss records use the ASCII hyphen ("14-9"); scores and series keep the en dash.
const DASH = '—', EN = '–', MINUS = '−', REC = '-';
const num = (n) => typeof n === 'number' && Number.isFinite(n);
const one = (n) => n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
const sum = (...xs) => (xs.some(num) ? xs.reduce((a, x) => a + (num(x) ? x : 0), 0) : null);

/** Private rooms never show on a profile (backend §1; the deployed server still lists them). */
export const isPublic = (m) => Boolean(m) && m.queue !== 'private';
export function publicQueues(rows) {
  return (Array.isArray(rows) ? rows : []).filter((r) => r && r.key !== 'private' && !/private/i.test(String(r.name ?? '')));
}

export const winRate = (p) => (num(p) ? `${one(p * 100)}%` : null);

/**
 * The profile's record (owner 10-03): ONLINE matches only (a profile never has offline games), Ranked and Casual as two
 * separate figures. The headline is ranked + casual from `by_queue` (the deployed server's `summary` still counts private
 * rooms); a server without `by_queue` gives its summary alone. Null when neither is sent.
 */
export function profileRecord(summary, byQueue) {
  const rows = publicQueues(byQueue);
  const q = (key) => {
    const r = rows.find((x) => x.key === key);
    if (!r) return null;
    const wins = num(r.wins) ? r.wins : 0, losses = num(r.losses) ? r.losses : 0;
    return { wins, losses, text: `${wins}${REC}${losses}`, pct: wins + losses ? wins / (wins + losses) : null };
  };
  const ranked = q('ranked'), casual = q('casual');
  let wins, losses;
  if (ranked || casual) { wins = (ranked?.wins ?? 0) + (casual?.wins ?? 0); losses = (ranked?.losses ?? 0) + (casual?.losses ?? 0); }
  else if (num(summary?.wins) && num(summary?.losses)) { wins = summary.wins; losses = summary.losses; }
  else return null;
  return { wins, losses, games: wins + losses, text: `${wins}${REC}${losses}`, pct: wins + losses ? wins / (wins + losses) : null, ranked, casual };
}

/** The card's one summary line: "#7 on the leaderboard | 1,184 Elo | 14-9 record" (owner 10-03; the record online only). */
export function summaryFacts(p) {
  const out = [];
  if (num(p?.elo?.rank)) out.push({ strong: `#${p.elo.rank}`, text: 'on the leaderboard' });
  if (num(p?.elo?.rating)) out.push({ strong: fmtInt(p.elo.rating), text: 'Elo' });
  const r = profileRecord(p?.summary, p?.by_queue);
  if (r && r.games) out.push({ strong: r.text, text: 'record' });
  return out;
}

/** The live card (`header.live`, backend §7): "Ranked · Game 2 of 3", "14 – 7", "vs Mikel · Series 1–0". No halves. */
export function liveCard(live) {
  if (!live || typeof live !== 'object' || live.queue === 'private' || !live.match_id) return null;
  const queue = live.queue === 'ranked' ? 'Ranked' : live.queue === 'casual' ? 'Casual' : 'Online';
  const bo = num(live.best_of) ? live.best_of : 1;
  const head = bo > 1 && num(live.game_number) ? `${queue} · Game ${live.game_number} of ${bo}` : queue;
  const score = Array.isArray(live.score) && num(live.score[0]) && num(live.score[1]) ? `${live.score[0]} ${EN} ${live.score[1]}` : null;
  const parts = [];
  if (live.opponent?.display_name) parts.push(`vs ${live.opponent.display_name}`);
  if (bo > 1 && Array.isArray(live.series) && num(live.series[0]) && num(live.series[1])) parts.push(`Series ${live.series[0]}${EN}${live.series[1]}`);
  return { head, me: live.me || null, opp: live.opp || null, score, line: parts.join(' · '), href: `live.html?match=${encodeURIComponent(live.match_id)}` };
}

/** "● In a match" while a (public) match runs, "● Online now" when online, else nothing. */
export function statusLine(header) {
  if (liveCard(header?.live)) return { text: 'In a match', kind: 'live' };
  if (header?.online === true) return { text: 'Online now', kind: 'online' };
  return null;
}

/** An area sparkline in a w×h box: the line's points and the filled area's path. */
export function areaPoints(values, w = 300, h = 70, pad = 6) {
  if (!Array.isArray(values) || values.length < 2) return null;
  const lo = Math.min(...values), hi = Math.max(...values);
  const r1 = (x) => Math.round(x * 10) / 10;
  const pts = values.map((v, i) => `${r1((i / (values.length - 1)) * w)},${r1(hi === lo ? h / 2 : pad + (h - 2 * pad) * (1 - (v - lo) / (hi - lo)))}`);
  return { line: pts.join(' '), area: `M${pts.join(' L')} L${w},${h} L0,${h}Z` };
}

/** Overview column 1: "1,184 +16", "Peak 1,212 · started at 1,000". The peak comes from the server (backend §11) or
 *  from the history it sent. */
export function eloOverview(elo) {
  if (!num(elo?.rating)) return null;
  const hist = (Array.isArray(elo.history) ? elo.history : []).filter((x) => x && num(x.rating));
  const start = hist.length ? hist[0].rating - (num(hist[0].delta) ? hist[0].delta : 0) : elo.rating;
  const peak = num(elo.peak) ? elo.peak : Math.max(elo.rating, start, ...hist.map((x) => x.rating));
  const n = hist.length;
  return { value: fmtInt(elo.rating), change: eloChange(elo.last_delta), sub: `Peak ${fmtInt(peak)} · started at ${fmtInt(start)}`,
    note: `${n} ranked ${n === 1 ? 'match' : 'matches'}`, points: areaPoints([start, ...hist.map((x) => x.rating)]) };
}

/** The Record tile: "14-9 60.9%" (online only, profileRecord), then Ranked and Casual as two separate figures. */
export function recordOverview(summary, byQueue) {
  const r = profileRecord(summary, byQueue);
  if (!r) return null;
  const queues = [['Ranked', r.ranked], ['Casual', r.casual]].filter(([, t]) => t)
    .map(([label, t]) => ({ label, record: t.text, pct: t.pct === null ? DASH : fmtPct(t.pct) }));
  return { value: r.text, pct: winRate(r.pct) ?? DASH, note: `${r.games} ${r.games === 1 ? 'match' : 'matches'}`, queues };
}

/** Overview column 3 (profile `recent_form`, backend §8): "6-4 +6.4", points scored and allowed per game. */
export function lastTenOverview(rf) {
  if (!rf || !num(rf.games) || rf.games === 0 || !num(rf.wins) || !num(rf.losses)) return null;
  const m = num(rf.avg_margin) ? rf.avg_margin : null;
  const margin = m === null ? { text: '', kind: '' } : { text: `${m > 0 ? '+' : m < 0 ? MINUS : ''}${one(Math.abs(m))}`, kind: m > 0 ? 'up' : m < 0 ? 'down' : '' };
  return { value: `${rf.wins}${REC}${rf.losses}`, margin, scored: num(rf.pts_for_pg) ? one(rf.pts_for_pg) : DASH, allowed: num(rf.pts_against_pg) ? one(rf.pts_against_pg) : DASH };
}

// Career tiles: [key (career_rank field), label, value from the career totals, per game?, one decimal?]
const TILES = [
  ['pass_yds', 'Passing Yards', (c) => c.passing?.total?.yds, true],
  ['pass_td', 'Passing TD', (c) => c.passing?.total?.td, true],
  ['pass_rtg', 'Passer Rating', (c) => c.passing?.total?.rtg, false, true],
  ['rush_yds', 'Rushing Yards', (c) => c.rushing?.total?.yds, true],
  ['rec_yds', 'Receiving Yards', (c) => c.receiving?.total?.yds, true],
  // The backend's total_td (§6) is rushing + receiving + defensive: a thrown TD is also a caught one.
  ['total_td', 'Total TD', (c) => sum(c.rushing?.total?.td, c.receiving?.total?.td, c.defense?.total?.td), true],
  ['tkl', 'Tackles', (c) => c.defense?.total?.tkl, true],
  ['sck', 'Sacks', (c) => c.defense?.total?.sck, true],
  ['def_int', 'Interceptions', (c) => c.defense?.total?.int, true],
];

/** The Career card: nine tiles, each "#3 · 125.9 / game" when the rank (backend §6) and a game count are known. */
export function careerTiles(career, rank, games) {
  const c = career || {};
  const tiles = TILES.map(([key, label, get, perGame, dec]) => {
    const v = get(c);
    return { key, label, value: num(v) ? (dec ? one(v) : fmtInt(v)) : DASH, rank: num(rank?.[key]) ? `#${rank[key]}` : null,
      perGame: perGame && num(v) && num(games) && games > 0 ? `${one(v / games)} / game` : null };
  });
  const pool = num(rank?.pool) && tiles.some((t) => t.rank) ? rank.pool : null;
  return { tiles, footnote: pool ? `Rank among ${fmtInt(pool)} online players` : null };
}

// Which career table holds each role's row (backend profile/players.js bestPlayers picks from these same rows).
const ROLE_TABLE = { passer: 'passing', rusher: 'rushing', receiver: 'receiving', dl_lb: 'defense', ballhawk: 'defense' };
const STAT_KEYS = ['games', 'cmp', 'att', 'yds', 'td', 'int', 'rtg', 'avg', 'long', 'rec', 'tkl', 'sck', 'tfl', 'ff', 'pbu'];
const pickStats = (row) => Object.fromEntries(STAT_KEYS.filter((k) => num(row?.[k])).map((k) => [k, row[k]]));

/** Your players' fuller statlines (owner 10-03): `best_players` sends one number per role, the profile's `career`
 *  tables (the same rows the server picked them from) send the rest. Each best player gets its own career row's
 *  numbers, matched by `key`; a field the server sent on the player itself wins. Most played takes its games and the
 *  rushing or receiving row with more yards (never the passing row: its `att` is pass attempts, not carries). */
export function withCareerLines(best, career) {
  const out = { ...(best || {}) };
  const rows = (t) => (Array.isArray(career?.[t]?.rows) ? career[t].rows : []);
  for (const [role, b] of Object.entries(out)) {
    if (!b || typeof b !== 'object' || typeof b.key !== 'string') continue;
    const find = (t) => rows(t).find((r) => r && r.key === b.key) ?? null;
    let extra = {};
    if (ROLE_TABLE[role]) extra = pickStats(find(ROLE_TABLE[role]));
    else if (role === 'most_played') {
      // A server that already sends the full row (backend cc79302) keeps it: its own columns say its role. Else one
      // career row, by role (Adam 10-03, position-relevant): a quarterback's passing row; else the rushing or
      // receiving row with more yards; else the defensive row. Never columns from two roles.
      if (playerKind(b)) { out[role] = { ...b }; continue; }
      const rush = find('rushing'), rec = find('receiving');
      const pick = find('passing') || (rush && rec ? ((num(rec.yds) ? rec.yds : 0) > (num(rush.yds) ? rush.yds : 0) ? rec : rush) : rush || rec) || find('defense');
      extra = pickStats(pick);
      if (num(b.value) && b.unit === 'games') extra.games = b.value;
    }
    out[role] = { ...extra, ...b };
  }
  return out;
}

/** A label spelled out in words says "Yards", never "YDS"/"yds" (owner 10-03): the server's record labels read
 *  "Passing yds"; this draws "Passing Yards". A bare "YDS" (a compact column heading) is left alone. */
export function spellYards(label) {
  const s = String(label ?? '');
  return /\S\s+yds\b|\byds\s+\S/i.test(s) ? s.replace(/\byds\b/gi, 'Yards') : s;
}

const dayOf = (iso) => { const d = new Date(iso); return Number.isFinite(d.getTime()) ? d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) : DASH; };
const whole = (n) => (num(n) ? fmtInt(Math.round(n)) : DASH);

/** game id -> the opponent's name, from a match list (each match's games, and a one-game match's own "g:<id>"). */
function opponentsByGame(matches) {
  const out = new Map();
  for (const m of Array.isArray(matches) ? matches : []) {
    if (!m || m.queue === 'private') continue;
    const name = m.opponent?.display_name || (m.opponent_kind === 'cpu' ? 'CPU' : null);
    if (!name) continue;
    for (const g of Array.isArray(m.games) ? m.games : []) if (g && typeof g.id === 'string') out.set(g.id, name);
    if (typeof m.id === 'string' && m.id.startsWith('g:')) out.set(m.id.slice(2), name);
  }
  return out;
}

/**
 * Records, Single game (owner 10-03): one row per record the server sends (`records`), "Yards" spelled out, the player
 * and their team, the best, the game as "vs <opponent>" and its date. The record itself carries no opponent: it is
 * looked up in the match list the page has (`matches`), else the server's own `opponent` when a later server sends one,
 * else a dash. A row opens its game (`game_id`).
 */
export function singleGameRecords(records, matches) {
  const opp = opponentsByGame(matches);
  return (Array.isArray(records) ? records : []).filter((r) => r && r.label).map((r) => {
    const vs = (typeof r.opponent === 'string' ? r.opponent : r.opponent?.display_name) || opp.get(r.game_id) || null;
    return { key: String(r.key ?? r.label), label: spellYards(r.label), name: r.name || null,
      team: r.name ? { name: r.team ?? null, logo_url: r.logo_url ?? null, abbrev: r.abbrev ?? null } : null,
      value: whole(r.value), game: vs ? `vs ${vs}` : DASH, date: dayOf(r.at), game_id: typeof r.game_id === 'string' ? r.game_id : null };
  });
}

// Career records (mv2-2): [label, table, value from a row, per game?]. Total TD is rushing + receiving + defensive
// (careerTiles' rule). Passer Rating's leader is among the passers with at least a quarter of the most attempts, so a
// backup's one perfect throw never leads it; it has no per-game figure.
const CAREER_RECORDS = [
  ['Passing Yards', 'passing', (r) => r.yds, true], ['Passing TD', 'passing', (r) => r.td, true], ['Passer Rating', 'passing', (r) => r.rtg, false],
  ['Rushing Yards', 'rushing', (r) => r.yds, true], ['Rushing TD', 'rushing', (r) => r.td, true],
  ['Receptions', 'receiving', (r) => r.rec, true], ['Receiving Yards', 'receiving', (r) => r.yds, true], ['Receiving TD', 'receiving', (r) => r.td, true],
  ['Tackles', 'defense', (r) => r.tkl, true], ['Sacks', 'defense', (r) => r.sck, true], ['Interceptions', 'defense', (r) => r.int, true],
  ['Total TD', null, null, true],
];

/**
 * Records, Career: the player who leads each category over their whole career, from the career tables the profile
 * sends (every player used, with their games): the total, and per game when the player's games are known. A category
 * nobody has is left out. "N games" stays on each row (`game`) for its label.
 */
export function careerRecords(career) {
  const rows = (t) => (Array.isArray(career?.[t]?.rows) ? career[t].rows.filter((r) => r && r.name) : []);
  const team = (r) => ({ name: r.team ?? null, logo_url: r.logo_url ?? null, abbrev: r.abbrev ?? null });
  const out = [];
  for (const [label, table, get, perGame] of CAREER_RECORDS) {
    let best = null;
    if (table) {
      let pool = rows(table);
      if (label === 'Passer Rating') { const most = Math.max(0, ...pool.map((r) => (num(r.att) ? r.att : 0))); pool = pool.filter((r) => num(r.att) && r.att > 0 && r.att * 4 >= most); }
      for (const r of pool) { const v = get(r); if (num(v) && v > 0 && (!best || v > best.v)) best = { r, v }; }
    } else {
      const by = new Map();
      for (const t of ['rushing', 'receiving', 'defense']) for (const r of rows(t)) {
        if (!num(r.td)) continue;
        const k = r.key ?? r.name;
        const was = by.get(k);
        by.set(k, { r: was?.r ?? r, v: (was?.v ?? 0) + r.td, games: Math.max(was?.games ?? 0, num(r.games) ? r.games : 0) });
      }
      for (const x of by.values()) if (x.v > 0 && (!best || x.v > best.v)) best = { r: { ...x.r, games: x.games }, v: x.v };
    }
    if (!best) continue;
    const g = num(best.r.games) && best.r.games > 0 ? best.r.games : null;
    const dec = label === 'Passer Rating';
    out.push({ key: label, label, name: best.r.name, team: team(best.r), value: dec ? one(best.v) : whole(best.v),
      perGame: perGame && g ? one(best.v / g) : DASH, game: g ? `${fmtInt(g)} ${g === 1 ? 'game' : 'games'}` : DASH, date: '', game_id: null });
  }
  return out;
}

/**
 * Team records (mv2-2): the records with no player (most points, biggest win, biggest comeback, longest TD), each a
 * cell: the label, the number (a win's margin "+42") and "Sep 24 vs Playz" (the opponent from the match list).
 */
export function teamRecords(records, matches) {
  return singleGameRecords(records, matches).filter((r) => !r.name).map((r) => ({
    key: r.key, label: r.label, value: r.key === 'biggest_win' && r.value !== DASH ? `+${r.value}` : r.value,
    note: [r.date !== DASH ? r.date : null, r.game !== DASH ? r.game : null].filter(Boolean).join(' '), game_id: r.game_id }));
}

/** Single game player records only (the ones with a player's name); the rest are Team records. */
export const playerRecords = (records, matches) => singleGameRecords(records, matches).filter((r) => r.name);

/** The Leaderboard card (`elo.around`, backend §5/§14). The profile's own player is marked; it reads "You" only when the
 *  viewer is that player. */
export function leaderboardRows(around, { self = false } = {}) {
  return (Array.isArray(around) ? around : []).filter((r) => r && num(r.rank)).map((r) => ({
    rank: `#${r.rank}`, name: r.is_player && self ? 'You' : String(r.display_name || 'Player'), elo: num(r.rating) ? fmtInt(r.rating) : DASH,
    me: Boolean(r.is_player), avatar_url: r.avatar_url ?? null, player_id: typeof r.player_id === 'string' ? r.player_id : null,
    letterOf: String(r.display_name || 'Player'),
  }));
}

const col = (label, title, key, extra = {}) => ({ label, title, key, ...extra });
/** Full stats (spec §8.7.5): the four tables' columns, units uppercase. */
export const FULL_STATS = {
  passing: { title: 'Passing', cols: [
    col('C/ATT', 'Completions / attempts', 'cmp', { text: (r) => (num(r?.cmp) && num(r?.att) ? `${fmtInt(r.cmp)}/${fmtInt(r.att)}` : DASH) }),
    col('YDS', 'Passing yards', 'yds'), col('TD', 'Touchdowns', 'td'), col('INT', 'Interceptions', 'int'), col('AIR', 'Air yards', 'air'),
    col('YAC', 'Yards after catch', 'yac'), col('Y/A', 'Yards per attempt', 'ypa', { dec: true }), col('RTG', 'Passer rating', 'rtg', { dec: true }),
    col('LNG', 'Longest', 'long')] },
  rushing: { title: 'Rushing', cols: [
    col('CAR', 'Carries', 'att'), col('YDS', 'Rushing yards', 'yds'), col('AVG', 'Yards per carry', 'avg', { dec: true }), col('TD', 'Touchdowns', 'td'),
    col('LNG', 'Longest', 'long'), col('BTK', 'Broken tackles', 'btk'),
    col('YCO', 'Yards after contact', 'yco', { get: (r) => (num(r?.yco) ? r.yco : r?.yac) })] },
  receiving: { title: 'Receiving', cols: [
    col('REC', 'Receptions', 'rec'), col('YDS', 'Receiving yards', 'yds'), col('AVG', 'Yards per catch', 'avg', { dec: true }),
    col('YAC', 'Yards after catch', 'yac'), col('TD', 'Touchdowns', 'td'), col('LNG', 'Longest', 'long'), col('DRP', 'Drops', 'drops')] },
  defense: { title: 'Defense', cols: [
    col('TKL', 'Tackles', 'tkl'), col('TFL', 'Tackles for loss', 'tfl'), col('SCK', 'Sacks', 'sck'), col('INT', 'Interceptions', 'int'),
    col('PBU', 'Passes broken up', 'pbu'), col('FF', 'Forced fumbles', 'ff'), col('FR', 'Fumble recoveries', 'fr'), col('TD', 'Defensive touchdowns', 'td')] },
};

export function sortValue(c, r) { return c.get ? c.get(r) : r?.[c.key]; }
export function cellValue(c, r) {
  if (c.text) return c.text(r);
  const v = sortValue(c, r);
  if (!num(v)) return DASH;
  return c.dec ? one(v) : fmtInt(Math.round(v));
}
export function splitTop(rows, n = 5) {
  const list = Array.isArray(rows) ? rows : [];
  return { top: list.slice(0, n), rest: list.slice(n) };
}
