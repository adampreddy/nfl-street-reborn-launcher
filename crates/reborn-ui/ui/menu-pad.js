// Browser input and Dolphin input names are different APIs. Only the browser's
// standard mapping has fixed indices; raw devices use a measured menu layout.
export const layoutKey = pad => `reborn:menu-pad:1:${pad.id}:${pad.buttons.length}:${pad.axes.length}`;
export function readLayout(pad, storage = localStorage) {
  try { return JSON.parse(storage.getItem(layoutKey(pad))); } catch { return null; }
}
const down = b => !!b && (b.pressed || b.value > .55);
export function bindingValue(pad, b) {
  if (!b) return 0;
  if (b.kind === 'button') return Number(down(pad.buttons[b.index]));
  const value = pad.axes[b.index];
  if (!Number.isFinite(value)) return 0;
  // Hats use discrete axis values, including neutral values outside [-1,1].
  if (b.kind === 'hat') return Number(Math.abs(value - b.value) < .1);
  if (b.kind === 'axis') return Math.max(0, Math.min(1, (value-b.rest)/(b.value-b.rest)));
  return 0;
}
export function samplePad(pad, layout) {
  if (layout?.bindings) {
    const v = key => bindingValue(pad, layout.bindings[key]);
    return {buttons: [v('accept'),v('back'),0,0,v('previous'),v('next')].map(v=>v>.55),
      dpad: ['up','down','left','right'].map(k=>v(k)>.55),
      x: v('stickRight')-v('stickLeft'), y: v('stickDown')-v('stickUp'), scroll: 0};
  }
  if (pad.mapping !== 'standard') return null;
  return {buttons: pad.buttons.slice(0,6).map(down),
    dpad: [12,13,14,15].map(i=>down(pad.buttons[i])),
    x: pad.axes[0] || 0, y: pad.axes[1] || 0, scroll: pad.axes[3] || 0};
}

export function menuDirection(sample, previous) {
  const [up,down,left,right] = sample.dpad;
  // Opposing presses cancel. D-pad takes precedence over stick drift.
  if (sample.dpad.some(Boolean)) {
    if (up !== down) return up ? 'up' : 'down';
    if (left !== right) return left ? 'left' : 'right';
    return null;
  }
  const {x,y} = sample, ax = Math.abs(x), ay = Math.abs(y);
  const held = {left:-x,right:x,up:-y,down:y}[previous] || 0;
  // Avoid oscillating between directions when holding a diagonal. Switching
  // axis requires a deliberate larger deflection, not frame-to-frame noise.
  const other = previous === 'left' || previous === 'right' ? ay : ax;
  if (held > .38 && other < held + .2) return previous;
  if (Math.max(ax,ay) < .58) return null;
  return ax > ay ? x < 0 ? 'left' : 'right' : y < 0 ? 'up' : 'down';
}

// One event stream for the whole window, even with mirrored/virtual devices.
// Every device must be seen neutral before it may take control.
export class MenuPadReader {
  constructor() { this.reset(); }
  reset() { this.states = new Map(); this.owner = null; this.lastTime = null; }
  update(pads, now, layoutFor = () => null) {
    if (this.lastTime !== null && now-this.lastTime > 250) this.reset();
    const elapsed = Math.min(40, this.lastTime === null ? 0 : now-this.lastTime);
    this.lastTime = now;
    const live = new Set(), candidates = [];
    for (const pad of pads) {
      if (!pad?.connected) continue;
      const id = `${pad.index}:${pad.id}`, sample = samplePad(pad, layoutFor(pad));
      if (!sample) continue;
      live.add(id);
      const state = this.states.get(id) || {armed:false,buttons:[],direction:null,next:0};
      const neutral = !sample.buttons.some(Boolean) && !sample.dpad.some(Boolean)
        && Math.abs(sample.x)<.3 && Math.abs(sample.y)<.3 && Math.abs(sample.scroll)<.3;
      const direction = menuDirection(sample,state.direction);
      if (!state.armed) { state.armed = neutral; state.buttons = sample.buttons; }
      else candidates.push({id,pad,state,sample,direction,neutral});
      this.states.set(id,state);
    }
    for (const id of this.states.keys()) if (!live.has(id)) this.states.delete(id);
    let chosen = candidates.find(c=>c.id===this.owner && !c.neutral);
    // Do not hand off to a second controller already held while the first was
    // driving. Virtual copies can release a frame later than the physical pad.
    chosen ||= candidates.find(c=>!c.neutral && (c.direction!==c.state.direction
      || c.sample.buttons.some((v,i)=>v&&!c.state.buttons[i])
      || (Math.abs(c.sample.scroll)>.3 && !c.state.scrolling)));
    const result = {elapsed,move:null,accept:false,back:false,previous:false,next:false,scroll:0,pad:chosen?.pad};
    for (const c of candidates) {
      const {state,sample,direction} = c;
      if (c === chosen) {
        // A newly active pad gets one move, never one move per connected pad.
        if (this.owner !== c.id) { state.direction=null; state.next=0; }
        this.owner=c.id;
        if (direction && (direction!==state.direction || now>=state.next)) {
          result.move=direction; state.next=now+(direction!==state.direction ? 330 : 115);
        }
        for (const [key,i] of [['accept',0],['back',1],['previous',4],['next',5]])
          result[key]=!!sample.buttons[i] && !state.buttons[i];
        const amount=Math.max(0,(Math.abs(sample.scroll)-.22)/.78);
        result.scroll=Math.sign(sample.scroll)*amount*amount;
      }
      state.buttons=sample.buttons; state.direction=direction; state.scrolling=Math.abs(sample.scroll)>.3;
    }
    return result;
  }
}
