// The public profile, the online player hub (spec §8.7): one player's ONLINE games (`player_profile`), opened as
// profile.html?player=<uuid>[#section]. Top to bottom: Back · the profile card (the cover from the avatar's colours,
// avatar, name, status, Joined, Plays as, the summary line, Share profile / View on web, the live card while they
// play) · the overview (Elo | Record | Last 10 games) · Match history beside Career, Leaderboard and Records · Full
// stats (one table at a time) · More stats (the old profile's other sections, kept).
// A piece whose data the server does not send yet is left out, never drawn as a placeholder (spec §8.7). Private rooms
// never show (the deployed server still sends them; they are dropped on the way in).
// Everything drawn is server data: el()'s `text` (textContent) only, never markup. Images only through assetUrl() or
// avatarSrc() (the CSP's two image prefixes). Script-set styles are the card's --c1/--c2 and a bar's width, through the
// CSSOM, never a style attribute.
import { bridge, call, el, markCurrent, quietCall, reason, wireCoreFold } from './shell.js';
import { PAGE_ORIGIN, assetUrl, avatarSrc } from './live-common.js';
import { notFound } from './live-game-render.js';
import { dominantColors } from './cover-color.js';
import { fmtInt, fmtPct, roleCards } from './stats-model.js';
import { avatar, matchHeader, matchRows, pager, storeGameList, teamLogo } from './match-rows.js';
import { FULL_STATS, careerTiles, cellValue, eloOverview, isPublic, lastTenOverview, leaderboardRows, liveCard, publicQueues, recordOverview, sortValue, splitTop, statusLine, summaryFacts } from './profile-model.js';
import { tabs } from './tabs.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const SHOWN = 5;     // a long list shows five rows; "All ›" shows every row in place
const HPAGE = 10;    // Match history rows per page
const DASH = '—';
const SVG_NS = 'http://www.w3.org/2000/svg';
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
const gameHref = (id) => `game.html?game=${encodeURIComponent(id)}`;
const webUrl = () => `${PAGE_ORIGIN}/u/${playerId}`;

function svg(tag, attrs = {}) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  return node;
}

/** A section's 40px header: its <h2> and, on the right, a link or tabs. */
function sectionHead(id, title, extra = null) {
  return el('div', { class: 'sec-head' }, el('h2', { id: `${id}-h`, text: title }), extra);
}

/** Hide every row after the fifth; "All ›" shows them in place, "Fewer ‹" hides them again. */
function expandable(id, head, rows, what) {
  if (rows.length <= SHOWN) return;
  const set = (open) => rows.slice(SHOWN).forEach((r) => { r.hidden = !open; });
  set(false);
  const b = el('button', { type: 'button', class: 'sec-link', 'aria-expanded': 'false', 'aria-controls': id, text: 'All ›', 'aria-label': `Show all ${rows.length} ${what}` });
  b.addEventListener('click', () => {
    const open = b.getAttribute('aria-expanded') !== 'true';
    set(open);
    b.setAttribute('aria-expanded', String(open));
    b.textContent = open ? 'Fewer ‹' : 'All ›';
    b.setAttribute('aria-label', open ? `Show the first ${SHOWN} ${what}` : `Show all ${rows.length} ${what}`);
  });
  head.append(b);
}

// ---- Back ----

// Back returns to the launcher page this one was opened from (Stats, a game, the live view, another profile); opened
// any other way, it goes to Stats.
function backButton() {
  const b = el('button', { type: 'button', class: 'pf-back', id: 'back', 'data-key': 'back', 'data-nav': 'rows' }, el('span', { class: 'chev', 'aria-hidden': 'true' }), el('span', { text: 'Back' }));
  b.addEventListener('click', () => {
    let fromLauncher = false;
    try { fromLauncher = Boolean(document.referrer) && new URL(document.referrer).origin === location.origin; } catch { /* no referrer */ }
    if (fromLauncher && history.length > 1) history.back(); else location.href = 'stats.html';
  });
  return b;
}

// ---- the profile card ----

function playsAsText(team) {
  if (!team || !team.name) return null;
  return el('span', { class: 'pf-team' }, el('span', { text: 'Plays as' }), teamLogo(team), el('span', { text: team.name }));
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
      const cols = dominantColors(x.getImageData(0, 0, 32, 32).data);
      if (cols) { target.style.setProperty('--c1', cols[0]); target.style.setProperty('--c2', cols[1]); target.classList.add('has-colour'); }
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

// The header's live parts (status, presence dot, live card), redrawn in place by the 30 s refresh; set by card().
let updateHeader = null;

function card(p, ownerId) {
  const h = p.header || {};
  const name = h.display_name || 'Player';
  const dot = el('span', { class: 'presence-dot', 'aria-hidden': 'true' });
  const pic = el('div', { class: 'pf-pic cover-pic' }, avatar(h.avatar_url, name, 'pf-avatar'));
  const meta = el('p', { class: 'pf-meta' });
  const bits = [];
  const joined = monthYear(h.joined);
  if (joined) bits.push(el('span', { text: `Joined ${joined}` }));
  const owner = Boolean(ownerId) && String(h.player_id || '').toLowerCase() === ownerId;
  const team = owner ? playsAsPicker(p) : playsAsText(h.plays_as);
  if (team) bits.push(team);
  bits.forEach((b, i) => { if (i) meta.append(el('span', { class: 'pf-dot', 'aria-hidden': 'true', text: '·' })); meta.append(b); });
  // The status leads the line while there is one; it comes and goes with the refresh without touching the rest.
  const statusEl = el('span', { class: 'pf-status' });
  const statusSep = bits.length ? el('span', { class: 'pf-dot', 'aria-hidden': 'true', text: '·' }) : null;
  const facts = summaryFacts(p);
  const said = el('span', { class: 'pf-said', role: 'status' });
  let saidTimer = null;
  const share = el('button', { type: 'button', class: 'btn primary sm', 'data-key': 'share', text: 'Share profile' });
  share.addEventListener('click', async () => {
    const url = webUrl();
    try { await navigator.clipboard.writeText(url); said.textContent = 'Link copied'; } catch { said.textContent = url; }
    clearTimeout(saidTimer);
    saidTimer = setTimeout(() => { said.textContent = ''; }, 2000);
  });
  const web = el('button', { type: 'button', class: 'btn sm', 'data-key': 'web', text: 'View on web' });
  web.addEventListener('click', async () => {
    try { await call('open_website', { url: webUrl() }); } catch (e) { said.textContent = reason(e); }
  });
  const id = el('div', { class: 'pf-id', 'data-nav-seq': '' }, el('h2', { class: 'pf-name', text: name }), meta,
    facts.length ? el('p', { class: 'pf-sum' }, ...facts.map((f) => el('span', {}, el('b', { class: 'num', text: f.strong }), ` ${f.text}`))) : null,
    el('div', { class: 'pf-acts' }, share, web, said));
  // data-nav="rows" (Back and the card only): Down reads the card row by row, never straight into the match list below
  // it; data-nav-seq: Plays as, Share profile, View on web in reading order. The rest of the page keeps column moves.
  const row = el('div', { class: 'pf-row' }, pic, id);
  const box = el('div', { class: 'glass pf-card', 'data-nav': 'rows' }, el('div', { class: 'cover-wash', 'aria-hidden': 'true' }), row);
  let liveKey = null, liveEl = null;
  // Only the status, the presence dot and the live card change; nothing focusable is rebuilt unless the live card
  // itself changed, and a focused Live feed keeps focus (or moves to View on web when the card leaves).
  updateHeader = (next) => {
    const status = statusLine(next);
    if (status) {
      statusEl.textContent = `● ${status.text}`;
      if (!statusEl.isConnected) meta.prepend(...[statusEl, statusSep].filter(Boolean));
      if (!dot.isConnected) pic.append(dot);
    } else {
      statusEl.remove(); statusSep?.remove(); dot.remove();
    }
    const key = JSON.stringify(next?.live ?? null);
    if (key === liveKey) return;
    liveKey = key;
    const hadFocus = Boolean(liveEl && typeof liveEl.contains === 'function' && liveEl.contains(document.activeElement));
    const fresh = liveBlock(next?.live);
    if (liveEl && fresh) liveEl.replaceWith(fresh);
    else if (liveEl) liveEl.remove();
    else if (fresh) row.append(fresh);
    liveEl = fresh;
    if (hadFocus) (fresh?.querySelector('[data-key="live-feed"]') ?? web).focus({ preventScroll: true });
  };
  updateHeader(h);
  coverColours(box, h.avatar_url);
  if (typeof document.title === 'string') document.title = `${name} | NFL Street Reborn`;
  return box;
}

// ---- the overview: one surface, three identical columns (spec §8.7.3) ----

function areaChart(points, label) {
  if (!points || typeof document.createElementNS !== 'function') return null;
  const s = svg('svg', { class: 'pf-spark', viewBox: '0 0 300 70', preserveAspectRatio: 'none', role: 'img', 'aria-label': label });
  const defs = svg('defs'), grad = svg('linearGradient', { id: 'pf-elo-fill', x1: 0, x2: 0, y1: 0, y2: 1 });
  grad.append(svg('stop', { offset: 0, 'stop-color': 'currentColor', 'stop-opacity': 0.22 }), svg('stop', { offset: 1, 'stop-color': 'currentColor', 'stop-opacity': 0 }));
  defs.append(grad);
  s.append(defs, svg('path', { d: points.area, fill: 'url(#pf-elo-fill)' }),
    svg('polyline', { points: points.line, fill: 'none', stroke: 'currentColor', 'stroke-width': 2, 'stroke-linejoin': 'round', 'vector-effect': 'non-scaling-stroke' }));
  return s;
}

function ovColumn(title, note, big, side, kind, sub, visual) {
  return el('section', { class: 'pf-ovc' },
    el('div', { class: 'pf-ovh' }, el('h2', { text: title }), note ? el('span', { text: note }) : null),
    el('div', { class: 'pf-big num' }, el('span', { text: big }), side ? el('em', { class: kind || null, text: side }) : null),
    el('p', { class: 'pf-sub', text: sub }), visual);
}

function overview(p) {
  const cols = [];
  const e = eloOverview(p.elo);
  if (e) {
    // No rating history yet: no "0 ranked matches" note, no "started at", no chart; the peak only when the server sends it.
    const hasHistory = list(p.elo.history).some((x) => x && num(x.rating));
    const note = hasHistory ? e.note : null;
    const sub = hasHistory ? e.sub : num(p.elo.peak) ? `Peak ${int(p.elo.peak)}` : '';
    cols.push(ovColumn('Elo', note, e.value, e.change.text, e.change.kind, sub, hasHistory ? areaChart(e.points, `Elo over ${e.note}, now ${e.value}`) : null));
  }
  const rr = recordOverview(p.summary, p.by_queue);
  // A new player (0–0) reads "—", with no "0 matches" filler and no empty bars (P4-23).
  const r = rr && rr.value === '0–0' ? { ...rr, value: DASH, pct: '', note: null, rows: [] } : rr;
  if (r) cols.push(ovColumn('Record', r.note, r.value, r.pct, '', r.sub, r.rows.length ? el('div', { class: 'pf-bars' }, ...r.rows.map((row) => {
    const fill = el('i');
    if (fill.style) fill.style.width = `${row.share}%`;
    return el('div', { class: 'pf-bar-row' }, el('span', { text: row.label }), el('span', { class: 'pf-bar', role: 'img', 'aria-label': `${row.label}: ${row.record}` }, fill),
      el('b', { class: 'num', text: row.record }), el('span', { class: 'pf-mute num', text: row.pct }));
  })) : null));
  const l = lastTenOverview(p.recent_form);
  if (l) cols.push(ovColumn('Last 10 games', null, l.value, l.margin.text, l.margin.kind, 'Average winning margin', el('div', { class: 'pf-pair' },
    el('div', {}, el('span', { text: 'Points scored' }), el('b', { class: 'num', text: l.scored })),
    el('div', {}, el('span', { text: 'Points allowed' }), el('b', { class: 'num', text: l.allowed })))));
  if (!cols.length) return null;
  return el('div', { class: `glass pf-ov cols-${cols.length}` }, ...cols);
}

// ---- Match history: the Stats rows, the queue as the second line; All / Ranked / Casual (spec §8.7.4) ----
// `player_profile` takes no queue, so the filter runs here, over the matches loaded so far, and "Older" loads more
// pages (`before`) until the page on screen is full or the server has no more.

const hist = { filter: 'all', page: 0, loaded: [], cursor: null, total: 0, done: false, busy: false, error: '', open: new Set(), render: () => {} };
const shownMatches = () => hist.loaded.filter((m) => hist.filter === 'all' || m.queue === hist.filter);

async function fill() {
  if (hist.busy) return;
  const need = () => shownMatches().length < (hist.page + 1) * HPAGE;
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

function historySection(p) {
  const raw = list(p.recent_matches);
  Object.assign(hist, { filter: 'all', page: 0, loaded: raw.filter(isPublic), cursor: raw.at(-1)?.at ?? null,
    total: num(p.matches_total) ? p.matches_total : raw.length, busy: false, error: '' });
  hist.done = raw.length >= hist.total || !hist.cursor;
  const label = `${p.header?.display_name || 'Player'}'s matches`;
  const openGame = (id) => { storeGameList(hist.loaded, label, profileHref(playerId)); location.href = gameHref(id); };
  const head = el('div', { class: 'sec-head' });
  const cardEl = el('div', { class: 'glass m-list' });
  hist.render = () => {
    const tabFocus = typeof head.contains === 'function' && head.contains(document.activeElement);
    const active = typeof cardEl.contains === 'function' && cardEl.contains(document.activeElement) ? document.activeElement?.getAttribute('data-key') : null;
    head.replaceChildren(el('h2', { id: 'history-h', text: 'Match history' }),
      tabs('profile-history', HISTORY_TABS.map(([, l]) => l), HISTORY_TABS.find(([k]) => k === hist.filter)[1],
        (l) => { hist.filter = HISTORY_TABS.find(([, x]) => x === l)[0]; hist.page = 0; hist.render(); fill(); }, { label: 'Show matches' }));
    const rows = shownMatches();
    const pageRows = rows.slice(hist.page * HPAGE, (hist.page + 1) * HPAGE);
    cardEl.replaceChildren(matchHeader());
    if (pageRows.length) cardEl.append(...pageRows.flatMap((m) => matchRows(m, { where: 'profile', open: hist.open, openGame })));
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
  // A first page that was mostly private rooms (the deployed server still sends them) tops up to a full page now,
  // not only on a tab or "Older".
  fill();
  // data-nav="column": Down from the last row is the pager at the card's foot, never Full stats' column headings
  // further down whose middle sits nearer the row's middle.
  return el('section', { id: 'history', class: 'pf-col', 'data-nav': 'column', 'aria-labelledby': 'history-h' }, head, cardEl);
}

// ---- the right column: Career, Leaderboard, Records ----

function careerSection(p) {
  const { tiles, footnote } = careerTiles(p.career, p.career_rank, p.per_game?.games);
  if (tiles.every((t) => t.value === DASH)) return null;
  const grid = el('div', { class: 'pf-car' }, ...tiles.map((t) => el('div', {},
    el('span', { class: 'k-label', text: t.label }), el('b', { class: 'num', text: t.value }),
    t.rank || t.perGame ? el('span', { class: 'pf-car-s num' }, t.rank ? el('b', { text: t.rank }) : null, t.rank && t.perGame ? ' · ' : null, t.perGame) : null)));
  const hasTables = Object.keys(FULL_STATS).some((k) => list(p.career?.[k]?.rows).length);
  return el('section', { id: 'career', 'aria-labelledby': 'career-h' },
    sectionHead('career', 'Career', hasTables ? el('a', { class: 'sec-link', href: '#full-stats', 'data-key': 'full-stats', text: 'Full stats ›',
      // Scroll in place: following the #full-stats link would add a history entry, and Back would then stay on this page.
      // The href stays for a middle-click.
      onclick: (e) => { e.preventDefault(); document.getElementById('full-stats')?.scrollIntoView?.({ block: 'start' }); } }) : null),
    el('div', { class: 'glass pf-pad' }, grid, footnote ? el('p', { class: 'pf-note', text: footnote }) : null));
}

function leaderboardSection(p, self) {
  const rows = leaderboardRows(p.elo?.around, { self });
  if (!rows.length) return null;
  return el('section', { id: 'leaderboard', 'aria-labelledby': 'leaderboard-h' }, sectionHead('leaderboard', 'Leaderboard'),
    el('div', { class: 'glass pf-lb' }, ...rows.map((r) => {
      const cells = [el('span', { class: 'pf-lb-rank num', text: r.rank }), avatar(r.avatar_url, r.letterOf), el('span', { class: 'pf-lb-name', text: r.name }), el('b', { class: 'num', text: r.elo })];
      if (r.player_id && !r.me && UUID_RE.test(r.player_id)) return el('a', { class: 'pf-lb-row', href: profileHref(r.player_id), 'data-key': `lb-${r.player_id}` }, ...cells);
      return el('div', { class: `pf-lb-row${r.me ? ' me' : ''}` }, ...cells);
    })));
}

function recordsSection(p) {
  const rows = list(p.records).filter((r) => r && r.label);
  if (!rows.length) return null;
  const head = sectionHead('records', 'Records');
  const nodes = rows.map((r) => {
    const cells = [el('span', { class: 'k', text: r.label }),
      r.name ? el('span', { class: 'who' }, teamLogo({ name: r.team, logo_url: r.logo_url, abbrev: r.abbrev }), el('span', { text: r.name })) : el('span'),
      el('b', { class: 'num', text: int(r.value) }), el('span', { class: 'pf-mute num', text: day(r.at) })];
    const label = `${r.label}: ${int(r.value)}${r.name ? `, ${r.name}` : ''}, ${day(r.at)}`;
    return r.game_id
      ? el('a', { class: 'pf-rec', href: gameHref(r.game_id), 'aria-label': `${label}. Open the game` }, ...cells)
      : el('div', { class: 'pf-rec', role: 'group', 'aria-label': label }, ...cells);
  });
  const body = el('div', { class: 'glass pf-recs', id: 'records-rows' }, ...nodes);
  expandable('records-rows', head, nodes, 'records');
  return el('section', { id: 'records', 'aria-labelledby': 'records-h' }, head, body);
}

// ---- Full stats: one table at a time, the top five, "Show all N players", then Total (spec §8.7.5) ----

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

// ---- More stats: the rest of the profile's numbers, kept so nothing a player could see before is lost ----

const oneOr = (n) => (num(n) ? n.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 }) : DASH);
const recordOf = (r) => `${num(r?.wins) ? r.wins : 0}–${num(r?.losses) ? r.losses : 0}`;
const pctOf = (p) => (num(p) ? fmtPct(p) : DASH);
const convOf = (p) => (num(p) ? `${Math.round(p * 100)}%` : DASH);

/** A More stats table. With `cap` ({ id, what }), a long list shows five rows and a "Show all N <what>" row below
 *  them; the same button then reads "Show top 5" (it stays, so focus never falls back to the page top). */
function smallTable(caption, headers, rows, cap = null) {
  const trs = rows.map((cells) => el('tr', {}, ...cells.map((c, i) => {
    const cell = i === 0 ? el('th', { scope: 'row' }) : el('td');
    if (typeof c === 'string') cell.textContent = c; else if (c) cell.append(c);
    return cell;
  })));
  const body = el('tbody', {}, ...trs);
  if (cap && trs.length > SHOWN) {
    const label = el('span');
    const b = el('button', { type: 'button', class: 'pf-showall', 'data-key': `showall-${cap.id}`, 'aria-expanded': 'false' }, label, el('span', { class: 'chev', 'aria-hidden': 'true' }));
    const set = (open) => {
      trs.slice(SHOWN).forEach((r) => { r.hidden = !open; });
      b.setAttribute('aria-expanded', String(open));
      label.textContent = open ? `Show top ${SHOWN}` : `Show all ${trs.length} ${cap.what}`;
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
    const view = box.getBoundingClientRect(), r = t.getBoundingClientRect();
    const sticky = t.closest('th:first-child, td:first-child') ? 0 : (box.querySelector('thead th:first-child')?.getBoundingClientRect().width ?? 0);
    if (r.right > view.right) box.scrollLeft += r.right - view.right + 8;
    else if (r.left < view.left + sticky) box.scrollLeft -= view.left + sticky - r.left + 8;
  });
  // The sticky Player column takes its solid surface only while numbers scroll under it.
  box.addEventListener('scroll', () => box.classList.toggle('scrolled', box.scrollLeft > 0), { passive: true });
  return box;
}

function moreCard(id, title, table, wide = false) {
  const card = el('section', { id, class: `glass pf-more-card${wide ? ' wide' : ''}`, 'aria-labelledby': `${id}-h` }, el('h3', { id: `${id}-h`, text: title }), table);
  return wide ? followFocus(card) : card;
}

// Head to head's "Last": a one-game match ("g:<game id>") shows its score; a series' summed points would read like a
// single score, so it shows its date (as the old profile did).
function lastOf(last) {
  const single = typeof last?.game_id === 'string' && last.game_id.startsWith('g:');
  const s = last?.score;
  return single && Array.isArray(s) && num(s[0]) && num(s[1]) ? `${s[0]}–${s[1]}` : day(last?.at);
}

/** The old profile's Best players (P4-25): the Stats page's six role rows, the portrait over the team colour, the
 *  role's statline when the server sends one, else its one number. */
function playersCard(best, self) {
  const cards = roleCards(best || {});
  if (!cards.some((c) => c.stat)) return null;
  const rows = cards.map((c) => {
    const face = el('span', { class: 'player-face', 'aria-hidden': 'true' });
    if (c.primary && face.style?.setProperty) face.style.setProperty('--tc', c.primary);
    const letter = () => el('span', { class: 'face letter', text: c.initial || '' });
    const src = assetUrl(c.portrait);
    if (src) { const img = el('img', { class: 'face', alt: '', src }); img.addEventListener('error', () => img.replaceWith(letter()), { once: true }); face.append(img); }
    else face.append(letter());
    const badge = assetUrl(c.logo);
    if (badge) { const img = el('img', { class: 'badge', alt: '', src: badge }); img.addEventListener('error', () => img.remove(), { once: true }); face.append(img); }
    const line = el('span', { class: 'pf-pl-line num' });
    if (c.line.length) c.line.forEach((s, i) => { if (i) line.append(el('span', { class: 'pf-dot', 'aria-hidden': 'true', text: '·' })); line.append(el('span', {}, el('b', { text: s.value }), ` ${s.unit}`)); });
    else if (c.stat) line.textContent = c.stat;
    return el('div', { class: `pf-pl${c.stat ? '' : ' none'}` }, face,
      el('span', { class: 'pf-pl-tx' }, el('span', { class: 'pf-pl-head' }, el('span', { class: 'pf-pl-name', text: c.name }), el('span', { class: 'k-label', text: c.label })),
        c.stat ? line : null));
  });
  return moreCard('more-players', self ? 'Your players' : 'Players', el('div', { class: 'pf-pls' }, ...rows));
}

function moreStats(p, self) {
  const cards = [];
  const h2h = list(p.head_to_head).filter(Boolean);
  if (h2h.length) cards.push(moreCard('h2h', 'Head to head', smallTable('Record against every opponent',
    ['Opponent', 'Record', 'PTS', 'Allowed', 'Last', 'Best win', 'Worst loss'], h2h.map((h) => {
      const name = h.display_name || 'Player';
      const inner = [avatar(h.avatar_url, name), el('span', { text: name })];
      const cell = h.player_id && UUID_RE.test(String(h.player_id).toLowerCase())
        ? el('a', { class: 'who', href: profileHref(h.player_id), 'data-key': `h2h-${h.player_id}` }, ...inner) : el('span', { class: 'who' }, ...inner);
      const letter = h.last?.result === 'win' ? 'W' : h.last?.result === 'loss' ? 'L' : '';
      return [cell, recordOf(h), int(h.pts_for), int(h.pts_against), h.last ? `${letter} ${lastOf(h.last)}`.trim() : DASH,
        num(h.biggest_win?.margin) ? `+${h.biggest_win.margin}` : DASH, num(h.biggest_loss?.margin) ? `−${h.biggest_loss.margin}` : DASH];
    }), { id: 'h2h', what: 'opponents' }), true));
  const players = playersCard(p.best_players, self);
  if (players) cards.push(players);
  const teams = list(p.by_team).filter(Boolean);
  if (teams.length) cards.push(moreCard('by-team', 'By team', smallTable('Record by team played as', ['Team', 'Record', 'Pts / gm', 'Win %'],
    teams.map((t) => [who(t.name, t.key === 'pickup' ? { abbrev: 'PU' } : { name: t.name, logo_url: t.logo_url, abbrev: t.abbrev }), recordOf(t), oneOr(t.pts_per_game), pctOf(t.win_pct)]), { id: 'by-team', what: 'teams' })));
  const pg = p.per_game;
  if (pg && pg.games) cards.push(moreCard('per-game', 'Per game', smallTable(`Per game, over ${int(pg.games)} games with play-by-play`, ['Stat', `Per game (${int(pg.games)} games)`], [
    ['Total yards', oneOr(pg.yds)], ['Pass / rush', `${oneOr(pg.pass)} / ${oneOr(pg.rush)}`], ['First downs', oneOr(pg.first_downs)], ['Turnovers', oneOr(pg.turnovers)],
    ['1-pt conversions', convOf(pg.conv1_pct)], ['2-pt conversions', convOf(pg.conv2_pct)], ['Points per drive', oneOr(pg.pts_per_drive)], ['Three-and-outs', oneOr(pg.three_outs)],
    ['Longest drive', num(pg.longest_drive) ? `${pg.longest_drive} plays` : DASH], ['Lead changes', oneOr(pg.lead_changes)], ['Biggest lead', oneOr(pg.biggest_lead)], ['Safeties', int(pg.safeties)]])));
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
  return el('section', { id: 'more', 'aria-labelledby': 'more-h' }, sectionHead('more', 'More stats'), el('div', { class: 'pf-more' }, ...cards));
}

// ---- the page ----

// The links from Stats land here: #passing | #rushing | #receiving | #defense open that table in Full stats; any other
// hash (#career, #records, #history) scrolls to its section.
function jump() {
  const hash = typeof location !== 'undefined' ? decodeURIComponent(location.hash || '').slice(1) : '';
  if (!hash) return;
  if (FULL_STATS[hash] && fs.kinds.includes(hash)) {
    fs.tab = hash;
    fs.render();
    document.getElementById('full-stats')?.scrollIntoView?.({ block: 'start' });
    return;
  }
  document.getElementById(hash)?.scrollIntoView?.({ block: 'start' });
}

function draw(p, ownerId) {
  const self = Boolean(ownerId) && ownerId === String(p.header?.player_id || '').toLowerCase();
  const out = [backButton(), card(p, ownerId)];
  // Nothing public to show: no matches at all, or only private rooms (dropped) and nothing more to load.
  const raw = list(p.recent_matches);
  const onlyHidden = !raw.some(isPublic) && raw.length >= (num(p.matches_total) ? p.matches_total : raw.length);
  if (!(num(p.matches_total) && p.matches_total > 0) || onlyHidden) {
    out.push(el('p', { class: 'pf-none', text: NONE_TEXT }));
    screen.replaceChildren(...out);
    return;
  }
  const ov = overview(p);
  if (ov) out.push(ov);
  out.push(el('div', { class: 'pf-cols' }, historySection(p),
    el('div', { class: 'pf-side' }, ...[careerSection(p), leaderboardSection(p, self), recordsSection(p)].filter(Boolean))));
  out.push(...[fullStatsSection(p), moreStats(p, self)].filter(Boolean));
  screen.replaceChildren(...out);
  jump();
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
// focus, scroll, open sections and the history pages stay as they are. Skipped while the window is hidden or a
// profile request (this one, or Match history loading more) is still out.
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
