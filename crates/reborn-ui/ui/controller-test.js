// Live input is read from WebView's Gamepad API. It does not change Dolphin bindings.
let active = null;
let sessionActive = false;
const names = ['A / Cross', 'B / Circle', 'X / Square', 'Y / Triangle', 'LB / L1', 'RB / R1', 'LT / L2', 'RT / R2', 'View / Share', 'Menu / Options', 'Left stick', 'Right stick', 'Up', 'Down', 'Left', 'Right', 'Guide'];
function node(tag, text, cls) {
  const n = document.createElement(tag);
  if (text) n.textContent = text;
  if (cls) n.className = cls;
  return n;
}
function textIfChanged(n, value) { if (n.textContent !== value) n.textContent = value; }
function stop() {
  if (!active) return;
  active.stop();
  active = null;
  document.body.removeAttribute('data-controller-testing');
  document.dispatchEvent(new Event('controller-test-state'));
}
function mount(host) {
  if (host.dataset.mounted) return;
  host.dataset.mounted = 'true';
  host.classList.add('block', 'controller-test');
  host.append(node('h2', 'Test your controller'), node('p', 'Press buttons and move both sticks. This checks input in the launcher; in-game bindings are configured separately.', 'note'));
  const start = node('button', 'Start controller test');
  start.type = 'button'; start.dataset.testStart = '';
  const body = node('div', '', 'test-body'); body.hidden = true;
  const status = node('p', '', 'test-status'); status.setAttribute('role', 'status');
  // Announce connection changes once, rather than repeating them every animation frame.
  function message(text) { if (status.textContent !== text) status.textContent = text; }
  const label = node('label', 'Controller to test');
  const select = node('select'); select.setAttribute('aria-label', 'Controller to test'); label.append(select);
  const note = node('p', 'Menu navigation is paused. Press View + Menu (Share + Options), Esc, or Finish to leave the test.', 'note');
  const silhouette = node('div', '', 'test-silhouette');
  let preview = null;
  const controls = node('div', '', 'test-buttons');
  const axes = node('div', '', 'test-axes');
  const seen = node('p', '', 'note');
  body.append(status, silhouette, label, note, controls, axes, seen);
  host.append(start, body);
  let frame = 0, signature = '', layout = '', tested = new Set(), lastDevice = '', exitChord = null;
  let buttons = [], axisViews = [];
  const tester = {stop() {
    cancelAnimationFrame(frame); body.hidden = true; start.textContent = 'Start controller test';
    start.setAttribute('aria-expanded', 'false'); start.focus({preventScroll:true});
  }};
  start.setAttribute('aria-expanded', 'false');
  function poll() {
    if (active !== tester) return;
    if (!host.isConnected || sessionActive) { stop(); return; }
    frame = requestAnimationFrame(poll);
    if (document.hidden || !document.hasFocus()) {
      message('Test paused. Return to this window to continue.');
      preview?.update(null);
      buttons.forEach(b => { b.classList.remove('pressed'); b.lastElementChild.textContent = '0%'; });
      axisViews.forEach(a => { a.meter.value = 0; a.value.textContent = 'Paused'; });
      return;
    }
    let pads;
    try { pads = [...(navigator.getGamepads?.() || [])].filter(p => p?.connected); }
    catch { message('Live controller input is unavailable in this window.'); return; }
    const nextSignature = pads.map(p => `${p.index}:${p.id}`).join('|');
    if (signature !== nextSignature) {
      const chosen = select.value; signature = nextSignature;
      select.replaceChildren(...pads.map(p => { const option = node('option', p.id); option.value = String(p.index); return option; }));
      if (pads.some(p => String(p.index) === chosen)) select.value = chosen;
    }
    const pad = pads.find(p => String(p.index) === select.value);
    select.disabled = !pad;
    if (!pad) {
      message(navigator.getGamepads ? 'No live input detected. Connect a controller and press a button to make it visible.' : 'Live controller input is not supported in this window.');
      silhouette.hidden = controls.hidden = axes.hidden = seen.hidden = true; lastDevice = ''; tested.clear(); return;
    }
    silhouette.hidden = controls.hidden = axes.hidden = seen.hidden = false;
    const device = `${pad.index}:${pad.id}`;
    if (lastDevice !== device) { tested.clear(); lastDevice = device; exitChord = null; }
    const chord = pad.mapping === 'standard' && pad.buttons[8]?.pressed && pad.buttons[9]?.pressed;
    if (exitChord === false && chord) { stop(); return; }
    exitChord = !!chord;
    message(pad.mapping === 'standard' ? 'Live input connected' : 'Live input connected. This device uses numbered inputs; labels vary by adapter.');
    const nextLayout = `${device}:${pad.mapping}:${pad.buttons.length}:${pad.axes.length}`;
    if (layout !== nextLayout) {
      layout = nextLayout;
      preview = window.createControllerPreview?.(pad.id);
      if (preview) silhouette.replaceChildren(preview);
      buttons = pad.buttons.map((_, i) => {
        const b = node('div', '', 'test-button'); b.dataset.button = String(i);
        b.append(node('span', pad.mapping === 'standard' ? names[i] || `Button ${i + 1}` : `Button ${i + 1}`), node('strong', '0%'));
        return b;
      });
      controls.replaceChildren(...buttons);
      axisViews = pad.axes.map((_, i) => {
        const row = node('label', '', 'test-axis');
        const axisName = pad.mapping === 'standard' ? ['Left stick X', 'Left stick Y', 'Right stick X', 'Right stick Y'][i] || `Axis ${i + 1}` : `Axis ${i + 1}`;
        const meter = node('meter'); meter.min = -1; meter.max = 1; meter.value = 0;
        const value = node('span', '0.00'); row.append(node('span', axisName), meter, value);
        return {row, meter, value};
      });
      axes.replaceChildren(...axisViews.map(a => a.row));
    }
    preview?.update(pad.mapping === 'standard' ? pad : null);
    pad.buttons.forEach((b, i) => {
      const amount = Math.max(0, Math.min(1, Number(b.value) || 0));
      buttons[i].classList.toggle('pressed', b.pressed || amount > .1);
      textIfChanged(buttons[i].lastElementChild, `${Math.round(amount * 100)}%`);
      if (b.pressed || amount > .5) tested.add(i);
    });
    pad.axes.forEach((v, i) => { const value = Number.isFinite(v) ? Math.max(-1, Math.min(1,v)) : 0; if (axisViews[i].meter.value !== value) axisViews[i].meter.value = value; textIfChanged(axisViews[i].value,value.toFixed(2)); });
    textIfChanged(seen,`${tested.size} of ${pad.buttons.length} reported buttons tested. Some system buttons may be reserved by Windows.`);
  }
  start.addEventListener('click', () => {
    if (active === tester) { stop(); return; }
    stop();
    if (sessionActive) return;
    active = tester; tested.clear(); body.hidden = false;
    start.textContent = 'Finish controller test'; start.setAttribute('aria-expanded','true');
    document.body.dataset.controllerTesting = 'true'; poll();
    document.dispatchEvent(new Event('controller-test-state'));
    host.scrollIntoView({block:'start', behavior:'instant'});
  });
}
function mountAll() {
  if (active && !document.querySelector('[data-controller-test] .test-body:not([hidden])')) stop();
  document.querySelectorAll('[data-controller-test]').forEach(mount);
  document.querySelectorAll('[data-test-start]').forEach(b => { b.disabled = sessionActive; });
}
new MutationObserver(mountAll).observe(document.getElementById('screen'), {childList:true, subtree:true});
document.addEventListener('keydown', e => { if (e.key === 'Escape' && active) { e.preventDefault(); e.stopImmediatePropagation(); stop(); } }, true);
document.addEventListener('session-state', e => { sessionActive = !!e.detail; if (sessionActive) stop(); mountAll(); });
window.__TAURI__?.core?.invoke('session_active').then(value => {
  sessionActive = !!value; if (sessionActive) stop(); mountAll();
}).catch(() => {});
window.addEventListener('pagehide', stop);
mountAll();
