(() => {
 const invoke=(name,args)=>window.__TAURI__.core.invoke(name,args);
 const physical=['`Button A`','`Button B`','`Button X`','`Button Y`','`Shoulder L`','`Shoulder R`','`Trigger L`','`Trigger R`','Back','Start','`Thumb L`','`Thumb R`','`Pad N`','`Pad S`','`Pad W`','`Pad E`'];
 const axisNames=[['`Left X-`','`Left X+`'],['`Left Y+`','`Left Y-`'],['`Right X-`','`Right X+`'],['`Right Y+`','`Right Y-`']];
 window.mountControllerMapping=(device,runtime)=>{
  const details=document.createElement('details');details.className='setup-details mapping-editor';
  const summary=document.createElement('summary');summary.textContent='Customize controls';details.append(summary);
  const body=document.createElement('div');details.append(body);let loaded=false,stopListening=null;
  const message=document.createElement('p');message.setAttribute('role','status');message.className='note';
  details.addEventListener('toggle',async()=>{
   if(!details.open||loaded)return;loaded=true;body.append(message);message.textContent='Reading your controller mapping…';
   try {
    const data=await invoke('setup_controller_mapping',{device,runtime});message.textContent='Automatic mapping is ready. Change any control below, then save.';
    const fields=[];
    for(const row of data.rows){
     const label=document.createElement('label');label.className='mapping-row';
     const name=document.createElement('span');name.textContent=row.label;
     const input=document.createElement('input');input.value=row.binding;input.setAttribute('aria-label',row.label+' binding');
     const capture=document.createElement('button');capture.type='button';capture.textContent='Assign';capture.setAttribute('aria-label','Assign '+row.label);
     capture.addEventListener('click',()=>{
      stopListening?.();let frame=0,neutral=false;
      document.body.dataset.controllerTesting='true';document.dispatchEvent(new Event('controller-test-state'));
      capture.textContent='Listening…';message.textContent='Release the controls, then press a button or move a stick. Esc cancels.';
      const cancel=()=>{cancelAnimationFrame(frame);delete document.body.dataset.controllerTesting;capture.textContent='Assign';document.removeEventListener('keydown',escape,true);document.dispatchEvent(new Event('controller-test-state'));stopListening=null;};
      const escape=e=>{if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();cancel();}};document.addEventListener('keydown',escape,true);stopListening=cancel;
      const poll=()=>{
       if(!details.isConnected||!details.open){cancel();return;}
       frame=requestAnimationFrame(poll);
       if(!document.hasFocus()){neutral=false;return;}
       if(!device.startsWith('XInput/')){message.textContent='Enter the binding shown by Dolphin for this device. Button capture currently supports XInput controllers.';cancel();return;}
       const pads=[...(navigator.getGamepads?.()||[])].filter(p=>p?.connected&&p.mapping==='standard');
       if(pads.length!==1){message.textContent='For button capture, connect one standard controller. You can also enter a Dolphin binding directly.';return;}
       const pad=pads[0];
       if(!neutral){neutral=pad.buttons.every(b=>!b.pressed)&&pad.axes.every(a=>Math.abs(a)<.3);return;}
       const b=pad.buttons.findIndex((b,i)=>i<physical.length&&(b.pressed||b.value>.6));
       const a=pad.axes.findIndex((v,i)=>i<4&&Math.abs(v)>.65);
       if(b>=0||a>=0){input.value=b>=0?physical[b]:axisNames[a][pad.axes[a]>0?1:0];cancel();message.textContent=row.label+' assigned. Save your mapping to apply it.';}
      };poll();
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
