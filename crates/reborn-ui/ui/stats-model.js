/**
 * Pure view models for the stats dashboard and profile pages.
 * No dependencies, vanilla ES modules, formatter utilities for numbers, Elo, records, etc.
 */

// Dash characters (exact Unicode required by tests)
const EM_DASH = '—'; // U+2014 for missing values
const EN_DASH = '–'; // U+2013 for score/record separators
const MINUS = '−'; // U+2212 for Elo changes

/**
 * Format integer with thousands separator, or em dash for null/NaN.
 * @param {number|null} n
 * @returns {string}
 */
export function fmtInt(n) {
  if (n === null || Number.isNaN(n)) return EM_DASH;
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
 * Format record as "W–L" or "W–L–T", using en dash as separator.
 * Ties only appear when above 0.
 * @param {{wins: number, losses: number, ties: number}} record
 * @returns {string}
 */
export function fmtRecord(record) {
  const { wins, losses, ties } = record;
  if (ties > 0) {
    return `${wins}${EN_DASH}${losses}${EN_DASH}${ties}`;
  }
  return `${wins}${EN_DASH}${losses}`;
}

/**
 * Format Elo rating with value, change indicator, and ladder rank.
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

  const rank = elo.rank ? `#${elo.rank} on the ladder` : '';

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
 * Get result character or empty string.
 * @param {string|null} result
 * @returns {string}
 */
function getResultChar(result) {
  if (!result) return '';
  const char = result.charAt(0).toUpperCase();
  return char;
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
        stat: ''
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
      stat
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
 * Build you grid (6 items).
 * @param {{wins, losses, ties, win_pct, streak, pts_per_game, yds_per_game}} summary
 * @param {{abbrev, logo_url}|null} playsAs
 * @returns {Array}
 */
export function youGrid(summary, playsAs) {
  const record = fmtRecord({ wins: summary.wins, losses: summary.losses, ties: summary.ties });
  const winPct = summary.win_pct !== null && summary.win_pct !== undefined ? fmtPct(summary.win_pct) : EM_DASH;
  const streak = summary.streak || EM_DASH;
  const ptsPerGame = summary.pts_per_game !== null && summary.pts_per_game !== undefined ? summary.pts_per_game.toString() : EM_DASH;
  const ydsPerGame = summary.yds_per_game !== null && summary.yds_per_game !== undefined ? fmtInt(summary.yds_per_game) : EM_DASH;
  const playsAsValue = playsAs?.abbrev || EM_DASH;

  const grid = [
    { label: 'Record', value: record },
    { label: 'Win %', value: winPct },
    { label: 'Streak', value: streak },
    { label: 'Pts / gm', value: ptsPerGame },
    { label: 'Yds / gm', value: ydsPerGame },
    { label: 'Plays as', value: playsAsValue }
  ];

  if (playsAs?.logo_url) {
    grid[5].logo = playsAs.logo_url;
  }

  return grid;
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
      // Normalize rating: min at top, max at bottom
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
