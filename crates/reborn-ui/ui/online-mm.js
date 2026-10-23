// Matchmaking on the Online page: the mode tiles, the action row, the match in progress and your room
// (spec docs/superpowers/specs/2026-10-02-online-page-redesign-design.md).
//
// Only the look changed in the redesign. Every command, argument, order and poll below is the one this page always
// sent: `mm_state` every second (a read of what the launcher's long-poll already holds in memory, so it never touches
// the network) and one request per action. Which state the mode area shows is decided exactly as before: a live match,
// then a room, then a ticket, then idle (a held match draws idle plus a notice).
import { bridge, call, el, get, quietCall, reason } from './shell.js';
import { seriesOverModel, drawSeriesOver } from './series-over.js';
import { avatar, teamLogo } from './match-rows.js';
import { acceptWithin, clock, eloBits, everyoneReady, heldNotice, inviteLine, inviteLink, modeLine, seekLine, seriesLine, standing, BAN_TEAMS, banStepOpen, banWithin, banLine } from './online-model.js';
import { openPanel } from './menu.js';
import { tabs } from './tabs.js';
import { createSignIn } from './sign-in.js';
import { shownName } from './names.js';
import { playerLink } from './player-link.js';
import { chatBox, drawRoomCard, settleChat } from './room-card.js';

// The relay opens Dolphin by itself when a match is ready, and must open the same runtime, game and
// graphics preset the page's own Play and private rooms use. Those live in page storage, so they are
// reported once here. A launcher without the command simply ignores it.
if (bridge) call('mm_launch_prefs', { runtime: get('runtime'), game: get('game'), preset: get('preset') || 'balanced' }).catch(() => {});

// No field pick anywhere on this page (Adam, 2026-09-22): the game plays on whatever field the
// shipped seed starts on, and the server no longer asks.

// Where a match is, in words a player would use. Anything unlisted reads as "Starting the game".
const RELAY = {
  pairing: 'Match found. Accept to play.',
  accepted: 'Getting ready',
  field: 'Getting ready',
  ready: 'Getting ready',
  go: 'Getting ready',
  host_launching: 'Starting the game',
  code_ready: 'Starting the game',
  joining: 'Starting the game',
  room_formed: 'Connecting you both',
  running: 'Playing',
  intermission: 'Between games',
  complete: 'Match over',
  held: 'Held for review',
  void: 'Match cancelled',
  abandoned: 'Match cancelled',
};

const screen = document.getElementById('screen');
const section = el('section', { class: 'mm', 'aria-label': 'Matchmaking' });
// The mode area's slot (online.html, spec 2026-10-02 §2); a page without one still gets the panel above #screen.
const slot = document.getElementById('mode-slot');
if (slot) slot.append(section); else screen?.parentNode?.insertBefore(section, screen);

// While matchmaking has the player queued, in a lobby or in a match, the private-room section below
// is hidden: a player cannot be in two sessions, and offering "Join a game" under "vs Jock" invites a
// click that the launch guard would only refuse. Done with a page attribute and one scoped rule so
// online.js - and the checker pinned to it - is never touched. !important because pages/online.css shows #screen
// with `.on-direct #screen{display:block}`, which outranks this rule's selector; without it the attribute would be
// set and the section would stay on screen.
document.head.append(el('style', { text: 'body[data-mm-busy="true"] #screen{display:none!important}' }));
function markBusy(busy) {
  if (busy) document.body.dataset.mmBusy = 'true';
  else delete document.body.dataset.mmBusy;
}

const store = {
  get(k, d) { try { return localStorage.getItem(k) ?? d; } catch { return d; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* private mode: fine */ } },
};

let busy = false;
let lastDrawn = '';
// The public snapshot of the series that just finished, for its per-game scores (read once per match).
let overLive = { id: null, snap: null };
function loadOverLive(id) {
  if (overLive.id === id) return;
  overLive = { id, snap: null };
  quietCall('live_match', { matchId: id }).then((s) => { if (overLive.id === id) { overLive.snap = s; lastDrawn = ''; draw(); } }).catch(() => {});
}
const status = el('p', { class: 'settings-status on-status', role: 'status' });

// The post-game screens put away with Back to Online (or left by Play again / Rematch), by match id, so the server
// still showing that finished match on every poll never brings the screen back (Adam, 2026-10-04: "go back to general
// online screen from the post game screen"). Kept for this session, so another tab and back stays on Online; a box score
// opened from the screen and closed again comes back to it, as nothing was put away.
const overDone = {
  has(id) { try { return JSON.parse(sessionStorage.getItem('mm:over-done') || '[]').includes(id); } catch { return overMemo.has(id); } },
  add(id) { overMemo.add(id); try { const v = JSON.parse(sessionStorage.getItem('mm:over-done') || '[]'); if (!v.includes(id)) sessionStorage.setItem('mm:over-done', JSON.stringify([...v, id].slice(-20))); } catch { /* memo holds it */ } },
};
const overMemo = new Set();
function leaveOver(id) {
  overDone.add(id);
  overShown = null;
  lastDrawn = '';
  draw();
}
// The match whose screen is up, so the first draw of it can put a controller on Play again.
let overShown = null;

function mode() { return store.get('mm:mode', 'casual'); }

const links = document.getElementById('on-links');
// `reserve`: hidden but keeping its line, so the idle cards are the same size whichever card is picked (Adam, 2026-10-02:
// picking Private room must not change the cards).
function setLinks(show, reserve = false) {
  if (!links) return;
  links.hidden = !show && !reserve;
  links.classList.toggle('reserve', !show && reserve);
}

// Private room is the third pick, held in page state only: `mm:mode` keeps ranked|casual, so the queue and the room
// commands get exactly the mode they always got (spec §3.1).
let privatePicked = false;
const picked = () => (privatePicked ? 'private' : mode());
function pick(value) {
  if (value === 'private') privatePicked = true;
  else { privatePicked = false; store.set('mm:mode', value); }
  lastDrawn = '';
  draw();
}

// Your rank, Elo and plays-as team (the leaderboard's row of yours, else the profile's plays-as), from leaderboard.js.
// Null until it answers; `elo` is null whenever the server sends none (Elo off): every spot that needs it is then
// hidden (spec §7).
let rankInfo = null;
document.addEventListener('online-rank', (e) => { rankInfo = e.detail ?? null; lastDrawn = ''; draw(); });
// A controller drives (spec §6): the pick follows focus across the tiles, so the D-pad onto Casual picks Casual.
section.addEventListener('focusin', (e) => {
  if (!document.body.classList.contains('gamepad-input')) return;
  const tile = e.target.closest?.('.on-tile');
  const key = tile?.dataset.key?.replace(/^mode-/, '');
  if (key && !tile.disabled && key !== picked()) pick(key);
});
// B / Esc: Cancel while searching, Decline on Match found, Back to Online on the post-game screen; anywhere else B goes
// on to the sidebar as always.
document.addEventListener('pad-back', (e) => {
  const target = section.querySelector('[data-key="cancel"], [data-key="decline"], [data-key="over-back"]');
  if (target && !target.disabled && !busy) { e.preventDefault(); target.click(); }
});

// Who you are, for your own picture on the matchup card: Play caches the Discord picture under `reborn:avatar`
// (home.js) keyed by player id; any other id or no cache draws your letter.
let me = null;
if (bridge) call('identity_status').then((v) => { if (v?.signed_in) { me = { id: v.player_id, name: shownName(v) }; lastDrawn = ''; } }).catch(() => {});

// The team you play most, shown while you search (Adam, 2026-10-03: "tells the user what team the user uses the most
// but make sure it's not like its their favorite team"). From your public profile's `most_used_team` (online games),
// else its `plays_as`; neither, and the line is left out. Read once per page load, the first time you search.
let mostPlayed = { asked: false, team: null };
function readMostPlayed() {
  if (mostPlayed.asked || !me?.id) return;
  mostPlayed.asked = true;
  quietCall('player_profile', { playerId: me.id }).then((p) => {
    const h = p?.header;
    const t = [h?.most_used_team, h?.plays_as].find((x) => x && typeof x === 'object' && typeof x.name === 'string' && x.name);
    if (t) { mostPlayed.team = t; lastDrawn = ''; draw(); }
  }).catch(() => {});
}
function myAvatarUrl() {
  try { const v = JSON.parse(localStorage.getItem('reborn:avatar') || 'null'); return v && me && v.id === me.id ? v.url : null; } catch { return null; }
}
// The relay is moving (accepted … running): one red line along the bottom of the card (spec §3.5).
const MOVING = new Set(['accepted', 'field', 'ready', 'go', 'host_launching', 'code_ready', 'joining', 'room_formed', 'running', 'intermission']);

async function act(cmd, args) {
  if (busy) return false;
  busy = true;
  status.textContent = '';
  section.setAttribute('aria-busy', 'true');
  let ok = false;
  try {
    await call(cmd, args);
    ok = true;
  } catch (e) {
    // The server's own sentence, verbatim, is always the most useful thing to show.
    status.textContent = reason(e);
  } finally {
    busy = false;
    section.removeAttribute('aria-busy');
    lastDrawn = '';
    await draw();
  }
  return ok;
}

// The components online play needs to be current, the same list as the manual Online tab's.
const ONLINE_PARTS = ['runtime', 'roster', 'game-patch', 'online-content'];
const pause = (ms) => new Promise((r) => setTimeout(r, ms));

// Before a queue: the update-before-room walk the manual Online tab does (online.js:71), then the
// image repair. 09-26: a player's game-patch component was current while his image was still an
// old build, so every online copy he built differed from everyone else's and the match was held
// as "content-mismatch" with nothing either player could do about it from the page.
async function queueWhenCurrent() {
  if (busy) return;
  busy = true;
  section.setAttribute('aria-busy', 'true');
  try {
    let c = await call('content_check', { depth: null });
    const needed = () => (c?.components || []).some((x) => ONLINE_PARTS.includes(x.component) && x.state === 'available');
    for (let waited = 0; waited < 1800; waited++) {
      if (['checking', 'fetching', 'installing'].includes(c?.phase)) {
        status.textContent = c.phase === 'checking' ? 'Checking for updates before going online…' : (c.note || 'Updating before going online…');
        await pause(1000);
        c = await call('content_check', { depth: null });
        continue;
      }
      if (c?.phase === 'available' && needed()) {
        status.textContent = `Downloading the update online play needs (${Math.ceil((c.download_bytes || 0) / 1048576)} MB)…`;
        c = await call('content_fetch');
        if (c?.phase === 'available') throw new Error(c.note || 'The download did not start.');
        continue;
      }
      if (c?.phase === 'staged' && needed()) {
        status.textContent = 'Installing the update…';
        c = await call('content_promote', { confirm: true });
        if (c?.phase === 'problem') throw new Error(c.note);
        continue;
      }
      break;
    }
    status.textContent = 'Checking your game files…';
    const repair = await call('content_repair_image', {});
    if (!repair?.current) throw new Error(repair?.note || 'Your game files are not up to date.');
    status.textContent = '';
    // Casual sends its Quick game / Pickup pick; ranked never sends one (always Quick games).
    if (mode() === 'casual') { queuedKind = queueKind(); await call('mm_queue', { mode: 'casual', seedKind: queuedKind }); }
    else await call('mm_queue', { mode: mode() });
  } catch (e) {
    status.textContent = reason(e);
  } finally {
    busy = false;
    section.removeAttribute('aria-busy');
    lastDrawn = '';
    await draw();
  }
}

// While queueWhenCurrent works, the Find button says what it is doing: the status lines it already writes (spec §3.1).
new MutationObserver(() => {
  const find = section.querySelector('[data-key="find"]');
  if (find && busy && status.textContent) find.textContent = status.textContent;
}).observe(status, { childList: true, characterData: true, subtree: true });

// The chat box and the room card live in room-card.js (shared with the League tab's game day).

/** Put `nodes` in `parent` in order, MOVING only what is out of place: a node already there in the right order (the
 *  chat box, the status line) is never detached, so it keeps its scroll position and focus. replaceChildren() detaches
 *  everything, which is what scrolled the chat back to the top. */
function placeChildren(parent, nodes) {
  const keep = new Set(nodes);
  for (const c of [...parent.childNodes]) if (!keep.has(c)) c.remove();
  let ref = parent.firstChild;
  for (const n of nodes) {
    if (n === ref) { ref = ref.nextSibling; continue; }
    parent.insertBefore(n, ref);
  }
}

// ---- the mode area (spec §3.1) ----
// No pictures yet (Adam, 2026-10-02): each tile's `.on-art` layer is a plain colour (pages/online.css). Real art goes
// into that same layer later (a background image, or an <img> inside it) without moving anything else.
const MODES = [
  { key: 'ranked', name: 'Ranked', line: 'First to two wins. Climb the board.' },
  { key: 'casual', name: 'Casual', line: 'No Elo, no pressure.' },
  { key: 'private', name: 'Private Room', line: 'Your crew, your code.' },
];

function tileClick(key) {
  if (picked() !== key) { pick(key); return; }
  // A on the picked tile (a controller drives): the action row's primary (spec §6). A mouse click on it does nothing.
  if (document.body.classList.contains('gamepad-input')) section.querySelector('.on-row [data-primary]')?.click();
}

/** The tiles. `searching`: the ticket's mode keeps colour and its pill, the others go dark and cannot be picked.
 *  `locked`: behind the sign-in panel (no counts, no rank, not reachable). `only`: just that tile (matchmaking off). */
function tiles(snap, { searching = null, locked = false, only = null } = {}) {
  const depth = snap?.queue_depth ?? null;
  const on = only ?? searching ?? picked();
  const bits = eloBits(rankInfo?.elo);
  const box = el('div', { class: `on-modes${only ? ' solo' : ''}`, 'data-nav': 'column', role: 'group', 'aria-label': 'Game modes' });
  if (locked) box.setAttribute('inert', '');
  for (const m of only ? MODES.filter((x) => x.key === only) : MODES) {
    const isOn = m.key === on;
    let pill = null;
    if (searching !== null && isOn) pill = el('span', { class: 'on-pill seek' }, el('i', { 'aria-hidden': 'true' }), seekLine(m.key, rankInfo?.elo));
    else if (searching === null && !locked && m.key !== 'private' && typeof depth?.[m.key] === 'number') {
      pill = el('span', { class: 'on-pill' }, el('i', { 'aria-hidden': 'true' }), `${depth[m.key]} searching`);
    }
    const rank = m.key === 'ranked' && !locked && (bits.rank || bits.elo)
      ? el('span', { class: 'on-rank' },
        bits.rank ? el('b', { class: 'num', text: bits.rank }) : null,
        bits.elo ? el('span', { class: 'num', text: bits.elo }) : null,
        bits.week ? el('span', { class: `on-week ${bits.week.dir}`, text: bits.week.text }) : null)
      : null;
    box.append(el('button', {
      type: 'button', class: `on-tile on-tile-${m.key}${isOn ? ' on' : ''}${searching !== null && !isOn ? ' dim' : ''}`,
      'data-key': `mode-${m.key}`, 'aria-pressed': String(isOn), disabled: searching !== null ? true : null,
      onclick: () => tileClick(m.key),
    },
    el('span', { class: 'on-art', 'aria-hidden': 'true' }),
    pill,
    el('span', { class: 'on-lab' },
      el('span', { class: 'on-tx' }, el('span', { class: 'on-name', text: m.name }), el('span', { class: 'on-line', text: m.line })),
      rank)));
  }
  return box;
}

// The two games a casual match or a room plays: Quick game (the `regular` seed) or Pickup.
const KINDS = { 'Quick game': 'regular', Pickup: 'pickup' };
const KIND_NAME = { regular: 'Quick game', pickup: 'Pickup' };
const kindName = (k) => KIND_NAME[k] ?? KIND_NAME.regular;

// Casual's pick (Adam, 2026-10-03: "make casual able to queue pickup and quick game"), remembered on this computer.
const queueKind = () => (store.get('mm:kind', 'regular') === 'pickup' ? 'pickup' : 'regular');
// What the last casual queue asked for, so the searching bar can name it when the server does not echo it.
let queuedKind = null;

function findRow(p) {
  if (p === 'ranked') {
    return el('div', { class: 'on-row' },
      el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'find', 'data-primary': 'true', text: 'Find a ranked game', onclick: () => findRanked() }),
      el('p', { class: 'on-note' }, 'Matched to your level. ',
        el('button', { type: 'button', class: 'sec-link on-rules-link', 'data-key': 'ranked-rules', text: 'Ranked rules ›', onclick: () => rankedRules() })));
  }
  const kind = tabs('mm-queue-kind', Object.keys(KINDS), kindName(queueKind()),
    (name) => { store.set('mm:kind', KINDS[name]); lastDrawn = ''; draw(); }, { label: 'Quick game or Pickup' });
  return el('div', { class: 'on-row on-casual' },
    kind,
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'find', 'data-primary': 'true', text: 'Find a casual game', onclick: () => queueWhenCurrent() }),
    el('p', { class: 'on-note on-casual-note', text: 'Best of 3. No Elo on the line.' }));
}

// ---- the ranked rules (Adam, 2026-10-03: "a notice for the format and rules of ranked games before you queue") ----
// Shown the first time Find a ranked game is pressed on this computer, and any time from "Ranked rules ›".
const RULES_SEEN = 'mm:ranked-rules-seen';
function findRanked() {
  if (store.get(RULES_SEEN, '') === '1') queueWhenCurrent();
  else rankedRules(true);
}
function rankedRules() {
  const dialog = el('dialog', { class: 'on-rules', 'aria-labelledby': 'on-rules-title' });
  const close = () => dialog.close();
  const go = el('button', { type: 'button', class: 'btn primary', 'data-key': 'rules-go', text: 'Got it, find a game',
    onclick: () => { store.set(RULES_SEEN, '1'); close(); queueWhenCurrent(); } });
  dialog.append(
    el('h2', { id: 'on-rules-title', text: 'Ranked rules' }),
    el('p', { class: 'on-rules-lead', text: 'Read this once, then go get your wins.' }),
    el('h3', { text: 'Format' }),
    el('ul', {},
      el('li', { text: 'Best of 3. First to two wins takes the match.' }),
      el('li', { text: 'Every game is played in full.' })),
    el('h3', { text: 'Rules' }),
    el('ul', {},
      el('li', { text: 'Play it out. Leaving a ranked match early can count as a forfeit.' }),
      el('li', { text: 'When the match is decided, the launcher closes the game for you.' })),
    el('div', { class: 'on-rules-acts' }, go,
      el('button', { type: 'button', class: 'btn quiet', 'data-key': 'rules-cancel', text: 'Cancel', onclick: close })));
  dialog.addEventListener('close', () => dialog.remove());
  document.body.append(dialog);
  dialog.showModal();
  go.focus();
}

// The code field's text survives the redraws (the field itself is rebuilt).
let roomCode = '';
// Host a room makes a Quick game room; the host switches it to Pickup from inside the room (Adam, 2026-10-03: "users
// can change quick game vs pickup game after they've made the room but not before").
function privateRow() {
  const code = el('input', { type: 'text', class: 'on-code', maxlength: 12, placeholder: 'Join with a code', 'aria-label': 'Join with a code', spellcheck: 'false', autocomplete: 'off', 'data-key': 'room-code', value: roomCode });
  const join = el('button', { type: 'submit', class: `btn on-join${roomCode.trim() ? '' : ' quiet'}`, 'data-key': 'join', 'aria-label': 'Join by code', text: 'Join' });
  code.addEventListener('input', () => { roomCode = code.value; join.classList.toggle('quiet', !code.value.trim()); });
  const form = el('form', { class: 'on-codeform' }, code, join);
  form.addEventListener('submit', (e) => { e.preventDefault(); const c = code.value.trim(); if (c) act('mm_room_join', { code: c }); });
  // A private room is a casual game, joined by its code only (no public lobbies, Adam 2026-10-03).
  return el('div', { class: 'on-row on-private' },
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'host', 'data-primary': 'true', text: 'Host a room',
      onclick: () => act('mm_room_create', { mode: 'casual' }) }),
    form);
}

function drawIdle(snap) {
  const p = picked();
  return [tiles(snap), p === 'private' ? privateRow() : findRow(p)];
}

// ---- signed out, offline, matchmaking off, a held match (spec §3.8, §3.9) ----
// The same Discord device-code flow as Stats (sign-in.js). Once it is done the launcher's own poll sees the account.
const signIn = createSignIn({ onChange: () => { lastDrawn = ''; draw(); }, onDone: () => { lastDrawn = ''; draw(); } });

/** One panel over the blurred, unreachable tiles. `withButton`: Sign in with Discord (and, while it waits, the code). */
function lockPanel(title, text, withButton) {
  const si = signIn.state;
  const parts = withButton && si.active
    ? [el('b', { text: 'Waiting for Discord' }),
      el('p', { role: 'status', text: `Your code is ${si.userCode}` }),
      si.verifyUrl ? el('p', { text: `Go to ${si.verifyUrl} to approve it.` }) : null,
      el('button', { type: 'button', class: 'btn', 'data-key': 'signin-cancel', text: 'Cancel', onclick: () => signIn.cancel() })]
    : [el('b', { text: title }),
      el('p', { text }),
      withButton ? el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'signin', 'data-primary': 'true',
        text: si.starting ? 'Opening Discord…' : 'Sign in with Discord', disabled: si.starting, onclick: () => signIn.start() }) : null,
      // A waiting invite can be put away without signing in (it is cleared, as Dismiss does).
      withButton && invite ? el('button', { type: 'button', class: 'btn quiet', 'data-key': 'invite-notnow', text: 'Not now', onclick: () => forgetInvite() }) : null,
      withButton && si.message ? el('p', { class: 'refusal', role: 'alert', text: si.message }) : null];
  return el('div', { class: 'on-locked' }, tiles(null, { locked: true }),
    el('div', { class: 'on-lock', role: 'region', 'aria-label': title }, ...parts.filter(Boolean)));
}

/** Matchmaking off: the private play that works without it is the direct connection (ruling 7). */
function directRow() {
  return el('div', { class: 'on-row' },
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'direct', 'data-primary': 'true', text: 'Direct connection',
      onclick: () => document.getElementById('direct-open')?.click() }),
    el('p', { class: 'on-note', text: 'Send the code to your crew' }));
}

// A held match is over for the player the moment it is held (seats released), but the server keeps showing it for a
// few minutes: one notice above the normal page, never the page itself (09-26). Dismiss hides it for this match.
let dismissedHold = null;
function heldBox(m) {
  const n = heldNotice(m);
  return el('div', { class: 'partial on-notice', role: 'status' },
    el('div', {}, el('b', { text: n.title }), el('span', { text: n.text })),
    el('button', { type: 'button', class: 'btn quiet sm', 'data-key': 'held-dismiss', text: 'Dismiss',
      onclick: () => { dismissedHold = m.id; lastDrawn = ''; draw(); } }));
}

// ---- a room invite (spec §3.10) ----
// A `nflstreetreborn://room/<code>` link left its code behind (mm_pending_room). On load and on window focus the page
// picks Private room, fills the code and offers one line with Join; it never joins on its own. Join, Dismiss or (signed
// out) Not now clears it; anything else (signed out, busy) leaves it waiting. The banner is drawn only while idle, so a
// queued, matched or roomed player never sees it and the invite waits.
// A lobby code fits the room-code field (maxlength 12); anything else is no invite (the launcher keeps up to 64).
const INVITE_CODE = /^[A-Za-z0-9-]{1,12}$/;
let invite = null; // { code, key, host }
// Invites put away this session (`code|at`), kept even when clearing the file failed, so a focus re-read cannot bring
// one back; and codes the server already said it knows no host for, so focus does not ask again.
const session = {
  has(k) { try { return JSON.parse(sessionStorage.getItem('mm:invites-done') || '[]').includes(k); } catch { return memo.has(k); } },
  add(k) { memo.add(k); try { const v = JSON.parse(sessionStorage.getItem('mm:invites-done') || '[]'); if (!v.includes(k)) sessionStorage.setItem('mm:invites-done', JSON.stringify([...v, k].slice(-20))); } catch { /* memo holds it */ } },
};
const memo = new Set();
const noHost = new Set();
async function takeRoomInvite() {
  if (!bridge) return;
  try {
    const p = await call('mm_pending_room');
    if (!p || typeof p.code !== 'string' || !INVITE_CODE.test(p.code)) return;
    const key = `${p.code}|${typeof p.at === 'string' ? p.at : ''}`;
    if (memo.has(key) || session.has(key)) return;
    if (invite?.key !== key) {
      invite = { code: p.code, key, host: null };
      privatePicked = true;
      roomCode = p.code;
      lastDrawn = '';
      draw();
    }
    // Asked again on focus until it answers: a signed-out player's lookup fails, and the name comes after sign-in.
    // A clear "no host" (none or a 404) is asked once per code.
    if (invite.host || noHost.has(p.code)) return;
    const found = await call('mm_room_lookup', { code: p.code }).then((v) => ({ v }), () => null);
    if (!found || invite?.key !== key) return;
    if (found.v && typeof found.v.host_name === 'string' && found.v.host_name) {
      invite.host = found.v.host_name;
      lastDrawn = '';
      draw();
    } else noHost.add(p.code);
  } catch { /* an invite that cannot be read is no invite */ }
}
async function forgetInvite() {
  if (!invite) return;
  session.add(invite.key);
  invite = null;
  await call('mm_pending_room_clear').catch(() => {});
  // Ruling P13: the banner goes at once, not on the next change the server reports.
  lastDrawn = '';
  draw();
}
function inviteBanner() {
  const line = inviteLine(invite?.host, invite?.code);
  if (!line) return null;
  const code = invite.code;
  return el('div', { class: 'glass on-invite', role: 'status' },
    el('p', {}, line.who ? el('b', { text: line.who }) : null, line.rest),
    el('button', { type: 'button', class: 'btn primary sm', 'data-key': 'invite-join', text: 'Join',
      onclick: async () => { if (await act('mm_room_join', { code })) await forgetInvite(); } }),
    el('button', { type: 'button', class: 'btn quiet sm', 'data-key': 'invite-dismiss', text: 'Dismiss', onclick: () => forgetInvite() }));
}
takeRoomInvite();
window.addEventListener('focus', () => takeRoomInvite());

// The queue timer counts on this page, every second. The snapshot's `waited_ms` only changes when
// the server has something new to say (a long poll can sit for 25 s), so drawing it as-is froze the
// timer (Adam, 2026-09-23: "The timer doesn't move"). It is anchored once per ticket, on the
// server's figure, and then advanced by this page's own clock.
let queueClock = { key: null, base: 0, at: 0 };
const queuedMs = () => queueClock.base + (Date.now() - queueClock.at);
// The same tick counts Match found's "Accept within" down to the server's relay_deadline (kept on the element: a redraw
// happens only when the snapshot changes) and hides it once the deadline has come.
setInterval(() => {
  const timer = section.querySelector('.mm-timer');
  if (timer && queueClock.key) timer.textContent = `Searching ${clock(queuedMs())}`;
  const accept = section.querySelector('.on-tm[data-deadline]');
  if (accept) {
    const left = acceptWithin(accept.dataset.deadline, Date.now());
    accept.hidden = !left;
    if (left) accept.textContent = left;
  }
  const ban = section.querySelector('.on-tm[data-ban-deadline]');
  if (ban) {
    const left = banWithin(ban.dataset.banDeadline, Date.now());
    ban.hidden = !left;
    if (left) ban.textContent = left;
  }
}, 1000);

function drawQueued(t) {
  if (queueClock.key !== t.enqueued_at) queueClock = { key: t.enqueued_at, base: t.waited_ms || 0, at: Date.now() };
  const ranked = t.mode === 'ranked';
  readMostPlayed();
  // Casual names its game: the ticket's own kind when the server says, else what this page asked for.
  const kind = ranked ? null : kindName(t.seed_kind ?? queuedKind ?? queueKind());
  const team = mostPlayed.team;
  return [
    tiles(null, { searching: ranked ? 'ranked' : 'casual' }),
    el('div', { class: 'on-row' },
      el('div', { class: 'on-search' },
        el('span', { class: 'mm-timer on-timer num', text: `Searching ${clock(queuedMs())}` }),
        el('span', { class: 'on-search-mode', text: ranked ? 'Ranked' : `Casual · ${kind}` }),
        el('span', { class: 'sweep', 'aria-hidden': 'true' })),
      el('button', { type: 'button', class: 'btn on-cancel', 'data-key': 'cancel', text: 'Cancel', onclick: () => act('mm_leave_queue') })),
    team ? el('p', { class: 'on-note on-most' }, 'Your most-played team: ', teamLogo(team, 'on-most-logo'), el('b', { text: team.name })) : null,
  ].filter(Boolean);
}

// Your room (spec §3.7): the code to share, who is in and ready, chat; Start for the host once everyone is ready.
function drawRoom(r, chat) {
  // The game the room plays (the snapshot's seed_kind; an older server sends none: Quick game). Everyone reads it in the
  // title; the host can switch it until the game starts (the server un-readies everyone, so the crew re-confirms).
  const kind = kindName(r.seed_kind);
  const started = typeof r.status === 'string' && r.status !== '' && r.status !== 'open';
  // A League edition room (Plan 3b) has no Quick game | Pickup switch (the server refuses one) and its title says what
  // it is: a fixture's game by its week, or a friendly by its rules. The backend names retail rooms 'retail'.
  const leagueGame = r.league && typeof r.league === 'object' ? r.league : null;
  const leagueRoom = r.edition === 'league';
  const kindControl = r.you_host === true && !leagueRoom
    ? el('div', { class: 'on-kindrow' }, el('span', { class: 'k-label', text: 'Game' }),
      tabs('mm-room-kind', Object.keys(KINDS), kind, (name) => changeKind(r.id, KINDS[name]), { label: 'Quick game or Pickup', disabled: started }))
    : null;
  const title = leagueGame ? `League game · Week ${leagueGame.week ?? '?'}` : leagueRoom ? 'League friendly' : `Your room · ${kind}`;
  const sub = leagueGame ? 'League rules · Best of 3'
    : leagueRoom ? (r.ruleset === 'raw' ? 'Raw game' : 'League settings')
    : r.mode === 'ranked' ? 'Ranked' : null;
  return [el('div', { class: 'on-room' }, ...drawRoomCard(r, chat, { act, title, sub, kindControl }))];
}

// The host switches the room's game. A server without the route (before the backend update) refuses it: one quiet
// sentence, and the room carries on as it is.
async function changeKind(roomId, seedKind) {
  if (busy) return;
  busy = true;
  status.textContent = '';
  section.setAttribute('aria-busy', 'true');
  try {
    await call('mm_room_action', { roomId, action: 'kind', value: seedKind });
  } catch (e) {
    const why = reason(e);
    status.textContent = /not found|404|isn't something/i.test(why)
      ? "Switching the game isn't on the servers yet. Leave and host a new room to change it."
      : why;
  } finally {
    busy = false;
    section.removeAttribute('aria-busy');
    lastDrawn = '';
    await draw();
  }
}

function face({ url, name, discord = null, sub, team, tick = false, id = null, key = null }) {
  return el('div', { class: 'on-p' },
    el('span', { class: 'on-avw' }, avatar(url, name, 'on-av'), tick ? el('span', { class: 'on-ok', role: 'img', 'aria-label': 'Accepted', text: '✓' }) : null),
    // The nickname (a link to their profile, owner 10-08); when the player set one, its Discord name leads the small
    // line under it (the face's rows are fixed, so it shares the standing's row rather than adding one) and shows on hover.
    playerLink(id, name, { cls: 'on-pname', key, title: discord ? `@${discord}` : null }),
    sub || discord ? el('span', { class: 'on-psub num', text: [discord ? `@${discord}` : null, sub].filter(Boolean).join(' · ') }) : null,
    team ? teamLogo(team, 'on-plogo') : null);
}

// A 3-4 player private room's match: who else is in the game, under the VS. Null for two players (the faces say it).
function crewLine(m) {
  const party = Array.isArray(m.party) ? m.party : [];
  if (party.length < 3) return null;
  const others = party.filter((p) => !p.you).map((p) => p.name || 'Player');
  return el('span', { class: 'on-crew', text: `${party.length} players · with ${others.join(', ')}` });
}

// Ranked team bans: one tap bans a team (tap another to change it) until both have picked or the 20 s run out. Your
// opponent's ban is hidden until then; a team nobody banned stays in team select.
function banPicker(m) {
  const foot = el('div', { class: 'on-vs-foot on-ban' },
    el('span', { class: 'k-label', text: 'Ban a team' }));
  const grid = el('div', { class: 'on-ban-grid', role: 'group', 'aria-label': 'Ban a team' });
  for (const t of BAN_TEAMS) {
    const on = m.your_ban === t.id;
    grid.append(el('button', {
      type: 'button', class: `on-ban-team${on ? ' on' : ''}`, 'aria-pressed': on ? 'true' : 'false', 'data-key': `ban-${t.id}`,
      onclick: () => act('mm_match_action', { matchId: m.id, action: 'ban', value: t.id }),
    }, teamLogo(t, 'on-ban-logo'), el('span', { text: t.name })));
  }
  foot.append(grid);
  const left = banWithin(m.relay_deadline, Date.now());
  if (left) foot.append(el('span', { class: 'on-tm num', 'data-ban-deadline': m.relay_deadline, text: left }));
  return foot;
}

// The matchup card (spec §3.3–§3.6). Only called for a match that is not over (complete, void, abandoned, held).
function drawMatch(m, chat, relayError, snap) {
  const words = RELAY[m.relay_state] ?? 'Starting the game';
  const opp = m.opponent || 'your opponent';
  const pairing = m.relay_state === 'pairing';
  // Ruling P9: between games of a series is still "In play", with the series line.
  const playing = m.relay_state === 'running' || m.relay_state === 'intermission';
  const series = playing ? seriesLine(m) : null;
  const mid = el('div', { class: 'on-mid' },
    el('span', { class: 'k-label on-found', text: playing ? 'In play' : 'Game on' }),
    el('span', { class: 'on-x', 'aria-hidden': 'true', text: 'VS' }),
    series ? el('span', { class: 'on-ser num', text: series }) : null,
    el('span', { class: 'on-mode-line', text: modeLine(m.mode, m.best_of) }),
    crewLine(m));
  // Accept/Decline and the timer sit in their own grid cell under the VS (launch audit H1): beside the faces at full
  // size, on a row of their own under both faces below 1000 px, so they never cover anyone's Elo line.
  let foot = null;
  if (pairing && !m.your_accepted) {
    foot = el('div', { class: 'on-vs-foot' });
    foot.append(el('div', { class: 'on-acts' },
      el('button', { type: 'button', class: 'btn primary', 'data-key': 'accept', 'data-primary': 'true', text: 'Accept', onclick: () => act('mm_match_action', { matchId: m.id, action: 'accept', value: true }) }),
      // Ruling P8: Decline is the quiet button (spec §3.3).
      el('button', { type: 'button', class: 'btn quiet', 'data-key': 'decline', text: 'Decline', onclick: () => act('mm_match_action', { matchId: m.id, action: 'accept', value: false }) })));
    const left = acceptWithin(m.relay_deadline, Date.now());
    if (left) foot.append(el('span', { class: 'on-tm num', 'data-deadline': m.relay_deadline, text: left }));
  } else if (pairing) {
    mid.append(el('span', { class: 'on-st' }, el('i', { 'aria-hidden': 'true' }), `You're in. Waiting on ${opp}…`));
  } else if (banStepOpen(m)) {
    foot = banPicker(m);
  } else if (m.relay_state !== 'running') {
    mid.append(el('span', { class: 'on-st' }, el('i', { 'aria-hidden': 'true' }), words), el('span', { class: 'on-tm', text: 'Your game opens on its own' }));
  }
  const bans = banLine(m);
  if (bans) mid.append(el('span', { class: 'on-tm on-bans', text: bans }));
  // The relay's own failure (the room would not open, the code would not send) in its own words.
  if (relayError) mid.append(el('p', { class: 'refusal', text: relayError }));
  const mine = eloBits(rankInfo?.elo);
  const card = el('section', { class: 'on-vs', 'aria-label': `${modeLine(m.mode, m.best_of)} against ${opp}` },
    el('span', { class: 'on-art', 'aria-hidden': 'true' }),
    el('button', { type: 'button', class: 'btn quiet sm on-leave', 'data-key': 'leave-match', text: 'Leave match', onclick: () => act('mm_match_action', { matchId: m.id, action: 'leave', value: null }) }),
    el('div', { class: 'on-vs-in on-vs-match' },
      face({ url: myAvatarUrl(), name: snap?.player?.name || me?.name || 'You', discord: snap?.player?.discord ?? null, sub: [mine.rank, mine.elo].filter(Boolean).join(' · '),
        id: snap?.player?.player_id ?? me?.id ?? null, key: 'vs-you',
        team: rankInfo?.playsAs ?? null, tick: !!m.your_accepted && !playing }),
      mid,
      // On the card itself a nameless opponent is "Opponent" (letter O); sentences keep "your opponent".
      // A 3rd or 4th player's card faces the room's host (the crew line under the VS names everyone else).
      face({ url: m.opponent_avatar_url ?? null, name: m.opponent || 'Opponent', discord: m.opponent_discord ?? null, sub: m.seat >= 2 ? 'Host' : standing(m.opponent_rank, m.opponent_elo), team: m.opponent_team ?? null,
        id: m.opponent_id ?? null, key: 'vs-opponent' }),
      foot),
    MOVING.has(m.relay_state) ? el('span', { class: 'sweep', 'aria-hidden': 'true' }) : null);
  // Match chat stays, under the card (nothing that exists is removed; plan ruling 15).
  return [card, chatBox(`match:${m.id}`, chat, (t) => act('mm_match_action', { matchId: m.id, action: 'chat', value: t }))];
}

// ---- players online and the presence switch, in the heading (spec §2) ----
const playersBtn = document.getElementById('on-players');
let lastOnline = { count: null, players: [] };
let presenceOn = false;
const ACTIVITY = { idle: 'Online', queued: 'Looking for a game', in_lobby: 'In a room', in_match: 'In a match' };

let panelEl = null;
let panelSig = '';

/** The heading button: "{n} players online" when the server counted, "Discord status" otherwise (the switch is still there,
 *  but nothing is invented, spec §7). Present wherever the presence switch was; hidden only by hideOnline(). */
function showOnline(snap, on) {
  presenceOn = !!on;
  const online = snap?.online;
  const has = !!online && typeof online === 'object';
  lastOnline = { count: has && typeof online.count === 'number' ? online.count : null, players: has && Array.isArray(online.players) ? online.players : [] };
  if (!playersBtn) return;
  const n = lastOnline.count;
  playersBtn.querySelector('span').textContent = n === null ? 'Discord status' : `${n} ${n === 1 ? 'player' : 'players'} online`;
  playersBtn.hidden = false;
  // An open panel follows the data (the 1 s poll), keeping focus on the control that had it.
  const sig = JSON.stringify([lastOnline, presenceOn]);
  if (panelEl?.isConnected && sig !== panelSig) {
    const hadSwitch = document.activeElement?.id === 'mm-presence';
    panelEl.replaceChildren(presencePanel());
    if (hadSwitch) panelEl.querySelector('#mm-presence')?.focus();
  }
  panelSig = sig;
}

/** Matchmaking off or the state unreadable: no switch to offer, so no button, and no stale panel. */
function hideOnline() {
  if (panelEl?.isConnected) panelEl.close();
  if (playersBtn) playersBtn.hidden = true;
}

// Who is online (the first 12, as before) and the switch that sat at the bottom of the page, unchanged: it sends
// presence_set_enabled exactly as the old checkbox did (ruling 4 keeps its words).
function presencePanel() {
  const list = el('ul', { class: 'on-who' });
  for (const p of lastOnline.players.slice(0, 12)) {
    list.append(el('li', {}, playerLink(p?.player_id, String(p?.name ?? ''), { key: `online-${p?.player_id}`, title: p?.discord ? `@${p.discord}` : null }), ACTIVITY[p?.activity] ? el('small', { text: ACTIVITY[p.activity] }) : null));
  }
  const sw = el('button', { type: 'button', class: 'look-switch', id: 'mm-presence', role: 'switch', 'aria-checked': String(presenceOn), 'aria-labelledby': 'mm-presence-label' });
  sw.addEventListener('click', async () => {
    const next = sw.getAttribute('aria-checked') !== 'true';
    if (await act('presence_set_enabled', { enabled: next })) sw.setAttribute('aria-checked', String(next));
  });
  return el('div', { class: 'on-panel' },
    lastOnline.count !== null ? el('span', { class: 'k-label', text: `${lastOnline.count} online` }) : null,
    lastOnline.count !== null ? list : null,
    el('div', { class: 'on-switch-row' }, el('span', { id: 'mm-presence-label', text: "Show what I'm doing on Discord" }), sw));
}
playersBtn?.addEventListener('click', () => { panelSig = JSON.stringify([lastOnline, presenceOn]); panelEl = openPanel(playersBtn, { label: 'Players online', build: presencePanel }); });

async function draw() {
  if (!bridge) {
    hideOnline();
    setLinks(true);
    section.replaceChildren(el('p', { class: 'note', text: 'Matchmaking needs the Reborn launcher.' }));
    return;
  }
  let state;
  try { state = await call('mm_state'); } catch (e) { hideOnline(); setLinks(true); section.replaceChildren(el('p', { class: 'note', text: reason(e) })); return; }
  // A launcher build without matchmaking, or a test bridge that does not know this command, answers null. That is not
  // an error the player can act on, so the panel simply stays out of the way.
  if (!state || typeof state !== 'object') { hideOnline(); setLinks(true); section.replaceChildren(); return; }
  // The kill switch: the server has matchmaking off (or predates it). Only the Private room tile, whose row opens the
  // direct connection (spec §3.9). Drawn once, not every second.
  if (state.link === 'disabled') {
    hideOnline();
    markBusy(false);
    setLinks(false); // the row's primary is Direct connection already
    if (lastDrawn === 'disabled') return;
    lastDrawn = 'disabled';
    section.replaceChildren(tiles(null, { only: 'private' }), directRow());
    return;
  }

  // Only redraw when something the player can see changed, so typing in chat is never interrupted.
  const signature = JSON.stringify({ l: state.link, s: state.snapshot, r: state.relay_error, p: state.presence_on, m: mode(), o: overLive.snap ? overLive.id : null });
  if (signature === lastDrawn || busy) return;
  // Only a text field being typed in (the chat, the room code) holds a redraw. The presence switch is a button, and
  // focus left on a control must never freeze the page (09-26).
  const active = document.activeElement;
  // The chat input is exempt: its box stays attached through a redraw (placeChildren), so typing is never interrupted
  // and new lines still arrive while a reply is half-written.
  const focused = section.contains(active) && active.tagName === 'INPUT' && ['text', 'search', 'url', 'email'].includes(active.type) && active.value !== ''
    && active.getAttribute('data-key') !== 'chat';
  if (focused) return;
  lastDrawn = signature;

  const snap = state.snapshot;
  // A HELD match is not busy: the server releases a held match's seats straight away, so the player is free to play
  // again. The hold notice still shows; the rest of the page stays usable.
  const liveMatch = snap?.match && !['complete', 'void', 'abandoned', 'held'].includes(snap.match.relay_state);
  markBusy(!!(liveMatch || snap?.room || snap?.ticket));
  let body;
  let showLinks = true;
  let reserveLinks = false;
  if (state.link === 'signed_out') {
    body = [lockPanel(invite ? `Sign in to join ${invite.code}` : 'Sign in to hit the street', 'Connect your Discord account to play online and track your stats!', true)];
  } else if (state.link === 'offline' && !snap) {
    body = [lockPanel("Can't reach the Reborn servers right now.", 'Trying again.', false)];
  } else if (!snap) {
    // The same blurred tiles as signed out, with one quiet pill (launch audit M5): never a bare line over empty space.
    body = [el('div', { class: 'on-locked on-connecting' }, tiles(null, { locked: true }),
      el('div', { class: 'on-lock', role: 'status' }, el('span', { class: 'on-st' }, el('i', { 'aria-hidden': 'true' }), 'Connecting…')))];
  } else if (liveMatch) {
    body = drawMatch(snap.match, snap.chat ?? [], state.relay_error, snap);
  } else if (snap.room) {
    body = drawRoom(snap.room, snap.chat ?? []);
  } else if (snap.ticket) {
    body = drawQueued(snap.ticket);
  } else if (snap.match?.relay_state === 'complete' && !overDone.has(snap.match.id) && seriesOverModel(snap.match, null)) {
    // The post-game screen (series spec 2026-10-02 §3.3). After the room and the queue, so a new search wins; gone for
    // good once put away (overDone).
    const m = snap.match;
    loadOverLive(m.id);
    const model = seriesOverModel(m, overLive.id === m.id ? overLive.snap : null, { name: snap.player?.name || me?.name, id: snap.player?.player_id ?? me?.id ?? null, avatar: myAvatarUrl() });
    body = drawSeriesOver(model, {
      el, avatar, teamLogo, playerLink,
      // The same queue as the mode the match was (ranked keeps its rules notice), and the screen is put away.
      onAgain: () => {
        if (m.mode === 'ranked' || m.mode === 'casual') { privatePicked = false; store.set('mm:mode', m.mode); }
        overDone.add(m.id);
        overShown = null;
        if (mode() === 'ranked') findRanked(); else queueWhenCurrent();
      },
      onBox: () => { if (model.boxHref) location.href = model.boxHref; },
      onRematch: async () => { if (await act('mm_match_action', { matchId: m.id, action: 'rematch', value: null })) leaveOver(m.id); },
      onBack: () => leaveOver(m.id),
    });
    showLinks = false;
  } else {
    body = drawIdle(snap);
    showLinks = picked() === 'private';
    reserveLinks = true;
    if (snap.match?.relay_state === 'held' && dismissedHold !== snap.match.id) body.unshift(heldBox(snap.match));
    const banner = invite ? inviteBanner() : null;
    if (banner) body.unshift(banner);
  }
  setLinks(showLinks, reserveLinks);
  showOnline(snap, state.presence_on);
  // Keep the control the player is on across the redraw, by data-key (spec §6): the page used to drop focus on every
  // state change, which left a controller nowhere.
  const had = document.activeElement;
  const focusKey = had && section.contains(had) ? had.getAttribute('data-key') : null;
  placeChildren(section, [...body, status].filter(Boolean));
  settleChat();
  const back = focusKey ? section.querySelector(`[data-key="${focusKey}"]`) : null;
  if (back && document.activeElement !== back) back.focus({ preventScroll: true });
  // A controller arriving on the post-game screen lands on Play again (B is Back to Online).
  const over = section.querySelector('[data-key="over-again"]');
  if (over && overShown !== snap?.match?.id) {
    overShown = snap.match.id;
    if (!focusKey && document.body.classList.contains('gamepad-input')) over.focus({ preventScroll: true });
  } else if (!over) overShown = null;
}

draw();
setInterval(draw, 1000);
