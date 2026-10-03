// Answers shaped like reborn-backend's (feat/profile-redesign-backend @7d94a3a, the redesign's new fields included) for
// the Stats page (`stats_dashboard` plus the launcher's own `signed_in`) and a public profile (`player_profile`).
// Sample data only: the numbers are placeholders, like the mockup's.
// oldDashboard() / oldProfile(): the same answers as the DEPLOYED backend (ec5027a) sends them: no header.live,
// recent_form, career_rank, elo total/peak/around or team colours; rushing `yac`; a private room still listed.
export const ME = '0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d';
const P2 = '1b2c3d4e-5f60-4b7c-8d9e-0f1a2b3c4d5e', P3 = '2c3d4e5f-6071-4c8d-9e0f-1a2b3c4d5e6f', P4 = '3d4e5f60-7182-4d9e-8f10-2b3c4d5e6f70';
const avatar = (n) => `https://cdn.discordapp.com/avatars/12345678901234567${n}/0123456789abcdef0123456789abcde${n}.png?size=64`;
const team = (tgid, name, abbrev, primary) => ({ name, abbrev, tgid, logo_url: `/live-assets/teams/${tgid}-128.png`, primary, secondary: '#101010' });
const ATL = team(14, 'Falcons', 'ATL', '#A71930'), PHI = team(13, 'Eagles', 'PHI', '#004C54'), KC = team(9, 'Chiefs', 'KC', '#E31837'), NYG = team(16, 'Giants', 'NYG', '#0B2265');
const PLAYZ = { player_id: P2, display_name: 'Playz', avatar_url: avatar(1) };
const MIKEL = { player_id: P3, display_name: 'Mikel', avatar_url: null };
const JOCK = { player_id: P4, display_name: 'Jock', avatar_url: avatar(2) };
const at = (day, hour = 20, min = 0) => new Date(Date.UTC(2026, 8, day, hour, min)).toISOString();
const clone = (x) => JSON.parse(JSON.stringify(x));
const face = (t) => ({ team: t.name, tgid: t.tgid, logo_url: t.logo_url, primary: t.primary, secondary: t.secondary });

function game(id, when, score, opp, opponent) {
  return { id, at: when, score, result: score[0] > score[1] ? 'win' : 'loss', elo_delta: null, me: ATL, opp, opponent,
    opponent_kind: opponent ? 'human' : 'cpu', game_type: 'normal' };
}
function match(id, games, { queue = 'ranked', mode = 'online', elo = null } = {}) {
  const bo = games.length > 1 ? 3 : 1;
  const won = games.filter((g) => g.result === 'win').length, lost = games.length - won;
  const last = games[games.length - 1];
  const q = mode === 'offline' ? null : queue;
  return { id, match_id: mode === 'online' ? id : null, at: last.at, mode, queue: q, game_type: 'normal', best_of: bo,
    me: last.me, opp: last.opp, opponent: last.opponent, opponent_kind: last.opponent_kind,
    games: games.map((g) => ({ ...g, mode, queue: q })), series: bo > 1 ? [won, lost] : null,
    score: [games.reduce((s, g) => s + g.score[0], 0), games.reduce((s, g) => s + g.score[1], 0)],
    result: bo > 1 ? (won > lost ? 'win' : 'loss') : last.result, elo_delta: elo };
}

/** The owner's whole list, newest first: a ranked best of 3 won 2–1, a casual game, an offline CPU game, a ranked best
 *  of 3 lost 0–2, a private room, then nine casual games (14 matches: two pages of ten). The private room (m-05) stays
 *  here on purpose: Stats must DROP it (ruling P4-3/P4-6), and a public profile never lists it. */
export function allMatches() {
  return clone([
    match('m-01', [game('g-011', at(26, 17, 0), [28, 21], PHI, PLAYZ), game('g-012', at(26, 17, 25), [20, 31], PHI, PLAYZ), game('g-013', at(26, 17, 50), [35, 14], PHI, PLAYZ)], { elo: 16 }),
    match('m-02', [game('g-021', at(25, 21), [42, 35], KC, MIKEL)], { queue: 'casual' }),
    match('g:g-031', [game('g-031', at(24, 19), [49, 7], NYG, null)], { mode: 'offline' }),
    match('m-04', [game('g-041', at(23, 20), [14, 28], PHI, JOCK), game('g-042', at(23, 20, 30), [21, 24], PHI, JOCK)], { elo: -18 }),
    match('m-05', [game('g-051', at(22, 18), [31, 30], KC, MIKEL)], { queue: 'private' }),
    ...Array.from({ length: 9 }, (_, i) => match(`m-1${i}`, [game(`g-1${i}`, at(21 - i), i % 3 ? [24, 17] : [10, 27], i % 2 ? PHI : KC, [PLAYZ, MIKEL, JOCK][i % 3])], { queue: 'casual' })),
  ]);
}
/** What a public profile may list: online, never a private room (backend §1). */
export const publicMatches = () => allMatches().filter((m) => m.mode === 'online' && m.queue !== 'private');

const RATINGS = [1000, 1016, 1004, 1030, 1022, 1048, 1070, 1062, 1091, 1120, 1212, 1168, 1184];
const history = () => RATINGS.map((rating, i) => ({ at: at(8 + i), rating, delta: i ? rating - RATINGS[i - 1] : 0, match_id: `m-h${i}` }));
const around = () => [
  { rank: 5, player_id: '4e5f6071-8293-4e0f-9a1b-3c4d5e6f7081', display_name: 'Kay', avatar_url: avatar(4), rating: 1231, games: 30, wins: 19, losses: 11, is_player: false },
  { rank: 6, player_id: '5f607182-93a4-4f1a-8b2c-4d5e6f708192', display_name: 'Lo', avatar_url: null, rating: 1197, games: 22, wins: 13, losses: 9, is_player: false },
  { rank: 7, player_id: ME, display_name: 'adamknowsball', avatar_url: avatar(3), rating: 1184, games: 13, wins: 8, losses: 5, is_player: true },
  { rank: 8, player_id: P2, display_name: 'Playz', avatar_url: avatar(1), rating: 1170, games: 18, wins: 9, losses: 9, is_player: false },
  { rank: 9, player_id: P4, display_name: 'Jock', avatar_url: avatar(2), rating: 1150, games: 14, wins: 7, losses: 7, is_player: false },
];
const elo = () => ({ rating: 1184, rank: 7, total: 214, games: 13, last_delta: 16, peak: 1212, peak_at: at(18), history: history(), around: around() });
const SUMMARY = { games: 23, wins: 14, losses: 9, win_pct: 0.609, streak: 'W2', pts_per_game: 31.4, yds_per_game: 412 };

function best(lines) {
  const b = {
    passer: { key: 'p-penix', name: 'M. Penix Jr.', ...face(ATL), portrait_url: '/live-assets/portraits/221-192.png', value: 3108, unit: 'pass_yds' },
    rusher: { key: 'p-barkley', name: 'S. Barkley', ...face(PHI), portrait_url: '/live-assets/portraits/198-192.png', value: 1715, unit: 'rush_yds' },
    receiver: { key: 'p-london', name: 'D. London', ...face(ATL), portrait_url: '/live-assets/portraits/216-192.png', value: 1714, unit: 'rec_yds' },
    dl_lb: { key: 'p-bolton', name: 'N. Bolton', ...face(KC), portrait_url: '/live-assets/portraits/128-192.png', value: 96, unit: 'tkl_sck' },
    ballhawk: { key: 'p-terrell', name: 'A.J. Terrell', ...face(ATL), portrait_url: '/live-assets/portraits/224-192.png', value: 7, unit: 'int' },
    most_played: { key: 'p-robinson', name: 'B. Robinson', ...face(ATL), portrait_url: '/live-assets/portraits/223-192.png', value: 23, unit: 'games' },
  };
  if (lines) {
    // the career row's own fields (backend profile/players.js PROJ), as a server that spreads the row would send them
    Object.assign(b.passer, { cmp: 198, att: 331, yds: 3108, td: 31, int: 11, rtg: 108.9, long: 74 });
    Object.assign(b.rusher, { att: 212, yds: 1715, td: 12, avg: 5.6, long: 61 });
    Object.assign(b.receiver, { rec: 98, yds: 1714, td: 14, avg: 14.3, long: 74 });
    Object.assign(b.dl_lb, { tkl: 71, sck: 5, tfl: 9, ff: 2, pbu: 6 });
    // no ff sent: left out of the line
    Object.assign(b.ballhawk, { int: 7, pbu: 14, tkl: 33, td: 1 });
    // the live server (backend cc79302) spreads the whole career row: most played here is a rusher's row
    Object.assign(b.most_played, { games: 23, att: 96, yds: 535, avg: 5.6, td: 5, long: 38, btk: 14, yco: 230 });
  }
  return clone(b);
}

function stripColours(x) {
  if (Array.isArray(x)) x.forEach(stripColours);
  else if (x && typeof x === 'object') { delete x.primary; delete x.secondary; Object.values(x).forEach(stripColours); }
}

/** `stats_dashboard`: the launcher adds `signed_in`. `lines: true` = a server that sends each role's full career row
 *  (backend cc79302); `byQueue: true` = one that sends `by_queue` (ranked and casual, online only); `week: n` = the
 *  Elo's `week_delta`. The default is the shape the deployed server sent before cc79302: value + unit only. */
export function dashboard({ lines = false, byQueue = false, week = null } = {}) {
  const d = { ok: true, signed_in: true,
    you: { player_id: ME, display_name: 'adamknowsball', plays_as: clone(ATL), elo: elo(), summary: { ...SUMMARY } },
    matches: allMatches(),
    recent_form: { results: ['W', 'W', 'L', 'W', 'L', 'W', 'W', 'L', 'W', 'L'], pts_for: 314, pts_against: 250 },
    head_to_head: [], best_players: best(lines) };
  // the server's counts (its whole history, not just the 200 listed): ranked 1-1 and casual 7-3, points from its rows
  if (byQueue) d.by_queue = [{ key: 'ranked', name: 'Ranked', games: 2, wins: 1, losses: 1, win_pct: 0.5, pts_per_game: 23.8 },
    { key: 'casual', name: 'Casual', games: 10, wins: 7, losses: 3, win_pct: 0.7, pts_per_game: 21.6 }];
  if (week !== null) d.you.elo.week_delta = week;
  return d;
}
/** Elo switched off until launch (backend eloSwitch): unranked on an empty board. */
export const ELO_OFF = { rating: null, rank: null, total: 0, games: 0, last_delta: null, peak: null, peak_at: null, history: [], around: [], week_delta: null };
export function oldDashboard() {
  const d = dashboard();
  for (const k of ['total', 'peak', 'peak_at', 'around']) delete d.you.elo[k];
  stripColours(d);
  return d;
}

// The best players' career rows carry the same key as best_players (the backend picks them from these rows).
const KEYS = { 'M. Penix Jr.': 'p-penix', 'S. Barkley': 'p-barkley', 'D. London': 'p-london', 'N. Bolton': 'p-bolton', 'A.J. Terrell': 'p-terrell', 'B. Robinson': 'p-robinson' };
const row = (name, t, cols) => ({ key: KEYS[name] ?? name, name, ...face(t), games: 20, ...cols });
function career() {
  return {
    passing: { rows: [
      row('M. Penix Jr.', ATL, { cmp: 141, att: 238, yds: 2210, td: 31, int: 11, air: 1402, yac: 808, ypa: 9.3, rtg: 110.5, long: 74, sacked: 6 }),
      row('J. Hurts', PHI, { cmp: 44, att: 81, yds: 561, td: 6, int: 4, air: 330, yac: 231, ypa: 6.9, rtg: 80.3, long: 52, sacked: 3 }),
      ...['A', 'B', 'C', 'D', 'E'].map((x, i) => row(`Backup ${x}`, NYG, { cmp: 5 - i, att: 9, yds: 60 - i * 5, td: 0, int: 0, air: 30, yac: 20, ypa: 6.6, rtg: 70.1, long: 18, sacked: 0 })) ],
      total: { key: 'total', name: 'Total', cmp: 212, att: 371, yds: 3021, td: 40, int: 18, air: 1900, yac: 1121, ypa: 8.1, rtg: 104.2, long: 74, sacked: 11 } },
    rushing: { rows: [
      row('S. Barkley', PHI, { att: 212, yds: 1180, avg: 5.6, td: 12, long: 61, btk: 31, yco: 540 }),
      row('B. Robinson', ATL, { att: 96, yds: 535, avg: 5.6, td: 5, long: 38, btk: 14, yco: 230 }) ],
      total: { key: 'total', name: 'Total', att: 308, yds: 1715, avg: 5.6, td: 17, long: 61, btk: 45, yco: 770 } },
    receiving: { rows: [
      row('D. London', ATL, { rec: 98, yds: 1402, avg: 14.3, yac: 512, td: 14, long: 74, drops: 4 }),
      row('K. Pitts', ATL, { rec: 22, yds: 312, avg: 14.2, yac: 101, td: 3, long: 41, drops: 2 }) ],
      total: { key: 'total', name: 'Total', rec: 120, yds: 1714, avg: 14.3, yac: 613, td: 17, long: 74, drops: 6 } },
    defense: { rows: [
      row('N. Bolton', KC, { tkl: 71, tfl: 9, sck: 5, int: 2, pbu: 6, ff: 2, fr: 1, td: 1 }),
      row('A.J. Terrell', ATL, { tkl: 33, tfl: 2, sck: 0, int: 7, pbu: 14, ff: 0, fr: 1, td: 1 }) ],
      total: { key: 'total', name: 'Total', tkl: 104, tfl: 11, sck: 5, int: 9, pbu: 20, ff: 2, fr: 2, td: 2 } },
  };
}
const RECORDS = () => [
  // the server's own labels and order (players.js LINE_RECORDS, then the team records): the page spells "yds" out
  { key: 'pass_yds', label: 'Passing yds', value: 288, game_id: 'g-011', at: at(19), name: 'M. Penix Jr.', ...face(ATL) },
  { key: 'pass_td', label: 'Passing TD', value: 5, game_id: 'g-012', at: at(19), name: 'M. Penix Jr.', ...face(ATL) },
  { key: 'rush_yds', label: 'Rushing yds', value: 171, game_id: 'g-012', at: at(19), name: 'S. Barkley', ...face(PHI) },
  { key: 'rush_td', label: 'Rushing TD', value: 3, game_id: 'g-021', at: at(21), name: 'S. Barkley', ...face(PHI) },
  { key: 'rec', label: 'Receptions', value: 7, game_id: 'g-013', at: at(20), name: 'D. London', ...face(ATL) },
  { key: 'rec_yds', label: 'Receiving yds', value: 164, game_id: 'g-013', at: at(20), name: 'D. London', ...face(ATL) },
  { key: 'rec_td', label: 'Receiving TD', value: 3, game_id: 'g-013', at: at(20), name: 'K. Pitts', ...face(ATL) },
  { key: 'tackles', label: 'Tackles', value: 12, game_id: 'g-021', at: at(21), name: 'N. Bolton', ...face(KC) },
  { key: 'sacks', label: 'Sacks', value: 3, game_id: 'g-021', at: at(21), name: 'N. Bolton', ...face(KC) },
  { key: 'def_int', label: 'Interceptions', value: 2, game_id: 'g-041', at: at(22), name: 'A.J. Terrell', ...face(ATL) },
  { key: 'rush_long', label: 'Longest run', value: 61, game_id: 'g-012', at: at(19), name: 'S. Barkley', ...face(PHI) },
  { key: 'longest_td', label: 'Longest TD', value: 99, game_id: 'g-041', at: at(22), name: null, team: null, tgid: null, logo_url: null },
  { key: 'most_points', label: 'Most points', value: 49, game_id: 'g-031', at: at(24), name: null, team: null, tgid: null, logo_url: null },
  { key: 'biggest_win', label: 'Biggest win', value: 42, game_id: 'g-031', at: at(24), name: null, team: null, tgid: null, logo_url: null },
  { key: 'biggest_comeback', label: 'Biggest comeback', value: 14, game_id: 'g-011', at: at(26), name: null, team: null, tgid: null, logo_url: null },
];

/** `player_profile` for ME. `live: false` = online, but not in a match. */
export function profile({ live = true } = {}) {
  const ms = publicMatches();
  return clone({ ok: true,
    header: { player_id: ME, display_name: 'adamknowsball', avatar_url: avatar(3), joined: at(1), plays_as: ATL, plays_as_auto: true, online: true,
      live: live ? { match_id: 'dc5226af-d3b8-4c0b-89e8-1c53b2972a30', queue: 'ranked', best_of: 3, game_number: 2, series: [1, 0], score: [14, 7],
        me: { abbrev: 'ATL', logo_url: ATL.logo_url, primary: ATL.primary }, opp: { abbrev: 'PHI', logo_url: PHI.logo_url, primary: PHI.primary },
        opponent: MIKEL } : null },
    elo: elo(),
    career_rank: { pass_yds: 3, pass_td: 2, pass_rtg: 5, rush_yds: 4, rec_yds: 6, total_td: 2, tkl: 8, sck: 5, def_int: 3, pool: 214 },
    summary: SUMMARY,
    recent_form: { games: 10, wins: 6, losses: 4, avg_margin: 6.4, pts_for_pg: 31.4, pts_against_pg: 25, last_loss_at: at(25) },
    recent_matches: ms.slice(0, 10), matches_total: ms.length,
    best_players: best(false),
    per_game: { games: 24, yds: 412.3, pass: 251.2, rush: 161.1, first_downs: 8.2, turnovers: 1.4, conv1_pct: 0.62, conv2_pct: 0.41, pts_per_drive: 3.1, three_outs: 1.2, longest_drive: 11, lead_changes: 1.6, biggest_lead: 21, safeties: 1 },
    career: career(),
    combos: [{ passer: { name: 'M. Penix Jr.', ...face(ATL) }, receiver: { name: 'D. London', ...face(ATL) }, rec: 61, yds: 902, td: 9 }],
    pitches: { pitches: 41, pitch_plays: 30, pitch_yds: 388 },
    by_team: [{ key: 'tgid:14', name: 'Falcons', abbrev: 'ATL', tgid: 14, logo_url: ATL.logo_url, primary: ATL.primary, secondary: ATL.secondary, games: 19, wins: 12, losses: 7, win_pct: 0.632, pts_per_game: 32.1 },
      { key: 'pickup', name: 'Pickup', abbrev: null, tgid: null, logo_url: null, primary: null, secondary: null, games: 4, wins: 2, losses: 2, win_pct: 0.5, pts_per_game: 27 }],
    by_queue: [{ key: 'ranked', name: 'Ranked', games: 13, wins: 8, losses: 5, win_pct: 0.615, pts_per_game: 30.2 }, { key: 'casual', name: 'Casual', games: 10, wins: 6, losses: 4, win_pct: 0.6, pts_per_game: 33 }],
    by_game_type: [{ key: 'normal', name: 'Normal', games: 23, wins: 14, losses: 9, win_pct: 0.609, pts_per_game: 31.4 }],
    wins_vs_losses: { wins: { games: 14, pts: 36.2, yds: 455, to: 0.9 }, losses: { games: 9, pts: 23.9, yds: 346, to: 2.1 } },
    head_to_head: [{ ...PLAYZ, wins: 5, losses: 3, pts_for: 212, pts_against: 180, last: { game_id: 'g-013', at: at(26), result: 'win', score: [35, 14], margin: 21 }, biggest_win: { margin: 21 }, biggest_loss: { margin: 11 } }],
    records: RECORDS() });
}
export function oldProfile() {
  const p = profile();
  delete p.header.live; delete p.recent_form; delete p.career_rank;
  for (const k of ['total', 'peak', 'peak_at', 'around']) delete p.elo[k];
  p.by_queue.push({ key: 'private', name: 'Private room', games: 3, wins: 2, losses: 1, win_pct: 0.667, pts_per_game: 30 });
  p.recent_matches = [...p.recent_matches.slice(0, 3), allMatches().find((m) => m.queue === 'private'), ...p.recent_matches.slice(3, 9)];
  p.matches_total += 1;
  for (const r of [...p.career.rushing.rows, p.career.rushing.total]) { r.yac = r.yco; delete r.yco; }
  stripColours(p);
  return p;
}
