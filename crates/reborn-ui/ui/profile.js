// The public profile, the online player hub (spec §8.7): one player's ONLINE games (`player_profile`), opened as
// profile.html?player=<uuid>[#view]. The approved mv2 mockup (owner 10-03): no tabs, one short scroll on the v2 grid.
//   Back · the hero: the cover from the avatar's colours, avatar and presence, name, status | Plays as | Joined, Share
//   profile and View on web (the live card under them while they play), and the Elo panel on the right (rating, the
//   week's change, rank of total, peak, ranked record, sparkline; or a clean Unranked state).
//   Then: Elo history (with the leaderboard around them, when the server sends it) · Record (Ranked and Casual apart)
//   · Last 10; Career highlights; Their players (five numbers each) · Recent matches; Head to head · Top connections.
// "All matches ›", "All stats ›", "All players ›" and "All opponents ›" open the fuller views in place (the whole match
// history, Full stats and the rest of the numbers, Head to head): the address's #matches, #stats (or #passing,
// #rushing, #receiving, #defense, the links from Stats), #records, #h2h, without a history entry; Back returns to the
// profile. A card whose data the server does not send is left out and its row partner takes the width. A match opens
// the whole match on one page. Private rooms never show (the deployed server still sends them; dropped on the way in).
// Everything drawn is server data: el()'s `text` (textContent) only, never markup. Images only through assetUrl() or
// avatarSrc() (the CSP's two image prefixes). Script-set styles are custom properties and the cover's --c1/--c2,
// through the CSSOM, never a style attribute.
import { bridge, call, el, markCurrent, quietCall, reason, wireCoreFold } from './shell.js';
import { PAGE_ORIGIN, avatarSrc } from './live-common.js';
import { notFound } from './live-game-render.js';
import { dominantColors } from './cover-color.js';
import { eloCard, fmtInt, fmtPct, lastTenGames, matchHref, matchRowModel, queueMark, rowSpans, streakOf } from './stats-model.js';
import { avatar, matchHeader, matchRows, pager, teamLogo } from './match-rows.js';
import { FULL_STATS, careerRecords, careerTiles, cellValue, isPublic, lastTenOverview, leaderboardRows, liveCard, playerRecords, profileRecord, publicQueues,
  sortValue, splitTop, statusLine, teamRecords, winRate, withCareerLines } from './profile-model.js';
import { recordsView } from './records-view.js';
import { ROLES, card, cellGrid, cols, eloLine, playerRows, tableHead, wlStrip } from './stats-cards.js';
import { tabs } from './tabs.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const SHOWN = 5;     // a long table shows five rows; "Show all N" shows every row in place
const HPAGE = 10;    // Match history rows per page
const RECENT = 7;    // Recent matches on the profile
const DASH = '—', EN = '–';
const HISTORY_TABS = [['all', 'All'], ['ranked', 'Ranked'], ['casual', 'Casual']];
const PAGER_OTHER = { older: 'newer', newer: 'older' };
// The empty states never say a player has never played: private rooms count on the server but never show here.
const NONE_TEXT = 'No online matches to show yet.';
const NONE_OF = { all: NONE_TEXT, ranked: 'No ranked matches to show yet.', casual: 'No casual matches to show yet.' };

const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const playerId = (params.get('player') || '').toLowerCase();
const screen = document.getElementById('screen');

const num = (n) => typeof n === 'number' && Number.isFinite(n);
const list = (v) => (Array.isArray(v) ? v : []);
const int = (n) => fmtInt(num(n) ? Math.round(n) : null);
const day = (iso) => { const d = new Date(iso); return Number.isFinite(d.getTime()) ? d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' }) : DASH; };
const monthYear = (iso) => { const d = new Date(iso); return Number.isFinite(d.getTime()) ? d.toLocaleDateString('en-US', { month: 'short', year: 'numeric' }) : null; };
const profileHref = (id) => `profile.html?player=${encodeURIComponent(id)}`;
const webUrl = () => `${PAGE_ORIGIN}/u/${playerId}`;
const one = (n) => n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });

/** A section's 40px header: its <h2> and, on the right, a link or tabs. */
function sectionHead(id, title, extra = null) {
  return el('div', { class: 'sec-head' }, el('h2', { id: `${id}-h`, text: title }), extra);
}
const link = (key, text, onclick) => el('button', { type: 'button', class: 'sec-link', 'data-key': key, text, onclick });

// ---- Back ----

// Back returns to the profile from a fuller view; on the profile, to the launcher page this one was opened from
// (Stats, a game, the live view, another profile); opened any other way, it goes to Stats.
function backButton() {
  const b = el('button', { type: 'button', class: 'pf-back', id: 'back', 'data-key': 'back', 'data-nav': 'rows' }, el('span', { class: 'chev', 'aria-hidden': 'true' }), el('span', { text: 'Back' }));
  b.addEventListener('click', () => {
    if (view.tab !== 'overview') { showTab('overview'); b.focus({ preventScroll: true }); return; }
    let fromLauncher = false;
    try { fromLauncher = Boolean(document.referrer) && new URL(document.referrer).origin === location.origin; } catch { /* no referrer */ }
    if (fromLauncher && history.length > 1) history.back(); else location.href = 'stats.html';
  });
  return b;
}

// ---- the hero ----

function playsAsText(team) {
  if (!team || !team.name) return null;
  return el('span', { class: 'pf-team' }, el('span', { text: 'Plays as' }), teamLogo(team), el('b', { text: team.name }));
}

/** The owner's own "Plays as" choice: "Most played" (the server's pick) or any team in By team except Pickup. */
function playsAsPicker(p) {
  const h = p.header || {};
  const teams = list(p.by_team).filter((t) => t && t.key !== 'pickup' && Number.isInteger(t.tgid));
  const mostPlayed = () => { const t = teams.find((x) => (x.games ?? 0) > 0); return t ? { name: t.name, abbrev: t.abbrev, tgid: t.tgid, logo_url: t.logo_url } : null; };
  const current = h.plays_as_auto === false && Number.isInteger(h.plays_as?.tgid) ? String(h.plays_as.tgid) : '';
  const select = el('select', { id: 'plays-as', 'data-key': 'plays-as' },
    el('option', { value: '', text: 'Most played' }),
    ...teams.map((t) => el('option', { value: String(t.tgid), text: t.name })));
  select.value = current;
  const logoSlot = el('span', { class: 'pf-team-logo' }, h.plays_as ? teamLogo(h.plays_as) : null);
  const note = el('span', { class: 'pf-said', role: 'status' });
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
  return el('span', { class: 'pf-team' }, el('label', { for: 'plays-as', text: 'Plays as' }), logoSlot, select, note);
}

/** The cover's two colours from the avatar; any failure (no avatar, CORS, a tainted canvas) keeps the neutral cover. */
function coverColours(target, url) {
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
      const colours = dominantColors(x.getImageData(0, 0, 32, 32).data);
      if (colours) { target.style.setProperty('--c1', colours[0]); target.style.setProperty('--c2', colours[1]); target.classList.add('has-colour'); }
    } catch { /* tainted canvas: keep the neutral cover */ }
  };
  im.src = src;
}

/** The live card, only while they play a public match (spec §8.7.2): LIVE · queue · game, the score, vs · series. */
function liveBlock(raw) {
  const c = liveCard(raw && raw.queue !== 'ranked' ? { ...raw, best_of: 1 } : raw);
  if (!c) return null;
  const side = (t, right) => el('span', { class: `t${right ? ' r' : ''}` },
    ...(right ? [el('b', { text: t?.abbrev || '' }), teamLogo(t)] : [teamLogo(t), el('b', { text: t?.abbrev || '' })]));
  return el('div', { class: 'pf-live', role: 'group', 'aria-label': `Live now: ${c.head}` },
    el('div', { class: 'pf-live-top' }, el('span', { class: 'live-dot', 'aria-hidden': 'true' }), el('span', { class: 'lbl', text: 'Live' }), el('span', { class: 'q', text: c.head })),
    c.score ? el('div', { class: 'pf-live-sc' }, side(c.me, false), el('strong', { class: 'num', text: c.score }), side(c.opp, true)) : null,
    el('div', { class: 'pf-live-bot' }, el('span', { text: c.line }),
      el('button', { type: 'button', class: 'btn sm', 'data-key': 'live-feed', text: 'Live feed', onclick: () => { location.href = c.href; } })));
}

/** The hero's Elo panel (mv2-3): the rating, the week's change, rank of total, peak, ranked record, the sparkline; or
 *  Unranked ("Rankings start soon" while Elo is off). */
function eloPanel(p) {
  const e = eloCard(p.elo);
  const box = el('div', { class: 'pf-elo', role: 'group', 'aria-label': 'Elo' }, el('span', { class: 'k-label', text: 'Elo · Ranked' }));
  if (e.state !== 'rated') {
    box.classList.add('unranked');
    box.append(el('div', { class: 'pf-elo-v', text: e.title }), el('p', { class: 'vc-sub', text: e.note }));
    return box;
  }
  const ranked = profileRecord(p.summary, p.by_queue)?.ranked;
  const facts = [['Rank', e.rankOf], ['Peak', e.peak], ['Record', ranked && ranked.wins + ranked.losses ? ranked.text : null]].filter(([, v]) => v);
  box.append(el('div', { class: 'pf-elo-v' }, e.value, e.week ? el('small', { class: e.week.kind || null, text: `${e.week.text} this week` }) : null),
    facts.length ? el('div', { class: 'pf-elo-facts' }, ...facts.flatMap(([k, v]) => [el('span', { text: k }), el('b', { class: 'num', text: v })])) : null,
    eloLine(e.values, { mini: true, label: `Elo over ${e.values.length - 1} ranked matches` }));
  return box;
}

// The header's live parts (status, presence dot, live card), redrawn in place by the 30 s refresh; set by hero().
let updateHeader = null;

function hero(p, ownerId) {
  const h = p.header || {};
  const name = h.display_name || 'Player';
  const dot = el('span', { class: 'presence-dot', 'aria-hidden': 'true' });
  const pic = el('div', { class: 'pf-pic cover-pic' }, avatar(h.avatar_url, name, 'pf-avatar'));
  const meta = el('p', { class: 'pf-meta' });
  const bits = [];
  const owner = Boolean(ownerId) && String(h.player_id || '').toLowerCase() === ownerId;
  const team = owner ? playsAsPicker(p) : playsAsText(h.plays_as);
  if (team) bits.push(team);
  const joined = monthYear(h.joined);
  if (joined) bits.push(el('span', { text: `Joined ${joined}` }));
  const sep = () => el('span', { class: 'pf-dot', 'aria-hidden': 'true', text: '|' });
  bits.forEach((b, i) => { if (i) meta.append(sep()); meta.append(b); });
  // The status leads the line while there is one; it comes and goes with the refresh without touching the rest.
  const statusEl = el('span', { class: 'pf-status' });
  const statusSep = bits.length ? sep() : null;
  const said = el('span', { class: 'pf-said', role: 'status' });
  let saidTimer = null;
  const shareBtn = el('button', { type: 'button', class: 'btn primary sm', 'data-key': 'share', text: 'Share profile' });
  shareBtn.addEventListener('click', async () => {
    const url = webUrl();
    try { await navigator.clipboard.writeText(url); said.textContent = 'Link copied'; } catch { said.textContent = url; }
    clearTimeout(saidTimer);
    saidTimer = setTimeout(() => { said.textContent = ''; }, 2000);
  });
  const web = el('button', { type: 'button', class: 'btn sm', 'data-key': 'web', text: 'View on web' });
  web.addEventListener('click', async () => {
    try { await call('open_website', { url: webUrl() }); } catch (e) { said.textContent = reason(e); }
  });
  const acts = el('div', { class: 'pf-acts' }, shareBtn, web, said);
  const id = el('div', { class: 'pf-id' }, el('h2', { class: 'pf-name', text: name }), meta, acts);
  // data-nav="rows" (Back and the hero only): Down reads the hero row by row; data-nav-seq: Share profile, View on web,
  // Plays as, Live feed in reading order. The rest of the page keeps column moves.
  const row = el('div', { class: 'pf-row', 'data-nav-seq': '' }, pic, id);
  const box = el('div', { class: 'glass pf-card', 'data-nav': 'rows' }, el('div', { class: 'cover-wash', 'aria-hidden': 'true' }), row, eloPanel(p));
  let liveKey = null, liveEl = null;
  // Only the status, the presence dot and the live card change; nothing focusable is rebuilt unless the live card
  // itself changed, and a focused Live feed keeps focus (or moves to View on web when the card leaves).
  updateHeader = (next) => {
    const status = statusLine(next) ?? (next?.online === false ? { text: 'Offline', kind: 'off' } : null);
    if (status) {
      statusEl.textContent = status.kind === 'off' ? status.text : `● ${status.text}`;
      statusEl.classList.toggle('off', status.kind === 'off');
      if (!statusEl.isConnected) meta.prepend(...[statusEl, statusSep].filter(Boolean));
    } else { statusEl.remove(); statusSep?.remove(); }
    if (status && status.kind !== 'off') { if (!dot.isConnected) pic.append(dot); } else dot.remove();
    const key = JSON.stringify(next?.live ?? null);
    if (key === liveKey) return;
    liveKey = key;
    const hadFocus = Boolean(liveEl && typeof liveEl.contains === 'function' && liveEl.contains(document.activeElement));
    const fresh = liveBlock(next?.live);
    if (liveEl && fresh) liveEl.replaceWith(fresh);
    else if (liveEl) liveEl.remove();
    else if (fresh) id.append(fresh);
    liveEl = fresh;
    if (hadFocus) (fresh?.querySelector('[data-key="live-feed"]') ?? web).focus({ preventScroll: true });
  };
  updateHeader(h);
  coverColours(box, h.avatar_url);
  if (typeof document.title === 'string') document.title = `${name} | NFL Street Reborn`;
  return box;
}

// ---- the views: the profile itself, or one fuller view (#matches, #stats, #records, #h2h) in its place ----
// The address's #<view> names it (no history entry: one Back still leaves the page), so a game opened from a view comes
// back to it; the links from Stats (#passing, #rushing, #receiving, #defense) open Full stats on that table.

const VIEWS = ['overview', 'matches', 'stats', 'records', 'h2h'];
const HASH_VIEW = { '': 'overview', overview: 'overview', career: 'overview', matches: 'matches', history: 'matches', stats: 'stats', 'full-stats': 'stats',
  more: 'stats', passing: 'stats', rushing: 'stats', receiving: 'stats', defense: 'stats', records: 'records', h2h: 'h2h' };
const view = { tab: 'overview', tabs: [], p: null, self: false, body: null };
const here = (tab = view.tab) => `profile.html?player=${playerId}${tab === 'overview' ? '' : `#${tab}`}`;

/** Show a view: the body redrawn, the address following without a history entry. `focus`: the view's first control. */
function showTab(tab, { focus = false, scroll = false } = {}) {
  view.tab = view.tabs.includes(tab) ? tab : 'overview';
  try { history.replaceState(history.state, '', `${location.pathname}${location.search}${view.tab === 'overview' ? '' : `#${view.tab}`}`); } catch { /* the address is a convenience */ }
  drawBody();
  if (focus) view.body.querySelector('button, a[href], select')?.focus({ preventScroll: true });
  if (scroll) view.body.scrollIntoView?.({ block: 'start' });
  else if (view.tab !== 'overview') document.getElementById('main')?.scrollTo?.({ top: 0 });
}

function drawBody() {
  const p = view.p;
  const parts = {
    overview: () => overview(p),
    matches: () => [historySection(p)],
    stats: () => [fullStatsSection(p), moreStats(p, view.self)],
    records: () => [recordsSection(p)],
    h2h: () => [h2hSection(p)],
  }[view.tab]();
  view.body.replaceChildren(...parts.filter(Boolean));
  view.body.setAttribute('data-tab', view.tab);
  if (view.tab === 'matches') fill();
}

/** One grid row from [[card, span], ...]; null when no card of it has data (rowSpans gives the partner the width). */
function gridRow(cards) {
  const present = rowSpans(cards);
  if (!present.length) return null;
  for (const [node, span] of present) node.className = node.className.replace(/\bs\d+\b/, `s${span}`);
  return el('div', { class: 'vgrid' }, ...present.map(([node]) => node));
}

// ---- the profile (mv2-3, mv2-4) ----

function overview(p) {
  return [
    gridRow([[eloHistoryCard(p), 6], [recordCard(p), 3], [lastTenCard(p), 3]]),
    gridRow([[careerCard(p), 12]]),
    gridRow([[playersCard(p), 7], [recentCard(), 5]]),
    gridRow([[h2hCard(p), 7], [combosCard(p), 5]]),
  ].filter(Boolean);
}

function eloHistoryCard(p) {
  const e = eloCard(p.elo);
  if (e.state !== 'rated') return null;
  const chart = eloLine(e.values, { labels: e.labels, peak: e.peakValue, label: `Elo over ${Math.max(0, e.values.length - 1)} ranked matches` });
  const lad = leaderboardRows(p.elo?.around, { self: view.self });
  if (!chart && !lad.length) return null;
  const ladder = lad.length ? el('div', { class: 'pf-lad' }, el('span', { class: 'k-label', text: `Around ${e.rank || ''}`.trim() }),
    el('div', { class: 'vc-rows' }, ...lad.map((r) => {
      const kids = [el('span', { class: 'num pf-lad-r', text: r.rank.slice(1) }), el('b', { text: r.name }), el('span', { class: 'num pf-lad-e', text: r.elo })];
      const attrs = { class: `vc-tr${r.me ? ' me' : ''}`, 'aria-label': `${r.rank} ${r.name}, ${r.elo} Elo` };
      return r.player_id && !r.me && UUID_RE.test(r.player_id) ? el('a', { ...attrs, href: profileHref(r.player_id), 'data-key': `lad-${r.player_id}` }, ...kids)
        : el('div', { ...attrs, role: 'group' }, ...kids);
    }))) : null;
  return card('elo-history', 'Elo history', el('div', { class: `pf-elo-h${ladder ? '' : ' solo'}` }, chart, ladder),
    { aside: e.games ? `${e.games} ranked ${e.games === 1 ? 'game' : 'games'}` : null });
}

function recordCard(p) {
  const r = profileRecord(p.summary, p.by_queue);
  // A new player (0-0) has no record to show (P4-23).
  if (!r || !r.games) return null;
  const halves = [['Ranked', 'ranked', r.ranked], ['Casual', 'casual', r.casual]].filter(([, , t]) => t)
    .map(([label, kind, t]) => el('div', { class: 'pf-q', role: 'group', 'aria-label': `${label} record ${t.text}` },
      el('span', { class: `qm ${kind}`, text: label }), el('b', { class: 'num', text: t.text }), el('small', { class: 'num', text: winRate(t.pct) ?? DASH })));
  return card('record', 'Record', [el('div', { class: 'pf-big', text: r.text }), el('p', { class: 'vc-sub' }, el('b', { text: winRate(r.pct) ?? DASH }), ' online'),
    halves.length ? el('div', { class: 'pf-halves' }, ...halves) : null]);
}

function lastTenCard(p) {
  const l = lastTenOverview(p.recent_form);
  if (!l) return null;
  const letters = lastTenGames(hist.loaded);
  const streak = streakOf(letters);
  return card('last10', 'Last 10', [el('div', { class: 'pf-big', text: l.value }),
    el('p', { class: 'vc-sub' }, 'Streak ', el('b', { class: streak.startsWith('W') ? 'up' : streak.startsWith('L') ? 'down' : null, text: streak })),
    letters.length ? el('div', { class: 'pf-strip' }, wlStrip(letters)) : null,
    el('div', { class: 'vc-kv pf-kv' }, el('span', { text: 'Scored / allowed' }), el('b', { text: `${l.scored} / ${l.allowed}` })),
    l.margin.text ? el('div', { class: 'vc-kv' }, el('span', { text: 'Avg margin' }), el('b', { class: l.margin.kind || null, text: l.margin.text })) : null]);
}

function careerCard(p) {
  const { tiles } = careerTiles(p.career, p.career_rank, p.per_game?.games);
  const shown = tiles.filter((t) => t.key !== 'sck' && t.value !== DASH);
  if (!shown.length) return null;
  const cells = shown.map((t) => ({ label: t.label, value: t.value, note: [t.rank, t.perGame].filter(Boolean).join(' · ') }));
  return card('career', 'Career highlights', cellGrid(cells, cells.length), { aside: link('all-stats', 'All stats ›', () => showTab('stats', { focus: true })), flush: true });
}

function playersCard(p) {
  const rows = playerRows(withCareerLines(p.best_players, p.career), { roles: ROLES.filter(([r]) => r !== 'most_played') });
  if (!rows.length) return null;
  return card('players', view.self ? 'Your players' : 'Their players', el('div', { class: 'vc-rows' }, ...rows),
    { aside: link('all-players', 'All players ›', () => showTab('stats', { focus: true })), flush: true });
}

const RECENT_COLS = '62px minmax(0,1fr) 74px 48px';
function recentCard() {
  const rows = hist.loaded.slice(0, RECENT).map((m) => {
    const r = matchRowModel({ ...m, games: list(m.games) });
    const q = queueMark(m);
    const href = matchHref(m, here('overview'));
    const cells = [el('span', { class: `pf-res ${r.result || ''}`, text: `${r.result} ${r.score}`.trim() }),
      el('span', { class: 'pf-who' }, avatar(m.opponent_kind === 'cpu' || !m.opponent ? null : r.avatar, r.opponent, '', { cpu: m.opponent_kind === 'cpu' }), el('b', { text: r.opponent })),
      el('span', { class: `qm ${q.kind}`, text: q.text }), el('span', { class: 'r num pf-date', text: r.date })];
    const label = `${r.date}. ${r.result === 'W' ? 'Win' : r.result === 'L' ? 'Loss' : ''} ${r.score} against ${r.opponent}${q.text ? `, ${q.text}` : ''}.`;
    return cols(href ? el('button', { type: 'button', class: 'vc-tr', 'data-key': `recent-${r.id}`, 'aria-label': label, onclick: () => { location.href = href; } }, ...cells)
      : el('div', { class: 'vc-tr', role: 'group', 'aria-label': label }, ...cells), RECENT_COLS);
  });
  return card('recent', 'Recent matches', [tableHead(['Result', 'Opponent', 'Queue', 'Date'], RECENT_COLS, [3]),
    el('div', { class: 'vc-rows pf-recent' }, ...(rows.length ? rows : [el('p', { class: 'm-empty', text: NONE_TEXT })]))],
  { aside: link('all-matches', 'All matches ›', () => showTab('matches', { focus: true })), flush: true });
}

// Head to head's "Last": a one-game match ("g:<game id>") shows its score; a series' summed points would read like a
// single score, so it shows its date (as the old profile did).
function lastOf(last) {
  const single = typeof last?.game_id === 'string' && last.game_id.startsWith('g:');
  const s = last?.score;
  return single && Array.isArray(s) && num(s[0]) && num(s[1]) ? `${s[0]}${EN}${s[1]}` : day(last?.at);
}
const recordOf = (r) => `${num(r?.wins) ? r.wins : 0}-${num(r?.losses) ? r.losses : 0}`;

function opponentCell(h) {
  const name = h.display_name || 'Player';
  const inner = [avatar(h.avatar_url, name), el('b', { text: name })];
  return h.player_id && UUID_RE.test(String(h.player_id).toLowerCase())
    ? el('a', { class: 'pf-who', href: profileHref(h.player_id), 'data-key': `h2h-${h.player_id}` }, ...inner) : el('span', { class: 'pf-who' }, ...inner);
}

const H2H_COLS = 'minmax(0,1fr) 64px 88px 96px 104px';
function h2hCard(p) {
  const all = list(p.head_to_head).filter(Boolean);
  if (!all.length) return null;
  const rows = all.slice(0, SHOWN).map((h) => {
    const letter = h.last?.result === 'win' ? 'W' : h.last?.result === 'loss' ? 'L' : '';
    const best = num(h.biggest_win?.margin) ? `+${h.biggest_win.margin}${h.biggest_win.at ? ` ${day(h.biggest_win.at)}` : ''}` : DASH;
    return cols(el('div', { class: 'vc-tr', role: 'group', 'aria-label': `${h.display_name || 'Player'}: ${recordOf(h)}` }, opponentCell(h),
      el('b', { class: 'r num', text: recordOf(h) }), el('span', { class: 'r num pf-mut', text: `${int(h.pts_for)}${EN}${int(h.pts_against)}` }),
      el('span', { class: `r num pf-res ${letter}`, text: h.last ? `${letter} ${lastOf(h.last)}`.trim() : DASH }), el('span', { class: 'r num pf-date', text: best })), H2H_COLS);
  });
  return card('h2h-card', 'Head to head', [tableHead(['Opponent', 'Record', 'Points', 'Last', 'Biggest win'], H2H_COLS, [1, 2, 3, 4]), el('div', { class: 'vc-rows' }, ...rows)],
    { aside: all.length > SHOWN ? link('all-h2h', `All ${all.length} opponents ›`, () => showTab('h2h', { focus: true })) : 'Online', flush: true });
}

const COMBO_COLS = 'minmax(0,1fr) 40px 46px 34px';
function combosCard(p) {
  const combos = list(p.combos).filter((c) => c && c.passer && c.receiver).slice(0, SHOWN);
  if (!combos.length) return null;
  // the surname: the last word that is not a suffix ("M. Penix Jr." is Penix)
  const last = (n) => String(n || '').trim().split(/\s+/).filter((w) => !/^(jr|sr|ii|iii|iv|v)\.?$/i.test(w)).at(-1) || DASH;
  return card('connections', 'Top connections', [tableHead(['Passer to receiver', 'Rec', 'Yards', 'TD'], COMBO_COLS, [1, 2, 3]),
    el('div', { class: 'vc-rows' }, ...combos.map((c) => cols(el('div', { class: 'vc-tr', role: 'group', 'aria-label': `${c.passer.name} to ${c.receiver.name}: ${int(c.rec)} catches, ${int(c.yds)} yards, ${int(c.td)} TD` },
      el('span', { class: 'pf-who' }, teamLogo({ name: c.passer.team, logo_url: c.passer.logo_url }), el('b', { text: `${last(c.passer.name)} → ${last(c.receiver.name)}` })),
      el('span', { class: 'r num', text: int(c.rec) }), el('b', { class: 'r num', text: int(c.yds) }), el('span', { class: 'r num', text: int(c.td) })), COMBO_COLS)))], { flush: true });
}

// ---- Matches: the Stats rows, the queue as the second line; All / Ranked / Casual (spec §8.7.4) ----
// `player_profile` takes no queue, so the filter runs here, over the matches loaded so far, and "Older" loads more
// pages (`before`) until the page on screen is full or the server has no more.

const hist = { filter: 'all', page: 0, loaded: [], cursor: null, total: 0, done: false, busy: false, error: '', render: () => {} };
const shownMatches = () => hist.loaded.filter((m) => hist.filter === 'all' || m.queue === hist.filter);

function initHistory(p) {
  const raw = list(p.recent_matches);
  Object.assign(hist, { filter: 'all', page: 0, loaded: raw.filter(isPublic), cursor: raw.at(-1)?.at ?? null,
    total: num(p.matches_total) ? p.matches_total : raw.length, busy: false, error: '', render: () => {} });
  hist.done = raw.length >= hist.total || !hist.cursor;
}

async function fill() {
  if (hist.busy) return;
  const need = () => shownMatches().length < Math.max((hist.page + 1) * HPAGE, RECENT);
  if (hist.done || !need()) return;
  hist.busy = true; hist.error = ''; hist.render();
  try {
    while (!hist.done && need()) {
      const page = await quietCall('player_profile', { playerId, before: hist.cursor });
      if (page && page.ok === false) throw new Error(typeof page.error === 'string' && page.error ? page.error : 'More matches could not be loaded.');
      const raw = list(page?.matches);
      hist.loaded.push(...raw.filter(isPublic));
      if (raw.length) hist.cursor = raw.at(-1).at;
      if (!raw.length || !hist.cursor) hist.done = true;
    }
  } catch (e) { hist.error = `More matches could not be loaded: ${reason(e)}`; }
  hist.busy = false;
  hist.render();
}

function historySection() {
  const head = el('div', { class: 'sec-head' });
  const cardEl = el('div', { class: 'glass m-list' });
  hist.render = () => {
    if (!cardEl.isConnected && view.tab !== 'matches') return;
    const tabFocus = typeof head.contains === 'function' && head.contains(document.activeElement);
    const active = typeof cardEl.contains === 'function' && cardEl.contains(document.activeElement) ? document.activeElement?.getAttribute('data-key') : null;
    head.replaceChildren(el('h2', { id: 'history-h', text: 'Match history' }),
      tabs('profile-history', HISTORY_TABS.map(([, l]) => l), HISTORY_TABS.find(([k]) => k === hist.filter)[1],
        (l) => { hist.filter = HISTORY_TABS.find(([, x]) => x === l)[0]; hist.page = 0; hist.render(); fill(); }, { label: 'Show matches' }));
    const rows = shownMatches();
    const pageRows = rows.slice(hist.page * HPAGE, (hist.page + 1) * HPAGE);
    cardEl.replaceChildren(matchHeader());
    if (pageRows.length) cardEl.append(...pageRows.flatMap((m) => matchRows(m, { where: 'profile', from: here('matches') })));
    else cardEl.append(el('p', { class: 'm-empty', text: hist.busy ? 'Loading…' : NONE_OF[hist.filter] }));
    if (hist.error) cardEl.append(el('p', { class: 'pf-err', role: 'status', text: hist.error }));
    const more = (hist.page + 1) * HPAGE < rows.length || !hist.done;
    if (hist.page > 0 || more) cardEl.append(pager({ page: hist.page, pages: hist.done ? Math.max(1, Math.ceil(rows.length / HPAGE)) : null,
      onNewer: () => { hist.page--; hist.render(); }, onOlder: () => { hist.page++; hist.render(); fill(); }, olderDisabled: !more || hist.busy }));
    if (tabFocus) head.querySelector('[aria-pressed="true"]')?.focus({ preventScroll: true });
    else if (active) {
      const all = Array.from(cardEl.querySelectorAll('[data-key]'));
      let t = all.find((n) => n.getAttribute('data-key') === active);
      if (t?.disabled && PAGER_OTHER[active]) t = all.find((n) => n.getAttribute('data-key') === PAGER_OTHER[active]) ?? t;
      t?.focus({ preventScroll: true });
    }
  };
  hist.render();
  // data-nav="column": Down from the last row is the pager at the card's foot.
  return el('section', { id: 'history', class: 'pf-col', 'data-nav': 'column', 'aria-labelledby': 'history-h' }, head, cardEl);
}

// ---- Records: Single game and Career side by side, Team records (records-view.js), as Stats › Records ----

function recordsSection(p) {
  const parts = recordsView({ single: playerRecords(p.records, hist.loaded), career: careerRecords(p.career), team: teamRecords(p.records, hist.loaded) },
    { from: here('records'), aside: 'Online games' });
  return el('section', { id: 'records', class: 'pf-records', 'aria-labelledby': 'records-h' }, sectionHead('records', 'Records'),
    ...(parts.length ? parts : [el('p', { class: 'pf-none', text: 'No records yet.' })]));
}

// ---- Stats: Full stats (one table at a time, the top five, "Show all N players", then Total; spec §8.7.5), then the
// rest of the profile's numbers ----

const fs = { tab: 'passing', kinds: [], open: new Set(), sort: {}, render: () => {} };

function who(name, team) {
  return el('span', { class: 'who' }, teamLogo(team), el('span', { text: name || DASH }));
}

function statTable(kind, data) {
  const def = FULL_STATS[kind];
  const order = fs.sort[kind] || null;
  const rows = list(data?.rows).slice();
  if (order) rows.sort((a, z) => {
    const x = order.key === 'name' ? String(a.name ?? '') : sortValue(def.cols[order.i], a);
    const y = order.key === 'name' ? String(z.name ?? '') : sortValue(def.cols[order.i], z);
    if (typeof x === 'string' || typeof y === 'string') return order.dir * String(x ?? '').localeCompare(String(y ?? ''));
    return order.dir * ((num(x) ? x : -Infinity) - (num(y) ? y : -Infinity));
  });
  const { top, rest } = splitTop(rows);
  const all = fs.open.has(kind);
  const head = (label, title, key, i) => {
    const th = el('th', { scope: 'col' });
    if (order && order.key === key) th.setAttribute('aria-sort', order.dir > 0 ? 'ascending' : 'descending');
    th.append(el('button', { type: 'button', class: 'pf-sort', 'data-key': `sort-${kind}-${key}`, text: label, title: `Sort by ${title.toLowerCase()}`,
      onclick: () => { fs.sort[kind] = { key, i, dir: order?.key === key ? -order.dir : key === 'name' ? 1 : -1 }; fs.render(); } }));
    return th;
  };
  const body = [...top, ...(all ? rest : [])].map((r) => el('tr', {},
    el('th', { scope: 'row' }, who(r.name, { name: r.team, logo_url: r.logo_url, abbrev: r.abbrev })), ...def.cols.map((c) => el('td', { text: cellValue(c, r) }))));
  if (rest.length && !all) body.push(el('tr', { class: 'more' }, el('td', { colspan: String(def.cols.length + 1) },
    el('button', { type: 'button', class: 'pf-showall', 'data-key': `showall-${kind}`, onclick: () => { fs.open.add(kind); fs.render(); } },
      el('span', { text: `Show all ${rows.length} players` }), el('span', { class: 'chev', 'aria-hidden': 'true' })))));
  if (data?.total) body.push(el('tr', { class: 'total' }, el('th', { scope: 'row', text: 'Total' }), ...def.cols.map((c) => el('td', { text: cellValue(c, data.total) }))));
  return el('table', { class: 'tbl' }, el('caption', { class: 'pf-sr', text: `${def.title}, every player used. Select a column heading to sort.` }),
    el('thead', {}, el('tr', {}, head('Player', 'Player', 'name', -1), ...def.cols.map((c, i) => head(c.label, c.title, c.key, i)))),
    el('tbody', {}, ...body));
}

function fullStatsSection(p) {
  fs.kinds = Object.keys(FULL_STATS).filter((k) => list(p.career?.[k]?.rows).length);
  if (!fs.kinds.length) return null;
  if (!fs.kinds.includes(fs.tab)) fs.tab = fs.kinds[0];
  const head = el('div', { class: 'sec-head' });
  const body = followFocus(el('div', { class: 'glass pf-tbl' }));
  fs.render = () => {
    const tabFocus = typeof head.contains === 'function' && head.contains(document.activeElement);
    const active = typeof body.contains === 'function' && body.contains(document.activeElement) ? document.activeElement?.getAttribute('data-key') : null;
    head.replaceChildren(el('h2', { id: 'full-stats-h', text: 'Full stats' }),
      tabs('profile-full', fs.kinds.map((k) => FULL_STATS[k].title), FULL_STATS[fs.tab].title,
        (title) => { fs.tab = fs.kinds.find((k) => FULL_STATS[k].title === title); fs.render(); }, { label: 'Stat tables' }));
    body.replaceChildren(statTable(fs.tab, p.career[fs.tab]));
    if (tabFocus) head.querySelector('[aria-pressed="true"]')?.focus({ preventScroll: true });
    else if (active) {
      // "Show all" leaves once every player shows: focus moves to the table's first heading, not back to the page top.
      const all = Array.from(body.querySelectorAll('[data-key]'));
      (all.find((n) => n.getAttribute('data-key') === active) ?? all.find((n) => n.getAttribute('data-key') === `sort-${fs.tab}-name`))?.focus({ preventScroll: true });
    }
  };
  fs.render();
  return el('section', { id: 'full-stats', 'aria-labelledby': 'full-stats-h' }, head, body);
}

const oneOr = (n) => (num(n) ? one(n) : DASH);
const pctOf = (p) => (num(p) ? fmtPct(p) : DASH);
const convOf = (p) => (num(p) ? `${Math.round(p * 100)}%` : DASH);

/** A small table. With `cap` ({ id, what }), a long list shows five rows and a "Show all N <what>" row below them; the
 *  same button then reads "Show top 5" (it stays, so focus never falls back to the page top). */
function smallTable(caption, headers, rows, capOpt = null) {
  const trs = rows.map((cells) => el('tr', {}, ...cells.map((c, i) => {
    const cell = i === 0 ? el('th', { scope: 'row' }) : el('td');
    if (typeof c === 'string') cell.textContent = c; else if (c) cell.append(c);
    return cell;
  })));
  const body = el('tbody', {}, ...trs);
  if (capOpt && trs.length > SHOWN) {
    const label = el('span');
    const b = el('button', { type: 'button', class: 'pf-showall', 'data-key': `showall-${capOpt.id}`, 'aria-expanded': 'false' }, label, el('span', { class: 'chev', 'aria-hidden': 'true' }));
    const set = (open) => {
      trs.slice(SHOWN).forEach((r) => { r.hidden = !open; });
      b.setAttribute('aria-expanded', String(open));
      label.textContent = open ? `Show top ${SHOWN}` : `Show all ${trs.length} ${capOpt.what}`;
    };
    b.addEventListener('click', () => set(b.getAttribute('aria-expanded') !== 'true'));
    set(false);
    body.append(el('tr', { class: 'more' }, el('td', { colspan: String(headers.length) }, b)));
  }
  return el('table', { class: 'tbl' }, el('caption', { class: 'pf-sr', text: caption }),
    el('thead', {}, el('tr', {}, ...headers.map((h) => el('th', { scope: 'col', text: h })))), body);
}

/** A table wider than its card scrolls sideways inside it (the small window): keep the focused control (a pad user's
 *  heading or link) wholly in view, clear of the sticky Player column. */
function followFocus(box) {
  box.addEventListener('focusin', (e) => {
    if (!(e.target instanceof Element) || box.scrollWidth <= box.clientWidth) return;
    // the whole cell, not just its button: a column's numbers sit under its heading's full width
    const t = e.target.closest('th, td') || e.target;
    const viewBox = box.getBoundingClientRect(), r = t.getBoundingClientRect();
    const sticky = t.closest('th:first-child, td:first-child') ? 0 : (box.querySelector('thead th:first-child')?.getBoundingClientRect().width ?? 0);
    if (r.right > viewBox.right) box.scrollLeft += r.right - viewBox.right + 8;
    else if (r.left < viewBox.left + sticky) box.scrollLeft -= viewBox.left + sticky - r.left + 8;
  });
  // The sticky Player column takes its solid surface only while numbers scroll under it.
  box.addEventListener('scroll', () => box.classList.toggle('scrolled', box.scrollLeft > 0), { passive: true });
  return box;
}

function moreCard(id, title, table, wide = false) {
  const c = el('section', { id, class: `glass pf-more-card${wide ? ' wide' : ''}`, 'aria-labelledby': `${id}-h` }, el('h3', { id: `${id}-h`, text: title }), table);
  return wide ? followFocus(c) : c;
}

/** Every player row (P4-25; Adam 10-03: Most played shows its own role's numbers), the v2 rows. */
function playersMore(p, self) {
  const rows = playerRows(withCareerLines(p.best_players, p.career));
  if (!rows.length) return null;
  return el('section', { id: 'more-players', class: 'glass pf-more-card', 'aria-labelledby': 'more-players-h' },
    el('h3', { id: 'more-players-h', text: self ? 'Your players' : 'Players' }), el('div', { class: 'vc-rows pf-pls' }, ...rows));
}

function moreStats(p, self) {
  const cards = [];
  const players = playersMore(p, self);
  if (players) cards.push(players);
  const pg = p.per_game;
  if (pg && pg.games) cards.push(moreCard('per-game', 'Per game', smallTable(`Per game, over ${int(pg.games)} games with play-by-play`, ['Stat', `Per game (${int(pg.games)} games)`], [
    ['Total yards', oneOr(pg.yds)], ['Pass / rush', `${oneOr(pg.pass)} / ${oneOr(pg.rush)}`], ['First downs', oneOr(pg.first_downs)], ['Turnovers', oneOr(pg.turnovers)],
    ['1-pt conversions', convOf(pg.conv1_pct)], ['2-pt conversions', convOf(pg.conv2_pct)], ['Points per drive', oneOr(pg.pts_per_drive)], ['Three-and-outs', oneOr(pg.three_outs)],
    ['Longest drive', num(pg.longest_drive) ? `${pg.longest_drive} plays` : DASH], ['Lead changes', oneOr(pg.lead_changes)], ['Biggest lead', oneOr(pg.biggest_lead)], ['Safeties', int(pg.safeties)]])));
  const teams = list(p.by_team).filter(Boolean);
  if (teams.length) cards.push(moreCard('by-team', 'By team', smallTable('Record by team played as', ['Team', 'Record', 'Pts / gm', 'Win %'],
    teams.map((t) => [who(t.name, t.key === 'pickup' ? { abbrev: 'PU' } : { name: t.name, logo_url: t.logo_url, abbrev: t.abbrev }), recordOf(t), oneOr(t.pts_per_game), pctOf(t.win_pct)]), { id: 'by-team', what: 'teams' })));
  const combos = list(p.combos).filter((c) => c && c.passer && c.receiver);
  if (combos.length) cards.push(moreCard('combos', 'Best QB → receiver', smallTable('Quarterback to receiver combinations', ['Combo', 'REC', 'YDS', 'TD'],
    combos.map((c) => [who(`${c.passer.name} → ${c.receiver.name}`, { name: c.passer.team, logo_url: c.passer.logo_url }), int(c.rec), int(c.yds), int(c.td)]), { id: 'combos', what: 'combos' })));
  const pt = p.pitches;
  if (pt && (pt.pitches || pt.pitch_plays)) cards.push(moreCard('pitches', 'Laterals & pitches', smallTable('Laterals and pitches', ['Stat', 'Total'],
    [['Pitches', int(pt.pitches)], ['Plays with a pitch', int(pt.pitch_plays)], ['Yards on those plays', int(pt.pitch_yds)]])));
  const w = p.wins_vs_losses?.wins, l = p.wins_vs_losses?.losses;
  if (w?.games || l?.games) cards.push(moreCard('wins-losses', 'In wins vs losses', smallTable('Per-game numbers in wins and in losses', ['Games', 'PTS', 'YDS', 'TO'],
    [[`Wins (${int(w?.games ?? 0)})`, oneOr(w?.pts), oneOr(w?.yds), oneOr(w?.to)], [`Losses (${int(l?.games ?? 0)})`, oneOr(l?.pts), oneOr(l?.yds), oneOr(l?.to)]])));
  // A private room is never drawn (P4-3): the deployed server may list one as a game type.
  const types = publicQueues(p.by_game_type);
  if (types.length) cards.push(moreCard('by-type', 'By game type', smallTable('Record by game type', ['Game type', 'Record', 'Pts / gm', 'Win %'],
    types.map((t) => [String(t.name ?? DASH), recordOf(t), oneOr(t.pts_per_game), pctOf(t.win_pct)]))));
  if (!cards.length) return null;
  const toRecords = view.tabs.includes('records') ? link('to-records', 'Records ›', () => showTab('records', { focus: true })) : null;
  return el('section', { id: 'more', 'aria-labelledby': 'more-h' }, sectionHead('more', 'More stats', toRecords), el('div', { class: 'pf-more' }, ...cards));
}

// ---- Head to head, every opponent ----

function h2hSection(p) {
  const h2h = list(p.head_to_head).filter(Boolean);
  return el('section', { id: 'h2h-tab', 'aria-label': 'Head to head' }, moreCard('h2h', 'Head to head', smallTable('Record against every opponent',
    ['Opponent', 'Record', 'PTS', 'Allowed', 'Last', 'Best win', 'Worst loss'], h2h.map((h) => {
      const name = h.display_name || 'Player';
      const inner = [avatar(h.avatar_url, name), el('span', { text: name })];
      const cell = h.player_id && UUID_RE.test(String(h.player_id).toLowerCase())
        ? el('a', { class: 'who', href: profileHref(h.player_id), 'data-key': `h2h-${h.player_id}` }, ...inner) : el('span', { class: 'who' }, ...inner);
      const letter = h.last?.result === 'win' ? 'W' : h.last?.result === 'loss' ? 'L' : '';
      return [cell, recordOf(h), int(h.pts_for), int(h.pts_against), h.last ? `${letter} ${lastOf(h.last)}`.trim() : DASH,
        num(h.biggest_win?.margin) ? `+${h.biggest_win.margin}` : DASH, num(h.biggest_loss?.margin) ? `−${h.biggest_loss.margin}` : DASH];
    }), { id: 'h2h', what: 'opponents' }), true));
}

// ---- the page ----

/** The address's #hash picks the view (and, for #passing / #rushing / #receiving / #defense, the Full stats table). */
function jump() {
  if (!view.p) return;
  const hash = typeof location !== 'undefined' ? decodeURIComponent(location.hash || '').slice(1) : '';
  if (FULL_STATS[hash]) fs.tab = hash;
  const tab = HASH_VIEW[hash] ?? 'overview';
  if (tab !== view.tab || FULL_STATS[hash]) showTab(tab, { scroll: Boolean(hash) && tab !== 'overview' });
}

function draw(p, ownerId) {
  const self = Boolean(ownerId) && ownerId === String(p.header?.player_id || '').toLowerCase();
  view.self = self;
  const out = [backButton(), hero(p, ownerId)];
  // Nothing public to show: no matches at all, or only private rooms (dropped) and nothing more to load.
  const raw = list(p.recent_matches);
  const onlyHidden = !raw.some(isPublic) && raw.length >= (num(p.matches_total) ? p.matches_total : raw.length);
  if (!(num(p.matches_total) && p.matches_total > 0) || onlyHidden) {
    out.push(el('p', { class: 'pf-none', text: NONE_TEXT }));
    screen.replaceChildren(...out);
    return;
  }
  initHistory(p);
  const hasStats = Object.keys(FULL_STATS).some((k) => list(p.career?.[k]?.rows).length);
  const hasRecords = list(p.records).some((r) => r && r.label) || careerRecords(p.career).length > 0;
  Object.assign(view, { p, tabs: VIEWS.filter((v) => v === 'overview' || v === 'matches' || (v === 'stats' && (hasStats || list(p.by_team).length))
    || (v === 'records' && hasRecords) || (v === 'h2h' && list(p.head_to_head).some(Boolean))), body: el('div', { class: 'pf-body' }) });
  out.push(view.body);
  screen.replaceChildren(...out);
  const hash = typeof location !== 'undefined' ? decodeURIComponent(location.hash || '').slice(1) : '';
  if (FULL_STATS[hash]) fs.tab = hash;
  const first = HASH_VIEW[hash] ?? 'overview';
  const deep = Boolean(hash) && first !== 'overview';
  showTab(first, { scroll: deep });
  // A deep link (#stats, #passing...) lands on its view again once the page has settled: a late stylesheet or web font
  // can change the layout under the first scroll.
  if (deep) {
    const again = () => { if (view.tab === first) view.body.scrollIntoView?.({ block: 'start' }); };
    requestAnimationFrame?.(again);
    if (document.readyState !== 'complete') window.addEventListener?.('load', again, { once: true });
    document.fonts?.ready.then(again);
  }
  // A first page that was mostly private rooms (the deployed server still sends them) tops up now, so Recent matches
  // and the match history are full.
  fill().then(() => { if (view.tab === 'overview') drawBody(); });
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
  else { draw(profile.value, ownerId); startRefresh(); }
  screen.removeAttribute?.('aria-busy');
}

// Every 30 s (as Stats), a quiet `player_profile` (the first load's arguments) redraws the header's live parts only:
// focus, scroll, the view and the history pages stay as they are. Skipped while the window is hidden or a profile
// request (this one, or Match history loading more) is still out.
const REFRESH_MS = 30000;
let refreshing = false, refreshTimer = null;
async function refreshHeader() {
  if (refreshing || hist.busy || !updateHeader || document.visibilityState === 'hidden') return;
  refreshing = true;
  try {
    const p = await quietCall('player_profile', { playerId });
    if (p && typeof p === 'object' && p.ok !== false && p.header) updateHeader(p.header);
  } catch { /* keep what shows; the next tick tries again */ }
  refreshing = false;
}
function startRefresh() {
  if (refreshTimer === null) refreshTimer = setInterval(refreshHeader, REFRESH_MS);
}

markCurrent();
wireCoreFold();
load();
if (typeof window !== 'undefined') window.addEventListener?.('hashchange', jump);
