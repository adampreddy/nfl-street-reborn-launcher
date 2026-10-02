// The direct (IP / host code) Netplay form, in a dialog (spec 2026-10-02 §3.1). online.js owns the form and the room
// and is unchanged; this file only opens and closes the dialog around #screen:
//   - "Direct connection ›" (#direct-open) opens it; Close, Esc and B close it;
//   - it opens by itself when a direct room opens (online.js shows .netplay-lobby: the hidden-to-shown change only,
//     since online.js re-sets `hidden` on every 750 ms poll; after Close the room stays closed and the page and the
//     nav stay usable, and "Direct connection ›" reopens it) and when an invite filled the form (online.js's
//     `netplay-invite` event);
//   - it closes when matchmaking takes the player (body[data-mm-busy]) and no direct room is open.
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
document.addEventListener('netplay-invite', open);
dialog?.addEventListener('close', () => { if (opener && !opener.closest('[hidden]')) opener.focus({ preventScroll: true }); });
if (dialog && typeof MutationObserver === 'function') {
  let wasShown = lobbyShown();
  new MutationObserver(() => {
    const shown = lobbyShown();
    if (shown && !wasShown) open();
    wasShown = shown;
  }).observe(dialog, { subtree: true, attributes: true, attributeFilter: ['hidden'] });
  new MutationObserver(() => {
    if (document.body.dataset.mmBusy === 'true' && dialog.open && !lobbyShown()) dialog.close();
  }).observe(document.body, { attributes: true, attributeFilter: ['data-mm-busy'] });
}
