import {bridge,call,el,get,icon,reason,markCurrent,set,wireCoreFold} from './shell.js';
import {mountNotices} from './notices.js';
import {openControllerReport} from './controller-report.js';
let active=0,busy=false,workspace=null,state={pads:[],slots:[null,null,null,null]},problem='';
const screen=document.getElementById('screen');
delete document.body.dataset.controllerTesting;
document.addEventListener('keydown',event=>{if(event.key==='Escape'&&!workspace&&document.body.dataset.controllerTesting==='setup'){event.preventDefault();event.stopImmediatePropagation();delete document.body.dataset.controllerTesting;document.dispatchEvent(new Event('controller-test-state'));}},true);
const friendly=device=>{if(!device)return 'No controller';const parts=device.split('/');const name=parts.slice(2).join('/')||device;return `${name==='Gamepad'&&parts[0]==='XInput'?'Xbox controller':name} · ${Number(parts[1])+1||1}`;};
const connected=device=>state.pads.some(p=>p.device_string===device);
// The launcher's own controller icon (the Controllers nav item's two paths), on each player card (spec §8.5).
const PAD_ICON=['M7 7h10c3 0 5 11 2 11-2 0-3-3-4-3H9c-1 0-2 3-4 3C2 18 4 7 7 7Z','M8 9v5m-2-2h4m5-1h.01m3 2h.01'];
const WARN_ICON=['M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z','M12 8v5','M12 16h.01'];
const padIcon=()=>{const svg=icon(PAD_ICON);svg?.setAttribute('stroke-width','1.6');return svg;};
// "B maps to B" (spec §8.5): a small glass note with Dismiss, no longer a permanent banner. Dismissal is
// kept in this install's window storage, so it does not come back on the next visit.
function mappingNote(){
 if(get('bx-note-dismissed'))return null;
 const note=el('section',{class:'glass ctl-note',role:'note','aria-label':'B and X mapping'},icon(WARN_ICON),
  el('p',{},el('strong',{text:'B maps to B. X maps to X.'}),' Older installs may have these reversed. Check the bindings below.'),
  el('button',{type:'button',class:'btn sm quiet',text:'Dismiss',onclick:()=>{set('bx-note-dismissed','1');note.remove();const tab=document.getElementById(`player-tab-${active}`);(tab&&!tab.disabled?tab:screen.querySelector('button:not(:disabled),select:not(:disabled)'))?.focus({preventScroll:true});}}));
 return note;
}
let handed=null,fallback=null; // the control focus was moved off by lock(true), and where it went
function lock(value){
 const held=value&&screen.contains(document.activeElement)?document.activeElement:null;busy=value;
 screen.querySelectorAll('.ctl-note button,.player-tabs button,.player-device select,.player-device button,.player-textures select').forEach(n=>n.disabled=value);
 if(value){handed=fallback=null;if(held?.disabled){(screen.querySelector('button:not(:disabled),select:not(:disabled)')||document.querySelector('.shell-nav [aria-current],.shell-nav a'))?.focus({preventScroll:true});handed=held;fallback=document.activeElement;}}
 else{if(handed?.isConnected&&!handed.disabled&&document.activeElement===fallback)handed.focus({preventScroll:true});handed=fallback=null;}
}
async function refresh(){
 if(busy)return;lock(true);problem='';
 try{
  const check=await call('launch_check',{runtime:get('runtime'),game:get('game'),userDir:get('userDir')}).catch(()=>null);
  state.runtime=get('runtime')||check?.runtime||null;
  const [devices,assigned,current,options]=await Promise.allSettled([call('devices',{runtime:state.runtime}),call('controller_slots'),call('controls_current'),call('controls_options',{runtime:state.runtime})]);
  if(devices.status==='rejected')throw devices.reason;if(assigned.status==='rejected')throw assigned.reason;
  state.pads=devices.value?.pads||[];state.partial=devices.value?.complete===false;
  const slots=assigned.value?.slots;
  if(Array.isArray(slots))state.slots=slots.slice(0,4);else{const pads=[...state.pads].sort((a,b)=>Number(b.device_string===assigned.value?.primary)-Number(a.device_string===assigned.value?.primary));state.slots=Array.from({length:4},(_,i)=>pads[i]?.device_string||null);}
  state.current=current.status==='fulfilled'?current.value:null;state.options=options.status==='fulfilled'?options.value:[];
 }catch(e){problem=reason(e);}finally{busy=false;draw();}
}
function draw(){
 workspace?.dispose?.();workspace=null;delete document.body.dataset.controllerTesting;document.dispatchEvent(new Event('controller-test-state'));
 const tabs=el('div',{class:'player-tabs',role:'tablist','aria-label':'Players'});
 for(let i=0;i<4;i++){
  // the subline clamps to two lines in a narrow window (pages/controls.css): its tooltip holds the full text
  const device=state.slots[i],sub=device?(connected(device)?friendly(device):'Disconnected'):'No controller';const tab=el('button',{type:'button',role:'tab',class:'player-card'+(device?'':' is-empty'),id:`player-tab-${i}`,'aria-selected':String(i===active),'aria-controls':'player-panel',tabindex:'0',onclick:()=>{if(busy)return;active=i;draw();document.getElementById(`player-tab-${i}`).focus({preventScroll:true});}},
   el('span',{class:'player-icon'},padIcon()),
   el('span',{class:'player-text'},el('span',{class:'t',text:`Player ${i+1}`}),el('small',{class:device?(connected(device)?'connected':'disconnected'):'',text:sub,title:sub})));
  tab.addEventListener('keydown',event=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(event.key)||busy)return;event.preventDefault();active=event.key==='Home'?0:event.key==='End'?3:(i+(event.key==='ArrowRight'?1:3))%4;draw();document.getElementById(`player-tab-${active}`).focus({preventScroll:true});});tabs.append(tab);
 }
 const panel=el('section',{id:'player-panel',class:'player-panel',role:'tabpanel','aria-labelledby':`player-tab-${active}`});
 const select=el('select',{id:'player-device','aria-label':`Controller for Player ${active+1}`});select.append(el('option',{value:'',text:'No controller'}));
 for(const pad of state.pads){const other=state.slots.indexOf(pad.device_string);select.append(el('option',{value:pad.device_string,text:friendly(pad.device_string)+(other>=0&&other!==active?` (Player ${other+1})`: '')}));}
 const device=state.slots[active];if(device&&!connected(device))select.append(el('option',{value:device,text:friendly(device)+' (disconnected)'}));select.value=device||'';
 const feedback=el('p',{class:'player-feedback',role:'status',text:problem||'Assignments and mappings save automatically, including during games and NetPlay.'});
 select.addEventListener('change',async()=>{if(busy)return;lock(true);try{const result=await call('controller_assign_player',{player:active+1,device:select.value||null,runtime:state.runtime});state.slots=result.slots;problem='';}catch(e){problem=reason(e);}finally{busy=false;draw();}});
 const scan=el('button',{type:'button',id:'scan-pads',text:'Scan again',onclick:refresh});
 panel.append(el('div',{class:'player-device'},el('label',{for:'player-device',text:'Input device'}),select,scan));
 if(device&&connected(device)&&window.createSetupController){workspace=window.createSetupController({device,runtime:state.runtime,onBusy:lock,layout:'players',singleDevice:state.pads.length===1});panel.append(workspace);}
 else panel.append(el('div',{class:'player-empty'},el('strong',{text:device?'Controller disconnected':`Connect a controller for Player ${active+1}`}),el('p',{text:device?'Reconnect it and scan again. This player slot and its mapping are kept for you.':'Connect your controller, then choose it from the input list above.'})));
 const textures=el('select',{id:'player-textures','aria-label':'Button textures'});for(const option of state.options||[])textures.append(el('option',{value:option.slug,text:option.label}));textures.value=state.current?.profile||'gamecube';
 textures.addEventListener('change',async()=>{if(busy)return;lock(true);const previous=state.current?.profile;try{const result=await call('controls_set',{profile:textures.value,runtime:state.runtime});state.current={profile:textures.value};feedback.textContent=result.sync_note||'Button textures saved. They apply to all players.';}catch(e){textures.value=previous||'gamecube';feedback.textContent=reason(e);}finally{lock(false);}});
 panel.append(el('div',{class:'player-textures'},el('label',{for:'player-textures',text:'Button textures'}),textures,el('span',{text:'Shared by all four players.'})),feedback);
 const noticeHost=el('div',{class:'notices'},mappingNote());
 screen.replaceChildren(noticeHost,tabs,panel);
 const report=el('button',{type:'button',id:'record-controller-problem',text:'Record controller problem',disabled:!bridge,onclick:()=>{
  if(busy)return;workspace?.dispose?.();workspace=null;lock(true);
  openControllerReport({runtime:state.runtime||null,device:state.slots[active],onClose:()=>{busy=false;draw();document.getElementById('record-controller-problem')?.focus();}});
 }});
 screen.append(el('section',{class:'controller-report-entry'},el('h2',{text:'Controller not responding correctly?'}),el('p',{text:'Record a short guided input test and save a report to share with support.'}),report));
 if(bridge)mountNotices(noticeHost);
}
markCurrent();wireCoreFold();
if(bridge)refresh();else{problem='Open Controllers from the launcher to connect your devices.';draw();}
document.addEventListener('refresh-devices',()=>{if(!busy&&!document.body.dataset.controllerTesting)refresh();});
window.addEventListener('pagehide',()=>workspace?.dispose?.());
