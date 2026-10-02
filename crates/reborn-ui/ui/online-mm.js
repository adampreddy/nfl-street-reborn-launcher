// Matchmaking on the Online page: the mode tiles, the action row, the match in progress and your room
// (spec docs/superpowers/specs/2026-10-02-online-page-redesign-design.md).
//
// Only the look changed in the redesign. Every command, argument, order and poll below is the one this page always
// sent: `mm_state` every second (a read of what the launcher's long-poll already holds in memory, so it never touches
// the network) and one request per action. Which state the mode area shows is decided exactly as before: a live match,
// then a room, then a ticket, then idle (a held match draws idle plus a notice).
import { bridge, call, el, get, reason } from './shell.js';
import { avatar, teamLogo } from './match-rows.js';
import { acceptWithin, clock, eloBits, everyoneReady, heldNotice, inviteLine, inviteLink, modeLine, seekLine, seriesLine, standing } from './online-model.js';
import { openPanel } from './menu.js';
import { tabs } from './tabs.js';
import { createSignIn } from './sign-in.js';

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
const status = el('p', { class: 'settings-status on-status', role: 'status' });

function mode() { return store.get('mm:mode', 'casual'); }

const links = document.getElementById('on-links');
function setLinks(show) { if (links) links.hidden = !show; }

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

// Your rank, Elo and plays-as team (stats_dashboard's `you`), from leaderboard.js. Null until it answers and whenever
// the server sends none (Elo off): every spot that needs it is then hidden (spec §7).
let rankInfo = null;
document.addEventListener('online-rank', (e) => { rankInfo = e.detail ?? null; lastDrawn = ''; draw(); });

// Who you are, for your own picture on the matchup card: Play caches the Discord picture under `reborn:avatar`
// (home.js) keyed by player id; any other id or no cache draws your letter.
let me = null;
if (bridge) call('identity_status').then((v) => { if (v?.signed_in) { me = { id: v.player_id, name: v.display_name }; lastDrawn = ''; } }).catch(() => {});
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
    await call('mm_queue', { mode: mode() });
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

function chatBox(messages, send) {
  const log = el('ol', { class: 'mm-chat', 'aria-label': 'Chat', 'aria-live': 'polite' });
  for (const m of messages.slice(-30)) log.append(el('li', {}, el('strong', { text: `${m.name}: ` }), m.body));
  const input = el('input', { type: 'text', maxlength: 300, placeholder: 'Say something…', 'aria-label': 'Chat message', 'data-key': 'chat' });
  const form = el('form', { class: 'mm-chat-form' }, input, el('button', { type: 'submit', class: 'btn quiet sm', text: 'Send' }));
  form.addEventListener('submit', (e) => { e.preventDefault(); const t = input.value.trim(); if (t) { send(t); input.value = ''; } });
  return el('div', { class: 'on-chat' }, log, form);
}

// ---- the mode area (spec §3.1) ----
const MODES = [
  { key: 'ranked', name: 'Ranked', line: 'First to two wins. Moves your Elo.', art: 'art/online/ranked.jpg' },
  { key: 'casual', name: 'Casual', line: 'One game. Elo stays put.', art: 'art/online/casual.jpg' },
  { key: 'private', name: 'Private room', line: 'Host a room or join with a code.', art: 'art/online/private.jpg' },
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
    el('img', { class: 'on-art', src: m.art, alt: '' }),
    pill,
    el('span', { class: 'on-lab' },
      el('span', { class: 'on-tx' }, el('span', { class: 'on-name', text: m.name }), el('span', { class: 'on-line', text: m.line })),
      rank)));
  }
  return box;
}

function findRow(p) {
  return el('div', { class: 'on-row' },
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'find', 'data-primary': 'true', text: `Find a ${p} game`, onclick: () => queueWhenCurrent() }),
    el('p', { class: 'on-note', text: p === 'ranked' ? 'Matched by Elo' : 'Matched with anyone' }));
}

// The code field's text survives the redraws (the field itself is rebuilt).
let roomCode = '';
function privateRow() {
  const code = el('input', { type: 'text', class: 'on-code', maxlength: 12, placeholder: 'Room code', 'aria-label': 'Room code', spellcheck: 'false', autocomplete: 'off', 'data-key': 'room-code', value: roomCode });
  const join = el('button', { type: 'submit', class: `btn on-join${roomCode.trim() ? '' : ' quiet'}`, 'data-key': 'join', 'aria-label': 'Join by code', text: 'Join' });
  code.addEventListener('input', () => { roomCode = code.value; join.classList.toggle('quiet', !code.value.trim()); });
  const form = el('form', { class: 'on-codeform' }, code, join);
  form.addEventListener('submit', (e) => { e.preventDefault(); const c = code.value.trim(); if (c) act('mm_room_join', { code: c }); });
  // Ruling P20: the room's mode, the choice the old Quick Match tabs made. Stored in `mm:mode` as before; it does not
  // move the tile pick off Private room.
  const roomMode = tabs('mm-room-mode', ['Casual', 'Ranked'], mode() === 'ranked' ? 'Ranked' : 'Casual',
    (name) => { store.set('mm:mode', name.toLowerCase()); lastDrawn = ''; draw(); }, { label: 'Casual or ranked' });
  return el('div', { class: 'on-row' },
    roomMode,
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'host', 'data-primary': 'true', text: 'Host a room', onclick: () => act('mm_room_create', { mode: mode(), public: false }) }),
    form,
    el('p', { class: 'on-note', text: 'Send the code to a friend' }));
}

function drawIdle(snap) {
  const p = picked();
  loadLobbies(); // the same `mm_rooms` read on every idle draw as before the redesign
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
      withButton && si.message ? el('p', { class: 'refusal', role: 'alert', text: si.message }) : null];
  return el('div', { class: 'on-locked' }, tiles(null, { locked: true }),
    el('div', { class: 'on-lock', role: 'region', 'aria-label': title }, ...parts.filter(Boolean)));
}

/** Matchmaking off: the private play that works without it is the direct connection (ruling 7). */
function directRow() {
  return el('div', { class: 'on-row' },
    el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'direct', 'data-primary': 'true', text: 'Direct connection',
      onclick: () => document.getElementById('direct-open')?.click() }),
    el('p', { class: 'on-note', text: 'Send the code to a friend' }));
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

// Public lobbies (spec §3.1): read on every idle draw, as always. Under the Private room row, "Open lobbies (n) ›"
// opens them in a panel with "Open a public lobby"; with none it reads "Open a public lobby ›" (ruling 8).
let lobbies = [];
let lobbyError = null;
const lobbiesLink = document.getElementById('lobbies-open');
const closePanel = () => document.querySelector('dialog.menu.panel[open]')?.close();
async function loadLobbies() {
  try {
    const rooms = await call('mm_rooms');
    lobbies = Array.isArray(rooms) ? rooms : [];
    lobbyError = null;
  } catch (e) {
    lobbies = [];
    lobbyError = reason(e);
  }
  if (lobbiesLink) {
    lobbiesLink.textContent = lobbies.length ? `Open lobbies (${lobbies.length}) ›` : 'Open a public lobby ›';
    lobbiesLink.hidden = false;
  }
}
function lobbyPanel() {
  const list = el('ul', { class: 'on-lobbies' });
  if (lobbyError) list.append(el('li', { class: 'note', text: lobbyError }));
  else if (!lobbies.length) list.append(el('li', { class: 'note', text: 'No open lobbies. Open one and someone can join you.' }));
  for (const r of lobbies) {
    list.append(el('li', {},
      el('span', { text: `${r.host ?? 'A player'} · ${r.mode === 'ranked' ? 'Ranked' : 'Casual'}` }),
      el('button', { type: 'button', class: 'btn sm', text: 'Join', onclick: () => { closePanel(); act('mm_room_join', { code: r.code }); } })));
  }
  return el('div', { class: 'on-panel' },
    el('span', { class: 'k-label', text: 'Public lobbies' }),
    list,
    el('button', { type: 'button', class: 'btn sm', 'data-key': 'open-public', text: 'Open a public lobby',
      onclick: () => { closePanel(); act('mm_room_create', { mode: mode(), public: true }); } }));
}
lobbiesLink?.addEventListener('click', () => openPanel(lobbiesLink, { label: 'Public lobbies', build: lobbyPanel }));

// ---- a room invite (spec §3.10) ----
// A `nflstreetreborn://room/<code>` link left its code behind (mm_pending_room). On load and on window focus the page
// picks Private room, fills the code and offers one line with Join; it never joins on its own. Join or Dismiss clears
// it; anything else (signed out, busy) leaves it waiting. The banner is drawn only while idle, so a queued, matched or
// roomed player never sees it and the invite waits.
const INVITE_CODE = /^[A-Za-z0-9-]{1,64}$/; // what the launcher keeps (main.rs invite_from); anything else is no invite
let invite = null; // { code, host }
async function takeRoomInvite() {
  if (!bridge) return;
  try {
    const p = await call('mm_pending_room');
    if (!p || typeof p.code !== 'string' || !INVITE_CODE.test(p.code)) return;
    if (invite?.code !== p.code) {
      invite = { code: p.code, host: null };
      privatePicked = true;
      roomCode = p.code;
      lastDrawn = '';
      draw();
    }
    // Asked again on focus until it answers: a signed-out player's lookup fails, and the name comes after sign-in.
    if (invite.host) return;
    const found = await call('mm_room_lookup', { code: p.code }).catch(() => null);
    if (found && typeof found.host_name === 'string' && found.host_name && invite?.code === p.code) {
      invite.host = found.host_name;
      lastDrawn = '';
      draw();
    }
  } catch { /* an invite that cannot be read is no invite */ }
}
function forgetInvite() {
  invite = null;
  call('mm_pending_room_clear').catch(() => {});
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
      onclick: async () => { if (await act('mm_room_join', { code })) forgetInvite(); } }),
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
setInterval(() => {
  const timer = section.querySelector('.mm-timer');
  if (timer && queueClock.key) timer.textContent = `Searching… ${clock(queuedMs())}`;
}, 1000);

function drawQueued(t) {
  if (queueClock.key !== t.enqueued_at) queueClock = { key: t.enqueued_at, base: t.waited_ms || 0, at: Date.now() };
  const ranked = t.mode === 'ranked';
  return [
    tiles(null, { searching: ranked ? 'ranked' : 'casual' }),
    el('div', { class: 'on-row' },
      el('div', { class: 'on-search' },
        el('span', { class: 'mm-timer on-timer num', text: `Searching… ${clock(queuedMs())}` }),
        el('span', { class: 'on-search-mode', text: ranked ? 'Ranked' : 'Casual' }),
        el('span', { class: 'sweep', 'aria-hidden': 'true' })),
      el('button', { type: 'button', class: 'btn on-cancel', 'data-key': 'cancel', text: 'Cancel', onclick: () => act('mm_leave_queue') })),
  ];
}

// Your room (spec §3.7): the code to share, who is in and ready, chat; Start for the host once everyone is ready.
function drawRoom(r, chat) {
  // The launcher marks which member is you; member order means nothing.
  const me = r.members.find((m) => m.you);
  const ready = !!me?.ready;
  const copyButton = (key, label, text) => {
    const b = el('button', { type: 'button', class: 'btn sm', 'data-key': key, text: label });
    b.addEventListener('click', () => navigator.clipboard?.writeText(text).then(() => { b.textContent = 'Copied'; setTimeout(() => { b.textContent = label; }, 1500); }, () => {}));
    return b;
  };
  const members = el('ul', { class: 'on-members' });
  for (const m of r.members) {
    members.append(el('li', {},
      avatar(null, m.name, 'on-mav'),
      el('b', {}, m.name, m.you ? el('small', { text: 'you' }) : null),
      el('span', { class: m.ready ? 'on-rd' : 'on-nr', text: m.ready ? 'Ready' : 'Not ready' })));
  }
  const room = el('section', { class: 'glass on-card', 'aria-label': 'Your room' },
    el('div', { class: 'on-card-h' }, el('h2', { text: 'Your room' }), el('span', { text: r.mode === 'ranked' ? 'Ranked' : 'Casual' })),
    el('span', { class: 'k-label', text: 'Room code' }),
    el('strong', { class: 'on-codebig', text: r.code }),
    el('div', { class: 'on-bt' }, copyButton('copy-code', 'Copy code', r.code), inviteLink(r.code) ? copyButton('copy-invite', 'Copy invite link', inviteLink(r.code)) : null),
    el('span', { class: 'k-label', text: 'Players' }),
    members);
  const talk = el('section', { class: 'glass on-card', 'aria-label': 'Chat' },
    el('div', { class: 'on-card-h' }, el('h2', { text: 'Chat' })),
    chatBox(chat, (t) => act('mm_room_action', { roomId: r.id, action: 'chat', value: t })));
  const row = el('div', { class: 'on-row on-roomrow' },
    r.you_host === true ? el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'room-start', 'data-primary': 'true', text: 'Start',
      disabled: !everyoneReady(r), onclick: () => act('mm_room_action', { roomId: r.id, action: 'start', value: null }) }) : null,
    el('button', { type: 'button', class: 'btn', 'data-key': 'room-ready', text: ready ? "I'm not ready" : "I'm ready",
      onclick: () => act('mm_room_action', { roomId: r.id, action: 'ready', value: !ready }) }),
    el('button', { type: 'button', class: 'btn quiet', 'data-key': 'room-leave', text: 'Leave room',
      onclick: () => act('mm_room_action', { roomId: r.id, action: 'leave', value: null }) }));
  return [el('div', { class: 'on-room' }, room, talk), row, el('p', { class: 'on-note', text: "Start works once you're both ready. The host starts the game." })];
}

function face({ url, name, sub, team, tick = false }) {
  return el('div', { class: 'on-p' },
    el('span', { class: 'on-avw' }, avatar(url, name, 'on-av'), tick ? el('span', { class: 'on-ok', role: 'img', 'aria-label': 'Accepted', text: '✓' }) : null),
    el('span', { class: 'on-pname', text: name }),
    sub ? el('span', { class: 'on-psub num', text: sub }) : null,
    team ? teamLogo(team, 'on-plogo') : null);
}

// The matchup card (spec §3.3–§3.6). Only called for a match that is not over (complete, void, abandoned, held).
function drawMatch(m, chat, relayError, snap) {
  const words = RELAY[m.relay_state] ?? 'Starting the game';
  const opp = m.opponent || 'your opponent';
  const pairing = m.relay_state === 'pairing';
  // Ruling P9: between games of a series is still "Playing", with the series line.
  const playing = m.relay_state === 'running' || m.relay_state === 'intermission';
  const series = playing ? seriesLine(m) : null;
  const mid = el('div', { class: 'on-mid' },
    el('span', { class: 'k-label on-found', text: playing ? 'Playing' : 'Match found' }),
    el('span', { class: 'on-x', 'aria-hidden': 'true', text: 'VS' }),
    series ? el('span', { class: 'on-ser num', text: series }) : null,
    el('span', { class: 'on-mode-line', text: modeLine(m.mode) }));
  if (pairing && !m.your_accepted) {
    mid.append(el('div', { class: 'on-acts' },
      el('button', { type: 'button', class: 'btn primary', 'data-key': 'accept', 'data-primary': 'true', text: 'Accept', onclick: () => act('mm_match_action', { matchId: m.id, action: 'accept', value: true }) }),
      // Ruling P8: Decline is the quiet button (spec §3.3).
      el('button', { type: 'button', class: 'btn quiet', 'data-key': 'decline', text: 'Decline', onclick: () => act('mm_match_action', { matchId: m.id, action: 'accept', value: false }) })));
    const left = acceptWithin(m.relay_deadline, Date.now());
    if (left) mid.append(el('span', { class: 'on-tm num', text: left }));
  } else if (pairing) {
    mid.append(el('span', { class: 'on-st' }, el('i', { 'aria-hidden': 'true' }), `Accepted. Waiting for ${opp}…`));
  } else if (m.relay_state !== 'running') {
    mid.append(el('span', { class: 'on-st' }, el('i', { 'aria-hidden': 'true' }), words), el('span', { class: 'on-tm', text: 'Your game opens on its own' }));
  }
  // The relay's own failure (the room would not open, the code would not send) in its own words.
  if (relayError) mid.append(el('p', { class: 'refusal', text: relayError }));
  const mine = eloBits(rankInfo?.elo);
  const card = el('section', { class: 'on-vs', 'aria-label': `${modeLine(m.mode)} against ${opp}` },
    el('img', { class: 'on-art', src: 'art/online/matchup.jpg', alt: '' }),
    el('button', { type: 'button', class: 'btn quiet sm on-leave', 'data-key': 'leave-match', text: 'Leave match', onclick: () => act('mm_match_action', { matchId: m.id, action: 'leave', value: null }) }),
    el('div', { class: 'on-vs-in' },
      face({ url: myAvatarUrl(), name: snap?.player?.name || me?.name || 'You', sub: [mine.rank, mine.elo].filter(Boolean).join(' · '),
        team: rankInfo?.playsAs ?? null, tick: !!m.your_accepted && !playing }),
      mid,
      // On the card itself a nameless opponent is "Opponent" (letter O); sentences keep "your opponent".
      face({ url: m.opponent_avatar_url ?? null, name: m.opponent || 'Opponent', sub: standing(m.opponent_rank, m.opponent_elo), team: m.opponent_team ?? null })),
    MOVING.has(m.relay_state) ? el('span', { class: 'sweep', 'aria-hidden': 'true' }) : null);
  // Match chat stays, under the card (nothing that exists is removed; plan ruling 15).
  return [card, chatBox(chat, (t) => act('mm_match_action', { matchId: m.id, action: 'chat', value: t }))];
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
    list.append(el('li', {}, el('span', { text: String(p?.name ?? '') }), ACTIVITY[p?.activity] ? el('small', { text: ACTIVITY[p.activity] }) : null));
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
  const signature = JSON.stringify({ l: state.link, s: state.snapshot, r: state.relay_error, p: state.presence_on, m: mode() });
  if (signature === lastDrawn || busy) return;
  // Only a text field being typed in (the chat, the room code) holds a redraw. The presence switch is a button, and
  // focus left on a control must never freeze the page (09-26).
  const active = document.activeElement;
  const focused = section.contains(active) && active.tagName === 'INPUT' && ['text', 'search', 'url', 'email'].includes(active.type) && active.value !== '';
  if (focused) return;
  lastDrawn = signature;

  const snap = state.snapshot;
  // A HELD match is not busy: the server releases a held match's seats straight away, so the player is free to play
  // again. The hold notice still shows; the rest of the page stays usable.
  const liveMatch = snap?.match && !['complete', 'void', 'abandoned', 'held'].includes(snap.match.relay_state);
  markBusy(!!(liveMatch || snap?.room || snap?.ticket));
  let body;
  let showLinks = true;
  if (state.link === 'signed_out') {
    body = [lockPanel(invite ? `Sign in to join ${invite.code}` : 'Sign in to play online', 'Connect your Discord account to play online and track your stats!', true)];
  } else if (state.link === 'offline' && !snap) {
    body = [lockPanel("Can't reach the Reborn servers right now.", 'Trying again.', false)];
  } else if (!snap) {
    body = [el('p', { class: 'loading', text: 'Connecting…' })];
  } else if (liveMatch) {
    body = drawMatch(snap.match, snap.chat ?? [], state.relay_error, snap);
  } else if (snap.room) {
    body = drawRoom(snap.room, snap.chat ?? []);
  } else if (snap.ticket) {
    body = drawQueued(snap.ticket);
  } else {
    body = drawIdle(snap);
    showLinks = picked() === 'private';
    if (snap.match?.relay_state === 'held' && dismissedHold !== snap.match.id) body.unshift(heldBox(snap.match));
    const banner = invite ? inviteBanner() : null;
    if (banner) body.unshift(banner);
  }
  setLinks(showLinks);
  showOnline(snap, state.presence_on);
  section.replaceChildren(...body, status);
}

draw();
setInterval(draw, 1000);
