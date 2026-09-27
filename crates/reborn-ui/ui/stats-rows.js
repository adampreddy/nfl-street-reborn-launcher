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
