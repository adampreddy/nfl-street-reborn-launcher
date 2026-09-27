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

// ---- the native game view's snapshot (game.js), kept here so it is testable under node ----

/**
 * The header's seats and sides from a finished game's participants: for each side, the first human with a player
 * account on that side (`team_index`) becomes a seat `{ display_name, avatar_url }` and `sides[side] = { seat }`.
 * A side with no such human (CPU, a guest, an unattributed seat) has `{ seat: null }`. No attributed human at all:
 * no seats and no sides, the renderers' "sides unknown" layout. Mirrors the backend's `participantSeats`
 * (reborn-backend src/web/live-render.js).
 */
export function participantSeats(participants) {
  const list = Array.isArray(participants) ? participants : [];
  const seats = [];
  const sides = [0, 1].map((side) => {
    const p = list.find((x) => x && x.kind === 'human' && x.player_id && x.team_index === side);
    if (!p) return { seat: null };
    seats.push({ display_name: typeof p.display_name === 'string' ? p.display_name : '', avatar_url: typeof p.avatar_url === 'string' ? p.avatar_url : null });
    return { seat: seats.length - 1 };
  });
  return seats.length ? { seats, sides } : { seats: [], sides: [] };
}

/**
 * The snapshot shape the live renderers read, from one finished game's record (`stats_game`). Mirrors the backend's
 * `gameSnapshot` (reborn-backend src/web/live-render.js), which draws the same game on the web: team identity from
 * the plays' own `teams`, logo and colours from `game.teams_view` when the server sends it (an older server does
 * not: names only), the capture's scores, and seats/sides from `game.participants`. Image URLs are only carried
 * here; the renderers pass them through assetUrl()/avatarSrc() before any <img> gets one.
 */
export function snapshotOf(record) {
  const g = record?.game ?? {};
  const plays = Array.isArray(record?.plays) ? record.plays : [];
  const fromPlays = plays.find((p) => Array.isArray(p?.body?.teams) && p.body.teams.length === 2)?.body.teams;
  const view = Array.isArray(g.teams_view) ? g.teams_view : [];
  const teams = [0, 1].map((side) => {
    const tv = view[side] && typeof view[side] === 'object' ? view[side] : {};
    return {
      side,
      tgid: Number.isInteger(fromPlays?.[side]?.tgid) ? fromPlays[side].tgid : Number.isInteger(tv.tgid) ? tv.tgid : null,
      tmc: fromPlays?.[side]?.tmc ?? null,
      name: g.teams?.[side]?.name ?? tv.name ?? `Side ${side + 1}`,
      abbrev: g.teams?.[side]?.abbrev ?? tv.abbrev ?? null,
      primary: tv.primary ?? null,
      secondary: tv.secondary ?? null,
      logo_url: tv.logo_url ?? null,
    };
  });
  const score = [0, 1].map((i) => (Number.isFinite(g.teams?.[i]?.score) ? g.teams[i].score : 0));
  const { seats, sides } = participantSeats(g.participants);
  return {
    match: { status: 'complete', best_of: 1 },
    seats,
    games: [{ game_seq: 1, game_id: g.id, score, teams, sides, plays,
      box: { provisional: false, lines: Array.isArray(record?.lines) ? record.lines : [] },
      drives: Array.isArray(record?.drives) ? record.drives : [] }],
  };
}

/** Whether the game view offers "Open on web": only an online game, the one kind the public page always shows
 *  (an offline game's page 404s for anyone but its players, and the browser is not signed in as them). */
export function webLinkShown(game) {
  return Boolean(game) && game.mode === 'online';
}
