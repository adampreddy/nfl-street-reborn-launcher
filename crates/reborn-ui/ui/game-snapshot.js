// A game that is not part of a matchmade match (an offline game, or an online one with no match id) opens on the same
// page as a matchmade match (owner 10-03: "it opens the wrong style box score page"): match.html, the site's
// /live/<match> page run by its vendored controller. This builds the snapshot that page reads (the shape of the
// backend's GET /v1/live/<match>) from the games' `stats_game` records, so a player's own offline game opens too.
//   - each game through the site's own gameSnapshot (site/live-render.js), in play SIDE order, its plays given their
//     field and running score as the site's game page does (fieldFromBody, rescore), the capture's final score kept;
//   - full team names: the server's teams_view city beside the name, as the live snapshot's teams carry it;
//   - one set of seats for the whole match (by player id), so a player keeps their side of the strip in every game;
//   - the match: complete, casual, best of the number of games (G1..Gn in the line score, no series banner for one
//     game), never `single` (the site's /games/<id> layout this replaces).
// Only a game the server verified gets its plays, box lines and drives; the others are totals only (`totalsOnly`).
import { fieldFromBody, gameSnapshot, rescore, sideTeams } from './site/live-render.js';

const list = (v) => (Array.isArray(v) ? v : []);
const str = (v) => (typeof v === 'string' && v ? v : null);

/**
 * @param {string} id  the snapshot's match id (the game opened)
 * @param {Array<Object|null>} records  each game's `stats_game` answer ({ game, plays, lines, drives }), oldest first;
 *   null (or a record with no game) for a game that could not be read: its column stays "-"
 * @returns {{ snap: Object, totalsOnly: Array<number> }}  totalsOnly: the game numbers with no verified plays
 */
export function matchSnapshot(id, records) {
  const seats = [], who = [];
  const seatOf = (p) => {
    let i = who.indexOf(p.player_id);
    if (i < 0) {
      if (seats.length >= 2) return null;
      who.push(p.player_id);
      seats.push({ display_name: typeof p.display_name === 'string' ? p.display_name : '', avatar_url: str(p.avatar_url), player_id: p.player_id });
      i = seats.length - 1;
    }
    return i;
  };
  const games = [], totalsOnly = [];
  list(records).forEach((record, i) => {
    const game = record && typeof record === 'object' ? record.game : null;
    if (!game || typeof game !== 'object' || !game.id) return;
    // The server's own verdict, not whether any rows came back: only a verified game's plays are trusted.
    const ok = game.plays_status === 'verified';
    const g = gameSnapshot(game, ok ? list(record.plays) : [], ok ? list(record.lines) : [], ok ? list(record.drives) : []).games[0];
    // The /plays route sends { body, situation } only: every play its field and running score_after (the server's
    // rule); the strip keeps the capture's own final score.
    for (const p of g.plays) p.field ??= fieldFromBody(p.body, p.situation);
    const captured = g.score;
    rescore(g);
    if (Array.isArray(captured)) g.score = captured;
    const view = list(game.teams_view);
    g.teams = g.teams.map((t, side) => ({ ...t, city: str(view[side]?.city), wordmark_url: str(view[side]?.wordmark_url) }));
    // Each side's first human with an account takes the match seat that player holds (participants are capture order).
    const { team } = sideTeams(game, view);
    const people = list(game.participants);
    g.sides = [0, 1].map((side) => {
      const p = people.find((x) => x && x.kind === 'human' && x.player_id && x.team_index === team[side]);
      return { seat: p ? seatOf(p) : null };
    });
    g.game_seq = i + 1;
    if (!ok) totalsOnly.push(i + 1);
    games.push(g);
  });
  // Nobody attributed in any game: the renderer's "sides unknown" layout (no seats, no sides).
  if (!seats.length) for (const g of games) g.sides = [];
  return { snap: { ok: true, match: { id, mode: 'casual', status: 'complete', best_of: Math.max(1, list(records).length) }, seats, games }, totalsOnly };
}
