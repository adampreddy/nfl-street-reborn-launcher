/* Shared onboarding and four-player controller mapping workspace. */
(() => {
 const make=(tag,cls,text)=>{const e=document.createElement(tag);e.className=cls||'';if(text)e.textContent=text;return e;};
 window.createSetupController=({device,runtime,onBusy,layout="setup",singleDevice=true})=>{
  const root=make('section',layout==='players'?'setup-controller player-workspace':'setup-controller');let saving=false;let disposed=false,frame=0,mode='test',capture=null,chord=false,rows=[],group='Buttons',preview=null,previewId='',liveIndex=null;
  const invoke=(cmd,args={})=>window.__TAURI__.core.invoke(cmd,{device,runtime,...args});
  // A press is read by the RUNTIME, never by this window.
  //
  // This workspace used to read the browser's Gamepad API and save Xbox names
  // (`Button A`, `Left Y+`), and for every device that was not XInput or SDL it
  // gave up and asked for "the input name from Dolphin", which a player never
  // sees. Every PlayStation pad and every generic pad is a WGInput device, so
  // none of them could be set up by pressing a button, from the page built for
  // exactly that. The browser cannot say what Dolphin calls a control, and a
  // name from the wrong backend binds nothing and reports nothing. The runtime
  // can: it answers with an expression it built itself, on the device it will
  // later resolve it on, so it works for any pad Windows can see.
  let listening=false;
  const listen=async()=>{
   if(!runtime)throw new Error('The game files were not found, so a press cannot be read here yet.');
   if(!listening){await window.__TAURI__.core.invoke('remap_start',{runtime});listening=true;}
   const answer=await window.__TAURI__.core.invoke('remap_detect',{device});
   return answer&&answer.binding?answer.binding:null;
  };
  // The listener holds the pad open. It is never left running behind the page.
  const stopListening=async()=>{if(!listening)return;listening=false;try{await window.__TAURI__.core.invoke('remap_stop');}catch{}};
  const left=make('div','controller-stage'),right=make('div','controller-controls');root.append(left,right);
  const art=make('div','controller-art'),live=make('p','controller-live','Press any button to wake your controller.');live.setAttribute('role','status');
  const select=make('select','live-device');select.setAttribute('aria-label','Controller to test');select.hidden=true;select.onchange=()=>{liveIndex=select.value===''?null:Number(select.value);if(capture)capture.neutral=false;};
  const toggle=make('button','test-toggle','Use controller to navigate');toggle.type='button';
  const hint=make('p','note','Live test is on. Press buttons and move the sticks.');
  left.append(art,live,select,toggle,hint);
  const title=make('h2',null,'Your controls'),message=make('p','mapping-status','Loading your mapping…');message.setAttribute('role','status');
  const tabs=make('div','mapping-tabs'),grid=make('div','compact-mappings');tabs.setAttribute('aria-label','Control groups');
  const editor=make('div','binding-editor');editor.hidden=true;
  const reset=make('button','mapping-reset','Restore automatic mapping');reset.type='button';reset.disabled=true;
  const swap=make('button','mapping-swap','Correct reversed B / X');swap.type='button';swap.hidden=true;
  // Shown only when the runtime says some controls cannot be pressed on this
  // pad. Every controller failure this launcher has shipped was silent; this
  // is where it stops being silent.
  const needs=make('section','controller-needs-setup');needs.setAttribute('role','alert');needs.hidden=true;
  const needsText=make('p'),needsStart=make('button','primary','Set up this controller');needsStart.type='button';needs.append(make('strong',null,'This controller needs a one-time setup'),needsText,needsStart);
  let guiding=false;
  const mappingNote=make('p','mapping-guidance','B maps to B. X maps to X. Older reversed mappings need to be corrected.');
  // First run: the standard layout is right for nearly everyone, so the
  // grid of bindings hides behind one line until someone asks for it.
  // Adam, 2026-09-11: "The initial controller remapping sucks and is a PIA."
  const custom=make('details','customize-controls');const customSummary=make('summary',null,'Customize controls');customSummary.title='The standard layout fits most controllers: A, B, X and Y as printed on your pad. Change any button here, or later under Controllers.';
  const buttons=make('div','mapping-buttons');buttons.append(swap,reset);
  if(layout==='players'){right.append(title,needs,message,tabs,grid,editor,buttons);}
  else{custom.append(customSummary,tabs,grid,editor,buttons);right.append(title,needs,message,mappingNote,custom);}
  const setMode=value=>{mode=value;if(value==='test')document.body.dataset.controllerTesting='setup';else delete document.body.dataset.controllerTesting;document.dispatchEvent(new Event('controller-test-state'));root.dataset.testing=String(value==='test');toggle.textContent=value==='test'?(layout==='players'?'Finish test':'Use controller to navigate'):'Test controller';toggle.setAttribute('aria-pressed',String(value==='test'));hint.textContent=layout==='players'?(value==='test'?'Navigation is paused. Finish test with Esc or View + Menu.':'Select a control to change its binding.'):value==='test'?'Live test is on. Esc or View + Menu switches to navigation.':'Controller navigation is on. Use the D-pad or left stick to move, A / Cross to select, and B / Circle to go back.';};
  toggle.onclick=()=>{if(saving)return;cancelCapture();setMode(mode==='test'?'navigate':'test');};
  const finishCapture=()=>{capture=null;editor.hidden=true;onBusy(false);setMode(mode);renderRows();};
  const cancelCapture=()=>{if(!capture)return;finishCapture();message.textContent='Assignment cancelled. Your mapping is unchanged.';};
  const save=async(row,binding)=>{
   if(!binding.trim()||saving)return;saving=true;onBusy(true);editor.querySelectorAll('button,input').forEach(n=>n.disabled=true);
   try{await invoke('setup_save_mapping',{bindings:{[row.key]:binding.trim()},reset:false});saving=false;row.binding=binding.trim();message.textContent=`${row.label} saved.`;finishCapture();}
   catch(e){saving=false;message.textContent=String(e.message||e);capture=null;onBusy(false);editor.hidden=true;setMode(mode);renderRows();}
  };
  function assign(row){
   if(saving)return;cancelCapture();capture={row,neutral:false};document.body.dataset.controllerTesting='setup';document.dispatchEvent(new Event('controller-test-state'));onBusy(true);
   message.textContent=`${row.label}: press the button on your controller now, or move the stick. Esc cancels.`;
   const label=make('label',null,`${row.label} binding`),input=make('input');input.value=row.binding;input.setAttribute('aria-label',`${row.label} binding`);label.append(input);
   const apply=make('button',null,'Use binding'),cancel=make('button',null,'Cancel');apply.type=cancel.type='button';apply.onclick=()=>save(row,input.value);cancel.onclick=cancelCapture;
   // Pressing the button is the way; typing a Dolphin input name is the
   // fallback for a pad the capture does not understand, kept out of the way.
   const typed=make('details','typed-binding');typed.append(make('summary',null,'Type a Dolphin input name instead'),label,apply);
   editor.setAttribute('role','dialog');editor.setAttribute('aria-label',`Assign ${row.label}`);editor.replaceChildren(make('p','note',`Press the button you want for ${row.label}.`),typed,cancel);editor.hidden=false;custom.open=true;renderRows();cancel.focus({preventScroll:true});
   // `capture` is compared by identity: a cancel, or a second assignment,
   // replaces it, and an answer that arrives late is then nobody's.
   const mine=capture;
   (async()=>{
    try{
     const binding=await listen();
     if(capture!==mine){if(!capture&&!guiding)await stopListening();return;}
     await stopListening();
     if(binding)save(row,binding);
     else{finishCapture();message.textContent=`Nothing was pressed, so ${row.label} was left as it was.`;}
    }catch(e){
     await stopListening();
     if(capture!==mine)return;
     typed.open=true;message.textContent=String(e.message||e)+' You can type the input name below instead.';
    }
   })();
  }
  const belongs=row=>group==='D-pad'?/^D-Pad\//.test(row.key):group==='Sticks'?/^(Main Stick|C-Stick)\//.test(row.key):group==='Triggers'?/^Triggers\//.test(row.key):! /^(Main Stick|C-Stick|Triggers|D-Pad)\//.test(row.key);
  function renderRows(){
   const b=rows.find(r=>r.key==='Buttons/B'),x=rows.find(r=>r.key==='Buttons/X');
   swap.hidden=!(b?.binding==='`Button X`'&&x?.binding==='`Button B`');
   const visible=group==='Sticks'?['Up','Down','Left','Right'].flatMap(d=>['Main Stick','C-Stick'].map(g=>rows.find(r=>r.key===g+'/'+d))).filter(Boolean):rows.filter(belongs);
   const button=(row,displayLabel=row.label)=>{const b=make('button','mapping-key');b.type='button';b.disabled=!!capture||saving;b.append(make('span',null,displayLabel),make('small',null,row.dead?'Not set up':row.binding.replaceAll('`','')));if(row.dead)b.dataset.dead='true';b.setAttribute('aria-label',`Assign ${row.label}`);b.onclick=()=>assign(row);return b;};
   if(layout==='players'){
    grid.replaceChildren(...[['Main Stick','Left stick','left-stick'],['D-Pad','D-pad','direction-pad'],['Buttons','Buttons','face-buttons'],['C-Stick','Right stick','right-stick'],['Triggers','Triggers','shoulders']].map(([prefix,label,area])=>{
     const section=make('section','mapping-group '+area);section.append(make('h3',null,label));const items=make('div','group-bindings');
     for(const row of rows.filter(r=>r.key.startsWith(prefix+'/')))items.append(button(row,prefix==='Main Stick'||prefix==='C-Stick'||prefix==='D-Pad'?row.key.split('/')[1]:row.label));
     section.append(items);return section;
    }));
   }else grid.replaceChildren(...visible.map(row=>button(row)));
   tabs.querySelectorAll('button').forEach(b=>b.setAttribute('aria-pressed',String(b.textContent===group)));
  }

  for(const name of ['Buttons','Sticks','Triggers','D-pad']){const b=make('button',null,name);b.type='button';b.onclick=()=>{if(saving)return;cancelCapture();group=name;renderRows();};tabs.append(b);}
  const load=async()=>{try{const result=await invoke('setup_controller_mapping');if(disposed)return;rows=result.rows.map(row=>({...row,label:row.key.startsWith('Main Stick/')?'Left stick '+row.key.split('/')[1].toLowerCase():row.key.startsWith('C-Stick/')?'Right stick '+row.key.split('/')[1].toLowerCase():row.label}));const dead=rows.filter(r=>r.dead);needs.hidden=dead.length===0;
   if(dead.length){needsText.textContent=`Reborn does not recognise the names this controller gives its buttons, so ${dead.length} of its ${rows.length} controls cannot be set up automatically. Press each one once and it will work from then on.`;message.textContent=`${dead.length} control${dead.length===1?' is':'s are'} not set up yet and will do nothing in the game.`;}
   else message.textContent=result.custom?'Your saved mapping is loaded. Select a control to change it.':result.checked?'Every control is mapped and checked against your controller. Select one to change it.':'Standard controls mapped automatically. Select a control to change it.';
   reset.disabled=false;renderRows();}catch(e){message.textContent=String(e.message||e);}};
  // One pass over exactly the controls that cannot be pressed, in order, saved
  // together at the end. Waiting without pressing skips a control: a pad with
  // no Z button has to be finishable.
  needsStart.onclick=async()=>{
   if(saving||capture||guiding)return;guiding=true;onBusy(true);needsStart.disabled=true;const list=rows.filter(r=>r.dead),found={};
   document.body.dataset.controllerTesting='setup';document.dispatchEvent(new Event('controller-test-state'));
   try{
    for(const [i,row] of list.entries()){
     if(disposed)break;message.textContent=`Step ${i+1} of ${list.length}: press ${row.label} on your controller. Wait a few seconds to skip it.`;
     const binding=await listen();if(binding){found[row.key]=binding;row.binding=binding;renderRows();}
    }
    await stopListening();
    if(Object.keys(found).length){saving=true;await invoke('setup_save_mapping',{bindings:found,reset:false});saving=false;}
    await load();if(!rows.some(r=>r.dead))message.textContent='Your controller is set up. It will work when the game starts.';
   }catch(e){saving=false;await stopListening();message.textContent=String(e.message||e);}
   finally{guiding=false;onBusy(false);needsStart.disabled=false;setMode(mode);}
  };
  swap.onclick=async()=>{
   if(saving)return;cancelCapture();const b=rows.find(r=>r.key==='Buttons/B'),x=rows.find(r=>r.key==='Buttons/X');if(!b||!x)return;
   if(b.binding!=='`Button X`'||x.binding!=='`Button B`')return;
   saving=true;onBusy(true);swap.disabled=true;
   try{await invoke('setup_save_mapping',{bindings:{'Buttons/B':'`Button B`','Buttons/X':'`Button X`'},reset:false});await load();message.textContent='Corrected. B maps to B and X maps to X.';}
   catch(e){message.textContent=String(e.message||e);}
   finally{saving=false;onBusy(false);swap.disabled=false;renderRows();}
  };
  reset.onclick=async()=>{if(saving)return;cancelCapture();saving=true;onBusy(true);reset.disabled=true;try{await invoke('setup_save_mapping',{bindings:{},reset:true});await load();}catch(e){message.textContent=String(e.message||e);}finally{saving=false;onBusy(false);reset.disabled=false;renderRows();}};
  function showArt(id){if(id===previewId)return;previewId=id;preview=window.createControllerPreview?.(id);if(preview)art.replaceChildren(preview);}
  showArt(device);
  const escape=e=>{if(e.key!=='Escape')return;if(saving){e.preventDefault();e.stopImmediatePropagation();return;}if(capture||mode==='test'){e.preventDefault();e.stopImmediatePropagation();if(capture)cancelCapture();else{setMode('navigate');toggle.focus({preventScroll:true});}}};
  document.addEventListener('keydown',escape,true);
  let inventory='';
  function poll(){
   if(disposed)return;frame=requestAnimationFrame(poll);if(!root.isConnected||!document.hasFocus()){if(capture)capture.neutral=false;return;}
   const pads=[...(navigator.getGamepads?.()||[])].filter(p=>p?.connected);
   const signature=pads.map(p=>`${p.index}:${p.id}`).join('|');
   if(signature!==inventory){inventory=signature;select.replaceChildren(...pads.map(p=>{const o=make('option',null,p.id);o.value=p.index;if(layout==='players')o.textContent=`Input ${p.index+1} · ${p.id}`;return o;}));select.hidden=pads.length<2&&(layout!=='players'||singleDevice);if(!pads.some(p=>p.index===liveIndex))liveIndex=pads.length===1&&(layout!=='players'||singleDevice)?pads[0].index:null;if(liveIndex===null&&pads.length>0){const o=make('option',null,'Choose a controller to test');o.value='';select.prepend(o);select.value='';}else select.value=liveIndex;}
   const pad=pads.find(p=>p.index===liveIndex);
   if(!pad){if(capture)capture.neutral=false;live.textContent=pads.length>1?'Choose the controller you want to test.':'Press any button to wake your controller.';preview?.update(null);return;}
   showArt(pad.id);preview?.update(pad);
   const active=pad.buttons.map((b,i)=>b.pressed?i:null).filter(i=>i!==null);
   const activity=active.length?'Button input received':pad.axes.some(a=>Math.abs(a)>.2)?'Stick input received':'Connected · live input ready';if(live.textContent!==activity)live.textContent=activity;
   const nowChord=pad.buttons[8]?.pressed&&pad.buttons[9]?.pressed;
   if(nowChord&&!chord&&!capture&&!saving){setMode('navigate');toggle.focus({preventScroll:true});}chord=nowChord;
   // Nothing here reads a press for an assignment any more: see `listen`.
   // This loop only draws the live preview.
  }
  setMode(layout==='players'||document.body.classList.contains('gamepad-input')?'navigate':'test');load();frame=requestAnimationFrame(poll);
  window.addEventListener('pagehide',stopListening);
  root.dispose=()=>{stopListening();window.removeEventListener('pagehide',stopListening);onBusy(false);disposed=true;cancelAnimationFrame(frame);document.removeEventListener('keydown',escape,true);if(document.body.dataset.controllerTesting==='setup')delete document.body.dataset.controllerTesting;document.dispatchEvent(new Event('controller-test-state'));};
  return root;
 };
})();
