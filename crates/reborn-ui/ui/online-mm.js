// Matchmaking on the Online page: Quick Match, lobbies, and the match in progress.
//
// Plain on purpose. The designed interface comes later on top of these same commands; this page's
// job is to make every command reachable and every state readable. It sits ABOVE the private-room
// section (online.js) and never touches it, so the existing host/join flow and its checkers are
// unchanged.
//
// Reads come from `mm_state`, which returns what the launcher's background long-poll already holds
// in memory - so polling it every second costs nothing and never touches the network. Only actions
// (queue, join, ready, chat) make a request.
import { bridge, call, el, get, reason } from './shell.js';

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
const status = el('p', { class: 'settings-status', role: 'status' });

function mode() { return store.get('mm:mode', 'casual'); }

function modeTabs() {
  const tabs = el('div', { class: 'section-tabs', 'aria-label': 'Casual or ranked' });
  for (const [value, label] of [['casual', 'Casual'], ['ranked', 'Ranked']]) {
    tabs.append(el('button', {
      type: 'button', text: label, 'aria-pressed': String(value === mode()),
      onclick: () => { store.set('mm:mode', value); lastDrawn = ''; draw(); },
    }));
  }
  return tabs;
}

async function act(cmd, args) {
  if (busy) return;
  busy = true;
  status.textContent = '';
  section.setAttribute('aria-busy', 'true');
  try {
    await call(cmd, args);
  } catch (e) {
    // The server's own sentence, verbatim, is always the most useful thing to show.
    status.textContent = reason(e);
  } finally {
    busy = false;
    section.removeAttribute('aria-busy');
    lastDrawn = '';
    await draw();
  }
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

function clock(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

function chatBox(messages, send) {
  const log = el('ol', { class: 'mm-chat', 'aria-label': 'Chat', 'aria-live': 'polite' });
  for (const m of messages.slice(-30)) log.append(el('li', {}, el('strong', { text: `${m.name}: ` }), m.body));
  const input = el('input', { type: 'text', maxlength: 300, placeholder: 'Say something', 'aria-label': 'Chat message' });
  const form = el('form', { class: 'mm-chat-form' }, input, el('button', { type: 'submit', text: 'Send' }));
  form.addEventListener('submit', (e) => { e.preventDefault(); const t = input.value.trim(); if (t) { send(t); input.value = ''; } });
  return el('div', {}, log, form);
}

function drawIdle(snap) {
  const depth = snap?.queue_depth ?? { casual: 0, ranked: 0 };
  const find = el('button', {
    type: 'button', class: 'primary', text: `Find a ${mode()} game`,
    onclick: () => queueWhenCurrent(),
  });
  const lobbies = el('div', { class: 'mm-lobbies' }, el('h3', { text: 'Lobbies' }), el('p', { class: 'loading', text: 'Loading lobbies…' }));
  loadLobbies(lobbies);

  const code = el('input', { type: 'text', maxlength: 12, placeholder: 'Lobby code', 'aria-label': 'Lobby code', spellcheck: 'false' });
  const join = el('form', { class: 'mm-join' }, code, el('button', { type: 'submit', text: 'Join by code' }));
  join.addEventListener('submit', (e) => { e.preventDefault(); const c = code.value.trim(); if (c) act('mm_room_join', { code: c }); });

  const open = el('button', { type: 'button', text: 'Open a public lobby', onclick: () => act('mm_room_create', { mode: mode(), public: true }) });
  const priv = el('button', { type: 'button', text: 'Private lobby for a friend', onclick: () => act('mm_room_create', { mode: mode(), public: false }) });

  return [
    el('h2', { text: 'Quick Match' }),
    modeTabs(),
    el('p', { class: 'note', text: mode() === 'ranked'
      ? 'Ranked is best of three and moves your rating.'
      : 'Casual is one game and never touches your rating.' }),
    find,
    el('p', { class: 'note', text: `${depth[mode()] ?? 0} waiting for a ${mode()} game right now.` }),
    lobbies,
    el('div', { class: 'mm-actions' }, open, priv),
    join,
  ];
}

async function loadLobbies(box) {
  try {
    const rooms = await call('mm_rooms');
    const list = el('ul', { class: 'mm-room-list' });
    if (!rooms?.length) {
      box.replaceChildren(el('h3', { text: 'Lobbies' }), el('p', { class: 'note', text: 'No open lobbies. Open one and someone can join you.' }));
      return;
    }
    for (const r of rooms) {
      list.append(el('li', {},
        `${r.host ?? 'A player'} · ${r.mode === 'ranked' ? 'Ranked, best of 3' : 'Casual'} `,
        el('button', { type: 'button', text: 'Join', onclick: () => act('mm_room_join', { code: r.code }) })));
    }
    box.replaceChildren(el('h3', { text: 'Lobbies' }), list);
  } catch (e) {
    box.replaceChildren(el('h3', { text: 'Lobbies' }), el('p', { class: 'note', text: reason(e) }));
  }
}

// The queue timer counts on this page, every second. The snapshot's `waited_ms` only changes when
// the server has something new to say (a long poll can sit for 25 s), so drawing it as-is froze the
// timer (Adam, 2026-09-23: "The timer doesn't move"). It is anchored once per ticket, on the
// server's figure, and then advanced by this page's own clock.
let queueClock = { key: null, base: 0, at: 0 };
const queuedMs = () => queueClock.base + (Date.now() - queueClock.at);
setInterval(() => {
  const timer = section.querySelector('.mm-timer');
  if (timer && queueClock.key) timer.textContent = `Waiting ${clock(queuedMs())}`;
}, 1000);

function drawQueued(t) {
  if (queueClock.key !== t.enqueued_at) queueClock = { key: t.enqueued_at, base: t.waited_ms || 0, at: Date.now() };
  return [
    el('h2', { text: `Looking for a ${t.mode} game` }),
    el('p', { class: 'mm-timer', text: `Waiting ${clock(queuedMs())}` }),
    el('button', { type: 'button', text: 'Stop looking', onclick: () => act('mm_leave_queue') }),
  ];
}

function drawRoom(r, chat) {
  const members = el('ul', { class: 'mm-members' });
  for (const m of r.members) members.append(el('li', { text: `${m.name}${m.you ? ' (you)' : ''}${m.ready ? ' — ready' : ''}` }));
  // The launcher marks which member is you; member order means nothing.
  const me = r.members.find((m) => m.you);
  const ready = !!me?.ready;
  return [
    el('h2', { text: `${r.mode === 'ranked' ? 'Ranked' : 'Casual'} lobby` }),
    el('p', {}, 'Lobby code: ', el('code', { text: r.code }), ' ',
      el('button', { type: 'button', text: 'Copy', onclick: () => navigator.clipboard?.writeText(r.code) })),
    members,
    el('div', { class: 'mm-actions' },
      el('button', { type: 'button', class: 'primary', text: ready ? 'Not ready' : 'Ready',
        onclick: () => act('mm_room_action', { roomId: r.id, action: 'ready', value: !ready }) }),
      el('button', { type: 'button', text: 'Start', onclick: () => act('mm_room_action', { roomId: r.id, action: 'start', value: null }) }),
      el('button', { type: 'button', text: 'Leave lobby', onclick: () => act('mm_room_action', { roomId: r.id, action: 'leave', value: null }) })),
    chatBox(chat, (t) => act('mm_room_action', { roomId: r.id, action: 'chat', value: t })),
  ];
}

function drawMatch(m, chat, relayError) {
  const words = RELAY[m.relay_state] ?? 'Starting the game';
  const parts = [
    el('h2', { text: `vs ${m.opponent || 'your opponent'}` }),
    el('p', { class: 'mm-state', text: words }),
    el('p', { class: 'note', text: `${m.mode === 'ranked' ? `Ranked · best of ${m.best_of}` : 'Casual · one game'}` }),
  ];
  if (m.best_of > 1 && Array.isArray(m.series)) {
    const mine = m.series[m.seat] ?? 0, theirs = m.series[1 - m.seat] ?? 0;
    parts.push(el('p', { class: 'mm-series', text: `Series: you ${mine} – ${theirs} ${m.opponent}` }));
  }
  if (m.relay_state === 'pairing' && !m.your_accepted) {
    parts.push(el('div', { class: 'mm-actions' },
      el('button', { type: 'button', class: 'primary', text: 'Accept', onclick: () => act('mm_match_action', { matchId: m.id, action: 'accept', value: true }) }),
      el('button', { type: 'button', text: 'Decline', onclick: () => act('mm_match_action', { matchId: m.id, action: 'accept', value: false }) })));
  } else if (m.relay_state === 'pairing') {
    parts.push(el('p', { class: 'note', text: `Accepted. Waiting for ${m.opponent} to accept.` }));
  }
  // The relay's own failure (the room would not open, the code would not send) in its own words.
  if (relayError && !['complete', 'void', 'abandoned', 'held'].includes(m.relay_state)) {
    parts.push(el('p', { class: 'refusal', text: relayError }));
  }
  if (m.relay_state === 'held' && m.hold_reason === 'seed-mismatch') {
    parts.push(el('p', { class: 'note', text: 'Your games would not have started from the same point, so neither started. Update the launcher and try again.' }));
  }
  if (m.relay_state === 'held' && m.hold_reason === 'content-stale') {
    parts.push(el('p', { class: 'note', text: 'A game update never reached one of your game discs, so the game was not started. Both of you: Settings → Repair game files (the one that is current is told so), then find a game again.' }));
  }
  if (m.relay_state === 'held' && m.hold_reason === 'content-mismatch') {
    parts.push(el('p', { class: 'note', text: `Your game files differ from ${m.opponent ?? 'your opponent'}'s, so the game was not started. Both of you: Settings → Check game content, install what it offers, then try again.` }));
  }
  if (m.relay_state === 'held' && m.hold_reason) {
    parts.push(el('p', { class: 'note', text: 'Nothing has changed on the ladder. A person will look at this match.' }));
  }
  if (!['complete', 'void', 'abandoned', 'held'].includes(m.relay_state)) {
    parts.push(el('button', { type: 'button', text: 'Leave match', onclick: () => act('mm_match_action', { matchId: m.id, action: 'leave', value: null }) }));
  }
  parts.push(chatBox(chat, (t) => act('mm_match_action', { matchId: m.id, action: 'chat', value: t })));
  return parts;
}

function drawOnline(snap) {
  const online = snap?.online ?? { count: 0, players: [] };
  const names = online.players.slice(0, 12).map((p) => p.name).join(', ');
  return el('p', { class: 'note mm-online', text: `${online.count} online${names ? `: ${names}` : ''}` });
}

function presenceToggle(on) {
  const box = el('input', { type: 'checkbox', id: 'mm-presence' });
  box.checked = !!on;
  box.addEventListener('change', () => act('presence_set_enabled', { enabled: box.checked }));
  return el('label', { class: 'mm-presence', for: 'mm-presence' }, box, ' Show what I\'m doing on Discord');
}

async function draw() {
  if (!bridge) {
    section.replaceChildren(el('p', { class: 'note', text: 'Matchmaking needs the Reborn launcher.' }));
    return;
  }
  let state;
  try { state = await call('mm_state'); } catch (e) { section.replaceChildren(el('p', { class: 'note', text: reason(e) })); return; }
  // A launcher build without matchmaking, or a test bridge that does not know this command, answers
  // null. That is not an error the player can act on, so the panel simply stays out of the way.
  if (!state || typeof state !== 'object') { section.replaceChildren(); return; }
  // The kill switch: the server has matchmaking off (or predates it). Nothing to offer, nothing to
  // explain - the page is exactly the private-room page it was before matchmaking existed.
  if (state.link === 'disabled') { lastDrawn = ''; markBusy(false); section.replaceChildren(); return; }

  // Only redraw when something the player can see changed, so typing in chat is never interrupted.
  const signature = JSON.stringify({ l: state.link, s: state.snapshot, r: state.relay_error, p: state.presence_on, m: mode() });
  if (signature === lastDrawn || busy) return;
  // Only a text field being typed in (the chat, the lobby code) holds a redraw. The presence
  // checkbox is an <input> too, and focus left on it froze the page on "Waiting m:ss" for a whole
  // game while the launcher underneath ran the match (09-26).
  const active = document.activeElement;
  const focused = section.contains(active) && active.tagName === 'INPUT' && ['text', 'search', 'url', 'email'].includes(active.type) && active.value !== '';
  if (focused) return;
  lastDrawn = signature;

  const snap = state.snapshot;
  // A HELD match is not busy: the server releases a held match's seats straight away, so the player
  // is free to play again. The hold notice still shows; the rest of the page stays usable.
  const liveMatch = snap?.match && !['complete', 'void', 'abandoned', 'held'].includes(snap.match.relay_state);
  markBusy(!!(liveMatch || snap?.room || snap?.ticket));
  let body;
  if (state.link === 'signed_out') {
    body = [el('h2', { text: 'Online play' }), el('p', { text: 'Sign in with Discord on the Play page to find games, join lobbies and play ranked.' })];
  } else if (state.link === 'offline' && !snap) {
    body = [el('h2', { text: 'Online play' }), el('p', { text: 'Can\'t reach the Reborn servers right now. Trying again.' })];
  } else if (!snap) {
    body = [el('p', { class: 'loading', text: 'Connecting…' })];
  } else if (snap.match && !['complete', 'void', 'abandoned', 'held'].includes(snap.match.relay_state)) {
    body = drawMatch(snap.match, snap.chat ?? [], state.relay_error);
  } else if (snap.room) {
    body = drawRoom(snap.room, snap.chat ?? []);
  } else if (snap.ticket) {
    body = drawQueued(snap.ticket);
  } else {
    body = drawIdle(snap);
    // A held match is over for the player the moment it is held (seats released), but the server
    // keeps showing it for a few minutes. It is a notice on the normal page, never the page itself:
    // 09-26 both players sat on "Held for review" with no Find a game button for five minutes.
    if (snap.match?.relay_state === 'held' && snap.match.hold_reason === 'content-stale') {
      body.unshift(el('p', { class: 'note', text: 'A game update never reached one of your game discs, so that game was not started. Both of you: Settings → Repair game files (the one that is current is told so), then find a game again. Finding a game now also repairs it first.' }));
    } else if (snap.match?.relay_state === 'held' && snap.match.hold_reason === 'content-mismatch') {
      body.unshift(el('p', { class: 'note', text: `Your game files differ from ${snap.match.opponent ?? 'your opponent'}'s, so that game was not started. Both of you: Settings → Check game content, install what it offers, then find a game again.` }));
    } else if (snap.match?.relay_state === 'held') {
      body.unshift(el('p', { class: 'note', text: `Your last match${snap.match.opponent ? ` against ${snap.match.opponent}` : ''} was held for review: nothing changed on the ladder, and a person will look at it. You can play again now.` }));
    }
  }
  section.replaceChildren(...body, status, drawOnline(snap), presenceToggle(state.presence_on));
}

draw();
setInterval(draw, 1000);
