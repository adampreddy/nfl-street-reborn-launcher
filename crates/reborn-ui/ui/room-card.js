// The room card and its chat, shared by Online's private rooms and the League tab's game day (spec
// docs/superpowers/specs/2026-10-21-league-game-day-design.md): the same snapshot, the same commands, the same look.
import { el } from './shell.js';
import { avatar } from './match-rows.js';
import { everyoneReady, inviteLink } from './online-model.js';
import { playerLink } from './player-link.js';

// THE CHAT IS ONE ELEMENT PER ROOM OR MATCH, kept across redraws (Adam 10-06: "text chat resets to the top every
// time someone types something, requiring you to scroll down"). Every new line changes the snapshot, and the page used
// to answer every change by building a brand-new chat list (and replaceChildren() detaching the old one), which starts
// scrolled to the top. Now the list, the input and the form live here, keyed by the room or match; a redraw appends
// only the lines it has not drawn yet (by seq), follows the newest line only when the player was already at (or near)
// the bottom, and otherwise leaves their place alone. placeChildren() keeps the box attached through the redraw where
// it can; where its card is rebuilt around it (the room), settleChat() puts the scroll position, the input's focus
// and its caret back. The text being typed is in the same input element, so it always survives.
const CHAT_LINES = 30;
const NEAR_BOTTOM_PX = 24;
const chatUi = { key: null, root: null, log: null, input: null, lastSeq: -Infinity, send: null, following: true, atBottom: true, top: null, caret: null };
const chatGap = (log) => log.scrollHeight - log.scrollTop - log.clientHeight;
/** Scroll the chat to its newest line, remembering where that landed so the scroll event it causes is not read as the
 *  player scrolling (the box can still shrink under it before that event arrives). */
function chatToBottom(log) {
  log.scrollTop = log.scrollHeight;
  chatUi.setTop = log.scrollTop;
}

function chatLine(m) {
  // The author's name opens their profile (owner 10-08); a nickname's Discord name shows on hover (null without one).
  const who = el('strong', {}, playerLink(m.player_id, m.name || 'Player', { key: `chat-${m.seq}`, title: m.discord ? `@${m.discord}` : null }), ': ');
  const li = el('li', { 'data-seq': Number.isFinite(m.seq) ? String(m.seq) : null }, who, String(m.body ?? m.text ?? ''));
  chatUi.resize?.observe(li);
  return li;
}

export function chatBox(key, messages, send) {
  if (chatUi.key !== key || !chatUi.root) {
    // A new room or match: a new conversation. Whatever was being typed carries over (a room that became a match).
    const typed = chatUi.input?.value ?? '';
    const log = el('ol', { class: 'mm-chat', 'aria-label': 'Chat', 'aria-live': 'polite' });
    const input = el('input', { type: 'text', maxlength: 300, placeholder: 'Say something…', 'aria-label': 'Chat message', 'data-key': 'chat', value: typed });
    const form = el('form', { class: 'mm-chat-form' }, input, el('button', { type: 'submit', class: 'btn quiet sm', text: 'Send' }));
    form.addEventListener('submit', (e) => {
      e.preventDefault();
      const t = input.value.trim();
      if (t) { chatUi.send?.(t); input.value = ''; }
      // Your own line always brings you to the bottom, where it is about to land.
      chatUi.following = true;
    });
    // Whether the player is following the conversation is decided by THEIR scrolling (a scroll event), not measured at
    // redraw time: a line that re-wraps once the font loads must not count as the player scrolling up.
    log.addEventListener('scroll', () => {
      if (!log.isConnected || !log.clientHeight || log.scrollTop === chatUi.setTop) return;
      chatUi.setTop = null;
      chatUi.following = chatGap(log) <= NEAR_BOTTOM_PX;
    });
    chatUi.resize?.disconnect();
    Object.assign(chatUi, { key, root: el('div', { class: 'on-chat' }, log, form), log, input, lastSeq: -Infinity, following: true });
    // A player following the conversation stays at the newest line when the lines re-wrap after a draw (the font
    // arriving, the page's scrollbar appearing, the window resized): every line and the box itself are watched.
    chatUi.resize = typeof ResizeObserver === 'function'
      ? new ResizeObserver(() => { if (chatUi.log === log && chatUi.following && log.isConnected) chatToBottom(log); }) : null;
    chatUi.resize?.observe(log);
  }
  chatUi.send = send;
  const log = chatUi.log;
  const lines = messages.slice(-CHAT_LINES);
  const seqs = lines.map((m) => m.seq);
  const numbered = seqs.every((q) => Number.isFinite(q));
  const newest = numbered && seqs.length ? Math.max(...seqs) : -Infinity;
  // Was the player reading the newest line? Decided before anything is added.
  const atBottom = chatUi.following;
  if (!numbered || newest < chatUi.lastSeq) {
    // The conversation restarted under the same key (the server re-sent it), or lines without a seq: draw it again.
    log.replaceChildren(...lines.map(chatLine));
  } else {
    for (const m of lines) if (m.seq > chatUi.lastSeq) log.append(chatLine(m));
    // Only the last CHAT_LINES stay. Lines leaving the top would pull a reader's place up by their height: put it back.
    const before = log.scrollHeight;
    while (log.children.length > CHAT_LINES) log.firstElementChild.remove();
    if (!atBottom && before !== log.scrollHeight) log.scrollTop -= before - log.scrollHeight;
  }
  chatUi.lastSeq = numbered ? Math.max(chatUi.lastSeq, newest) : -Infinity;
  chatUi.atBottom = atBottom;
  // Measured now, while the box is still where the player left it (a detached box reads 0).
  chatUi.top = log.isConnected ? log.scrollTop : null;
  const typing = document.activeElement === chatUi.input;
  chatUi.caret = typing ? [chatUi.input.selectionStart, chatUi.input.selectionEnd] : null;
  return chatUi.root;
}

/** After the chat box is on screen: follow the newest line if the player was at the bottom. */
export function settleChat() {
  if (!chatUi.log?.isConnected) return;
  if (chatUi.atBottom) chatToBottom(chatUi.log);
  else if (chatUi.top !== null && chatUi.log.scrollTop !== chatUi.top) chatUi.log.scrollTop = chatUi.top;
  chatUi.following = chatUi.atBottom;
  // The player was typing: the input has the focus and the caret where they were.
  if (chatUi.caret && document.activeElement !== chatUi.input) {
    chatUi.input.focus({ preventScroll: true });
    try { chatUi.input.setSelectionRange(...chatUi.caret); } catch { /* a caret is a convenience */ }
  }
}

const CHECK = '<svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>';

/**
 * Your room (spec §3.7): the code to share, who is in and ready, Start for the host once everyone is ready, and the chat
 * beside it. `title` and `sub` head the card; `kindControl` is Online's Quick game | Pickup switch (null elsewhere).
 * `holdUp: false` (game day, Adam 10-21 "no hold up button"): once ready, the button is a green Ready that sends nothing.
 * Returns [room card, chat card].
 */
export function drawRoomCard(r, chat, { act, title, sub = null, holdUp = true, kindControl = null }) {
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
      // A nickname's Discord name beside it, small (null without a nickname).
      el('b', { title: m.discord ? `@${m.discord}` : null }, playerLink(m.player_id, m.name, { key: `member-${m.player_id || m.name}` }), m.you ? el('small', { text: 'you' }) : null,
        m.discord ? el('small', { class: 'on-discord', text: `@${m.discord}` }) : null),
      el('span', { class: m.ready ? 'on-rd' : 'on-nr', text: m.ready ? 'Ready' : 'Not ready' })));
  }
  // A 2v2 room takes four (an older launcher's room, or an older server's, takes two): the spots still open show.
  const capacity = Math.max(2, Math.min(4, Number(r.capacity) || 2));
  for (let i = r.members.length; i < capacity; i++) {
    members.append(el('li', { class: 'on-open' },
      el('span', { class: 'on-mav on-mav-open', 'aria-hidden': 'true' }),
      el('b', { text: 'Open spot' }),
      el('span', { class: 'on-nr', text: 'Send the code' })));
  }
  let readyButton;
  if (!holdUp && ready) {
    readyButton = el('button', { type: 'button', class: 'btn done', 'data-key': 'room-ready', 'aria-disabled': 'true' });
    readyButton.innerHTML = `${CHECK}Ready`;
  } else {
    readyButton = el('button', { type: 'button', class: 'btn', 'data-key': 'room-ready', text: ready ? 'Hold up' : "I'm ready",
      onclick: () => act('mm_room_action', { roomId: r.id, action: 'ready', value: !ready }) });
  }
  const row = el('div', { class: 'on-row on-roomrow' },
    r.you_host === true ? el('button', { type: 'button', class: 'btn primary on-go', 'data-key': 'room-start', 'data-primary': 'true', text: 'Start',
      disabled: !everyoneReady(r), onclick: () => act('mm_room_action', { roomId: r.id, action: 'start', value: null }) }) : null,
    readyButton,
    el('button', { type: 'button', class: 'btn quiet', 'data-key': 'room-leave', text: 'Leave room',
      onclick: () => act('mm_room_action', { roomId: r.id, action: 'leave', value: null }) }));
  const note = capacity > 2
    ? 'Up to 4 players. 2v2 works. Everyone ready, then the host kicks it off.'
    : 'Both ready, then the host kicks it off.';
  const room = el('section', { class: 'glass on-card on-roomcard', 'aria-label': 'Your room' },
    el('div', { class: 'on-card-h' }, el('h2', { text: title }), sub ? el('span', { text: sub }) : null),
    kindControl,
    el('div', { class: 'on-coderow' },
      el('strong', { class: 'on-codebig', text: r.code }),
      el('div', { class: 'on-bt' }, copyButton('copy-code', 'Copy code', r.code), inviteLink(r.code) ? copyButton('copy-invite', 'Copy invite link', inviteLink(r.code)) : null)),
    el('span', { class: 'k-label', text: capacity > 2 ? `Your crew · ${r.members.length} of ${capacity}` : 'Your crew' }),
    members,
    // Start, ready and Leave sit at the foot of the room card, the note under them (Adam 10-20: one card, not a loose row).
    el('div', { class: 'on-roomfoot' }, row, el('p', { class: 'on-note', text: note })));
  // on-talk: the chat card takes the room card's height and scrolls inside it (pages/online.css), never the page.
  const talk = el('section', { class: 'glass on-card on-talk', 'aria-label': 'Chat' },
    el('div', { class: 'on-card-h' }, el('h2', { text: 'Chat' })),
    chatBox(`room:${r.id}`, chat, (t) => act('mm_room_action', { roomId: r.id, action: 'chat', value: t })));
  return [room, talk];
}
