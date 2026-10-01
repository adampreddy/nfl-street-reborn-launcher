// Pure view models for the public profile, the online player hub (spec §8.7). No DOM. Each function takes the server's
// answer as it is and returns strings ready to draw, or null / [] when the server did not send what a piece needs, so
// the page leaves that piece out: nothing is invented (spec §8.7, "Degrades gracefully until the backend work ships").
// An older server's shapes are handled here too: private rooms (backend §1) and rushing `yac` for YCO (backend §9).
import { eloChange, fmtInt, fmtPct } from './stats-model.js';

const DASH = '—', EN = '–', MINUS = '−';
const num = (n) => typeof n === 'number' && Number.isFinite(n);
const one = (n) => n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
const sum = (...xs) => (xs.some(num) ? xs.reduce((a, x) => a + (num(x) ? x : 0), 0) : null);

/** Private rooms never show on a profile (backend §1; the deployed server still lists them). */
export const isPublic = (m) => Boolean(m) && m.queue !== 'private';
export function publicQueues(rows) {
  return (Array.isArray(rows) ? rows : []).filter((r) => r && r.key !== 'private' && !/private/i.test(String(r.name ?? '')));
}

export const winRate = (p) => (num(p) ? `${one(p * 100)}%` : null);

/** The card's one summary line: "#7 on the leaderboard | 1,184 Elo | 60.9% win rate" (owner round 11). */
export function summaryFacts(p) {
  const out = [];
  if (num(p?.elo?.rank)) out.push({ strong: `#${p.elo.rank}`, text: 'on the leaderboard' });
  if (num(p?.elo?.rating)) out.push({ strong: fmtInt(p.elo.rating), text: 'Elo' });
  const w = winRate(p?.summary?.win_pct);
  if (w) out.push({ strong: w, text: 'win rate' });
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

/** Overview column 2: "14–9 60.9%", "Ranked and casual", then Ranked and Casual as label / bar / record / pct. */
export function recordOverview(summary, byQueue) {
  const s = summary || {};
  if (!num(s.wins) || !num(s.losses)) return null;
  const total = s.wins + s.losses;
  const rows = publicQueues(byQueue).filter((r) => r.key === 'ranked' || r.key === 'casual').map((r) => {
    const w = num(r.wins) ? r.wins : 0, l = num(r.losses) ? r.losses : 0, n = w + l;
    return { label: r.key === 'ranked' ? 'Ranked' : 'Casual', record: `${w}${EN}${l}`, pct: n ? fmtPct(w / n) : DASH, share: n ? Math.round((w / n) * 1000) / 10 : 0 };
  });
  return { value: `${s.wins}${EN}${s.losses}`, pct: winRate(s.win_pct) ?? DASH, note: `${total} ${total === 1 ? 'match' : 'matches'}`, sub: 'Ranked and casual', rows };
}

/** Overview column 3 (profile `recent_form`, backend §8): "6–4 +6.4", points scored and allowed per game. */
export function lastTenOverview(rf) {
  if (!rf || !num(rf.games) || rf.games === 0 || !num(rf.wins) || !num(rf.losses)) return null;
  const m = num(rf.avg_margin) ? rf.avg_margin : null;
  const margin = m === null ? { text: '', kind: '' } : { text: `${m > 0 ? '+' : m < 0 ? MINUS : ''}${one(Math.abs(m))}`, kind: m > 0 ? 'up' : m < 0 ? 'down' : '' };
  return { value: `${rf.wins}${EN}${rf.losses}`, margin, scored: num(rf.pts_for_pg) ? one(rf.pts_for_pg) : DASH, allowed: num(rf.pts_against_pg) ? one(rf.pts_against_pg) : DASH };
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

/** The Career card: nine tiles, each "#3 · 125.9 / G" when the rank (backend §6) and a game count are known. */
export function careerTiles(career, rank, games) {
  const c = career || {};
  const tiles = TILES.map(([key, label, get, perGame, dec]) => {
    const v = get(c);
    return { key, label, value: num(v) ? (dec ? one(v) : fmtInt(v)) : DASH, rank: num(rank?.[key]) ? `#${rank[key]}` : null,
      perGame: perGame && num(v) && num(games) && games > 0 ? `${one(v / games)} / G` : null };
  });
  const pool = num(rank?.pool) && tiles.some((t) => t.rank) ? rank.pool : null;
  return { tiles, footnote: pool ? `Rank among ${fmtInt(pool)} online players` : null };
}

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
