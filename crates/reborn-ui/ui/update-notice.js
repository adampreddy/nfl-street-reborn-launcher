// The launcher's own update, on the Play page. One check when the launcher
// starts, then again every five minutes and whenever the window comes back
// into view after a while, so a launcher left open still notices a build
// published after it started (Adam, 2026-09-12: "doesn't check for updates
// as often as it should"). The content notice does the same for game
// content in content-notice.js.
const invoke=(name,args={})=>window.__TAURI__.core.invoke(name,args);
const RECHECK_MS=5*60*1000,STALE_MS=2*60*1000;
let checking=false;
const host=()=>document.getElementById('play-notices')||document.getElementById('main');
const lastCheck=()=>Number(sessionStorage.getItem('updates:last-check')||0);

async function checkForUpdate(first){
 // One flow at a time, and never a second notice while one is on screen.
 if(checking||document.querySelector('.update-notice[data-launcher-update]'))return;
 checking=true;
 try{
  const progress=await invoke('onboarding_progress',{legacyRoot:null});if(!progress.complete)return;
  if(first)sessionStorage.setItem('updates:checked','true');
  sessionStorage.setItem('updates:last-check',String(Date.now()));
  const check=await invoke('update_check',{source:null});
  if(first){
   const receipt=JSON.parse(localStorage.getItem('updates:pending-summary')||'null');
   if(receipt&&check.current===receipt.version){
    const summary=document.createElement('aside');summary.className='update-notice';summary.setAttribute('role','status');
    const copy=document.createElement('span');copy.textContent=`Updated to Reborn ${receipt.version}. ${receipt.notes||'Your update is installed.'}`;
    const dismiss=document.createElement('button');dismiss.textContent='Got it';dismiss.onclick=()=>{summary.remove();localStorage.removeItem('updates:pending-summary');};summary.append(copy,dismiss);host().append(summary);
   }
  }
  if(check.state!=='available')return;
  const policy=await invoke('update_policy');
  const notice=document.createElement('aside');notice.className='update-notice';notice.setAttribute('role','status');notice.dataset.launcherUpdate='true';
  const text=document.createElement('span');text.textContent=`Reborn ${check.latest} is available.`;
  const button=document.createElement('button');button.type='button';button.textContent=policy.automatic?'Downloading…':'View update';button.disabled=!!policy.automatic;button.onclick=()=>location.href='settings.html#updates';notice.append(text,button);host().append(notice);
  if(!policy.automatic)return;
  try{
   // The percentage while the installer downloads: the size of the partial
   // file on disk against the size the check reported, once a second.
   const bar=document.createElement('progress');bar.className='notice-bar';bar.max=100;bar.setAttribute('aria-label','Download progress');notice.append(bar);
   const ticker=setInterval(async()=>{try{const got=await invoke('update_progress');if(check.bytes){const pct=Math.min(99,Math.floor(got*100/check.bytes));bar.value=pct;text.textContent=`Downloading Reborn ${check.latest}… ${pct}%`;}}catch(_){}},1000);
   let result;
   try{result=await invoke('update_download',{source:null});}finally{clearInterval(ticker);bar.remove();}
   if(!result.path){text.textContent=result.check.note;button.textContent='View updates';button.disabled=false;return;}
   let installing=false;
   // Installing restarts the launcher, so never during a game, and never
   // while the soundtrack is being encoded or the disc rebuilt.
   const busy=async()=>{
    if(await invoke('session_active'))return 'Update verified. Installation will start after your game.';
    try{const music=await invoke('music_status');if(music&&['encoding','building'].includes(music.phase))return 'Update verified. Installation will start once your soundtrack is done.';}catch(_){}
    return null;
   };
   const install=async()=>{
    if(installing)return true;
    const why=await busy();
    if(why){text.textContent=why;return false;}
    installing=true;
    localStorage.setItem('updates:pending-summary',JSON.stringify({version:check.latest,notes:check.notes}));
    text.textContent='Installing verified update…';await invoke('update_apply',{path:result.path,confirm:true});return true;
   };
   if(!await install()){const timer=setInterval(async()=>{try{if(await install())clearInterval(timer);}catch(e){clearInterval(timer);text.textContent=String(e.message||e);button.textContent='View updates';button.disabled=false;}},5000);}
  }catch(e){text.textContent='The update could not be completed. Try again in Settings.';button.textContent='View updates';button.disabled=false;}
 }catch(_){/* Settings offers a manual retry. */}
 finally{checking=false;}
}

if(window.__TAURI__){
 setInterval(()=>checkForUpdate(false),RECHECK_MS);
 document.addEventListener('visibilitychange',()=>{if(document.visibilityState==='visible'&&Date.now()-lastCheck()>STALE_MS)checkForUpdate(false);});
 if(!sessionStorage.getItem('updates:checked'))checkForUpdate(true);
 else if(Date.now()-lastCheck()>RECHECK_MS)checkForUpdate(false);
}
