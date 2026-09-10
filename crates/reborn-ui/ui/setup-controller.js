/* Shared onboarding and four-player controller mapping workspace. */
(() => {
 const buttonNames=['`Button A`','`Button B`','`Button X`','`Button Y`','`Shoulder L`','`Shoulder R`','`Trigger L`','`Trigger R`','Back','Start','`Thumb L`','`Thumb R`','`Pad N`','`Pad S`','`Pad W`','`Pad E`'];
 const axes=[['`Left X-`','`Left X+`'],['`Left Y+`','`Left Y-`'],['`Right X-`','`Right X+`'],['`Right Y+`','`Right Y-`']];
 const make=(tag,cls,text)=>{const e=document.createElement(tag);e.className=cls||'';if(text)e.textContent=text;return e;};
 window.createSetupController=({device,runtime,onBusy,layout="setup",singleDevice=true})=>{
  const root=make('section',layout==='players'?'setup-controller player-workspace':'setup-controller');let saving=false;let disposed=false,frame=0,mode='test',capture=null,chord=false,rows=[],group='Buttons',preview=null,previewId='',liveIndex=null;
  const invoke=(cmd,args={})=>window.__TAURI__.core.invoke(cmd,{device,runtime,...args});
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
  right.append(title,message,tabs,grid,editor,reset);
  const setMode=value=>{mode=value;if(value==='test')document.body.dataset.controllerTesting='setup';else delete document.body.dataset.controllerTesting;document.dispatchEvent(new Event('controller-test-state'));root.dataset.testing=String(value==='test');toggle.textContent=value==='test'?(layout==='players'?'Finish test':'Use controller to navigate'):'Test controller';toggle.setAttribute('aria-pressed',String(value==='test'));hint.textContent=layout==='players'?(value==='test'?'Navigation is paused. Click Finish test or press Esc.':'Select a control to change its binding.'):value==='test'?'Live test is on. Esc or View + Menu switches to navigation.':'Controller navigation is on. Use the D-pad or left stick to move, A / Cross to select, and B / Circle to go back.';};
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
   message.textContent=`Assign ${row.label}: release the controls, then press a button or move a stick.`;
   const label=make('label',null,`${row.label} binding`),input=make('input');input.value=row.binding;input.setAttribute('aria-label',`${row.label} binding`);label.append(input);
   const apply=make('button',null,'Use binding'),cancel=make('button',null,'Cancel');apply.type=cancel.type='button';apply.onclick=()=>save(row,input.value);cancel.onclick=cancelCapture;
   editor.setAttribute('role','dialog');editor.setAttribute('aria-label',`Assign ${row.label}`);editor.replaceChildren(label,apply,cancel);editor.hidden=false;renderRows();cancel.focus({preventScroll:true});
  }
  const belongs=row=>group==='D-pad'?/^D-Pad\//.test(row.key):group==='Sticks'?/^(Main Stick|C-Stick)\//.test(row.key):group==='Triggers'?/^Triggers\//.test(row.key):! /^(Main Stick|C-Stick|Triggers|D-Pad)\//.test(row.key);
  function renderRows(){
   const visible=group==='Sticks'?['Up','Down','Left','Right'].flatMap(d=>['Main Stick','C-Stick'].map(g=>rows.find(r=>r.key===g+'/'+d))).filter(Boolean):rows.filter(belongs);
   const button=(row,displayLabel=row.label)=>{const b=make('button','mapping-key');b.type='button';b.disabled=!!capture||saving;b.append(make('span',null,displayLabel),make('small',null,row.binding.replaceAll('`','')));b.setAttribute('aria-label',`Assign ${row.label}`);b.onclick=()=>assign(row);return b;};
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
  const load=async()=>{try{const result=await invoke('setup_controller_mapping');if(disposed)return;rows=result.rows.map(row=>({...row,label:row.key.startsWith('Main Stick/')?'Left stick '+row.key.split('/')[1].toLowerCase():row.key.startsWith('C-Stick/')?'Right stick '+row.key.split('/')[1].toLowerCase():row.label}));message.textContent=result.custom?'Your saved mapping is loaded. Select a control to change it.':'Standard controls mapped automatically. Select a control to change it.';reset.disabled=false;renderRows();}catch(e){message.textContent=String(e.message||e);}};
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
   if(layout!=='players'&&nowChord&&!chord&&!capture&&!saving){setMode('navigate');toggle.focus({preventScroll:true});}chord=nowChord;
   if(!capture)return;
   if(!/^(XInput|SDL)\//i.test(device)||pad.mapping!=='standard'){message.textContent='Enter the input name from Dolphin for this controller, then choose Use binding.';return;}
   if(pads.length!==1&&layout!=='players'){message.textContent='Connect only the controller being mapped before assigning an input.';return;}
   if(!capture.neutral){capture.neutral=pad.buttons.every(b=>!b.pressed&&b.value<.2)&&pad.axes.every(a=>Math.abs(a)<.3);return;}
   const b=pad.buttons.findIndex((b,i)=>i<buttonNames.length&&(b.pressed||b.value>.6)),a=pad.axes.findIndex((v,i)=>i<4&&Math.abs(v)>.65);
   if(b>=0||a>=0){const row=capture.row;capture=null;save(row,b>=0?buttonNames[b]:axes[a][pad.axes[a]>0?1:0]);}
  }
  setMode('test');load();frame=requestAnimationFrame(poll);
  root.dispose=()=>{onBusy(false);disposed=true;cancelAnimationFrame(frame);document.removeEventListener('keydown',escape,true);if(document.body.dataset.controllerTesting==='setup')delete document.body.dataset.controllerTesting;document.dispatchEvent(new Event('controller-test-state'));};
  return root;
 };
})();
