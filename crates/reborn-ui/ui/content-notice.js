// Content updates on the Play page: textures, emulator, mods, game patches
// and the roster. Mirrors update-notice.js, which does the same for the
// launcher's own installer. The flow's state lives in the launcher process
// (content_status), so a page change never loses a download.
const invoke=(name,args={})=>window.__TAURI__.core.invoke(name,args);
const mb=b=>Math.max(1,Math.round((b||0)/1048576));
if(window.__TAURI__)(async()=>{
 try{
  const progress=await invoke('onboarding_progress',{legacyRoot:null});if(!progress.complete)return;
  let status=await invoke('content_status');
  // One check per launcher session; every page still shows what is in flight.
  if(['idle','up_to_date','problem'].includes(status.phase)&&!sessionStorage.getItem('content:checked')){
   sessionStorage.setItem('content:checked','true');
   status=await invoke('content_check',{depth:null});
  }
  const main=document.getElementById('main');
  if(status.phase==='installed'){
   const summary=document.createElement('aside');summary.className='update-notice';summary.setAttribute('role','status');
   const copy=document.createElement('span');copy.textContent=status.installed?.note||status.note;
   const dismiss=document.createElement('button');dismiss.type='button';dismiss.textContent='Got it';dismiss.onclick=()=>{summary.remove();invoke('content_dismiss').catch(()=>{});};
   summary.append(copy,dismiss);main.append(summary);return;
  }
  if(!['available','fetching','staged','installing'].includes(status.phase))return;
  const policy=await invoke('update_policy');
  const notice=document.createElement('aside');notice.className='update-notice';notice.setAttribute('role','status');
  const text=document.createElement('span');text.textContent=`Game content update: ${mb(status.download_bytes)} MB.`;
  const button=document.createElement('button');button.type='button';button.textContent=policy.automatic?'Downloading…':'View update';button.disabled=!!policy.automatic;button.onclick=()=>location.href='settings.html';
  notice.append(text,button);main.append(notice);
  if(!policy.automatic)return;
  if(status.phase==='available')status=await invoke('content_fetch');
  while(status.phase==='fetching'||status.phase==='installing'){await new Promise(r=>setTimeout(r,5000));status=await invoke('content_status');}
  if(status.phase!=='staged'){text.textContent=status.note;button.textContent='View updates';button.disabled=false;return;}
  let installing=false;
  const install=async()=>{
   if(installing)return true;
   if(await invoke('session_active')){text.textContent=`Update ready. ${mb(status.download_bytes)} MB. Installs the next time you close the game.`;return false;}
   installing=true;text.textContent='Installing content update…';
   const done=await invoke('content_promote',{confirm:true});
   const good=done.phase==='installed';
   text.textContent=good?(done.installed?.note||done.note):done.note;
   button.textContent=good?'Got it':'View updates';button.disabled=false;
   button.onclick=good?()=>{notice.remove();invoke('content_dismiss').catch(()=>{});}:()=>location.href='settings.html';
   return true;
  };
  if(!await install()){const timer=setInterval(async()=>{try{if(await install())clearInterval(timer);}catch(e){clearInterval(timer);text.textContent=String(e.message||e);button.textContent='View updates';button.disabled=false;}},5000);}
 }catch(_){/* Settings shows the state and offers a retry. */}
})();
