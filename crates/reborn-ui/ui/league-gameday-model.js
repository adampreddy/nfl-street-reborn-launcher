// League game day (spec docs/superpowers/specs/2026-10-21-league-game-day-design.md): what the League tab draws while
// you are in your league fixture's room or match. Pure: /v1/league/me's answer + the mm_state snapshot in, plain data out.

export const TAPE_LABELS = ['Points', 'Points allowed', 'Total yards', 'Passing yards', 'Rushing yards', 'Turnovers', 'Sacks', 'Defensive TDs'];
// [team_season key, lower is better]
const TAPE_KEYS = [['pts', false], ['pts_allowed', true], ['yds', false], ['pass_yds', false], ['rush_yds', false], ['turnovers', true], ['sacks', false], ['def_td', false]];
const POSITION_ORDER = ['QB', 'RB', 'WR', 'OL', 'DL', 'LB', 'DB'];

const n = (v) => (Number.isFinite(Number(v)) ? Number(v) : 0);
const fmt = (v) => n(v).toLocaleString('en-US');
const parts = (bits) => bits.filter(([v]) => n(v) > 0).map(([v, unit]) => `${fmt(v)} ${unit}`).join(' · ');

/** A player's season line: the first group he has (passing, rushing, receiving, defence), non-zero parts only. */
export function statLine(season, position) {
  const s = season ?? {};
  const groups = [
    [[s.pass_yds, 'yds'], [s.pass_td, 'TD'], [s.thrown_int, 'INT']],
    [[s.rush_yds, 'yds'], [s.rush_td, 'TD']],
    [[s.rec_yds, 'yds'], [s.rec_td, 'TD']],
    [[s.sack, 'sk'], [s.int, 'INT'], [s.tackle, 'tkl']],
  ];
  for (const g of groups) {
    const line = parts(g);
    if (line) return line;
  }
  return season && position === 'OL' ? 'No stats kept' : '';
}

/** A roster in position order, ties by name; `blank` (Week 1) empties every line. */
export function lineup(roster, { blank }) {
  const rank = (p) => { const i = POSITION_ORDER.indexOf(p); return i < 0 ? POSITION_ORDER.length : i; };
  return (Array.isArray(roster) ? roster : [])
    .map((p) => ({ poid: p.poid, name: String(p.name ?? ''), pos: p.position ?? '', number: p.number ?? null,
      portraitUrl: p.portrait_url ?? null, line: blank ? '' : statLine(p.season, p.position) }))
    .sort((a, b) => rank(a.pos) - rank(b.pos) || a.name.localeCompare(b.name));
}

/** The tale of the tape, or null before either side has played (or when a side's totals are missing). */
export function tape(mine, theirs) {
  if (!mine || !theirs || (n(mine.gp) <= 0 && n(theirs.gp) <= 0)) return null;
  return TAPE_KEYS.map(([key, lowerBetter], i) => {
    const a = n(mine[key]), b = n(theirs[key]);
    const max = Math.max(a, b);
    return { label: TAPE_LABELS[i], mine: a, theirs: b,
      mineBetter: lowerBetter ? a < b : a > b, theirsBetter: lowerBetter ? b < a : b > a,
      mineWidth: max > 0 ? (a / max) * 100 : 0, theirsWidth: max > 0 ? (b / max) * 100 : 0 };
  });
}

/** The game-day view's model, or null when the snapshot is not this fixture's room or match. */
export function gameDay(home, snap) {
  const mu = home?.matchup;
  const fid = mu?.fixture_id;
  if (!home?.member || fid === null || fid === undefined) return null;
  const room = snap?.room?.league?.fixture_id === fid ? snap.room : null;
  const match = snap?.match?.league?.fixture_id === fid ? snap.match : null;
  if (!room && !match) return null;
  const opp = mu.opponent ?? null;
  const blank = n(home.team_season?.gp) <= 0 && n(opp?.team_season?.gp) <= 0;
  const me = { id: home.team?.id, name: home.team?.name ?? '', owner: home.team?.abbr ?? '', record: home.team?.record ?? '0-0',
    color: home.team?.color ?? '', logoUrl: home.team?.logo_url ?? null, lineup: lineup(home.roster, { blank }) };
  const them = opp ? { id: opp.team_id, name: opp.name ?? '', owner: opp.owner_display || opp.abbr || '', record: opp.record ?? '0-0',
    color: opp.color ?? '', logoUrl: opp.logo_url ?? null, lineup: lineup(opp.roster, { blank }) } : null;
  const week = mu.week ?? home.league?.week ?? null;
  const [away, homeName] = mu.home ? [them?.name ?? '', me.name] : [me.name, them?.name ?? ''];
  return { week, title: `Week ${week} · ${away} at ${homeName}`, me, opp: them,
    tape: tape(home.team_season, opp?.team_season), oppMissing: !Array.isArray(opp?.roster),
    stale: home.stale === true, room, match };
}
