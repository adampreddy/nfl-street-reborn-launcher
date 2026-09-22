import { layoutKey, readLayout } from './menu-pad.js';

// Measure unknown layouts instead of interpreting arbitrary USB axes as a
// standard pad. The rest sample also distinguishes a hat from an analog stick.
export function detectBinding(pad, rest, kind) {
  const button = pad.buttons.findIndex((b,i)=>(b.pressed || b.value>.65)
    && !(rest.buttons[i]?.pressed || rest.buttons[i]?.value>.3));
  if (button>=0) return {kind:'button',index:button};
  let index=-1, delta=.6;
  for (let i=0;i<pad.axes.length;i++) {
    const d=Math.abs(pad.axes[i]-rest.axes[i]);
    if (Number.isFinite(d) && d>delta) { index=i; delta=d; }
  }
  if (index<0) return null;
  return {kind:kind==='dpad'?'hat':'axis',index,rest:rest.axes[index],value:pad.axes[index]};
}

export function mountMenuPadSetup() {
  const main=document.getElementById('main');
  if (!main) return;
  const button=document.createElement('button');
  button.type='button'; button.id='menu-pad-setup'; button.textContent='Set up launcher controls';
  button.hidden=true; main.insertBefore(button,document.getElementById('screen'));
  let pads=[];
  const refresh=()=>{
    try { pads=[...(navigator.getGamepads?.()||[])].filter(p=>p?.connected); } catch { pads=[]; }
    button.hidden=!document.body.classList.contains('page-controls')
      && !pads.some(p=>p.mapping!=='standard' && !readLayout(p));
    button.disabled=!pads.length;
  };
  refresh(); const timer=setInterval(refresh,1000);
  window.addEventListener('pagehide',()=>clearInterval(timer),{once:true});
  button.addEventListener('click',()=>{
    refresh(); if (!pads.length) return;
    const dialog=document.createElement('dialog');dialog.className='update-dialog menu-pad-setup-dialog';
    const title=document.createElement('h2');title.textContent='Launcher controls';
    const text=document.createElement('p');text.textContent='Choose your controller, release its controls, then begin. These controls navigate the launcher. Game controls are set on the Controllers page.';
    text.setAttribute('role','status');
    const select=document.createElement('select');select.setAttribute('aria-label','Controller for launcher navigation');
    for(const pad of pads){const option=document.createElement('option');option.value=pad.index;option.textContent=pad.id;select.append(option);}
    const begin=document.createElement('button');begin.type='button';begin.textContent='Begin';
    const skip=document.createElement('button');skip.type='button';skip.textContent='Skip this control';skip.hidden=true;
    const cancel=document.createElement('button');cancel.type='button';cancel.textContent='Cancel';
    const reset=document.createElement('button');reset.type='button';reset.textContent='Use automatic launcher controls';
    const actions=document.createElement('div');actions.className='update-dialog-actions';actions.append(begin,skip,reset,cancel);
    dialog.append(title,text,select,actions);document.body.append(dialog);
    let finished=false,frame=0,step=0,rest=null,neutralSince=null,waiting=true,bindings={},chosen=null;
    const steps=[['up','D-pad up','dpad'],['down','D-pad down','dpad'],['left','D-pad left','dpad'],['right','D-pad right','dpad'],
      ['stickUp','left stick up','stick'],['stickDown','left stick down','stick'],['stickLeft','left stick left','stick'],['stickRight','left stick right','stick'],
      ['accept','the button you want to select with','button'],['back','the button you want to go back with','button'],
      ['previous','the left shoulder button','button'],['next','the right shoulder button','button']];
    const finish=()=>{if(finished)return;finished=true;chosen=null;cancelAnimationFrame(frame);delete document.body.dataset.controllerTesting;
      document.dispatchEvent(new Event('controller-test-state'));document.dispatchEvent(new Event('menu-pad-layout'));
      document.removeEventListener('keydown',escape,true);dialog.remove();button.focus();refresh();};
    const close=()=>{dialog.close();finish();};
    dialog.addEventListener('close',finish,{once:true});cancel.onclick=()=>close();
    // Capture before app-shell's Escape handler, which would close twice.
    const escape=e=>{if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();close();}};
    dialog.addEventListener('close',()=>document.removeEventListener('keydown',escape,true),{once:true});
    document.addEventListener('keydown',escape,true);
    reset.onclick=()=>{const pad=pads.find(p=>p.index===Number(select.value));if(!pad)return;
      try {localStorage.removeItem(layoutKey(pad));close();}catch{text.textContent='Could not save the change. Try again.';}};
    const show=()=>{text.textContent=`Step ${step+1} of ${steps.length}: release everything, then press ${steps[step][1]}. Skip controls your pad does not have.`;};
    const advance=()=>{
      step++;waiting=true;neutralSince=null;
      if(step===steps.length){
        skip.hidden=true;
        const allDirections=['up','down','left','right'].every((k,i)=>bindings[k]||bindings[['stickUp','stickDown','stickLeft','stickRight'][i]]);
        if(!allDirections||!bindings.accept||!bindings.back){text.textContent='Four directions, Select, and Back are required. Begin again to record the missing controls.';begin.hidden=false;skip.hidden=true;chosen=null;return;}
        try{localStorage.setItem(layoutKey(chosen),JSON.stringify({bindings}));close();}
        catch{text.textContent='Could not save these controls. Cancel and try again.';chosen=null;}return;
      }show();
    };
    skip.onclick=advance;
    begin.onclick=()=>{
      chosen=pads.find(p=>p.index===Number(select.value));if(!chosen)return;
      step=0;bindings={};waiting=true;rest=null;neutralSince=null;
      begin.hidden=true;reset.hidden=true;select.disabled=true;skip.hidden=false;
      document.body.dataset.controllerTesting='menu-setup';document.dispatchEvent(new Event('controller-test-state'));show();
      cancelAnimationFrame(frame);
      const poll=now=>{
        if(!dialog.open || !chosen || step>=steps.length)return;frame=requestAnimationFrame(poll);
        const pad=[...(navigator.getGamepads?.()||[])].find(p=>p?.connected&&p.index===chosen.index&&p.id===chosen.id);
        if(!pad){text.textContent='Controller disconnected. Reconnect it to continue.';rest=null;waiting=true;neutralSince=null;return;}
        if(document.hidden||!document.hasFocus()){neutralSince=null;waiting=true;return;}
        if(!rest){rest={axes:[...pad.axes],buttons:pad.buttons.map(b=>({pressed:b.pressed,value:b.value}))};neutralSince=now;return;}
        const neutral=pad.buttons.every(b=>!b.pressed&&!(b.value>.3))&&pad.axes.every((v,i)=>Math.abs(v-rest.axes[i])<.2);
        if(waiting){if(!neutral){neutralSince=null;return;}neutralSince??=now;if(now-neutralSince>=180)waiting=false;return;}
        const binding=detectBinding(pad,rest,steps[step][2]);
        if(binding){
          const action=steps[step][0],direction=key=>key.replace(/^stick/, '').toLowerCase();
          const duplicate=Object.entries(bindings).some(([key,b])=>direction(key)!==direction(action)
            && b.index===binding.index && (b.kind==='button')===(binding.kind==='button')
            && (b.kind==='button'||Math.abs(b.value-binding.value)<.1));
          if(duplicate){text.textContent='That control already has a different action. Release it, then choose another control, or skip this step.';waiting=true;neutralSince=null;return;}
          bindings[action]=binding;advance();
        }
      };frame=requestAnimationFrame(poll);
    };
    dialog.showModal();begin.focus();
  });
}
