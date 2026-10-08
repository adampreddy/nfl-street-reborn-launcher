// The Leaderboard card on the Online page (spec docs/superpowers/specs/2026-10-02-online-page-redesign-design.md §4, §7):
// the ranked top 10 and, when you are further down, your own row after a gap. `leaderboard` answers null while the
// server has no leaderboard (an older backend, matchmaking off) and `enabled: false` while Elo is off: the card then
// says "Rankings start soon." and nothing else. The same answer carries your row (rank, Elo, week's change, plays-as
// team): with `identity_status` (local, no network) it is handed to the mode area (online-mm.js) as an `online-rank`
// event, only when that changed. A row with no team (not ranked, Elo off, a "most played" team) takes the plays-as
// team from one `player_profile` read per page load, as Play reads it (home.js).
// Server text only through el()'s `text`; images only through match-rows.js's avatar() and teamLogo() (the CSP's two
// image prefixes). Polling stops while the page is hidden or a game runs, like Live now.
import { bridge, el, quietCall, reason } from './shell.js';
import { avatar, teamLogo } from './match-rows.js';
import { makePoller } from './live-common.js';
import { boardStanding, leaderboardView } from './online-model.js';
import { playerIdOf, playerLink, profileHref } from './player-link.js';

const POLL_MS = 60000;
const slot = document.getElementById('leaderboard-slot');
const card = el('section', { class: 'glass on-lb', 'aria-label': 'Leaderboard' });
slot?.append(card);
const head = () => el('div', { class: 'sec-head' }, el('h2', { text: 'Leaderboard' }));

/** A row; the name is the link to the player's profile (owner 10-08), and a click anywhere on the row opens it too. */
function row(r) {
  const id = playerIdOf(r.player_id);
  return el('li', { class: `on-lb-row${r.rank !== null && r.rank <= 3 ? ' top' : ''}${r.you ? ' you' : ''}${id ? ' linked' : ''}`,
    onclick: id ? (e) => { if (!e.target?.closest?.('a')) location.href = profileHref(id); } : null },
    el('span', { class: 'on-lb-rank', text: r.rank === null ? '' : String(r.rank) }),
    el('span', { class: 'on-lb-face' }, avatar(r.avatar, r.name), r.team ? teamLogo(r.team) : null),
    el('span', { class: 'on-lb-who' }, playerLink(id, r.name, { cls: 'on-lb-name', key: `lb-card-${id}`, title: r.discord ? `@${r.discord}` : null }), r.record ? el('small', { class: 'num', text: r.record }) : null),
    el('span', { class: 'on-lb-elo' }, r.elo ? el('b', { text: r.elo }) : null, r.change ? el('small', { class: r.change.dir, text: r.change.text }) : null));
}

let lastJson;
function draw(answer) {
  const json = JSON.stringify(answer ?? null);
  if (json === lastJson) return;
  lastJson = json;
  const v = leaderboardView(answer);
  if (v.soon) { card.replaceChildren(head(), el('p', { class: 'on-lb-soon', text: 'Rankings drop soon.' })); return; }
  const h = head();
  if (v.total !== null) h.append(el('span', { class: 'on-lb-total', text: `${v.total} ranked` }));
  const list = el('ol', { class: 'on-lb-rows' }, ...v.rows.map(row));
  if (v.you) list.append(el('li', { class: 'on-lb-gap', 'aria-hidden': 'true', text: '···' }), row(v.you));
  card.replaceChildren(h,
    el('div', { class: 'on-lb-hd', 'aria-hidden': 'true' }, el('span', { text: '#' }), el('span', { text: 'Player' }), el('span', { text: 'Elo' })),
    list,
    // The whole ladder in the launcher (owner 10-08), not the website.
    el('button', { type: 'button', class: 'sec-link on-lb-more', 'data-key': 'full-leaderboard', text: 'Full leaderboard ›',
      onclick: () => { location.href = 'leaderboard.html'; } }));
}

/** A failed read: the last good card stays; with none yet, the header and the failure's own sentence. */
function showError(text) {
  if (lastJson !== undefined) return;
  card.replaceChildren(head(), el('p', { class: 'note on-lb-error', role: 'alert', text }));
}

// Ruling P18: the mode area redraws on every `online-rank`, so the event goes out only when its detail changed.
let lastRank = 'null'; // online-mm.js starts with no rank: nothing to tell until there is one
function tellRank(detail) {
  const json = JSON.stringify(detail);
  if (json === lastRank) return;
  lastRank = json;
  document.dispatchEvent(new CustomEvent('online-rank', { detail }));
}

// Your plays-as team from your public profile: read once per page load (per signed-in player). A failed read is tried
// again on the next beat and keeps the team already shown (undefined).
let profileRead = { id: null, team: null };
async function profileTeam(playerId) {
  if (profileRead.id === playerId) return profileRead.team;
  try {
    const p = await quietCall('player_profile', { playerId });
    const t = p?.header?.plays_as;
    profileRead = { id: playerId, team: t && typeof t === 'object' ? t : null };
    return profileRead.team;
  } catch {
    return undefined;
  }
}

let lastDetail = null;
async function rankDetail(answer, who) {
  if (!who?.signed_in) return null;
  const mine = boardStanding(answer);
  let playsAs = mine?.team ?? null;
  if (!playsAs && typeof who.player_id === 'string' && who.player_id) {
    const t = await profileTeam(who.player_id);
    playsAs = t === undefined ? lastDetail?.playsAs ?? null : t;
  }
  return { elo: mine?.elo ?? null, playsAs };
}

async function poll() {
  const [board, who] = await Promise.allSettled([quietCall('leaderboard'), quietCall('identity_status')]);
  // A failed read (a network blip) tells nothing: the last rank stays. Only a real answer (signed out, Elo off, not
  // ranked, or your row) changes it.
  if (board.status === 'rejected') { showError(reason(board.reason)); throw board.reason; }
  draw(board.value);
  if (who.status === 'fulfilled') {
    lastDetail = await rankDetail(board.value, who.value);
    tellRank(lastDetail);
  }
}

card.replaceChildren(head());
if (bridge) {
  const poller = makePoller(poll, POLL_MS);
  let sessionActive = false;
  const update = () => { if (document.visibilityState === 'hidden' || sessionActive) poller.stop(); else poller.start(); };
  document.addEventListener('visibilitychange', update);
  document.addEventListener('session-state', (e) => { sessionActive = Boolean(e?.detail); update(); });
  update();
} else {
  draw(null);
}
