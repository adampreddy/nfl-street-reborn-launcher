// Pure helpers for the Online page (spec docs/superpowers/specs/2026-10-02-online-page-redesign-design.md §3–§4, §7):
// the words and numbers online-mm.js and leaderboard.js draw. No DOM, no commands. Every value comes from an answer;
// a missing one is null (or "") and the page hides its spot. Nothing here invents a number.

/** A finite number, or null. */
export const int = (n) => (typeof n === 'number' && Number.isFinite(n) ? n : null);
export const fmtInt = (n) => n.toLocaleString('en-US');

export function clock(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

// "first to two wins" for a best of 3, "first to three wins" for a best of 5. A best of anything else (an older
// server's open-ended casual match, 20 games) is no series to name.
const FIRST_TO = { 2: 'two', 3: 'three' };

/** Under the VS and in a room. Never "best of 3" (spec §8, DESIGN.md §9). Casual names its series too when the server
 *  says it is one (`bestOf` > 1; casual is a best of 3 too, Adam 10-04). */
export function modeLine(mode, bestOf) {
  if (mode === 'ranked') return 'Ranked · first to two wins';
  const words = int(bestOf) > 1 ? FIRST_TO[Math.ceil(bestOf / 2)] : null;
  return words ? `Casual · first to ${words} wins` : 'Casual';
}

/** "Game 2 · Series 1–0" while a series runs (best_of > 1), counted from this seat; null otherwise. */
export function seriesLine(m) {
  if (!m || !(int(m.best_of) > 1) || !Array.isArray(m.series)) return null;
  const seat = m.seat === 1 ? 1 : 0;
  const mine = int(m.series[seat]) ?? 0, theirs = int(m.series[1 - seat]) ?? 0;
  return `Game ${mine + theirs + 1} · Series ${mine}–${theirs}`;
}

/** "Accept within 0:14" from the server's relay_deadline; null when there is none, it does not read as a time or it
 *  has come (the spot hides; online-mm.js's 1 s ticker counts it down). */
export function acceptWithin(deadline, nowMs) {
  const t = typeof deadline === 'string' ? Date.parse(deadline) : NaN;
  return Number.isFinite(t) && t > nowMs ? `${clock(t - nowMs)} to accept` : null;
}

/** The Ranked tile's numbers, from boardStanding()'s `elo`. With no rating (Elo off) nothing shows at all. */
export function eloBits(elo) {
  const rating = int(elo?.rating), rank = int(elo?.rank), week = int(elo?.week_delta);
  if (rating === null) return { rank: null, elo: null, week: null };
  return {
    rank: rank !== null && rank > 0 ? `#${rank}` : null,
    elo: `${fmtInt(rating)} Elo`,
    week: week ? { dir: week > 0 ? 'up' : 'down', text: `${week > 0 ? '▲' : '▼'} ${Math.abs(week)} this week` } : null,
  };
}

/** "#9 · 1,162 Elo" under a face on the matchup card, from whatever exists. */
export function standing(rank, elo) {
  const r = int(rank), e = int(elo);
  return [r !== null && r > 0 ? `#${r}` : null, e !== null ? `${fmtInt(e)} Elo` : null].filter(Boolean).join(' · ');
}

/** The pill on the picked tile while searching (spec §3.2). */
export function seekLine(mode, elo) {
  if (mode !== 'ranked') return 'Finding a casual game';
  const r = int(elo?.rating);
  return r !== null ? `Finding players near ${fmtInt(r)} Elo` : 'Looking for a ranked game';
}

/** The held notice (spec §3.8): today's text per hold_reason; the generic one is §8's new sentence. */
export function heldNotice(m) {
  const opp = typeof m?.opponent === 'string' && m.opponent ? m.opponent : 'your opponent';
  const text = m?.hold_reason === 'content-stale'
    ? 'A game update never reached one of your game discs, so that game was not started. Both of you: Settings → Repair game files (the one that is current is told so), then find a game again. Finding a game now also repairs it first.'
    : m?.hold_reason === 'content-mismatch'
      ? `Your game files differ from ${opp}'s, so that game was not started. Both of you: Settings → Check game content, install what it offers, then find a game again.`
      : 'Nothing changed on the leaderboard. Someone will look at it. You can play again now.';
  return { title: 'Your last match is held for review', text };
}

/** Start's enabled state: two or more players in the room, every one ready (spec §3.7). */
export function everyoneReady(room) {
  const ms = Array.isArray(room?.members) ? room.members : [];
  return ms.length >= 2 && ms.every((x) => x && x.ready === true);
}

/** The link "Copy invite link" copies (spec §3.10): a web page, so it pastes as a clickable link. */
export function inviteLink(code) {
  return typeof code === 'string' && code ? `https://nflstreetreborn.com/join.php?r=${encodeURIComponent(code)}` : null;
}

/** The invite line above the tiles; `who` is drawn bold. No code, no line. */
export function inviteLine(host, code) {
  if (typeof code !== 'string' || !code) return null;
  return typeof host === 'string' && host
    ? { who: host, rest: ` wants you in the room · ${code}` }
    : { who: null, rest: `You've got a room invite · ${code}` };
}

function change(d) {
  const n = int(d);
  return n ? { dir: n > 0 ? 'up' : 'down', text: `${n > 0 ? '▲' : '▼'} ${Math.abs(n)}` } : null;
}
function lbRow(r) {
  const wins = int(r?.wins), losses = int(r?.losses), elo = int(r?.elo);
  return {
    rank: int(r?.rank), name: typeof r?.display_name === 'string' ? r.display_name : '', avatar: typeof r?.avatar_url === 'string' ? r.avatar_url : null,
    team: r?.team && typeof r.team === 'object' ? r.team : null,
    record: wins !== null && losses !== null ? `${wins}-${losses}` : null,
    elo: elo !== null ? fmtInt(elo) : null, change: change(r?.delta_week), you: r?.you === true,
  };
}

/** The Leaderboard card (spec §4, §7): "Rankings start soon." (soon) while there is no leaderboard, Elo is off or nobody
 *  is ranked; otherwise the top 10 and, when you are further down, your own row. */
export function leaderboardView(answer) {
  if (!answer || answer.enabled === false || !Array.isArray(answer.rows) || !answer.rows.length) return { soon: true };
  const youRank = int(answer.you?.rank);
  const rows = answer.rows.slice(0, 10).map(lbRow);
  // Your own row goes after a gap only when you are not on the card already: by the server's `you` flag, or (a server
  // that leaves the flag off) by your rank being one of the ranks shown.
  const shown = rows.some((r) => r.you) || (youRank !== null && rows.some((r) => r.rank === youRank));
  const you = answer.you && youRank !== null && !shown ? { ...lbRow(answer.you), you: true } : null;
  return { soon: false, total: int(answer.total), rows, you };
}

/** Your standing from the leaderboard answer (spec §4, §7): your flagged row on the card, or the `you` row the server
 *  pins after it, as `elo` in the shape eloBits() and seekLine() read ({rating, rank, week_delta}) plus that row's
 *  plays-as `team`. Null when the answer has no row of yours: no leaderboard, Elo off, signed out or not ranked. */
export function boardStanding(answer) {
  if (!answer || answer.enabled === false || !Array.isArray(answer.rows)) return null;
  const r = answer.rows.find((x) => x?.you === true) ?? (answer.you && typeof answer.you === 'object' ? answer.you : null);
  if (!r) return null;
  return { elo: { rating: int(r.elo), rank: int(r.rank), week_delta: int(r.delta_week) }, team: r.team && typeof r.team === 'object' ? r.team : null };
}
