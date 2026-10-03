/**
 * Pure view models for the stats dashboard and profile pages.
 * No dependencies, vanilla ES modules, formatter utilities for numbers, Elo, records, etc.
 */

// Dash characters (exact Unicode required by tests)
const EM_DASH = '—'; // U+2014 for missing values
const EN_DASH = '–'; // U+2013 for score and series separators
const HYPHEN = '-'; // ASCII hyphen-minus for win-loss records (owner ruling: "14-9", never "14–9")
const MINUS = '−'; // U+2212 for Elo changes

/**
 * Format integer with thousands separator, or em dash for anything that is not a number.
 * @param {number|null} n
 * @returns {string}
 */
export function fmtInt(n) {
  // Anything that is not a number (null, undefined, a string, NaN) is a dash: a missing server field never throws.
  if (typeof n !== 'number' || Number.isNaN(n)) return EM_DASH;
  return n.toLocaleString('en-US');
}

/**
 * Format percentage with exactly 3 decimals, or em dash for null.
 * @param {number|null} p
 * @returns {string}
 */
export function fmtPct(p) {
  if (p === null) return EM_DASH;
  const fixed = p.toFixed(3);
  // Remove leading zero for values < 1 (e.g., "0.630" -> ".630")
  return fixed.startsWith('0') ? fixed.slice(1) : fixed;
}

/**
 * Format Elo rating with value, change indicator, and leaderboard rank.
 * @param {{rating: number|null, last_delta: number, rank: number}|undefined} elo
 * @returns {{value: string, change: string, rank: string}}
 */
export function fmtElo(elo) {
  if (!elo || elo.rating === null) {
    return { value: 'Unranked', change: '', rank: '' };
  }

  const value = fmtInt(elo.rating);
  let change = '';
  if (elo.last_delta > 0) {
    change = `▲ ${elo.last_delta}`;
  } else if (elo.last_delta < 0) {
    change = `▼ ${-elo.last_delta}`;
  }

  const rank = elo.rank ? `#${elo.rank} on the leaderboard` : '';

  return { value, change, rank };
}

/**
 * Convert unit key to display label.
 * @param {string} unit
 * @returns {string}
 */
function unitLabel(unit) {
  const labels = {
    pass_yds: 'passing yards',
    rush_yds: 'rushing yards',
    rec_yds: 'receiving yards',
    tkl_sck: 'tkl + sck',
    int: 'INT',
    games: 'games'
  };
  return labels[unit] || unit;
}

/**
 * Get opponent display name from row data.
 * @param {Object} row
 * @returns {string}
 */
function getOpponentName(row) {
  if (!row.opponent) {
    if (row.opponent_kind === 'cpu') return 'CPU';
    if (row.opponent_kind === 'guest') return 'Guest';
    return '—';
  }
  return row.opponent.display_name || '—';
}

/**
 * Get avatar URL from opponent.
 * @param {Object} row
 * @returns {string}
 */
function getAvatar(row) {
  return row.opponent?.avatar_url || '';
}

/**
 * Format score as "X–Y" or em dash.
 * @param {Array|null} score
 * @returns {string}
 */
function formatScore(score) {
  if (!score || !Array.isArray(score)) return EM_DASH;
  return `${score[0]}${EN_DASH}${score[1]}`;
}

/**
 * The result letter: W or L. Ties do not exist in Reborn (spec §2), so anything else draws nothing.
 * @param {string|null} result
 * @returns {string}
 */
function getResultChar(result) {
  const r = String(result || '').toLowerCase();
  return r === 'win' ? 'W' : r === 'loss' ? 'L' : '';
}

/**
 * Build tag and tagKind for a game row.
 * @param {Object} row
 * @returns {{tag: string, tagKind: string}}
 */
function buildTag(row) {
  if (row.mode === 'offline' || row.opponent_kind === 'cpu') {
    return { tag: 'Offline', tagKind: 'offline' };
  }

  if (row.elo_delta === null || row.elo_delta === undefined) {
    return { tag: '', tagKind: '' };
  }

  if (row.elo_delta > 0) {
    return { tag: `+${row.elo_delta} Elo`, tagKind: 'up' };
  } else if (row.elo_delta < 0) {
    return { tag: `${MINUS}${-row.elo_delta} Elo`, tagKind: 'down' };
  }

  return { tag: '', tagKind: '' };
}

/**
 * Format game row into view model.
 * @param {Object} row
 * @returns {Object}
 */
export function gameRowModel(row) {
  const dateObj = new Date(row.at);
  const date = dateObj.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
  const time = dateObj.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });

  const score = formatScore(row.score);
  const result = getResultChar(row.result);
  const opponent = getOpponentName(row);
  const avatar = getAvatar(row);
  const { tag, tagKind } = buildTag(row);

  return {
    id: row.id,
    date,
    time,
    me: row.me,
    opp: row.opp,
    opponent,
    avatar,
    score,
    result,
    tag,
    tagKind
  };
}

/**
 * Format match row into view model with series and games.
 * @param {Object} match
 * @returns {Object}
 */
export function matchRowModel(match) {
  const dateObj = new Date(match.at);
  const date = dateObj.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
  const time = dateObj.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });

  // Score: series or game score
  let score;
  if (match.series) {
    score = `${match.series[0]}${EN_DASH}${match.series[1]}`;
  } else {
    score = formatScore(match.score);
  }

  const result = getResultChar(match.result);
  const opponent = getOpponentName(match);
  const avatar = getAvatar(match);

  // Elo tag shows only on match, not on games
  const { tag, tagKind } = buildTag(match);

  // Build games with elo_delta: null so Elo shows once on the match
  const games = match.games.map(g => {
    const gameModel = gameRowModel({ ...g, elo_delta: null });
    return gameModel;
  });

  const expandable = games.length > 1;
  const onlyGame = expandable ? null : (games.length === 1 ? games[0].id : null);

  return {
    id: match.id,
    date,
    time,
    me: match.me,
    opp: match.opp,
    opponent,
    avatar,
    score,
    result,
    tag,
    tagKind,
    games,
    expandable,
    onlyGame
  };
}

/**
 * Build role cards array (6 cards in order).
 * @param {{passer, rusher, receiver, dl_lb, ballhawk, most_played}} best
 * @returns {Array}
 */
export function roleCards(best) {
  const roles = [
    { key: 'passer', label: 'Best passer' },
    { key: 'rusher', label: 'Best rusher' },
    { key: 'receiver', label: 'Best receiver' },
    { key: 'dl_lb', label: 'Best DL/LB' },
    { key: 'ballhawk', label: 'Ballhawk' },
    { key: 'most_played', label: 'Most played' }
  ];

  return roles.map(({ key, label }) => {
    const data = best[key];

    if (!data) {
      return {
        role: key,
        label,
        name: 'None yet',
        team: '',
        logo: '',
        portrait: '',
        initial: '',
        stat: '',
        primary: null,
        line: []
      };
    }

    const initial = data.name ? data.name.charAt(0).toUpperCase() : '';
    const stat = `${fmtInt(data.value)} ${unitLabel(data.unit)}`;

    return {
      role: key,
      label,
      name: data.name,
      team: data.team,
      logo: data.logo_url,
      portrait: data.portrait_url,
      initial,
      stat,
      primary: teamColour(data.primary),
      line: statLine(key, data)
    };
  });
}

/**
 * Format stats totals line or empty if no total.
 * @param {{total: Object|null}} table
 * @param {string} kind - 'passing', 'rushing', 'receiving', 'defense'
 * @returns {string}
 */
export function totalLine(table, kind) {
  if (!table.total) return '';

  const t = table.total;
  let line = 'Total ';

  if (kind === 'passing') {
    line += ` ${fmtInt(t.cmp)}/${fmtInt(t.att)} · ${fmtInt(t.yds)} yds · ${fmtInt(t.td)} TD · ${fmtInt(t.int)} INT · ${t.rtg} rtg`;
  } else if (kind === 'rushing') {
    line += ` ${fmtInt(t.att)} att · ${fmtInt(t.yds)} yds · ${t.avg} avg · ${fmtInt(t.td)} TD`;
  } else if (kind === 'receiving') {
    line += ` ${fmtInt(t.rec)} rec · ${fmtInt(t.yds)} yds · ${fmtInt(t.td)} TD`;
  } else if (kind === 'defense') {
    line += ` ${fmtInt(t.tkl)} tkl · ${fmtInt(t.sck)} sck · ${fmtInt(t.int)} INT · ${fmtInt(t.pbu)} PBU`;
  }

  return line;
}

const BACK_PROFILE_RE = /^profile\.html\?player=[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}(#[a-z0-9-]{1,24})?$/;
const BACK_STATS_RE = /^stats\.html(#[a-z0-9-]{1,24})?$/;

/**
 * Where the game view's "← Back" (and B / Esc) goes: the page that opened it (its `from` address parameter), accepted
 * only as 'stats.html' or 'profile.html?player=<lowercase uuid>', each with an optional short #view (the tab the player
 * was on); anything else is Stats.
 * @param {unknown} from
 * @returns {string}
 */
export function backHref(from) {
  return typeof from === 'string' && (BACK_STATS_RE.test(from) || BACK_PROFILE_RE.test(from)) ? from : 'stats.html';
}

const GAME_UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const MAX_GAMES = 25;

/**
 * The address a match row opens (owner 10-03, "the box score is the whole match"). A matchmade match (the server's
 * `match_id`, a lowercase UUID) opens the match page (match.html), the site's /live/<match> page: every game of it, the
 * last one shown, as the site draws it. Anything else (an offline game, an unlinked one) opens the game page
 * (game.html): a match of one game just that game; a match of several ONE page holding every game of it (oldest first,
 * as the server sends them) with its last game shown, and the series from the list's side ("2-1"). `from` is the page
 * Back returns to (backHref). Null when the match has no recorded game.
 * @param {Object} match
 * @param {string} from
 * @returns {string|null}
 */
export function matchHref(match, from) {
  const ids = (Array.isArray(match?.games) ? match.games : []).map((g) => (typeof g?.id === 'string' ? g.id : null)).filter(Boolean);
  if (!ids.length) return null;
  if (typeof match.match_id === 'string' && GAME_UUID_RE.test(match.match_id)) {
    return `match.html?${new URLSearchParams({ match: match.match_id, from: backHref(from) })}`;
  }
  const q = new URLSearchParams({ game: ids[ids.length - 1] });
  if (ids.length > 1) {
    q.set('games', ids.slice(0, MAX_GAMES).join(','));
    const s = match.series;
    if (Array.isArray(s) && Number.isInteger(s[0]) && Number.isInteger(s[1])) q.set('series', `${s[0]}${HYPHEN}${s[1]}`);
  }
  q.set('from', backHref(from));
  return `game.html?${q}`;
}

/** The game page's address for one game (a record's game, say). */
export function gameHref(id, from) {
  return `game.html?${new URLSearchParams({ game: String(id ?? ''), from: backHref(from) })}`;
}

/**
 * The game page's reading of its own address (matchHref): the game shown, the match's games (just that game when there
 * is no list, or the list does not hold it), the series ("2-1" or null) and where Back goes. Only lowercase game UUIDs
 * make the list, at most 25.
 * @param {string} search  location.search
 * @returns {{game: string, games: Array<string>, series: string|null, back: string}}
 */
export function readMatchAddress(search) {
  const q = new URLSearchParams(search || '');
  const game = (q.get('game') || '').toLowerCase();
  const games = [...new Set((q.get('games') || '').toLowerCase().split(',').filter((id) => GAME_UUID_RE.test(id)))].slice(0, MAX_GAMES);
  const series = /^\d{1,2}-\d{1,2}$/.test(q.get('series') || '') ? q.get('series') : null;
  const many = games.length > 1 && games.includes(game);
  return { game, games: many ? games : [game], series: many ? series : null, back: backHref(q.get('from')) };
}

/**
 * The record counts ONLINE matches only (owner 10-03): offline and CPU games and private rooms never count. The
 * headline record, and Ranked and Casual as two separate figures, plus the online results newest first (Last 10, the
 * streak). Built from a match list (newest first) for the Stats page, whose server answer does not split by queue.
 * @param {Array<Object>|null|undefined} matches
 */
export function onlineRecord(matches) {
  const online = (Array.isArray(matches) ? matches : []).filter((m) => m && m.mode === 'online' && m.opponent_kind !== 'cpu' && m.queue !== 'private');
  const tally = (list) => {
    const r = recordOf(list);
    const n = r.wins + r.losses;
    return { ...r, games: n, text: n ? `${r.wins}${HYPHEN}${r.losses}` : EM_DASH, pct: n ? r.wins / n : null };
  };
  const results = resultsOf(online);
  let streak = EM_DASH;
  if (results.length) { let n = 0; while (n < results.length && results[n] === results[0]) n++; streak = `${results[0]}${n}`; }
  return { all: tally(online), ranked: tally(online.filter((m) => m.queue === 'ranked')), casual: tally(online.filter((m) => m.queue === 'casual')), results, streak };
}

/**
 * Generate SVG polyline points string for trend chart.
 * Maps ratings to a box: min at bottom, max at top, x evenly spaced.
 * @param {Array<{rating: number}>} history
 * @param {number} w - Width (default 160)
 * @param {number} h - Height (default 50)
 * @returns {string}
 */
export function trendPoints(history, w = 160, h = 50) {
  if (history.length < 2) return '';

  const ratings = history.map(h => h.rating);
  const min = Math.min(...ratings);
  const max = Math.max(...ratings);
  const range = max - min;

  const points = history.map((item, i) => {
    const x = (i / (history.length - 1)) * w;
    let y;
    if (range === 0) {
      // All ratings equal, y is middle
      y = h / 2;
    } else {
      // Normalize rating: max at the top (y = 0), min at the bottom (y = h)
      y = h - ((item.rating - min) / range) * h;
    }

    // Round y to 1 decimal, drop trailing .0
    let yRounded = Math.round(y * 10) / 10;
    let yStr = yRounded.toString();
    if (yStr.endsWith('.0')) {
      yStr = yStr.slice(0, -2);
    }

    return `${x},${yStr}`;
  });

  return points.join(' ');
}

// ---- the redesign (spec §8.6, §8.7) ----

/**
 * A row's Elo change: "+16" (up), "−18" (down), or nothing (unranked, offline, zero).
 * @param {number|null|undefined} delta
 * @returns {{text: string, kind: string}}
 */
export function eloChange(delta) {
  if (typeof delta !== 'number' || !Number.isFinite(delta) || delta === 0) return { text: '', kind: '' };
  return delta > 0 ? { text: `+${delta}`, kind: 'up' } : { text: `${MINUS}${-delta}`, kind: 'down' };
}

/**
 * The second line under a match row's opponent. Stats: "Offline" for an offline or CPU match, else nothing (private
 * matches are dropped before they get here, see dropPrivate). Profile: the queue, "Ranked" or "Casual". Never
 * "Best of 3" and never any "Private" text.
 * @param {Object} match
 * @param {'stats'|'profile'} where
 * @returns {string}
 */
export function matchSubline(match, where = 'stats') {
  if (where === 'profile') return match?.queue === 'ranked' ? 'Ranked' : match?.queue === 'casual' ? 'Casual' : '';
  if (match?.mode === 'offline' || match?.opponent_kind === 'cpu') return 'Offline';
  return '';
}

/**
 * Stats shows no private matches (owner ruling): drop them from a match list before anything is counted or drawn.
 * @param {Array<Object>|null|undefined} matches
 * @returns {Array<Object>}
 */
export function dropPrivate(matches) {
  return (Array.isArray(matches) ? matches : []).filter((m) => m && m.queue !== 'private');
}

/**
 * Result letters (newest first, W or L) of a match list, for lastTen.
 * @param {Array<Object>|null|undefined} matches
 * @returns {Array<string>}
 */
export function resultsOf(matches) {
  return (Array.isArray(matches) ? matches : []).map((m) => getResultChar(m?.result)).filter(Boolean);
}

/**
 * Wins and losses of a match list.
 * @param {Array<Object>|null|undefined} matches
 * @returns {{wins: number, losses: number}}
 */
export function recordOf(matches) {
  const r = resultsOf(matches);
  const wins = r.filter((x) => x === 'W').length;
  return { wins, losses: r.length - wins };
}

/**
 * "6-4" from the dashboard's recent_form.results (newest first; only W and L count), or an em dash.
 * @param {Array<string>|null|undefined} results
 * @returns {string}
 */
export function lastTen(results) {
  const r = (Array.isArray(results) ? results : []).filter((x) => x === 'W' || x === 'L').slice(0, 10);
  if (!r.length) return EM_DASH;
  const w = r.filter((x) => x === 'W').length;
  return `${w}${HYPHEN}${r.length - w}`;
}

/**
 * The You card's numbers (spec §8.6), ONLINE matches only (owner 10-03; onlineRecord): Record, Win %, Last 10,
 * Streak, then Ranked and Casual as two separate figures, never one blended number. A winning streak is marked 'up'.
 * @param {ReturnType<typeof onlineRecord>} online
 * @returns {{nums: Array<{label: string, value: string, kind: string}>, queues: Array<{label: string, value: string, pct: string}>}}
 */
export function youNumbers(online) {
  const o = online || onlineRecord([]);
  const pct = (t) => (t.pct === null ? EM_DASH : fmtPct(t.pct));
  return {
    nums: [
      { label: 'Record', value: o.all.text, kind: '' },
      { label: 'Win %', value: pct(o.all), kind: '' },
      { label: 'Last 10', value: lastTen(o.results), kind: '' },
      { label: 'Streak', value: o.streak, kind: o.streak.startsWith('W') ? 'up' : '' },
    ],
    queues: [['Ranked', o.ranked], ['Casual', o.casual]].map(([label, t]) => ({ label, value: t.text, pct: pct(t) })),
  };
}

// Each role's statline (spec §8.6 table, fuller per owner 10-03): [field, unit, one decimal?]. The fields are the
// career table row's own (backend profile/players.js PROJ); 'cmp/att' draws "141/238 C/ATT".
const STATLINES = {
  passer: [['cmp/att', 'C/ATT'], ['yds', 'YDS'], ['td', 'TD'], ['int', 'INT'], ['rtg', 'RTG', true]],
  rusher: [['att', 'CAR'], ['yds', 'YDS'], ['avg', 'AVG', true], ['td', 'TD'], ['long', 'LNG']],
  receiver: [['rec', 'REC'], ['yds', 'YDS'], ['avg', 'AVG', true], ['td', 'TD'], ['long', 'LNG']],
  dl_lb: [['tkl', 'TKL'], ['sck', 'SCK'], ['tfl', 'TFL'], ['ff', 'FF'], ['pbu', 'PBU']],
  ballhawk: [['int', 'INT'], ['pbu', 'PBU'], ['tkl', 'TKL'], ['ff', 'FF'], ['td', 'TD']],
};
const UNIT_SHORT = { pass_yds: 'YDS', rush_yds: 'YDS', rec_yds: 'YDS', tkl_sck: 'TKL + SCK', int: 'INT', games: 'GAMES' };

/**
 * A player row's statline: every number of the role's line the server sends (numbers formatted, units uppercase; a
 * field it does not send is left out, never drawn as a dash), when it sends at least two of them; else the one
 * number it always sends (`value` + `unit`), else nothing. Most played keeps its games count first.
 * @param {string} role
 * @param {Object|null} data
 * @returns {Array<{value: string, unit: string}>}
 */
export function statLine(role, data) {
  if (!data) return [];
  const isNum = (n) => typeof n === 'number' && Number.isFinite(n);
  const fmt = (v, dec) => (dec ? v.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 }) : fmtInt(v));
  const spec = role === 'most_played'
    ? [['games', 'GAMES'], isNum(data.att) && !isNum(data.rec) ? ['att', 'CAR'] : ['rec', 'REC'], ['yds', 'YDS'], ['td', 'TD']]
    : STATLINES[role];
  const line = (spec || []).flatMap(([k, unit, dec]) => {
    if (k === 'cmp/att') return isNum(data.cmp) && isNum(data.att) ? [{ value: `${fmtInt(data.cmp)}/${fmtInt(data.att)}`, unit }] : [];
    return isNum(data[k]) ? [{ value: fmt(data[k], dec), unit }] : [];
  });
  if (line.length >= 2) return line;
  if (!isNum(data.value)) return [];
  return [{ value: fmtInt(data.value), unit: UNIT_SHORT[data.unit] || String(data.unit || '').toUpperCase() }];
}

/**
 * A team's primary colour as the server sent it, only when it is exactly #RRGGBB (it is set as a CSS custom property).
 * @param {unknown} hex
 * @returns {string|null}
 */
export function teamColour(hex) {
  return typeof hex === 'string' && /^#[0-9a-fA-F]{6}$/.test(hex) ? hex : null;
}

// ---- Stats + Profile v2 (owner 10-03, "I like the mockup": one 12-column grid, equal-height rows) ----
// Pure: every function takes the server's answer as it is and returns strings and numbers ready to draw, or null / []
// when the data a piece needs is not there, so the page leaves that piece out. Nothing here invents a number.

const isNum = (n) => typeof n === 'number' && Number.isFinite(n);
const one = (n) => n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
const pctText = (p) => (isNum(p) ? `${one(p * 100)}%` : EM_DASH);
const dayLabel = (iso) => { const d = new Date(iso); return Number.isFinite(d.getTime()) ? d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) : EM_DASH; };
const dayKey = (iso) => { const d = new Date(iso); return Number.isFinite(d.getTime()) ? d.toISOString().slice(0, 10) : null; };
const isOnline = (m) => Boolean(m) && m.mode === 'online' && m.opponent_kind !== 'cpu' && m.queue !== 'private';
/** A match list's decided games, newest first (a best of 3 adds every game of it). */
function gamesNewestFirst(matches) {
  const out = [];
  for (const m of Array.isArray(matches) ? matches : []) for (const g of Array.isArray(m?.games) ? m.games : []) {
    const r = getResultChar(g?.result);
    if (r) out.push({ ...g, letter: r });
  }
  return out.sort((a, b) => String(b.at).localeCompare(String(a.at)));
}
const avgOf = (xs) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);
const scoredOf = (gs) => gs.filter((g) => Array.isArray(g.score) && isNum(g.score[0]) && isNum(g.score[1]));

/**
 * Your Record (Stats, mv2-0): ONLINE matches only, Ranked and Casual as two halves. The halves come from the server's
 * `by_queue` when it sends it (its ranked and casual rows), else they are counted from the online matches the page
 * has, as before. Each half: "8-5", its win rate, the bar's share and points per game when known. Null when there is
 * no online match at all.
 */
export function recordCard(byQueue, matches) {
  const rows = (Array.isArray(byQueue) ? byQueue : []).filter((r) => r && (r.key === 'ranked' || r.key === 'casual'));
  const online = (Array.isArray(matches) ? matches : []).filter(isOnline);
  const fromList = (q) => {
    const list = online.filter((m) => m.queue === q);
    const r = recordOf(list);
    return { wins: r.wins, losses: r.losses, pts: avgOf(scoredOf(gamesNewestFirst(list)).map((g) => g.score[0])) };
  };
  const half = (key) => {
    const row = rows.find((r) => r.key === key);
    const t = row ? { wins: isNum(row.wins) ? row.wins : 0, losses: isNum(row.losses) ? row.losses : 0, pts: isNum(row.pts_per_game) ? row.pts_per_game : null } : fromList(key);
    const n = t.wins + t.losses;
    return { wins: t.wins, losses: t.losses, games: n, text: `${t.wins}${HYPHEN}${t.losses}`, share: n ? t.wins / n : null, rate: n ? pctText(t.wins / n) : EM_DASH,
      pts: isNum(t.pts) ? one(t.pts) : EM_DASH };
  };
  const ranked = half('ranked'), casual = half('casual');
  const wins = ranked.wins + casual.wins, losses = ranked.losses + casual.losses, games = wins + losses;
  if (!games) return null;
  return { text: `${wins}${HYPHEN}${losses}`, games, rate: pctText(wins / games), ranked, casual, source: rows.length ? 'server' : 'list' };
}

/** The current streak from W/L letters (newest first): "W6", or an em dash. */
export function streakOf(results) {
  const r = (Array.isArray(results) ? results : []).filter((x) => x === 'W' || x === 'L');
  if (!r.length) return EM_DASH;
  let n = 0;
  while (n < r.length && r[n] === r[0]) n++;
  return `${r[0]}${n}`;
}

/**
 * Last 10 (Stats' Your Record foot): the newest ten online MATCHES' letters (newest first), their record, the streak
 * over every online match, and points scored / allowed per game over the games of those ten. Null with no online
 * match.
 */
export function lastTenStrip(matches) {
  const online = (Array.isArray(matches) ? matches : []).filter(isOnline);
  const ten = online.filter((m) => getResultChar(m.result)).slice(0, 10);
  if (!ten.length) return null;
  const results = ten.map((m) => getResultChar(m.result));
  const streak = streakOf(resultsOf(online));
  const gs = scoredOf(gamesNewestFirst(ten));
  const w = results.filter((x) => x === 'W').length;
  return { results, text: `${w}${HYPHEN}${results.length - w}`, streak, streakKind: streak.startsWith('W') ? 'up' : streak.startsWith('L') ? 'down' : '',
    scored: gs.length ? one(avgOf(gs.map((g) => g.score[0]))) : null, allowed: gs.length ? one(avgOf(gs.map((g) => g.score[1]))) : null };
}

/**
 * The newest ten decided GAMES' letters of a match list (newest first): the profile's Last 10 strip, the same games
 * the server's `recent_form` counts (backend lastGames).
 */
export function lastTenGames(matches) {
  return gamesNewestFirst((Array.isArray(matches) ? matches : []).filter((m) => m && m.queue !== 'private' && getResultChar(m.result))).slice(0, 10).map((g) => g.letter);
}

/**
 * The Elo card (Stats mv2-0; Profile's hero panel and Elo history). `state`:
 *   'rated'    -- a rating: the value, "#7" ("#7 of 214" when the server sends the board's size), the peak, the week's
 *                 change when the server sends `week_delta`, and the curve (the start, then every ranked match).
 *   'unranked' -- Elo is on but this player has no rating yet (the board has players).
 *   'off'      -- no Elo at all (off until launch: no rating on an empty board, or no `elo` sent).
 * Never a fake number: anything the server does not send is null.
 */
export function eloCard(elo) {
  if (!elo || typeof elo !== 'object' || !isNum(elo.rating)) {
    const on = Boolean(elo) && isNum(elo.total) && elo.total > 0;
    return { state: on ? 'unranked' : 'off', title: 'Unranked', note: on ? 'No ranked games yet. Play a ranked match to get a rating.' : 'Rankings start soon.' };
  }
  const hist = (Array.isArray(elo.history) ? elo.history : []).filter((h) => h && isNum(h.rating));
  const start = hist.length ? hist[0].rating - (isNum(hist[0].delta) ? hist[0].delta : 0) : null;
  const values = hist.length ? [start, ...hist.map((h) => h.rating)] : [];
  const dates = hist.length ? [hist[0].at, ...hist.map((h) => h.at)] : [];
  const peak = isNum(elo.peak) ? elo.peak : values.length ? Math.max(elo.rating, ...values) : null;
  const week = isNum(elo.week_delta) ? elo.week_delta : null;
  const mid = dates.length > 2 ? Math.floor((dates.length - 1) / 2) : null;
  return {
    state: 'rated', value: fmtInt(elo.rating), rating: elo.rating,
    rank: isNum(elo.rank) ? `#${elo.rank}` : null,
    rankOf: isNum(elo.rank) ? (isNum(elo.total) && elo.total > 0 ? `#${elo.rank} of ${fmtInt(elo.total)}` : `#${elo.rank}`) : null,
    peak: isNum(peak) ? fmtInt(peak) : null, peakValue: isNum(peak) ? peak : null,
    week: week === null ? null : { text: week > 0 ? `+${week}` : week < 0 ? `${MINUS}${-week}` : '0', kind: week > 0 ? 'up' : week < 0 ? 'down' : '' },
    games: isNum(elo.games) && elo.games > 0 ? elo.games : hist.length || null,
    values,
    labels: dates.length >= 2 ? [{ at: 0, text: dayLabel(dates[0]) }, ...(mid === null ? [] : [{ at: mid / (dates.length - 1), text: dayLabel(dates[mid]) }]), { at: 1, text: dayLabel(dates.at(-1)) }] : [],
  };
}

// Each role's five numbers as the mockup spells them (owner 10-03): [field, label, one decimal?]. Labels are words
// ("Yards", "Carries", "Rating"); the abbreviations kept are the game's own (TD, INT, TFL, FF, PBU).
const CELLS = {
  passer: [['cmp/att', 'Cmp/Att'], ['yds', 'Yards'], ['td', 'TD'], ['int', 'INT'], ['rtg', 'Rating', true]],
  rusher: [['att', 'Carries'], ['yds', 'Yards'], ['avg', 'Average', true], ['td', 'TD'], ['long', 'Long']],
  receiver: [['rec', 'Catches'], ['yds', 'Yards'], ['avg', 'Average', true], ['td', 'TD'], ['long', 'Long']],
  dl_lb: [['tkl', 'Tackles'], ['sck', 'Sacks'], ['tfl', 'TFL'], ['ff', 'FF'], ['pbu', 'PBU']],
  ballhawk: [['int', 'INT'], ['pbu', 'PBU'], ['tkl', 'Tackles'], ['ff', 'FF'], ['td', 'TD']],
};
// Most played (Adam 10-03, "more relevant to the player's position"): Games, then the four numbers of that player's
// role, never another role's (no tackles on a quarterback).
const GAME_LINES = {
  passer: [['cmp/att', 'Cmp/Att'], ['yds', 'Yards'], ['td', 'TD'], ['rtg', 'Rating', true]],
  rusher: [['att', 'Carries'], ['yds', 'Yards'], ['avg', 'Average', true], ['td', 'TD']],
  receiver: [['rec', 'Catches'], ['yds', 'Yards'], ['avg', 'Average', true], ['td', 'TD']],
  defender: [['tkl', 'Tackles'], ['sck', 'Sacks'], ['int', 'INT'], ['ff', 'FF']],
};
const POSITION_KIND = { QB: 'passer', RB: 'rusher', HB: 'rusher', FB: 'rusher', WR: 'receiver', TE: 'receiver',
  DL: 'defender', DE: 'defender', DT: 'defender', LB: 'defender', OLB: 'defender', MLB: 'defender', CB: 'defender', S: 'defender', FS: 'defender', SS: 'defender', DB: 'defender' };
/**
 * Which role a player row is: its `position` when the server sends one, else the career table the row came from,
 * told by that table's own columns (backend players.js PROJ): passing has `cmp`, rushing `att` with `avg`/`btk`/`yco`
 * (no `cmp`), receiving `rec`, defense `tkl`. Null when nothing says.
 */
export function playerKind(row) {
  if (!row || typeof row !== 'object') return null;
  const pos = typeof row.position === 'string' ? POSITION_KIND[row.position.trim().toUpperCase()] : null;
  if (pos) return pos;
  if (isNum(row.cmp)) return 'passer';
  if (isNum(row.rec)) return 'receiver';
  if (isNum(row.att)) return 'rusher';
  if (isNum(row.tkl) || isNum(row.sck)) return 'defender';
  return null;
}
const VALUE_LABEL = { pass_yds: 'Passing Yards', rush_yds: 'Rushing Yards', rec_yds: 'Receiving Yards', tkl_sck: 'Tackles + Sacks', int: 'Interceptions', games: 'Games' };

/**
 * A player row's numbers (Your players / Their players, at most five): the role's line from the full career row the
 * server sends on `best_players` (backend cc79302), a field it does not send left out. Most played: its games, then
 * four numbers of whichever row the server picked (a passer's, a rusher's, a receiver's or a defender's). With fewer
 * than two numbers (the deployed server sends only `value` + `unit`): that one number, spelled out ("3,108" "Passing
 * Yards"). Nothing at all: [].
 */
export function statCells(role, data) {
  if (!data || typeof data !== 'object') return [];
  const take = (spec) => spec.flatMap(([k, label, dec]) => {
    if (k === 'cmp/att') return isNum(data.cmp) && isNum(data.att) ? [{ value: `${fmtInt(data.cmp)}/${fmtInt(data.att)}`, label }] : [];
    return isNum(data[k]) ? [{ value: dec ? one(data[k]) : fmtInt(data[k]), label }] : [];
  });
  let cells;
  if (role === 'most_played') {
    const kind = playerKind(data);
    const games = isNum(data.games) ? data.games : data.unit === 'games' && isNum(data.value) ? data.value : null;
    const rest = kind ? take(GAME_LINES[kind]) : [];
    cells = rest.length ? [...(games === null ? [] : [{ value: fmtInt(games), label: 'Games' }]), ...rest] : [];
  } else cells = take(CELLS[role] || []);
  if (cells.length >= 2) return cells.slice(0, 5);
  if (!isNum(data.value)) return [];
  return [{ value: fmtInt(data.value), label: VALUE_LABEL[data.unit] || String(data.unit || '') }];
}

/**
 * Points per game (Stats mv2-1): the newest `n` online games with a score, OLDEST first (the chart reads left to
 * right), each { for, against, won, label } with a date under every fourth bar and the last. The axis top rounds up
 * to its step. Null with fewer than two games.
 */
export function pointsSeries(matches, n = 16) {
  const gs = scoredOf(gamesNewestFirst((Array.isArray(matches) ? matches : []).filter(isOnline))).slice(0, n).reverse();
  if (gs.length < 2) return null;
  const games = gs.map((g, i) => ({ for: g.score[0], against: g.score[1], won: g.letter === 'W', label: i % 4 === 0 || i === gs.length - 1 ? dayLabel(g.at) : '' }));
  const top = Math.max(10, ...games.map((p) => Math.max(p.for, p.against)));
  const step = top <= 40 ? 10 : top <= 80 ? 20 : 50;
  return { games, max: Math.ceil(top / step) * step, step, scored: one(avgOf(games.map((p) => p.for))), allowed: one(avgOf(games.map((p) => p.against))) };
}

/**
 * Results by day (Stats mv2-1): `days` days ending on the newest online match's day, each with the W/L letters of
 * the matches played that day (oldest first, so a day's stack builds upward), and the best day (most wins, then the
 * fewest losses, then the newest). Null with no online match.
 */
export function resultsByDay(matches, days = 24) {
  const online = (Array.isArray(matches) ? matches : []).filter(isOnline).filter((m) => getResultChar(m.result) && dayKey(m.at));
  if (!online.length) return null;
  const end = new Date(`${online.map((m) => dayKey(m.at)).sort().at(-1)}T00:00:00Z`);
  const by = new Map();
  for (const m of [...online].sort((a, b) => String(a.at).localeCompare(String(b.at)))) {
    const k = dayKey(m.at);
    if (!by.has(k)) by.set(k, []);
    by.get(k).push(getResultChar(m.result));
  }
  const out = [];
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(end.getTime() - i * 86400000);
    const k = d.toISOString().slice(0, 10);
    out.push({ key: k, label: d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' }), results: by.get(k) || [] });
  }
  let best = null;
  for (const d of out) {
    const w = d.results.filter((x) => x === 'W').length, l = d.results.length - w;
    if (w && (!best || w > best.w || (w === best.w && l <= best.l))) best = { w, l, label: d.label };
  }
  return { days: out, tallest: Math.max(1, ...out.map((d) => d.results.length)), best: best ? `${best.label} · ${best.w}${HYPHEN}${best.l}` : null };
}

/** Win rate by team (Stats mv2-1): the server's `by_team` rows that have games, most played first, at most `n`; the
 *  Pickup row reads "Pickup" (no logo, its badge "PU"). */
export function teamRates(byTeam, n = 5) {
  return (Array.isArray(byTeam) ? byTeam : []).filter((t) => t && isNum(t.wins) && isNum(t.losses) && t.wins + t.losses > 0)
    .sort((a, b) => (b.wins + b.losses) - (a.wins + a.losses)).slice(0, n).map((t) => {
      const g = t.wins + t.losses;
      return { key: String(t.key ?? t.name), name: String(t.name ?? EM_DASH), team: t.key === 'pickup' ? { abbrev: 'PU' } : { name: t.name, logo_url: t.logo_url ?? null, abbrev: t.abbrev ?? null },
        record: `${t.wins}${HYPHEN}${t.losses}`, share: t.wins / g, pct: `${Math.round((t.wins / g) * 100)}%` };
    });
}

/** Yards split (Stats mv2-1): passing and rushing yards per game from `per_game`, with each one's share. Null unless
 *  both are numbers and their sum is above zero. */
export function yardsSplit(perGame) {
  const p = perGame?.pass, r = perGame?.rush;
  if (!isNum(p) || !isNum(r) || p + r <= 0) return null;
  const share = p / (p + r);
  return { total: fmtInt(Math.round(p + r)), pass: one(p), rush: one(r), passShare: share, passPct: `${Math.round(share * 100)}%`, rushPct: `${100 - Math.round(share * 100)}%` };
}

/** Team per game (Stats mv2-1): the `per_game` numbers the server sends, each a cell { key, label, value, unit, note };
 *  a number it does not send leaves its cell out. */
export function teamPerGame(perGame) {
  if (!perGame || typeof perGame !== 'object') return [];
  const cell = (k, label, unit, note, whole = false) => (isNum(perGame[k]) ? [{ key: k, label, value: whole ? fmtInt(perGame[k]) : one(perGame[k]), unit, note }] : []);
  return [
    ...cell('yds', 'Yards / game', '', 'per game'), ...cell('pass', 'Passing', 'Yards', 'per game'), ...cell('rush', 'Rushing', 'Yards', 'per game'),
    ...cell('pts_per_drive', 'Points / drive', '', 'per drive'), ...cell('first_downs', 'First downs', '', 'per game'), ...cell('turnovers', 'Turnovers', '', 'per game'),
    ...cell('three_outs', 'Three-and-outs', '', 'per game'), ...cell('biggest_lead', 'Biggest lead', '', 'in one game', true),
  ];
}

/** Wins vs losses (Stats mv2-1): per-game points, yards and turnovers in wins and in losses (`wins_vs_losses`). Null
 *  unless both sides have games. */
export function winsLosses(wvl) {
  const w = wvl?.wins, l = wvl?.losses;
  if (!isNum(w?.games) || !isNum(l?.games) || !w.games || !l.games) return null;
  const v = (x, whole) => (isNum(x) ? (whole ? fmtInt(Math.round(x)) : one(x)) : EM_DASH);
  const rows = [['Points', 'pts'], ['Yards', 'yds', true], ['Turnovers', 'to']].map(([label, k, whole]) => ({ label, win: v(w[k], whole), loss: v(l[k], whole) }))
    .filter((r) => r.win !== EM_DASH || r.loss !== EM_DASH);
  return rows.length ? { wins: w.games, losses: l.games, rows } : null;
}

/** The queue column's word and marker: Ranked (a red dot), Casual (a ring), Offline (no marker). */
export function queueMark(match) {
  if (match?.mode === 'offline' || match?.opponent_kind === 'cpu') return { text: 'Offline', kind: 'off' };
  if (match?.queue === 'ranked') return { text: 'Ranked', kind: 'ranked' };
  if (match?.queue === 'casual') return { text: 'Casual', kind: 'casual' };
  return { text: '', kind: '' };
}

/** A 12-column row: the cards present, their preferred spans scaled so the row adds up to 12 (one card alone spans
 *  12). `cards` is [[node, span], ...]; a null node is a card left out. */
export function rowSpans(cards) {
  const list = (Array.isArray(cards) ? cards : []).filter((c) => c && c[0]);
  if (!list.length) return [];
  const total = list.reduce((a, c) => a + c[1], 0);
  const spans = list.map((c) => Math.max(1, Math.round((c[1] * 12) / total)));
  spans[spans.length - 1] += 12 - spans.reduce((a, b) => a + b, 0);
  return list.map((c, i) => [c[0], spans[i]]);
}
