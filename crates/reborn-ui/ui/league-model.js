// The League tab's words and rows (spec 2026-10-08 v2 §3-4; data = Plan 1's site contract + Plan 2's room/live fields;
// edition state = Plan 3b's league_edition_status), pure: no DOM, no calls. league.js draws what this returns.
// The site stores no kickoff time (`when` is always null), so nothing here prints a time. Series and scores arrive as
// {home, away}; `matchup.home` turns them into you/opp. `winner_team_id` is the series LEADER until status is final.

const str = (v) => (typeof v === 'string' ? v : '');
const num = (v) => (Number.isFinite(v) ? v : null);
const arr = (v) => (Array.isArray(v) ? v : []);
const HEX = /^#[0-9A-Fa-f]{6}$/;

export function ordinal(n) {
  const v = n % 100;
  if (v >= 11 && v <= 13) return `${n}th`;
  return `${n}${({ 1: 'st', 2: 'nd', 3: 'rd' })[n % 10] || 'th'}`;
}

/** The only team colour that may reach a `--tc` property: an exact #RRGGBB. */
export function teamColor(team) {
  return typeof team?.color === 'string' && HEX.test(team.color) ? team.color : null;
}

export function pageMode(home) {
  if (home && home.signed_in === false) return 'signed_out';
  return home && home.member === true ? 'member' : 'redirect';
}

export function headerLine(home) {
  const parts = [];
  if (num(home?.league?.season) !== null) parts.push(`Season ${home.league.season}`);
  const week = str(home?.league?.week_label) || (num(home?.league?.week) !== null ? `Week ${home.league.week}` : '');
  if (week) parts.push(week);
  if (str(home?.team?.record)) parts.push(home.team.record);
  if (num(home?.team?.standing) !== null) parts.push(ordinal(home.team.standing));
  return parts.join(' · ');
}

export function bannerLine(home) {
  return home?.stale === true ? 'League data unavailable. Showing what was last loaded.' : null;
}

function oppName(m) {
  return str(m?.opponent?.owner_display) || str(m?.opponent?.name) || 'your opponent';
}

/** {home, away} as [you, opp] from this team's side, or null. */
function mine(pair, home) {
  if (!pair || num(pair.home) === null || num(pair.away) === null) return null;
  return home ? [pair.home, pair.away] : [pair.away, pair.home];
}

function standing(you, opp) {
  if (you > opp) return `You lead ${you}-${opp}`;
  if (you < opp) return `You trail ${you}-${opp}`;
  return `Tied ${you}-${opp}`;
}

function finalLine(m, teamId) {
  const decided = m.winner_team_id !== null && m.winner_team_id !== undefined;
  const won = decided && m.winner_team_id === teamId;
  if (m.forfeit === true) return decided ? `Final · ${won ? 'Won' : 'Lost'} by forfeit` : 'Final · Forfeit';
  if (!decided) return 'Final';
  const s = mine(m.series, m.home);
  return `Final · ${won ? 'Won' : 'Lost'} the series${s ? ` ${s[0]}-${s[1]}` : ''}`;
}

const join = (...parts) => parts.filter(Boolean).join(' · ');

const STALE_ROOM = 'Room status comes back when the league does';

/** The This Week card. A room only counts when it has a code to join. `stale`: the data is the last good answer, not
 *  the server's current one (a cached fallback, or a refresh that failed): no room button and no "online" claim is
 *  built from it, since the room may be gone and Join/Cancel would act on old state. Play still asks the server. */
export function cardState(home, { stale = false } = {}) {
  const m = home?.matchup;
  const base = { line: '', action: null, cancel: false, room: null };
  if (home?.schedule_published === false) return { ...base, kind: 'unscheduled', line: 'Schedule not published yet' };
  if (!m || m.status === 'bye' || !m.opponent) return { ...base, kind: 'bye', line: 'No game this week' };
  if (m.status === 'final') return { ...base, kind: 'final', line: finalLine(m, home?.team?.id) };
  const s = mine(m.series, m.home);
  const who = oppName(m);
  const room = m.room && str(m.room.code) ? m.room : null;
  const live = m.status === 'live' || m.room?.state === 'in_series';
  const series = (m.status === 'in_progress' || m.status === 'live') && s ? `Series · ${standing(...s)}` : '';
  if (stale && (live || room)) return { ...base, kind: 'stale', line: join(series, STALE_ROOM) };
  if (live) return { ...base, kind: 'live', line: s ? `LIVE · ${standing(...s)}` : 'LIVE', action: 'Open room', room: m.room || null };
  // Plan 2 sends no `you_host`: the opponent hosts exactly when the room's host is the opponent's Discord ID.
  if (room) {
    const oppId = str(m.opponent.discord_user_id);
    const theirs = oppId !== '' && room.host_discord_user_id === oppId;
    const lobby = room.state === 'lobby';
    if (theirs) return lobby ? { ...base, kind: 'lobby', line: join(series, `You're in @${who}'s room`), action: 'Open room', room }
      : { ...base, kind: 'join', line: join(series, `@${who} is waiting`), action: `Join @${who}'s game`, room };
    return { ...base, kind: lobby ? 'lobby' : 'waiting', line: join(series, lobby ? `@${who} joined` : `Waiting for @${who}…`), action: 'Open room', cancel: true, room };
  }
  if (!str(m.opponent.discord_user_id)) return { ...base, kind: 'unlinked', line: join(series, "Opponent hasn't linked Discord") };
  // Mid-series the score is the line; otherwise say whether the opponent is online.
  return { ...base, kind: 'play', line: series || (!stale && m.opponent.online === true ? `@${who} is online` : ''), action: 'Play league game' };
}

function gb(bytes) {
  const n = num(bytes);
  return n && n > 0 ? ` (${(n / 1073741824).toFixed(1)} GB)` : '';
}

function pct(p) {
  return num(p?.total) && p.total > 0 && num(p?.done) !== null ? Math.max(0, Math.min(100, Math.round((p.done / p.total) * 100))) : null;
}

/** The header's Install/Update button, or null when the League edition is installed and current. `progress` is the
 *  latest `league-install-progress` payload while an install runs, else null. */
export function installState(status, progress) {
  const busy = (label, percent) => ({ label: percent === null ? label : `${label} ${percent}%`, disabled: true, primary: false, percent });
  if (progress && typeof progress === 'object') {
    const p = pct(progress);
    // The first verify (`checking`, before anything downloads) looks at the files already installed.
    if (progress.phase === 'verify') return busy(progress.checking === true ? 'Checking your league files…' : 'Checking downloaded files…', null);
    if (progress.phase === 'compose') return busy('Setting up textures…', p);
    return busy('Downloading league files…', p);
  }
  if (!status || typeof status !== 'object') return { label: 'Checking league files…', disabled: true, primary: false, percent: null };
  if (status.installed !== true) return { label: `Install league files${gb(status.size_bytes)}`, disabled: false, primary: true, percent: null };
  if (status.update_available === true) return { label: `Update league files${gb(status.size_bytes)}`, disabled: false, primary: true, percent: null };
  return null;
}

/** The page's edition state after a poll: the local answer (`league_edition_installed`: install state, no network)
 *  over the last release check (`league_edition_status`), whose `latest` and size it keeps. The update offer is worked
 *  out again, so a build installed meanwhile reads as current. */
export function withInstalled(full, local) {
  if (!local || typeof local !== 'object') return full;
  if (!full || typeof full !== 'object') return local;
  const installed = local.installed === true;
  const version = installed ? local.version : null;
  return { ...full, installed, version, installing: local.installing === true, progress: local.progress ?? null,
    update_available: installed && typeof full.latest === 'string' && full.latest !== version };
}

/** An install the core is already running (`installing: true` on either status answer), as the progress to draw:
 *  its last `league-install-progress` payload, or the first check when none was sent yet. Null when none runs. */
export function runningInstall(status) {
  if (status?.installing !== true) return null;
  const p = status.progress;
  return p && typeof p === 'object' && typeof p.phase === 'string' ? p : { phase: 'verify', done: 0, total: 1, checking: true };
}

/** Whether the three play buttons work: only with the League edition installed, current and not mid-install. */
export function playGate(status, progress) {
  if (progress && typeof progress === 'object') return { ready: false, hint: 'League files are installing…' };
  if (!status || typeof status !== 'object') return { ready: false, hint: 'Checking league files…' };
  if (status.installed !== true) return { ready: false, hint: 'Install league files to play.' };
  if (status.update_available === true) return { ready: false, hint: 'Update league files to play.' };
  return { ready: true, hint: null };
}

/** A room refused because the two players' league files differ (Plan 2 `league_version_mismatch`, mapped by 3b to
 *  kind `league_update_required`; the existing join path may pass the raw code through as the message). */
export function isUpdateRequired(e) {
  if (typeof e === 'string') return e === 'league_version_mismatch';
  if (!e || typeof e !== 'object') return false;
  return e.kind === 'league_update_required' || e.message === 'league_version_mismatch';
}

/** Dotted versions ("2026.10.08") compared segment by segment as numbers; <0, 0, >0. */
function cmpVersion(a, b) {
  const pa = a.split('.').map((x) => parseInt(x, 10) || 0);
  const pb = b.split('.').map((x) => parseInt(x, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i += 1) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d !== 0) return d;
  }
  return 0;
}

/** The sentence for a league version refusal, or null when `e` is not one. The room's `required` version being OLDER
 *  than ours means the opponent is behind (contracts Ruling after Plan 2 final review); otherwise ours are. */
export function updateRequiredLine(e, status) {
  if (!isUpdateRequired(e)) return null;
  const required = str(e?.required);
  const installed = status?.installed === true ? str(status.version) : '';
  if (required && installed && cmpVersion(required, installed) < 0) return 'Your opponent needs to update their league files.';
  return 'Update league files to play.';
}

export function standingsRows(home) {
  const me = home?.team?.id;
  return arr(home?.standings).filter(Boolean).map((r) => ({
    rank: num(r.rank), team_id: num(r.team_id), name: str(r.name), abbrev: str(r.abbr), record: str(r.record), mine: r.team_id === me,
  }));
}


// ---- League page v3, layout F (spec 2026-10-17-league-page-v3-design.md) ----

const SITE = 'https://nflstreetreborn.com/league/';
const posInt = (v) => (Number.isSafeInteger(v) && v > 0 ? v : null);

/** "Reborn Season 4 · Week 3": the league's own name and the week. */
export function seasonLine(home) {
  const week = str(home?.league?.week_label) || (num(home?.league?.week) !== null ? `Week ${home.league.week}` : '');
  return [str(home?.league?.name), week].filter(Boolean).join(' · ');
}

/** The team card's "2-0 · 2nd". */
export function teamCardLine(home) {
  const parts = [str(home?.team?.record)];
  if (num(home?.team?.standing) !== null) parts.push(ordinal(home.team.standing));
  return parts.filter(Boolean).join(' · ');
}

/** "you lead 1-0" / "you trail 0-1" / "tied 1-1", from [you, opp]. */
function seriesWords([you, opp]) {
  if (you > opp) return `you lead ${you}-${opp}`;
  if (you < opp) return `you trail ${you}-${opp}`;
  return `tied ${you}-${opp}`;
}

/** The matchup banner: what sits between the teams (`mid`), the numbers beside them (`scores`, you first), one line
 *  (`text`, `tone` '' | 'ok' | 'bad') and the buttons on the right (`actions`). `edition` / `progress` are the League
 *  files' state: while they are missing, behind or installing, the install button takes the action slot, except when
 *  a game already runs from the installed files (live, or a room already open). `needsUpdate`: a room refused our
 *  league files (league_version_mismatch) for a newer build than the release check knows. */
export function bannerState(home, opts = {}) {
  const s = bannerStateFrom(home, opts);
  // Your fixture's room is open (the 1 s snapshot says so) but you stepped back to the home: Open room leads, whatever
  // the 60 s-old league answer still says (final review 10-21: the waiting host had only Cancel).
  if (opts.roomOpen === true && !s.actions.some((x) => x.key === 'open' || x.key === 'back')) {
    return { ...s, actions: [{ key: 'open', label: 'Open room', primary: true, disabled: false }, ...s.actions.map((x) => ({ ...x, primary: false }))] };
  }
  return s;
}

function bannerStateFrom(home, { stale = false, edition = null, progress = null, needsUpdate = false } = {}) {
  const m = home?.matchup;
  const c = cardState(home, { stale });
  const owner = oppName(m);
  const lw = num(home?.league?.week);
  const mw = num(m?.week);
  const vs = mw !== null && lw !== null && mw < lw ? `Week ${mw} game` : 'VS';
  const series = mine(m?.series, m?.home);
  const off = !playGate(edition, progress).ready;
  const offline = { key: 'offline', label: 'Play offline', primary: false, disabled: off };
  const friendly = { key: 'friendly', label: 'Friendly', primary: false, disabled: off };
  const play = (disabled) => ({ key: 'play', label: `Play ${str(m?.opponent?.name) || owner}`, primary: true, disabled });
  const out = (kind, { mid = vs, scores = null, text = '', tone = '', actions = [] } = {}) => ({ kind, mid, scores, text, tone, actions });
  let s;
  switch (c.kind) {
    case 'unscheduled': s = out('unscheduled', { mid: null, text: 'No schedule yet', actions: [offline, friendly] }); break;
    case 'bye': s = out('bye', { mid: null, text: 'No game this week', actions: [offline, friendly] }); break;
    case 'final': {
      const decided = m.winner_team_id !== null && m.winner_team_id !== undefined;
      const won = decided && m.winner_team_id === home?.team?.id;
      const text = !decided ? 'Final' : m.forfeit === true ? `You ${won ? 'won' : 'lost'} by forfeit`
        : `You ${won ? 'won' : 'lost'} the series${series ? ` ${series[0]}-${series[1]}` : ''}`;
      s = out('final', { mid: 'FINAL', scores: m.forfeit === true ? null : series, text, tone: decided ? (won ? 'ok' : 'bad') : '', actions: [offline, friendly] });
      break;
    }
    case 'stale': s = out('stale', { scores: series, actions: [offline, friendly] }); break;
    case 'live': {
      const n = series ? series[0] + series[1] + 1 : null;
      s = out('live', { mid: 'LIVE', scores: mine(m.score, m.home), text: series ? `Game ${n} of 3 · ${seriesWords(series)}` : '',
        actions: [{ key: 'back', label: 'Back to the game', primary: true, disabled: false }] });
      break;
    }
    case 'join': s = out('join', { text: `${owner} opened the room`, actions: [{ key: 'join', label: 'Join', primary: true, disabled: off }] }); break;
    case 'lobby': s = out('lobby', { text: `${owner} is in the room`, actions: [{ key: 'open', label: 'Open room', primary: true, disabled: false }] }); break;
    case 'waiting': s = out('waiting', { text: `Waiting for ${owner} to join`, actions: [{ key: 'cancel', label: 'Cancel', primary: false, disabled: false }] }); break;
    case 'unlinked': s = out('unlinked', { text: `${owner} hasn't linked Discord on the league site`, actions: [offline, friendly, play(true)] }); break;
    default: {
      const between = (m?.status === 'in_progress' || m?.status === 'live') && series;
      s = out('play', { scores: between ? series : null, text: between ? `Game ${series[0] + series[1] + 1} next · ${seriesWords(series)}` : '',
        actions: [offline, friendly, play(off)] });
    }
  }
  // A room refused our league files for a build the release check has not seen yet: still offer the update
  // (install fetches it).
  const files = installState(edition, progress) || (needsUpdate ? { label: 'Update league files', disabled: false, percent: null } : null);
  if (files && !['live', 'waiting', 'lobby'].includes(s.kind)) {
    s.actions = [{ key: 'install', label: files.label, primary: true, disabled: files.disabled, percent: files.percent }];
  }
  return s;
}

function gameSide(t, score, winner) {
  return { team_id: num(t?.team_id), name: str(t?.name) || str(t?.abbr), record: str(t?.record), score, winner };
}

/** The Week card: finished games first, then yours, then the rest, each keeping the server's order. `marker` is the
 *  right-hand word: 'You' on yours, 'Live', 'Final', or ''. */
export function weekGames(home) {
  const mineId = home?.matchup?.fixture_id;
  const rows = arr(home?.week_games).filter((g) => g && g.fixture_id !== undefined && g.fixture_id !== null && g.home && g.away);
  const rank = (g) => (g.status === 'final' ? 0 : g.fixture_id === mineId ? 1 : 2);
  return rows.map((g, i) => ({ g, i })).sort((a, b) => rank(a.g) - rank(b.g) || a.i - b.i).map(({ g }) => {
    const status = ['final', 'in_progress', 'live'].includes(g.status) ? g.status : 'scheduled';
    const isMine = g.fixture_id === mineId;
    const decided = status === 'final' && g.winner_team_id !== null && g.winner_team_id !== undefined;
    const pts = (k) => (status === 'scheduled' ? null : num(g.score?.[k]));
    return {
      fixture_id: g.fixture_id, mine: isMine, status,
      marker: isMine ? 'You' : status === 'live' ? 'Live' : status === 'final' ? 'Final' : '',
      away: gameSide(g.away, pts('away'), decided && g.winner_team_id === g.away.team_id),
      home: gameSide(g.home, pts('home'), decided && g.winner_team_id === g.home.team_id),
    };
  });
}

/** "Week N's Top Performers": the site's `week_best` (top 3 per group, the leader marked `top`), or the empty line. */
export function weekBest(home) {
  const wb = home?.week_best;
  const week = num(wb?.week) ?? num(home?.league?.week);
  const title = week !== null ? `Week ${week}'s Top Performers` : 'Top Performers';
  const groups = arr(wb?.groups).filter((g) => g && str(g.label)).map((g) => ({
    label: g.label,
    rows: arr(g.rows).filter(Boolean).slice(0, 3).map((r, i) => ({ poid: num(r.poid), name: str(r.name), team_id: num(r.team_id), line: str(r.line), top: i === 0 })),
  }));
  if (!groups.some((g) => g.rows.length)) return { title, empty: 'No games played yet' };
  return { title, groups };
}

/** A league-site page, built only from numeric ids (never a URL from the answer). Null when an id is not one. */
export function siteUrl(kind, home, id) {
  const lid = posInt(home?.league?.id);
  if (lid === null) return null;
  const q = `league_id=${lid}`;
  const need = (fn) => (posInt(id) === null ? null : fn(id));
  switch (kind) {
    case 'team': return need((v) => `${SITE}team.php?${q}&team_id=${v}`);
    case 'game': return need((v) => `${SITE}stats/box.php?${q}&game_id=${v}`);
    case 'player': return need((v) => `${SITE}stats/player.php?${q}&poid=${v}`);
    case 'standings': return `${SITE}standings.php?${q}`;
    case 'schedule': { const w = posInt(home?.league?.week); return `${SITE}schedule.php?${q}${w === null ? '' : `&week=${w}`}`; }
    case 'leaders': return `${SITE}stats/leaders.php?${q}`;
    default: return null;
  }
}
