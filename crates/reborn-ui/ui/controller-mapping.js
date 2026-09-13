(() => {
 const invoke=(name,args)=>window.__TAURI__.core.invoke(name,args);
 const physical=['`Button A`','`Button B`','`Button X`','`Button Y`','`Shoulder L`','`Shoulder R`','`Trigger L`','`Trigger R`','Back','Start','`Thumb L`','`Thumb R`','`Pad N`','`Pad S`','`Pad W`','`Pad E`'];
 const axisNames=[['`Left X-`','`Left X+`'],['`Left Y+`','`Left Y-`'],['`Right X-`','`Right X+`'],['`Right Y+`','`Right Y-`']];
 window.mountControllerMapping=(device,runtime)=>{
  const details=document.createElement('details');details.className='setup-details mapping-editor';
  const summary=document.createElement('summary');summary.textContent='Customize controls';details.append(summary);
  const body=document.createElement('div');details.append(body);let loaded=false,stopListening=null,started=false;
  const message=document.createElement('p');message.setAttribute('role','status');message.className='note';
  // Closing the editor stops the runtime. One left running holds the pad open
  // and makes the next launch look broken in exactly the way this fixes.
  details.addEventListener('toggle',async()=>{
   if(!details.open&&started){started=false;stopListening?.();try{await invoke('remap_stop');}catch{}return;}
   if(!details.open||loaded)return;loaded=true;body.append(message);message.textContent='Reading your controller mapping…';
   try {
    const data=await invoke('setup_controller_mapping',{device,runtime});message.textContent='Automatic mapping is ready. Change any control below, then save.';
    const fields=[];
    for(const row of data.rows){
     const label=document.createElement('label');label.className='mapping-row';
     const name=document.createElement('span');name.textContent=row.label;
     const input=document.createElement('input');input.value=row.binding;input.setAttribute('aria-label',row.label+' binding');
     const capture=document.createElement('button');capture.type='button';capture.textContent='Assign';capture.setAttribute('aria-label','Assign '+row.label);
     // Detection happens in the RUNTIME, not in this window.
     //
     // It used to poll the browser's Gamepad API and refused outright for
     // anything that was not XInput, which left a PlayStation or a generic pad
     // with no way to remap at all. The browser also cannot say what Dolphin
     // calls a control, and a name from the wrong backend binds nothing without
     // reporting it. Asking the runtime is the only answer that cannot
     // disagree with the thing that later resolves the name.
     capture.addEventListener('click',async()=>{
      stopListening?.();
      let cancelled=false;
      const escape=e=>{if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();cancel();}};
      const cancel=()=>{cancelled=true;delete document.body.dataset.controllerTesting;capture.textContent='Assign';document.removeEventListener('keydown',escape,true);document.dispatchEvent(new Event('controller-test-state'));stopListening=null;};
      document.addEventListener('keydown',escape,true);stopListening=cancel;
      document.body.dataset.controllerTesting='true';document.dispatchEvent(new Event('controller-test-state'));
      capture.textContent='Listening…';
      message.textContent='Release everything, then press the control you want for '+row.label+'. Esc cancels.';
      try{
       if(!started){await invoke('remap_start',{runtime});started=true;}
       const answer=await invoke('remap_detect',{device});
       if(cancelled)return;
       if(answer&&answer.binding){
        input.value=answer.binding;
        message.textContent=row.label+' assigned. Save your mapping to apply it.';
       }else{
        // Not an error. A pad with no Z button has to be finishable.
        message.textContent='Nothing was pressed, so '+row.label+' was left as it was.';
       }
      }catch(e){ if(!cancelled) message.textContent=String(e.message||e); }
      finally{ cancel(); }
     });
     label.append(name,input,capture);body.append(label);fields.push([row.key,input]);
    }
    for(const reset of [false,true]){
     const button=document.createElement('button');button.type='button';button.textContent=reset?'Restore automatic mapping':'Save mapping';button.className=reset?'':'primary';
     button.addEventListener('click',async()=>{
      stopListening?.();button.disabled=true;
      try{await invoke('setup_save_mapping',{runtime,device,reset,bindings:Object.fromEntries(fields.map(([k,input])=>[k,input.value]))});message.textContent=reset?'Automatic mapping restored.':'Mapping saved. It will be applied when the game starts.';if(reset){loaded=false;body.replaceChildren();details.open=false;}}
      catch(e){message.textContent=String(e.message||e);}finally{button.disabled=false;}
     });body.append(button);
    }
   }catch(e){message.textContent=String(e.message||e);loaded=false;}
  });
  return details;
 };
})();
