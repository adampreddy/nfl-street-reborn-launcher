// Shared by every launcher page. Downloads live in Rust and survive navigation.
const invoke=(name,args={})=>window.__TAURI__.core.invoke(name,args);
const CHECK_MS=2*60*1000,STALE_MS=60*1000,LATER_MS=15*60*1000;
let running=false,checking=false,started=false,launcher=JSON.parse(sessionStorage.getItem("updates:latest")||"null"),content=null,activity={},policy={},dialog=null;
const jobs=new Set(),cards=new Map();
const saved=(key)=>{try{return JSON.parse(sessionStorage.getItem(key)||'null');}catch(_){return null;}};
const put=(key,value)=>sessionStorage.setItem(key,JSON.stringify(value));
const node=(tag,cls,text)=>{const n=document.createElement(tag);if(cls)n.className=cls;if(text)n.textContent=text;return n;};
const updateKey=(kind,version)=>'update-choice:'+kind+':'+version;
const version=kind=>kind==='launcher'?launcher?.latest:content?.release;
const deferred=kind=>(saved(updateKey(kind,version(kind)))?.until||0)>Date.now();
const accepted=kind=>saved(updateKey(kind,version(kind)))?.accepted===true;
const host=()=>{let h=document.getElementById('update-center');if(!h){h=node('aside','update-center');h.id='update-center';h.setAttribute('aria-label','Updates');document.body.append(h);}return h;};
const downloadPhase=()=>activity.downloading||content?.phase==='fetching';
const size=bytes=>Math.max(1,Math.round((bytes||0)/1048576))+' MB';

function later(kind){put(updateKey(kind,version(kind)),{until:Date.now()+LATER_MS});}
function dismissDialog(){if(dialog){dialog.close();dialog.remove();dialog=null;}}
function showDialog(kind){
 if(dialog||document.querySelector('dialog[open]'))return;
 const name=kind==='launcher'?'Launcher update':'Game content update';
 const current=kind==='launcher'?launcher:content;
 const d=node('dialog','update-dialog');dialog=d;d.setAttribute('aria-labelledby','update-dialog-title');
 const heading=node('h2','',name);heading.id='update-dialog-title';
 const intro=node('p','update-version',kind==='launcher'?'Reborn '+current.latest:'Release '+(current.release||'update'));
 const changes=node('div','update-changes');
 const notes=kind==='launcher'?current.notes:current.notes||current.components?.filter(c=>['available','repair'].includes(c.state)||c.repair).map(c=>c.component+(c.offered?' '+c.offered:'')+(c.message?': '+c.message:'')).join('\n');
 const lines=String(notes||current.note||'A verified update is available.').split(/\r?\n/).map(t=>t.replace(/^\s*[-*#]+\s*/, '').trim()).filter(Boolean);
 // A line ending in a colon ("Roster update:") heads the lines under it.
 let list=null;for(const line of lines){if(/:$/.test(line)){changes.append(node('p','update-changes-heading',line));list=null;continue;}if(!list){list=node('ul');changes.append(list);}list.append(node('li','',line));}
 const countdown=node('p','update-countdown');countdown.setAttribute('role','status');
 const actions=node('div','update-dialog-actions'),skip=node('button','','Remind me later'),go=node('button','primary','Update now');
 skip.type=go.type='button';
 skip.onclick=()=>{later(kind);dismissDialog();render();};
 go.onclick=()=>{put(updateKey(kind,version(kind)),{accepted:true});dismissDialog();tick();};
 d.addEventListener('cancel',e=>{e.preventDefault();skip.click();});
 actions.append(skip,go);d.append(heading,intro,changes,countdown,actions);document.body.append(d);d.showModal();skip.focus();
 let seconds=15;
 if(policy.automatic) {
  countdown.textContent='Starts automatically in '+seconds+' seconds.';
  const timer=setInterval(()=>{
   if(dialog!==d){clearInterval(timer);return;}
   if(!policy.automatic){clearInterval(timer);countdown.textContent='Nothing downloads until you choose Update now.';return;}
   // A background or unattended page must not eat the opportunity to postpone.
   if(document.hidden||!document.hasFocus())return;
   seconds--;countdown.textContent='Starts automatically in '+seconds+' seconds.';
   if(seconds<=0){clearInterval(timer);go.click();}
  },1000);
 } else countdown.textContent='Nothing downloads until you choose Update now.';
}

function card(kind,title,message,progress,buttonLabel,action){
 let c=cards.get(kind);
 if(!c){c=node('section','update-card');c.setAttribute('aria-label',title);c.append(node('strong','notice-headline'),node('p','update-message'),node('progress','notice-bar'),node('button'));c.lastChild.type='button';cards.set(kind,c);host().append(c);}
 c.children[0].textContent=title;c.children[1].textContent=message;
 const bar=c.children[2];bar.hidden=progress===undefined;bar.max=100;if(progress===null)bar.removeAttribute('value');else bar.value=progress||0;
 const button=c.children[3];button.hidden=!buttonLabel;button.textContent=buttonLabel||'';button.onclick=action;c.classList.toggle('downloading',progress!==undefined);
}
function removeCard(kind){cards.get(kind)?.remove();cards.delete(kind);}
function render(){
 if(launcher?.state==='available'||activity.downloading||activity.ready?.path){
  const pending=activity.ready?.path;
  card('launcher',activity.downloading?'Downloading launcher':pending?'Launcher update ready':'Launcher update available',
   activity.downloading?'Your download continues while you use the launcher.':pending?'Installs when your game and other changes are finished.':'Reborn '+launcher?.latest+(deferred('launcher')?' · Reminder postponed':''),
   activity.downloading?null:undefined,activity.downloading?null:pending?'View updates':'View update',
   ()=>pending?location.assign('settings.html#updates'):showDialog('launcher'));
 }else removeCard('launcher');
 if(content&&['available','fetching','staged','installing','installed','problem'].includes(content.phase)){
  const phase=content.phase;
  let progress;
  if(phase==='fetching')progress=content.download_bytes?Math.min(99,Math.floor((content.fetched_bytes||0)*100/content.download_bytes)):null;
  if(phase==='installing')progress=null;
  const title={available:'Game update available',fetching:'Downloading game content',staged:'Game update ready',installing:'Installing game content',installed:'Game updated',problem:'Update needs attention'}[phase];
  const message=phase==='fetching'?size(content.fetched_bytes)+' of '+size(content.download_bytes)+(progress===null?'':' · '+progress+'%'):
   phase==='staged'?'Installs when your game and other changes are finished.':
   phase==='installed'&&content.notes?content.notes+'\n\n'+(content.installed?.note||''):content.installed?.note||content.note||'New Reborn content is available.';
  card('content',title,message,progress,['fetching','installing'].includes(phase)?null:phase==='installed'?'Got it':'View update',
   ()=>{if(phase==='installed'){notesSeen(content.release);invoke('content_dismiss').then(()=>{content=null;removeCard('content');});}else if(phase==='available')showDialog('content');else location.assign('settings.html#updates');});
 }else removeCard('content');
 // What's new: a release's notes once, even when its content arrived before
 // this launcher could show them (an older launcher installed it, or the card
 // was missed). Never over an update in progress, which shows its own notes.
 if(content?.notes&&content.release&&['idle','checking','up_to_date'].includes(content.phase)&&seenNotes()!==content.release){
  const release=content.release;
  card('whatsnew',"What's new",content.notes,undefined,'Got it',()=>{notesSeen(release);removeCard('whatsnew');});
 }else if(content&&!['idle','checking','up_to_date'].includes(content.phase))removeCard('whatsnew');
}
function seenNotes(){try{return localStorage.getItem('updates:notes-seen');}catch(_){return null;}}
function notesSeen(release){try{if(release)localStorage.setItem('updates:notes-seen',release);}catch(_){}}
async function canInstall(){
 if(document.querySelector('dialog[open]')||document.getElementById('screen')?.getAttribute('aria-busy')==='true'||await invoke('session_active'))return false;
 const music=await invoke('music_status').catch(()=>null);
 return !music||!['encoding','building'].includes(music.phase);
}
async function job(kind,fn){if(jobs.has(kind))return;jobs.add(kind);try{await fn();}catch(e){
 card(kind, 'Update could not finish', String(e?.message||e),undefined,'View updates',()=>location.assign('settings.html#updates'));
 // A failed automatic operation must not spin or keep stealing focus.
 later(kind);
}finally{jobs.delete(kind);}}

async function check(force=false){
 if(checking||(!force&&Date.now()-(saved('updates:last-auto-check')||0)<CHECK_MS))return;
 checking=true;
 try{
  const progress=await invoke('onboarding_progress',{legacyRoot:null});if(!progress.complete)return;
  started=true;put('updates:last-auto-check',Date.now());
  const [l,c,p]=await Promise.allSettled([invoke('update_check',{source:null}),invoke('content_status'),invoke('update_policy')]);
  if(l.status==='fulfilled'){launcher=l.value;put('updates:latest',launcher);}
  if(c.status==='fulfilled')content=c.value;
  if(p.status==='fulfilled')policy=p.value;
  if(content&&['idle','up_to_date','problem'].includes(content.phase))content=await invoke('content_check',{depth:null}).catch(()=>content);
  const receipt=JSON.parse(localStorage.getItem('updates:pending-summary')||'null');
  if(receipt&&launcher?.current===receipt.version){
   card('receipt','Launcher updated','Updated to Reborn '+receipt.version+'. '+(receipt.notes||'Your update is installed.'),undefined,'Got it',()=>{removeCard('receipt');localStorage.removeItem('updates:pending-summary');});
  }
  render();
 }finally{checking=false;}
}
async function tick(){
 if(running||!started)return;
 running=true;
 try {
  [activity,content,policy]=await Promise.all([invoke('update_activity').catch(()=>activity),invoke('content_status').catch(()=>content),invoke('update_policy').catch(()=>policy)]);
  if(activity.ready?.check)launcher=activity.ready.check;
  render();
  if(activity.downloading&&launcher?.bytes){const bytes=await invoke('update_progress').catch(()=>0);const bar=cards.get('launcher')?.querySelector('progress');if(bar)bar.value=Math.min(99,bytes*100/launcher.bytes);}
  if(activity.ready?.path&&!deferred('launcher')&&(accepted('launcher')||policy.automatic)&&!jobs.has('launcher')&&await canInstall()){
   const result=activity.ready;
   job('launcher',async()=>{localStorage.setItem('updates:pending-summary',JSON.stringify({version:result.check.latest,notes:result.check.notes}));await invoke('update_apply',{path:result.path,confirm:true});});
   return;
  }
  if(content?.phase==='staged'&&!deferred('content')&&(accepted('content')||policy.automatic)&&!jobs.has('content')&&await canInstall()){
   job('content',async()=>{content=await invoke('content_promote',{confirm:true});render();});return;
  }
  for(const kind of ['launcher','content']){
   const available=kind==='launcher'?launcher?.state==='available'&&!activity.downloading&&!activity.ready?.path:content?.phase==='available';
   if(!available||jobs.has(kind)||deferred(kind))continue;
   if(accepted(kind)){
    if(kind==='launcher')job(kind,async()=>{await invoke('update_download',{source:null});});
    else job(kind,async()=>{content=await invoke('content_fetch');render();});
   }else if((policy.automatic||document.body.classList.contains('page-home'))&&!dialog&&!document.querySelector('dialog[open]')&&!document.body.dataset.controllerTesting&&document.getElementById('screen')?.getAttribute('aria-busy')!=='true')showDialog(kind);
  }
 }catch(_){/* A failed check leaves existing controls and downloads alone. */}
 finally{running=false;}
}

if(window.__TAURI__&&!document.body.classList.contains('page-setup')){
 // Page changes reattach to native download state, and never reset the check clock.
 invoke('onboarding_progress',{legacyRoot:null}).then(async p=>{
  if(!p.complete)return;started=true;policy=await invoke('update_policy').catch(()=>({}));
  await check();await tick();
 }).catch(()=>{});
 setInterval(()=>{check().catch(()=>{});tick();},1500);
 document.addEventListener('visibilitychange',()=>{if(!document.hidden&&Date.now()-(saved('updates:last-auto-check')||0)>STALE_MS)check(true).then(tick).catch(()=>{});});
 window.addEventListener('focus',()=>{if(Date.now()-(saved('updates:last-auto-check')||0)>STALE_MS)check(true).then(tick).catch(()=>{});});
 window.addEventListener('online',()=>check(true).then(tick).catch(()=>{}));
}
