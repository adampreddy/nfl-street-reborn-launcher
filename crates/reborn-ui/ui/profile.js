// The native public profile (spec §2): one player's ONLINE career from the server (`player_profile`), opened as
// profile.html?player=<uuid>[#section]. The same page as the web's /u/<id> (reborn-backend src/web/profile-page.js),
// with the launcher's own row, number and role-card models (stats-model.js).
// Everything drawn here is server data (display names, team names and player names are typed by people): el()'s
// `text` (textContent) only, never markup. An image only ever comes through assetUrl() (our own /live-assets/ catalog)
// or avatarSrc() (the Discord avatar CDN), the two image prefixes the CSP allows. The only style the script sets is
// the cover's two colour custom properties, through the CSSOM (style.setProperty), never a style attribute.
import { bridge, call, el, markCurrent, quietCall, reason, wireCoreFold } from './shell.js';
import { PAGE_ORIGIN, assetUrl, avatarSrc } from './live-common.js';
import { notFound } from './live-game-render.js';
import { dominantColors } from './cover-color.js';
import { fmtElo, fmtInt, fmtPct, fmtRecord, matchRowModel, roleCards, totalLine } from './stats-model.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const SHOWN = 5; // R8: a long list shows five rows; "All ›" shows every row in place.
const DASH = '—', EN = '–', MINUS = '−';
const RESULT_WORD = { W: 'win', L: 'loss', T: 'tie' };
const LETTER = { win: 'W', loss: 'L', tie: 'T' };
const SVG_NS = 'http://www.w3.org/2000/svg';

const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const playerId = (params.get('player') || '').toLowerCase();
const screen = document.getElementById('screen');

// ---- numbers (every formatter here is null-safe: a field the server left out draws as a dash, never a throw) ----

const num = (n) => typeof n === 'number' && Number.isFinite(n);
const int = (n) => fmtInt(num(n) ? Math.round(n) : null);
const pct = (p) => fmtPct(num(p) ? p : null);
const one = (n) => (num(n) ? n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 }) : DASH);
const numOr1 = (n) => (num(n) ? (Number.isInteger(n) ? int(n) : one(n)) : DASH);
const conv = (p) => (num(p) ? `${Math.round(p * 100)}%` : DASH);
const record = (r) => (r ? fmtRecord({ wins: r.wins ?? 0, losses: r.losses ?? 0, ties: r.ties ?? 0 }) : DASH);
const score = (s) => (Array.isArray(s) && num(s[0]) && num(s[1]) ? `${s[0]}${EN}${s[1]}` : DASH);
const day = (iso) => { const d = new Date(iso); return Number.isFinite(d.getTime()) ? d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) : DASH; };
const monthYear = (iso) => { const d = new Date(iso); return Number.isFinite(d.getTime()) ? d.toLocaleDateString('en-US', { month: 'short', year: 'numeric' }) : null; };
const list = (v) => (Array.isArray(v) ? v : []);

// ---- small pieces ----

const profileHref = (id) => `profile.html?player=${encodeURIComponent(id)}`;
const gameHref = (id) => `game.html?game=${encodeURIComponent(id)}`;
const webUrl = () => `${PAGE_ORIGIN}/u/${playerId}`;

// The ids of a set of matches' games, newest first (matches here already are; the server sends each match's own
// games oldest first, so each match's games are reversed on the way in). This is the ordered list game.html's
// ‹ Previous / Next › bar steps through, remembered in sessionStorage['nsr.gameList'] right before we navigate
// to one of them.
function gameIdsOf(matches) {
  const ids = [];
  for (const m of matches) {
    const games = Array.isArray(m?.games) ? m.games : [];
    for (let i = games.length - 1; i >= 0; i--) {
      const g = games[i];
      if (g && typeof g.id === 'string') ids.push(g.id);
    }
  }
  return ids;
}

function storeGameList(matches, label) {
  try { sessionStorage.setItem('nsr.gameList', JSON.stringify({ ids: gameIdsOf(matches), label, from: profileHref(playerId) })); } catch { /* best effort */ }
}

function initial(name) {
  const first = Array.from(String(name || '').trim())[0];
  return first ? first.toUpperCase() : '?';
}

/** A team's logo (16px by default, 22px in match rows), or its abbreviation in a small chip when it has none or the
    image fails. `alt` is empty: the team or player name always stands beside it. */
function teamLogo(t, cls = 'pf-logo') {
  const chip = () => el('span', { class: `${cls} pf-nologo`, 'aria-hidden': 'true', text: t?.abbrev || (t?.name ? String(t.name).slice(0, 3).toUpperCase() : DASH) });
  const src = assetUrl(t?.logo_url);
  if (!src) return chip();
  const img = el('img', { class: cls, alt: '', src, title: t?.name || null });
  img.addEventListener('error', () => img.replaceWith(chip()), { once: true });
  return img;
}

/** A player's Discord avatar, or the first letter of their name in the same circle. */
function avatar(url, name, cls = 'pf-av') {
  const letter = () => el('span', { class: `${cls} pf-blank`, 'aria-hidden': 'true', text: initial(name) });
  const src = avatarSrc({ avatar_url: url });
  if (!src) return letter();
  const img = el('img', { class: cls, alt: '', src });
  img.addEventListener('error', () => img.replaceWith(letter()), { once: true });
  return img;
}

function svg(tag, attrs = {}) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  return node;
}

const none = (text = 'None yet') => el('p', { class: 'pf-none', text });

/** A section: `<section id>` with its `<h2>` in a head row (where "All ›" goes) and a glass panel after it. */
function section(id, title, { panel = true } = {}) {
  const h = el('h2', { id: `${id}-h`, text: title });
  const head = el('div', { class: 'pf-sec' }, h);
  const body = el('div', { class: panel ? 'pf-glass' : 'pf-bare' });
  const s = el('section', { id, class: 'pf-section', 'aria-labelledby': `${id}-h` }, head, body);
  return { s, head, body };
}

/** R8: hide every row after the fifth; "All ›" shows them in place, "Fewer ‹" hides them again. */
function expandable(id, head, rows, what) {
  if (rows.length <= SHOWN) return;
  const set = (open) => rows.slice(SHOWN).forEach((r) => { r.hidden = !open; });
  set(false);
  const b = el('button', { type: 'button', class: 'pf-all', 'aria-expanded': 'false', 'aria-controls': id, text: 'All ›', 'aria-label': `Show all ${rows.length} ${what}` });
  b.addEventListener('click', () => {
    const open = b.getAttribute('aria-expanded') !== 'true';
    set(open);
    b.setAttribute('aria-expanded', String(open));
    b.textContent = open ? 'Fewer ‹' : 'All ›';
    b.setAttribute('aria-label', open ? `Show the first ${SHOWN} ${what}` : `Show all ${rows.length} ${what}`);
  });
  head.append(b);
}

/** One labelled number in a stat grid: the label above, the number under it. */
function stat(label, value) {
  return el('div', {}, el('span', { class: 'pf-label', text: label }), el('b', { class: 'pf-num', text: value }));
}

// ---- header ----

function header(p, ownerId) {
  const h = p.header || {};
  const cover = el('div', { class: 'pf-cover', 'aria-hidden': 'true' });
  const meta = el('p', { class: 'pf-meta' });
  const bits = [];
  const joined = monthYear(h.joined);
  if (joined) bits.push(el('span', { text: `Joined ${joined}` }));
  const owner = Boolean(ownerId) && String(h.player_id || '').toLowerCase() === ownerId;
  bits.push(owner ? playsAsPicker(p) : playsAsText(h.plays_as));
  if (h.online === true) bits.push(el('span', { class: 'pf-online', text: 'Online now' }));
  bits.filter(Boolean).forEach((b, i) => { if (i) meta.append(el('span', { class: 'pf-dot', 'aria-hidden': 'true', text: '·' })); meta.append(b); });

  const said = el('span', { class: 'pf-copied', role: 'status' });
  let saidTimer = null;
  const share = el('button', { type: 'button', class: 'pf-btn', 'data-key': 'share', text: 'Share profile' });
  share.addEventListener('click', async () => {
    const url = webUrl();
    try { await navigator.clipboard.writeText(url); said.textContent = 'Link copied'; } catch { said.textContent = url; }
    clearTimeout(saidTimer);
    saidTimer = setTimeout(() => { said.textContent = ''; }, 2000);
  });
  const web = el('button', { type: 'button', class: 'pf-btn pf-btn-o', 'data-key': 'web', text: 'View on web' });
  web.addEventListener('click', async () => {
    try { await call('open_website', { url: webUrl() }); } catch (e) { said.textContent = reason(e); }
  });

  const name = h.display_name || 'Player';
  const body = el('div', { class: 'pf-body' },
    avatar(h.avatar_url, name, 'pf-pav'),
    el('div', { class: 'pf-id' }, el('h2', { class: 'pf-name', text: name }), meta),
    el('div', { class: 'pf-act' }, share, web, said));

  const elo = fmtElo(num(p.elo?.rating) ? p.elo : null);
  const s = p.summary || {};
  const count = (label, value, cls = '') => el('div', { class: cls || null }, el('dt', { text: label }), el('dd', { class: 'pf-num', text: value }));
  const counts = el('dl', { class: 'pf-counts' },
    count('Elo', elo.value, num(p.elo?.rating) ? '' : 'pf-unranked'),
    count('Ranked', num(p.elo?.rank) ? `#${p.elo.rank}` : DASH),
    count('Record', record(s)),
    count('Win %', pct(s.win_pct)),
    count('Matches', int(s.games ?? 0)));
  // A div, not <header>: styles.css gives every header element the shell's grid-area and bar look.
  const prof = el('div', { class: 'pf-glass pf-prof' }, cover, body, counts);
  coverColours(cover, h.avatar_url);
  if (typeof document.title === 'string') document.title = `${name} | NFL Street Reborn`;
  return prof;
}

function playsAsText(team) {
  if (!team || !team.name) return null;
  return el('span', { class: 'pf-team' }, el('span', { class: 'pf-team-l', text: 'Plays as' }), teamLogo(team), el('span', { text: team.name }));
}

/** The owner's own "Plays as" choice: "Most played" (the server's pick) or any team in By team except Pickup. */
function playsAsPicker(p) {
  const h = p.header || {};
  const teams = list(p.by_team).filter((t) => t && t.key !== 'pickup' && Number.isInteger(t.tgid));
  const mostPlayed = () => { const t = teams.find((x) => (x.games ?? 0) > 0); return t ? { name: t.name, abbrev: t.abbrev, tgid: t.tgid, logo_url: t.logo_url } : null; };
  const current = h.plays_as_auto === false && Number.isInteger(h.plays_as?.tgid) ? String(h.plays_as.tgid) : '';
  const select = el('select', { id: 'plays-as', class: 'pf-select', 'data-key': 'plays-as' },
    el('option', { value: '', text: 'Most played' }),
    ...teams.map((t) => el('option', { value: String(t.tgid), text: t.name })));
  select.value = current;
  const logoSlot = el('span', { class: 'pf-team-logo' }, h.plays_as ? teamLogo(h.plays_as) : null);
  const note = el('span', { class: 'pf-saved', role: 'status' });
  let saved = current;
  select.addEventListener('change', async () => {
    const value = select.value;
    select.disabled = true;
    note.textContent = '';
    try {
      const reply = await call('set_plays_as', { tgid: value === '' ? null : Number(value) });
      if (reply && reply.ok === false) throw new Error(typeof reply.error === 'string' && reply.error ? reply.error : 'Your team could not be saved.');
      saved = value;
      const team = reply?.plays_as ?? (value === '' ? mostPlayed() : teams.find((t) => String(t.tgid) === value) ?? null);
      logoSlot.replaceChildren(...(team ? [teamLogo(team)] : []));
      note.textContent = 'Saved';
      setTimeout(() => { note.textContent = ''; }, 2000);
    } catch (e) {
      select.value = saved;
      note.textContent = reason(e);
    }
    select.disabled = false;
  });
  return el('span', { class: 'pf-team pf-team-own' }, el('label', { class: 'pf-team-l', for: 'plays-as', text: 'Plays as' }), logoSlot, select, note);
}

/** The cover's two colours from the avatar; any failure (no avatar, CORS, a tainted canvas) keeps the neutral cover. */
function coverColours(cover, url) {
  const src = avatarSrc({ avatar_url: url });
  if (!src || typeof Image === 'undefined') return;
  const im = new Image();
  im.crossOrigin = 'anonymous';
  im.decoding = 'async';
  im.onload = () => {
    try {
      const c = document.createElement('canvas');
      c.width = c.height = 32;
      const x = c.getContext('2d');
      x.drawImage(im, 0, 0, 32, 32);
      const cols = dominantColors(x.getImageData(0, 0, 32, 32).data);
      if (cols) { cover.style.setProperty('--c1', cols[0]); cover.style.setProperty('--c2', cols[1]); cover.classList.add('has-colour'); }
    } catch { /* tainted canvas: keep the neutral cover */ }
  };
  im.src = src;
}

// ---- 1. Elo over time ----

function eloSection(elo) {
  const { s, body } = section('elo', 'Elo over time');
  const hist = list(elo?.history).filter((x) => x && num(x.rating));
  if (!num(elo?.rating) || !hist.length) { body.append(none('Unranked')); return s; }
  const start = hist[0].rating - (num(hist[0].delta) ? hist[0].delta : 0);
  const pts = [start, ...hist.map((x) => x.rating)];
  const W = 400, H = 110, pad = 8, lo = Math.min(...pts), hi = Math.max(...pts);
  const y = (r) => (hi === lo ? H / 2 : pad + (H - 2 * pad) * (1 - (r - lo) / (hi - lo)));
  const chart = svg('svg', { viewBox: `0 0 ${W} ${H}`, preserveAspectRatio: 'none', class: 'pf-chart', role: 'img',
    'aria-label': `Elo from ${int(start)} to ${int(elo.rating)} over ${hist.length} ranked matches` });
  const grid = svg('g', { class: 'pf-gridl' });
  for (const gy of [25, 60, 95]) grid.append(svg('line', { x1: 0, x2: W, y1: gy, y2: gy }));
  chart.append(grid, svg('polyline', { class: 'pf-trend', points: pts.map((r, i) => `${Math.round((i / (pts.length - 1)) * W * 10) / 10},${Math.round(y(r) * 10) / 10}`).join(' ') }));
  const peak = hist.reduce((a, x) => (x.rating > a.rating ? x : a), hist[0]);
  body.classList.add('pf-pad');
  body.append(chart, el('div', { class: 'pf-chart-foot' },
    el('span', { text: `Start ${int(start)}${hist[0].at ? ` · ${day(hist[0].at)}` : ''}` }),
    el('span', { text: `Peak ${int(Math.max(peak.rating, start))}${peak.rating >= start && peak.at ? ` · ${day(peak.at)}` : ''}` }),
    el('span', { text: `Now ${int(elo.rating)}` })));
  return s;
}

// ---- 2. Recent matches (rows as on the Stats page: one game opens it, a longer match expands to its games) ----

function matchLabel(m, r) {
  const teams = `${m.me?.name || 'Their team'} vs ${m.opp?.name || 'the other team'}`;
  const who = m.opponent_kind === 'cpu' ? 'against the CPU' : r.opponent !== DASH ? `against ${r.opponent}` : '';
  const sc = r.expandable ? `series ${r.score}` : r.score !== DASH ? `score ${r.score}` : 'no score';
  const parts = [`${r.date} ${r.time}`, [teams, who].filter(Boolean).join(', '), [sc, RESULT_WORD[r.result]].filter(Boolean).join(', ')];
  if (r.tag) parts.push(r.tag);
  if (r.expandable) parts.push(`${r.games.length} games`);
  return `${parts.join('. ')}.`;
}

function rowCells(when, me, opp, opponent, sc, result, tag, tagKind, caret) {
  return [
    el('span', { class: 'pf-when' }, el('b', { text: when[0] }), when[1] ? el('span', { text: when[1] }) : null),
    el('span', { class: 'pf-mu' }, teamLogo(me, 'pf-rlogo'), el('span', { class: 'pf-vs', text: 'vs' }), teamLogo(opp, 'pf-rlogo'), opponent),
    el('span', { class: 'pf-sc', text: sc }),
    el('span', { class: `pf-res${result ? ` ${result}` : ''}`, text: result }),
    el('span', { class: `pf-tag${tagKind ? ` ${tagKind}` : ''}`, text: tag }),
    el('span', { class: 'pf-caret', 'aria-hidden': 'true', text: caret }),
  ];
}

let matchSeq = 0;
// `loaded` is every match shown so far (the initial page plus every "All ›" load); `listLabel` is the
// "<name>'s matches" line stored beside them. Both stay live across "All ›" clicks: matchesSection mutates
// `loaded` in place (push, never reassign), so a row built before a later load still stores the fuller list.
function matchRow(m, loaded, listLabel) {
  const r = matchRowModel({ ...m, games: list(m.games) });
  const opponent = el('span', { class: 'pf-opp' },
    m.opponent_kind === 'cpu' || !m.opponent ? null : avatar(r.avatar, r.opponent),
    el('span', { class: 'pf-oname', text: r.opponent }));
  const cells = rowCells([r.date, r.time], r.me, r.opp, opponent, r.score, r.result, r.tag, r.tagKind, r.expandable ? '›' : '');
  const wrap = el('div', { class: 'pf-match' });
  if (!r.expandable) {
    const attrs = { class: 'pf-row', 'data-key': `match-${r.id}`, 'aria-label': matchLabel(m, r) };
    // A match with no recorded game has nothing to open.
    wrap.append(r.onlyGame
      ? el('a', { ...attrs, href: gameHref(r.onlyGame), onclick: () => storeGameList(loaded, listLabel) }, ...cells)
      : el('div', { ...attrs, role: 'group' }, ...cells));
    return wrap;
  }
  const gamesId = `pf-games-${++matchSeq}`;
  const games = el('div', { class: 'pf-games', id: gamesId, hidden: '' }, ...r.games.map((g, i) => {
    const label = `Game ${i + 1}. ${g.score !== DASH ? `Score ${g.score}` : 'No score'}${g.result ? `, ${RESULT_WORD[g.result]}` : ''}. ${g.date} ${g.time}.`;
    return el('a', { class: 'pf-row pf-game', href: gameHref(g.id), 'data-key': `game-${g.id}`, 'aria-label': label, onclick: () => storeGameList(loaded, listLabel) },
      ...rowCells([`Game ${i + 1}`, ''], g.me, g.opp, el('span', { class: 'pf-gtime', text: g.time }), g.score, g.result, '', '', ''));
  }));
  const row = el('button', { type: 'button', class: 'pf-row pf-exp', 'data-key': `match-${r.id}`, 'aria-expanded': 'false', 'aria-controls': gamesId, 'aria-label': matchLabel(m, r) }, ...cells);
  row.addEventListener('click', () => {
    const open = row.getAttribute('aria-expanded') !== 'true';
    row.setAttribute('aria-expanded', String(open));
    games.hidden = !open;
  });
  wrap.append(row, games);
  return wrap;
}

function matchesSection(p) {
  const { s, head, body } = section('matches', 'Recent matches');
  const first = list(p.recent_matches);
  if (!first.length) { body.append(none()); return s; }
  const listLabel = `${p.header?.display_name || 'Player'}'s matches`;
  // Every match rendered so far, oldest loaded last: matchRow's click handlers close over this array by
  // reference, so pushing onto it (never reassigning) keeps every already-built row's ‹ Previous / Next › list
  // current, including games from a later "All ›" load.
  const loaded = first.slice();
  const rows = el('div', { class: 'pf-rows', id: 'pf-match-rows' }, ...first.map((m) => matchRow(m, loaded, listLabel)));
  body.append(rows);
  let last = first.at(-1)?.at ?? null;
  let shown = first.length;
  const total = num(p.matches_total) ? p.matches_total : shown;
  if (shown < total && last) {
    const msg = el('p', { class: 'pf-err', role: 'status' });
    const more = el('button', { type: 'button', class: 'pf-all', 'data-key': 'matches-all', 'aria-controls': 'pf-match-rows', text: 'All ›', 'aria-label': `Show all ${total} matches` });
    more.addEventListener('click', async () => {
      more.disabled = true;
      msg.textContent = '';
      try {
        const page = await quietCall('player_profile', { playerId, before: last });
        const ms = list(page?.matches);
        if (page && page.ok === false) throw new Error(typeof page.error === 'string' && page.error ? page.error : 'More matches could not be loaded.');
        loaded.push(...ms);
        rows.append(...ms.map((m) => matchRow(m, loaded, listLabel)));
        shown += ms.length;
        if (ms.length) last = ms.at(-1).at;
        if (!ms.length || shown >= total || !last) more.remove();
        else more.disabled = false;
      } catch (e) {
        msg.textContent = `More matches could not be loaded: ${reason(e)}`;
        more.disabled = false;
      }
    });
    head.append(more);
    s.insertBefore(msg, body);
  }
  return s;
}

// ---- 3. Best players (the Stats page's six role cards) ----

function playersSection(best) {
  const { s, body } = section('players', 'Best players', { panel: false });
  body.className = 'pf-pgrid';
  for (const c of roleCards(best || {})) {
    const face = el('span', { class: 'pf-face', 'aria-hidden': 'true' });
    const letter = () => el('span', { class: 'pf-face-initial', text: c.initial || '' });
    const src = assetUrl(c.portrait);
    if (src) { const img = el('img', { class: 'pf-face-img', alt: '', src }); img.addEventListener('error', () => img.replaceWith(letter()), { once: true }); face.append(img); }
    else face.append(letter());
    if (c.stat) face.append(teamLogo({ name: c.team, logo_url: c.logo }, 'pf-face-badge'));
    body.append(el('div', { class: `pf-glass pf-lead${c.stat ? '' : ' none'}` }, face,
      el('span', { class: 'pf-lead-text' }, el('span', { class: 'pf-label', text: c.label }),
        el('span', { class: 'pf-lead-n', text: c.name }), c.stat ? el('span', { class: 'pf-lead-s', text: c.stat }) : null)));
  }
  return s;
}

// ---- 4. Per game ----

function perGameSection(pg) {
  const { s, body } = section('per-game', 'Per game');
  if (!pg || !pg.games) { body.append(none()); return s; }
  body.append(el('div', { class: 'pf-gs' },
    stat('Total yds', numOr1(pg.yds)), stat('Pass / rush', `${numOr1(pg.pass)} / ${numOr1(pg.rush)}`),
    stat('First downs', numOr1(pg.first_downs)), stat('Turnovers', numOr1(pg.turnovers)),
    stat('1-pt conv', conv(pg.conv1_pct)), stat('2-pt conv', conv(pg.conv2_pct)),
    stat('Pts / drive', numOr1(pg.pts_per_drive)), stat('3-and-outs', numOr1(pg.three_outs)),
    stat('Longest drive', num(pg.longest_drive) ? `${pg.longest_drive} plays` : DASH), stat('Lead changes', numOr1(pg.lead_changes)),
    stat('Biggest lead', numOr1(pg.biggest_lead)), stat('Safeties', int(pg.safeties))),
  el('p', { class: 'pf-total', text: `Over ${int(pg.games)} game${pg.games === 1 ? '' : 's'} with play-by-play` }));
  return s;
}

// ---- 5. Career stats: every player used, sortable by any column, one muted total line ----

const CA = (r) => `${int(r.cmp)}/${int(r.att)}`;
const CAREER = {
  passing: { title: 'Passing', cols: [['ca', 'C/A', 'Completions / attempts', CA, (r) => r.cmp], ['yds', 'Yds', 'Passing yards'], ['td', 'TD', 'Touchdowns'], ['int', 'INT', 'Interceptions'],
    ['air', 'Air', 'Air yards'], ['yac', 'YAC', 'Yards after catch'], ['ypa', 'Y/A', 'Yards per attempt', (r) => one(r.ypa)], ['rtg', 'Rtg', 'Passer rating', (r) => one(r.rtg)], ['long', 'Lng', 'Longest'], ['sacked', 'Sacked', 'Times sacked']] },
  rushing: { title: 'Rushing', cols: [['att', 'Att', 'Attempts'], ['yds', 'Yds', 'Rushing yards'], ['avg', 'Avg', 'Yards per carry', (r) => one(r.avg)], ['td', 'TD', 'Touchdowns'], ['long', 'Lng', 'Longest'],
    ['btk', 'BTK', 'Broken tackles'], ['yac', 'YAC', 'Yards after contact']] },
  receiving: { title: 'Receiving', cols: [['rec', 'Rec', 'Receptions'], ['yds', 'Yds', 'Receiving yards'], ['avg', 'Avg', 'Yards per catch', (r) => one(r.avg)], ['yac', 'YAC', 'Yards after catch'],
    ['td', 'TD', 'Touchdowns'], ['long', 'Lng', 'Longest'], ['drops', 'Drops', 'Drops']] },
  defense: { title: 'Defense', cols: [['tkl', 'Tkl', 'Tackles'], ['tfl', 'TFL', 'Tackles for loss'], ['sck', 'Sck', 'Sacks'], ['int', 'INT', 'Interceptions'], ['pbu', 'PBU', 'Passes broken up'],
    ['ff', 'FF', 'Forced fumbles'], ['fr', 'FR', 'Fumble recoveries'], ['td', 'TD', 'Defensive touchdowns']] },
};

/** A player (or team) name with its 16px team logo in front. */
function named(team, logoUrl, name, abbrev = null) {
  return el('span', { class: 'pf-pl' }, teamLogo({ name: team, logo_url: logoUrl, abbrev }), el('span', { class: 'pf-nm', text: name }));
}

/** A table. `rows` are arrays of cells (a string or a node); the first cell is the row's `<th scope="row">`. */
function table(caption, headers, rows) {
  const trs = rows.map((cells) => el('tr', {}, ...cells.map((c, i) => {
    const cell = i === 0 ? el('th', { scope: 'row', class: 'pf-who' }) : el('td', { class: 'pf-num' });
    if (typeof c === 'string') cell.textContent = c; else if (c) cell.append(c);
    return cell;
  })));
  const hr = el('tr', {}, ...headers.map((h, i) => (h instanceof Node ? h : el('th', { scope: 'col', class: i === 0 ? 'pf-who' : null, text: h }))));
  const tbody = el('tbody', {}, ...trs);
  const t = el('table', { class: 'pf-table' }, el('caption', { class: 'pf-sr', text: caption }), el('thead', {}, hr), tbody);
  return { wrap: el('div', { class: 'pf-tbwrap' }, t), trs, tbody, hr };
}

function careerTable(kind, data) {
  const def = CAREER[kind];
  const { s, head, body } = section(kind, def.title);
  const rows = list(data?.rows);
  if (!rows.length) { body.append(none()); return s; }
  const tr = new Map(rows.map((r) => [r, [named(r.team, r.logo_url, r.name || DASH), ...def.cols.map(([key, , , f]) => (f ? f(r) : int(r[key])))]]));
  let expanded = false, order = { key: null, dir: -1 }, current = rows.slice();
  const heads = [['name', 'Player', 'Player', null, (r) => String(r.name ?? '')], ...def.cols].map(([key, label, title, , sortBy], i) => {
    const th = el('th', { scope: 'col', class: i === 0 ? 'pf-who' : null });
    const b = el('button', { type: 'button', class: 'pf-sort', text: label, title: `Sort by ${title.toLowerCase()}`, 'aria-label': `${title}, sort` });
    b.addEventListener('click', () => {
      const dir = order.key === key ? -order.dir : key === 'name' ? 1 : -1;
      order = { key, dir };
      const val = sortBy ?? ((r) => r[key]);
      current = rows.slice().sort((a, z) => {
        const x = val(a), y = val(z);
        if (typeof x === 'string' || typeof y === 'string') return dir * String(x ?? '').localeCompare(String(y ?? ''));
        return dir * ((num(x) ? x : -Infinity) - (num(y) ? y : -Infinity));
      });
      for (const h of built.hr.children) h.removeAttribute('aria-sort');
      th.setAttribute('aria-sort', dir > 0 ? 'ascending' : 'descending');
      draw();
    });
    th.append(b);
    return th;
  });
  const built = table(`${def.title}, every player used. Select a column heading to sort.`, heads, rows.map((r) => tr.get(r)));
  const rowOf = new Map(rows.map((r, i) => [r, built.trs[i]]));
  const draw = () => {
    built.tbody.replaceChildren(...current.map((r) => rowOf.get(r)));
    current.forEach((r, i) => { rowOf.get(r).hidden = !expanded && i >= SHOWN; });
  };
  draw();
  body.append(built.wrap);
  const line = data?.total ? totalLine(data, kind) : '';
  if (line) body.append(el('p', { class: 'pf-total pf-num', text: line }));
  if (rows.length > SHOWN) {
    const b = el('button', { type: 'button', class: 'pf-all', 'aria-expanded': 'false', 'aria-controls': kind, text: 'All ›', 'aria-label': `Show all ${rows.length} ${def.title.toLowerCase()} players` });
    b.addEventListener('click', () => {
      expanded = !expanded;
      b.setAttribute('aria-expanded', String(expanded));
      b.textContent = expanded ? 'Fewer ‹' : 'All ›';
      b.setAttribute('aria-label', expanded ? `Show the first ${SHOWN} ${def.title.toLowerCase()} players` : `Show all ${rows.length} ${def.title.toLowerCase()} players`);
      draw();
    });
    head.append(b);
  }
  return s;
}

// ---- 6-12: the right column ----

function combosSection(combos) {
  const { s, head, body } = section('combos', 'Best QB → receiver');
  const rows = list(combos).filter((c) => c && c.passer && c.receiver);
  if (!rows.length) { body.append(none()); return s; }
  const { wrap, trs } = table('Quarterback to receiver combinations', ['Combo', 'Rec', 'Yds', 'TD'],
    rows.map((c) => [named(c.passer.team, c.passer.logo_url, `${c.passer.name} → ${c.receiver.name}`), int(c.rec), int(c.yds), int(c.td)]));
  body.append(wrap);
  expandable('combos', head, trs, 'combos');
  return s;
}

function pitchesSection(pt) {
  const { s, body } = section('pitches', 'Laterals & pitches');
  if (!pt || !(pt.pitches || pt.pitch_plays)) { body.append(none()); return s; }
  body.append(el('div', { class: 'pf-gs pf-three' }, stat('Pitches', int(pt.pitches)), stat('Plays with a pitch', int(pt.pitch_plays)), stat('Yds on those plays', int(pt.pitch_yds))));
  return s;
}

const recCells = (r) => [record(r), num(r.pts_per_game) ? one(r.pts_per_game) : DASH, pct(r.win_pct)];

function byTeamSection(teams) {
  const { s, head, body } = section('by-team', 'By team');
  const rows = list(teams).filter(Boolean);
  if (!rows.length) { body.append(none()); return s; }
  const { wrap, trs } = table('Record by team played as', ['Team', 'Record', 'Pts / gm', 'Win %'], rows.map((r) => [
    r.key === 'pickup'
      ? el('span', { class: 'pf-pl' }, el('span', { class: 'pf-logo pf-nologo', 'aria-hidden': 'true', text: 'PU' }), el('span', { class: 'pf-nm', text: 'Pickup' }))
      : named(r.name, r.logo_url, r.name || DASH, r.abbrev),
    ...recCells(r)]));
  body.append(wrap);
  expandable('by-team', head, trs, 'teams');
  return s;
}

function byModeSection(queue, type) {
  const { s, body } = section('by-mode', 'By mode', { panel: false });
  body.className = 'pf-stack';
  const small = (caption, first, rows) => el('div', { class: 'pf-glass' },
    list(rows).length ? table(caption, [first, 'Record', 'Pts / gm', 'Win %'], list(rows).map((r) => [String(r.name ?? DASH), ...recCells(r)])).wrap : none());
  body.append(small('Record by queue', 'Queue', queue), small('Record by game type', 'Game type', type));
  return s;
}

function winsLossesSection(wl) {
  const { s, body } = section('wins-losses', 'In wins vs losses');
  const w = wl?.wins, l = wl?.losses;
  if (!w?.games && !l?.games) { body.append(none()); return s; }
  const row = (label, x) => [`${label} (${int(x?.games ?? 0)})`, numOr1(x?.pts), numOr1(x?.yds), numOr1(x?.to)];
  body.append(table('Per-game numbers in wins and in losses', ['Games', 'Pts', 'Yds', 'TO'], [row('Wins', w), row('Losses', l)]).wrap);
  return s;
}

function h2hSection(opponents) {
  const { s, head, body } = section('h2h', 'Head to head');
  const rows = list(opponents).filter(Boolean);
  if (!rows.length) { body.append(none()); return s; }
  const margin = (ref, sign) => (ref && num(ref.margin) ? `${sign}${ref.margin}` : DASH);
  const { wrap, trs } = table('Record against every opponent', ['Opponent', 'Record', 'Pts', 'Allowed', 'Last', 'Best win', 'Worst loss'], rows.map((h) => {
    const name = h.display_name || 'Player';
    const who = [avatar(h.avatar_url, name), el('span', { class: 'pf-nm', text: name })];
    const cell = h.player_id
      ? el('a', { class: 'pf-pl pf-who-link', href: profileHref(h.player_id), 'data-key': `h2h-${h.player_id}` }, ...who)
      : el('span', { class: 'pf-pl' }, ...who);
    // A one-game match ("g:<game id>") shows its score; a series' summed points would read like a tie, so it shows its date.
    const single = typeof h.last?.game_id === 'string' && h.last.game_id.startsWith('g:');
    const last = h.last ? `${LETTER[h.last.result] ?? ''} ${single ? score(h.last.score) : day(h.last.at)}`.trim() : DASH;
    return [cell, record(h), int(h.pts_for), int(h.pts_against), last, margin(h.biggest_win, '+'), margin(h.biggest_loss, MINUS)];
  }));
  body.append(wrap);
  expandable('h2h', head, trs, 'opponents');
  return s;
}

function recordsSection(records) {
  const { s, head, body } = section('records', 'Records');
  const rows = list(records).filter((r) => r && r.label);
  if (!rows.length) { body.append(none()); return s; }
  const nodes = rows.map((r) => {
    const cells = [el('span', { class: 'pf-mut', text: r.label }),
      r.name ? named(r.team, r.logo_url, r.name) : el('span'),
      el('b', { class: 'pf-num', text: int(r.value) }),
      el('span', { class: 'pf-mut pf-num', text: day(r.at) })];
    const label = `${r.label}: ${int(r.value)}${r.name ? `, ${r.name}` : ''}, ${day(r.at)}`;
    return r.game_id
      ? el('a', { class: 'pf-rrow', href: gameHref(r.game_id), 'aria-label': `${label}. Open the game` }, ...cells)
      : el('div', { class: 'pf-rrow', role: 'group', 'aria-label': label }, ...cells);
  });
  body.append(el('div', { class: 'pf-rows' }, ...nodes));
  expandable('records', head, nodes, 'records');
  return s;
}

// ---- the page ----

function draw(p, ownerId) {
  const out = [header(p, ownerId)];
  if (!(num(p.matches_total) && p.matches_total > 0)) {
    out.push(el('p', { class: 'pf-none pf-big', text: 'No online games yet.' }));
    screen.replaceChildren(...out);
    return;
  }
  // Spec §2's scroll order: sections 1-5 are the left column and 6-12 the right; one column when narrow.
  const career = el('div', { class: 'pf-career', id: 'career' }, ...['passing', 'rushing', 'receiving', 'defense'].map((k) => careerTable(k, p.career?.[k])));
  const left = el('div', { class: 'pf-col' }, eloSection(p.elo), matchesSection(p), playersSection(p.best_players), perGameSection(p.per_game), career);
  const right = el('div', { class: 'pf-col' }, combosSection(p.combos), pitchesSection(p.pitches), byTeamSection(p.by_team),
    byModeSection(p.by_queue, p.by_game_type), winsLossesSection(p.wins_vs_losses), h2hSection(p.head_to_head), recordsSection(p.records));
  out.push(el('div', { class: 'pf-two' }, left, right));
  screen.replaceChildren(...out);
  const hash = typeof location !== 'undefined' ? decodeURIComponent(location.hash || '').slice(1) : '';
  if (hash) document.getElementById(hash)?.scrollIntoView?.({ block: 'start' });
}

function missing() {
  const nodes = notFound('stats.html', 'Back to Stats');
  nodes[0].textContent = "That profile couldn't be found.";
  screen.replaceChildren(...nodes);
}

async function load() {
  if (!screen) return;
  if (!UUID_RE.test(playerId)) { missing(); screen.removeAttribute?.('aria-busy'); return; }
  if (!bridge) { screen.replaceChildren(el('p', { class: 'refusal', role: 'alert', text: 'Open this page inside the launcher window.' })); return; }
  const [profile, identity] = await Promise.allSettled([call('player_profile', { playerId }), quietCall('identity_status')]);
  const ownerId = identity.status === 'fulfilled' && identity.value?.signed_in && typeof identity.value.player_id === 'string' ? identity.value.player_id.toLowerCase() : null;
  if (profile.status === 'rejected') {
    screen.replaceChildren(el('p', { class: 'refusal', role: 'alert', text: `That profile could not be loaded: ${reason(profile.reason)}` }),
      el('a', { class: 'button', id: 'back', href: 'stats.html', text: 'Back to Stats' }));
  } else if (!profile.value || typeof profile.value !== 'object' || profile.value.ok === false || !profile.value.header) missing();
  else draw(profile.value, ownerId);
  screen.removeAttribute?.('aria-busy');
}

markCurrent();
wireCoreFold();
load();
