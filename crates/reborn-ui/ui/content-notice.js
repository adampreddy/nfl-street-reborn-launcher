// Content updates on the Play page: textures, emulator, mods, game patches
// and the roster. Mirrors update-notice.js, which does the same for the
// launcher's own installer. The flow's state lives in the launcher process
// (content_status), so a page change never loses a download.
const invoke=(name,args={})=>window.__TAURI__.core.invoke(name,args);
const mb=b=>Math.max(1,Math.round((b||0)/1048576));
if(window.__TAURI__)(async()=>{
 try{
  const progress=await invoke('onboarding_progress',{legacyRoot:null});if(!progress.complete)return;
  // A launcher left open still notices what was published after it started:
  // every 5 minutes, and again when its window comes back into view after a
  // while, whenever nothing is in flight. Adam published textures while his
  // launcher sat open and had to press Check for updates by hand (2026-09-11).
  // Never while a game runs, a dialog is open or the page is busy: a reload
  // then would throw away the launch result or a sign-in in progress.
  const RECHECK_MS=5*60*1000,STALE_MS=2*60*1000;let lastCheck=Date.now();
  const pageBusy=async()=>!!document.querySelector('dialog[open]')||document.getElementById('screen')?.getAttribute('aria-busy')==='true'||await invoke('session_active');
  const recheck=async()=>{try{if(await pageBusy())return;const s=await invoke('content_status');if(!['idle','up_to_date','problem'].includes(s.phase))return;lastCheck=Date.now();const r=await invoke('content_check',{depth:null});if(['available','fetching','staged','installing'].includes(r.phase)&&!await pageBusy())location.reload();}catch(_){}};
  setInterval(recheck,RECHECK_MS);
  document.addEventListener('visibilitychange',()=>{if(document.visibilityState==='visible'&&Date.now()-lastCheck>STALE_MS)recheck();});
  let status=await invoke('content_status');
  // One check per launcher session; every page still shows what is in flight.
  if(['idle','up_to_date','problem'].includes(status.phase)&&!sessionStorage.getItem('content:checked')){
   sessionStorage.setItem('content:checked','true');
   status=await invoke('content_check',{depth:null});
  }
  const main=(document.getElementById('play-notices')||document.getElementById('main'));
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
  const button=document.createElement('button');button.type='button';button.textContent=policy.automatic?'Downloading…':'View update';button.disabled=!!policy.automatic;button.onclick=()=>location.href='settings.html#updates';
  // The bar and the percentage: what a person waiting on a gigabyte wants
  // to see. Polled every second and a half while the fetch runs.
  const bar=document.createElement('progress');bar.className='notice-bar';bar.max=100;bar.hidden=true;bar.setAttribute('aria-label','Download progress');
  // While something is on its way the notice becomes a panel, not a line: a
  // headline, the size, the percentage and a thick bar. Adam: "the indicator
  // that something is downloading needs to be way more prominent".
  const headline=document.createElement('strong');headline.className='notice-headline';headline.hidden=true;
  const show=s=>{if(s.phase==='fetching'&&s.download_bytes){const pct=Math.min(99,Math.floor((s.fetched_bytes||0)*100/s.download_bytes));notice.classList.add('downloading');headline.hidden=false;headline.textContent='Downloading game content';bar.hidden=false;bar.value=pct;text.textContent=`${mb(s.fetched_bytes||0)} of ${mb(s.download_bytes)} MB · ${pct}%`;}else if(s.phase==='installing'){notice.classList.add('downloading');headline.hidden=false;headline.textContent='Installing game content';bar.hidden=false;bar.removeAttribute('value');text.textContent='Almost there. Keep the launcher open.';}else{notice.classList.remove('downloading');headline.hidden=true;}};
  notice.append(headline,text,button,bar);main.append(notice);
  if(!policy.automatic)return;
  if(status.phase==='available')status=await invoke('content_fetch');
  show(status);
  while(status.phase==='fetching'||status.phase==='installing'){await new Promise(r=>setTimeout(r,1500));status=await invoke('content_status');show(status);}
  bar.hidden=true;
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
   button.onclick=good?()=>{notice.remove();invoke('content_dismiss').catch(()=>{});}:()=>location.href='settings.html#updates';
   return true;
  };
  if(!await install()){const timer=setInterval(async()=>{try{if(await install())clearInterval(timer);}catch(e){clearInterval(timer);text.textContent=String(e.message||e);button.textContent='View updates';button.disabled=false;}},5000);}
 }catch(_){/* Settings shows the state and offers a retry. */}
})();
