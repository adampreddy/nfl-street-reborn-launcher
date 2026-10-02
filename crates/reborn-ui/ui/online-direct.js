// The direct (IP / host code) Netplay form, in a dialog (spec 2026-10-02 §3.1). online.js owns the form and the room
// and is unchanged; this file only opens and closes the dialog around #screen:
//   - "Direct connection ›" (#direct-open) opens it; Close, Esc and B close it;
//   - it opens by itself while a direct room is open (online.js shows .netplay-lobby; the room was always on screen
//     before the redesign, so it stays on screen now) and when an invite filled the form (online.js's
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
  new MutationObserver(() => { if (lobbyShown()) open(); }).observe(dialog, { subtree: true, attributes: true, attributeFilter: ['hidden'] });
  new MutationObserver(() => {
    if (document.body.dataset.mmBusy === 'true' && dialog.open && !lobbyShown()) dialog.close();
  }).observe(document.body, { attributes: true, attributeFilter: ['data-mm-busy'] });
}
