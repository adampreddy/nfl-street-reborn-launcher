// The file picker drawn inside the window, for Steam Deck Game Mode, where the
// native dialog opens no window and a "choose your game" step could not be
// finished (Deck tester, 2026-10). The core asks with a `pick-path-request`
// event (window_picker.rs) and waits for `pick_path_answer`; outside Game Mode
// the core uses the native dialog and this never shows.
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

function style() {
  if (document.getElementById('window-picker-style')) return;
  const s = node('style');
  s.id = 'window-picker-style';
  s.textContent = `
.wp-veil{position:fixed;inset:0;z-index:9999;background:rgba(0,0,0,.6);display:flex;align-items:center;justify-content:center;padding:16px}
.wp{width:min(880px,100%);max-height:100%;display:flex;flex-direction:column;gap:10px;background:#16181c;border:1px solid rgba(255,255,255,.14);border-radius:12px;padding:18px;color:#eceee4;font:13px/1.4 Inter,system-ui,sans-serif}
.wp h2{margin:0;font-size:18px}
.wp-where{color:#9a9c96;font-size:12px;overflow-wrap:anywhere}
.wp-places,.wp-actions{display:flex;flex-wrap:wrap;gap:8px}
.wp button{font:inherit;color:inherit;background:rgba(255,255,255,.06);border:1px solid rgba(255,255,255,.14);border-radius:6px;padding:8px 12px;cursor:pointer;text-align:left}
.wp button:focus-visible,.wp button:hover{border-color:#eceee4;outline:none;background:rgba(255,255,255,.12)}
.wp-list{overflow:auto;display:flex;flex-direction:column;gap:4px;min-height:120px;flex:1}
.wp-list button{display:flex;justify-content:space-between;gap:12px}
.wp-list .wp-dir::before{content:"\\1F4C1  "}
.wp-size{color:#9a9c96;white-space:nowrap}
.wp-none{color:#9a9c96;padding:12px 2px}
.wp-actions{justify-content:flex-end}
.wp-actions .wp-primary{background:#eceee4;color:#16181c;border-color:#eceee4}`;
  document.head.append(s);
}

function finish(path) {
  if (!open) return;
  open.veil.remove();
  document.removeEventListener('keydown', open.onKey, true);
  open = null;
  Promise.resolve(bridge.invoke('pick_path_answer', { path: path || null })).catch(() => {});
}

async function show(req) {
  if (open) finish(null);
  style();
  const exts = (req.extensions || []).map(e => e.toLowerCase());
  const matches = name => !exts.length || exts.some(e => name.toLowerCase().endsWith('.' + e));
  const veil = node('div', 'wp-veil');
  const box = node('div', 'wp');
  box.setAttribute('role', 'dialog');
  box.setAttribute('aria-modal', 'true');
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
  actions.append(cancel, choose);
  box.append(title, where, places, list, actions);
  veil.append(box);
  document.body.append(veil);
  const onKey = e => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); finish(null); } };
  document.addEventListener('keydown', onKey, true);
  open = { veil, onKey };

  async function go(path) {
    let listing;
    try { listing = await bridge.invoke('browse_dir', { path: path || null }); }
    catch (e) { where.textContent = String(e && e.message || e); return; }
    where.textContent = listing.path;
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
