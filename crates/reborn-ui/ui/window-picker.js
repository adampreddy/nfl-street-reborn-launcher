// The file picker drawn inside the window, for Steam Deck Game Mode, where the
// native dialog opens no window and a "choose your game" step could not be
// finished (Deck tester, 2026-10). The core asks with a `pick-path-request`
// event (window_picker.rs) and waits for `pick_path_answer`; outside Game Mode
// the core uses the native dialog and this never shows.
//
// It is a modal <dialog>, so the launcher's own pad navigation (app-shell.js)
// drives it like every other dialog: the D-pad moves inside it, A presses the
// focused button, and B arrives as the dialog's `cancel`, which goes up a
// folder (and cancels at the top). It reads no pad itself: in 3.0.39 it did,
// beside app-shell, and one press acted twice (focus stuck, B left the page).
// The arrow keys move too, in case Steam maps the D-pad to keys.
import { bridge } from './shell.js';

const ev = window.__TAURI__ && window.__TAURI__.event;
let open = null;

function node(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
}

function size(bytes) {
  if (bytes >= 1073741824) return (bytes / 1073741824).toFixed(2) + ' GB';
  if (bytes >= 1048576) return Math.round(bytes / 1048576) + ' MB';
  return Math.max(1, Math.round(bytes / 1024)) + ' KB';
}

// Surface, border, radius, padding and backdrop come from theme.css's dialog
// rule; this sizes the picker and lays out its parts.
function style() {
  if (document.getElementById('window-picker-style')) return;
  const s = node('style');
  s.id = 'window-picker-style';
  s.textContent = `
dialog.wp[open]{width:min(880px,calc(100vw - 32px));max-height:calc(100vh - 32px);box-sizing:border-box;display:flex;flex-direction:column;gap:8px;font:13px/1.4 Inter,system-ui,sans-serif}
.wp h2{margin:0 0 4px;font-size:18px;font-weight:600}
.wp-where{color:var(--fg-3);font-size:12px;overflow-wrap:anywhere}
.wp-places,.wp-actions{display:flex;flex-wrap:wrap;gap:8px}
.wp button{font:inherit;color:var(--fg);background:var(--glass-2);border:1px solid var(--stroke);border-radius:var(--r-sm);padding:8px 12px;cursor:pointer;text-align:left}
.wp button:hover{background:var(--glass-3)}
.wp-list{overflow:auto;display:flex;flex-direction:column;gap:4px;min-height:120px;flex:1;margin:8px 0}
.wp-list button{display:flex;justify-content:space-between;gap:16px;flex:none}
.wp-list .wp-dir::before{content:"\\1F4C1  "}
.wp-size{color:var(--fg-3);white-space:nowrap}
.wp-none{color:var(--fg-3);padding:12px 2px}
.wp-hint{color:var(--fg-3);font-size:12px;margin-right:auto;align-self:center}
.wp-actions .wp-primary{background:var(--fg);color:var(--surface-solid);border-color:var(--fg)}
html:not(#_) dialog.wp h2{text-transform:none;letter-spacing:0;font-size:18px}
html:not(#_) dialog.wp .wp-list button{justify-content:flex-start;text-align:left}
html:not(#_) dialog.wp .wp-list .wp-size{margin-left:auto}`;
  document.head.append(s);
}

function finish(path) {
  if (!open) return;
  const { box, onKey } = open;
  open = null;
  document.removeEventListener('keydown', onKey, true);
  if (box.open) box.close();
  box.remove();
  Promise.resolve(bridge.invoke('pick_path_answer', { path: path || null })).catch(() => {});
}

async function show(req) {
  if (open) finish(null);
  style();
  const exts = (req.extensions || []).map(e => e.toLowerCase());
  const matches = name => !exts.length || exts.some(e => name.toLowerCase().endsWith('.' + e));
  const box = node('dialog', 'wp');
  box.setAttribute('aria-label', req.title || 'Choose a file');
  const title = node('h2', null, req.title || 'Choose a file');
  const where = node('div', 'wp-where');
  const places = node('div', 'wp-places');
  const list = node('div', 'wp-list');
  const actions = node('div', 'wp-actions');
  const cancel = node('button', null, 'Cancel');
  cancel.type = 'button';
  cancel.onclick = () => finish(null);
  const choose = node('button', 'wp-primary', 'Choose this folder');
  choose.type = 'button';
  choose.hidden = !req.folder;
  const hint = node('span', 'wp-hint', 'Controller: D-pad to move · A to open · B to go up');
  actions.append(hint, cancel, choose);
  box.append(title, where, places, list, actions);
  document.body.append(box);

  let parent = null;
  function back() {
    if (parent) go(parent); else finish(null);
  }
  // B (app-shell dispatches `cancel` on the open dialog, and closes it only
  // when nobody prevents that) goes up a folder. Escape cancels outright.
  box.addEventListener('cancel', e => { e.preventDefault(); back(); });
  // The list scrolls inside the dialog; keep the focused row in view however
  // focus got there (pad, arrow keys, Tab).
  box.addEventListener('focusin', e => { if (list.contains(e.target)) e.target.scrollIntoView({ block: 'nearest' }); });
  const buttons = () => [...box.querySelectorAll('button')].filter(b => !b.hidden);
  function move(step) {
    const all = buttons();
    if (!all.length) return;
    const at = all.indexOf(document.activeElement);
    all[at < 0 ? 0 : Math.max(0, Math.min(all.length - 1, at + step))].focus();
  }
  const onKey = e => {
    let done = true;
    if (e.key === 'Escape') finish(null);
    else if (e.key === 'ArrowDown' || e.key === 'ArrowRight') move(1);
    else if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') move(-1);
    else if (e.key === 'Backspace') back();
    else done = false;
    if (done) { e.preventDefault(); e.stopPropagation(); }
  };
  document.addEventListener('keydown', onKey, true);
  open = { box, onKey };
  box.showModal();

  async function go(path) {
    let listing;
    try { listing = await bridge.invoke('browse_dir', { path: path || null }); }
    catch (e) { where.textContent = String(e && e.message || e); return; }
    if (!open || open.box !== box) return;
    where.textContent = listing.path;
    parent = listing.parent || null;
    choose.onclick = () => finish(listing.path);
    places.replaceChildren(...listing.places.map(p => {
      const b = node('button', null, p.label);
      b.type = 'button';
      b.onclick = () => go(p.path);
      return b;
    }));
    const rows = [];
    if (listing.parent) {
      const up = node('button', 'wp-dir', 'Up one folder');
      up.type = 'button';
      up.onclick = () => go(listing.parent);
      rows.push(up);
    }
    for (const e of listing.entries) {
      if (!e.dir && (req.folder || !matches(e.name))) continue;
      const b = node('button', e.dir ? 'wp-dir' : null);
      b.type = 'button';
      b.append(node('span', null, e.name));
      if (!e.dir) b.append(node('span', 'wp-size', size(e.bytes)));
      b.onclick = () => (e.dir ? go(e.path) : finish(e.path));
      rows.push(b);
    }
    if (rows.length <= 1 && !listing.entries.some(e => e.dir)) {
      rows.push(node('div', 'wp-none', exts.length ? `No ${exts.join(' / ')} files here.` : 'This folder is empty.'));
    }
    list.replaceChildren(...rows);
    (list.querySelector('button') || cancel).focus();
  }
  go(null);
}

if (ev && typeof ev.listen === 'function') {
  ev.listen('pick-path-request', e => { show(e.payload || {}); }).catch(() => {});
}
