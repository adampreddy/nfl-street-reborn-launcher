// The direct (IP / host code) Netplay form, in a dialog (spec 2026-10-02 §3.1). online.js owns the form and the room
// and is unchanged; this file only opens and closes the dialog around #screen:
//   - "Direct connection ›" (#direct-open) opens it; Close, Esc and B close it;
//   - it opens by itself when a direct room opens (online.js shows .netplay-lobby: the hidden-to-shown change only,
//     since online.js re-sets `hidden` on every 750 ms poll; after Close the room stays closed and the page and the
//     nav stay usable, and "Direct connection ›" reopens it) and when an invite filled the form (online.js's
//     `netplay-invite` event);
//   - it closes when matchmaking takes the player (body[data-mm-busy]) and no direct room is open.
//   - never for a room matchmaking opened: online.js draws every Netplay room, matchmade or not, into #screen, and
//     data-mm-busy can lag the room (10-02: the dialog popped up over a matchmade match). netplay_status says
//     `matchmade`; such a room never opens the dialog and closes it if it is open.
import { bridge, call } from './shell.js';

const dialog = document.getElementById('direct-dialog');
const opener = document.getElementById('direct-open');
const closer = document.getElementById('direct-close');

function open() {
  if (dialog && !dialog.open) dialog.showModal();
}
const lobbyShown = () => {
  const lobby = dialog?.querySelector('.netplay-lobby');
  return !!lobby && !lobby.hidden;
};

opener?.addEventListener('click', open);
closer?.addEventListener('click', () => dialog?.close());
// Not while matchmaking has the player: the filled form waits behind "Direct connection ›".
document.addEventListener('netplay-invite', () => { if (document.body.dataset.mmBusy !== 'true') open(); });
dialog?.addEventListener('close', () => { if (opener && !opener.closest('[hidden]')) opener.focus({ preventScroll: true }); });
// The lobby's room: one status read, only when it appears. No bridge (a plain browser) means no matchmaking.
async function openUnlessMatchmade() {
  let matchmade = false;
  try { matchmade = !!bridge && (await call('netplay_status'))?.matchmade === true; } catch { /* unknown: a direct room */ }
  if (matchmade) { if (dialog?.open) dialog.close(); return; }
  if (document.body.dataset.mmBusy !== 'true' && lobbyShown()) open();
}
if (dialog && typeof MutationObserver === 'function') {
  let wasShown = lobbyShown();
  new MutationObserver(() => {
    const shown = lobbyShown();
    if (shown && !wasShown) openUnlessMatchmade();
    wasShown = shown;
  }).observe(dialog, { subtree: true, attributes: true, attributeFilter: ['hidden'] });
  new MutationObserver(() => {
    if (document.body.dataset.mmBusy === 'true' && dialog.open && !lobbyShown()) dialog.close();
  }).observe(document.body, { attributes: true, attributeFilter: ['data-mm-busy'] });
}
