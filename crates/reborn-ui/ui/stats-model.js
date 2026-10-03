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
    pass_yds: 'pass yds',
    rush_yds: 'rush yds',
    rec_yds: 'rec yds',
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

/**
 * Find a game's neighbours in a newest-first list of game ids: "Previous" is the older game (the next
 * entry in the list), "Next" the newer one (the entry before it). Used by the game view's ‹ Previous / Next ›
 * bar to step through the list of games the page it was opened from was showing.
 * @param {Array<string>|null|undefined} list
 * @param {string} id
 * @returns {{prev: string|null, next: string|null}}
 */
export function neighbours(list, id) {
  const ids = Array.isArray(list) ? list : [];
  const i = ids.indexOf(id);
  if (i === -1) return { prev: null, next: null };
  return { prev: ids[i + 1] ?? null, next: ids[i - 1] ?? null };
}

const BACK_PROFILE_RE = /^profile\.html\?player=[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/**
 * Where the game view's "← Back" (and B / Esc) goes: the page that stored sessionStorage['nsr.gameList'] (its
 * `from`), accepted only as exactly 'stats.html' or 'profile.html?player=<lowercase uuid>'; anything else is Stats.
 * @param {unknown} from
 * @returns {string}
 */
export function backHref(from) {
  return typeof from === 'string' && (from === 'stats.html' || BACK_PROFILE_RE.test(from)) ? from : 'stats.html';
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
 * The You card's four numbers (spec §8.6): Record, Win %, Last 10, Streak. A winning streak is marked 'up'.
 * @param {Object|null} summary
 * @param {{results: Array<string>}|null} recentForm
 * @returns {Array<{label: string, value: string, kind: string}>}
 */
export function youNumbers(summary, recentForm) {
  const s = summary || {};
  const isNum = (n) => typeof n === 'number' && Number.isFinite(n);
  const streak = typeof s.streak === 'string' && /^[WL]\d+$/.test(s.streak) ? s.streak : EM_DASH;
  return [
    { label: 'Record', value: isNum(s.wins) && isNum(s.losses) ? `${s.wins}${HYPHEN}${s.losses}` : EM_DASH, kind: '' },
    { label: 'Win %', value: isNum(s.win_pct) ? fmtPct(s.win_pct) : EM_DASH, kind: '' },
    { label: 'Last 10', value: lastTen(recentForm?.results), kind: '' },
    { label: 'Streak', value: streak, kind: streak.startsWith('W') ? 'up' : '' },
  ];
}

// Each role's statline (spec §8.6 table): [field, unit, one decimal?].
const STATLINES = {
  passer: [['yds', 'YDS'], ['td', 'TD'], ['int', 'INT'], ['rtg', 'RTG', true]],
  rusher: [['att', 'CAR'], ['yds', 'YDS'], ['td', 'TD'], ['avg', 'AVG', true]],
  receiver: [['rec', 'REC'], ['yds', 'YDS'], ['td', 'TD'], ['avg', 'AVG', true]],
  dl_lb: [['tkl', 'TKL'], ['sck', 'SCK'], ['tfl', 'TFL'], ['pbu', 'PBU']],
  ballhawk: [['int', 'INT'], ['pbu', 'PBU'], ['tkl', 'TKL'], ['td', 'TD']],
};
const UNIT_SHORT = { pass_yds: 'YDS', rush_yds: 'YDS', rec_yds: 'YDS', tkl_sck: 'TKL + SCK', int: 'INT', games: 'GAMES' };

/**
 * A player row's statline: the role's four numbers when the server sends them (numbers formatted, units uppercase),
 * else the one number it always sends (`value` + `unit`), else nothing.
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
  if (spec && spec.every(([k]) => isNum(data[k]))) return spec.map(([k, unit, dec]) => ({ value: fmt(data[k], dec), unit }));
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
