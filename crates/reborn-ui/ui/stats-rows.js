// The Stats tab's row model: pure, no DOM, so it is testable under node. Everything here is data the
// server returned (player-typed team names among it); the page draws it with textContent only.
const RESULT = { win: 'W', loss: 'L', tie: 'T' };
// The local results queue's states (main.rs `stats_queue`) that mean "not on the server yet": waiting to be
// sent, or sending keeps failing. Accepted and held are uploaded; quarantined and rejected never will be.
const NOT_UPLOADED = new Set(['queued', 'stalled']);

function team(t, side) {
  return {
    name: t && t.name ? String(t.name) : `Side ${side + 1}`,
    abbrev: t && t.abbrev ? String(t.abbrev) : null,
    tgid: Number.isInteger(t?.tgid) ? t.tgid : null,
    // The server's own asset path, when it sends one; the page still passes it through assetUrl() before drawing.
    logo: t && typeof t.logo_url === 'string' ? t.logo_url : null,
  };
}

export function rowModel(game) {
  const teams = Array.isArray(game.teams) ? game.teams : [];
  const when = new Date(game.captured_at ?? game.first_seen_at ?? NaN);
  const ok = Number.isFinite(when.getTime());
  const score = Array.isArray(game.score) && game.score.length === 2 ? game.score : null;
  const badge = game.status === 'held' ? 'Held for review' : game.plays_status === 'verified' ? 'Verified' : 'Totals only';
  return {
    id: String(game.id ?? ''),
    date: ok ? when.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' }) : 'Date unavailable',
    time: ok ? when.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' }) : '',
    home: team(teams[0], 0), away: team(teams[1], 1),
    score,
    mine: game.team_index === 0 || game.team_index === 1 ? game.team_index : null,
    result: game.status === 'accepted' ? (RESULT[game.result] ?? null) : null,
    badge,
    mode: game.mode === 'online' ? 'Online' : 'Offline',
  };
}

function unsent(entries) {
  return (Array.isArray(entries) ? entries : []).filter((e) => NOT_UPLOADED.has(String(e?.state ?? '')));
}

/** How many local results are not on the server yet (queued or stalled). */
export function unsentCount(entries) {
  return unsent(entries).length;
}

/** "N games not uploaded yet", plus the queue's own reason (the first unsent entry that has one, a stalled one),
    or null when nothing is waiting. The reason is the core's player-facing sentence, drawn as text. */
export function queueLine(entries) {
  const list = unsent(entries);
  if (!list.length) return null;
  const why = list.map((e) => (typeof e?.reason === 'string' ? e.reason.trim() : '')).find(Boolean);
  const line = `${list.length} ${list.length === 1 ? 'game' : 'games'} not uploaded yet`;
  return why ? `${line}: ${why}` : line;
}

// ---- one finished game as a snapshot (the site's gameSnapshot, restated; game.js now uses the vendored original) ----

const bit = (v) => v === 0 || v === 1;

/**
 * Which CAPTURE team each play SIDE is: `{ team: [captureIndexOfSide0, captureIndexOfSide1], key }`. Mirrors the
 * backend's `sideTeams` (reborn-backend src/web/live-render.js); keep the two identical.
 *
 * A finished game carries two orders. `game.teams` (names, scores), `participants[].team_index` and the stored box
 * lines' `team_index` are CAPTURE order: the fork writes teams[] off the Game Stats screen and every team_index is
 * `side_to_team[side]` into it. The plays, drives, `teams_view` and the header are play SIDE order. The two differ
 * whenever the fork's `side_to_team` is not the identity (its measured default is side s -> teams[1 - s]). `key`:
 *   * 'players' -- the capture's own players, each carrying `side` AND `team_index`: the fork's own side_to_team,
 *                  the same map it applied to the seats' team_index. Exact.
 *   * 'names'   -- each side's team name (the capture players' `team` by side, else `teams_view[side].name`) found
 *                  among the capture's team names.
 *   * 'order'   -- nothing links them: capture order is taken as side order, what every builder did before.
 */
export function sideTeams(game, view = null) {
  const pairs = [null, null];
  let clash = false;
  for (const p of Array.isArray(game?.players) ? game.players : []) {
    if (!p || !bit(p.side) || !bit(p.team_index)) continue;
    if (pairs[p.side] === null) pairs[p.side] = p.team_index;
    else if (pairs[p.side] !== p.team_index) clash = true;
  }
  const settle = (t) => {
    if (t[0] === null && t[1] === null) return null;
    const out = [t[0] ?? (t[1] === null ? null : 1 - t[1]), t[1] ?? 1 - t[0]];
    return out[0] !== out[1] ? out : null;
  };
  const byPlayers = clash ? null : settle(pairs);
  if (byPlayers) return { team: byPlayers, key: 'players' };

  const capture = [0, 1].map((i) => (typeof game?.teams?.[i]?.name === 'string' ? game.teams[i].name : null));
  if (capture[0] !== null && capture[1] !== null && capture[0] !== capture[1]) {
    const named = [null, null];
    for (const p of Array.isArray(game?.players) ? game.players : [])
      if (p && bit(p.side) && typeof p.team === 'string' && p.team && named[p.side] === null) named[p.side] = p.team;
    const v = Array.isArray(view) ? view : [];
    const found = [0, 1].map((side) => {
      const name = named[side] ?? (typeof v[side]?.name === 'string' ? v[side].name : null);
      const i = name === null ? -1 : capture.indexOf(name);
      return i === -1 ? null : i;
    });
    const byNames = settle(found);
    if (byNames) return { team: byNames, key: 'names' };
  }
  return { team: [0, 1], key: 'order' };
}

/** A capture-order team_index as a play side under `team` (sideTeams(...).team); null stays null. */
const sideOfTeam = (team, t) => (bit(t) ? team.indexOf(t) : null);

/**
 * The header's seats and sides from a finished game's participants: for each side, the first human with a player
 * account on that side becomes a seat `{ display_name, avatar_url }` and `sides[side] = { seat }`. A participant's
 * `team_index` is CAPTURE order; `team` (sideTeams(...).team, default the identity) says which capture team each
 * side is. A side with no such human (CPU, a guest, an unattributed seat) has `{ seat: null }`. No attributed human
 * at all: no seats and no sides, the renderers' "sides unknown" layout. Mirrors the backend's `participantSeats`
 * (reborn-backend src/web/live-render.js).
 */
export function participantSeats(participants, team = [0, 1]) {
  const list = Array.isArray(participants) ? participants : [];
  const seats = [];
  const sides = [0, 1].map((side) => {
    const p = list.find((x) => x && x.kind === 'human' && x.player_id && bit(x.team_index) && x.team_index === team[side]);
    if (!p) return { seat: null };
    seats.push({ display_name: typeof p.display_name === 'string' ? p.display_name : '', avatar_url: typeof p.avatar_url === 'string' ? p.avatar_url : null });
    return { seat: seats.length - 1 };
  });
  return seats.length ? { seats, sides } : { seats: [], sides: [] };
}

/**
 * The snapshot shape the live renderers read, from one finished game's record (`stats_game`), all in play SIDE
 * order. Mirrors the backend's `gameSnapshot` (reborn-backend src/web/live-render.js), which draws the same game on
 * the web: team identity from the plays' own `teams`, logo and colours from `game.teams_view` when the server sends
 * it (an older server does not: names only), and the capture's names, scores, seats and box lines moved from
 * capture order onto sides by `sideTeams`. Image URLs are only carried here; the renderers pass them through
 * assetUrl()/avatarSrc() before any <img> gets one.
 */
export function snapshotOf(record) {
  const g = record?.game ?? {};
  const plays = Array.isArray(record?.plays) ? record.plays : [];
  const fromPlays = plays.find((p) => Array.isArray(p?.body?.teams) && p.body.teams.length === 2)?.body.teams;
  const view = Array.isArray(g.teams_view) ? g.teams_view : [];
  const { team } = sideTeams(g, view);
  const teams = [0, 1].map((side) => {
    const tv = view[side] && typeof view[side] === 'object' ? view[side] : {};
    const cap = g.teams?.[team[side]];
    return {
      side,
      tgid: Number.isInteger(fromPlays?.[side]?.tgid) ? fromPlays[side].tgid : Number.isInteger(tv.tgid) ? tv.tgid : null,
      tmc: fromPlays?.[side]?.tmc ?? null,
      name: cap?.name ?? tv.name ?? `Side ${side + 1}`,
      abbrev: cap?.abbrev ?? tv.abbrev ?? null,
      primary: tv.primary ?? null,
      secondary: tv.secondary ?? null,
      logo_url: tv.logo_url ?? null,
    };
  });
  const score = [0, 1].map((side) => (Number.isFinite(g.teams?.[team[side]]?.score) ? g.teams[team[side]].score : 0));
  const { seats, sides } = participantSeats(g.participants, team);
  // The stored box lines' team_index is capture order too; the renderers read it as the side.
  const lines = (Array.isArray(record?.lines) ? record.lines : [])
    .map((l) => (l && typeof l === 'object' ? { ...l, team_index: sideOfTeam(team, l.team_index) } : l));
  return {
    match: { status: 'complete', best_of: 1 },
    seats,
    games: [{ game_seq: 1, game_id: g.id, score, teams, sides, plays,
      box: { provisional: false, lines },
      drives: Array.isArray(record?.drives) ? record.drives : [] }],
  };
}

/** Whether the game view offers "Open on web": only an online game, the one kind the public page always shows
 *  (an offline game's page 404s for anyone but its players, and the browser is not signed in as them). */
export function webLinkShown(game) {
  return Boolean(game) && game.mode === 'online';
}
