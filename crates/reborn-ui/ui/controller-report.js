import {ControllerRecording,REPORT_STEPS} from './controller-report-data.js';
const node=(tag,text)=>{const n=document.createElement(tag);if(text)n.textContent=text;return n;};
const button=text=>{const n=node('button',text);n.type='button';return n;};
function field(label,options){const wrap=node('label',label),input=node('select');
 input.setAttribute('aria-label',label);for(const [value,text]of options){const o=node('option',text);o.value=value;input.append(o);}wrap.append(input);return {wrap,input};}
const delay=(ms,signal)=>new Promise((resolve,reject)=>{const finish=()=>{signal.removeEventListener('abort',abort);resolve();},timer=setTimeout(finish,ms);
 const abort=()=>{clearTimeout(timer);reject(new DOMException('Closed','AbortError'));};signal.addEventListener('abort',abort,{once:true});});

export function openControllerReport({runtime,device,onClose}){
 const invoke=(command,args={})=>window.__TAURI__.core.invoke(command,args);
 const token=crypto.randomUUID(),abort=new AbortController();let closed=false,busy=true,recording=null,index=0,interval=0;
 const dialog=node('dialog');dialog.className='update-dialog controller-report-dialog';dialog.setAttribute('aria-labelledby','controller-report-title');
 const title=node('h2','Record controller problem');title.id='controller-report-title';
 const intro=node('p','Use your mouse for this recording. Follow the prompts with the controller that has the problem. Your controls will stay unchanged.');
 const privacy=node('p','The report includes controller names, input samples, mappings, software details, and your description. It records no keyboard input or gameplay. Nothing is uploaded; you choose where to save the file.');privacy.className='controller-report-note';
 const form=node('div');form.className='controller-report-fields';
 const location=field('Where does it happen?',[['both','Launcher and game'],['launcher','Launcher'],['game','Inside the game']]);
 const connection=field('How is it connected?',[['unknown','Not sure'],['usb','USB cable'],['bluetooth','Bluetooth'],['adapter','USB or wireless adapter']]);
 const translation=field('Controller software',[['unknown','Not sure'],['none','None'],['steam_input','Steam Input'],['ds4windows','DS4Windows'],['other','Other']]);
 const runtimePad=field('Controller seen by the game',[['','Checking controllers…']]);runtimePad.input.disabled=true;
 const description=node('textarea');description.maxLength=2000;description.rows=3;description.setAttribute('aria-label','Describe the problem');description.placeholder='Example: Down skips an option on the home screen.';
 const label=node('label','Describe the problem');label.append(description);
 form.append(location.wrap,connection.wrap,translation.wrap,runtimePad.wrap,label);
 const status=node('p','Checking the controller connection…');status.setAttribute('role','status');status.setAttribute('aria-live','polite');
 const prompt=node('h3');prompt.hidden=true;
 const start=button('Begin recording'),record=button('Record input'),skip=button('Skip this control'),review=button('Review report'),save=button('Save report'),cancel=button('Cancel');
 record.hidden=skip.hidden=review.hidden=save.hidden=true;start.disabled=true;
 const actions=node('div');actions.className='update-dialog-actions';actions.append(start,record,skip,review,save,cancel);
 dialog.append(title,intro,privacy,form,prompt,status,actions);document.body.append(dialog);
 const stop=()=>invoke('controller_report_stop',{token}).catch(()=>{});
 const close=()=>{if(closed)return;closed=true;abort.abort();clearInterval(interval);stop();
  document.removeEventListener('keydown',escape,true);window.removeEventListener('pagehide',close);
  if(document.body.dataset.controllerTesting==='report')delete document.body.dataset.controllerTesting;
  document.dispatchEvent(new Event('controller-test-state'));dialog.close();dialog.remove();onClose();};
 const escape=e=>{if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();close();}};
 document.addEventListener('keydown',escape,true);window.addEventListener('pagehide',close,{once:true});
 dialog.addEventListener('cancel',e=>{e.preventDefault();close();});dialog.addEventListener('close',close);cancel.onclick=close;
 document.body.dataset.controllerTesting='report';document.dispatchEvent(new Event('controller-test-state'));
 title.tabIndex=-1;dialog.showModal();title.focus({preventScroll:true});dialog.scrollTop=0;
 invoke('controller_report_start',{token,runtime}).then(result=>{
  if(closed){stop();return;}
  runtimePad.input.replaceChildren();const empty=node('option','Not listed / record launcher input only');empty.value='';runtimePad.input.append(empty);
  for(const pad of result.devices||[]){const o=node('option',pad.device);o.value=pad.device;runtimePad.input.append(o);}
  runtimePad.input.value=(result.devices||[]).some(p=>p.device===device)?device:result.devices?.length===1?result.devices[0].device:'';
  runtimePad.input.disabled=false;start.disabled=false;busy=false;
  status.textContent=result.status==='ready'?'Ready. This takes about two minutes. Skip controls your controller does not have.':'The game input reader is unavailable. You can still record launcher input and save a report.';
 }).catch(()=>{if(closed){stop();return;}status.textContent='The recorder could not start. Close any running game, then close this window and try again.';});
 const showStep=()=>{
  if(index>=REPORT_STEPS.length){showReview();return;}
  prompt.hidden=false;prompt.textContent=`${index+1} of ${REPORT_STEPS.length}: ${REPORT_STEPS[index][1]}`;
  status.textContent='Release the controls, then click Record input. Wait for “Now” before pressing.';
  record.hidden=skip.hidden=false;review.hidden=recording.data.steps.length===0;record.focus();
 };
 const showReview=()=>{
  record.hidden=skip.hidden=review.hidden=true;save.hidden=false;prompt.hidden=false;prompt.textContent='Your report is ready';
  const recorded=recording.data.steps.filter(s=>s.status==='recorded').length;
  status.textContent=`${recorded} input prompts recorded. ${recording.data.devices.length} browser controller layout(s) seen. Save the JSON file and attach it when reporting the problem.`;
  cancel.textContent='Close';save.focus();
 };
 start.onclick=()=>{if(busy)return;recording=new ControllerRecording({location:location.input.value,connection:connection.input.value,
  translation:translation.input.value,description:description.value,runtime_device:runtimePad.input.value||null});form.hidden=true;start.hidden=true;showStep();};
 skip.onclick=()=>{if(busy)return;recording.step(REPORT_STEPS[index][0]);index++;showStep();};
 review.onclick=()=>{if(!busy)showReview();};
 record.onclick=async()=>{
  if(busy)return;busy=true;record.disabled=skip.disabled=review.disabled=true;
  const [name,instruction]=REPORT_STEPS[index];let step=null;
  try{
   status.textContent='Release all controls…';await delay(1500,abort.signal);if(closed)return;
   step=recording.step(name,'recorded');const began=performance.now();
   status.textContent=`Now: ${instruction.toLowerCase()}. Hold briefly, then release.`;
   const sample=()=>{if(closed||step.samples.length>=160)return;
    if(performance.now()-began>10000){step.status='interrupted';return;}
    try{step.samples.push(recording.sample(navigator.getGamepads?.()||[],performance.now()-began,!document.hidden&&document.hasFocus(),localStorage));}catch{step.status='interrupted';}};
   sample();interval=setInterval(sample,50);
   const detected=name==='rest'?Promise.resolve({status:'not_requested',binding:null}):recording.data.runtime_device?
    invoke('controller_report_detect',{token,device:recording.data.runtime_device}).catch(()=>({status:'error',binding:null})):
    Promise.resolve({status:'unavailable',binding:null});
   const timed=delay(5000,abort.signal).then(()=>{clearInterval(interval);sample();});
   const [result]=await Promise.all([detected,timed]);if(closed)return;
   step.runtime_status=result.status;step.binding=result.binding;index++;showStep();
  }catch{if(step)step.status='interrupted';}
  finally{clearInterval(interval);busy=false;record.disabled=skip.disabled=review.disabled=false;}
 };
 save.onclick=async()=>{
  if(busy)return;busy=true;save.disabled=true;status.textContent='Choose where to save your report…';
  try{const name=await invoke('controller_report_save',{token,recording:recording.data});if(closed)return;
   status.textContent=name?`Saved ${name}. Attach this file when reporting the problem.`:'Save canceled. Your report is still here; choose Save report to try again.';
  }catch{if(!closed)status.textContent='The report could not be saved. Choose Save report to try again.';}
  finally{busy=false;save.disabled=false;}
 };
 return {close};
}
