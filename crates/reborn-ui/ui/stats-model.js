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
 * The game page's address for a whole match (owner 10-03, "the box score is the whole match"): a match of one game
 * opens just that game; a match of several opens ONE page holding every game of it (oldest first, as the server sends
 * them) with its last game shown, and the series from the list's side ("2-1"). `from` is the page Back returns to
 * (backHref). Null when the match has no recorded game.
 * @param {Object} match
 * @param {string} from
 * @returns {string|null}
 */
export function matchHref(match, from) {
  const ids = (Array.isArray(match?.games) ? match.games : []).map((g) => (typeof g?.id === 'string' ? g.id : null)).filter(Boolean);
  if (!ids.length) return null;
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
