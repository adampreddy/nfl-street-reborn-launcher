// Controller navigation belongs to the window, independently of page data.
import { bridge } from './shell.js';
import { MenuPadReader, readLayout } from './menu-pad.js';
import { mountMenuPadSetup } from './menu-pad-setup.js';

// Tell Discord presence which page is open. Fire-and-forget and fully guarded: presence must never
// be able to break a page, and the Playwright checkers run these pages against a stubbed bridge that
// may not know this command at all - may reject, may return undefined, may throw.
try {
  const page = (document.body.className.match(/(?:^|\s)page-([a-z]+)/) || [])[1] || 'home';
  if (bridge && typeof bridge.invoke === 'function') {
    Promise.resolve(bridge.invoke('presence_page', { page })).catch(() => {});
  }
} catch { /* never a reason for a page not to load */ }

const selector = 'a[href],button,input,select,textarea,summary';
const main = document.getElementById('main');
const screen = document.getElementById('screen');
const hints = document.getElementById('input-hints');
// Pages with a save bar keep it in its own lane after #main (spec §7). It is content too: the pad reaches
// it from the bottom of the page, and Back from it goes to the sidebar, never off the page.
const saveLane = () => document.getElementById('save-lane');
const inContent = n => !!n && (main.contains(n) || !!saveLane()?.contains(n));
// Pending changes are never dropped silently: while the bar is open, leaving the page is refused and
// the save bar (which states how many changes are unsaved) takes focus instead.
const pending = () => !!saveLane()?.classList.contains('open');
let calledTimer = 0;
function uncall() { clearTimeout(calledTimer); document.querySelector('.save-bar.called')?.classList.remove('called'); }
// Save first; with both buttons locked (savebar.js lock()) the bar itself, so focus never goes nowhere.
const barTarget = lane => lane && (reachable(lane).find(n => n.matches('.primary')) || reachable(lane)[0] || lane.querySelector('.save-bar[tabindex]'));
function toSaveBar() {
  const lane = saveLane();
  const target = barTarget(lane);
  if (target) focus(target, false);
  // A visible cue for mouse users too (the pad ring only shows in controller mode): Save is outlined briefly.
  const bar = lane?.querySelector('.save-bar');
  if (bar) { uncall(); bar.classList.add('called'); calledTimer = setTimeout(uncall, 800); }
}
document.addEventListener('pointerdown', uncall, true);
// Scripted navigation (update-notice.js "View updates") is no <a> click, so the guard below never sees
// it: such callers ask first and, while changes are pending, point here instead of leaving.
window.pointAtSaveBar = toSaveBar;
// Any link that would leave this page (sidebar, "Resume game setup", "Developer tools", a reload of this
// page) is refused while changes are pending. In-page anchors (#…) and links opened elsewhere still work.
function leavesPage(link) {
  if (link.hasAttribute('download') || (link.target && link.target !== '_self')) return false;
  let url; try { url = new URL(link.href, location.href); } catch (_) { return false; }
  const samePage = url.origin === location.origin && url.pathname === location.pathname && url.search === location.search;
  return !(samePage && url.hash);
}
document.addEventListener('click', e => {
  if (!pending() || !(e.target instanceof Element)) return;
  const link = e.target.closest('a[href]');
  if (!link || !leavesPage(link)) return;
  e.preventDefault(); e.stopImmediatePropagation();
  toSaveBar();
}, true);
let active = false, checking = false, lastFocus = null, contentFocus = null, selectEdit = null;
let controllerMode = false, restorePending = false;
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
const scrollTargets = new Map();
let scrollFrame = 0, scrollTime = 0;
function stopScrolling() {
  cancelAnimationFrame(scrollFrame); scrollFrame = 0; scrollTime = 0; scrollTargets.clear();
}
function animateScroll(now) {
  if (document.hidden || !document.hasFocus() || screen?.getAttribute("aria-busy")==="true" || document.body.dataset.controllerTesting) { stopScrolling(); return; }
  const dt = Math.min(40, now - (scrollTime || now - 16)); scrollTime = now;
  // One continuously retargeted curve, rather than restarting a browser smooth
  // scroll every time the D-pad repeats. Time-based damping works at any refresh rate.
  const blend = 1 - Math.exp(-dt / 65);
  const modal = document.querySelector('dialog[open]');
  for (const [area, requested] of scrollTargets) {
    // A modal owns scrolling while open; discard momentum on the page behind it.
    if (!area.isConnected || area.matches('dialog:not([open])') || (modal && !modal.contains(area))) { scrollTargets.delete(area); continue; }
    const target = Math.max(0, Math.min(area.scrollHeight-area.clientHeight, requested));
    const distance = target-area.scrollTop;
    if (reducedMotion.matches || Math.abs(distance)<.7) { area.scrollTop=target; scrollTargets.delete(area); }
    else area.scrollTop += distance*blend;
  }
  if (scrollTargets.size) scrollFrame=requestAnimationFrame(animateScroll);
  else { scrollFrame=0; scrollTime=0; }
}
function scrollToTarget(area, target) {
  target=Math.max(0,Math.min(area.scrollHeight-area.clientHeight,target));
  if (reducedMotion.matches) { scrollTargets.delete(area); area.scrollTop=target; return; }
  scrollTargets.set(area,target);
  if (!scrollFrame) scrollFrame=requestAnimationFrame(animateScroll);
}
document.addEventListener('wheel',stopScrolling,{passive:true});
document.addEventListener('pointerdown',stopScrolling,{passive:true});
document.addEventListener('keydown',stopScrolling);
document.addEventListener('visibilitychange',stopScrolling);
window.addEventListener('blur',stopScrolling);
window.addEventListener('pagehide',stopScrolling);
reducedMotion.addEventListener('change',stopScrolling);
try { controllerMode = sessionStorage.getItem('reborn:input') === 'controller'; } catch (_) {}
syncInputMode();

function visible(n) {
  const modal=document.querySelector('dialog[open]');
  return (!modal||modal.contains(n)) && n?.isConnected && !n.disabled && n.getAttribute('aria-disabled') !== 'true' && n.tabIndex >= 0 && n.getClientRects().length > 0 &&
    !n.closest('[hidden],[inert]') && getComputedStyle(n).visibility !== 'hidden' &&
    !n.classList.contains('skip-link') && !n.closest('details.core:not([open])');
}
function reachable(root = document) { return [...root.querySelectorAll(selector)].filter(visible); }
function identity(n) { return n.id ? `id:${n.id}` : [n.tagName,n.getAttribute('name'),n.value,n.getAttribute('href'),n.textContent?.trim()].join('|'); }
function box(n) { return (n.closest('label.choice') || n).getBoundingClientRect(); }
function syncInputMode() {
  document.body.classList.toggle('gamepad-input', controllerMode);
  // The skip link remains available to keyboard users, outside pad navigation.
  document.querySelectorAll('.skip-link').forEach(n => { n.tabIndex = controllerMode ? -1 : 0; });
}
function connectedPads() {
  try { return [...(navigator.getGamepads?.() || [])].filter(p => p?.connected); }
  catch (_) { return []; }
}
function renderHints() {
  const pad = connectedPads()[0];
  document.body.classList.toggle('controller-connected', !!pad);
  if (!hints) return;
  if (!pad) { hints.replaceChildren(); delete hints.dataset.layout; return; }
  const ps = /playstation|dualsense|dualshock|054c/i.test(pad.id);
  const testing = !!document.body.dataset.controllerTesting;
  const needsSetup = pad.mapping !== 'standard' && !readLayout(pad);
  // Prompts are for whoever is holding the controller (spec §6.3). A pad the launcher
  // cannot read yet still says how to set it up, or it could never be set up.
  if (!controllerMode && !testing && !needsSetup) { hints.replaceChildren(); delete hints.dataset.layout; return; }
  const items = needsSetup && !testing ? [['Setup','Set up launcher controls']] : testing
    ? [[document.body.classList.contains('page-controls') ? 'Esc' : ps ? 'Share + Options' : 'View + Menu', 'Finish test']]
    : selectEdit ? [['D-pad','Choose'],[ps?'✕':'A','Apply'],[ps?'○':'B','Cancel']]
    : [['D-pad','Navigate'],[ps?'✕':'A','Select'],[ps?'○':'B','Back'],...(document.body.classList.contains('page-setup') ? [] : [[ps?'L1 / R1':'LB / RB','Pages'],['Right stick','Scroll']])];
  const key = JSON.stringify(items);
  if (hints.dataset.layout === key) return;
  hints.dataset.layout = key;
  hints.replaceChildren(...items.map(([key,label]) => {
    const item = document.createElement('span'); item.className = 'input-hint';
    const button = document.createElement('kbd'); button.textContent = key;
    item.append(button,document.createTextNode(' '+label)); return item;
  }));
}

function mode(value) {
  if (controllerMode === value) { renderHints(); return; }
  controllerMode = value;
  syncInputMode();
  try { sessionStorage.setItem('reborn:input',value ? 'controller' : 'keyboard'); } catch (_) {}
  renderHints();
}
function focus(n, reveal = true) {
  // The locked save bar (tabindex -1, no enabled button) is a valid target though visible() skips it.
  if (!visible(n) && !(n?.matches('.save-bar[tabindex]') && !n.closest('[inert]'))) return;
  document.querySelectorAll('.pad-focus').forEach(x => x.classList.remove('pad-focus'));
  n.focus({preventScroll:true});
  n.classList.add('pad-focus');
  // The save lane never scrolls; everything else scrolls its own area to show the control.
  const area = n.closest('dialog[open]') || n.closest('.shell-nav') || (main.contains(n) ? main : null);
  if (reveal && area) {
    const target = n.closest('label.choice') || n;
    const rect = target.getBoundingClientRect(), viewport = area.getBoundingClientRect();
    const top = viewport.top + 16, bottom = viewport.bottom - 16;
    const delta = rect.top < top ? rect.top-top : rect.bottom > bottom ? rect.bottom-bottom : 0;
    if (delta) scrollToTarget(area,area.scrollTop+delta);
  }
}
function primary() {
  const scope = document.querySelector('dialog[open]') || main;
  return reachable(scope).find(n => n.matches('[data-primary],#next')) ||
    reachable(scope).find(n => n.matches('input:checked')) || reachable(scope)[0];
}
document.addEventListener('focusin',e => {
  const n = e.target;
  if (n.classList.contains('skip-link')) {
    if (controllerMode) focus(primary(), false);
    return;
  }
  if (!n.matches(selector)) return;
  document.querySelectorAll('.pad-focus').forEach(x => x.classList.remove('pad-focus'));
  n.classList.add('pad-focus');
  lastFocus = {node:n,key:identity(n),index:reachable().indexOf(n)};
  if (inContent(n)) contentFocus = lastFocus;
});
new MutationObserver(() => {
  if (restorePending) return;
  restorePending = true;
  queueMicrotask(() => {
    restorePending = false;
    if (document.body.dataset.controllerTesting || screen.getAttribute('aria-busy') === 'true') return;
    if (lastFocus && !lastFocus.node.isConnected) {
      const items = reachable();
      if ([...document.querySelectorAll('button:disabled,input:disabled,select:disabled')].some(n => identity(n) === lastFocus.key)) return;
      focus(items.find(n => identity(n) === lastFocus.key) || primary() || items[Math.min(lastFocus.index,items.length-1)], false);
    } else if (controllerMode && !visible(document.activeElement)) focus(visible(lastFocus?.node) ? lastFocus.node : primary(), false);
  });
}).observe(screen,{childList:true,subtree:true});

// Keep the mounted controls and layout while a request is in flight. Rebuilding
// just to disable controls caused a visible intermediate frame on every click.
window.setLauncherBusy = value => {
  if (value) {
    screen.setAttribute('aria-busy','true');
    screen.querySelectorAll('button,input,select,textarea').forEach(n => { n.disabled = true; });
  } else screen.removeAttribute('aria-busy');
};

function move(direction) {
  const current = document.activeElement;
  if (selectEdit) {
    if(selectEdit.node.matches('input[type=range]')){const n=selectEdit.node,step=Number(n.step)||1;n.value=String(Math.max(Number(n.min)||0,Math.min(Number(n.max)||100,Number(n.value)+step*(direction==='left'||direction==='down'?-1:1))));n.dispatchEvent(new Event('input',{bubbles:true}));return;}
    if (direction === 'up' || direction === 'down') {
      const options = [...selectEdit.node.options].filter(o => !o.disabled && !o.hidden);
      const index = options.indexOf(selectEdit.node.selectedOptions[0]);
      const next = options[Math.max(0,Math.min(options.length-1,index+(direction === 'up' ? -1 : 1)))];
      if (next) selectEdit.node.value = next.value;
    }
    return;
  }
  if (!visible(current)) { focus(primary()); return; }
  const inNav = !!current.closest('.shell-nav');
  if (inNav && direction === 'right') {
    const content = [...reachable(main), ...(saveLane() ? reachable(saveLane()) : [])];
    focus(contentFocus && content.find(n => identity(n) === contentFocus.key) || primary()); return;
  }
  // Keep vertical movement within content or navigation. A distant sidebar
  // control should never steal focus from the next settings row.
  const modal = document.querySelector('dialog[open]');
  const lane = saveLane(), inLane = !modal && !!lane?.contains(current);
  // From the save bar, Up goes back into the page; Left and Right stay on the bar.
  const items = reachable(modal || (inNav ? document.querySelector('.shell-nav') : inLane && direction !== 'up' ? lane : main));
  const a = box(current), ax = a.x+a.width/2, ay = a.y+a.height/2;
  const viewport = (current.closest('dialog[open]') || main).getBoundingClientRect();
  if (!inNav && !inLane && (a.bottom < viewport.top || a.top > viewport.bottom)) {
    const onScreen = items.filter(n => { const b = box(n); return b.top >= viewport.top && b.bottom <= viewport.bottom; });
    if (onScreen.length) { focus(direction === 'up' ? onScreen.at(-1) : onScreen[0]); return; }
  }
  const horizontal = direction === 'left' || direction === 'right';
  const sign = direction === 'left' || direction === 'up' ? -1 : 1;
  const ranked = items.filter(n => n !== current).map(n => {
    const b = box(n), dx = b.x+b.width/2-ax, dy = b.y+b.height/2-ay;
    const distance = (horizontal ? dx : dy)*sign;
    const overlap = horizontal ? Math.min(a.bottom,b.bottom)-Math.max(a.top,b.top) : Math.min(a.right,b.right)-Math.max(a.left,b.left);
    const cross = Math.abs(horizontal ? dy : dx);
    return {n,distance,overlap,score:distance+cross*2};
  }).filter(x => x.distance > 4 && (!horizontal || x.overlap > 0));
  ranked.sort((a,b) => Number(b.overlap > 0)-Number(a.overlap > 0) || a.score-b.score);
  // Only where a page asks for it (data-nav="rows": Settings, Mods): a control in the same column far below
  // must not jump over nearer rows whose controls sit in another column (tabs on the left, each row's
  // switch on the right). When the column pick is more than a row farther than the nearest control wholly
  // above/below, the nearest row wins. Controls beside this one (gap < 0) never count, so it never moves
  // sideways. Every other page keeps plain column moves (the Controllers grid depends on them).
  if (!horizontal && ranked.length > 1 && current.closest('[data-nav="rows"]')) {
    const gap = x => { const b = box(x.n); return sign > 0 ? b.top-a.bottom : a.top-b.bottom; };
    const clear = ranked.filter(x => gap(x) >= 0);
    const nearest = clear.length ? Math.min(...clear.map(gap)) : Infinity;
    if (clear.length && gap(ranked[0]) > nearest+48) ranked.unshift(clear.filter(x => gap(x) <= nearest+8).sort((p,q) => p.score-q.score)[0]);
  }
  if (ranked[0]) focus(ranked[0].n);
  // Nothing further down the page: the save bar, while it is open, comes next (Save first).
  else if (!modal && !inNav && !inLane && direction === 'down' && lane && pending() && barTarget(lane)) focus(barTarget(lane));
  else if (!modal && !inNav && direction === 'left') focus(document.querySelector('.shell-nav a.current') || reachable(document.querySelector('.shell-nav'))[0]);
}
function endSelect(commit) {
  if (!selectEdit) return;
  const {node,original} = selectEdit; selectEdit = null;
  node.classList.remove('pad-editing');
  if (!commit) {node.value = original;node.dispatchEvent(new Event('input',{bubbles:true}));}
  else if (node.value !== original) { node.dispatchEvent(new Event('input',{bubbles:true})); node.dispatchEvent(new Event('change',{bubbles:true})); }
  renderHints();
}
function activate() {
  const n = document.activeElement;
  if (!visible(n)) { focus(primary()); return; }
  if (selectEdit) { endSelect(true); return; }
  if (n.matches('select,input[type=range]')) {
    selectEdit = {node:n,original:n.value}; n.classList.add('pad-editing'); renderHints(); return;
  }
  if (n.matches('input[type="text"],input[type="password"],textarea')) {
    // File paths use the native picker; do not pretend an untrusted click can
    // summon an OS keyboard. Manual typing remains available via keyboard.
    const browse = n.closest('.path-entry')?.querySelector('button');
    if (visible(browse)) { focus(browse); browse.click(); }
    return;
  }
  n.click();
}
function back() {
  if (selectEdit) { endSelect(false); return; }
  const modal=document.querySelector('dialog[open]');if(modal){if(modal.dispatchEvent(new Event('cancel',{cancelable:true})))modal.close();return;}
  const current = document.activeElement;
  const fold = current?.closest('details[open]') || document.querySelector('#game-options[open]');
  if (fold) {
    fold.open = false;
    focus(visible(fold.querySelector('summary')) ? fold.querySelector('summary') : document.querySelector('a[href="#game-options"]'));
    return;
  }
  const wizardBack = document.getElementById('back');
  if (visible(wizardBack)) { wizardBack.click(); return; }
  if (inContent(current)) { focus(document.querySelector('.shell-nav a.current') || document.querySelector('.shell-nav a[href="index.html"]')); return; }
  // Back from the sidebar would leave the page: never while changes are pending.
  if (pending()) { toSaveBar(); return; }
  if (!location.pathname.endsWith('/index.html') && location.pathname !== '/') location.href = 'index.html';
}
function pageStep(delta) {
  if (selectEdit) endSelect(false);
  // First run has saved, ordered steps; bumpers must not bypass its Next action.
  if (document.body.classList.contains('page-setup')||document.querySelector('dialog[open]')) return;
  // Unsaved changes are never dropped by a bumper: LB/RB do nothing while the save bar is open.
  if (pending()) return;
  const links = [...document.querySelectorAll('.shell-nav a:not(.brand)')].filter(visible);
  const index = links.findIndex(n => n.classList.contains('current'));
  const next = links[index+delta];
  if (next) next.click();
}
document.addEventListener('keydown',e => {
  if (e.key === 'Escape') { e.preventDefault(); back(); }
  if (e.key === 'Tab') endSelect(false);
  mode(false);
});
document.addEventListener('pointerdown',() => { endSelect(false); mode(false); });
// A real mouse move (not the synthetic one a scroll under a still cursor fires) leaves controller mode.
document.addEventListener('mousemove',e => { if (controllerMode && (Math.abs(e.movementX) + Math.abs(e.movementY)) > 2) mode(false); });
document.addEventListener('controller-test-state',() => { stopScrolling(); renderHints(); });

async function sessionState() {
  if (!bridge || checking) return;
  checking = true;
  try {
    const next = await bridge.invoke('session_active');
    // Repeat authoritative snapshots so a page mounted after a transition,
    // or a late initial read, cannot stay stuck on an old active value.
    if (typeof next === 'boolean') {
      active = next;
      document.dispatchEvent(new CustomEvent('session-state',{detail:active}));
    }
  } catch (_) {} finally { checking = false; }
}
if (bridge) { sessionState(); setInterval(sessionState,1500); }
const reader = new MenuPadReader();
function poll(now) {
  requestAnimationFrame(poll);
  if (active || document.hidden || !document.hasFocus() || screen?.getAttribute("aria-busy")==="true" || document.body.dataset.controllerTesting) {
    reader.reset(); return;
  }
  const pads = connectedPads();
  if (document.body.classList.contains('controller-connected') !== !!pads.length) renderHints();
  const input = reader.update(pads, now, readLayout);
  const hasInput = input.move || input.scroll || input.accept || input.back || input.previous || input.next;
  if (!hasInput) return;
  mode(true);
  // Enter controller navigation at the primary control. The first press must
  // not both establish focus and skip it (or launch it before it was visible).
  if (!visible(document.activeElement)) {
    focus(primary());
    if (!input.previous && !input.next && !input.back) return;
  }
  if (input.move) move(input.move);
  if (input.scroll && !selectEdit) {
    const area = document.querySelector('dialog[open]') || main;
    scrollToTarget(area,(scrollTargets.get(area) ?? area.scrollTop)+input.scroll*input.elapsed*1.35);
  }
  if (input.previous) pageStep(-1);
  else if (input.next) pageStep(1);
  else if (input.back) back();
  else if (input.accept) activate();
}
document.addEventListener('menu-pad-layout',()=>reader.reset());
mountMenuPadSetup();

renderHints();
if (controllerMode) focus(primary());
requestAnimationFrame(poll);
window.addEventListener('gamepadconnected',() => {renderHints();document.dispatchEvent(new Event('refresh-devices'));});
window.addEventListener('gamepaddisconnected',() => { reader.reset(); renderHints(); document.dispatchEvent(new Event('refresh-devices')); });
import './update-notice.js';
