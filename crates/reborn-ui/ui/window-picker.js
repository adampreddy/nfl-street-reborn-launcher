// The file picker drawn inside the window, for Steam Deck Game Mode, where the
// native dialog opens no window and a "choose your game" step could not be
// finished (Deck tester, 2026-10). The core asks with a `pick-path-request`
// event (window_picker.rs) and waits for `pick_path_answer`; outside Game Mode
// the core uses the native dialog and this never shows.
//
// It is driven by the controller too (Deck tester: only the trackpad mouse
// moved through the folders): D-pad or left stick moves, A opens or chooses,
// B goes up a folder. The arrow keys do the same, in case Steam maps the
// D-pad to keys.
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
.wp-hint{color:#9a9c96;font-size:12px;margin-right:auto;align-self:center}
.wp-actions{justify-content:flex-end}
.wp-actions .wp-primary{background:#eceee4;color:#16181c;border-color:#eceee4}`;
  document.head.append(s);
}

function finish(path) {
  if (!open) return;
  open.veil.remove();
  document.removeEventListener('keydown', open.onKey, true);
  cancelAnimationFrame(open.frame);
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
  const hint = node('span', 'wp-hint', 'Controller: D-pad to move · A to open · B to go up');
  actions.append(hint, cancel, choose);
  box.append(title, where, places, list, actions);
  veil.append(box);
  document.body.append(veil);
  let parent = null;
  // Every button in reading order: places, the folder list, then the actions.
  const buttons = () => [...box.querySelectorAll('button')].filter(b => !b.hidden);
  function move(step) {
    const all = buttons();
    if (!all.length) return;
    const at = all.indexOf(document.activeElement);
    const next = all[at < 0 ? 0 : Math.max(0, Math.min(all.length - 1, at + step))];
    next.focus();
    next.scrollIntoView({ block: 'nearest' });
  }
  function back() {
    if (parent) go(parent); else finish(null);
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
  open = { veil, onKey, frame: 0 };

  // The controller, read directly (standard mapping: 0 A, 1 B, 12-15 D-pad,
  // axis 1 the left stick's up/down). A button held when the picker opened
  // (the A that pressed "Browse") counts only after it is let go. Holding a
  // direction repeats, like a keyboard.
  const held = new Set();
  let first = true;
  let repeatAt = 0;
  function poll(now) {
    if (!open || open.veil !== veil) return;
    let pads = [];
    try { pads = [...(navigator.getGamepads?.() || [])].filter(p => p?.connected); } catch { pads = []; }
    const down = new Set();
    for (const p of pads) {
      const b = i => !!(p.buttons[i] && p.buttons[i].pressed);
      const y = p.axes[1] || 0;
      const x = p.axes[0] || 0;
      if (b(0)) down.add('a');
      if (b(1)) down.add('b');
      if (b(13) || b(15) || y > 0.6 || x > 0.6) down.add('next');
      if (b(12) || b(14) || y < -0.6 || x < -0.6) down.add('prev');
    }
    if (first) { for (const k of down) held.add(k); first = false; }
    for (const k of [...held]) if (!down.has(k)) held.delete(k);
    for (const k of down) {
      const dir = k === 'next' ? 1 : k === 'prev' ? -1 : 0;
      if (!held.has(k)) {
        held.add(k);
        if (dir) { move(dir); repeatAt = now + 350; }
        else if (k === 'a') { const f = document.activeElement; if (f && box.contains(f) && f.tagName === 'BUTTON') f.click(); else move(0); }
        else if (k === 'b') back();
        if (!open || open.veil !== veil) return;
      } else if (dir && now >= repeatAt) {
        move(dir);
        repeatAt = now + 110;
      }
    }
    open.frame = requestAnimationFrame(poll);
  }
  open.frame = requestAnimationFrame(poll);

  async function go(path) {
    let listing;
    try { listing = await bridge.invoke('browse_dir', { path: path || null }); }
    catch (e) { where.textContent = String(e && e.message || e); return; }
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
